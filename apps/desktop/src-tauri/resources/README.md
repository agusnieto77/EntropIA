This directory is reserved for bundled Tauri resources.

## OCR Models

Runtime assets for the `ocrs` engine are downloaded automatically before each build:

- `text-detection.rten` (~2.4 MB) — text detection model
- `text-recognition.rten` (~9.3 MB) — text recognition model

These files are **NOT committed** to the repository (ignored by `*.rten` in `.gitignore`).
They are downloaded from the official ocrs-models S3 bucket by the pre-build script:

```
apps/desktop/src-tauri/scripts/download-ocr-models.ps1
```

The script runs automatically via `tauri.conf.json` `beforeBuildCommand` and `beforeDevCommand`.
If you need to download manually:

```powershell
powershell -File apps/desktop/src-tauri/scripts/download-ocr-models.ps1
```

## Operational Notes

- `sqlite-vec-windows-tradeoff.md`: rationale and rollback plan for the temporary Windows sqlite-vec shim used to unblock default-features Rust tests.
