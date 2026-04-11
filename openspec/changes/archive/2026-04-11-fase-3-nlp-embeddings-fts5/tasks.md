# Tasks: Fase 3 — NLP / Embeddings / FTS5

## Phase 1: Database Layer (no deps)

- [x] 1.1 Add `packages/store/src/migrations/0004_fts5.sql` — `CREATE VIRTUAL TABLE fts_items USING fts5(item_id UNINDEXED, title, metadata, extracted_text, tokenize='unicode61 remove_diacritics 1', content='')` + backfill INSERT from `items JOIN assets JOIN extractions`
- [x] 1.2 Add `packages/store/src/migrations/0005_embeddings.sql` — `CREATE TABLE entities` with all required columns + indexes + `CREATE VIRTUAL TABLE vec_items USING vec0(item_id TEXT PRIMARY KEY, embedding FLOAT[384])`
- [x] 1.3 Update `packages/store/src/runner.ts` — inline both migrations as string constants; ensure sqlite-vec extension is loaded before 0005 executes; update migration list
- [x] 1.4 Update `packages/store/src/schema.ts` — add Drizzle `entities` table definition (all columns matching migration); update `jobs.type` column enum to `'ocr' | 'embeddings' | 'ner'`
- [x] 1.5 Write Vitest tests for `runner.ts` migration 0004+0005 — use `createMockDbClient`; assert both SQL statements are executed; verify idempotency (run twice, no error); min 4 test cases

## Phase 2: Store Layer (depends on Phase 1)

- [x] 2.1 Create `packages/store/src/repos/entity.repo.ts` — `EntityRepo` class with `create(data)`, `findByItemId(itemId)`, `findByType(type)`, `deleteByItemId(itemId)`; constructor-inject `DrizzleClient`
- [x] 2.2 Create `packages/store/src/repos/entity.repo.test.ts` — test all four methods with `createMockDbClient`; cover create+retrieve, deleteByItemId returns empty, findByType; min 6 test cases
- [x] 2.3 Create `packages/store/src/repos/embedding.repo.ts` — `EmbeddingRepo` class with `upsert(itemId, vector)`, `findByItemId(itemId)`, `deleteByItemId(itemId)`, `knnSearch(vector, limit)`; raw SQL via `db_select` for vec0 operations
- [x] 2.4 Create `packages/store/src/repos/embedding.repo.test.ts` — mock `db_select` returning raw `vec_search` rows; test upsert/find/replace/knn; min 5 test cases
- [x] 2.5 Update `packages/store/src/repos/item.repo.ts` — rewrite `searchByText(term, collectionId?)` to use FTS5 MATCH via `fts_items` with `sanitizeFts5Query`; collectionId JOIN filter; remove LIKE fallback
- [x] 2.6 Update `packages/store/src/repos/item.repo.test.ts` — add FTS5 path tests: match emits MATCH SQL, scoped collection filter, empty result, sanitized input; min 4 new test cases
- [x] 2.7 Create `packages/store/src/repos/fts.repo.ts` — `FtsRepo` with `indexItem(itemId, title, metadata, extractedText)`, `search(query, collectionId?)`, `deleteByItemId(itemId)`; export `sanitizeFts5Query(raw): string` pure function
- [x] 2.8 Create `packages/store/src/repos/fts.repo.test.ts` — test `sanitizeFts5Query` with ≥10 cases (plain text, AND/OR/NOT, parens, special chars, empty string); test indexItem and search with mocked client; min 14 test cases
- [x] 2.9 Update `packages/store/src/repos/store.ts` — add `entities: EntityRepo`, `embeddings: EmbeddingRepo`, `fts: FtsRepo` to `StoreApi` interface and `createStore()` factory
- [x] 2.10 Update `packages/store/src/index.ts` — export `EntityRepo`, `EmbeddingRepo`, `FtsRepo`, `sanitizeFts5Query`, and all new types (`Entity`, `FtsResult`)

## Phase 3: Rust NLP Module (depends on Phase 1)

- [x] 3.1 Update `apps/desktop/src-tauri/Cargo.toml` — add `fastembed = "5"`, `sqlite-vec = "0.1.10-alpha.3"`, `regex = "1"`, `once_cell = "1"` to `[dependencies]`
- [x] 3.2 Create `apps/desktop/src-tauri/src/nlp/mod.rs` — define `NlpJob` enum (`IndexFts`, `ComputeEmbedding`, `ExtractEntities`), `NlpQueue` struct with `mpsc::Sender<NlpJob>`, `start_worker(conn, app_handle)` spawning tokio task
- [x] 3.3 Create `apps/desktop/src-tauri/src/nlp/ner.rs` — `NerEngine` with `once_cell::sync::Lazy` compiled `RegexSet`; patterns for PERSON, PLACE, DATE (numeric + written), INSTITUTION; `extract_entities(text) -> Vec<Entity>`; unit tests with 3 historical fixtures asserting ≥4 entity types
- [x] 3.4 Create `apps/desktop/src-tauri/src/nlp/embeddings.rs` — `EmbeddingEngine` with `load_model(model_path)` using fastembed `TextEmbedding`; `embed_text(text) -> Result<Vec<f32>>`; graceful fallback if model fails to load (log warning, return Ok(()))
- [x] 3.5 Create `apps/desktop/src-tauri/src/nlp/fts.rs` — `fts_index_item(conn, item_id, title, metadata, extracted_text)` raw SQL INSERT OR REPLACE into `fts_items`; `fts_search(conn, query, collection_id?) -> Vec<FtsRow>`; unit tests with in-memory rusqlite + FTS5
- [x] 3.6 Create `apps/desktop/src-tauri/src/nlp/commands.rs` — Tauri commands: `index_fts(item_id)`, `embed_item(item_id)`, `extract_entities(item_id)`, `fts_search(query, collection_id?)`, `similar_items(item_id, limit?)`; each pushes to `NlpQueue` and emits `nlp:progress`/`nlp:complete`/`nlp:error` events; event payloads match `NlpProgressPayload`, `NlpCompletePayload`, `NlpErrorPayload`
- [x] 3.7 Update `apps/desktop/src-tauri/src/lib.rs` — add `mod nlp`; extend `AppDbState` with `nlp_conn: Mutex<Connection>`; call `sqlite_vec::load(&nlp_conn)` before `AppDbState::new()`; register `NlpQueue` as Tauri state; register all 5 NLP commands

## Phase 4: Frontend Store + Components (depends on Phase 2 types)

- [x] 4.1 Create `apps/desktop/src/lib/nlp.ts` — `NlpStore` class with `$state` per-item `ItemNlpState` map; `startListening(listen)` wiring `nlp:progress`/`nlp:complete`/`nlp:error` events; `stopListening()`; export standalone invoke wrappers `indexFts`, `embedItem`, `extractEntities`, `ftsSearch`, `similarItems`
- [x] 4.2 Create `apps/desktop/src/lib/nlp.test.ts` — inject mock `listen`; test state transitions idle→running→done and idle→running→error for each of 3 job types; test `startListening`/`stopListening`; min 8 test cases
- [x] 4.3 Create `packages/ui/src/components/EntityViewer/EntityViewer.svelte` — accepts `entities: Entity[]` prop; groups by `entity_type`; renders colored pills per type; dispatches `highlight` event with `{ start_offset, end_offset }` on entity click; shows empty state when array is empty
- [x] 4.4 Create `packages/ui/src/components/EntityViewer/EntityViewer.test.ts` — test grouped rendering (2 PERSON + 1 PLACE + 1 DATE → 3 sections), click dispatches highlight event with correct offsets, empty array shows empty state message; min 5 test cases
- [x] 4.5 Update `apps/desktop/src/views/ItemView.svelte` — add collapsible "Analysis" panel below extraction panel (hidden when no assets); three action buttons (Full-Text Index / Generate Embeddings / Extract Entities) with status badges (idle/running/done/error); Similar Items section (empty state when no embedding, up to 5 linked cards); `EntityViewer` component wired to current item's entities; highlight event forwarded to text viewer

## Phase 5: Integration + Quality Gate

- [x] 5.1 Run `pnpm test` from workspace root — verify 0 test failures; total test count MUST exceed 144
- [x] 5.2 Run `pnpm typecheck` — verify 0 TypeScript errors across all packages
- [x] 5.3 Run `pnpm lint` — verify 0 lint errors
- [ ] 5.4 Verify `cargo test` passes for `apps/desktop/src-tauri` — NER unit tests (3 fixtures) + FTS in-memory tests all green (deferred — requires Rust toolchain)
- [ ] 5.5 Manual smoke test: open item with extraction → click "Full-Text Index" → badge transitions done; click "Extract Entities" → EntityViewer populates; search bar returns FTS5 ranked results (deferred — requires running app)
