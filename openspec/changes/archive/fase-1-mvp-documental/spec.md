# Specifications: Fase 1 — MVP Documental

## Overview

This document indexes all delta specifications for the Fase 1 MVP Documental change. Each domain has its own spec file under `specs/{domain}/spec.md`.

## New Specifications (9)

| Domain                 | Path                                   | Requirements | Scenarios |
| ---------------------- | -------------------------------------- | ------------ | --------- |
| testing-infrastructure | `specs/testing-infrastructure/spec.md` | 6            | 10        |
| file-import            | `specs/file-import/spec.md`            | 7            | 11        |
| collection-management  | `specs/collection-management/spec.md`  | 4            | 9         |
| item-management        | `specs/item-management/spec.md`        | 4            | 11        |
| document-viewer        | `specs/document-viewer/spec.md`        | 5            | 9         |
| notes                  | `specs/notes/spec.md`                  | 4            | 6         |
| search                 | `specs/search/spec.md`                 | 5            | 8         |
| export                 | `specs/export/spec.md`                 | 3            | 6         |
| navigation             | `specs/navigation/spec.md`             | 4            | 7         |

## Delta Specifications (4 — modify existing specs)

| Domain        | Path                          | Added               | Modified   | Scenarios |
| ------------- | ----------------------------- | ------------------- | ---------- | --------- |
| data-store    | `specs/data-store/spec.md`    | 5 added             | 0 modified | 8         |
| design-system | `specs/design-system/spec.md` | 5 added             | 0 modified | 8         |
| desktop-app   | `specs/desktop-app/spec.md`   | 2 added, 1 modified | 1 modified | 6         |
| ci            | `specs/ci/spec.md`            | 1 added, 1 modified | 1 modified | 6         |

## Totals

- **13 domains** specced
- **51 requirements** (42 new + 5 added to existing + 2 modified + 2 ADDED that overlap with modified)
- **105 scenarios** total
- All scenarios use Given/When/Then format
- All requirements use RFC 2119 keywords (MUST, SHALL, SHOULD, MAY)

## Key Technical Decisions Encoded

- Search uses LIKE queries (not FTS5) — sufficient for 100-doc target
- File storage copies to `appDataDir/files/{coll_id}/{item_id}/`
- PDF via `pdfjs-dist`, images via `convertFileSrc()`
- Navigation via runes-based `NavigationStore` (no router library)
- Repository pattern with `DrizzleClient` injection for testability
- Mock `DbClient` interface for store tests (not Tauri invoke)
- `happy-dom` as test environment for component tests
- Export is JSON metadata only (no binary files)
