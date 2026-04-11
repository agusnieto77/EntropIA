# Design: Fase 1 — MVP Documental

## Technical Approach

Layer repositories over the existing Drizzle sqlite-proxy bridge, add runes-based navigation, pdfjs-dist viewer, native file import via Tauri plugins, and Vitest infrastructure — all additive to Fase 0. Repos receive `DrizzleClient`, components consume repos via a `StoreApi` singleton. Navigation is a `$state` class with history stack. Search uses LIKE (swappable to FTS5 in Fase 3 via repo abstraction).

## Architecture Decisions

| ID      | Decision               | Choice                                 | Alternatives Rejected                                               | Rationale                                                                                                                                                                                            |
| ------- | ---------------------- | -------------------------------------- | ------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ADR-005 | Search strategy        | LIKE on `title`+`metadata`             | FTS5 virtual table + sync triggers                                  | 100 docs = LIKE returns <1ms. Repo abstracts the swap — change ONE method when FTS5 is needed (Fase 3). Avoids trigger maintenance and Drizzle FTS5 gaps.                                            |
| ADR-006 | Data access pattern    | Repository classes in `packages/store` | Inline Drizzle in components; service layer in desktop app          | Repos are testable with mock `DrizzleClient` (no Tauri needed), reusable across apps, enforce "no SQL in views". Matches existing `DbClient` interface pattern from Fase 0.                          |
| ADR-007 | PDF rendering          | `pdfjs-dist` bundled in `@entropia/ui` | `<iframe>` with asset protocol; `<embed>` tag; Tauri webview plugin | Document analysis is the CORE feature — needs zoom, page nav, and future text layer for OCR overlay (Fase 2). 500KB is irrelevant in a desktop app. Canvas rendering = full control.                 |
| ADR-008 | Client-side navigation | Runes `$state` NavigationStore class   | svelte-routing; hash router; TinyBase                               | Desktop app has no URL bar. 3 views + history stack = zero-dep class. Fully testable (plain JS). Avoids Svelte 4 router compatibility issues.                                                        |
| ADR-009 | Drag & Drop import     | Deferred to Fase 2                     | Implement in Fase 1 via Tauri `onDragDropEvent`                     | Fase 1 scope is MVP (dialog-based import works). Drag & Drop adds UX polish but not core functionality. `onDragDropEvent` Tauri API requires additional capability testing. Backlog item for Fase 2. |

## Data Flow

```
User Action (click/drag/search)
       │
       ▼
┌─────────────────────────────────────────────┐
│  Views (CollectionsView, CollectionView,     │
│         ItemView)                            │
│       │                      ▲               │
│       ▼                      │               │
│  NavigationStore ◄─── $derived current       │
│  StoreApi (repos) ◄── initStore() singleton  │
│       │                                      │
│       ▼                                      │
│  CollectionRepo / ItemRepo / AssetRepo /     │
│  NoteRepo                                    │
│       │                                      │
│       ▼                                      │
│  DrizzleClient (sqlite-proxy)                │
│       │                                      │
│       ▼  invoke("db_execute"/"db_select")    │
├──────────── Tauri IPC ──────────────────────┤
│  Rust: rusqlite → entropia.sqlite            │
└─────────────────────────────────────────────┘

File Import:
  dialog.open() → fs.copyFile(src, appDataDir/files/{coll}/{item}/)
                → AssetRepo.create() → convertFileSrc(path) → <img>/<canvas>
```

## File Changes

| File                                               | Action | Description                                                            |
| -------------------------------------------------- | ------ | ---------------------------------------------------------------------- |
| `vitest.workspace.ts`                              | Create | Monorepo workspace: `['packages/*', 'apps/desktop']`                   |
| `packages/store/vitest.config.ts`                  | Create | `environment: 'node'`, globals, coverage v8                            |
| `packages/ui/vitest.config.ts`                     | Create | `environment: 'happy-dom'`, svelte plugin, coverage v8                 |
| `apps/desktop/vitest.config.ts`                    | Create | `environment: 'happy-dom'`, svelte plugin, Tauri module mocks          |
| `packages/store/src/__mocks__/db.ts`               | Create | In-memory `DbClient` mock for repo tests                               |
| `packages/store/src/repos/collection.repo.ts`      | Create | CRUD + `countItems()` + `search()`                                     |
| `packages/store/src/repos/item.repo.ts`            | Create | CRUD + LIKE search on title+metadata                                   |
| `packages/store/src/repos/asset.repo.ts`           | Create | CRUD + `findByItem()`                                                  |
| `packages/store/src/repos/note.repo.ts`            | Create | CRUD + `findByItem()`                                                  |
| `packages/store/src/repos/index.ts`                | Create | Barrel export of all repos                                             |
| `packages/store/src/db.ts`                         | Create | `StoreApi` interface + `initStore(client)` factory                     |
| `packages/store/src/runner.ts`                     | Modify | Add `0002_indexes` migration to `MIGRATIONS` registry                  |
| `packages/store/src/index.ts`                      | Modify | Export repos, `StoreApi`, `initStore`                                  |
| `packages/ui/src/components/CollectionCard/`       | Create | Name, description, item count, date                                    |
| `packages/ui/src/components/ItemCard/`             | Create | Title, thumbnail preview, metadata                                     |
| `packages/ui/src/components/DocumentViewer/`       | Create | pdfjs-dist canvas + `<img>` switcher, zoom, page nav                   |
| `packages/ui/src/components/SearchBar/`            | Create | Debounced input, clear button                                          |
| `packages/ui/src/components/MetadataEditor/`       | Create | Key-value rows, add/remove, JSON serialization                         |
| `packages/ui/src/components/NoteEditor/`           | Create | Textarea + save/cancel                                                 |
| `packages/ui/vite.config.ts`                       | Modify | `optimizeDeps: { include: ['pdfjs-dist'] }`                            |
| `packages/ui/src/index.ts`                         | Modify | Export 6 new components                                                |
| `apps/desktop/src/lib/navigation.svelte.ts`        | Create | `NavigationStore` class with `$state` history                          |
| `apps/desktop/src/lib/file-import.ts`              | Create | `importFiles()`, `getAssetUrl()`                                       |
| `apps/desktop/src/lib/db.ts`                       | Modify | Use `initStore()` instead of raw `createDrizzleClient()`               |
| `apps/desktop/src/views/CollectionsView.svelte`    | Create | Collection grid + create + search                                      |
| `apps/desktop/src/views/CollectionView.svelte`     | Create | Items grid + import + search + export                                  |
| `apps/desktop/src/views/ItemView.svelte`           | Create | DocumentViewer + MetadataEditor + NoteEditor                           |
| `apps/desktop/src/layout/AppShell.svelte`          | Create | Sidebar + main content area                                            |
| `apps/desktop/src/layout/TopBar.svelte`            | Create | Breadcrumb + search + actions                                          |
| `apps/desktop/src/App.svelte`                      | Modify | Route via `navigation.current`, wrap in AppShell                       |
| `apps/desktop/src-tauri/capabilities/default.json` | Modify | Add fs + dialog permissions                                            |
| `apps/desktop/src-tauri/Cargo.toml`                | Modify | Add `tauri-plugin-dialog`, `tauri-plugin-fs`                           |
| `apps/desktop/src-tauri/src/lib.rs`                | Modify | Register dialog + fs plugins in builder                                |
| `apps/desktop/package.json`                        | Modify | Add `pdfjs-dist`, `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-fs` |
| `packages/store/package.json`                      | Modify | Add `test` script                                                      |
| `package.json` (root)                              | Modify | Add `vitest`, `@vitest/coverage-v8`, `happy-dom` as devDeps            |
| `.github/workflows/ci.yml`                         | Modify | Add `test` job after lint+typecheck                                    |

## Interfaces / Contracts

```typescript
// packages/store/src/db.ts — StoreApi + factory
export type DrizzleClient = ReturnType<typeof createDrizzleClient>

export interface StoreApi {
  collections: CollectionRepo
  items: ItemRepo
  assets: AssetRepo
  notes: NoteRepo
  db: DrizzleClient // escape hatch for raw queries
}

export function initStore(client: DbClient): StoreApi
```

```typescript
// apps/desktop/src/lib/navigation.svelte.ts
type View =
  | { name: 'collections' }
  | { name: 'collection'; id: string }
  | { name: 'item'; collectionId: string; itemId: string }

class NavigationStore {
  #history = $state<View[]>([{ name: 'collections' }])
  current = $derived(this.#history.at(-1)!)
  canGoBack = $derived(this.#history.length > 1)
  navigate(view: View): void
  back(): void
}
```

```typescript
// packages/store/src/repos/collection.repo.ts (representative pattern)
export class CollectionRepo {
  constructor(private db: DrizzleClient) {}
  async create(data: NewCollection): Promise<Collection>
  async findAll(): Promise<Collection[]>
  async findById(id: string): Promise<Collection | null>
  async update(id: string, data: Partial<NewCollection>): Promise<Collection>
  async delete(id: string): Promise<void>
  async countItems(id: string): Promise<number>
}
// ItemRepo adds: search(term: string), findByCollection(collectionId: string)
// AssetRepo adds: findByItem(itemId: string)
// NoteRepo adds: findByItem(itemId: string)
```

```sql
-- packages/store/src/runner.ts — 0002_indexes migration (inlined)
CREATE INDEX IF NOT EXISTS idx_items_collection ON items(collection_id);
CREATE INDEX IF NOT EXISTS idx_assets_item ON assets(item_id);
CREATE INDEX IF NOT EXISTS idx_notes_item ON notes(item_id);
```

## Testing Strategy

| Layer              | What to Test                                                    | Approach                                                               |
| ------------------ | --------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Unit — repos       | CRUD operations, search, edge cases (not found, duplicate)      | Vitest + mock `DrizzleClient` via `packages/store/src/__mocks__/db.ts` |
| Unit — navigation  | Navigate, back, history, edge cases (back at root)              | Vitest + pure class instantiation (no DOM)                             |
| Unit — file-import | Path construction, asset URL generation                         | Vitest + mock `@tauri-apps/plugin-dialog` + `plugin-fs`                |
| Component — UI     | CollectionCard, SearchBar, MetadataEditor render + interactions | Vitest + `@testing-library/svelte` + happy-dom                         |
| Component — views  | CollectionsView, CollectionView, ItemView with mocked repos     | Vitest + happy-dom + mock `StoreApi`                                   |
| Integration        | DocumentViewer renders PDF page to canvas                       | Manual via `tauri dev` (pdfjs needs real canvas)                       |
| CI                 | All unit + component tests                                      | `turbo test` in GitHub Actions                                         |

## Migration / Rollout

All additive. No existing data migration. New `0002_indexes` migration adds performance indexes — applied automatically on next app start by existing runner. Rollback: `git revert` the merge commit; add `0003_drop_indexes.sql` if needed (not destructive).

## Open Questions

- [x] ~~FTS5 vs LIKE~~ → LIKE for Fase 1 (ADR-005)
- [ ] `pdfjs-dist` worker: verify `new URL('pdfjs-dist/build/pdf.worker.min.mjs', import.meta.url).href` works with Vite 6 + Tauri build pipeline — test in first task
- [ ] `@testing-library/svelte` v5 + Svelte 5 runes: confirm `$state` reactivity triggers in tests — validate during test infra setup task
