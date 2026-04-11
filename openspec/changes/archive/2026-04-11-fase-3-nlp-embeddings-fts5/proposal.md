# Proposal: Fase 3 — NLP / Embeddings / FTS5

## Intent

Historians using EntropIA currently search sources only by LIKE pattern on title and metadata — slow, brittle, and unable to reach extracted text. Fase 3 gives them:

1. **Full-text search** over extracted OCR text (not just titles)
2. **Semantic similarity** — find documents by meaning, not just keywords
3. **Named-entity recognition** — auto-surface persons, places, dates, and institutions from historical Spanish texts

These three capabilities transform EntropIA from a filing cabinet into an analytical workbench.

## Scope

### In Scope

- FTS5 virtual table `fts_items` (title + metadata + extracted_text); migration 0004
- `sanitizeFts5Query()` utility to prevent MATCH operator injection
- SQLite-vec `vec_items` table + fastembed `all-MiniLM-L6-v2` embeddings; migration 0005
- Rule-based NER (Rust regex) for Person, Place, Date, Institution in historical Spanish; `entities` table
- `NlpQueue` background worker in Rust (mirrors OcrQueue pattern)
- Tauri events: `nlp:progress`, `nlp:complete`, `nlp:error`
- `EntityRepo` + `EmbeddingRepo` in `packages/store`
- `NlpStore` (TypeScript class) + `EntityViewer.svelte` component
- Analysis panel in `ItemView.svelte` right panel

### Out of Scope

- Knowledge triples / relationship graphs → **deferred to Fase 4**
- ML-based NER (rust-bert, candle) → blocked by libtorch size / no historical Spanish model
- Multilingual embeddings model → use `all-MiniLM-L6-v2` (25 MB) for MVP; upgrade path documented
- Scanned PDF OCR fallback → **separate change `fase-2.5-scanned-pdf`**

## Capabilities

> Contract between proposal and sdd-spec.

### New Capabilities

- `fts5-search`: Full-text search via FTS5 virtual table replacing LIKE-based search
- `embeddings`: Offline vector embeddings generation and similarity search
- `ner-entities`: Rule-based named-entity recognition and entity display

### Modified Capabilities

- `search`: Requirement "Search Execution" changes from SQL LIKE to FTS5 MATCH
- `data-store`: New schema tables (entities, vec_items, fts_items) and migrations 0004–0005; `ItemRepo.searchByText` delegates to FTS5

## Approach

| Layer      | Decision                                                      | Rationale                                                                                                              |
| ---------- | ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| FTS5       | SQLite built-in virtual table, inline SQL in migration runner | No extra deps; Drizzle sqlite-proxy passes raw SQL through                                                             |
| Embeddings | fastembed 5.13.2 + sqlite-vec 0.1.10-alpha.3                  | Pure Rust + ONNX, no Python, no libtorch; 25 MB model bundles cleanly in Tauri                                         |
| NER        | Rust regex rule engine                                        | rust-bert requires 2 GB libtorch — impossible to ship in Tauri; no fine-tuned ONNX model for historical Spanish exists |
| Worker     | NlpQueue (mpsc + tokio::spawn)                                | Mirrors proven OcrQueue pattern; non-blocking, backpressure-safe                                                       |
| Frontend   | NlpStore (class) + EntityViewer.svelte                        | Mirrors OcrStore pattern; zero new architectural concepts                                                              |

## Affected Areas

| Area                                             | Impact   | Description                                                  |
| ------------------------------------------------ | -------- | ------------------------------------------------------------ |
| `src-tauri/src/nlp/`                             | New      | `mod.rs`, `ner.rs`, `embeddings.rs`, `commands.rs`, NlpQueue |
| `src-tauri/src/db.rs`                            | Modified | Load sqlite-vec extension; add `nlp_conn` to AppDbState      |
| `src-tauri/Cargo.toml`                           | Modified | Add `fastembed`, `sqlite-vec`, `regex`, `once_cell`          |
| `packages/store/src/schema.ts`                   | Modified | Add `entities`, `vec_items`, `fts_items` tables              |
| `packages/store/src/migrations/`                 | New      | `0004_fts5.sql`, `0005_embeddings.sql`                       |
| `packages/store/src/repos/entity.repo.ts`        | New      | EntityRepo CRUD                                              |
| `packages/store/src/repos/embedding.repo.ts`     | New      | EmbeddingRepo + kNN search                                   |
| `apps/desktop/src/lib/nlp.ts`                    | New      | NlpStore (Svelte 5 runes)                                    |
| `packages/ui/src/components/EntityViewer.svelte` | New      | Entity display component                                     |
| `apps/desktop/src/routes/.../ItemView.svelte`    | Modified | Analysis panel in right column                               |

## Risks

| Risk                                               | Likelihood | Mitigation                                                                                   |
| -------------------------------------------------- | ---------- | -------------------------------------------------------------------------------------------- |
| sqlite-vec alpha API breaks between patch versions | Med        | Pin exact version `0.1.10-alpha.3`; wrap extension load in fallback that disables embeddings |
| FTS5 MATCH injection via user query                | High       | `sanitizeFts5Query()` strips/escapes all special chars before every MATCH call               |
| fastembed model download on first run (25 MB)      | Low        | Bundle model in `resources/`; disable network fetch in production build                      |
| NER precision low on archaic spelling variants     | Med        | Accept MVP quality; expose confidence score; allow user corrections in Fase 4                |

## Rollback Plan

1. Drop migrations 0004 and 0005 (virtual tables — `DROP TABLE IF EXISTS fts_items; DROP TABLE IF EXISTS vec_items; DROP TABLE IF EXISTS entities`)
2. Revert `ItemRepo.searchByText` to LIKE pattern
3. Remove `src-tauri/src/nlp/` module and Cargo.toml entries
4. No user data is lost — virtual tables hold no primary content

## Dependencies

- Fase 2 (OCR + extractions) must be complete — `extractions` table must exist before migration 0004 can join it
- sqlite-vec requires loading a native `.dll`/`.so` via `rusqlite::Connection::load_extension`; Tauri MUST have `allowlist.shell.open` disabled and extension loading sandboxed

## ADRs to Create

1. **ADR-FTS5**: SQLite FTS5 over external search engine (Meilisearch, Tantivy)
2. **ADR-Embeddings**: fastembed + all-MiniLM-L6-v2 vs multilingual model vs deferred
3. **ADR-NER**: Rule-based regex vs rust-bert vs candle-ONNX for historical Spanish

## Success Criteria

- [ ] FTS5 `fts_items` table created by migration 0004; `ItemRepo.searchByText` uses MATCH, not LIKE
- [ ] `sanitizeFts5Query()` has unit tests covering quotes, `AND/OR/NOT`, parentheses, special chars
- [ ] Embedding generated for every item with extracted text; kNN query returns top-5 similar items
- [ ] NER detects ≥ 4 entity types (Person, Place, Date, Institution) on test corpus of 3 historical documents
- [ ] `EntityViewer.svelte` renders entity list grouped by type
- [ ] `NlpQueue` processes items non-blocking; UI remains responsive during analysis
- [ ] All new repos (`EntityRepo`, `EmbeddingRepo`) have Vitest unit tests with mock DrizzleClient
- [ ] No existing Fase 1/2 tests regress
