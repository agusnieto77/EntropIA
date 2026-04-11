# Design: Fase 2 — OCR + Document Processing

## Technical Approach

The pipeline follows a **fire-and-forget command → serial queue → event-driven feedback** model. The Tauri command `start_ocr_job` enqueues a job ID over an `mpsc::channel` and returns immediately; a single background worker (spawned once in `setup()`) drains the queue serially, delegates CPU-heavy work to `spawn_blocking`, and emits `ocr:progress` / `ocr:complete` / `ocr:error` events back to the frontend. This keeps the UI thread free and limits resource usage to one concurrent job — appropriate for a single-user desktop app.

The only breaking change is the `AppDb` → `AppDbState` refactor: the existing single `Mutex<Connection>` is replaced by two separate connections (`ui_conn` for IPC commands, `worker_conn` for the OCR worker). Both open in WAL mode, which SQLite already configured in `lib.rs`. This eliminates deadlock without any concurrency primitives beyond the existing per-connection `Mutex`.

---

## Architecture Decisions

### ADR-010: OCR Engine — `ocrs` v0.12.2

| Option                           | Tradeoff                                                                                 | Decision      |
| -------------------------------- | ---------------------------------------------------------------------------------------- | ------------- |
| **`ocrs` (pure Rust)**           | Zero system deps; `cargo build` on Windows with no vcpkg; ML-based; ~20 MB model bundle  | ✅ **Chosen** |
| `tesseract-rs`                   | High accuracy but requires Tesseract system install + MSVC; breaks offline Windows build | ✗             |
| CLI subprocess (`tesseract.exe`) | Flexible but requires user to install Tesseract; breaks offline-first constraint         | ✗             |

**Rationale**: `ocrs` compiles to a single static binary with no vcpkg dependency — the only viable pure-`cargo build` option on Windows. Model files committed to `resources/` satisfy the offline-first requirement.

---

### ADR-011: PDF Text Extraction — `pdf-extract` + quality heuristic

| Option                        | Tradeoff                                                                           | Decision      |
| ----------------------------- | ---------------------------------------------------------------------------------- | ------------- |
| **`pdf-extract` + heuristic** | Pure Rust; fast for born-digital PDFs; heuristic triggers OCR fallback when needed | ✅ **Chosen** |
| Always OCR                    | Simple but slow; destroys accuracy for born-digital PDFs                           | ✗             |
| `pdfium-render`               | Better layout fidelity but requires pdfium DLL; breaks pure-`cargo build`          | ✗ (Fase 2.5)  |

**Rationale**: The quality heuristic (`< 50 valid UTF-8 alphanumeric chars → fallback to OCR`) handles the two main cases historians face: born-digital PDFs (fast native path) and scanned archival PDFs (OCR path). `pdfium-render` is deferred to Fase 2.5.

---

### ADR-012: Job Queue — Serial `mpsc::channel`

| Option                            | Tradeoff                                                                                  | Decision      |
| --------------------------------- | ----------------------------------------------------------------------------------------- | ------------- |
| **Serial `mpsc` + single worker** | Simple; prevents resource exhaustion; deterministic progress events                       | ✅ **Chosen** |
| Thread pool (`rayon`)             | Faster for parallel assets but complicates progress reporting and risks OOM on large PDFs | ✗             |
| OS thread per job                 | Simplest spawn model but unbounded thread creation for batch imports                      | ✗             |

**Rationale**: A single-user desktop app rarely needs parallel OCR. Serial execution simplifies progress math (one job active at a time) and eliminates thread-pool exhaustion risk for large PDFs.

---

### ADR-013: `AppDbState` Dual-Connection Refactor

| Option                                    | Tradeoff                                                                            | Decision      |
| ----------------------------------------- | ----------------------------------------------------------------------------------- | ------------- |
| **`AppDbState { ui_conn, worker_conn }`** | Two separate `Mutex<Connection>` in WAL mode; UI commands never block on worker     | ✅ **Chosen** |
| Single `Arc<Mutex<Connection>>` shared    | Deadlock risk: worker holds lock while UI command waits                             | ✗             |
| `tokio::sync::Mutex` for async unlock     | Requires fully async rusqlite usage; significant refactor of existing sync commands | ✗             |

**Rationale**: SQLite WAL mode allows concurrent readers and one writer without a global lock. Two separate connections give each context its own lock scope. `ui_conn` serves existing `db_execute`/`db_select` commands; `worker_conn` is owned exclusively by `OcrWorker`. No lock contention possible.

---

## Data Flow

### Flow 1 — OCR Job Submission

```
User clicks "Extract Text"
        │
        ▼
ocr.ts: invoke("start_ocr_job", { assetId })
        │
        ▼
Rust: start_ocr_job cmd
  ├─ validate asset exists (ui_conn)
  ├─ insert job row: status=pending (ui_conn)
  ├─ tx.send(job_id) → mpsc channel
  └─ return job_id to frontend
        │
        ▼ (background worker loop)
OcrWorker::run()
  ├─ rx.recv() → job_id
  ├─ load asset path (worker_conn)
  ├─ update job: status=running (worker_conn)
  ├─ emit ocr:progress { assetId, pct: 0, stage: "preprocessing" }
  │
  ├─ [PDF?] pdf::extract_native_text()
  │     └─ quality_heuristic < 50 chars? → fallback to OCR
  │
  ├─ [Image or fallback] preprocessor::pipeline()
  │     └─ grayscale → adaptive_threshold → resize
  │
  ├─ engine::run_ocr() → text  (spawn_blocking)
  ├─ emit ocr:progress { pct: 90, stage: "saving" }
  ├─ insert extractions row (worker_conn)
  ├─ update job: status=done (worker_conn)
  └─ emit ocr:complete { assetId, method, textLength }
```

### Flow 2 — Progress Reporting

```
OcrWorker (Rust)
  └─ app_handle.emit("ocr:progress", OcrProgress { assetId, pct, stage })
        │  (Tauri event system)
        ▼
ocr.ts: listen("ocr:progress", handler)
        │
        ▼
ocrStore.update(assetId, { pct, stage })   ← Svelte class store
        │
        ▼
ItemView.svelte: $ocrStore[assetId]         ← reactive binding
  ├─ <ProgressBar value={pct} />
  └─ stage badge: "preprocessing" | "extracting" | "saving"
```

---

## New Rust Module Structure

```
apps/desktop/src-tauri/src/
├── ocr/
│   ├── mod.rs          — OcrWorker struct, channel setup, start_ocr_job command
│   ├── engine.rs       — ocrs model loading + run_ocr(image) → String
│   ├── pdf.rs          — pdf-extract wrapper + quality_heuristic()
│   └── preprocessor.rs — image pipeline: grayscale → threshold → resize
├── db/
│   ├── state.rs        — REFACTOR: AppDbState { ui_conn, worker_conn }
│   └── commands.rs     — db_execute / db_select → use state.ui_conn
```

`OcrWorker` holds `Arc<AppHandle>` (for emit) and `worker_conn: Arc<Mutex<Connection>>` passed from `setup()`. The `mpsc::Sender<String>` is stored in Tauri state as `OcrQueue(Mutex<Sender<String>>)`.

---

## Schema Changes

### Migration `0003_extractions.sql`

```sql
CREATE TABLE extractions (
  id          TEXT    PRIMARY KEY,
  asset_id    TEXT    NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  text_content TEXT   NOT NULL,
  method      TEXT    NOT NULL CHECK(method IN ('native', 'ocr')),
  confidence  REAL,
  created_at  INTEGER NOT NULL
);
CREATE INDEX idx_extractions_asset_id ON extractions(asset_id);
```

> `jobs` table already exists in `schema.ts` (Fase 1 schema includes it). No new jobs migration needed — the table is already defined.

### `packages/store/src/schema.ts` addition

```typescript
export const extractions = sqliteTable('extractions', {
  id: text('id').primaryKey(),
  assetId: text('asset_id')
    .notNull()
    .references(() => assets.id),
  textContent: text('text_content').notNull(),
  method: text('method', { enum: ['native', 'ocr'] }).notNull(),
  confidence: integer('confidence'), // stored as 0–100 integer
  createdAt: integer('created_at').notNull(),
})
```

---

## Frontend Architecture

### `apps/desktop/src/lib/ocr.ts`

Exports:

- `startOcrJob(assetId: string): Promise<string>` — wraps `invoke("start_ocr_job")`
- `ocrStore` — Svelte class store:
  - `listen("ocr:progress")` → updates `Map<assetId, OcrProgress>`
  - `listen("ocr:complete")` → marks done, stores `OcrResult`
  - `listen("ocr:error")` → stores error message

### `apps/desktop/src/stores/ocrStore.ts`

```typescript
// Per-asset OCR state held in a Map
interface AssetOcrState {
  status: 'idle' | 'running' | 'done' | 'error'
  pct: number
  stage?: OcrProgress['stage']
  result?: OcrResult
  error?: string
}
```

### `apps/desktop/src/views/ItemView.svelte` additions

- "Extract Text" button (disabled while `status === 'running'`)
- `<ProgressBar>` shown when `status === 'running'`
- Collapsible `<ExtractionPanel>` showing `text_content` when `status === 'done'`
- Stage label: "Preprocessing…" / "Extracting…" / "Saving…"

---

## File Changes

| File                                               | Action | Description                                                           |
| -------------------------------------------------- | ------ | --------------------------------------------------------------------- |
| `apps/desktop/src-tauri/Cargo.toml`                | Modify | Add `ocrs`, `pdf-extract`, `imageproc`, `tokio` deps                  |
| `apps/desktop/src-tauri/src/lib.rs`                | Modify | Register OCR commands, manage `AppDbState` + `OcrQueue`, spawn worker |
| `apps/desktop/src-tauri/src/db/state.rs`           | Modify | `AppDb` → `AppDbState { ui_conn, worker_conn }`                       |
| `apps/desktop/src-tauri/src/db/commands.rs`        | Modify | Update `State<AppDb>` → `State<AppDbState>`, use `.ui_conn`           |
| `apps/desktop/src-tauri/src/ocr/mod.rs`            | Create | `OcrQueue` state, `start_ocr_job` command, `OcrWorker::spawn()`       |
| `apps/desktop/src-tauri/src/ocr/engine.rs`         | Create | `ocrs` model loader + `run_ocr(DynamicImage) → String`                |
| `apps/desktop/src-tauri/src/ocr/pdf.rs`            | Create | `extract_native_text(path) → String` + `quality_heuristic()`          |
| `apps/desktop/src-tauri/src/ocr/preprocessor.rs`   | Create | `pipeline(DynamicImage) → DynamicImage`                               |
| `apps/desktop/src-tauri/resources/ocrs_model.rten` | Create | Bundled ONNX model (~20 MB)                                           |
| `apps/desktop/src-tauri/resources/ocrs_vocab.rten` | Create | Bundled vocab file                                                    |
| `packages/store/src/schema.ts`                     | Modify | Add `extractions` table definition                                    |
| `packages/store/src/repos/extraction.repo.ts`      | Create | `ExtractionRepo` (create, findByAsset, upsert)                        |
| `packages/store/src/repos/job.repo.ts`             | Create | `JobRepo` (create, updateStatus, findById, findByAsset)               |
| `packages/store/src/repos/store.ts`                | Modify | Add `jobs: JobRepo`, `extractions: ExtractionRepo` to `StoreApi`      |
| `packages/store/src/index.ts`                      | Modify | Export new repos and types                                            |
| `apps/desktop/src/lib/ocr.ts`                      | Create | `startOcrJob()` Tauri wrapper + `ocrStore`                            |
| `apps/desktop/src/stores/ocrStore.ts`              | Create | Svelte class store for per-asset OCR state                            |
| `apps/desktop/src/views/ItemView.svelte`           | Modify | "Extract Text" button, progress bar, extraction panel                 |
| `drizzle/migrations/0003_extractions.sql`          | Create | `extractions` table + index migration                                 |

---

## Interfaces / Contracts

```typescript
// Tauri event payloads
interface OcrProgress {
  assetId: string
  pct: number // 0–100
  stage: 'preprocessing' | 'extracting' | 'saving'
}

interface OcrResult {
  assetId: string
  method: 'native' | 'ocr'
  textLength: number
}

interface OcrError {
  assetId: string
  message: string
}

// Drizzle inference
export type Extraction = typeof extractions.$inferSelect
export type NewExtraction = typeof extractions.$inferInsert

export type Job = typeof jobs.$inferSelect
export type NewJob = typeof jobs.$inferInsert
```

```rust
// Rust state types
pub struct AppDbState {
    pub ui_conn:     Arc<Mutex<Connection>>,
    pub worker_conn: Arc<Mutex<Connection>>,
}

pub struct OcrQueue(pub Mutex<mpsc::Sender<String>>);

pub struct OcrProgress {
    pub asset_id: String,
    pub pct:      u8,
    pub stage:    String,
}
```

---

## Testing Strategy

| Layer              | What to Test                                       | Approach                           |
| ------------------ | -------------------------------------------------- | ---------------------------------- |
| Unit (Rust)        | `quality_heuristic()` threshold logic              | `#[test]` in `pdf.rs`              |
| Unit (Rust)        | `preprocessor::pipeline()` output dimensions       | `#[test]` in `preprocessor.rs`     |
| Unit (Rust)        | `start_ocr_job` inserts job row + sends to channel | `#[test]` with in-memory SQLite    |
| Unit (TS)          | `ExtractionRepo.upsert` keeps 1 row per asset      | vitest + `db.mock.ts`              |
| Unit (TS)          | `JobRepo.updateStatus` transitions                 | vitest + `db.mock.ts`              |
| Integration (Rust) | `AppDbState` dual-conn: worker writes, UI reads    | `#[test]` with temp file DB in WAL |
| Integration (TS)   | `ocrStore` reacts to `ocr:progress` event          | vitest with mocked Tauri listen    |

---

## Migration / Rollout

1. **Task 1** — Refactor `AppDbState` first; run full test suite (`cargo test` + `vitest`). Gate all subsequent tasks on green.
2. **Task 2–5** — OCR Rust module (engine, pdf, preprocessor, worker) can land in any order within the module.
3. **Task 6** — Store repos (`JobRepo`, `ExtractionRepo`) are independent of Rust tasks.
4. **Task 7** — Frontend (`ocr.ts`, `ocrStore`, `ItemView`) lands last, depends on Tauri commands being registered.
5. Migration `0003_extractions.sql` runs on first app launch via `runMigrations()`.

---

## Open Questions

- [ ] `ocrs` model file exact filenames/paths — confirm with `ocrs` v0.12.2 release notes before committing to `resources/`
- [ ] `confidence` field type: `ocrs` returns per-word confidence — store as average `REAL` or omit for Fase 2? (Spec says optional; default to `NULL` if not available)
