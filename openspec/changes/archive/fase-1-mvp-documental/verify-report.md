# Verification Report: fase-1-mvp-documental

**Date**: 2026-04-11
**Mode**: Standard
**Verifier**: sdd-verify agent

---

## Completeness

| Metric                            | Value                                             |
| --------------------------------- | ------------------------------------------------- |
| Tasks total                       | 47                                                |
| Tasks complete (in codebase)      | 44                                                |
| Tasks incomplete (implementation) | 3 (Phase 6 — but files ARE present, see below)    |
| tasks.md on-disk sync             | ❌ STALE — shows 21/47 checked; reality is ~44/47 |

**Phase 6 status note**: The apply-progress in engram marks Phase 6 as "not assigned", but reading the actual files confirms:

- `default.json` — has `dialog:allow-open`, `dialog:allow-save`, `fs:*` permissions ✅
- `Cargo.toml` — has `tauri-plugin-dialog = "2"`, `tauri-plugin-fs = "2"` ✅
- `lib.rs` — registers `.plugin(tauri_plugin_dialog::init())` and `.plugin(tauri_plugin_fs::init())` ✅

Phase 6 is DONE in code. tasks.md just wasn't updated.

---

## Build & Tests Execution

**Tests**: ✅ **112/112 passed**, 0 failed, 0 skipped (confirmed by live run)

```
@entropia/store:test:  ✓ src/types.test.ts           (2 tests)
@entropia/store:test:  ✓ src/repos/item.repo.test.ts (11 tests)
@entropia/store:test:  ✓ src/repos/collection.repo.test.ts (10 tests)
@entropia/store:test:  ✓ src/repos/asset.repo.test.ts (7 tests)
@entropia/store:test:  ✓ src/repos/note.repo.test.ts (7 tests)
@entropia/store:test:  ✓ src/repos/store.test.ts     (3 tests)
@entropia/store:test:  Test Files  6 passed (6) | Tests  40 passed (40)

@entropia/ui:test:  ✓ CollectionCard.test.ts  (7 tests)
@entropia/ui:test:  ✓ ItemCard.test.ts        (7 tests)
@entropia/ui:test:  ✓ NoteEditor.test.ts      (9 tests)
@entropia/ui:test:  ✓ DocumentViewer.test.ts  (6 tests)
@entropia/ui:test:  ✓ SearchBar.test.ts       (8 tests)
@entropia/ui:test:  ✓ MetadataEditor.test.ts  (7 tests)
@entropia/ui:test:  Test Files  6 passed (6) | Tests  44 passed (44)

@entropia/desktop:test:  ✓ keyboard.test.ts    (3 tests)
@entropia/desktop:test:  ✓ export.test.ts      (3 tests)
@entropia/desktop:test:  ✓ navigation.test.ts  (9 tests)
@entropia/desktop:test:  ✓ file-import.test.ts (13 tests)
@entropia/desktop:test:  Test Files  4 passed (4) | Tests  28 passed (28)
```

**Typecheck**: ✅ 0 errors (confirmed from Phase 8 apply-progress)

**Lint**: ✅ 0 errors, 3 acceptable warnings (confirmed from Phase 8)

**Build**: ✅ `@entropia/ui` builds successfully with 7 `state_referenced_locally` Svelte warnings (intentional unidirectional $state pattern, documented in apply-progress)

**Coverage**: ➖ Not configured / not run

---

## Spec Compliance Matrix

Sampled from all 13 domains (105 scenarios total; ~62 sampled below):

| Domain                | Requirement               | Scenario                                    | Test                                                            | Result       |
| --------------------- | ------------------------- | ------------------------------------------- | --------------------------------------------------------------- | ------------ |
| testing-infra         | Monorepo Test Runner      | Run all tests from root                     | turbo test (all packages)                                       | ✅ COMPLIANT |
| testing-infra         | Per-Package Config        | Store=node, UI=happy-dom, Desktop=happy-dom | vitest.config.ts x3                                             | ✅ COMPLIANT |
| testing-infra         | Store Test Mocking        | Mock DbClient records calls                 | store.test.ts (3 tests)                                         | ✅ COMPLIANT |
| testing-infra         | Coverage Reporting        | coverage-v8 available                       | vitest configs include @vitest/coverage-v8                      | ✅ COMPLIANT |
| data-store            | Collection Repository     | Create and retrieve                         | collection.repo.test.ts > create                                | ✅ COMPLIANT |
| data-store            | Collection Repository     | Count items                                 | collection.repo.test.ts > countItems                            | ✅ COMPLIANT |
| data-store            | Item Repository           | List by collection                          | item.repo.test.ts > findByCollection                            | ✅ COMPLIANT |
| data-store            | Item Repository           | Search items by title LIKE                  | item.repo.test.ts > searchByText                                | ✅ COMPLIANT |
| data-store            | Item Repository           | **Search metadata content**                 | **No test; code only searches title**                           | ❌ UNTESTED  |
| data-store            | Asset Repository          | Create and list assets                      | asset.repo.test.ts (7 tests)                                    | ✅ COMPLIANT |
| data-store            | Note Repository           | Create and list notes                       | note.repo.test.ts > create+findByItem                           | ✅ COMPLIANT |
| data-store            | Note Repository           | Newest note first                           | note.repo.test.ts > sorted desc                                 | ✅ COMPLIANT |
| data-store            | DrizzleClient Type Export | Repos accept injected client                | all repo tests                                                  | ✅ COMPLIANT |
| navigation            | View Definitions          | App starts on collections                   | navigation.test.ts > starts at collections                      | ✅ COMPLIANT |
| navigation            | View Definitions          | Navigate to collection                      | navigation.test.ts > navigate adds view                         | ✅ COMPLIANT |
| navigation            | Back/Forward History      | Go back to previous view                    | navigation.test.ts > back traverses history                     | ✅ COMPLIANT |
| navigation            | Back/Forward History      | Back at root is no-op                       | navigation.test.ts > back is no-op                              | ✅ COMPLIANT |
| navigation            | Keyboard Navigation       | Escape goes back                            | keyboard.test.ts (3 tests)                                      | ✅ COMPLIANT |
| navigation            | No URL Dependency         | Plain TS no router                          | navigation.ts is pure class                                     | ✅ COMPLIANT |
| file-import           | File Picker Import        | User picks files                            | file-import.test.ts > copies selected files                     | ✅ COMPLIANT |
| file-import           | File Picker Import        | User cancels                                | file-import.test.ts > returns empty on cancel                   | ✅ COMPLIANT |
| file-import           | **Drag and Drop Import**  | **User drags supported files**              | **Not implemented**                                             | ❌ UNTESTED  |
| file-import           | **Drag and Drop Import**  | **User drags unsupported format**           | **Not implemented**                                             | ❌ UNTESTED  |
| file-import           | File Storage              | File copied to directory                    | Path uses `assets/` not `files/`                                | ⚠️ PARTIAL   |
| file-import           | File Storage              | Original filename preserved                 | UUID prefix added to filename                                   | ⚠️ PARTIAL   |
| file-import           | Asset Record Creation     | Asset record in DB                          | pickAndImportFiles doesn't write to DB; caller's responsibility | ⚠️ PARTIAL   |
| file-import           | **Duplicate Detection**   | **Duplicate filename detected**             | **Not implemented in function**                                 | ❌ UNTESTED  |
| file-import           | Import Progress           | Large file shows progress                   | Not implemented (SHOULD, not MUST)                              | ❌ UNTESTED  |
| file-import           | Tauri Capabilities        | fs:allow-appdata-\*                         | default.json has file-level perms, not appdata-scoped           | ⚠️ PARTIAL   |
| document-viewer       | Image Rendering           | Image displays inline                       | DocumentViewer.test.ts > renders img                            | ✅ COMPLIANT |
| document-viewer       | PDF Rendering             | PDF displays first page                     | DocumentViewer.test.ts > renders canvas                         | ✅ COMPLIANT |
| document-viewer       | PDF Rendering             | Navigate between pages                      | DocumentViewer.test.ts > nav controls                           | ✅ COMPLIANT |
| document-viewer       | PDF Rendering             | Zoom controls                               | DocumentViewer.test.ts > zoom buttons                           | ✅ COMPLIANT |
| document-viewer       | Loading State             | PDF shows loading                           | DocumentViewer.test.ts > shows loading state                    | ✅ COMPLIANT |
| document-viewer       | Error State               | File not found                              | Code has error state; not specifically tested                   | ⚠️ PARTIAL   |
| document-viewer       | Viewer Layout             | Side-by-side                                | ItemView.svelte has side-by-side layout                         | ✅ COMPLIANT |
| search                | Debounced Input           | 300ms debounce                              | SearchBar.test.ts (8 tests)                                     | ✅ COMPLIANT |
| search                | Search Execution          | Match item title                            | item.repo.test.ts > searchByText matches                        | ✅ COMPLIANT |
| search                | **Search Execution**      | **Match metadata content**                  | **searchByText only uses like(items.title, ...)**               | ❌ UNTESTED  |
| search                | Search Availability       | Bars on views                               | CollectionsView + CollectionView have SearchBar                 | ✅ COMPLIANT |
| export                | Export Collection to JSON | Full export + structure                     | export.test.ts > writes JSON, pretty-printed                    | ✅ COMPLIANT |
| export                | Save Dialog               | Default filename                            | export.test.ts > save called with defaultPath                   | ✅ COMPLIANT |
| export                | Save Dialog               | User cancels                                | export.test.ts > returns null                                   | ✅ COMPLIANT |
| export                | Export Excludes Binaries  | No base64                                   | exportCollectionToJson takes data: object                       | ✅ COMPLIANT |
| notes                 | Create Note               | Add note to item                            | NoteEditor + NoteRepo.create() tested                           | ✅ COMPLIANT |
| notes                 | Edit Note                 | Edit note content                           | NoteEditor onsave + NoteRepo.update() tested                    | ✅ COMPLIANT |
| notes                 | Delete Note               | Delete a note                               | NoteRepo.delete() tested                                        | ✅ COMPLIANT |
| notes                 | Note Display Order        | Newest first                                | note.repo.test.ts > findByItem sorted desc                      | ✅ COMPLIANT |
| ci                    | Test Job                  | Tests pass in CI                            | ci.yml has `test` job after lint-typecheck                      | ✅ COMPLIANT |
| ci                    | Quality Jobs              | All jobs fail fast                          | lint-typecheck → test + build (parallel)                        | ✅ COMPLIANT |
| design-system         | CollectionCard            | Renders all fields                          | CollectionCard.test.ts (7 tests)                                | ✅ COMPLIANT |
| design-system         | ItemCard                  | Shows thumbnail/placeholder                 | ItemCard.test.ts (7 tests)                                      | ✅ COMPLIANT |
| design-system         | SearchBar                 | Debounced input                             | SearchBar.test.ts (8 tests)                                     | ✅ COMPLIANT |
| design-system         | MetadataEditor            | Add/remove pairs                            | MetadataEditor.test.ts (7 tests)                                | ✅ COMPLIANT |
| design-system         | NoteEditor                | Save/cancel                                 | NoteEditor.test.ts (9 tests)                                    | ✅ COMPLIANT |
| collection-management | Create Collection         | Create with name                            | CollectionsView has create form + repo                          | ✅ COMPLIANT |
| collection-management | List Collections          | Sorted by updated_at desc                   | CollectionRepo.findAll() orderBy desc                           | ✅ COMPLIANT |
| collection-management | Delete Collection         | Confirm before delete                       | CollectionView has confirm flow                                 | ✅ COMPLIANT |
| desktop-app           | App Launch                | App starts on collections view              | App.svelte routes to CollectionsView                            | ✅ COMPLIANT |
| desktop-app           | Layout Shell              | AppShell + TopBar                           | AppShell.svelte + TopBar.svelte created                         | ✅ COMPLIANT |

**Compliance summary**: ~47/62 sampled COMPLIANT | 5 UNTESTED | 5 PARTIAL

---

## Correctness (Static — Structural Evidence)

| Requirement                                                           | Status         | Notes                                                                         |
| --------------------------------------------------------------------- | -------------- | ----------------------------------------------------------------------------- |
| CollectionRepo: create/findAll/findById/update/delete/countItems      | ✅ Implemented | Full API, constructor DI                                                      |
| ItemRepo: create/findByCollection/findById/update/delete/searchByText | ⚠️ Partial     | searchByText searches only `title`, NOT `metadata`                            |
| AssetRepo: create/findByItem/findById/delete                          | ✅ Implemented |                                                                               |
| NoteRepo: create/findByItem/update/delete                             | ✅ Implemented | findByItem orders by createdAt DESC                                           |
| StoreApi interface + initStore factory                                | ✅ Implemented | store.ts + db.ts wires correctly                                              |
| NavigationStore with history stack                                    | ✅ Implemented | Plain TS class (ADR-008)                                                      |
| File import via dialog + fs plugins                                   | ⚠️ Partial     | Picker + copy work; no drag-drop, no duplicate check                          |
| DocumentViewer with pdfjs-dist                                        | ✅ Implemented | Zoom, page nav, lazy-load, error state                                        |
| Export to JSON via save dialog                                        | ⚠️ Partial     | Save + write implemented; data assembly is caller's responsibility            |
| SearchBar with 300ms debounce                                         | ✅ Implemented |                                                                               |
| MetadataEditor key-value serialize                                    | ✅ Implemented | Serializes to JSON                                                            |
| CI test job                                                           | ✅ Implemented |                                                                               |
| Tauri plugins Phase 6                                                 | ✅ Implemented | Cargo.toml + lib.rs + default.json                                            |
| 0002 migration (indexes)                                              | ✅ Implemented | Named `0002_metadata_search`; includes indexes + search_text generated column |
| tasks.md updated                                                      | ❌ Missing     | Stale — needs manual update before archive                                    |

---

## Coherence (Design)

| Decision                                      | Followed?          | Notes                                                                                                            |
| --------------------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------- |
| ADR-005: LIKE search on title+metadata        | ⚠️ Partial         | 0002 migration creates `search_text` (title + metadata), but item.repo.ts uses `like(items.title, ...)` directly |
| ADR-006: Repository classes in packages/store | ✅ Yes             | All 4 repos accept DrizzleClient via constructor                                                                 |
| ADR-007: pdfjs-dist in @entropia/ui           | ✅ Yes             | Lazy `import('pdfjs-dist')` in DocumentViewer                                                                    |
| ADR-008: NavigationStore plain TS (not runes) | ✅ Followed        | Documented deviation from spec; improves testability                                                             |
| Storage path `files/{coll}/{item}/`           | ⚠️ Deviated        | Code uses `assets/{coll}/{item}/`                                                                                |
| Original filename preserved                   | ⚠️ Deviated        | Code prefixes with UUID to avoid collisions (`{uuid}_{name}`)                                                    |
| Drag & Drop via onDragDropEvent               | ❌ Not implemented | MUST requirement in file-import spec                                                                             |
| Export: gather collection+items+assets+notes  | ⚠️ Partial         | export.ts only handles save+write; data assembly in CollectionView                                               |

---

## Issues Found

### CRITICAL (must fix before archive)

1. **`searchByText` only searches `items.title` — metadata not searched**
   - File: `packages/store/src/repos/item.repo.ts:58`
   - Spec: "Search MUST query `items.title` AND `items.metadata` using SQL LIKE patterns"
   - Migration `0002_metadata_search` created `search_text` generated column = `title || ' ' || json(metadata)`, but repo ignores it
   - Fix: change `like(items.title, ...)` to use `items.searchText` column, OR add `or(like(items.title,...), like(items.metadata,...))`
   - **All 112 tests still pass** because mock tests don't validate which column is queried

2. **Drag & Drop Import not implemented**
   - Spec: `Requirement: Drag and Drop Import` (MUST — 2 scenarios)
   - No `onDragDropEvent` usage anywhere in the codebase
   - Decision needed: implement now or formally defer to Fase 2

3. **tasks.md on disk is stale (21/47 checked, should be ~44/47)**
   - File: `openspec/changes/fase-1-mvp-documental/tasks.md`
   - Administrative blocker — archive phase needs accurate task file
   - Fix: update checkboxes for Phases 2, 3 (partially done), 4, 5, 6, 7, 8

### WARNING (should fix before archive)

4. **Storage path `assets/` instead of `files/`**
   - File: `apps/desktop/src/lib/file-import.ts:49`
   - Spec says: `{appDataDir}/files/{collection_id}/{item_id}/`
   - Code uses: `{appDataDir}/assets/{collection_id}/{item_id}/`

5. **UUID prefix on imported filename breaks original filename preservation**
   - File: `apps/desktop/src/lib/file-import.ts:59`
   - Spec says: "The original filename MUST be preserved"
   - Code saves: `{uuid}_{filename}` on disk; originalName stored separately in `ImportedFile` struct

6. **Duplicate detection missing from `pickAndImportFiles`**
   - Spec: "Duplicate Detection — MUST detect when filename already exists" (MUST)
   - Not implemented in the function; no test covers it

7. **Tauri capabilities may be insufficient for appDataDir access**
   - `default.json` has `fs:allow-read-file`, `fs:allow-copy-file`, `fs:allow-write-file`, `fs:allow-create-dir`
   - Spec requires: `fs:allow-appdata-read-recursive`, `fs:allow-appdata-write-recursive`
   - The current permissions may not grant access to `appDataDir` specifically in all Tauri versions

### SUGGESTION (nice to have)

- Update tasks.md before archive to correctly reflect status
- Add test asserting that `searchByText` also searches metadata content
- Add DocumentViewer test for error state (file not found scenario)
- Add duplicate detection logic into `pickAndImportFiles`
- Decide on drag-drop: add `// NOTE: Drag & Drop deferred to Fase 2` comment in ADR or implement

---

## Verdict

### **PASS WITH WARNINGS**

**Score**: 112/112 tests ✅ | 0 typecheck errors ✅ | 0 lint errors ✅ | ~76% scenarios COMPLIANT

**Summary**: The core MVP architecture is solid and fully functional. All 8 implementation phases have real code in the codebase. Tests pass, types are clean, and CI is configured. The blocking issues are: (1) search only covers title not metadata — a 1-line fix that changes which column is queried; (2) drag-drop was not implemented and needs an explicit decision; (3) tasks.md file needs updating for archive integrity.

**Recommendation**: Fix CRITICALs #1 and #3 before archiving. For CRITICAL #2 (drag-drop), either implement or explicitly defer to Fase 2 via an ADR note. The WARNINGs (storage path, filename, duplicate detection, capabilities) should be resolved before release but are acceptable for MVP development.

**Proceed to archive?**: ❌ NO — fix 3 CRITICALs first
