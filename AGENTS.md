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

### Python (required for Transcription and PaddleOCR-VL)

- **Python 3.8+** with the following packages:
  - `faster-whisper` — for audio transcription
  - `paddleocr` (with `paddleocr[doc-parser]`) — for PaddleOCR-VL layout-aware OCR
- Install: `pip install faster-whisper "paddleocr[doc-parser]"`
- On first run:
  - `faster-whisper` downloads the Whisper model (~150MB for `base`) to `~/.cache/faster-whisper/`
  - `paddleocr` downloads PP-DocLayoutV3 (~30MB) + PaddleOCR-VL-1.5-0.9B (~150MB) to `~/.paddlex/official_models/`
- The Rust backend auto-detects Python by checking candidates (Conda first, then system Python) and verifying the respective module is importable

## OCR Engine Architecture

- **Primary engine**: PaddleOCR-VL via Python subprocess (`paddle_vl.py`)
  - PaddleOCRVL class does layout detection + OCR in a single pass
  - Returns structured blocks with text content, labels, bounding boxes, and reading order
  - Label mapping (paddle_vl.py → Rust): doc_title/paragraph_title → title, text → plain_text, image → figure, table → table, vision_footnote → abandoned
  - Text formatting rules in paddle_vl.py: titles get `## ` prefix, tables get `---` wrapper, images skipped, vision_footnote gets `Note: ` prefix
  - Method field: `"paddle_vl"` (when PaddleVL succeeds) or `"paddle"`/`"tesseract"` (fallback to plain OCR)
- **Native OCR engine** (feature-gated as `paddle-ocr`): PaddleOCR via `ocr-rs` crate (MNN backend)
  - PP-OCRv5 detection + latin recognition
  - Used as fallback provider when PaddleVL is unavailable
  - PP-LCNet document orientation model (`PP-LCNet_x1_0_doc_ori.mnn`, bundled): auto-detects rotation and corrects before OCR
  - Postprocessing heuristics (`postprocess.rs`) are DISABLED — kept for reference only
- **Fallback engine**: Tesseract via `leptess` crate
  - Languages: `spa+eng` (Spanish primary, English fallback)
  - `LepTess` is NOT `Send` → created per-call inside `spawn_blocking`
- **Provider chain**: PaddleVL (layout-aware) → PaddleOCR (plain) → Tesseract (plain) → Error
  - PaddleVL is tried first on every image; if it fails or is unavailable, plain OCR via the OcrProvider runs
  - For PDFs: native text extraction first, then each page is rendered and PaddleVL is tried per page
- **PDF pipeline**: Native text extraction first (`pdf-extract`), quality-checked with `is_quality_text()` (≥50 alphanumeric chars). If native text fails quality check, ALL pages are rendered via `pdfium-render` at 300 DPI and OCR'd sequentially. Results are concatenated with `---` page separators. Method field: `"native"` | `"pdf_paddle_vl"` | `"pdf_paddle"` | `"pdf_tesseract"`.
- **No preprocessing**: Tesseract handles its own binarization internally. PaddleOCR does its own internally.

## PaddleOCR-VL Architecture (Primary OCR Engine)

- **Engine**: PaddleOCR-VL (`PaddleOCRVL` class from `paddleocr[doc-parser]`) spawned as Python subprocess via `std::process::Command`

### ONNX Layout Detection Engine (Primary Layout Engine)

- **Engine**: PP-DocLayout-S (PicoDet architecture) via `ort` crate ONNX Runtime
- **Model**: `PP-DocLayout-S.onnx` (4.68 MB) in `resources/models/ocr/`
- **Input**: 2 tensors — `image` [1,3,480,480] (resized, ImageNet normalized) + `scale_factor` [1,2] (scale_y, scale_x)
- **Output**: 2 tensors — `fetch_name_0` [N,6] (class_id, score, x1, y1, x2, y2) + `fetch_name_1` [1] (int32 count)
- **23 classes**: doc_title, paragraph_title, text, abandoned, figure_title, figure_note, text, page_header, page_footer, table, table_caption, table_note, image, chart, vision_footnote, formula, seal, paragraph_title, code, reference, abstract, page_number, text
- **PicoDet applies NMS internally** — no separate NMS step needed
- **Preprocessing**: Direct resize to 480×480 (no letterbox), scale_factor passed to model for coordinate remapping
- **Coordinate mapping**: Output coords are in 480×480 space; scaled back to original via `scale_x = orig_w / 480`, `scale_y = orig_h / 480`
- **ORT DLL resolution**: Scans `resources/models/ner/` sibling directory for `onnxruntime.dll` (shared with NER module)
- **Feature-gated**: `paddle-ocr` feature for native PaddleOCR fallback; layout engine always available when model file exists
- **Conversion note**: Paddle → ONNX is Linux-only (paddle2onnx DLL bug on Windows). Script: `scripts/convert_layout_to_onnx.py`

## PaddleOCR-VL Architecture (Python Subprocess)
- **Why subprocess**: Same pattern as transcription — isolates Python crashes, no native Rust bindings for PaddleOCR-VL
- **Python auto-detection**: `which_python_for_paddle_vl()` probes for `paddleocr` module (same strategy as transcription's `which_python()` for `faster_whisper`)
- **Script path**: `scripts/paddle_vl.py` → bundled via `tauri.conf.json` resources → resolved at runtime via `BaseDirectory::Resource` with `CARGO_MANIFEST_DIR` fallback. CRITICAL: Tauri's `resolve()` doesn't verify file existence — must check and fall back to dev path. Also strip Windows `\\?\` prefix.
- **Script path resolution**: `create_paddle_vl_engine()` uses 3-tier resolution: (1) `BaseDirectory::Resource` with existence check + `\\?\` prefix stripping, (2) `CARGO_MANIFEST_DIR/resources/scripts/paddle_vl.py`, (3) `CARGO_MANIFEST_DIR/scripts/paddle_vl.py`
- **JSON output safety**: Sentinel marker pattern (`===VL_JSON_BEGIN===` / `===VL_JSON_END===`) for reliable JSON extraction
- **Output structure**: `PaddleVlOutput` with `text` (formatted plain text), `method`, `blocks` (parsed content blocks), `regions` (layout detection boxes), `image_width`, `image_height`
- **PaddleVlOutput data model**: Each block has `label` (mapped category), `content` (text), `bbox` (x, y, width, height), `order` (reading order), `group_id`. Each region has `category`, `bbox`, `confidence`.
- **No DB persistence**: PaddleVL results flow through the OCR pipeline but are not stored separately. The formatted text is saved in `extractions` table as usual.
- **Auto-wired pipeline**: PaddleVL runs AUTOMATICALLY inside OCR worker when available. `process_image()` and `process_pdf()` try PaddleVL first, fall back to plain OCR.

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

- `apps/desktop/src-tauri/src/ocr/provider.rs` - OcrProvider trait + OcrOutput/OcrRegion/BoundingBox types + LayoutCategory/LayoutRegion/LayoutOutput types
- `apps/desktop/src-tauri/src/ocr/layout_onnx.rs` - OnnxLayoutEngine: PP-DocLayout-S ONNX inference, PicoDet output parsing, label mapping
- `apps/desktop/src-tauri/src/ocr/tesseract.rs` - TesseractProvider (LepTess wrapper)
- `apps/desktop/src-tauri/src/ocr/paddle.rs` - PaddleOcrProvider with ocr-rs, optional OriModel, Debug impl, integration test
- `apps/desktop/src-tauri/src/ocr/paddle_vl.rs` - PaddleVlEngine, PaddleVlConfig, which_python_for_paddle_vl, create_paddle_vl_engine, sentinel JSON parsing
- `apps/desktop/src-tauri/src/ocr/layout_onnx.rs` - OnnxLayoutEngine: PP-DocLayout-S (PicoDet) ONNX layout detection, scale_factor input, [N,6] output parsing
- `apps/desktop/src-tauri/src/ocr/postprocess.rs` - Column grouping, hyphen merge, paragraph detection (DISABLED — kept for reference)
- `apps/desktop/src-tauri/src/ocr/pdf.rs` - PDF text extraction + multi-page rendering with pdfium-render
- `apps/desktop/src-tauri/src/ocr/mod.rs` - Worker with Arc<dyn OcrProvider> fallback chain, PaddleVL integration, multi-page PDF OCR
- `apps/desktop/src-tauri/src/ocr/engine.rs` - Original Tesseract engine (pub(crate) fields, Debug derive)
- `apps/desktop/src-tauri/src/transcription/engine.rs` - Python subprocess adapter (spawns transcribe.py)
- `apps/desktop/src-tauri/src/transcription/mod.rs` - Transcription job queue, worker loop, persistence, Python detection
- `apps/desktop/src-tauri/src/transcription/commands.rs` - Tauri IPC commands for transcription
- `apps/desktop/src-tauri/src/transcription/audio.rs` - Commented out (faster-whisper handles audio)
- `apps/desktop/src-tauri/scripts/transcribe.py` - Python transcription script using faster-whisper
- `apps/desktop/src-tauri/scripts/paddle_vl.py` - Python PaddleOCR-VL script (layout + OCR in one pass)
- `apps/desktop/src-tauri/resources/scripts/transcribe.py` - Copy for Tauri resource bundling
- `apps/desktop/src-tauri/resources/scripts/paddle_vl.py` - Copy for Tauri resource bundling

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

Both background systems (OCR, Transcription) follow the same pattern:
1. **Frontend** calls Tauri command → submits job to mpsc channel → returns "queued"
2. **Worker thread** drains jobs serially, emits `progress/complete/error` events
3. **Frontend** listens to events via `OcrStore`/`TranscriptionStore` → updates UI reactively
4. **DB** stores results in `extractions`/`transcriptions` table for persistence between sessions