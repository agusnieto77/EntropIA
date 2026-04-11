# Proposal: Fase 2 — OCR + Document Processing

## Intent

Historical archivists need to extract readable text from imported PDFs and images so EntropIA can surface, search, and analyse document content. Today the app stores files but cannot read them. This change adds a non-blocking OCR pipeline (pure-Rust, offline, no system deps) that extracts text and stores it in a dedicated `extractions` table — making content available to Fase 3 NLP and FTS5 search.

## Scope

### In Scope

- **Rust OCR module** `src-tauri/src/ocr/` — worker, engine (`ocrs` v0.12.2), PDF layer (`pdf-extract`), preprocessor (`image` + `imageproc`)
- **Serial job queue** — `tokio::sync::mpsc` channel + single background worker, `spawn_blocking` for CPU work
- **Progress events** — `app_handle.emit("ocr:progress" | "ocr:complete" | "ocr:error", ...)` over existing Tauri event permission
- **PDF quality heuristic** — native text layer first; if < 50 valid chars → OCR fallback
- **Image preprocessing** — grayscale → adaptive threshold → resize before `ocrs`
- **`ocrs` models** (~20 MB) bundled in `apps/desktop/src-tauri/resources/` for offline use
- **DB refactor** — `AppDb` → `AppDbState { ui_conn: Mutex<Connection>, worker_conn: Mutex<Connection> }` (SQLite WAL, no deadlock)
- **New `extractions` table** + migration (id, asset_id, text_content, method, created_at)
- **New repos** — `JobRepo`, `ExtractionRepo` in `packages/store`
- **Tauri commands** — `start_ocr_job(asset_id) → job_id`, `get_job(job_id) → Job`
- **Frontend** — `ocr.ts` Tauri client, "Extract Text" button in `ItemView`, per-asset progress indicator, extracted-text panel
- **Strict TDD** — all new Rust logic and new repos covered by tests

### Out of Scope

- NLP (NER, embeddings, triples) → Fase 3
- FTS5 full-text search upgrade → Fase 3
- `pdfium-render` DLL for complex PDF layouts → Fase 2.5 (quality upgrade)
- Multi-language / non-Latin OCR → future
- Cloud OCR APIs → never (offline-first design principle)
- Drag-and-drop file import UX → separate change (UX debt)
- Parallel / concurrent OCR jobs → Fase 3

## Capabilities

### New Capabilities

- `ocr-processing`: End-to-end job lifecycle — trigger, queue, extract (PDF native or OCR), store result, emit progress events
- `text-extraction-store`: `extractions` table schema, `ExtractionRepo`, `JobRepo`; query extraction by asset

### Modified Capabilities

- `data-store`: New `extractions` table + migration; `AppDb` refactored to dual-connection state; new `JobRepo`; existing `data-store` spec gains new requirements for `extractions` and job repository

## Approach

1. **Rust backend** — add `ocr/mod.rs` (commands + queue setup), `ocr/worker.rs` (serial loop), `ocr/engine.rs` (`ocrs` wrapper), `ocr/pdf.rs` (`pdf-extract` + heuristic), `ocr/preprocessor.rs` (image pipeline). Register commands and spawn worker in `lib.rs` `setup()`.
2. **DB** — `AppDbState` struct replaces `AppDb`. `worker_conn` used exclusively by `OcrWorker`; `ui_conn` used by existing IPC bridge. Both open with WAL mode.
3. **Store** — `JobRepo` (create, updateStatus, findById, findByAsset) + `ExtractionRepo` (create, findByAsset, upsert). New Drizzle table definition for `extractions`.
4. **Frontend** — `ocr.ts` wraps `invoke("start_ocr_job")` and `listen("ocr:*")`. `ocrStore` Svelte store holds per-job state. `ItemView` renders status badge + text panel.

## Affected Areas

| Area                                     | Impact   | Description                                     |
| ---------------------------------------- | -------- | ----------------------------------------------- |
| `apps/desktop/src-tauri/Cargo.toml`      | Modified | Add `ocrs`, `pdf-extract`, `imageproc` deps     |
| `apps/desktop/src-tauri/src/ocr/`        | New      | OCR module (worker, engine, pdf, preprocessor)  |
| `apps/desktop/src-tauri/src/lib.rs`      | Modified | Register OCR commands, spawn worker on setup    |
| `apps/desktop/src-tauri/src/db/state.rs` | Modified | `AppDb` → `AppDbState` dual-connection refactor |
| `apps/desktop/src-tauri/resources/`      | New      | Bundled `ocrs` model files (~20 MB)             |
| `packages/store/src/schema.ts`           | Modified | Add `extractions` table definition              |
| `packages/store/src/repos/job.ts`        | New      | `JobRepo`                                       |
| `packages/store/src/repos/extraction.ts` | New      | `ExtractionRepo`                                |
| `apps/desktop/src/lib/ocr.ts`            | New      | Tauri command + event client                    |
| `apps/desktop/src/views/ItemView.svelte` | Modified | "Extract Text" button, progress, text panel     |
| `apps/desktop/src/stores/ocrStore.ts`    | New      | Svelte store for per-job OCR state              |
| `drizzle/migrations/`                    | New      | Migration for `extractions` table               |

## Risks

| Risk                                                      | Likelihood | Mitigation                                                                    |
| --------------------------------------------------------- | ---------- | ----------------------------------------------------------------------------- |
| `AppDb` deadlock during refactor breaks existing tests    | Med        | Refactor `AppDb` first (own task), run full test suite before OCR work        |
| `pdf-extract` garbled text on old historical PDFs         | Med        | Quality heuristic (< 50 valid chars → OCR fallback); show word count in UI    |
| `ocrs` model bundle adds 20 MB to installer               | Low        | Acceptable for desktop app; historians work offline — no alternative          |
| `ocrs` accuracy insufficient for degraded manuscripts     | Low        | Acceptable for Fase 2; `tesseract` opt-in and deskew planned for Fase 3       |
| `spawn_blocking` exhausts Tokio thread pool on large PDFs | Low        | Serial queue limits to 1 concurrent job; monitor in Fase 3 for parallel needs |

## Rollback Plan

All new code lives in `src/ocr/` (new module) and `repos/job.ts` / `repos/extraction.ts` (additive). The `AppDbState` refactor is the only breaking change — it is isolated to `db/state.rs` and the command handlers that receive it. To rollback:

1. Revert `db/state.rs` to `AppDb(Mutex<Connection>)`
2. Remove `ocr/` module from `lib.rs`
3. Remove new store repos
4. Drop `extractions` table via a rollback migration

Existing Fase 1 functionality (collections, items, file import, viewer) is not affected by the new module.

## Dependencies

- `ocrs` v0.12.2 — pure Rust, no system deps (confirmed Windows-compatible)
- `pdf-extract` v0.10.0 — pure Rust, no system deps
- `imageproc` — pure Rust (transitively uses `image`, already present via `ocrs`)
- `ocrs` ONNX model files — static assets, downloaded once and committed to `resources/`

## Success Criteria

- [ ] "Extract Text" button triggers a job and returns a `job_id` without blocking the UI
- [ ] Progress updates arrive in the frontend as the job runs (% or page n/N)
- [ ] Born-digital PDFs: native text extracted correctly (≥ 90% of alphanumeric content intact)
- [ ] Scanned image PDFs/images: OCR fallback runs automatically when native extraction < 50 chars
- [ ] Extracted text persists in `extractions` table and survives app restart
- [ ] Re-running extraction on the same asset overwrites/updates the previous result
- [ ] Full test suite (113 + new) passes with `cargo test` and `vitest`
- [ ] App compiles with `cargo build` on Windows (no vcpkg, no system libs)
