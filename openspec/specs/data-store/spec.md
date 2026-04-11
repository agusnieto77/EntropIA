# Data Store Specification

## Purpose

Defines the SQLite database lifecycle, IPC bridge, Drizzle ORM integration, schema, and migration runner that form the persistence layer of EntropIA.

## Requirements

### Requirement: Database File Creation

The SQLite database file MUST be created automatically in the Tauri `appDataDir` on the application's first launch. Subsequent launches MUST reuse the existing file.

#### Scenario: First launch creates database

- GIVEN the application has never been launched on this machine
- WHEN the app starts for the first time
- THEN a SQLite database file is created inside `appDataDir`
- AND the file persists after the app closes

#### Scenario: Subsequent launch reuses database

- GIVEN a database file already exists in `appDataDir`
- WHEN the app starts again
- THEN the existing database is opened without data loss

### Requirement: IPC Bridge

The Tauri backend MUST expose `execute` and `select` commands via IPC, allowing the JavaScript frontend to run SQL statements against the SQLite database. `execute` is for write operations (INSERT, UPDATE, DELETE, DDL). `select` is for read operations (SELECT).

#### Scenario: Select returns rows

- GIVEN the database contains rows in the `collections` table
- WHEN the frontend invokes the `select` IPC command with a SELECT query
- THEN the command returns an array of row objects

#### Scenario: Execute runs write operations

- GIVEN a valid INSERT statement and parameters
- WHEN the frontend invokes the `execute` IPC command
- THEN the row is inserted into the database
- AND the command returns the number of affected rows

### Requirement: Drizzle sqlite-proxy Client

`packages/store` MUST export a Drizzle client configured with the `sqlite-proxy` adapter. The proxy callbacks MUST delegate to the Tauri IPC bridge for all SQL execution.

#### Scenario: Drizzle query uses IPC bridge

- GIVEN a Drizzle client initialized with `sqlite-proxy`
- WHEN a type-safe query is executed (e.g., `db.select().from(collections)`)
- THEN the generated SQL is sent through the IPC bridge
- AND results are returned as typed objects matching the Drizzle schema

### Requirement: Base Schema

`packages/store` MUST define Drizzle schema tables for: `collections`, `items`, `assets`, `notes`, `jobs`, and `extractions`. All tables MUST use `TEXT` primary keys and include `created_at` timestamps.

#### Scenario: Schema defines all base tables

- GIVEN the Drizzle schema file in `packages/store`
- WHEN the schema is inspected
- THEN tables `collections`, `items`, `assets`, `notes`, `jobs`, and `extractions` are defined
- AND each table has a `TEXT` primary key column named `id`
- AND each table has a `created_at` column

#### Scenario: Foreign key relationships

- GIVEN the schema defines `items.collection_id`, `assets.item_id`, `notes.item_id`, `jobs.asset_id`, and `extractions.asset_id`
- WHEN these columns are inspected
- THEN each references the `id` column of its parent table

### Requirement: Migration Runner

The app MUST apply pending SQL migration files in sequential order on startup. Migrations MUST be generated at dev time by `drizzle-kit generate` and bundled with the application.

#### Scenario: Pending migrations applied on startup

- GIVEN 3 migration SQL files exist and 1 has already been applied
- WHEN the app starts
- THEN the 2 unapplied migrations are executed in filename order
- AND each is recorded in the migrations tracking table

#### Scenario: No pending migrations is a no-op

- GIVEN all migration files have already been applied
- WHEN the app starts
- THEN no SQL is executed and startup completes normally

### Requirement: Migration Idempotency

The migration runner MUST be safe to re-run. It MUST track applied migrations in a `_drizzle_migrations` table and MUST NOT re-apply already-applied migrations.

#### Scenario: Re-running migrations is safe

- GIVEN all migrations have been applied
- WHEN the migration runner executes again (e.g., on next app start)
- THEN no migrations are re-applied
- AND no errors occur

#### Scenario: Interrupted migration is recoverable

- GIVEN a migration was partially applied (app crashed mid-migration)
- WHEN the app restarts and the migration runner executes
- THEN the runner SHOULD detect the incomplete state
- AND either complete or re-apply the failed migration safely

### Requirement: Collection Repository

`packages/store` MUST export a `CollectionRepo` class that provides: `create(name, description?)`, `findAll()`, `findById(id)`, `update(id, data)`, `delete(id)`, and `countItems(id)`. All methods MUST accept a `DrizzleClient` via constructor injection.

#### Scenario: Create and retrieve a collection

- GIVEN a `CollectionRepo` with a valid DB client
- WHEN `create({ name: 'Test' })` is called
- THEN a new row is inserted in `collections`
- AND `findAll()` returns the created collection

#### Scenario: Count items in collection

- GIVEN a collection with 5 items
- WHEN `countItems(collectionId)` is called
- THEN it returns `5`

### Requirement: Item Repository

`packages/store` MUST export an `ItemRepo` class that provides: `create(collectionId, title)`, `findByCollection(collectionId, options?)`, `findById(id)`, `update(id, data)`, `delete(id)`, and `searchByText(term, collectionId?)`.

#### Scenario: List items by collection with pagination

- GIVEN a collection with 75 items
- WHEN `findByCollection(id, { limit: 50, offset: 0 })` is called
- THEN it returns the first 50 items sorted by `created_at` descending

#### Scenario: Search items by text using LIKE

- GIVEN items with titles "Acta de cabildo" and "Carta al gobernador"
- WHEN `searchByText('cabildo')` is called
- THEN it returns only the "Acta de cabildo" item

#### Scenario: Search metadata content

- GIVEN an item with metadata `{"author": "Moreno"}`
- WHEN `searchByText('Moreno')` is called
- THEN that item is included in results

### Requirement: Asset Repository

`packages/store` MUST export an `AssetRepo` class that provides: `create(itemId, filename, mimeType, size, path)`, `findByItem(itemId)`, and `delete(id)`.

#### Scenario: Create and list assets for item

- GIVEN an `AssetRepo` with a valid DB client
- WHEN `create(itemId, 'doc.pdf', 'application/pdf', 1024, 'assets/c/i/doc.pdf')` is called
- THEN `findByItem(itemId)` returns the asset

### Requirement: Note Repository

`packages/store` MUST export a `NoteRepo` class that provides: `create(itemId, content)`, `findByItem(itemId)`, `update(id, content)`, and `delete(id)`.

#### Scenario: Create and list notes for item

- GIVEN a `NoteRepo` with a valid DB client
- WHEN `create(itemId, 'Revisar fecha')` is called
- THEN `findByItem(itemId)` returns the note sorted by `created_at` descending

### Requirement: DrizzleClient Type Export

`packages/store` MUST export a `DrizzleClient` TypeScript type that represents the Drizzle instance. Repository constructors MUST accept this type for dependency injection and testability.

#### Scenario: Repos accept injected client

- GIVEN a mock `DrizzleClient` conforming to the exported type
- WHEN passed to a repository constructor
- THEN the repository operates using the mock without errors

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
