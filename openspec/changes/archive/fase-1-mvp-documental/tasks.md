# Tasks: Fase 1 — MVP Documental

## Phase 1: Vitest Infrastructure [FIRST — activates Strict TDD]

- [x] 1.1 Add `vitest`, `@vitest/coverage-v8`, `happy-dom`, `@testing-library/svelte` as root devDeps in `package.json`
  - Satisfies: testing-infrastructure/Monorepo Test Runner, Coverage Reporting
- [x] 1.2 Create `vitest.workspace.ts` at repo root — projects: `['packages/*', 'apps/desktop']`
  - Satisfies: testing-infrastructure/Monorepo Test Runner (run all tests from root)
- [x] 1.3 Create `packages/store/vitest.config.ts` — environment: `node`, globals, coverage-v8
  - Satisfies: testing-infrastructure/Per-Package Configuration (store=node)
- [x] 1.4 Create `packages/ui/vitest.config.ts` — environment: `happy-dom`, svelte plugin, coverage-v8
  - Satisfies: testing-infrastructure/Per-Package Configuration (ui=happy-dom)
- [x] 1.5 Create `apps/desktop/vitest.config.ts` — environment: `happy-dom`, svelte plugin, Tauri module mocks via `vi.mock`
  - Satisfies: testing-infrastructure/Per-Package Configuration (desktop=happy-dom+Tauri mocks)
- [x] 1.6 Create `packages/store/src/__mocks__/db.ts` — mock `DbClient` with configurable `execute`/`select` stubs
  - Satisfies: testing-infrastructure/Store Test Mocking Strategy
- [x] 1.7 Add `"test": "vitest run"` script to `packages/store/package.json`, `packages/ui/package.json`, `apps/desktop/package.json`
  - Satisfies: testing-infrastructure/Monorepo Test Runner (single package)
- [x] 1.8 Verify `turbo.json` `test` task config is correct (already scaffolded — confirm `dependsOn: ["^build"]`, `outputs: ["coverage/**"]`)
  - Satisfies: testing-infrastructure/Turborepo Integration
- [x] 1.9 Write smoke test `packages/store/src/__tests__/setup.test.ts` — verify mock DbClient works, `vitest run` passes
  - Satisfies: testing-infrastructure/Store Test Mocking Strategy (mock returns configured data)

## Phase 2: packages/store — Repos + Migration [parallelizable with Phase 3]

- [x] 2.1 Add `0002_indexes` migration to `MIGRATIONS` in `packages/store/src/runner.ts`: indexes on `items.collection_id`, `assets.item_id`, `notes.item_id`
  - Satisfies: data-store/design (0002_indexes migration)
- [x] 2.2 Create `packages/store/src/db.ts` — `DrizzleClient` type, `StoreApi` interface, `initStore(client)` factory
  - Satisfies: data-store/DrizzleClient Type Export, design/Interfaces
- [x] 2.3 Create `packages/store/src/repos/collection.repo.ts` — `CollectionRepo` class: `create`, `findAll`, `findById`, `update`, `delete`, `countItems`
  - Satisfies: data-store/Collection Repository
- [x] 2.4 Write tests `packages/store/src/repos/__tests__/collection.repo.test.ts` — CRUD + countItems using mock DbClient
  - Satisfies: data-store/Collection Repository scenarios (create+retrieve, count items)
- [x] 2.5 Create `packages/store/src/repos/item.repo.ts` — `ItemRepo` class: `create`, `findByCollection(id, {limit, offset})`, `findById`, `update`, `delete`, `searchByText(term, collectionId?)`
  - Satisfies: data-store/Item Repository, search/Search Execution (LIKE on title+metadata)
- [x] 2.6 Write tests `packages/store/src/repos/__tests__/item.repo.test.ts` — CRUD + pagination + LIKE search on title and metadata
  - Satisfies: data-store/Item Repository scenarios, search/Search Execution scenarios
- [x] 2.7 Create `packages/store/src/repos/asset.repo.ts` — `AssetRepo` class: `create`, `findByItem`, `delete`
  - Satisfies: data-store/Asset Repository
- [x] 2.8 Write tests `packages/store/src/repos/__tests__/asset.repo.test.ts` — create + findByItem using mock
  - Satisfies: data-store/Asset Repository scenario
- [x] 2.9 Create `packages/store/src/repos/note.repo.ts` — `NoteRepo` class: `create`, `findByItem`, `update`, `delete`
  - Satisfies: data-store/Note Repository, notes/Note Display Order (order by created_at DESC)
- [x] 2.10 Write tests `packages/store/src/repos/__tests__/note.repo.test.ts` — CRUD + ordering using mock
  - Satisfies: data-store/Note Repository scenario, notes/Newest note appears first
- [x] 2.11 Create `packages/store/src/repos/index.ts` — barrel export all repos
  - Satisfies: design/File Changes (repos barrel)
- [x] 2.12 Update `packages/store/src/index.ts` — export repos, `StoreApi`, `initStore`, `DrizzleClient` type
  - Satisfies: data-store/DrizzleClient Type Export (repos accept injected client)

## Phase 3: packages/ui — New Components [parallelizable with Phase 2]

- [x] 3.1 Create `packages/ui/src/components/CollectionCard/` — props: name, description, itemCount, updatedAt; renders Card with badge + date
  - Satisfies: design-system/CollectionCard Component
- [x] 3.2 Write test `packages/ui/src/components/CollectionCard/__tests__/CollectionCard.test.ts` — renders all fields
  - Satisfies: design-system/CollectionCard renders all fields scenario
- [x] 3.3 Create `packages/ui/src/components/ItemCard/` — props: title, thumbnailUrl?, metadataPreview?; shows thumbnail or placeholder
  - Satisfies: design-system/ItemCard Component
- [x] 3.4 Create `packages/ui/src/components/SearchBar/` — debounced input (300ms), clear button, emits `onsearch` event
  - Satisfies: design-system/SearchBar Component, search/Debounced Input
- [x] 3.5 Write test `packages/ui/src/components/SearchBar/__tests__/SearchBar.test.ts` — debounce fires after 300ms, clear resets
  - Satisfies: search/Rapid typing triggers single query scenario
- [x] 3.6 Create `packages/ui/src/components/MetadataEditor/` — key-value rows, add/remove, serializes to JSON
  - Satisfies: design-system/MetadataEditor Component, item-management/Edit Item Metadata
- [x] 3.7 Write test `packages/ui/src/components/MetadataEditor/__tests__/MetadataEditor.test.ts` — add pair, remove pair
  - Satisfies: design-system/MetadataEditor add+remove scenarios
- [x] 3.8 Create `packages/ui/src/components/NoteEditor/` — textarea + save/cancel buttons, emits `onsave` with content
  - Satisfies: notes/Create Note, notes/Edit Note
- [x] 3.9 Create `packages/ui/src/components/DocumentViewer/` — pdfjs-dist canvas for PDF, `<img>` for images, page nav, zoom, loading/error states
  - Satisfies: design-system/DocumentViewer, document-viewer/Image Rendering, PDF Rendering, Loading State, Error State
- [x] 3.10 Modify `packages/ui/vite.config.ts` — add `optimizeDeps: { include: ['pdfjs-dist'] }`
  - Satisfies: design/File Changes (optimizeDeps)
- [x] 3.11 Add `pdfjs-dist` as dependency in `packages/ui/package.json`
  - Satisfies: document-viewer/PDF Rendering (pdfjs-dist)
- [x] 3.12 Update `packages/ui/src/index.ts` — export CollectionCard, ItemCard, DocumentViewer, SearchBar, MetadataEditor, NoteEditor
  - Satisfies: design/File Changes (export 6 new components)

## Phase 4: apps/desktop — Navigation + Layout [depends on Phase 2+3]

- [x] 4.1 Create `apps/desktop/src/lib/navigation.svelte.ts` — `NavigationStore` class: `$state` history stack, `$derived current`, `canGoBack`, `navigate(view)`, `back()`
  - Satisfies: navigation/View Definitions, Back/Forward History, No URL Dependency
  - Note: implemented as plain TS class (not runes) for Vitest testability — see ADR-008
- [x] 4.2 Write test `apps/desktop/src/__tests__/navigation.test.ts` — navigate, back, back-at-root noop, history stack
  - Satisfies: navigation/all 7 scenarios
- [x] 4.3 Create `apps/desktop/src/lib/file-import.ts` — `importFiles(itemId, collectionId)` via dialog+fs plugins, `getAssetUrl(path)` via convertFileSrc, duplicate detection
  - Satisfies: file-import/File Picker, File Storage, Asset Record Creation
- [x] 4.4 Write test `apps/desktop/src/__tests__/file-import.test.ts` — mocked dialog+fs, path construction
  - Satisfies: file-import/File copied to structured directory scenario
- [x] 4.5 Create `apps/desktop/src/lib/export.ts` — `exportCollection(collectionId)`: gather collection+items+assets+notes, save dialog, write JSON (no binaries)
  - Satisfies: export/Export Collection to JSON, Save Dialog, Export Excludes Binaries
- [x] 4.6 Modify `apps/desktop/src/lib/db.ts` — use `initStore()` from `@entropia/store`, export `getStore(): StoreApi`
  - Satisfies: desktop-app/design (db.ts uses initStore)
- [x] 4.7 Create `apps/desktop/src/layout/AppShell.svelte` — sidebar + main content area slot
  - Satisfies: desktop-app/Layout Shell
- [x] 4.8 Create `apps/desktop/src/layout/TopBar.svelte` — breadcrumb from NavigationStore, back button, search bar, action buttons
  - Satisfies: desktop-app/Layout Shell (back button visible), search/Search Availability
- [x] 4.9 Modify `apps/desktop/src/App.svelte` — replace Fase 0 placeholder: init NavigationStore, route via `navigation.current`, wrap in AppShell+TopBar, bind Escape key for back
  - Satisfies: desktop-app/Router Integration, App Launch (renders collections as default), navigation/Keyboard Navigation
- [x] 4.10 Update `apps/desktop/package.json` — add `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-fs`, `pdfjs-dist` deps
  - Satisfies: file-import/Tauri Capabilities, design/File Changes

## Phase 5: apps/desktop — Views [depends on Phase 4]

- [x] 5.1 Create `apps/desktop/src/views/CollectionsView.svelte` — collection grid via CollectionCard, create form (name+desc), search bar, empty state
  - Satisfies: collection-management/Create Collection, List Collections, empty state scenario
- [x] 5.2 Create `apps/desktop/src/views/CollectionView.svelte` — items grid via ItemCard, create item form, search bar, import button, export button, pagination, delete collection, empty state
  - Satisfies: item-management/List Items, Create Item, search/Search Scope (scoped), collection-management/Delete Collection
- [x] 5.3 Create `apps/desktop/src/views/ItemView.svelte` — side-by-side: DocumentViewer (left) + MetadataEditor + NoteEditor + notes list (right), title editing, delete item, import files
  - Satisfies: document-viewer/Viewer Layout (side-by-side), item-management/Edit Item Metadata, notes/Create+Edit+Delete Note, file-import/File Picker Import

## Phase 6: Tauri Rust — Capabilities + Plugins [parallelizable with Phase 4+5]

- [x] 6.1 Modify `apps/desktop/src-tauri/capabilities/default.json` — add `dialog:allow-open`, `dialog:allow-save`, `fs:allow-copy-file`, `fs:allow-create-dir`, `fs:allow-read-file`, `fs:allow-write-file`, `fs:allow-exists`
  - Satisfies: file-import/Tauri Capabilities, export/Save Dialog
- [x] 6.2 Modify `apps/desktop/src-tauri/Cargo.toml` — add `tauri-plugin-dialog`, `tauri-plugin-fs` dependencies
  - Satisfies: file-import/Tauri Capabilities (Rust side)
- [x] 6.3 Modify `apps/desktop/src-tauri/src/lib.rs` — register `.plugin(tauri_plugin_dialog::init())`, `.plugin(tauri_plugin_fs::init())`
  - Satisfies: file-import/Tauri Capabilities (plugin registration)

## Phase 7: CI Update

- [x] 7.1 Modify `.github/workflows/ci.yml` — add `test` job after `lint-typecheck`: checkout, pnpm, node, install, `pnpm test --run`
  - Satisfies: ci/Test Job, ci/Quality Jobs (lint+typecheck+test all fail-fast)

## Phase 8: Integration & Validation

- [x] 8.1 Run `pnpm install` — all new deps resolve
- [x] 8.2 Run `pnpm test --run` — all unit+component tests pass (113/113 after metadata-search fix)
  - Satisfies: testing-infrastructure/Run all tests from root scenario
- [x] 8.3 Run `pnpm typecheck` — 0 errors across all packages
- [x] 8.4 Run `pnpm lint` — 0 errors across all packages
- [ ] 8.5 Manual: `pnpm tauri dev` — app launches, collections CRUD works, file import works, PDF renders, search returns results
  - Satisfies: desktop-app/App Launch, document-viewer/PDF displays first page, file-import/User picks files
  - Note: manual validation deferred — automated tests confirm all units pass
