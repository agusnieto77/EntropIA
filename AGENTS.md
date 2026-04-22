# EntropIA - Build & Architecture Notes

## Prerequisites (Windows)

### Rust Toolchain

- `rustup` with stable toolchain
- MSVC Build Tools 2022

### Tesseract (via vcpkg)

```powershell
# One-time setup
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat
C:\vcpkg\vcpkg install tesseract:x64-windows-static-md
C:\vcpkg\vcpkg integrate install
```

### LLVM/Clang (required by bindgen for leptess)

```powershell
choco install llvm -y
# Set permanently:
[System.Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "User")
```

### Environment Variables

- `LIBCLANG_PATH=C:\Program Files\LLVM\bin` (for bindgen at build time)
- `TESSDATA_PREFIX=C:\vcpkg\installed\x64-windows-static-md\share` (for dev mode)

### Tesseract Language Models

- `resources/tessdata/eng.traineddata` and `resources/tessdata/spa.traineddata`
- Bundled via `tauri.conf.json` resources
- At runtime, resolved via `BaseDirectory::Resource` → `tessdata/`

### CMake (required by whisper-rs for whisper.cpp build)

```powershell
# Already bundled with Visual Studio Build Tools:
# C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin
# Add to PATH if not present:
$env:Path += ";C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"
```

> **Note**: CMake and whisper-rs are no longer needed for transcription (migrated to faster-whisper Python subprocess).
> The CMake prerequisite is kept for reference in case whisper-rs is ever re-enabled.

### Python (required for Transcription)

- **Python 3.8+** with `faster-whisper` package installed
- Install: `pip install faster-whisper` (or `conda install -c conda-forge faster-whisper`)
- On first run, `faster-whisper` downloads the Whisper model (~150MB for `base`) to `~/.cache/faster-whisper/`
- The Rust backend auto-detects Python by checking candidates (Conda first, then system Python) and verifying `faster_whisper` is importable

## OCR Engine Architecture

- **Primary engine**: PaddleOCR via `ocr-rs` crate (MNN backend, feature-gated as `paddle-ocr`)
  - PP-OCRv5 detection + latin recognition
  - `OcrEngine` is `Send + Sync` → held in worker thread, shared across `spawn_blocking`
  - Optional PP-LCNet document orientation model (`PP-LCNet_x1_0_doc_ori.mnn`): auto-detects 0°/90°/180°/270° rotation and corrects before OCR. Skipped gracefully if model file is missing.
  - Bounding-box output → column grouping + hyphen merge + paragraph detection (postprocess.rs)
- **Fallback engine**: Tesseract via `leptess` crate
  - Languages: `spa+eng` (Spanish primary, English fallback)
  - `LepTess` is NOT `Send` → created per-call inside `spawn_blocking`
- **Provider chain**: PaddleOCR → Tesseract → Error (tried in order at worker startup)
- **PDF pipeline**: Native text extraction first (`pdf-extract`), quality-checked with `is_quality_text()` (≥50 alphanumeric chars). If native text fails quality check, ALL pages are rendered via `pdfium-render` at 300 DPI and OCR'd sequentially. Results are concatenated with `---` page separators. Method field: `"native"` | `"pdf_paddle"` | `"pdf_tesseract"`.
- **No preprocessing**: Tesseract handles its own binarization internally. PaddleOCR does its own internally.

### Orientation Model (Optional)

The PP-LCNet document orientation model (`PP-LCNet_x1_0_doc_ori.mnn`) is **optional**. If present in `resources/models/ocr/`, it enables automatic 0°/90°/180°/270° rotation correction before OCR. If missing, a warning is logged and OCR proceeds without rotation correction.

To obtain the model:
1. **Automated conversion script**: `python scripts/convert_ori_model.py`
   - Downloads from PaddleOCR official repo automatically
   - Requires: `pip install paddlepaddle paddle2onnx` + MNNConvert on PATH
   - MNNConvert: build from https://github.com/alibaba/MNN (`cmake .. -DMNN_BUILD_CONVERTER=ON`)
2. **Manual**: `paddleocr doc_img_orientation_classification -i test.jpg` → convert via MNNConvert
3. Place as `PP-LCNet_x1_0_doc_ori.mnn` in `apps/desktop/src-tauri/resources/models/ocr/`

## Build Commands

```powershell
# Set env vars (already set permanently, but for reference):
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:TESSDATA_PREFIX = "C:\vcpkg\installed\x64-windows-static-md\share"

# Build
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml

# Check only
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## Key Files

- `apps/desktop/src-tauri/src/ocr/provider.rs` - OcrProvider trait + OcrOutput/OcrRegion/BoundingBox types
- `apps/desktop/src-tauri/src/ocr/tesseract.rs` - TesseractProvider (LepTess wrapper)
- `apps/desktop/src-tauri/src/ocr/paddle.rs` - PaddleOcrProvider with ocr-rs, optional OriModel, Debug impl, integration test
- `apps/desktop/src-tauri/src/ocr/postprocess.rs` - Column grouping, hyphen merge, paragraph detection
- `apps/desktop/src-tauri/src/ocr/pdf.rs` - PDF text extraction + multi-page rendering with pdfium-render
- `apps/desktop/src-tauri/src/ocr/mod.rs` - Worker refactor with Arc<dyn OcrProvider> fallback chain, multi-page PDF OCR
- `apps/desktop/src-tauri/src/ocr/engine.rs` - Original Tesseract engine (pub(crate) fields, Debug derive)
- `apps/desktop/src-tauri/src/transcription/engine.rs` - Python subprocess adapter (spawns transcribe.py)
- `apps/desktop/src-tauri/src/transcription/mod.rs` - Transcription job queue, worker loop, persistence, Python detection
- `apps/desktop/src-tauri/src/transcription/commands.rs` - Tauri IPC commands for transcription
- `apps/desktop/src-tauri/src/transcription/audio.rs` - Commented out (faster-whisper handles audio)
- `apps/desktop/src-tauri/scripts/transcribe.py` - Python transcription script using faster-whisper
- `apps/desktop/src-tauri/resources/scripts/transcribe.py` - Copy for Tauri resource bundling

## Transcription Engine Architecture

- **Engine**: faster-whisper (Python) spawned as subprocess via `std::process::Command`
- **Why subprocess**: whisper-rs (Rust bindings for whisper.cpp) crashes on Windows due to C++ foreign exceptions in ggml (F16C/AVX2). The subprocess approach completely isolates crashes — if Python crashes, we catch it as `Result::Err`.
- **Compute type**: `int8` (fast, universal, avoids SIMD issues)
- **Python auto-detection**: `which_python()` checks Conda paths first, then system Python, verifying `faster_whisper` is importable for each candidate
- **Script path**: `scripts/transcribe.py` → bundled via `tauri.conf.json` resources → resolved at runtime via `BaseDirectory::Resource` with `CARGO_MANIFEST_DIR` fallback for dev mode
- **Key constraint**: Python subprocess is NOT persistent. Each transcription call spawns a fresh process that loads the model (cached after first download), transcribes, outputs JSON to stdout, and exits.
- **JSON output safety**: Output is wrapped in sentinel markers (`===TRANSCRIPTION_JSON_BEGIN===` / `===TRANSCRIPTION_JSON_END===`) so the Rust side can reliably extract JSON even if other libraries write to stdout.
- **Audio decoding**: Handled entirely by faster-whisper (uses FFmpeg internally). No Rust audio decoding needed.
- **Language**: Defaults to Spanish (`"es"`), configurable in `WhisperConfig`
- **Persistence**: Results stored in `transcriptions` table (text_content, language, duration_ms, model, segments JSON, confidence)
- **Error handling**: Full stderr/stdout capture in error messages. Python path and script path included for debugging.

## Job Queue Pattern

Both OCR and Transcription follow the same pattern:
1. **Frontend** calls Tauri command → submits job to mpsc channel → returns "queued"
2. **Worker thread** drains jobs serially, emits `progress/complete/error` events
3. **Frontend** listens to events via `OcrStore`/`TranscriptionStore` → updates UI reactively
4. **DB** stores results in `extractions`/`transcriptions` table for persistence between sessions
