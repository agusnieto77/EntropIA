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

`packages/store` MUST define Drizzle schema tables for: `collections`, `items`, `assets`, `notes`, `jobs`, `extractions`, `entities`, and `fts_items` (as a virtual table declaration). All tables MUST use `TEXT` primary keys and include `created_at` timestamps where applicable.

(Previously: schema defined only `collections`, `items`, `assets`, `notes`, `jobs`, `extractions`)

#### Scenario: Schema defines all base tables including NLP tables

- GIVEN the Drizzle schema file in `packages/store`
- WHEN the schema is inspected
- THEN tables `collections`, `items`, `assets`, `notes`, `jobs`, `extractions`, and `entities` are defined
- AND `fts_items` is declared as a virtual table reference
- AND each regular table has a `TEXT` primary key column named `id`
- AND each regular table has a `created_at` column

#### Scenario: Foreign key relationships

- GIVEN the schema defines `items.collection_id`, `assets.item_id`, `notes.item_id`, `jobs.asset_id`, `extractions.asset_id`, `entities.item_id`
- WHEN these columns are inspected
- THEN each references the `id` column of its parent table

### Requirement: Migration Runner

The app MUST apply pending migrations in sequential order on startup. Migration definitions MAY be sourced from bundled SQL files OR an in-code migration registry (for Tauri-safe bundling), as long as they are versioned and deterministic.

#### Scenario: Pending migrations applied on startup

- GIVEN 3 versioned migrations exist and 1 has already been applied
- WHEN the app starts
- THEN the 2 unapplied migrations are executed in filename order
- AND each is recorded in the migrations tracking table

#### Scenario: No pending migrations is a no-op

- GIVEN all known migrations have already been applied
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

`packages/store` MUST export an `ItemRepo` class that provides: `create(collectionId, title)`, `findByCollection(collectionId, options?)`, `findById(id)`, `update(id, data)`, `delete(id)`, and `searchByText(term, collectionId?)`. `searchByText` MUST use FTS5 MATCH via `fts_items` instead of SQL LIKE.

(Previously: `searchByText` used SQL LIKE on `items.title` and `items.metadata`)

#### Scenario: List items by collection with pagination

- GIVEN a collection with 75 items
- WHEN `findByCollection(id, { limit: 50, offset: 0 })` is called
- THEN it returns the first 50 items sorted by `created_at` descending

#### Scenario: Search items by text using FTS5

- GIVEN items with titles "Acta de cabildo" and "Carta al gobernador" are indexed in `fts_items`
- WHEN `searchByText('cabildo')` is called
- THEN it returns only the "Acta de cabildo" item

#### Scenario: Search extracted text content

- GIVEN an item whose extraction contains "Gobernador de la Provincia"
- WHEN `searchByText('Gobernador')` is called
- THEN that item is included in results

#### Scenario: Search scoped to collection

- GIVEN items in collection A and B are indexed
- WHEN `searchByText('cabildo', collectionIdA)` is called
- THEN only items from collection A appear in results

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

The Tauri backend MUST expose the SQLite connection as `AppDbState { ui_conn: Mutex<Connection>, worker_conn: Mutex<Connection>, nlp_conn: Mutex<Connection> }` with all three connections opened in WAL mode. The `ui_conn` MUST be used exclusively by the IPC bridge; the `worker_conn` MUST be used exclusively by the OCR worker; the `nlp_conn` MUST be used exclusively by the NLP worker. Concurrent access MUST NOT cause deadlocks.

(Previously: AppDbState had only `ui_conn` and `worker_conn` — two connections)

#### Scenario: IPC and OCR worker run concurrently without deadlock

- GIVEN the application is running with `AppDbState` managing three connections
- WHEN an OCR job is running (holding `worker_conn`) and the frontend issues a `select` command (using `ui_conn`)
- THEN both operations complete successfully
- AND neither blocks or errors due to database lock contention

#### Scenario: NLP and OCR workers run concurrently without deadlock

- GIVEN an NLP embedding job is running (holding `nlp_conn`) and an OCR job is running (holding `worker_conn`)
- WHEN both run simultaneously
- THEN both complete successfully without deadlock

#### Scenario: All connections open in WAL mode

- GIVEN the application initialises `AppDbState`
- WHEN all three connections are opened
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

---

### Requirement: Migration 0004 — FTS5 Virtual Table

Migration `0004_fts5` MUST create the `fts_items` FTS5 virtual table using `unicode61` tokenizer with columns `item_id UNINDEXED`, `title`, `metadata`, `extracted_text`. The migration MUST also populate `fts_items` with all existing items that have extractions at the time of migration.

#### Scenario: Migration 0004 creates fts_items

- GIVEN a database with migrations 0001–0003 applied
- WHEN migration 0004 is applied
- THEN `fts_items` virtual table exists
- AND `_drizzle_migrations` records `0004_fts5` as applied

#### Scenario: Migration 0004 backfills existing items

- GIVEN items with extractions exist before migration 0004
- WHEN migration 0004 is applied
- THEN those items are inserted into `fts_items`
- AND items without extractions are inserted with empty `extracted_text`

#### Scenario: Applying 0004 twice is safe

- GIVEN migration 0004 has already been applied
- WHEN the migration runner executes again
- THEN no error is thrown and the table is not re-created

---

### Requirement: Migration 0005 — Entities Table and sqlite-vec Init

Migration `0005_embeddings` MUST create the `entities` table with columns `id TEXT PRIMARY KEY`, `item_id TEXT NOT NULL REFERENCES items(id)`, `entity_type TEXT NOT NULL`, `value TEXT NOT NULL`, `start_offset INTEGER NOT NULL`, `end_offset INTEGER NOT NULL`, `confidence REAL NOT NULL`, `created_at INTEGER NOT NULL`. It MUST create an index on `entities(item_id)`. It MUST also initialize the sqlite-vec `vec_items` virtual table with dimension 384.

#### Scenario: Migration 0005 creates entities table

- GIVEN migration 0004 is applied
- WHEN migration 0005 is applied
- THEN the `entities` table exists with all required columns
- AND an index on `entities(item_id)` exists

#### Scenario: Migration 0005 creates vec_items table

- GIVEN migration 0004 is applied
- WHEN migration 0005 is applied
- THEN a `vec_items` virtual table exists (managed by sqlite-vec)
- AND it accepts 384-dimension float32 vectors

#### Scenario: Migration 0005 idempotent

- GIVEN migration 0005 has already been applied
- WHEN the migration runner executes again
- THEN no error is thrown

---

### Requirement: Jobs Type Enum Extension

The `jobs` table `type` column MUST accept values `'ocr'`, `'embeddings'`, and `'ner'`. `JobRepo.create` MUST accept a `type` parameter with these three values. Existing `'ocr'` jobs MUST continue to function without schema changes.

#### Scenario: Embeddings job created with correct type

- GIVEN a `JobRepo` with a valid DB client
- WHEN `create({ asset_id, type: 'embeddings' })` is called
- THEN a job row is inserted with `type = 'embeddings'` and `status = 'pending'`

#### Scenario: NER job created with correct type

- GIVEN a `JobRepo` with a valid DB client
- WHEN `create({ asset_id, type: 'ner' })` is called
- THEN a job row is inserted with `type = 'ner'` and `status = 'pending'`

#### Scenario: Existing OCR jobs unaffected

- GIVEN jobs of type `'ocr'` exist in the database
- WHEN migration 0005 is applied
- THEN all existing `'ocr'` job rows are intact and queryable

---

### Requirement: Entity Repository (StoreApi)

`packages/store` MUST export an `EntityRepo` class that provides `findByItemId(itemId)`, `create(data)`, and `deleteByItemId(itemId)`. All methods MUST accept a `DrizzleClient` via constructor injection. `packages/store` MUST re-export `EntityRepo` from its public index.

#### Scenario: EntityRepo create and findByItemId

- GIVEN an `EntityRepo` with a valid DB client
- WHEN `create({ item_id, entity_type: 'PERSON', value: 'Don Pedro', start_offset: 0, end_offset: 9, confidence: 0.95, created_at: now })` is called
- THEN `findByItemId(item_id)` returns an array containing the new entity

#### Scenario: EntityRepo deleteByItemId clears entities

- GIVEN 3 entity rows exist for item-1
- WHEN `deleteByItemId('item-1')` is called
- THEN `findByItemId('item-1')` returns an empty array

---

### Requirement: Embedding Repository (StoreApi)

`packages/store` MUST export an `EmbeddingRepo` class that provides `upsert(itemId, vector)`, `findByItemId(itemId)`, `deleteByItemId(itemId)`, and `knnSearch(vector, limit)`. All methods MUST accept a `DrizzleClient` via constructor injection. `knnSearch` MUST delegate to sqlite-vec `knn_search` and return an array of `{ item_id, distance }`. `packages/store` MUST re-export `EmbeddingRepo` from its public index.

#### Scenario: EmbeddingRepo upsert and findByItemId

- GIVEN an `EmbeddingRepo` with a valid DB client
- WHEN `upsert('item-1', Float32Array(384))` is called
- THEN `findByItemId('item-1')` returns the stored vector

#### Scenario: knnSearch returns nearest neighbors

- GIVEN 5 vectors are stored in `vec_items`
- WHEN `knnSearch(queryVector, 3)` is called
- THEN up to 3 results are returned ordered by distance ascending
- AND each result has `item_id` and `distance`

#### Scenario: EmbeddingRepo upsert replaces existing

- GIVEN `vec_items` already has a row for `item-1`
- WHEN `upsert('item-1', newVector)` is called
- THEN `findByItemId('item-1')` returns the new vector
- AND only one row exists for `item-1`

---

### Requirement: Triple-Connection Database State

The Tauri backend AppDbState MUST be extended to include an `nlp_conn: Mutex<Connection>` alongside `ui_conn` and `worker_conn`. The `nlp_conn` MUST have sqlite-vec extension loaded. The `nlp_conn` MUST be used exclusively by the NlpQueue. All three connections MUST be opened in WAL mode.

#### Scenario: NlpQueue uses nlp_conn without blocking IPC

- GIVEN the NlpQueue is processing an embedding job (holding `nlp_conn`)
- WHEN the frontend issues a `select` command (using `ui_conn`)
- THEN both operations complete successfully without lock contention

#### Scenario: sqlite-vec loaded on nlp_conn

- GIVEN the app initialises `AppDbState`
- WHEN `nlp_conn` is opened
- THEN the sqlite-vec extension is loaded
- AND `vec_items` virtual table operations succeed on that connection
