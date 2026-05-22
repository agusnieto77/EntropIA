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
- **`ocr/`** — OCR engine with PaddleOCR light + PaddleOCR-VL high mode, PDF text extraction, layout-aware OCR, async job queue
- **`nlp/`** — FTS5 indexing, embeddings (Python subprocess), hybrid NER (ONNX BERT + spaCy + rule-based), semantic triple extraction, async job queue. NER is a sub-module (`nlp/ner/`) with its own engine registry.
- **`transcription/`** — Audio transcription via Python faster-whisper subprocess, async job queue
- **`app_logs/`** — Structured log viewer. Tauri commands: `logs_get`, `logs_clear`, `logs_open_dir`. Frontend: `LogsTab.svelte` in settings.

- **`llm/`** — LLM pipeline with dual backend: local Gemma via llama.cpp sidecar OR OpenRouter API. Jobs: OCR correction, entity extraction, triple extraction, summarization, classification, Q&A. Results persisted in `llm_results` table. Asset-level variants avoid context-window overflow on multi-page documents.
- **`geo/`** — Nominatim geocoding for place entities (populates latitude/longitude/geoStatus on entities)
- **`settings/`** — Key-value settings store (`app_settings` table). Tauri commands: `settings_get`, `settings_set`, `settings_get_all`, `settings_delete`. Keys: `OPENROUTER_API_KEY`, `OPENROUTER_MODEL`, `LLM_MODE`, `ASSEMBLYAI_API_KEY`, `STT_MODE`, `GLM_OCR_API_KEY`, `OCRH_MODE`, `LANGUAGE`, `DEPS_VENV_PYTHON_PATH`.
- **`image_edit/`** — Image manipulation commands (rotation, cropping)
- **`deps/`** — Python dependency manager using **uv** (venv creation + pip). Checks/installs: Python, fastembed, paddleocr, faster-whisper, spacy. Emits `deps://progress|complete|error` Tauri events. Frontend: `DependenciasTab.svelte` in settings. Critical deps block AI features if missing.

`openspec/` contains SDD (Specification-Driven Development) specs and change archives — not code.
`AGENTS.md` contains detailed build prerequisites (Windows toolchain, LLVM/Clang, Python OCR/NLP packages) and engine architecture notes.

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

# Vitest browser UI
pnpm test:ui
```

**First-time setup**: See `AGENTS.md` for Windows prerequisites (MSVC Build Tools, LLVM/Clang, CMake, Python OCR/NLP packages). Before `pnpm tauri dev` or `pnpm tauri build`, OCR models must be downloaded — Tauri's `beforeDevCommand` and `beforeBuildCommand` both run `pnpm download-ocr-models` (PowerShell script) automatically. NER ONNX model tokenizer/vocab are bundled in `resources/models/ner/`; the ONNX model binary itself must be prepared via `scripts/prepare-ner-model.ps1`. Python scripts live in both `scripts/` (dev) and `resources/scripts/` (bundled with release).

## Testing

- **TypeScript/Svelte**: Vitest with happy-dom. Tests are co-located (`*.test.ts`). Desktop app uses `$lib` alias resolved in `vitest.config.ts` and a `test-setup.ts` setup file.
- **`@entropia/store`**: Tests run in `environment: 'node'` (not happy-dom). Mock the Tauri SQL plugin via `packages/store/src/__mocks__/db.mock.ts`.
- **Rust**: Standard `cargo test`. Modules have inline `#[cfg(test)]` tests.
- **Rust quality contract** (Windows): Pester `.ps1` test suites in `apps/desktop/src-tauri/scripts/` validate builds (`windows-feature-contract.ps1`, `rust-quality-contract.Tests.ps1`). ONNX Runtime is loaded dynamically (`load-dynamic` feature) — tests that need it will skip gracefully if the runtime is absent.

## Architecture Details

### Frontend Navigation (Not File-Based Routing)

The desktop app does **not** use SvelteKit or file-based routing. Navigation is a manual state machine in `src/lib/navigation.ts` (`NavigationStore` class) with five views conditionally rendered in `App.svelte`:

- `collections` — list all collections
- `collection` — single collection (requires `id`, `collectionName`)
- `item` — single item (requires `itemId`, `collectionId`, `collectionName`, `itemTitle`)
- `db-browser` — SQLite table browser with pagination, sorting, search, column inspection
- `settings` — app settings (API keys, model selection, dependency manager via `DependenciasTab`, logs via `LogsTab`)

NavigationStore supports `push()`, `back()`, `replace()` (sibling without stacking), `resetToPath()`, and breadcrumb-aware pub/sub that reacts to i18n locale changes.

Views live in `src/views/`, layout in `src/layout/`:
- `AppShell.svelte` — Zotero-style collapsible sidebar (`Ctrl+B`), toolbar, `DocumentExplorer` tree-view, deps toast system, status bar
- `TopBar.svelte` — breadcrumb nav, back button, theme toggle (dim/dark), settings/db-browser buttons, prev/next document navigation
- `DocumentExplorer.svelte` — tree-view sidebar (collections → items → assets), resizable (drag handle, persisted to `localStorage`)
- `EntropicConstellation.svelte` — canvas-based animated background (spatial grid culling, respects `prefers-reduced-motion`)

### Data Flow (Frontend to Rust)

1. Svelte views call repos from `@entropia/store` (e.g., `item.repo.ts`)
2. Repos use `client.ts` which wraps Tauri's `@tauri-apps/plugin-sql` for SQL operations, or calls `invoke()` for Rust commands
3. Rust Tauri commands (`db_execute`, `db_select`) operate on shared `AppDbState` (rusqlite)
4. Background AI commands (`extract_text`, `index_fts`, `embed_asset`, `extract_entities`, `extract_entities_for_asset`, `extract_triples`, `transcribe_audio`) go through async job queues (`OcrQueue`, `NlpQueue`, `TranscriptionQueue`). Layout detection runs inside the OCR pipeline (PaddleVL). Direct read/admin commands like `similar_assets` and `backfill_asset_embeddings` use direct DB/blocking pathways instead of the queue.
5. LLM commands (`llm_correct_ocr`, `llm_summarize`, `llm_extract_entities`, etc.) go through `LlmQueue`. Settings commands (`settings_get`, `settings_set`) use direct DB access via `AppDbState`.

### SQLite Connections

The Rust backend manages multiple SQLite connections to `entropia.sqlite`:

- **UI connection** — used by Tauri IPC commands (reads/writes from frontend)
- **OCR worker connection** — dedicated to OCR job queue
- **NLP worker connection** — opened inside the NLP worker for queue processing (asset embeddings stored as BLOBs in `vec_assets`)

Current runtime/product architecture is **asset-only** for embeddings and similarity: `embed_asset`, `backfill_asset_embeddings`, and `similar_assets` are the active APIs. Treat `vec_items`, `similar_items`, `embed_item`, and `embeddings_fallback` as legacy/archive references only.

All connections use WAL mode + foreign keys enabled. Each queue worker opens its own connection independently.

On startup, `lib.rs` runs: (1) suppress Windows CRT error dialogs via `SetErrorMode`, (2) legacy migration from old `com.entropia.app` directory (SQLite bundle comparison by "richness score"), (3) legacy asset path rewriting, (4) deduplication of extractions/transcriptions rows (unique index enforcement), (5) `extractions.method` CHECK constraint migration, (6) `llm_results` table creation, (7) `layouts` table creation, (8) `assets.sort_index` column addition, (9) `app_settings` table creation, (10) `RuntimeManager` init + `ensure_ready_or_bootstrap`, (11) background dep check (`probe_all_once`), (12) queue workers start.

### OCR Provider Chain

OCR uses a fallback chain defined in `ocr/mod.rs`:

- **PaddleOCR-VL** (primary) — Python subprocess (`paddle_vl.py`) using `paddleocr[doc-parser]`. Does layout detection + OCR in a single pass, returns structured blocks with labels, bounding boxes, and reading order. Method field: `"paddle_vl"`.
- **PaddleOCR** (fallback) — `ocr-rs` crate with MNN backend, feature-gated as `paddle-ocr`. PP-OCRv5 detection + latin recognition. PP-LCNet orientation model auto-corrects 0°/90°/180°/270° rotation. `OcrEngine` is `Send + Sync`.
- **Provider chain**: PaddleVL → PaddleOCR → Error. PaddleVL is tried first automatically in OCRH; if unavailable, fails, or times out, it falls back to PaddleOCR light.
- **PDF pipeline** — Native text extraction first (`pdf-extract`), quality-checked (`is_quality_text()`: ≥50 alphanumeric chars). Falls back to pdfium-render at 300 DPI + OCR per page. Method field: `"native"` | `"pdf_paddle_vl"` | `"pdf_paddle"`.

Postprocessing heuristics in `postprocess.rs` are **DISABLED** (mixed columns). Kept for reference only.

### Layout Detection

Two layout engines available:

- **PaddleVL** (primary) — layout detection is integrated into PaddleOCR-VL's single-pass pipeline.
- **ONNX PP-DocLayout-S** — standalone PicoDet ONNX model (`resources/models/ocr/PP-DocLayout-S.onnx`, 4.68 MB). 23 region categories. Input: 2 tensors (image [1,3,480,480] + scale_factor [1,2]).

Reading order uses union-find column grouping: regions with ≥50% horizontal overlap → same column, columns left-to-right, regions within columns top-to-bottom. Results stored in `layouts` table. See `AGENTS.md` for full architecture details.

### Python Subprocess Architecture

Several features delegate to Python scripts (ORT/MSVC linker failures on Windows made native Rust unusable for some tasks):

- **`scripts/embed.py`** — fastembed with `paraphrase-multilingual-MiniLM-L12-v2` (384 dims, 50+ languages). Returns JSON wrapped in `===EMBED_JSON_BEGIN===` / `===EMBED_JSON_END===` sentinels.
- **`scripts/transcribe.py`** — faster-whisper with `base` model, `int8` compute, default language `es`. Same sentinel pattern.
- **`scripts/spacy_ner.py`** — spaCy NER backend (optional, used by hybrid NER engine when spaCy is available).
- **`scripts/layout_detect.py`** — DocLayout-YOLO layout detection. Same sentinel pattern (`===LAYOUT_JSON_BEGIN===` / `===LAYOUT_JSON_END===`).
- **`scripts/paddle_vl.py`** — PaddleOCR-VL layout + OCR in one pass. Sentinel pattern (`===VL_JSON_BEGIN===` / `===VL_JSON_END===`). Label mapping: doc_title/paragraph_title → title, text → plain_text, image → figure.

Rust spawns Python via `which_python()` / `which_python_for_layout()` (searches conda envs first, falls back to system Python). All Python-backed features degrade non-fatally if Python or dependencies are unavailable.

**Python deps required**: `fastembed`, `faster-whisper`, `doclayout-yolo`, `paddleocr[doc-parser]` (install via pip/conda). Optional: `spacy` + `es_core_news_sm` model for spaCy NER.

### Hybrid NER Architecture

NER uses a multi-engine approach (`nlp/ner/`):

- **ONNX** (`onnx.rs`) — BERT-based NER via `ort` (ONNX Runtime) + `tokenizers`. Model files bundled in `resources/models/ner/` (config, tokenizer, vocab). Requires ONNX Runtime dynamic library at runtime (`load-dynamic` feature).
- **spaCy** (`spacy.rs`) — Python subprocess calling `spacy_ner.py`. Optional fallback/complement.
- **Rule-based** (`rule_based.rs`) — Pattern matching for dates, locations, etc. Always available.
- **Hybrid** (`hybrid.rs`) — Orchestrates all three engines, merges results via `merge.rs`.

Engine selection is configured via `NerConfig` with `NerEngineKind` (Onnx, Spacy, Hybrid, RuleBased). The `NerRegistry` initializes available engines at startup and logs preflight status.

### Cloud Backend Alternatives

OCR and transcription each support local and cloud modes, selectable in settings:

- **GLM OCR** (`ocr/glm_ocr.rs`) — ZhipuAI cloud OCR API. Mode key: `OCRH_MODE` (`local` | `glm_ocr` | `auto`). API key: `GLM_OCR_API_KEY`.
- **AssemblyAI** (`transcription/assemblyai.rs`) — Cloud speech-to-text. Mode key: `STT_MODE` (`local` | `assemblyai` | `auto`). API key: `ASSEMBLYAI_API_KEY`.

### LLM Architecture

LLM system in `llm/`:

- **OpenRouter** (`openrouter.rs`) — Cloud API via `reqwest`. Model and API key stored in `app_settings` table. Frontend configures via `SettingsView`.
- **Engine** (`engine.rs`) — `LlmEngine` abstracts the backend behind `LlmConfig`. Reads settings from `app_settings` to decide configuration.
- **Prompts** (`prompt.rs`) — All prompts in Spanish, matching source text language. Structured prompts for each job type (OCR correction, entity extraction, summarization, classification, Q&A, triple extraction).
- **Results** persisted in `llm_results` table (target_id, job_type, result JSON, timestamp).

### Job Queue Pattern

All background systems (OCR, NLP, Transcription, Layout, LLM, Geo) follow the same pattern:

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

### i18n

`src/lib/i18n.ts` provides `es`/`en` locale support via a Svelte writable store. Locale is persisted to `app_settings` via `SETTINGS_KEYS.LANGUAGE`. All UI strings go through `t('key')`. NavigationStore breadcrumbs react to locale changes.

### Additional Schema Tables

Beyond the core Drizzle tables (collections, items, assets, notes, extractions, entities, triples, transcriptions, layouts, llm_results), the Drizzle schema includes:

- `annotations` — visual overlays on assets (rectangle/underline with bbox, color, page). Cascade delete from assets.
- `topics` — reusable tags (unique name).
- `item_topics` — many-to-many junction between items and topics.

**Rust-only tables** (created in `lib.rs`, not in Drizzle schema): `jobs`, `embeddings`/`vec_assets` (BLOBs), `fts` (FTS5), `app_settings`, `layouts` (also has Drizzle definition).

### Collection Export

`src/lib/export.ts` — JSON export of entire collections (assets, notes, extractions, annotations, transcriptions). Uses `@tauri-apps/plugin-dialog` save dialog + `@tauri-apps/plugin-fs` write. Export format version 2.

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
- Cargo feature flag: `paddle-ocr` (default, enables `ocr-rs` MNN backend). Without it, PaddleOCR native engine is excluded.
