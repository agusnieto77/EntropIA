This directory is reserved for bundled Tauri resources.

Expected runtime assets such as OCR models can be placed here when available.
The file exists so `tauri.conf.json` bundle resource globs resolve during local test/build tooling.

## Operational Notes

- `sqlite-vec-windows-tradeoff.md`: rationale and rollback plan for the temporary Windows sqlite-vec shim used to unblock default-features Rust tests.
