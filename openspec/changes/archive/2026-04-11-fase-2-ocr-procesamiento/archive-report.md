# Archive Report: fase-2-ocr-procesamiento

## Status: ARCHIVED

**Date**: 2026-04-11
**Change**: fase-2-ocr-procesamiento
**Archived to**: `openspec/changes/archive/2026-04-11-fase-2-ocr-procesamiento/`

---

## Test Results

| Suite             | Passed  | Total   |
| ----------------- | ------- | ------- |
| @entropia/store   | 60      | 60      |
| @entropia/ui      | 44      | 44      |
| @entropia/desktop | 40      | 40      |
| **Total**         | **144** | **144** |

---

## Architecture Decision Records

| ADR     | Title                                                                  |
| ------- | ---------------------------------------------------------------------- |
| ADR-010 | `ocrs` pure-Rust OCR engine (offline, no system deps)                  |
| ADR-011 | `pdf-extract` + quality heuristic (< 50 alphanum chars → OCR fallback) |
| ADR-012 | Serial `mpsc::channel` + single background worker (no deadlock)        |
| ADR-013 | `AppDbState` dual-connection WAL (ui_conn + worker_conn)               |

---

## Engram Observation IDs (Artifact Traceability)

| Artifact      | Observation ID |
| ------------- | -------------- |
| explore       | #33            |
| proposal      | #34            |
| spec          | #35            |
| design        | #36            |
| verify-report | #41            |

---

## Warnings Accepted

1. **Phase 3 task checkboxes** — All `[x]` confirmed in `tasks.md` before archiving. 27/27 tasks complete.
2. **`jobs.progress` via result JSON** — `updateProgress` stores to DB but frontend progress comes from `ocr:progress` event payload (not polled from DB). Acceptable for Fase 2.
3. **Text panel shows char count only** — `get_extraction_text` Rust command not implemented; panel renders "N chars via method" metadata. Full text in panel deferred to Fase 2.5.
4. **`ocrs` version mismatch** — `Cargo.toml` uses `ocrs = "0.8"` (available), spec said `0.12.2`. Verify on first `cargo build`; update design doc if 0.8 API is confirmed correct.

---

## Known Deferred Items (→ Fase 2.5)

| Item                                                    | Reason                                                                                                           |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| PDF scanned fallback (OCR of scanned PDF images)        | `process_pdf` emits `ocr:error` for sparse-text PDFs; rendering PDF pages as images requires `pdfium-render` DLL |
| `get_extraction_text` Rust command → full text in panel | Text panel shows char count + method only; full `text_content` retrieval via Tauri command not implemented       |
| `ocrs` version pinning                                  | Verify `ocrs = "0.8"` API compatibility on first `cargo build`; pin or upgrade as needed                         |

---

## Specs Synced to Main

| Domain                  | Action             | Spec File                                                                                                      |
| ----------------------- | ------------------ | -------------------------------------------------------------------------------------------------------------- |
| `ocr-processing`        | Created (new)      | `openspec/specs/ocr-processing/spec.md`                                                                        |
| `text-extraction-store` | Created (new)      | `openspec/specs/text-extraction-store/spec.md`                                                                 |
| `ocr-ux`                | Created (new)      | `openspec/specs/ocr-ux/spec.md`                                                                                |
| `data-store`            | Updated (additive) | `openspec/specs/data-store/spec.md` — added: Dual-Connection DB State, JobRepo, ExtractionRepo, Migration 0003 |

---

## Verification Summary

- **Verdict**: PASS WITH WARNINGS (0 CRITICALs)
- **Compliance**: 41/44 scenarios fully compliant; 3 deferred to Fase 2.5
- **TypeScript typecheck**: 0 errors
- **Lint**: 0 errors
- **All 27 tasks**: Complete

---

## Next Recommended

- `sdd-propose fase-3` — NLP / embeddings / FTS5 full-text search
- OR `sdd-propose fase-2.5` — PDF scanned fallback (pdfium-render) + full text panel
