# Exploration: Fase 1 — MVP Documental

## Current State

Fase 0 (Fundaciones) is complete and archived. The codebase provides:

- **Monorepo**: PNPM workspaces + Turborepo (`apps/desktop`, `packages/store`, `packages/ui`, `packages/config-ts`)
- **Desktop shell**: Tauri 2 + Svelte 5 SPA, Vite bundler, port 1420
- **DB layer**: Drizzle `sqlite-proxy` → Tauri IPC → rusqlite. Schema: `collections`, `items`, `assets`, `notes`, `jobs`
- **Rust backend**: `db_execute` + `db_select` IPC commands, WAL mode, foreign keys ON
- **UI package**: Button, Input, Card components + CSS design tokens (dark theme)
- **DB init**: `initDb()` runs migrations on mount, `getDb()` returns Drizzle instance
- **No test runner** — Vitest not yet installed (noted as pending for Fase 1)
- **No routing** — App.svelte is a single card saying "Ready. Database initialized."
- **No file handling** — no Tauri FS/dialog/drag-drop plugins configured

### Affected Areas

- `apps/desktop/src/` — all new views, routing, state management
- `apps/desktop/src/lib/` — only has `db.ts`; needs repos, services, stores
- `apps/desktop/src-tauri/` — needs FS commands for file copy, possibly asset protocol config
- `apps/desktop/src-tauri/capabilities/default.json` — needs FS + dialog permissions
- `packages/store/src/` — may need repository layer, FTS5 migration
- `packages/ui/src/` — new components (Modal, Sidebar, DocumentViewer, SearchInput, etc.)
- Root `package.json` / `turbo.json` — Vitest setup

---

## Topic 1: File Import Strategy (Tauri 2)

### How It Works

1. **File picker**: `@tauri-apps/plugin-dialog` → `open()` returns native file path(s)
2. **File copy**: `@tauri-apps/plugin-fs` → `copyFile(source, dest)` copies to appDataDir
3. **Drag & drop**: Tauri 2 has BUILT-IN drag-drop events on the webview — `getCurrentWebview().onDragDropEvent()` from `@tauri-apps/api/webview`. Returns native file paths. No separate plugin needed.
4. **Serving files**: `convertFileSrc(filePath)` from `@tauri-apps/api/core` converts a native path to a `asset://` URL loadable by the webview.

### Recommended File Storage Layout

```
{appDataDir}/
├── entropia.sqlite
└── files/
    └── {collection_id}/
        └── {item_id}/
            ├── original.pdf
            ├── original.jpg
            └── thumb_400.jpg     ← future: generated thumbnail
```

### Approaches

| Approach                                      | Description                                                                     | Pros                                                     | Cons                                                                   | Effort |
| --------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------- | ------ |
| **A. Copy to appDataDir**                     | On import, copy file to `files/{coll_id}/{item_id}/`, store relative path in DB | Self-contained, portable, works offline, backup-friendly | Doubles disk usage; large collections = slow import                    | Medium |
| **B. Reference original path**                | Store the original filesystem path in DB, serve via `convertFileSrc()`          | No duplication, instant "import"                         | Files moved/deleted = broken refs; not portable; security scope issues | Low    |
| **C. Hybrid: copy by default, link optional** | Default to copy, offer "link" mode for large archives                           | Best of both worlds                                      | More UI complexity, two code paths                                     | High   |

### Recommendation: **Approach A — Copy to appDataDir**

For a historian's workflow, portability and reliability are paramount. The DB + files directory IS the archive. If they move files around on disk, we can't lose their data. Disk space is cheap; data integrity is not.

**Implementation notes**:

- Use `@tauri-apps/plugin-fs` `copyFile()` for the actual copy
- Use `@tauri-apps/plugin-dialog` `open({ multiple: true, filters })` for file picker
- Use `getCurrentWebview().onDragDropEvent()` for drag-and-drop (NO extra plugin needed)
- Generate UUID for item_id, create directory, copy file, insert DB records
- Serve images via `convertFileSrc(absolutePath)` → `<img src={assetUrl}>`
- For PDFs: store original, render via pdfjs-dist (see Topic 2)
- Thumbnails: defer to Fase 2 (OCR pipeline); for now, show generic PDF icon or first-page render

### Tauri Capability Changes Required

```json
{
  "permissions": [
    "core:default",
    "core:path:default",
    "core:event:default",
    "core:window:default",
    "dialog:default",
    "fs:default",
    "fs:allow-appdata-read-recursive",
    "fs:allow-appdata-write-recursive"
  ]
}
```

Need to add `@tauri-apps/plugin-dialog` and `@tauri-apps/plugin-fs` to both:

- `apps/desktop/package.json` (JS side)
- `apps/desktop/src-tauri/Cargo.toml` (Rust side, e.g. `tauri-plugin-dialog`, `tauri-plugin-fs`)
- Register plugins in `lib.rs` builder: `.plugin(tauri_plugin_dialog::init())`, etc.

---

## Topic 2: Document Viewer

### PDF Rendering

| Approach                              | Description                                   | Pros                                                            | Cons                                                           | Effort |
| ------------------------------------- | --------------------------------------------- | --------------------------------------------------------------- | -------------------------------------------------------------- | ------ |
| **A. pdfjs-dist**                     | Bundle Mozilla's PDF.js, render in `<canvas>` | Full control, zoom, page nav, text layer for future OCR overlay | +500KB bundle, need to manage worker                           | Medium |
| **B. `<iframe>` with asset protocol** | Load PDF via `asset://` URL in iframe         | Zero deps, browser handles rendering                            | No control over UI, no page navigation, no text layer, no zoom | Low    |
| **C. `<embed>`/`<object>` tag**       | Native browser PDF viewer                     | Simplest                                                        | Chromium's PDF viewer varies, no customization, poor DX        | Low    |
| **D. Tauri webview plugin**           | Open PDF in separate webview                  | Isolated, native feel                                           | Complex, limited control, overkill for this use case           | High   |

### Recommendation: **Approach A — pdfjs-dist**

EntropIA is a DOCUMENT ANALYSIS tool. The viewer is a core feature, not an afterthought. We need:

- Page navigation (multi-page historical documents are common)
- Zoom controls (examining details in degraded manuscripts)
- Text layer (future OCR overlay in Fase 2)
- Thumbnail strip (navigating 50+ page documents)

pdfjs-dist is the industry standard. The 500KB cost is irrelevant in a desktop app. We'd ship `pdf.worker.min.mjs` as a static asset in `public/`.

### Image Rendering

Simple `<img>` tag with `convertFileSrc()`:

```svelte
<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core'
  let { asset } = $props<{ asset: { path: string } }>()
  const src = $derived(convertFileSrc(asset.path))
</script>

<img {src} alt="Document" style="max-width: 100%; height: auto;" />
```

### Performance: Virtual Scrolling

For the collection view (100+ documents), we have two options:

| Approach                           | Pros                                   | Cons                            |
| ---------------------------------- | -------------------------------------- | ------------------------------- |
| **CSS `content-visibility: auto`** | Zero deps, native browser optimization | Less control, newer API         |
| **Svelte virtual list**            | Full control, battle-tested            | Extra dependency or custom code |

**Recommendation**: Start with CSS `content-visibility: auto` on collection grid items. It's sufficient for 100 docs (Fase 1 target). If we hit 1000+, add virtual scrolling in a later fase.

---

## Topic 3: App Routing / Navigation

### Options for Plain Svelte 5 (NO SvelteKit)

| Approach                              | Description                                              | Pros                                     | Cons                                                                   | Effort |
| ------------------------------------- | -------------------------------------------------------- | ---------------------------------------- | ---------------------------------------------------------------------- | ------ |
| **A. Runes-based store**              | Hand-rolled `$state` with a `currentView` + `params`     | Zero deps, Svelte 5 native, full control | No URL sync (desktop doesn't need it), manual transitions              | Low    |
| **B. svelte-routing**                 | Community router for plain Svelte                        | URL-based, familiar API                  | Last updated for Svelte 4; Svelte 5 compatibility uncertain, extra dep | Medium |
| **C. @melt-ui/router**                | Melt UI ecosystem router                                 | Designed for headless UI                 | Overkill, tied to Melt ecosystem we're not using                       | Medium |
| **D. TinyBase / custom store router** | Use a reactive store that maps route names to components | Flexible, testable                       | More boilerplate than A                                                | Medium |

### Recommendation: **Approach A — Runes-based navigation store**

This is a desktop app. There is NO URL bar. We don't need hash routing, history API, or URL sync. What we need is:

1. A reactive `currentView` state
2. A way to pass parameters (collection_id, item_id)
3. A way to go back (breadcrumb navigation)

```typescript
// lib/navigation.svelte.ts
type View =
  | { name: 'collections' }
  | { name: 'collection-detail'; collectionId: string }
  | { name: 'item-detail'; collectionId: string; itemId: string }

class NavigationStore {
  current = $state<View>({ name: 'collections' })
  #history: View[] = []

  navigate(view: View) {
    this.#history.push(this.current)
    this.current = view
  }

  back() {
    const prev = this.#history.pop()
    if (prev) this.current = prev
  }
}

export const nav = new NavigationStore()
```

Then in App.svelte:

```svelte
{#if nav.current.name === 'collections'}
  <CollectionsList />
{:else if nav.current.name === 'collection-detail'}
  <CollectionDetail collectionId={nav.current.collectionId} />
{:else if nav.current.name === 'item-detail'}
  <ItemDetail collectionId={nav.current.collectionId} itemId={nav.current.itemId} />
{/if}
```

Zero dependencies. Fully testable (it's just a class). Svelte 5 runes-native.

### Views Needed (Fase 1)

1. **Collections List** — grid/list of collections with create/edit/delete
2. **Collection Detail** — items grid, import button, search, export
3. **Item Detail** — document viewer (image/PDF), metadata editor, notes panel
4. **Modals**: Create/Edit collection, Create/Edit item metadata, Confirm delete

---

## Topic 4: State Management

### Local State

Svelte 5 runes (`$state`, `$derived`, `$effect`) are sufficient for ALL component-local state. No debate here.

### Shared App State

| Approach                       | Description                                                               | Pros                                   | Cons                                                   |
| ------------------------------ | ------------------------------------------------------------------------- | -------------------------------------- | ------------------------------------------------------ |
| **A. Runes class + context**   | Class with `$state` fields, provided via Svelte `setContext`/`getContext` | Native, zero deps, type-safe, testable | Must pass through context manually                     |
| **B. Module-level singletons** | Export `$state` runes from module files                                   | Simplest, works everywhere             | Harder to test (module-level side effects), no scoping |
| **C. Svelte stores (legacy)**  | `writable()`, `derived()` from `svelte/store`                             | Familiar, works                        | Svelte 5 moving away from them; mixing paradigms       |
| **D. Nano Stores**             | External lib, framework-agnostic                                          | Tiny, works with any framework         | Extra dep, doesn't leverage runes                      |

### Recommendation: **Approach A — Runes class + context for app state, Module singletons for services**

- **Navigation**: Module singleton (`lib/navigation.svelte.ts`) — app-wide, no scoping needed
- **Current data** (loaded collection, items list): Runes class via context — scoped to component tree, testable
- **DB access**: Repository pattern (see below)

### Repository Pattern for DB Access

| Approach                            | Description                                                               | Pros                                 | Cons                                                               |
| ----------------------------------- | ------------------------------------------------------------------------- | ------------------------------------ | ------------------------------------------------------------------ |
| **A. Repository classes**           | `CollectionRepo`, `ItemRepo`, `AssetRepo`, `NoteRepo` in `packages/store` | Clean separation, testable, reusable | More files, more indirection                                       |
| **B. Inline Drizzle in components** | Call `db.select().from(collections)` directly in Svelte components        | Fast to write                        | Untestable (Tauri IPC deps), business logic in views, not reusable |
| **C. Service layer in desktop app** | `lib/services/collection-service.ts` that wraps Drizzle calls             | Keeps store package thin             | Logic lives in app, not reusable across apps                       |

### Recommendation: **Approach A — Repository classes in `packages/store`**

Repositories belong in the store package because:

1. They encapsulate Drizzle queries — the ONLY place SQL lives
2. They're testable by mocking `DbClient` (no Tauri needed)
3. They're reusable if we ever add a web frontend or sync layer
4. They enforce the "no SQL in components" rule

```typescript
// packages/store/src/repos/collection-repo.ts
import { eq } from 'drizzle-orm'
import { collections } from '../schema'
import type { DrizzleClient } from '../types'

export class CollectionRepo {
  constructor(private db: DrizzleClient) {}

  async findAll() {
    return this.db.select().from(collections).orderBy(collections.updatedAt)
  }

  async findById(id: string) {
    const [row] = await this.db.select().from(collections).where(eq(collections.id, id))
    return row ?? null
  }

  async create(data: { name: string; description?: string }) {
    const now = Math.floor(Date.now() / 1000)
    const id = crypto.randomUUID()
    await this.db.insert(collections).values({
      id,
      name: data.name,
      description: data.description ?? null,
      createdAt: now,
      updatedAt: now,
    })
    return id
  }

  async update(id: string, data: Partial<{ name: string; description: string }>) {
    await this.db
      .update(collections)
      .set({
        ...data,
        updatedAt: Math.floor(Date.now() / 1000),
      })
      .where(eq(collections.id, id))
  }

  async delete(id: string) {
    await this.db.delete(collections).where(eq(collections.id, id))
  }
}
```

The desktop app creates repos after `initDb()` and provides them via context.

---

## Topic 5: Search Strategy

### Options

| Approach                  | Description                                                       | Pros                                                 | Cons                                                                                | Effort      |
| ------------------------- | ----------------------------------------------------------------- | ---------------------------------------------------- | ----------------------------------------------------------------------------------- | ----------- |
| **A. LIKE queries**       | `WHERE title LIKE '%term%' OR metadata LIKE '%term%'`             | Dead simple, zero setup                              | Slow on large datasets, no ranking, no word tokenization                            | Low         |
| **B. FTS5 virtual table** | `CREATE VIRTUAL TABLE items_fts USING fts5(title, metadata_text)` | Fast full-text, ranking, tokenization, prefix search | Extra migration, need to keep FTS in sync with items table, Drizzle has no FTS5 API | Medium      |
| **C. FTS5 + triggers**    | FTS5 table + INSERT/UPDATE/DELETE triggers to auto-sync           | FTS always in sync, no app-level sync code           | More complex migration, trigger debugging is painful                                | Medium-High |

### Analysis

The Fase 1 target is 100 documents. LIKE queries on 100 rows with a JSON metadata field will return in < 1ms. FTS5 is clearly overkill for this scale.

However, the metadata field is JSON. To search INSIDE it with LIKE, we'd do:

```sql
WHERE title LIKE '%term%' OR metadata LIKE '%term%'
```

This actually works fine for JSON — it searches the raw string. For 100 docs, it's instant.

FTS5 becomes relevant in Fase 3 (semantic search, OCR text search) when we have thousands of extracted text passages.

### Recommendation: **Approach A (LIKE) for Fase 1, FTS5 in Fase 3**

- Fase 1: LIKE on `items.title` + `items.metadata` — simple, effective, zero setup
- Fase 3: FTS5 virtual table for OCR text, entity names, full corpus search
- The repo layer abstracts this — swapping LIKE for FTS5 later requires changing ONE method

```typescript
// In ItemRepo
async search(term: string) {
  const pattern = `%${term}%`
  return this.db.select().from(items)
    .where(or(
      like(items.title, pattern),
      like(items.metadata, pattern)
    ))
}
```

---

## Topic 6: Vitest Setup (CRITICAL — Activates Strict TDD)

### Monorepo Configuration

| Approach                       | Description                                                                | Pros                                             | Cons                                              |
| ------------------------------ | -------------------------------------------------------------------------- | ------------------------------------------------ | ------------------------------------------------- |
| **A. Root config + workspace** | `vitest.config.ts` at root with `test.projects: ['packages/*', 'apps/*']`  | One command runs all, shared config              | Package-specific config requires inline overrides |
| **B. Per-package config**      | Each package has its own `vitest.config.ts`, root just runs `turbo test`   | Full independence, package-specific environments | More config files, potential drift                |
| **C. Hybrid**                  | Root config with workspace pointing to packages, each package can override | Best of both                                     | Slightly more complex setup                       |

### Recommendation: **Approach C — Hybrid (root workspace + per-package overrides)**

```
entropia/
├── vitest.workspace.ts          ← defines projects
├── packages/store/
│   └── vitest.config.ts         ← environment: 'node'
├── packages/ui/
│   └── vitest.config.ts         ← environment: 'jsdom', svelte plugin
└── apps/desktop/
    └── vitest.config.ts         ← environment: 'jsdom', svelte plugin, Tauri mocks
```

Root workspace file:

```typescript
// vitest.workspace.ts
import { defineWorkspace } from 'vitest/config'

export default defineWorkspace(['packages/*', 'apps/desktop'])
```

Each package gets its own `vitest.config.ts` that extends the Vite config:

```typescript
// packages/store/vitest.config.ts
import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node', // store is pure logic, no DOM
    globals: true,
  },
})
```

### Mocking Tauri IPC

This is the CRITICAL piece. `@entropia/store` imports `invoke` from `@tauri-apps/api/core`. In tests, Tauri doesn't exist. We need to mock it.

**Strategy**: `vi.mock('@tauri-apps/api/core')` at the test level, or provide a mockable `DbClient` interface.

Since our `DbClient` is already an interface:

```typescript
interface DbClient {
  execute(sql: string, params?: unknown[]): Promise<{ rowsAffected: number }>
  select<T>(sql: string, params?: unknown[]): Promise<T[]>
}
```

We can test repositories by passing a mock `DbClient` — NO Tauri mock needed. The repo tests become:

```typescript
const mockClient = {
  execute: vi.fn().mockResolvedValue({ rowsAffected: 1 }),
  select: vi.fn().mockResolvedValue([{ id: '1', name: 'Test' }]),
}
const db = createDrizzleClient(mockClient)
const repo = new CollectionRepo(db)
```

For component tests that use Tauri APIs directly (e.g., `convertFileSrc`, dialog), mock at the module level:

```typescript
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: vi.fn((path) => `asset://localhost/${path}`),
}))
```

### Test Environments

- `packages/store`: `environment: 'node'` — pure TypeScript, no DOM
- `packages/ui`: `environment: 'jsdom'` or `happy-dom` — Svelte component tests
- `apps/desktop`: `environment: 'jsdom'` — component tests with Tauri mocks

**jsdom vs happy-dom**: `happy-dom` is 2-3x faster but has edge cases with custom elements and some SVG APIs. For Svelte 5 component tests, `happy-dom` works fine. Recommend `happy-dom` for speed.

### Testing Library

- `@testing-library/svelte` v5 for Svelte 5 support
- `@testing-library/jest-dom` for DOM matchers

### Packages to Install

Root devDependencies:

```
vitest @testing-library/svelte @testing-library/jest-dom happy-dom
```

---

## Topic 7: Export (JSON)

### Export Structure

```json
{
  "version": "1.0",
  "exportedAt": "2026-04-11T12:00:00Z",
  "collection": {
    "id": "uuid",
    "name": "Archivo Histórico Municipal",
    "description": "...",
    "createdAt": 1712844000,
    "updatedAt": 1712844000
  },
  "items": [
    {
      "id": "uuid",
      "title": "Acta de cabildo 1810-05-25",
      "metadata": { "date": "1810-05-25", "author": "..." },
      "createdAt": 1712844000,
      "updatedAt": 1712844000,
      "assets": [{ "id": "uuid", "filename": "original.pdf", "type": "pdf", "size": 1234567 }],
      "notes": [{ "id": "uuid", "content": "...", "createdAt": 1712844000 }]
    }
  ]
}
```

**Note**: Assets exported by FILENAME only (not full path). If we later add "export with files", we'd create a ZIP with the JSON + files directory.

### Implementation

Use `@tauri-apps/plugin-dialog` `save()` for the save location, then `@tauri-apps/plugin-fs` `writeTextFile()` to write the JSON.

---

## Key Decisions Summary

| #   | Decision            | Recommendation                       | Rationale                                         |
| --- | ------------------- | ------------------------------------ | ------------------------------------------------- |
| 1   | File storage        | Copy to appDataDir                   | Portability + data integrity for historians       |
| 2   | Drag & drop         | Built-in Tauri webview events        | No extra plugin needed                            |
| 3   | PDF viewer          | pdfjs-dist                           | Core feature needs zoom, pages, future text layer |
| 4   | Image serving       | `convertFileSrc()`                   | Native Tauri API, zero overhead                   |
| 5   | Router              | Runes-based navigation store         | Desktop = no URL bar, zero deps                   |
| 6   | State mgmt          | Runes class + context                | Svelte 5 native, testable                         |
| 7   | DB access           | Repository pattern in store pkg      | Testable, reusable, clean separation              |
| 8   | Search              | LIKE queries (Fase 1), FTS5 (Fase 3) | 100 docs = LIKE is instant, FTS5 is overkill      |
| 9   | Vitest config       | Hybrid workspace + per-package       | Independence with shared tooling                  |
| 10  | Test environment    | happy-dom                            | Faster than jsdom, works with Svelte 5            |
| 11  | Tauri mock strategy | Mock DbClient interface (not invoke) | Clean, no module mocking needed for store tests   |
| 12  | Export format       | JSON with metadata only (no files)   | Simple, extensible to ZIP later                   |
| 13  | Virtual scrolling   | CSS content-visibility: auto         | Sufficient for 100 docs target                    |

---

## Risks

1. **pdfjs-dist + Svelte 5**: pdfjs-dist is framework-agnostic (canvas-based), but the worker setup in Vite needs careful configuration (`pdf.worker.min.mjs` as static asset, correct `workerSrc` path). Mitigation: test early, isolate in a `PdfViewer.svelte` component.

2. **Tauri plugin permissions**: Tauri 2 has a strict capability system. If FS permissions aren't scoped correctly, file operations will fail silently or throw. Mitigation: test file copy flow early in development.

3. **Drizzle sqlite-proxy limitations**: Drizzle's proxy mode doesn't support all features (e.g., `onConflictDoUpdate` may not work). Mitigation: test CRUD operations through the repo layer; fall back to raw SQL via `DbClient.execute()` if needed.

4. **Svelte 5 testing maturity**: `@testing-library/svelte` v5 (for Svelte 5 runes) is relatively new. Some patterns (testing `$state`, `$derived`) may have rough edges. Mitigation: start with store/repo tests (pure TS), add component tests incrementally.

5. **File import performance**: Copying large PDFs (50MB+) will block the UI if done synchronously. Mitigation: use async operations, show progress indicator, potentially batch imports.

6. **Metadata as JSON blob**: The `metadata` column is an unstructured JSON text field. Searching, validation, and editing are all on the client. Mitigation: define a metadata schema interface in TypeScript; validate on write.

---

## Affected Files (New or Modified)

### New Files

- `apps/desktop/src/lib/navigation.svelte.ts` — runes-based router
- `apps/desktop/src/lib/context.ts` — app-wide context (repos, state)
- `apps/desktop/src/views/CollectionsList.svelte` — collections grid
- `apps/desktop/src/views/CollectionDetail.svelte` — items grid + import
- `apps/desktop/src/views/ItemDetail.svelte` — viewer + metadata + notes
- `apps/desktop/src/components/PdfViewer.svelte` — pdfjs-dist wrapper
- `apps/desktop/src/components/ImageViewer.svelte` — image display
- `apps/desktop/src/components/MetadataEditor.svelte` — JSON metadata form
- `apps/desktop/src/components/NotesPanel.svelte` — notes CRUD
- `apps/desktop/src/components/SearchBar.svelte` — search input
- `apps/desktop/src/components/ImportDialog.svelte` — file import flow
- `packages/store/src/repos/` — CollectionRepo, ItemRepo, AssetRepo, NoteRepo
- `packages/store/src/repos/index.ts` — barrel export
- `vitest.workspace.ts` — root workspace config
- `packages/store/vitest.config.ts` — store test config
- `apps/desktop/vitest.config.ts` — desktop test config

### Modified Files

- `apps/desktop/src/App.svelte` — add router, layout shell
- `apps/desktop/src/lib/db.ts` — expose repos alongside db
- `apps/desktop/src-tauri/capabilities/default.json` — add FS + dialog permissions
- `apps/desktop/src-tauri/Cargo.toml` — add tauri-plugin-dialog, tauri-plugin-fs
- `apps/desktop/src-tauri/src/lib.rs` — register new plugins
- `apps/desktop/package.json` — add pdfjs-dist, dialog/fs plugins, vitest
- `packages/store/package.json` — add vitest
- `packages/store/src/index.ts` — export repos
- `packages/store/src/types.ts` — add DrizzleClient type export
- `package.json` (root) — add vitest

---

## Ready for Proposal

**Yes.** All 7 exploration areas are investigated with clear recommendations. No blockers found. The codebase from Fase 0 provides a solid foundation — schema, IPC bridge, Drizzle client, and base UI components are all in place. The next step is to create a formal proposal that scopes the work, defines phases/batches, and estimates effort.
