# Embeddings Specification

## Purpose

Defines vector embedding generation, storage, and similarity search for EntropIA items using fastembed (all-MiniLM-L6-v2) and sqlite-vec.

## Requirements

### Requirement: Embed Item Command

The system MUST expose an `embed_item` Tauri command that accepts an `item_id`, reads the item's extracted text, computes a 384-dimension vector embedding using fastembed `all-MiniLM-L6-v2`, and stores the vector in the `vec_items` sqlite-vec table. If the item has no extraction, the command MUST return without error and without writing any embedding.

#### Scenario: Embed item with extraction

- GIVEN an item with id `item-1` has a non-empty extraction
- WHEN `embed_item({ item_id: 'item-1' })` is invoked
- THEN a 384-dimension float32 vector is computed
- AND a row is inserted or replaced in `vec_items` with `item_id` and the vector
- AND the command returns success

#### Scenario: Embed item without extraction (no-op)

- GIVEN an item has no extraction record
- WHEN `embed_item({ item_id })` is invoked
- THEN no vector is written to `vec_items`
- AND the command returns success without error

#### Scenario: Re-embed replaces existing vector

- GIVEN `vec_items` already contains a vector for `item-1`
- WHEN `embed_item({ item_id: 'item-1' })` is invoked again
- THEN the existing vector is replaced with the newly computed one
- AND `vec_items` contains exactly one row for `item-1`

---

### Requirement: Similar Items Command

The system MUST expose a `similar_items` Tauri command that accepts an `item_id` and a `limit` (default 5, max 20). The command MUST perform an ANN search using sqlite-vec `knn_search` on `vec_items` and return the top-N most similar items (excluding the query item itself), each including `item_id`, `distance`, and item metadata.

#### Scenario: Returns similar items ordered by distance

- GIVEN `vec_items` contains embeddings for items A, B, C, D
- WHEN `similar_items({ item_id: A, limit: 3 })` is called
- THEN up to 3 items are returned (not including A)
- AND results are ordered by cosine distance ascending (most similar first)

#### Scenario: Item with no embedding returns empty

- GIVEN item X has no entry in `vec_items`
- WHEN `similar_items({ item_id: X })` is called
- THEN an empty array is returned
- AND no error is thrown

#### Scenario: Limit is respected

- GIVEN `vec_items` contains 10 items
- WHEN `similar_items({ item_id: A, limit: 3 })` is called
- THEN exactly 3 results are returned (or fewer if not enough items exist)

---

### Requirement: NLP Queue for Background Embedding

The system MUST process embedding jobs through an `NlpQueue` background worker using a serial `mpsc` channel and `tokio::spawn`. The queue MUST process one embedding at a time. The queue MUST NOT block the Tauri IPC bridge or the OCR queue.

#### Scenario: Queue processes embedding job non-blocking

- GIVEN an `embed_item` job is sent to the NlpQueue
- WHEN the queue is processing
- THEN the Tauri UI thread remains responsive
- AND the IPC bridge can serve concurrent requests

#### Scenario: Multiple items queued process sequentially

- GIVEN 5 items are sent to the NlpQueue for embedding
- WHEN the queue processes them
- THEN they are processed one at a time in FIFO order
- AND all 5 embeddings are eventually written to `vec_items`

---

### Requirement: Embedding Job Type

The `jobs` table MUST support a new job type `'embeddings'`. `JobRepo.create` MUST accept `type: 'ocr' | 'embeddings' | 'ner'` as a parameter. The NlpQueue MUST use `JobRepo` to track embedding job status through the standard `pending → running → done | error` lifecycle.

#### Scenario: Embedding job tracked in jobs table

- GIVEN an item is queued for embedding
- WHEN the NlpQueue picks it up
- THEN a job row with type `'embeddings'` transitions from `pending` to `running` to `done`

#### Scenario: Embedding failure recorded

- GIVEN an embedding job encounters an error (e.g., model load failure)
- WHEN the error occurs
- THEN the job status is set to `'error'`
- AND the error message is persisted in `jobs.error`
- AND an `nlp:error` event is emitted

---

### Requirement: Re-Embed on Extraction Update

When an extraction is saved or updated for an item that already has an embedding, the system MUST enqueue a re-embedding job for that item via the NlpQueue.

#### Scenario: Extraction update triggers re-embed

- GIVEN an item already has a vector in `vec_items`
- WHEN the item's extraction is updated (e.g., via `ExtractionRepo.upsert`)
- THEN the NlpQueue receives a new embedding job for that item
- AND upon completion the vector in `vec_items` is replaced with the new one

---

### Requirement: NLP Tauri Events

The system MUST emit Tauri events during the NLP lifecycle: `nlp:progress` (with `job_id`, `item_id`, `step`), `nlp:complete` (with `job_id`, `item_id`), and `nlp:error` (with `job_id`, `item_id`, `error`).

#### Scenario: Complete event emitted after successful embed

- GIVEN an embedding job completes successfully
- WHEN the NlpQueue finishes the job
- THEN an `nlp:complete` event is emitted with the `job_id` and `item_id`

#### Scenario: Error event emitted on failure

- GIVEN an embedding job fails
- WHEN the NlpQueue catches the error
- THEN an `nlp:error` event is emitted with the error details
