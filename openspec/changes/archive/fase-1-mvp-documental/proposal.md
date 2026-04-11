# Proposal: Fase 1 — MVP Documental

## Intent

Enable a historian to import, organize, annotate, and search a collection of 100+ historical documents (images/PDFs) without any AI features. This is the first user-facing phase — Fase 0 laid infrastructure; Fase 1 delivers the core document management workflow.

## Scope

### In Scope

- Image/PDF import (drag & drop + file picker) with native file copy to `appDataDir`
- Collection CRUD (create, read, update, delete)
- Document viewer (images via `convertFileSrc()`, PDFs via `pdfjs-dist`)
- Editable metadata per item (JSON key-value editor)
- Notes per item (create, edit, delete)
- Full-text search via SQLite FTS5 on item title + metadata
- JSON export of collections
- Store-based client-side router (Svelte 5 `$state`)
- Vitest + `@testing-library/svelte` setup — Strict TDD activates
- Repository pattern in `@entropia/store`

### Out of Scope

- AI/ML processing (OCR, NER, embeddings, knowledge graph) — Fase 2+
- Cloud sync / multi-user — Fase 4+
- Tagging / taxonomy system — Fase 2
- Thumbnails / image optimization — deferred
- Platform-specific builds (macOS/Windows installers) — Fase 3
- Batch operations on items

## Capabilities

### New Capabilities

- `file-import`: Drag & drop + file picker import, copies files to structured `appDataDir` path, creates item + asset records
- `collection-management`: Full CRUD for collections with validation
- `document-viewer`: Image rendering via `convertFileSrc()` + PDF rendering via `pdfjs-dist` (offline)
- `item-management`: Item CRUD, editable metadata (JSON key-value), sort/filter within collection
- `notes`: Create/edit/delete text notes attached to items
- `search`: FTS5 virtual table on `items(title, metadata)`, exposed via IPC or `db_select`
- `export`: JSON export of collection with items, metadata, and notes
- `navigation`: Store-based router with `$state` — views: collections list, collection detail, item detail
- `testing-infrastructure`: Vitest + `@testing-library/svelte` + Tauri invoke mocking

### Modified Capabilities

- `data-store`: New repositories (`CollectionRepo`, `ItemRepo`, `AssetRepo`, `NoteRepo`), new migration for FTS5 virtual table
- `desktop-app`: Replace placeholder App.svelte with router + layout shell
- `design-system`: New components (CollectionCard, ItemCard, DocumentViewer, SearchBar, MetadataEditor, NoteEditor, Modal, FileDropZone)
- `ci`: Add `test` job to pipeline (Vitest now available)

## Approach

**File storage**: `@tauri-apps/plugin-fs` + `@tauri-apps/plugin-dialog` for native file access. Files copied to `appDataDir/files/{collection_id}/{item_id}/original.{ext}`. Asset records store relative paths. `convertFileSrc()` converts native paths to webview-safe URLs.

**Data access**: Repository pattern in `packages/store` — each repo wraps Drizzle queries for a domain entity. Components call repos; repos call the Drizzle client which proxies through IPC.

**Navigation**: Hand-rolled router using Svelte 5 `$state` rune. A `router.svelte.ts` module exports reactive `currentRoute` state. No external router dependency — keeps the bundle small and avoids SvelteKit.

**PDF viewer**: `pdfjs-dist` (Mozilla) bundled in `packages/ui` as a `DocumentViewer` component. Works fully offline. Canvas-based rendering.

**Search**: New migration creates FTS5 virtual table + triggers to keep it in sync with `items`. Search queries go through the existing `db_select` IPC command.

**Testing**: Vitest as root devDependency with per-package configs. `@testing-library/svelte` for component tests. Tauri `invoke` mocked via `vi.mock`. Strict TDD enabled — tests before implementation.

## Affected Areas

| Area                                    | Impact   | Description                                                                                          |
| --------------------------------------- | -------- | ---------------------------------------------------------------------------------------------------- |
| `packages/store/src/repos/`             | New      | `CollectionRepo`, `ItemRepo`, `AssetRepo`, `NoteRepo`                                                |
| `packages/store/src/migrations/`        | New      | `0002_fts5.sql` — FTS5 virtual table + sync triggers                                                 |
| `packages/store/src/index.ts`           | Modified | Export new repos                                                                                     |
| `packages/ui/src/components/`           | New      | CollectionCard, ItemCard, DocumentViewer, SearchBar, MetadataEditor, NoteEditor, Modal, FileDropZone |
| `packages/ui/src/index.ts`              | Modified | Export new components                                                                                |
| `apps/desktop/src/lib/router.svelte.ts` | New      | Store-based router                                                                                   |
| `apps/desktop/src/lib/services/`        | New      | `fileImport.ts`, `exportService.ts`                                                                  |
| `apps/desktop/src/views/`               | New      | CollectionList, CollectionDetail, ItemDetail                                                         |
| `apps/desktop/src/App.svelte`           | Modified | Router integration + layout shell                                                                    |
| `apps/desktop/src-tauri/`               | Modified | Add `fs` + `dialog` plugin permissions                                                               |
| `vitest.config.ts` (root + packages)    | New      | Test infrastructure                                                                                  |
| `.github/workflows/ci.yml`              | Modified | Add test step                                                                                        |

## Risks

| Risk                                                  | Likelihood | Mitigation                                                       |
| ----------------------------------------------------- | ---------- | ---------------------------------------------------------------- |
| `pdfjs-dist` bundle size bloats desktop app           | Med        | Tree-shake, lazy-load PDF viewer component, measure bundle       |
| FTS5 sync triggers degrade write performance at scale | Low        | Benchmark with 1000+ items; triggers are per-row, not per-query  |
| `convertFileSrc()` path handling differs across OS    | Med        | Test on Windows early; use `path.join` from Tauri plugin-path    |
| Drag & drop API inconsistencies across platforms      | Med        | Use Tauri's native drag-drop event, not web API; test on Windows |
| Large PDF files (100MB+) may freeze the viewer        | Low        | Render pages lazily (one at a time); add loading indicator       |
| Mocking Tauri `invoke` in Vitest may be brittle       | Med        | Create a thin `ipc.ts` abstraction that's easy to mock           |

## Rollback Plan

All changes are additive — no existing Fase 0 code is deleted. Rollback strategy:

1. **Git**: Single feature branch `feat/fase-1-mvp-documental` — revert the merge commit
2. **Database**: New FTS5 migration is forward-only, but the virtual table is isolated — drop it in a new migration if needed
3. **Dependencies**: `pdfjs-dist`, `vitest`, `@testing-library/svelte` are devDependencies or UI deps — remove from `package.json` and run `pnpm install`
4. **Tauri plugins**: Remove `fs` + `dialog` from `capabilities` in `tauri.conf.json`

## Dependencies

- `@tauri-apps/plugin-fs` — native filesystem access
- `@tauri-apps/plugin-dialog` — native file picker dialog
- `@tauri-apps/plugin-path` — cross-platform path resolution (may already be available)
- `pdfjs-dist` — Mozilla's PDF renderer (offline, no CDN)
- `vitest` — test runner
- `@testing-library/svelte` — component testing utilities
- `jsdom` or `happy-dom` — DOM environment for Vitest

## Success Criteria

- [ ] A user can create, rename, and delete a collection
- [ ] A user can import images and PDFs via drag & drop and file picker
- [ ] Imported files are copied to `appDataDir/files/{collection_id}/{item_id}/`
- [ ] A user can view images and PDFs inline in the document viewer
- [ ] A user can edit metadata (key-value pairs) on any item
- [ ] A user can create, edit, and delete notes on any item
- [ ] A user can search items by title or metadata text and see results
- [ ] A user can export a collection as JSON (items + metadata + notes)
- [ ] Navigation between collections list, collection detail, and item detail works
- [ ] Vitest runs with `pnpm test` across all packages
- [ ] All new code has test coverage (repos, components, services)
- [ ] CI pipeline passes with lint + typecheck + test
- [ ] App handles 100+ documents in a single collection without degradation
