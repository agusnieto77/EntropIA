# Archive Report: fase-3-nlp-embeddings-fts5

**Archived**: 2026-04-11
**Status**: PASS WITH WARNINGS
**Tests**: 236/236 (Store: 110, UI: 57, Desktop: 69)

## Delivered

- FTS5 virtual table `fts_items` (migration 0004) — unicode61 tokenizer
- `FtsRepo` TypeScript (indexItem, search, sanitizeFts5Query, removeItem)
- `searchByText` upgraded to use FTS5 first, LIKE fallback
- Rule-based NER in Rust `nlp/ner.rs` (PERSON, PLACE, DATE, INSTITUTION) — 13 #[test]
- `EntityRepo` TypeScript (findByItemId, create, deleteByItemId) — 8 tests
- `entities` table + migration 0005
- fastembed + `EmbeddingRepo` (store, get, knnSearch) + `embeddings_fallback` table
- `NlpQueue` (Rust, mirrors OcrQueue) + 5 Tauri commands
- `NlpStore` TypeScript class — 28 tests
- `EntityViewer.svelte` component — 13 tests
- Analysis panel in `ItemView.svelte` (Index/Embed/Extract buttons)
- OcrStore.onComplete callback → auto-FTS indexing after OCR

## ADRs

- ADR-014: FTS5 virtual table (vs Tantivy/Meilisearch)
- ADR-015: fastembed all-MiniLM-L6-v2 + sqlite-vec alpha
- ADR-016: Rule-based NER (vs rust-bert/candle)

## Specs Synced

| Domain       | Action  | File                                                                                                                                                                                                                                |
| ------------ | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| search       | Updated | `openspec/specs/search/spec.md` — Search Execution modified to FTS5; FTS5 Virtual Table, FTS5 Sync, FTS5 Query Sanitization, FTS5 Search Execution added                                                                            |
| data-store   | Updated | `openspec/specs/data-store/spec.md` — Dual-Connection upgraded to Triple-Connection; Base Schema and Item Repository updated; Migration 0004, 0005, Jobs Type Enum, EntityRepo, EmbeddingRepo, Triple-Connection requirements added |
| embeddings   | Created | `openspec/specs/embeddings/spec.md` — new full spec                                                                                                                                                                                 |
| ner-entities | Created | `openspec/specs/ner-entities/spec.md` — new full spec                                                                                                                                                                               |
| nlp-ux       | Created | `openspec/specs/nlp-ux/spec.md` — new full spec                                                                                                                                                                                     |

## Archive Contents

- `proposal.md` ✅
- `exploration.md` ✅
- `specs/` ✅ (5 delta spec files)
- `design.md` ✅
- `tasks.md` ✅ (38/38 tasks complete, 2 deferred: cargo test + smoke test)
- `verify-report.md` ✅

## Deferred

- Knowledge triples → Fase 4
- `cargo test` (Rust unit tests) → requires Rust toolchain
- Manual smoke test → requires running app
- sqlite-vec vec_items integration → runtime-only (sqlite-vec alpha)
- scanned PDF fallback → `fase-2.5-scanned-pdf` (separate change)

## SDD Cycle Complete

The change has been fully planned, implemented, verified, and archived.
**Test count at archive**: 236 (Store: 110, UI: 57, Desktop: 69)
Ready for the next change (Fase 4 — knowledge graph / triples, or Fase 2.5 — scanned PDF).
