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

### Python (required for Transcription and Layout Detection)

- **Python 3.8+** with the following packages:
  - `faster-whisper` — for audio transcription
  - `doclayout-yolo` — for document layout detection (DocLayout-YOLO model)
- Install: `pip install faster-whisper doclayout-yolo`
- On first run:
  - `faster-whisper` downloads the Whisper model (~150MB for `base`) to `~/.cache/faster-whisper/`
  - `doclayout-yolo` downloads the YOLOv10 model (~30-50MB) from HuggingFace to `~/.cache/huggingface/hub/`
- The Rust backend auto-detects Python by checking candidates (Conda first, then system Python) and verifying the respective module is importable

## OCR Engine Architecture

- **Primary engine**: PaddleOCR via `ocr-rs` crate (MNN backend, feature-gated as `paddle-ocr`)
  - PP-OCRv5 detection + latin recognition
  - `OcrEngine` is `Send + Sync` → held in worker thread, shared across `spawn_blocking`
  - PP-LCNet document orientation model (`PP-LCNet_x1_0_doc_ori.mnn`, bundled): auto-detects 0°/90°/180°/270° rotation and corrects before OCR. Confidence threshold 0.7. Skipped gracefully if model file is missing.
  - Postprocessing heuristics (`postprocess.rs`) are DISABLED — they mixed lines from different columns. The `postprocess()` function is kept for reference but not called.
- **Fallback engine**: Tesseract via `leptess` crate
  - Languages: `spa+eng` (Spanish primary, English fallback)
  - `LepTess` is NOT `Send` → created per-call inside `spawn_blocking`
- **Provider chain**: PaddleOCR → Tesseract → Error (tried in order at worker startup)
- **Region-level OCR**: `recognize_region()` on the `OcrProvider` trait crops the image to a layout region's bounding box before OCR. PaddleOCR overrides this for efficient crop-based recognition with 3% padding (proportional to bbox dimensions) on left/right to avoid cutting off characters at box edges. Tesseract uses the default (full-image fallback).
- **Layout-aware OCR**: `layout_aware_ocr()` function in `ocr/mod.rs` takes a `LayoutResult` from DocLayout-YOLO, sorts regions by reading order, and runs OCR on each text-bearing region. Figures are skipped, tables are wrapped with `---` markers, titles get `## ` prefix. Method field: `"paddle+layout"` or `"tesseract+layout"`.
- **PDF pipeline**: Native text extraction first (`pdf-extract`), quality-checked with `is_quality_text()` (≥50 alphanumeric chars). If native text fails quality check, ALL pages are rendered via `pdfium-render` at 300 DPI and OCR'd sequentially. Results are concatenated with `---` page separators. Method field: `"native"` | `"pdf_paddle"` | `"pdf_tesseract"`.
- **No preprocessing**: Tesseract handles its own binarization internally. PaddleOCR does its own internally.

## Layout Detection Architecture

- **Engine**: DocLayout-YOLO (Python) spawned as subprocess via `std::process::Command`
- **Model**: `juliozhao/DocLayout-YOLO-DocStructBench` — YOLO-v10 based, 10 categories (title, plain_text, abandoned, figure, figure_caption, table, table_caption, table_footnote, isolate_formula, formula_caption)
- **Model loading**: `YOLOv10.from_pretrained()` is BROKEN — it internally creates `YOLOv10(model="yolov10n.pt")` which fails. Instead, we download the `.pt` file via `huggingface_hub.hf_hub_download()` and load directly with `YOLOv10(local_path)`. Strategy 1: search HF cache for the model file. Strategy 2: download from HuggingFace.
- **Why subprocess**: Same pattern as transcription — isolates Python crashes, no native Rust bindings for DocLayout-YOLO
- **Python auto-detection**: `which_python_for_layout()` probes for `doclayout_yolo` module (same strategy as transcription's `which_python()` for `faster_whisper`)
- **Script path**: `scripts/layout_detect.py` → bundled via `tauri.conf.json` resources → resolved at runtime via `BaseDirectory::Resource` with `CARGO_MANIFEST_DIR` fallback. CRITICAL: Tauri's `resolve()` doesn't verify file existence — must check and fall back to dev path. Also strip Windows `\\?\` prefix.
- **Script path resolution**: Both `create_layout_engine()` and `LayoutQueue::start_worker()` use identical 3-tier resolution: (1) `BaseDirectory::Resource` with existence check + `\\?\` prefix stripping, (2) `CARGO_MANIFEST_DIR/resources/scripts/layout_detect.py`, (3) `CARGO_MANIFEST_DIR/scripts/layout_detect.py`
- **JSON output safety**: Same sentinel marker pattern (`===LAYOUT_JSON_BEGIN===` / `===LAYOUT_JSON_END===`)
- **Reading order algorithm**: Union-find column grouping: regions with ≥50% horizontal overlap belong to the same column. Columns sorted left-to-right, regions within columns sorted top-to-bottom. Reading order = start at top-left column, go down, then move to next column right. Abandoned at page top → first, abandoned at bottom → last.
- **DB persistence**: `layouts` table stores detection results (regions as JSON TEXT) keyed by `asset_id`
- **Auto-wired pipeline**: Layout detection runs AUTOMATICALLY inside OCR worker when DocLayout-YOLO is available. `process_image()` and `process_pdf()` try layout detection first, fall back to plain OCR. Method field: `"paddle+layout"` or `"pdf_paddle+layout"`.

### Orientation Model (Included)

The PP-LCNet document orientation model (`PP-LCNet_x1_0_doc_ori.mnn`, ~6.4 MB) is **bundled** in `resources/models/ocr/`. It enables automatic 0°/90°/180°/270° rotation correction before OCR. If the file is missing, a warning is logged and OCR proceeds without rotation correction.

Conversion pipeline used to generate the `.mnn` file (Linux-only, PaddlePaddle 2.5.2):
1. `pip install paddlepaddle==2.5.2 -f https://www.paddlepaddle.org.cn/whl/linux/mkl/avx/stable.html`
2. `pip install paddle2onnx paddleclas`
3. `PaddleClas(model_name='text_image_orientation')` → downloads legacy `.pdmodel`/`.pdiparams` to `~/.paddleclas/`
4. `paddle2onnx --model_filename inference.pdmodel --params_filename inference.pdiparams --save_file ori.onnx --opset_version 12`
5. `pip install MNN && python -m MNN.tools.mnnconvert -f ONNX --modelFile ori.onnx --MNNModel PP-LCNet_x1_0_doc_ori.mnn --bizCode EntropIA`

Note: Conversion does NOT work on Windows due to paddle2onnx incompatibility with PaddlePaddle ≥ 2.6. Use a Linux environment or Colab.

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

- `apps/desktop/src-tauri/src/ocr/provider.rs` - OcrProvider trait + OcrOutput/OcrRegion/BoundingBox types, recognize_region default method
- `apps/desktop/src-tauri/src/ocr/tesseract.rs` - TesseractProvider (LepTess wrapper)
- `apps/desktop/src-tauri/src/ocr/paddle.rs` - PaddleOcrProvider with ocr-rs, optional OriModel, Debug impl, recognize_region crop-based OCR, integration test
- `apps/desktop/src-tauri/src/ocr/postprocess.rs` - Column grouping, hyphen merge, paragraph detection (DISABLED — kept for reference)
- `apps/desktop/src-tauri/src/ocr/pdf.rs` - PDF text extraction + multi-page rendering with pdfium-render
- `apps/desktop/src-tauri/src/ocr/mod.rs` - Worker refactor with Arc<dyn OcrProvider> fallback chain, multi-page PDF OCR, layout_aware_ocr function
- `apps/desktop/src-tauri/src/ocr/engine.rs` - Original Tesseract engine (pub(crate) fields, Debug derive)
- `apps/desktop/src-tauri/src/layout/region.rs` - LayoutCategory enum (10 variants), LayoutRegion, LayoutResult, BoundingBox
- `apps/desktop/src-tauri/src/layout/engine.rs` - DocLayoutEngine (Python subprocess), which_python_for_layout, sentinel JSON parsing
- `apps/desktop/src-tauri/src/layout/mod.rs` - LayoutQueue, LayoutJob, worker loop, SQLite persistence, event payloads
- `apps/desktop/src-tauri/src/layout/reading_order.rs` — Reading order: union-find column grouping (≥50% horizontal overlap = same column), columns sorted left-to-right, regions within columns sorted top-to-bottom
- `apps/desktop/src-tauri/src/layout/commands.rs` - Tauri IPC command (extract_layout)
- `apps/desktop/src-tauri/src/transcription/engine.rs` - Python subprocess adapter (spawns transcribe.py)
- `apps/desktop/src-tauri/src/transcription/mod.rs` - Transcription job queue, worker loop, persistence, Python detection
- `apps/desktop/src-tauri/src/transcription/commands.rs` - Tauri IPC commands for transcription
- `apps/desktop/src-tauri/src/transcription/audio.rs` - Commented out (faster-whisper handles audio)
- `apps/desktop/src-tauri/scripts/transcribe.py` - Python transcription script using faster-whisper
- `apps/desktop/src-tauri/scripts/layout_detect.py` - Python layout detection script using DocLayout-YOLO
- `apps/desktop/src-tauri/resources/scripts/transcribe.py` - Copy for Tauri resource bundling
- `apps/desktop/src-tauri/resources/scripts/layout_detect.py` - Copy for Tauri resource bundling

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

All three background systems (OCR, Transcription, Layout Detection) follow the same pattern:
1. **Frontend** calls Tauri command → submits job to mpsc channel → returns "queued"
2. **Worker thread** drains jobs serially, emits `progress/complete/error` events
3. **Frontend** listens to events via `OcrStore`/`TranscriptionStore`/`LayoutStore` → updates UI reactively
4. **DB** stores results in `extractions`/`transcriptions`/`layouts` table for persistence between sessions
