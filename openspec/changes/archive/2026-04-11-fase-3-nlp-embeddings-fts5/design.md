# Design: Fase 3 — NLP / Embeddings / FTS5

## Technical Approach

Extend the existing Rust worker + Tauri IPC + Drizzle store stack with three NLP capabilities — full-text search (FTS5), vector similarity (fastembed + sqlite-vec), and rule-based NER — all offline, all in the same SQLite file, mirroring the proven OcrQueue architecture.

---

## Architecture Decisions

### ADR-014: FTS5 Search Strategy

| Option                        | Tradeoff                                                                                  | Decision      |
| ----------------------------- | ----------------------------------------------------------------------------------------- | ------------- |
| **SQLite FTS5 virtual table** | Zero extra deps, offline, ranked results, same file, raw SQL through Drizzle sqlite-proxy | ✅ **Chosen** |
| Tantivy (Rust crate)          | High performance, external index file, Tauri bundleable but no SQL join                   | ❌ Rejected   |
| Meilisearch                   | Best UX, requires separate process, breaks offline-first                                  | ❌ Rejected   |

**Schema**:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS fts_items USING fts5(
  item_id UNINDEXED,
  title,
  metadata,
  extracted_text,
  tokenize = 'unicode61 remove_diacritics 1',
  content = ''
);
```

**Tokenizer**: `unicode61 remove_diacritics 1` — handles historical Spanish accents (ñ, é, ü) and diacritic normalization without custom tokenizer Rust code.

**Sync strategy**: application-level. When an extraction is saved → `NlpQueue.push(IndexFts(item_id))`. Migration 0004 includes a one-time backfill SELECT across `items JOIN assets JOIN extractions`.

**Query sanitization**: `sanitizeFts5Query(raw: string): string` — strips/escapes `AND`, `OR`, `NOT`, `NEAR`, `*`, `"`, `(`, `)`, `-`. Every `MATCH` call passes through this utility. Unit-tested in Vitest.

---

### ADR-015: Embedding Model & Storage

| Option                                            | Tradeoff                                                        | Decision                   |
| ------------------------------------------------- | --------------------------------------------------------------- | -------------------------- |
| **fastembed `all-MiniLM-L6-v2` (384-dim, 25 MB)** | Pure Rust, offline, ships as Tauri resource, proven crate       | ✅ **Chosen for MVP**      |
| `paraphrase-multilingual-MiniLM-L12-v2` (470 MB)  | Better Spanish recall, too large for MVP bundle                 | 🔲 Upgrade path documented |
| External API (OpenAI/Cohere)                      | Best quality, requires internet + API key, breaks offline-first | ❌ Rejected                |

**Model location**: `apps/desktop/src-tauri/resources/models/all-MiniLM-L6-v2/`  
**Cargo deps**:

```toml
fastembed = "5"
sqlite-vec = "0.1.10-alpha.3"
```

**Extension load** (in `lib.rs` setup, before `AppDbState::new`):

```rust
sqlite_vec::load(&worker_conn)?;
```

**vec0 schema** (migration 0005):

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS vec_items USING vec0(
  item_id TEXT PRIMARY KEY,
  embedding FLOAT[384]
);
```

**Fallback**: if sqlite-vec fails to load, `ComputeEmbedding` jobs log a warning and return `Ok(())` — embeddings are degraded silently; FTS5 and NER continue unaffected.

---

### ADR-016: NER Approach

| Option                                    | Tradeoff                                                      | Decision               |
| ----------------------------------------- | ------------------------------------------------------------- | ---------------------- |
| **Rule-based regex (Rust `regex` crate)** | Zero deps, deterministic, tunable by historians, ~200 LoC     | ✅ **Chosen**          |
| `rust-bert` (BERT NER)                    | Best precision, requires libtorch 2 GB — unshippable in Tauri | ❌ Rejected            |
| candle + ONNX mBERT                       | Better recall, no fine-tuned historical Spanish model exists  | ❌ Rejected for Fase 3 |

**Patterns** (compiled once via `once_cell::sync::Lazy`):

| Entity Type | Pattern Example                                                                           |
| ----------- | ----------------------------------------------------------------------------------------- |
| PERSON      | `(Don\|Doña\|Dr\|Fray\|Sor\|Fr\.)\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+(\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)*` |
| PLACE       | `(ciudad\|villa\|pueblo\|río\|sierra\|provincia\s+de)\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+`          |
| DATE        | `\b(\d{1,2}\s+de\s+[a-záéíóúñ]+\s+de\s+\d{4}\|\d{1,2}/\d{1,2}/\d{4})\b`                   |
| INSTITUTION | `(Real\|Cabildo\|Iglesia\|Convento\|Universidad\|Audiencia)(\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)*` |

**Cargo deps**: `regex = "1"`, `once_cell = "1"`

---

## Data Flow

```
[UI: "Analyze" button]
        │ invoke("index_fts" | "embed_item" | "extract_entities")
        ▼
[nlp::commands.rs] ──push(NlpJob)──▶ [NlpQueue mpsc channel]
                                              │
                                     tokio::spawn (worker loop)
                                              │
                              ┌───────────────┼───────────────┐
                              ▼               ▼               ▼
                        IndexFts(id)  ComputeEmbedding(id) ExtractEntities(id)
                              │               │               │
                         fts.rs         embeddings.rs       ner.rs
                              │               │               │
                         INSERT INTO    INSERT INTO       INSERT INTO
                          fts_items      vec_items          entities
                              │               │               │
                              └───────────────┴───────────────┘
                                              │
                                   app.emit("nlp:progress"|"nlp:complete"|"nlp:error")
                                              │
                                      [NlpStore.ts]
                                     updates reactive state
```

**Connection**: all worker operations use `AppDbState.worker_conn` (WAL mode, same pattern as OcrQueue).  
**FTS5 auto-index**: when OCR extraction is saved via `ExtractionRepo`, `NlpQueue.push(IndexFts(item_id))` is called from the OCR complete handler in `nlp/commands.rs` — no DB trigger required.

---

## File Changes

| File                                             | Action | Description                                                                                  |
| ------------------------------------------------ | ------ | -------------------------------------------------------------------------------------------- |
| `apps/desktop/src-tauri/src/nlp/mod.rs`          | Create | `NlpQueue`, `NlpJob` enum, `start_worker()`                                                  |
| `apps/desktop/src-tauri/src/nlp/embeddings.rs`   | Create | `EmbeddingEngine`, `load_model()`, `embed_text()`                                            |
| `apps/desktop/src-tauri/src/nlp/ner.rs`          | Create | `NerEngine`, `extract_entities()`, compiled `RegexSet`                                       |
| `apps/desktop/src-tauri/src/nlp/fts.rs`          | Create | `fts_index_item()`, `fts_search()` raw SQL helpers                                           |
| `apps/desktop/src-tauri/src/nlp/commands.rs`     | Create | Tauri commands: `index_fts`, `embed_item`, `extract_entities`, `fts_search`, `similar_items` |
| `apps/desktop/src-tauri/src/lib.rs`              | Modify | Add `mod nlp`, register `NlpQueue` state, register NLP commands, load sqlite-vec             |
| `apps/desktop/src-tauri/Cargo.toml`              | Modify | Add `fastembed = "5"`, `sqlite-vec = "0.1.10-alpha.3"`, `regex = "1"`, `once_cell = "1"`     |
| `packages/store/src/runner.ts`                   | Modify | Inline migrations `0004_fts5` and `0005_nlp_tables`                                          |
| `packages/store/src/schema.ts`                   | Modify | Add TypeScript type definitions for `entities`; note vec0/fts5 are raw SQL only              |
| `packages/store/src/repos/item.repo.ts`          | Modify | `searchByText()` → delegates to FTS5 MATCH via raw SQL `db_select`; LIKE fallback removed    |
| `packages/store/src/repos/entity.repo.ts`        | Create | `EntityRepo`: `create()`, `findByItem()`, `findByType()`, `deleteByItem()`                   |
| `packages/store/src/repos/embedding.repo.ts`     | Create | `EmbeddingRepo`: `upsert()`, `findSimilar()` (kNN via raw SQL `vec_search`)                  |
| `packages/store/src/repos/store.ts`              | Modify | Add `entities: EntityRepo`, `embeddings: EmbeddingRepo` to `StoreApi`                        |
| `packages/store/src/index.ts`                    | Modify | Export `EntityRepo`, `EmbeddingRepo`, new types                                              |
| `apps/desktop/src/lib/nlp.ts`                    | Create | `NlpStore` class (mirrors `OcrStore`), `NlpStatus` type, event wiring                        |
| `packages/ui/src/components/EntityViewer.svelte` | Create | Entity list grouped by type with colored pills                                               |
| `apps/desktop/src/routes/.../ItemView.svelte`    | Modify | Add "Analysis" section to right panel below Text Extraction                                  |

---

## Interfaces / Contracts

### NlpJob (Rust)

```rust
pub enum NlpJob {
    IndexFts { item_id: String },
    ComputeEmbedding { item_id: String },
    ExtractEntities { item_id: String },
}
```

### Tauri Events (Rust → TS)

```rust
// Payloads: same shape pattern as OcrProgressPayload
pub struct NlpProgressPayload { pub item_id: String, pub job: String, pub pct: u8 }
pub struct NlpCompletePayload  { pub item_id: String, pub job: String }
pub struct NlpErrorPayload     { pub item_id: String, pub job: String, pub error: String }
// Events: "nlp:progress", "nlp:complete", "nlp:error"
```

### NlpStore (TypeScript)

```typescript
export type NlpJobType = 'fts' | 'embed' | 'ner'
export type NlpStatus = 'idle' | 'pending' | 'running' | 'done' | 'error'

export interface ItemNlpState {
  fts: NlpStatus
  embed: NlpStatus
  ner: NlpStatus
}

export class NlpStore {
  getState(itemId: string): ItemNlpState
  async startListening(listen): Promise<void>
  stopListening(): void
}

export async function indexFts(itemId: string): Promise<void>
export async function embedItem(itemId: string): Promise<void>
export async function extractEntities(itemId: string): Promise<void>
export async function ftsSearch(query: string, collectionId?: string): Promise<FtsResult[]>
export async function similarItems(itemId: string, limit?: number): Promise<SimilarItem[]>
export function sanitizeFts5Query(raw: string): string
```

### EntityRepo (TypeScript)

```typescript
export interface Entity {
  id: string
  itemId: string
  entityType: 'person' | 'place' | 'date' | 'institution' | 'custom'
  value: string
  startOffset: number | null
  endOffset: number | null
  confidence: number | null
  createdAt: number
}
```

---

## Testing Strategy

| Layer                | What to Test                                                      | Approach                                                    |
| -------------------- | ----------------------------------------------------------------- | ----------------------------------------------------------- |
| Unit (Vitest)        | `sanitizeFts5Query()` — quotes, AND/OR/NOT, parens, special chars | Pure function, 10+ cases                                    |
| Unit (Vitest)        | `EntityRepo.create/findByItem/findByType`                         | `createMockDbClient` from `__mocks__/db.mock.ts`            |
| Unit (Vitest)        | `EmbeddingRepo.upsert/findSimilar`                                | Mock `db_select` returns raw vec_search rows                |
| Unit (Vitest)        | `NlpStore` event handling                                         | Inject mock `listen`; assert state transitions              |
| Unit (Vitest)        | `ItemRepo.searchByText` FTS5 path                                 | Mock returns FTS5 result rows; verify MATCH SQL emitted     |
| Unit (Rust)          | `NerEngine.extract_entities()`                                    | 3 historical text fixtures, assert ≥4 entity types detected |
| Unit (Rust)          | `fts_index_item` / `fts_search`                                   | In-memory rusqlite + FTS5 virtual table                     |
| Integration (Vitest) | `runMigrations` runs 0004+0005                                    | `createMockDbClient` verifies SQL statements executed       |

---

## Migration Plan

**Migration 0004** (`0004_fts5` inline in `runner.ts`):

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS fts_items USING fts5(
  item_id UNINDEXED, title, metadata, extracted_text,
  tokenize = 'unicode61 remove_diacritics 1', content = ''
);
-- Backfill from existing items + extractions
INSERT INTO fts_items(item_id, title, metadata, extracted_text)
SELECT i.id, i.title, COALESCE(i.metadata,''),
       COALESCE((SELECT GROUP_CONCAT(e.text_content,' ') FROM extractions e
                 JOIN assets a ON e.asset_id=a.id WHERE a.item_id=i.id), '')
FROM items i;
```

**Migration 0005** (`0005_nlp_tables` inline in `runner.ts`):

```sql
CREATE TABLE IF NOT EXISTS entities (
  id TEXT PRIMARY KEY,
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  entity_type TEXT NOT NULL CHECK(entity_type IN ('person','place','date','institution','custom')),
  value TEXT NOT NULL,
  start_offset INTEGER, end_offset INTEGER, confidence REAL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_entities_item ON entities(item_id);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);
-- vec0 virtual table (created after sqlite-vec extension load)
CREATE VIRTUAL TABLE IF NOT EXISTS vec_items USING vec0(
  item_id TEXT PRIMARY KEY, embedding FLOAT[384]
);
```

**Extension load order**: sqlite-vec must be loaded into `worker_conn` before migration 0005 runs. `lib.rs` setup calls `sqlite_vec::load(&worker_conn)` before `AppDbState::new()`.

---

## Open Questions

- [ ] Does `fastembed = "5"` (crates.io) resolve to `5.x` or is the semver `"5"` correct? Verify exact published version matches `fastembed-rs` on crates.io before applying.
- [ ] `sqlite-vec` extension load: `sqlite_vec::load()` takes `&Connection` — confirm this works with `Arc<Mutex<Connection>>` pattern in `AppDbState` (needs lock acquisition before load).
- [ ] FTS5 `content=''` (contentless) means snippets/highlights require storing text in fts_items itself — confirm this is acceptable for MVP vs. `content='items'` (content table mode requires triggers).
