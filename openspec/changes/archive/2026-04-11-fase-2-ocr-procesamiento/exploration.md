# Exploration: Fase 2 — OCR + Document Processing

**Date**: 2026-04-11
**Change**: fase-2-ocr-procesamiento
**Status**: Complete

---

## Current State

Fase 1 (MVP Documental) is archived and passing (113 tests). The app can:

- Import PDFs and images into appDataDir (`assets/{coll_id}/{item_id}/`)
- View PDFs via `pdfjs-dist` in the frontend (canvas rendering)
- View images via `convertFileSrc()` webview URLs
- Store asset records in SQLite (`assets` table: id, item_id, path, type, size)
- The `jobs` table is **already defined** in schema with: id, type (`ocr|ner|embeddings|triples`), status (`pending|running|done|error`), asset_id (FK), result (JSON blob), error, created_at, updated_at

### Relevant Existing Infrastructure

| Layer          | What exists                                                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Rust backend   | `rusqlite` (bundled), `tokio` (via Tauri), `serde_json`, `thiserror`                                                           |
| Tauri commands | `db_execute` + `db_select` (raw SQL IPC bridge)                                                                                |
| State          | `AppDb(Mutex<Connection>)` — **single connection, sync**                                                                       |
| Events         | `core:event:default` permission already granted                                                                                |
| Frontend       | Svelte 5 runes, Drizzle sqlite-proxy, `StoreApi` with `JobRepo` (not yet in store — `jobs` table exists in schema but no repo) |
| Schema         | `jobs` table ready, `assets` table needs `text_content` column                                                                 |

### Critical Constraint: Single Mutex Connection

The current `AppDb` is `Mutex<Connection>` — **blocking**. Long OCR jobs (30s–5min per document) would deadlock the UI if run on the same mutex-guarded connection. This is the most important architectural issue to solve in Fase 2.

---

## Affected Areas

- `apps/desktop/src-tauri/Cargo.toml` — new OCR/PDF crates, `tokio` features
- `apps/desktop/src-tauri/src/lib.rs` — new job processor module, async spawn, event emit
- `apps/desktop/src-tauri/src/db/state.rs` — needs dedicated async connection pool or separate write connection
- `apps/desktop/src-tauri/src/db/commands.rs` — new OCR commands: `start_ocr_job`, `get_job_status`
- `packages/store/src/schema.ts` — add `text_content` column to `assets` table or create `extractions` table
- `packages/store/src/repos/` — add `JobRepo` (create, findById, updateStatus, findByAsset)
- `apps/desktop/src/views/ItemView.svelte` — OCR trigger button, progress indicator per asset
- `apps/desktop/src/lib/` — `ocr.ts` client wrapper for new Tauri commands + event listeners

---

## Area 1: OCR Strategy (Rust)

### Option A: `tesseract` crate (v0.15.2) — Higher-level Tesseract bindings

- **What it is**: Rust wrapper over `tesseract-sys` + Leptonica
- **Windows**: Requires vcpkg or pre-built Tesseract DLLs. Build is complex on Windows; needs `TESSERACT_INCLUDE_PATH`, `TESSERACT_LIB_PATH` env vars and MSVC toolchain
- **Accuracy**: Production-grade (Tesseract 5.x LSTM). Best for historical docs with varied fonts
- **Language support**: Excellent — 100+ languages including Spanish, Latin scripts
- **Bundling**: HARD. Tesseract + Leptonica + language data files (~50MB/lang) must all be bundled or downloaded at runtime
- **Async**: Blocking C API — must run in `tokio::task::spawn_blocking`
- **Effort**: High (build setup + bundling)
- **Pros**: Best accuracy for complex historical docs, language data customization, proven
- **Cons**: Windows build hell, large bundle, `vcpkg` dependency in CI

### Option B: `leptess` crate (v0.14.0) — Tesseract + Leptonica bundled

- **What it is**: "Productive" Tesseract+Leptonica binding — higher API level than `tesseract`
- **Windows**: Same issues as Option A — still needs system Tesseract or build from source
- **Accuracy**: Same as Tesseract (it IS Tesseract under the hood)
- **Language support**: Same as A
- **Bundling**: Same problem
- **Effort**: Medium-High (slightly simpler API but same build complexity)
- **Pros**: Cleaner API (set_image from bytes directly)
- **Cons**: Same Windows build pain, less maintained than `tesseract` crate

### Option C: `ocrs` crate (v0.12.2) — Pure Rust, ML-based, NO SYSTEM DEPS ✅

- **What it is**: Neural network OCR engine using RTen (Rust ONNX runtime). Models downloaded separately (~20MB total for detection + recognition)
- **Windows**: Pure Rust — `cargo build` just works. MSVC or GNU toolchain, no vcpkg, no system libs
- **Accuracy**: Good for printed text, slightly behind Tesseract 5 for difficult historical manuscripts. **CRITICAL LIMITATION: Latin alphabet ONLY** — no support for non-Latin scripts (Issue #8 open)
- **Language support**: English/Latin script ONLY currently
- **Bundling**: Models (`.rten` files) need to be bundled or downloaded at first run (~20MB). The CLI downloads automatically; library requires explicit model loading
- **Async**: Pure Rust — can run in `tokio::task::spawn_blocking` easily
- **Effort**: Low (add dep + download models)
- **Pros**: Zero native deps, clean Rust API, cross-platform, 1.8k GitHub stars, active development
- **Cons**: Latin-only, less mature than Tesseract, models need distribution strategy

### Option D: `tesseract` CLI via `std::process::Command`

- **What it is**: Shell out to system `tesseract` executable
- **Windows**: Requires user to install Tesseract themselves, or app must bundle the exe
- **Accuracy**: Same as Tesseract
- **Bundling**: Must bundle Tesseract installer/portable binary (~50MB+) or require user install
- **Effort**: Low to code, High for distribution
- **Pros**: No Rust build complexity, easily testable
- **Cons**: Requires external install, terrible UX for end users, unreliable path detection on Windows

### Recommendation: OCR

**Phase 2A: `ocrs` (pure Rust, zero deps)** — start here for rapid delivery and zero build friction. Models are bundled in `resources/` (Tauri bundles them into the app).

**Phase 2B (later): add `tesseract` as opt-in** for users who have it installed, as a quality upgrade path. This is a backend decision that doesn't affect the job API.

The historical documents in scope for EntropIA are primarily Spanish-language Latin-script documents — `ocrs` covers this exactly. The accuracy gap vs Tesseract is real but acceptable for Fase 2; Fase 3 NLP can work with imperfect text.

---

## Area 2: PDF Text Extraction (Native Layer)

### Option A: `pdf-extract` crate (v0.10.0) — Pure Rust ✅

- **What it is**: Extracts text from PDF's native text layer (encoded glyphs). **No image rendering**
- **Windows**: Pure Rust — works out of the box
- **Quality**: Good for born-digital PDFs with embedded text. Struggles with multi-column layouts, complex encoding, CJK. 576 GitHub stars, 49 open issues
- **Handles scanned PDFs?**: NO — scanned PDFs are images. Falls through to OCR path
- **Effort**: Very low (add dep, call `extract_text_from_mem(&bytes)`)
- **Pros**: Dead simple API, no deps, covers the "happy path" for digital PDFs
- **Cons**: Won't handle column ordering in multi-column layouts, some encoding bugs

### Option B: `lopdf` crate (v0.40.0) — PDF manipulation

- **What it is**: Low-level PDF parser and manipulator. Text extraction is possible but manual
- **Quality**: Better structural access than `pdf-extract` but requires more code to extract text properly
- **Async**: Has `async` feature (tokio)
- **Effort**: High — text extraction requires walking the content stream manually
- **Pros**: More control over extraction logic
- **Cons**: Not focused on text extraction; requires significant code to get readable output

### Option C: `pdfium-render` (v0.9.0) — PDFium bindings ✅

- **What it is**: Rust wrapper for Google's PDFium (same engine as Chrome and our existing pdfjs-dist)
- **Windows**: Requires bundling `pdfium.dll` (~5MB). `pdfium-auto` crate can download it automatically
- **Quality**: BEST — handles complex layouts, multi-column, CJK, encrypted PDFs, renders pages to images for OCR fallback
- **Bundling**: PDFium DLL must be in release bundle (Tauri resources)
- **Effort**: Medium (DLL bundling + Tauri resources setup)
- **Key advantage**: Can render PDF pages to images → feed directly to OCR pipeline when no text layer found
- **Pros**: Same engine as frontend viewer, best extraction quality, page-to-image for OCR, handles edge cases
- **Cons**: DLL bundling adds build complexity, larger app size

### Recommendation: PDF Extraction

**Hybrid approach:**

1. **`pdf-extract`** as primary: attempt native text layer extraction first (simple, zero deps)
2. **If text is empty/insufficient** (page count > 0 but text < threshold): fall back to **page-to-image via `pdfium-render`** → then OCR via `ocrs`

This gives the best UX: fast extraction for digital PDFs, full OCR for scanned ones. The `pdfium.dll` bundle is justified by the quality gain and the fact pdfjs-dist is ALREADY the same engine on the frontend.

Alternative: skip `pdfium-render` initially, use `pdf-extract` only for Fase 2, add pdfium in Fase 2.5 when quality demands it.

---

## Area 3: Async Job Processing in Tauri

### The Core Problem

Current `AppDb(Mutex<Connection>)` is synchronous. A `#[tauri::command]` that runs OCR would block the entire Tauri thread while the job runs, making the UI unresponsive.

### Solution: Dedicated Job Runner with Tokio + Separate DB Connections

**Architecture:**

```
Frontend (Svelte)
    │ invoke("start_ocr_job", { assetId })
    ▼
Tauri Command (async)
    │ 1. INSERT job row (status=pending)
    │ 2. tauri::async_runtime::spawn(ocr_task)
    │ 3. Return job_id immediately
    ▼
Background Task (tokio::spawn)
    │ 1. UPDATE job status=running
    │ 2. Run OCR (spawn_blocking for CPU work)
    │ 3. app_handle.emit("ocr:progress", { job_id, progress })
    │ 4. UPDATE job status=done, result=text
    │ 5. app_handle.emit("ocr:complete", { job_id, text })
    ▼
Frontend listener (onMount → listen("ocr:progress"))
    │ Update $state progress indicator
```

**Key implementation decisions:**

- `AppDb` must become `Arc<Mutex<Connection>>` or better: **separate read/write connections** — one for the main app (Drizzle IPC), one for the job runner. SQLite WAL mode (already enabled) supports concurrent readers + one writer.
- OCR CPU work must be in `tokio::task::spawn_blocking` — never block the async executor
- **One job at a time** for Fase 2 (serial queue). Historical docs are large; concurrent OCR would thrash memory/CPU. Fase 3 can add parallel processing.
- Job queue: simple polling vs channel. Recommendation: **use a `tokio::sync::mpsc` channel** to send job requests to a dedicated background worker task that processes them serially.

**Tauri event pattern (already have `core:event:default` permission):**

```rust
app_handle.emit("ocr:progress", serde_json::json!({
    "jobId": job_id,
    "progress": 45,       // 0-100
    "message": "Recognizing text..."
})).ok();
```

**Frontend:**

```typescript
import { listen } from '@tauri-apps/api/event'

const unlisten = await listen<OcrProgress>('ocr:progress', (event) => {
  ocrProgress[event.payload.jobId] = event.payload.progress
})
```

### Job State Machine

```
pending → running → done
                 ↘ error
```

All transitions written atomically to SQLite via the job runner's dedicated connection.

---

## Area 4: Image Preprocessing for OCR Quality

### `image` crate (pure Rust) — Sufficient for Fase 2

The `image` crate provides:

- `image::DynamicImage::grayscale()` → convert to grayscale
- Custom threshold (convert grayscale to binary black/white)
- Resize: upscale to 300 DPI equivalent if image is small
- No deskew (not in `image` crate)

### `imageproc` crate — Adds threshold + basic filters

Provides adaptive thresholding, gaussian blur, and dilation — exactly what improves OCR accuracy on noisy historical docs.

### `opencv` — WAY overkill

C++ bindings, huge build, not justified. Reject.

### Recommendation: Image Preprocessing

- **`image` crate** (likely already transitively present via `ocrs`) for grayscale + resize
- **`imageproc` crate** for adaptive threshold (better than global threshold for uneven lighting in historical docs)
- **NO deskew in Fase 2** — complex and rarely needed; add in Fase 3 if accuracy issues arise
- Pipeline: `grayscale → adaptive_threshold → resize if < 150dpi` before feeding to `ocrs`

Note: `ocrs` itself does internal preprocessing via `rten-imageproc`. For good-quality scans, `ocrs` may not need extra preprocessing. Reserve heavy preprocessing for when quality metrics show issues.

---

## Area 5: Text Storage Strategy

### Option A: Add `text_content` column to `assets` table

```sql
ALTER TABLE assets ADD COLUMN text_content TEXT;
ALTER TABLE assets ADD COLUMN ocr_status TEXT DEFAULT 'none'; -- 'none'|'pending'|'done'|'error'
```

- **Pros**: Simple, single JOIN when loading an asset, no extra table
- **Cons**: Assets table grows large; `text_content` can be 50KB+ per doc; mixing file metadata with content

### Option B: Separate `extractions` table ✅

```sql
CREATE TABLE extractions (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES assets(id),
  text_content TEXT NOT NULL,
  method TEXT NOT NULL,     -- 'native_pdf' | 'ocr'
  created_at INTEGER NOT NULL
);
```

- **Pros**: Clean separation, enables multiple extraction versions, easily extensible for Fase 3 (store embeddings alongside text, or add `chunks` child table), assets table stays lean
- **Cons**: One extra JOIN when searching

### Option C: Store in `jobs.result` JSON blob

The `jobs.result` column is already a JSON blob. Could store extracted text there directly.

- **Pros**: Zero schema change
- **Cons**: Makes `jobs` table a dumping ground; text cannot be efficiently searched; result is lost if job row is cleaned up

### Recommendation: Text Storage

**Option B — separate `extractions` table**. This is the RIGHT architectural decision:

- Fase 3 NLP will need `extractions` as its input source
- Enables FTS5 virtual table over `extractions.text_content` for full-text search (Fase 3)
- Multiple extraction passes (first OCR run, then re-run with better model) don't conflict
- `jobs.result` remains clean (just job metadata/errors)

Schema addition:

```typescript
// packages/store/src/schema.ts
export const extractions = sqliteTable('extractions', {
  id: text('id').primaryKey(),
  assetId: text('asset_id')
    .notNull()
    .references(() => assets.id),
  textContent: text('text_content').notNull(),
  method: text('method').notNull(), // 'native_pdf' | 'ocr'
  createdAt: integer('created_at').notNull(),
})
```

Also add `ExtractionRepo` to `packages/store` and a `JobRepo` (was planned but not implemented in Fase 1).

---

## Area 6: UX for Async Processing

### Process Trigger: Manual Button (NOT auto-process)

**Decision: Manual "Extract Text" button** on each asset in ItemView.

Why: Historical docs can be 200+ page PDFs. Auto-processing on import would make import slow and unresponsive. Users should control when OCR runs (they may not need it for all documents).

### Progress Indicator Design

In `ItemView.svelte` right panel, per-asset:

```
[asset thumb]  acta_1820.pdf
               ● No text extracted   [Extract Text]

[after clicking]:
               ◌ Extracting... 45%  [Cancel?]

[done]:
               ✓ 3,421 words extracted
```

- Use Tauri `listen()` on `ocr:progress` event to update `$state` reactively
- Show word count after extraction (proxies quality check)
- If error: show error message with retry button

### Non-blocking: YES

While OCR runs in the background Tokio task:

- User can navigate to other items/collections
- The app stays fully responsive
- Progress events arrive via Tauri events regardless of current route
- `ItemView` subscribes/unsubscribes to events on mount/unmount

### Processing Order: Serial Queue

One job at a time, FIFO. Reasons:

- OCR is CPU-intensive — concurrent jobs would degrade quality and responsiveness
- Historical archivists work through documents sequentially anyway
- Simpler implementation with `mpsc` channel + single worker

---

## Recommended Architecture Summary

```
┌─────────────────────────────────────────────────────────┐
│                     FASE 2 ARCH                          │
│                                                          │
│  Frontend (Svelte)                                       │
│  ┌─────────────────────────────────────────────────┐   │
│  │ ItemView                                         │   │
│  │  - "Extract Text" button per asset               │   │
│  │  - listen("ocr:progress") → $state update        │   │
│  │  - shows word count / error                      │   │
│  └──────────────────┬──────────────────────────────┘   │
│                     │ invoke("start_ocr_job")            │
│  Tauri IPC          │                                    │
│  ┌──────────────────▼──────────────────────────────┐   │
│  │ ocr::commands                                    │   │
│  │  - start_ocr_job(asset_id) → job_id             │   │
│  │  - get_job(job_id) → Job                         │   │
│  └──────────────────┬──────────────────────────────┘   │
│                     │ mpsc::Sender<JobRequest>           │
│  Job Runner         │                                    │
│  ┌──────────────────▼──────────────────────────────┐   │
│  │ OcrWorker (tokio task, spawned in setup())       │   │
│  │  loop {                                          │   │
│  │    rx.recv() → JobRequest                        │   │
│  │    spawn_blocking(|| {                           │   │
│  │      1. pdf-extract OR pdfium page-to-image      │   │
│  │      2. image preprocess (image + imageproc)     │   │
│  │      3. ocrs engine → text                       │   │
│  │    })                                            │   │
│  │    INSERT extractions row                        │   │
│  │    UPDATE job status                             │   │
│  │    app.emit("ocr:complete", ...)                 │   │
│  │  }                                               │   │
│  └─────────────────────────────────────────────────┘   │
│                                                          │
│  Storage                                                 │
│  ┌─────────────────────────────────────────────────┐   │
│  │ SQLite (WAL)                                     │   │
│  │  jobs      → status tracking                     │   │
│  │  extractions → text_content (+ FTS5 in Fase 3)  │   │
│  └─────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

---

## Approaches Comparison Table

| Area       | Option                | Effort               | Accuracy            | Windows         | Verdict         |
| ---------- | --------------------- | -------------------- | ------------------- | --------------- | --------------- |
| OCR        | `ocrs` (pure Rust)    | Low                  | Good (Latin)        | ✅ native       | **RECOMMENDED** |
| OCR        | `tesseract` crate     | High                 | Best                | ⚠️ vcpkg        | Future upgrade  |
| OCR        | tesseract CLI         | Low code / High dist | Best                | ⚠️ user install | Rejected        |
| PDF native | `pdf-extract`         | Low                  | Good (born-digital) | ✅ native       | **RECOMMENDED** |
| PDF native | `pdfium-render`       | Medium               | Best                | ⚠️ DLL bundle   | Fase 2.5        |
| PDF native | `lopdf`               | High                 | Manual extraction   | ✅ native       | Rejected        |
| Async      | tokio spawn + mpsc    | Medium               | N/A                 | ✅              | **RECOMMENDED** |
| Image pre  | `image` + `imageproc` | Low                  | Good                | ✅ native       | **RECOMMENDED** |
| Storage    | `extractions` table   | Low                  | N/A                 | ✅              | **RECOMMENDED** |
| UX         | Manual button         | Low                  | N/A                 | ✅              | **RECOMMENDED** |

---

## Risks

### Risk 1: `AppDb` Single Mutex Blocks Job Worker

**Current**: `AppDb(Mutex<Connection>)` — one connection, sync. If the OCR worker tries to UPDATE job status while the UI is running a SELECT, they contend on the same mutex.

**Impact**: HIGH — potential deadlock, very poor UX

**Mitigation**: In Fase 2 setup, refactor `AppDb` to `Arc<Mutex<Connection>>` for UI path PLUS a second `Mutex<Connection>` for the job worker (SQLite WAL mode allows concurrent readers + 1 writer). Alternatively, use `rusqlite_async` or a simple connection pool.

### Risk 2: `ocrs` Model Distribution

**ocrs** requires two `.rten` model files (~10MB each: text detection + recognition models). These must be either:

- Bundled inside the Tauri app (adds ~20MB to installer)
- Downloaded on first OCR run (requires internet, bad offline UX for historians)

**Impact**: Medium — affects installer size and first-run UX

**Mitigation**: Bundle models in `apps/desktop/src-tauri/resources/`. Tauri's `bundle.resources` config handles this. Models are static and don't change per user. 20MB is acceptable for a desktop app.

### Risk 3: `pdf-extract` Encoding Issues with Historical PDFs

Many historical digitized PDFs use non-standard font encodings or ToUnicode tables that `pdf-extract` doesn't handle well. The extracted text may be garbled or empty for old scanned PDFs with OCR text layers.

**Impact**: Medium — poor extraction quality for a significant subset of historical documents

**Mitigation**:

1. Validate extracted text with a quality heuristic (e.g., word density, character ratio)
2. If quality is below threshold, fall back to OCR path automatically
3. In the UI, show extracted character/word count so user can judge quality
4. Plan `pdfium-render` integration in Fase 2.5 as quality upgrade

---

## Recommendation per Area

| Area                    | Recommendation                                                                                             |
| ----------------------- | ---------------------------------------------------------------------------------------------------------- |
| **OCR engine**          | `ocrs` v0.12.2 (pure Rust, Latin alphabet, 20MB models bundled)                                            |
| **PDF native text**     | `pdf-extract` v0.10.0 (primary) + quality threshold → OCR fallback                                         |
| **Async architecture**  | `tokio::task::spawn_blocking` for CPU work + `mpsc::channel` serial job queue + `app_handle.emit()` events |
| **Image preprocessing** | `image` crate for grayscale/resize + `imageproc` for adaptive threshold                                    |
| **Text storage**        | New `extractions` table (clean separation, FTS5-ready for Fase 3)                                          |
| **UX**                  | Manual "Extract Text" per asset, non-blocking background processing, per-asset progress via Tauri events   |
| **Connection strategy** | Separate `Arc<Mutex<Connection>>` for job worker vs. UI Drizzle path                                       |

---

## Ready for Proposal

**YES** — sufficient clarity to proceed. Key open questions before spec:

1. **Model distribution**: Bundle `ocrs` models in app (`resources/`) or download on first run? → Recommend bundle (historian offline use case)
2. **PDF native first or OCR directly?**: Always try native layer first, with quality fallback? → YES, it's fast and free
3. **Re-run OCR**: Should users be able to re-run OCR (overwrite extraction)? → YES, allow re-run (update existing extraction row)
4. **Multi-page PDF progress**: Report per-page progress (1/47 pages) or just % of total? → Per-page is better UX

No blockers. All technology choices are confirmed compatible with Windows + Tauri 2 + the existing stack.
