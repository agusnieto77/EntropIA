# Spec Summary: Fase 2 — OCR + Document Processing

## Overview

| Spec File                        | Type          | Requirements | Scenarios |
| -------------------------------- | ------------- | ------------ | --------- |
| `specs/ocr-processing.md`        | New           | 6            | 14        |
| `specs/text-extraction-store.md` | New           | 5            | 13        |
| `specs/ocr-ux.md`                | New           | 5            | 15        |
| `specs/data-store.md`            | Delta (ADDED) | 4            | 9         |
| **Total**                        |               | **20**       | **51**    |

## Spec Files

- [`specs/ocr-processing.md`](./specs/ocr-processing.md) — OCR job lifecycle, PDF native extraction with quality heuristic, image preprocessing, non-blocking execution, manual trigger, re-processing, error handling
- [`specs/text-extraction-store.md`](./specs/text-extraction-store.md) — `extractions` table schema, migration 0003, `ExtractionRepo` (create/findByAsset/upsert/delete), `JobRepo` (create/findPending/updateStatus/updateProgress), text access by asset
- [`specs/ocr-ux.md`](./specs/ocr-ux.md) — "Extract Text" button states, per-asset progress indicator, extracted text collapsible panel, per-asset status badge, background operation UI interactivity
- [`specs/data-store.md`](./specs/data-store.md) — DELTA: dual-connection `AppDbState` (WAL, no deadlock), `JobRepo` additions, `ExtractionRepo` additions, migration 0003

## Coverage

| Area                                                   | Status     |
| ------------------------------------------------------ | ---------- |
| Happy paths                                            | ✅ Covered |
| Edge cases (empty text layer, null confidence, re-run) | ✅ Covered |
| Error states (job error, UI error display, retry)      | ✅ Covered |
| Concurrency / non-blocking                             | ✅ Covered |
| Manual trigger / no auto-process                       | ✅ Covered |

## Next Step

Ready for **design** (`sdd-design`): technical architecture for OCR module, dual-connection DB state, serial queue, and frontend event wiring.
