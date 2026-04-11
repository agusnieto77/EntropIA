# Delta for Data Store

## ADDED Requirements

### Requirement: Dual-Connection Database State

The Tauri backend MUST expose the SQLite connection as `AppDbState { ui_conn: Mutex<Connection>, worker_conn: Mutex<Connection> }` with both connections opened in WAL mode. The `ui_conn` MUST be used exclusively by the IPC bridge; the `worker_conn` MUST be used exclusively by the OCR worker. Concurrent access MUST NOT cause deadlocks.

#### Scenario: IPC and OCR worker run concurrently without deadlock

- GIVEN the application is running with `AppDbState` managing two connections
- WHEN an OCR job is running (holding `worker_conn`) and the frontend issues a `select` command (using `ui_conn`)
- THEN both operations complete successfully
- AND neither blocks or errors due to database lock contention

#### Scenario: Both connections open in WAL mode

- GIVEN the application initialises `AppDbState`
- WHEN both connections are opened
- THEN `PRAGMA journal_mode=WAL` is active on each connection

---

### Requirement: Job Repository (data-store)

`packages/store` MUST export a `JobRepo` class that provides `create(assetId)`, `findById(jobId)`, `findByAsset(assetId)`, `findPending()`, `updateStatus(jobId, status, error?)`, and `updateProgress(jobId, progress)`. All methods MUST accept a `DrizzleClient` via constructor injection.

#### Scenario: Create job and find pending

- GIVEN a `JobRepo` with a valid DB client
- WHEN `create(asset_id)` is called
- THEN `findPending()` includes the new job with status `'pending'`

#### Scenario: updateStatus persists transition

- GIVEN a job with status `pending`
- WHEN `updateStatus(jobId, 'running')` is called
- THEN `findById(jobId)` returns status `'running'`

---

### Requirement: Extraction Repository (data-store)

`packages/store` MUST export an `ExtractionRepo` class that provides `create(data)`, `findByAsset(assetId)`, `findAllByAsset(assetId)`, `upsert(assetId, data)`, and `delete(id)`. All methods MUST accept a `DrizzleClient` via constructor injection.

#### Scenario: Create and retrieve extraction

- GIVEN an `ExtractionRepo` with a valid DB client
- WHEN `create({ asset_id, text_content, method: 'native' })` is called
- THEN `findByAsset(asset_id)` returns the newly created extraction

#### Scenario: upsert maintains single extraction per asset

- GIVEN an asset already has one extraction
- WHEN `upsert(asset_id, { text_content: 'New text', method: 'ocr' })` is called
- THEN `findAllByAsset(asset_id)` returns exactly one row with the new content

---

### Requirement: Migration 0003 — Extractions Table

The migration `0003_extractions` MUST be bundled with the application and applied by the existing migration runner. It MUST create the `extractions` table with columns `id TEXT PRIMARY KEY`, `asset_id TEXT NOT NULL REFERENCES assets(id)`, `text_content TEXT NOT NULL`, `method TEXT NOT NULL`, `confidence REAL`, `created_at INTEGER NOT NULL`. It MUST also create an index on `extractions(asset_id)`.

#### Scenario: Migration 0003 applied after Fase 1 migrations

- GIVEN a database with migrations 0001 and 0002 applied
- WHEN the application starts with migration 0003 bundled
- THEN the `extractions` table is created
- AND the index on `asset_id` exists
- AND `_drizzle_migrations` records `0003_extractions` as applied

#### Scenario: Running migration 0003 twice is safe

- GIVEN migration 0003 has already been applied
- WHEN the migration runner executes again
- THEN no error is thrown and the table is not re-created
