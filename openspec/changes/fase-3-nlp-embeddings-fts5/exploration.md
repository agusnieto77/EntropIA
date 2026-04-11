# Exploration: Fase 3 — NLP / Embeddings / FTS5

**Date**: 2026-04-11  
**Agent**: sdd-explore  
**Status**: Complete

---

## Current State

### Search (as-is)

- `packages/store/src/repos/item.repo.ts` → `searchByText()` uses Drizzle `like()` operator on `items.title` and `items.metadata` (LIKE pattern matching)
- Migration `0002_metadata_search.sql` adds a STORED generated column `search_text = title || ' ' || metadata` with a plain B-tree index
- `openspec/specs/search/spec.md` explicitly documents LIKE as the search mechanism
- **Limitation**: no ranking, no fuzzy match, no full-text tokenization, no search across `extractions.text_content`

### OCR Pipeline (as-is)

- `apps/desktop/src-tauri/src/ocr/` — fire-and-forget Rust queue → mpsc channel → serial worker
- Worker emits `ocr:progress`, `ocr:complete`, `ocr:error` Tauri events
- Text stored in `extractions` table: `{ id, asset_id, text_content, method, confidence }`
- OCR worker `process_pdf()` has a documented `TODO` in mod.rs: "PDF page rendering for OCR fallback (Fase 2.5)" — currently errors on scanned PDFs
- Frontend `ocr.ts` (OcrStore class) mirrors the queue pattern exactly — good template for NLP client

### Schema (as-is)

- 6 tables: `collections`, `items`, `assets`, `notes`, `jobs`, `extractions`
- `jobs.type` already typed as `'ocr' | 'ner' | 'embeddings' | 'triples'` — schema anticipates NLP
- `extractions` stores raw text; NLP results would need new table(s)

### Architecture Patterns

- DB client: `drizzle/sqlite-proxy` over Tauri IPC (`db_execute` / `db_select` commands)
- Migrations: inline SQL strings in `runner.ts` (not filesystem reads — required for Tauri webview)
- Every new Rust capability = new `mod nlp/` + `mod nlp/commands.rs` + register in `lib.rs`
- Async jobs: `OcrQueue` (mpsc channel + `tokio::spawn`) — direct pattern for `NlpQueue`
- Frontend: `ocr.ts` pattern (plain TS class + Tauri events) ready to replicate as `nlp.ts`
- StoreApi extension: add new repo to `store.ts`, add new table to `schema.ts`, add migration

---

## Affected Areas

| File                                       | Reason                                                       |
| ------------------------------------------ | ------------------------------------------------------------ |
| `packages/store/src/schema.ts`             | New tables: `fts_items`, `entities`, `embeddings`, `triples` |
| `packages/store/src/migrations/0004_*.sql` | FTS5 virtual table, entity/embedding/triple tables           |
| `packages/store/src/runner.ts`             | Inline new migration SQL                                     |
| `packages/store/src/repos/item.repo.ts`    | Replace `like()` with FTS5 MATCH query                       |
| `packages/store/src/repos/store.ts`        | Add `EntityRepo`, `EmbeddingRepo`, `TripleRepo`              |
| `apps/desktop/src-tauri/src/lib.rs`        | Add `mod nlp`, register NLP commands, add NlpQueue state     |
| `apps/desktop/src-tauri/src/ocr/mod.rs`    | Fase 2.5 fix: scanned PDF fallback                           |
| `apps/desktop/src-tauri/Cargo.toml`        | Add `fastembed`, `sqlite-vec`, regex crates                  |
| `apps/desktop/src/lib/nlp.ts`              | New: NlpStore class mirroring OcrStore                       |
| `apps/desktop/src/views/ItemView.svelte`   | New "Analysis" section in right panel                        |

---

## Decision 1: FTS5 Upgrade Strategy

### Current: LIKE on `search_text` generated column

### Option A — FTS5 Virtual Table (recommended)

Create a separate `fts_items` FTS5 virtual table that includes `title`, `metadata`, and most importantly **`extractions.text_content`** (synced via triggers or explicit insert).

```sql
-- Migration 0004_fts5
CREATE VIRTUAL TABLE IF NOT EXISTS fts_items USING fts5(
  item_id UNINDEXED,
  title,
  metadata,
  extracted_text,
  content=''  -- external content table
);
```

Query via raw SQL in a new `items.fts5Search()` repo method (Drizzle sqlite-proxy passes raw SQL through `db_select`):

```typescript
async fts5Search(query: string, collectionId?: string): Promise<Item[]> {
  const ftsQuery = query.split(/\s+/).map(t => `"${t}"`).join(' ')
  // raw SQL: SELECT item_id, rank FROM fts_items WHERE fts_items MATCH ? ORDER BY rank
}
```

- **Pros**: native SQLite, zero extra deps, offline, ranked results, prefix queries, snippet extraction, integrates with `extractions` text
- **Cons**: FTS5 virtual table is NOT a real Drizzle schema table → needs raw SQL for FTS5-specific queries; sync between `items`/`extractions` and FTS5 must be managed in migrations or application code
- **Effort**: Medium — 1 migration + 2 new repo methods + update search spec

### Option B — Keep LIKE + add column for `text_content`

Extend `search_text` to join `extractions.text_content`.

- **Pros**: zero extra complexity
- **Cons**: LIKE is O(n), no ranking, no tokenization, no prefix match, can't tokenize historical Spanish properly
- **Effort**: Low
- **Verdict**: not viable for Fase 3 ambitions

### FTS5 Migration Strategy

Since `runner.ts` inlines SQL and executes via Tauri IPC:

1. Add `0004_fts5.sql` inline string in `runner.ts`
2. Create the virtual table + populate from existing `items` + `extractions`
3. Add `INSERT`/`UPDATE`/`DELETE` trigger hooks (or manage explicitly in repo methods)
4. Keep old `search_text` column + index for backward compatibility during transition
5. Update `item.repo.ts`: `searchByText()` → delegates to FTS5 when available, LIKE fallback

**Key constraint**: Drizzle sqlite-proxy does NOT prevent raw SQL — `db_select` accepts any SQL string. FTS5 MATCH queries can pass through unchanged.

---

## Decision 2: Embeddings — Storage Backend

### Option A — `sqlite-vec` (recommended for Fase 3)

Rust crate `sqlite-vec = "0.1.10-alpha.3"` loads the sqlite-vec C extension at runtime into the existing rusqlite connection. Stores embedding vectors as BLOBs in a virtual vtable.

```rust
// In lib.rs setup:
sqlite_vec::load(&conn)?; // loads the vec0 extension
```

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS vec_items USING vec0(
  item_id TEXT PRIMARY KEY,
  embedding FLOAT[384]  -- all-MiniLM-L6-v2 = 384 dims
);
```

- **Pros**: same SQLite file, offline-first, ANN search (`knn_search`), no external process, Tauri-compatible
- **Cons**: alpha crate (0.1.10-alpha.3), requires loading extension into rusqlite, 384-dim f32 vectors = ~1.5KB/item (manageable)
- **Effort**: Medium — Cargo.toml + extension load + new Rust module + virtual table migration

### Option B — `vectra` (file-based JSON)

npm package, stores vectors as JSON on disk. No SQLite involvement.

- **Pros**: very simple TypeScript-only, no Rust changes
- **Cons**: completely separate from SQLite, no join capability, file sync issues, not transactional
- **Effort**: Low but architecturally wrong for this stack

### Option C — `chromadb` (external HTTP server)

Requires running a Python process alongside Tauri.

- **Pros**: full-featured, industry standard
- **Cons**: breaks offline-first, requires Python runtime, installation complexity, no Windows bundle story
- **Effort**: Very High — not viable

**Recommendation**: sqlite-vec. Despite alpha status, the pattern of loading C extensions into rusqlite is well-established (the bundled SQLite in rusqlite already supports extensions). Risk mitigated by pinning version.

---

## Decision 3: Embedding Model

### Option A — `fastembed = "5.13.2"` (recommended)

Pure Rust, ONNX Runtime under the hood. Ships `all-MiniLM-L6-v2` (384-dim, ~25MB model) or `bge-small-en-v1.5` (384-dim, ~35MB). No Python. No PyTorch.

```rust
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))?;
let embeddings = model.embed(vec!["texto histórico..."], None)?;
```

- **Pros**: pure Rust, offline, proven in production, multilingual models available (paraphrase-multilingual-MiniLM-L12-v2 for Spanish), bundles ONNX RT
- **Cons**: ONNX Runtime binary download at first run (configurable), ~30-80MB model files to bundle as Tauri resources
- **Effort**: Medium — add to Cargo.toml, new `nlp/embeddings.rs` module, background worker

### Option B — `candle-core = "0.10.2"` (Hugging Face Candle)

Pure Rust ML framework. Can run BERT-style models. More low-level than fastembed.

- **Pros**: no ONNX dependency, Hugging Face native
- **Cons**: more boilerplate than fastembed (manual tokenization, model loading), less batteries-included
- **Effort**: High

### Option C — External API (OpenAI, Cohere)

HTTP call to embedding endpoint.

- **Pros**: best quality, zero model bundling
- **Cons**: requires internet, API key management, not offline-first
- **Effort**: Low code but HIGH user friction

**Recommendation**: fastembed with `paraphrase-multilingual-MiniLM-L12-v2` for Spanish historical texts. Bundle model in Tauri resources.

---

## Decision 4: NER (Named Entity Recognition)

### Option A — Rule-based / Regex (recommended for Fase 3 MVP)

Custom Spanish NER patterns: regex rules for dates (dd/mm/yyyy, roman numerals), person titles ("Don", "Doña", "fray"), places (uppercase noun phrases), institutions ("Cabildo", "Real Audiencia").

```rust
// nlp/ner.rs
struct NerRule { pattern: Regex, entity_type: EntityType }
enum EntityType { Person, Place, Date, Institution, Custom }
```

- **Pros**: zero deps beyond `regex` crate, deterministic, offline, fast, domain-tunable (historian can add rules), no model download
- **Cons**: recall limited by rule coverage, no generalization beyond defined patterns
- **Effort**: Low-Medium — ~200 lines Rust

### Option B — `rust-bert = "0.23.0"` (full BERT NER)

Uses PyTorch via `tch` bindings (libtorch ~2GB download).

- **Pros**: state-of-the-art Spanish NER (bert-base-multilingual)
- **Cons**: requires libtorch (2GB), NOT offline-bundleable in Tauri, massive binary size, no Spanish historical fine-tune
- **Effort**: Very High — essentially impossible to bundle in Tauri desktop app
- **Verdict**: ruled out

### Option C — `candle` + BERT ONNX model

Load a quantized mBERT NER model (~100MB) via candle or ONNX Runtime (fastembed infrastructure).

- **Pros**: better recall than rules, offline, feasible bundle size
- **Cons**: no fine-tuned historical Spanish NER model publicly available; generic mBERT NER struggles with colonial-era spelling
- **Effort**: High (model search + integration + testing)

**Recommendation**: Rule-based for Fase 3. Extensible design (NerRule registry) so historians can add domain-specific patterns. Plan candle/ONNX NER as Fase 4 upgrade when a fine-tuned model emerges.

---

## Decision 5: Knowledge Triples (Subject-Predicate-Object)

### Option A — Rule-based dependency patterns (recommended: defer)

Build on NER output: extract triples by finding (PERSON verb PERSON/PLACE/INSTITUTION) patterns.

- **Pros**: no deps, reuses NER output
- **Cons**: requires POS tagging (not trivially available in pure Rust), low precision on historical Spanish syntax
- **Effort**: High for decent quality

### Option B — LLM prompt-based extraction (defer to Fase 4+)

Send extraction text + NER entities to local LLM (Ollama) or API with structured output.

- **Pros**: high quality, flexible
- **Cons**: requires Ollama or internet, optional feature
- **Effort**: Medium (depends on Ollama integration Fase)

**Recommendation**: **Defer knowledge triples to Fase 4**. Fase 3 focuses on NER + embeddings + FTS5. Triple extraction without LLM or POS tagger produces poor results on historical texts.

---

## Decision 6: Fase 2.5 — Scanned PDF Fallback

The `ocr/mod.rs` `process_pdf()` function has an explicit TODO:

```rust
// TODO: Implement PDF page rendering for OCR fallback (Fase 2.5)
Err("PDF native text extraction failed quality check and PDF-to-image rendering is not yet implemented")
```

### Options

- **A**: Tackle as sub-task 0 of Fase 3 (before NLP work begins)
- **B**: Separate micro-change `fase-2.5-scanned-pdf`
- **C**: Defer entirely to Fase 4

The `pdfium-render` crate requires the Pdfium C binary bundled as Tauri resource (~10MB). It's non-trivial but well-documented.

**Recommendation**: **Option B** — create a minimal `fase-2.5-scanned-pdf` change. It's orthogonal to NLP, has a clear scope, and unblocks real-world usage (most historical archives are scanned). Estimate: 2-3 tasks, independent of Fase 3 NLP.

---

## Decision 7: UI Integration (NLP Panel in ItemView)

Current `ItemView.svelte` layout: `grid-template-columns: 1fr 380px` with:

- Left: DocumentViewer + asset thumbnails
- Right panel (380px): Metadata → Add Note → Notes list → Text Extraction

### Proposed right panel additions (after Text Extraction)

```
[Text Extraction] ← existing section
[Analysis]        ← new section
  ├── [Run Analysis] button (triggers NER + embeddings job)
  ├── Status badge (idle/running/done/error)
  ├── Entities tab: Person | Place | Date | Institution pills
  └── Semantic Search hint: "Similar items: ..."
```

Pattern: same `NlpStore` class (mirrors `OcrStore`) with events `nlp:progress`, `nlp:complete`, `nlp:error`.

**Key UX decision**: trigger analysis manually per item (user clicks "Analyze") vs. auto-trigger after OCR completes. Recommend manual trigger for Fase 3 (gives user control, avoids heavy computation on import).

New UI component needed: `EntityViewer.svelte` in `packages/ui/src/components/`.

---

## Recommended Scope for Fase 3

### Include in Fase 3

1. **FTS5 upgrade** — migration 0004, virtual table, update `item.repo.ts` search methods, include `extractions.text_content` in index
2. **Embeddings** — fastembed + sqlite-vec, new `nlp/embeddings.rs` Rust module, `NlpQueue` worker, `embeddings` virtual table, background job
3. **NER (rule-based)** — `nlp/ner.rs` with Spanish historical entity rules (Person, Place, Date, Institution), results stored in new `entities` table
4. **Frontend NLP client** — `nlp.ts` (NlpStore pattern), `EntityViewer.svelte` component, Analysis section in `ItemView.svelte`
5. **New store repos** — `EntityRepo`, `EmbeddingRepo` in `packages/store`
6. **Migrations** — 0004_fts5, 0005_nlp_tables (entities, embeddings virtual table)
7. **TDD** — Vitest tests for repos + nlp.ts, Rust unit tests for ner.rs rules

### Defer to Fase 3.5 / 4

- Knowledge triples (needs POS tagger or LLM)
- `rust-bert` / candle NER (no viable Spanish historical fine-tune exists yet)
- External LLM integration
- Parallel processing of multiple items
- `fase-2.5-scanned-pdf` → **separate change** (independent scope)

---

## Risks

### Risk 1 — sqlite-vec alpha stability

`sqlite-vec` is `0.1.10-alpha.3`. The C extension loading API may change. Mitigation: pin exact version, write integration test that verifies `vec0` table creation. Fallback: store embeddings as BLOB in a regular table and do cosine similarity in Rust (no SQL-level ANN).

### Risk 2 — fastembed model bundling size

`paraphrase-multilingual-MiniLM-L12-v2` is ~470MB. `all-MiniLM-L6-v2` is ~25MB (English-only). Historical Spanish texts need the multilingual model. Mitigation: bundle `all-MiniLM-L6-v2` for MVP (v1 quality is acceptable for Latin-alphabet Spanish), provide model upgrade path. Target: keep Tauri resources under 100MB total (ocrs models ~20MB already present).

### Risk 3 — FTS5 + Drizzle sqlite-proxy interaction

Drizzle sqlite-proxy generates parameterized SQL. FTS5 MATCH syntax (`fts_items MATCH ?`) with phrase operators requires carefully escaped queries. FTS5 MATCH does NOT use standard SQL LIKE syntax — special chars like `-`, `OR`, `AND`, `"` must be sanitized. Mitigation: write `sanitizeFts5Query()` utility (lessons from Fase 2 bugfix on FTS5 syntax).

---

## New Tables Required

```sql
-- 0004_fts5.sql
CREATE VIRTUAL TABLE IF NOT EXISTS fts_items USING fts5(
  item_id UNINDEXED,
  title,
  metadata,
  extracted_text,
  content=''
);

-- 0005_nlp_tables.sql
CREATE TABLE IF NOT EXISTS entities (
  id TEXT PRIMARY KEY,
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  entity_type TEXT NOT NULL CHECK(entity_type IN ('person','place','date','institution','custom')),
  text TEXT NOT NULL,
  start_char INTEGER,
  end_char INTEGER,
  confidence REAL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_entities_item ON entities(item_id);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);

-- sqlite-vec virtual table (loaded after extension)
CREATE VIRTUAL TABLE IF NOT EXISTS vec_items USING vec0(
  item_id TEXT PRIMARY KEY,
  embedding FLOAT[384]
);
```

---

## Ready for Proposal

Yes. The exploration reveals a coherent, offline-first NLP strategy:

- FTS5 for text search (high value, low risk)
- fastembed + sqlite-vec for embeddings (medium risk, manageable)
- Rule-based NER for Fase 3 MVP (low risk, tunable)
- Knowledge triples deferred (high effort, unclear value without LLM)
- Scanned PDF as separate `fase-2.5` change

The orchestrator should present this scope to the user before proceeding to proposal.
