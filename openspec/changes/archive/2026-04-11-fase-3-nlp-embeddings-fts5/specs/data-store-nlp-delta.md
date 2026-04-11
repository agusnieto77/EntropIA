# Delta for Data Store — NLP Tables and Repos

## ADDED Requirements

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

## MODIFIED Requirements

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

---

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
