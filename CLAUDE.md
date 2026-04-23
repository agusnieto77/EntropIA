# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is EntropIA

A desktop app for analyzing digitized historical sources using AI pipelines (OCR, NER, embeddings, semantic triples). Built offline-first with Tauri 2 + Svelte 5 + SQLite. Target users are historians working with fragmentary/degraded documents.

## Monorepo Structure

PNPM 9.15.4 workspaces + Turborepo. Three layers:

- **`apps/desktop/`** — Tauri 2 shell. Svelte 5 frontend (`src/`) + Rust backend (`src-tauri/`).
- **`packages/store/`** — Data layer: Drizzle ORM schema, SQLite repos (collection, item, asset, note, job, extraction, entity, embedding, fts, triple), migration runner.
- **`packages/ui/`** — Svelte 5 component library (Button, Card, DocumentViewer, EntityViewer, SearchBar, etc.) + design tokens CSS.
- **`packages/config-ts/`** — Shared tsconfig.

The Rust backend (`apps/desktop/src-tauri/`) contains these modules:
- **`db/`** — SQLite state management, Tauri IPC commands (`db_execute`, `db_select`, `db_select_rows`)
- **`ocr/`** — OCR engine with provider chain (PaddleOCR primary → Tesseract fallback), PDF text extraction, layout-aware OCR, async job queue
- **`nlp/`** — FTS5 indexing, embeddings (Python subprocess), hybrid NER (ONNX BERT + spaCy + rule-based), semantic triple extraction, async job queue. NER is a sub-module (`nlp/ner/`) with its own engine registry.
- **`layout/`** — DocLayout-YOLO document structure analysis (Python subprocess), reading order algorithm, stores results in `layouts` table
- **`transcription/`** — Audio transcription via Python faster-whisper subprocess, async job queue

In-progress modules on `feat/gemma4-llm-nlp-v1` branch (not yet merged):
- **`llm/`** — Local LLM via Gemma + llama.cpp (OCR correction, entity extraction, summarization, classification, Q&A)
- **`geo/`** — Nominatim geocoding for place entities (populates latitude/longitude/geoStatus on entities)

`openspec/` contains SDD (Specification-Driven Development) specs and change archives — not code.
`AGENTS.md` contains detailed build prerequisites (Windows toolchain, vcpkg Tesseract, LLVM/Clang) and engine architecture notes.

## Common Commands

```bash
pnpm install              # install all workspace deps
pnpm dev                  # turbo dev (all packages)
pnpm build                # turbo build (all packages)
pnpm lint                 # eslint across all packages
pnpm typecheck            # tsc + svelte-check across all packages
pnpm test                 # vitest run across all packages
pnpm test:run             # vitest run (explicit --run flag)

# Single package
pnpm --filter @entropia/store test
pnpm --filter @entropia/desktop lint

# Single test file
pnpm --filter @entropia/store test -- --run src/repos/item.repo.test.ts

# Single Rust test (from apps/desktop/src-tauri/)
cargo test nlp::tests::test_extract_entities

# Tauri desktop
cd apps/desktop && pnpm tauri dev     # run desktop app with hot reload
cd apps/desktop && pnpm tauri build   # production build

# Rust (from apps/desktop/src-tauri/)
cargo test                            # run Rust tests
cargo test -- --skip onnx              # skip ONNX tests if runtime not available
cargo clippy                          # lint Rust code
cargo fmt --check                     # check Rust formatting

# Rust quality report (Windows, PowerShell)
pnpm rust:quality:report
```

**First-time setup**: See `AGENTS.md` for Windows prerequisites (MSVC Build Tools, vcpkg Tesseract, LLVM/Clang, CMake). Before `pnpm tauri dev` or `pnpm tauri build`, OCR models must be downloaded — Tauri's `beforeDevCommand` and `beforeBuildCommand` both run `pnpm download-ocr-models` (PowerShell script) automatically. NER ONNX model tokenizer/vocab are bundled in `resources/models/ner/`; the ONNX model binary itself must be prepared via `scripts/prepare-ner-model.ps1`. Python scripts live in both `scripts/` (dev) and `resources/scripts/` (bundled with release).

## Testing

- **TypeScript/Svelte**: Vitest with happy-dom. Tests are co-located (`*.test.ts`).
- **Rust**: Standard `cargo test`. Modules have inline `#[cfg(test)]` tests.
- Tests mock the Tauri SQL plugin via `packages/store/src/__mocks__/db.mock.ts`.
- **Rust quality contract** (Windows): Pester `.ps1` test suites in `apps/desktop/src-tauri/scripts/` validate builds (`windows-feature-contract.ps1`, `rust-quality-contract.Tests.ps1`). ONNX Runtime is loaded dynamically (`load-dynamic` feature) — tests that need it will skip gracefully if the runtime is absent.

## Architecture Details

### Frontend Navigation (Not File-Based Routing)

The desktop app does **not** use SvelteKit or file-based routing. Navigation is a manual state machine in `src/lib/navigation.ts` with three views conditionally rendered in `App.svelte`:

- `collections` — list all collections
- `collection` — single collection (requires `id`)
- `item` — single item (requires `itemId` + `collectionId`)

Views live in `src/views/`, layout in `src/layout/` (AppShell, TopBar).

### Data Flow (Frontend to Rust)

1. Svelte views call repos from `@entropia/store` (e.g., `item.repo.ts`)
2. Repos use `client.ts` which wraps Tauri's `@tauri-apps/plugin-sql` for SQL operations, or calls `invoke()` for Rust commands
3. Rust Tauri commands (`db_execute`, `db_select`) operate on shared `AppDbState` (rusqlite)
4. AI commands (`extract_text`, `index_fts`, `embed_item`, `extract_entities`, `extract_triples`, `fts_search`, `similar_items`, `transcribe_audio`, `extract_layout`) go through async job queues (`OcrQueue`, `NlpQueue`, `TranscriptionQueue`, `LayoutQueue`)

### SQLite Connections

The Rust backend manages multiple SQLite connections to `entropia.sqlite`:
- **UI connection** — used by Tauri IPC commands (reads/writes from frontend)
- **OCR worker connection** — dedicated to OCR job queue
- **NLP worker connection** — dedicated to NLP queue (embeddings stored as BLOBs in `vec_items`)

All connections use WAL mode + foreign keys enabled. Each queue worker opens its own connection independently.

On startup, `lib.rs` runs: (1) legacy migration from old `com.entropia.app` directory (SQLite bundle comparison by "richness score" + asset path rewriting), (2) `extractions.method` CHECK constraint migration (removes legacy `CHECK(method IN ('native','ocr'))` to allow PaddleOCR methods), (3) `layouts` table creation.

### OCR Provider Chain

OCR uses a fallback chain defined in `ocr/mod.rs`:
- **PaddleOCR** (primary) — `ocr-rs` crate with MNN backend, feature-gated as `paddle-ocr`. PP-OCRv5 detection + latin recognition. PP-LCNet orientation model auto-corrects 0°/90°/180°/270° rotation. `OcrEngine` is `Send + Sync`.
- **Tesseract** (fallback) — `leptess` crate, languages `spa+eng`. `LepTess` is NOT `Send` → created per-call inside `spawn_blocking`.
- **Layout-aware OCR** — When DocLayout-YOLO is available, OCR runs layout detection first, then OCR per text-bearing region in reading order. Method field: `"paddle+layout"` or `"tesseract+layout"`.
- **PDF pipeline** — Native text extraction first (`pdf-extract`), quality-checked. Falls back to pdfium-render at 300 DPI + OCR per page.

Postprocessing heuristics in `postprocess.rs` are **DISABLED** (mixed columns). Kept for reference only.

### Layout Detection

DocLayout-YOLO (Python subprocess) detects 10 region categories (title, plain_text, figure, table, formula, etc.). Reading order uses union-find column grouping: regions with ≥50% horizontal overlap → same column, columns left-to-right, regions within columns top-to-bottom. Results stored in `layouts` table (Rust-side, not yet in Drizzle schema). Auto-wired into OCR pipeline when available. See `AGENTS.md` for full architecture details.

### Python Subprocess Architecture

Several features delegate to Python scripts (ORT/MSVC linker failures on Windows made native Rust unusable for some tasks):

- **`scripts/embed.py`** — fastembed with `paraphrase-multilingual-MiniLM-L12-v2` (384 dims, 50+ languages). Returns JSON wrapped in `===EMBED_JSON_BEGIN===` / `===EMBED_JSON_END===` sentinels.
- **`scripts/transcribe.py`** — faster-whisper with `base` model, `int8` compute, default language `es`. Same sentinel pattern.
- **`scripts/spacy_ner.py`** — spaCy NER backend (optional, used by hybrid NER engine when spaCy is available).
- **`scripts/layout_detect.py`** — DocLayout-YOLO layout detection. Same sentinel pattern (`===LAYOUT_JSON_BEGIN===` / `===LAYOUT_JSON_END===`).

Rust spawns Python via `which_python()` / `which_python_for_layout()` (searches conda envs first, falls back to system Python). All Python-backed features degrade non-fatally if Python or dependencies are unavailable.

**Python deps required**: `fastembed`, `faster-whisper`, `doclayout-yolo` (install via pip/conda). Optional: `spacy` + `es_core_news_sm` model for spaCy NER.

### Hybrid NER Architecture

NER uses a multi-engine approach (`nlp/ner/`):

- **ONNX** (`onnx.rs`) — BERT-based NER via `ort` (ONNX Runtime) + `tokenizers`. Model files bundled in `resources/models/ner/` (config, tokenizer, vocab). Requires ONNX Runtime dynamic library at runtime (`load-dynamic` feature).
- **spaCy** (`spacy.rs`) — Python subprocess calling `spacy_ner.py`. Optional fallback/complement.
- **Rule-based** (`rule_based.rs`) — Pattern matching for dates, locations, etc. Always available.
- **Hybrid** (`hybrid.rs`) — Orchestrates all three engines, merges results via `merge.rs`.

Engine selection is configured via `NerConfig` with `NerEngineKind` (Onnx, Spacy, Hybrid, RuleBased). The `NerRegistry` initializes available engines at startup and logs preflight status.

### Job Queue Pattern

All background systems (OCR, NLP, Transcription, Layout) follow the same pattern:
1. Frontend calls Tauri command → submits job to mpsc channel → returns "queued"
2. Worker thread drains jobs serially, emits `progress/complete/error` events
3. Frontend listens to events via reactive stores → updates UI
4. DB stores results in `extractions`/`transcriptions`/`layouts` table for persistence

## CI

GitHub Actions (`.github/workflows/ci.yml`) runs on push/PR to `main`:
- **lint-typecheck** — ESLint + svelte-check + tsc (Ubuntu, Node 20)
- **windows-rust-feature-contract** — validates Rust builds on Windows
- **rust-quality-report** — clippy, fmt, coverage via cargo-llvm-cov, Pester test suites (Windows, Node 22)
- **test** — `pnpm test` (depends on lint-typecheck + Rust jobs, Node 20)
- **build** — `pnpm build --filter=!@entropia/desktop` (TS/Svelte packages only; full Tauri build is release-only)

CI includes extensive **pnpm lockfile forensics** (SHA256 + git blob verification). Modifying `pnpm-lock.yaml` carelessly can cause CI failures — always use `pnpm install` to regenerate it.

## Package Exports

- `@entropia/store` — single entry `"."` → `src/index.ts` (exports all repos + `New*` mutation types)
- `@entropia/ui` — dual exports: `"."` (Svelte components) + `"./tokens"` (design tokens CSS)
- Internal dependencies use `workspace:*` protocol.

## Code Style

- **Prettier**: no semicolons, single quotes, trailing commas (es5), printWidth 100, tabWidth 2. Svelte files use `prettier-plugin-svelte`.
- **ESLint**: Flat config (ESLint 9+), TypeScript only. Unused vars prefixed with `_` are allowed. Svelte linting not yet enabled.
- **Turbo**: `typecheck` depends on `^build` (dependencies must build first). `dev` is non-cached and persistent.

## Conventions

- **Code**: English. **UI**: Spanish.
- Svelte 5 runes syntax (`$state`, `$derived`, `$effect` — not legacy Svelte 4 stores).
- Drizzle schema is the source of truth for the data model (`packages/store/src/schema.ts`).
- Migrations live in `packages/store/src/migrations/` (committed to repo, applied by `runner.ts`).
- All IDs are text (UUIDs generated client-side).
- Timestamps are integer (Unix epoch).
- Tauri dev server is hardcoded to port 1420 (`strictPort: true`).
- Rust release profile uses LTO + `opt-level = "s"` + strip for small binaries.
