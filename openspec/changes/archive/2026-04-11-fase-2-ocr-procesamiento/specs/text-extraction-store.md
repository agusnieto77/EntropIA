# Text Extraction Store Specification

## Purpose

Defines the `extractions` database table, the `ExtractionRepo` and `JobRepo` repositories, and the migration that introduces them. This spec covers all persistence operations for OCR results and job tracking.

## Requirements

### Requirement: Extractions Table Schema

The database MUST contain an `extractions` table with columns: `id TEXT PRIMARY KEY`, `asset_id TEXT NOT NULL` (FK → `assets.id`), `text_content TEXT NOT NULL`, `method TEXT NOT NULL` (values: `'native'` | `'ocr'`), `confidence REAL` (nullable), `created_at INTEGER NOT NULL`.

#### Scenario: Schema defines extractions table correctly

- GIVEN the Drizzle schema in `packages/store/src/schema.ts`
- WHEN the `extractions` table definition is inspected
- THEN columns `id`, `asset_id`, `text_content`, `method`, `confidence`, `created_at` are present with correct types
- AND `asset_id` references `assets.id`
- AND `method` is constrained to values `'native'` and `'ocr'`

#### Scenario: confidence is nullable

- GIVEN an extraction created via native PDF method (no inference confidence)
- WHEN the row is inserted with `confidence: null`
- THEN the row is stored without error

---

### Requirement: Migration 0003 — Extractions

A migration file `0003_extractions` MUST create the `extractions` table and an index on `asset_id`. The migration MUST be applied by the existing migration runner on startup.

#### Scenario: Migration creates table and index

- GIVEN the app has applied migrations 0001 and 0002 (Fase 1)
- WHEN migration 0003 is applied
- THEN the `extractions` table exists in the database
- AND an index on `extractions.asset_id` exists
- AND the `_drizzle_migrations` tracking table records `0003_extractions` as applied

#### Scenario: Migration is idempotent

- GIVEN migration 0003 has already been applied
- WHEN the migration runner executes again
- THEN 0003 is skipped without error

---

### Requirement: ExtractionRepo

`packages/store` MUST export an `ExtractionRepo` class accepting a `DrizzleClient` via constructor injection. It MUST provide:

| Method                    | Behaviour                                                                                          |
| ------------------------- | -------------------------------------------------------------------------------------------------- |
| `create(data)`            | Insert a new extraction row; returns the created row                                               |
| `findByAsset(assetId)`    | Return the most recent extraction for the asset (ordered by `created_at DESC`, limit 1), or `null` |
| `findAllByAsset(assetId)` | Return all extractions for the asset ordered by `created_at DESC`                                  |
| `upsert(assetId, data)`   | Replace the latest extraction if one exists; otherwise create. Single row per asset after call.    |
| `delete(id)`              | Delete extraction row by id                                                                        |

#### Scenario: Create and retrieve latest extraction

- GIVEN an `ExtractionRepo` with a valid DB client
- WHEN `create({ asset_id, text_content: 'Acta...', method: 'native', confidence: null })` is called
- THEN `findByAsset(asset_id)` returns that extraction

#### Scenario: findByAsset returns most recent when multiple exist

- GIVEN an asset has two extractions with different `created_at` values
- WHEN `findByAsset(asset_id)` is called
- THEN only the most recent extraction is returned

#### Scenario: upsert replaces existing extraction

- GIVEN an asset already has one extraction
- WHEN `upsert(asset_id, { text_content: 'Updated', method: 'ocr' })` is called
- THEN `findByAsset(asset_id)` returns the new extraction
- AND `findAllByAsset(asset_id)` returns exactly one row

#### Scenario: findByAsset returns null when no extraction exists

- GIVEN an asset with no extractions
- WHEN `findByAsset(asset_id)` is called
- THEN it returns `null`

---

### Requirement: JobRepo

`packages/store` MUST export a `JobRepo` class accepting a `DrizzleClient` via constructor injection. It MUST provide:

| Method                                | Behaviour                                                                |
| ------------------------------------- | ------------------------------------------------------------------------ |
| `create(assetId)`                     | Insert job with status `pending`; returns `job_id`                       |
| `findById(jobId)`                     | Return job row or `null`                                                 |
| `findByAsset(assetId)`                | Return all jobs for the asset ordered by `created_at DESC`               |
| `findPending()`                       | Return all jobs with status `pending` ordered by `created_at ASC` (FIFO) |
| `updateStatus(jobId, status, error?)` | Update job status; if `error` provided, write to `jobs.error` column     |
| `updateProgress(jobId, progress)`     | Update `jobs.progress` (0–100)                                           |

#### Scenario: Create job returns pending status

- GIVEN a valid `asset_id`
- WHEN `create(asset_id)` is called
- THEN a job row with status `'pending'` is inserted
- AND the returned `job_id` is a non-empty string

#### Scenario: findPending returns jobs in FIFO order

- GIVEN three jobs created in sequence for different assets, all with status `pending`
- WHEN `findPending()` is called
- THEN jobs are returned in creation order (oldest first)

#### Scenario: updateStatus transitions to error with message

- GIVEN a job in `running` state
- WHEN `updateStatus(jobId, 'error', 'inference crashed')` is called
- THEN `findById(jobId)` returns status `'error'`
- AND `error` field contains `'inference crashed'`

#### Scenario: updateProgress stores percentage

- GIVEN a job in `running` state
- WHEN `updateProgress(jobId, 45)` is called
- THEN `findById(jobId)` returns `progress: 45`

---

### Requirement: Text Access by Asset

Given an `assetId`, the system MUST allow retrieval of the latest extracted text in a single call. This is the primary read path for the UI text panel.

#### Scenario: Retrieve latest text for asset

- GIVEN an asset with a completed extraction
- WHEN `ExtractionRepo.findByAsset(assetId)` is called
- THEN `text_content` is returned as a non-empty string
- AND `method` indicates whether native or OCR was used
