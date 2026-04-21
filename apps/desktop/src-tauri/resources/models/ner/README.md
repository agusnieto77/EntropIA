# Spanish NER ONNX Assets

This folder contains the **runtime assets** for the desktop app's Spanish NER engine.

The Rust backend expects these files under:

```text
apps/desktop/src-tauri/resources/models/ner/
```

## Expected Files

### Required

- `model.onnx` — token-classification ONNX model
- `tokenizer.json` — Hugging Face tokenizer export used by the model

### Recommended

- `config.json` — Hugging Face config with `id2label` and `_name_or_path`

### Required for local ORT runtime loading (unless `ORT_DYLIB_PATH` is set)

On Windows:

- `onnxruntime.dll`

On Linux:

- `libonnxruntime.so` or `libonnxruntime.so.1`

On macOS:

- `libonnxruntime.dylib`

## Recommended Model

Baseline model for EntropIA:

- `mrm8488/bert-spanish-cased-finetuned-ner`

The current backend supports BIO-style labels and maps at least:

- `PER` → `person`
- `LOC` → `place`
- `ORG` → `organization` / `institution` (heuristic normalization)
- `MISC` → `misc`

## One-time Export Flow

Use the helper script from the project root:

```powershell
powershell -File apps/desktop/src-tauri/scripts/prepare-ner-model.ps1
```

That script exports the model to ONNX, copies the tokenizer/config, and tells you where to place the ONNX Runtime DLL if you are using Windows.

## Final Layout Example

```text
resources/models/ner/
  model.onnx
  tokenizer.json
  config.json
  manifest.json
  onnxruntime.dll   # Windows only, unless ORT_DYLIB_PATH is set
```

## Notes

- `model.onnx` and runtime binaries are ignored by git on purpose.
- `tokenizer.json`, `config.json`, and `manifest.json` can be committed if you want deterministic packaging metadata.
- The app runs in **hybrid mode** by default: if ONNX assets are missing or fail to initialize, it falls back to rule-based NER.
