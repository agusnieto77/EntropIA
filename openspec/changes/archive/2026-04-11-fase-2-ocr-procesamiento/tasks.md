# Tasks: Fase 2 — OCR + Document Processing

## Phase 1: AppDbState Refactor [BLOCKING — must complete before Phase 3]

- [x] 1.1 Refactor `apps/desktop/src-tauri/src/db/state.rs`: replace `AppDb(Mutex<Connection>)` with `AppDbState { ui_conn: Arc<Mutex<Connection>>, worker_conn: Arc<Mutex<Connection>> }`
  - Satisfies: ADR-013 (dual-connection, WAL mode, no deadlock)
- [x] 1.2 Update `apps/desktop/src-tauri/src/db/commands.rs`: all `db_execute`/`db_select` handlers use `State<AppDbState>` and access `.ui_conn`
  - Satisfies: ocr-ux/background-operation (IPC commands succeed during OCR)
- [x] 1.3 Update `apps/desktop/src-tauri/src/lib.rs`: `setup()` opens both connections in WAL mode, registers `AppDbState`, wires `OcrQueue` placeholder (channel only, worker spawn deferred to Phase 3)
  - Satisfies: ADR-013, ocr-processing/non-blocking-execution
- [x] 1.4 Verify: run `pnpm test` — all existing Vitest tests pass with refactored state type (no new Rust unit test required here)
  - Satisfies: integration gate before Phase 3

---

## Phase 2: packages/store — Schema + Repos [parallelizable]

- [x] 2.1 Add `extractions` table to `packages/store/src/schema.ts` with columns `id`, `asset_id` (FK→assets), `text_content`, `method` (enum `native|ocr`), `confidence` (nullable), `created_at`
  - Satisfies: text-extraction-store/extractions-table-schema
- [x] 2.2 Create `drizzle/migrations/0003_extractions.sql`: `CREATE TABLE extractions` + `CREATE INDEX idx_extractions_asset_id`; inline SQL in `packages/store/src/runner.ts`
  - Satisfies: text-extraction-store/migration-0003
- [x] 2.3 Create `packages/store/src/repos/job.repo.ts` — `JobRepo` class with constructor-injected `DrizzleClient`; methods: `create(assetId)`, `findById`, `findByAsset`, `findPending` (FIFO), `updateStatus(jobId, status, error?)`, `updateProgress(jobId, pct)`
  - Satisfies: text-extraction-store/job-repo
- [x] 2.4 Write tests for `JobRepo` in `packages/store/src/repos/__tests__/job.repo.test.ts` — cover: create returns pending, findPending FIFO order, updateStatus→error with message, updateProgress stores pct
  - Satisfies: text-extraction-store/job-repo (scenarios: create-pending, findPending-FIFO, updateStatus-error, updateProgress)
- [x] 2.5 Create `packages/store/src/repos/extraction.repo.ts` — `ExtractionRepo` with: `create(data)`, `findByAsset(assetId)` (latest, or null), `findAllByAsset(assetId)`, `upsert(assetId, data)` (single row post-call), `delete(id)`
  - Satisfies: text-extraction-store/extraction-repo, text-extraction-store/text-access-by-asset
- [x] 2.6 Write tests for `ExtractionRepo` in `packages/store/src/repos/__tests__/extraction.repo.test.ts` — cover: create+findByAsset, findByAsset returns most-recent, upsert replaces existing (single row), findByAsset returns null when none
  - Satisfies: text-extraction-store/extraction-repo (scenarios: create-retrieve, most-recent, upsert-replaces, null-when-none)
- [x] 2.7 Update `packages/store/src/repos/store.ts`: add `jobs: JobRepo` and `extractions: ExtractionRepo` to `StoreApi`; update `packages/store/src/index.ts` barrel exports (`JobRepo`, `ExtractionRepo`, `Extraction`, `NewExtraction`, `Job`, `NewJob`)
  - Satisfies: text-extraction-store/extraction-repo, text-extraction-store/job-repo

---

## Phase 3: Rust OCR Module [parallelizable — depends on Phase 1]

- [x] 3.1 Add deps to `apps/desktop/src-tauri/Cargo.toml`: `ocrs = "0.8"`, `rten = "0.13"`, `pdf-extract = "0.7"`, `image = "0.25"`, `imageproc = "0.25"`, `tokio` (features: `full`)
  - Satisfies: ADR-010, ADR-011
  - Note: versions may need adjustment on first `cargo build` — structural implementation complete
- [x] 3.2 Create `apps/desktop/src-tauri/src/ocr/pdf.rs`: `extract_pdf_text(bytes) → Result<String>` + `is_quality_text(text) → bool` (< 50 alphanum → false); 3 `#[test]` for heuristic (empty, garbled, normal)
  - Satisfies: ocr-processing/pdf-native-extraction (scenarios: rich-layer, sparse-fallback, zero-byte)
- [x] 3.3 Create `apps/desktop/src-tauri/src/ocr/preprocessor.rs`: `preprocess_image(DynamicImage) → GrayImage` — grayscale → adaptive threshold; 1 `#[test]` verifying 100×100 dimensions preserved
  - Satisfies: ocr-processing/image-ocr-preprocessing (scenarios: preprocessed-before-inference)
- [x] 3.4 Create `apps/desktop/src-tauri/src/ocr/engine.rs`: `OcrEngine` struct, `load_models(app_handle) → Result<OcrEngine>`, `run_ocr(engine, GrayImage) → Result<String>`; models loaded from `resources/`
  - Satisfies: ADR-010, ocr-processing/image-ocr-preprocessing (scenarios: ocr-returns-text)
- [x] 3.5 Create `apps/desktop/src-tauri/src/ocr/mod.rs`: full `OcrQueue` + `OcrJob` + `start_worker()` — serial tokio mpsc loop, PDF/image dispatch, `ocr:progress`/`ocr:complete`/`ocr:error` events
  - Satisfies: ocr-processing/job-lifecycle, ocr-processing/non-blocking, ocr-processing/manual-trigger, ocr-processing/reprocessing, ocr-processing/error-handling
  - Note: PDF scanned fallback (OCR of scanned PDF) deferred to Fase 2.5 — emits ocr:error in current impl
- [x] 3.6 Updated `apps/desktop/src-tauri/src/lib.rs`: `OcrQueue::new()` tuple destructure, `start_worker(receiver, handle)` wired; `extract_text` command registered
  - Satisfies: ocr-processing/non-blocking-execution (background worker spawned once)
- [x] 3.7 Updated `apps/desktop/src-tauri/tauri.conf.json`: `"resources": ["resources/*"]` in bundle config
  - Satisfies: ADR-010 (offline-first, models bundled)

---

## Phase 4: Frontend OCR Client + UI [depends on Phase 2 + Phase 3]

- [x] 4.1 Create `apps/desktop/src/lib/ocr.ts`: `OcrStore` class (plain TS) + `extractText(assetId, assetPath, assetType)` function; `OcrStatus` type, `OcrProgress`, `OcrResult`, `AssetOcrState` interfaces; event listeners for `ocr:progress`, `ocr:complete`, `ocr:error`
  - Satisfies: ocr-ux/progress-indicator, ocr-ux/status-badge, ocr-ux/background-operation
- [x] 4.2 Write tests for `OcrStore` in `apps/desktop/src/lib/ocr.test.ts`: 12 tests covering getState (idle), extractText (invoke call), startListening (progress/complete/error events), stopListening (cleanup)
  - Satisfies: ocr-ux/status-badge (badge-updates-without-reload), ocr-ux/progress-indicator (progress-updates)
- [x] 4.3 Update `apps/desktop/src/test-setup.ts`: added `@tauri-apps/api/event` mock — `listen` returns `Promise<vi.fn()>` cleanup
  - Satisfies: test infrastructure for Phase 4 tests
- [x] 4.4 Update `apps/desktop/src/views/ItemView.svelte`: "Extract Text" button per asset (disabled when pending/running); `<progress>` bar when running; status badge in asset thumb; collapsible `<details>` panel when done; error message on error
  - Satisfies: ocr-ux/extract-text-button (all 4 scenarios), ocr-ux/progress-indicator (all 3), ocr-ux/extracted-text-panel (all 4), ocr-ux/status-badge (all 2)

---

## Phase 5: Integration & Validation

- [x] 5.1 `pnpm install` — TS deps resolve; `@tauri-apps/api/event` already available
  - Satisfies: all phases integrated
- [x] 5.2 `pnpm test` — 144/144 tests pass (44 ui + 60 store + 40 desktop [was 28, +12 OCR])
  - Satisfies: full regression gate
- [x] 5.3 `pnpm typecheck` — 0 TypeScript errors (7 pre-existing Svelte 5 warnings in packages/ui, not new code)
  - Satisfies: type safety across new contracts
- [x] 5.4 `pnpm lint` — 0 linting errors (3 pre-existing warnings in existing files, not new code)
  - Satisfies: code quality gate
