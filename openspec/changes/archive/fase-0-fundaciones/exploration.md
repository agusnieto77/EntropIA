# Exploration: Fase 0 — Fundaciones

> **Change**: fase-0-fundaciones
> **Date**: 2026-04-11
> **Status**: Complete — ready for proposal

---

## Current State

The project is in pure planning phase. The repository contains only:
- `README.md` — full architecture, stack, data model, and roadmap
- `.gitignore`
- `LICENSE`
- `.atl/skill-registry.md`

No code, no `package.json`, no `Cargo.toml`, no dependencies installed. This exploration defines the technical foundation for everything that follows.

---

## Investigation Areas

### 1. Turborepo + PNPM Workspaces Setup

**Research findings:**

PNPM workspaces are configured via `pnpm-workspace.yaml`:
```yaml
packages:
  - "apps/*"
  - "packages/*"
  - "services/*"
```

Turborepo adds task orchestration on top. The `turbo.json` config defines task dependencies, caching, and parallelism:

```jsonc
{
  "$schema": "https://turborepo.dev/schema.json",
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "outputs": ["dist/**", ".svelte-kit/**"]
    },
    "dev": {
      "cache": false,
      "persistent": true
    },
    "lint": {
      "outputs": []
    },
    "check-types": {
      "dependsOn": ["^build"],
      "outputs": []
    },
    "test": {
      "dependsOn": ["build"],
      "outputs": ["coverage/**"]
    }
  }
}
```

Root `package.json`:
```json
{
  "private": true,
  "scripts": {
    "build": "turbo run build",
    "dev": "turbo run dev",
    "lint": "turbo run lint",
    "check-types": "turbo run check-types",
    "test": "turbo run test"
  },
  "devDependencies": {
    "turbo": "^2"
  },
  "packageManager": "pnpm@9.0.0"
}
```

**Key insight**: `workspace:*` protocol for inter-package dependencies makes versions irrelevant in monorepo. Turborepo's `^build` means "build my dependencies before building me."

#### Turborepo vs plain PNPM for Fase 0

| Factor | PNPM only | PNPM + Turborepo |
|--------|-----------|-------------------|
| Setup complexity | Minimal | Low (one `turbo.json`) |
| Task caching | None | Built-in (huge CI speedup) |
| Parallel tasks | Manual | Automatic |
| `dev` orchestration | Multiple terminals | Single `turbo dev` |
| Migration cost later | Would need to add Turborepo | Already there |
| Learning curve | None | Very low |

**Recommendation**: Use Turborepo from day one. The overhead is literally one file (`turbo.json`) and one devDependency. The payoff in CI caching alone justifies it. Adding it later means retrofitting task definitions — better to start right.

---

### 2. Tauri 2 + Svelte Scaffold

**Two approaches:**

#### A. `create-tauri-app` (CTA)

```bash
pnpm create tauri-app --template svelte --manager pnpm
```

- Generates a complete Tauri + Svelte + Vite project
- Creates `src-tauri/` (Rust) alongside `src/` (Svelte/Vite)
- Includes `tauri.conf.json` pre-configured for Vite dev server
- Problem: generates a standalone project, NOT monorepo-aware

#### B. Manual setup in monorepo

1. Create `apps/desktop/` as a Vite + Svelte project
2. Run `pnpm tauri init` inside `apps/desktop/` to add `src-tauri/`
3. Configure `tauri.conf.json` to point at the Vite dev server

`apps/desktop/tauri.conf.json`:
```json
{
  "build": {
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build",
    "devUrl": "http://localhost:5173",
    "frontendDist": "../dist"
  }
}
```

**Recommended approach**: **Hybrid** — use `create-tauri-app` to scaffold initially, then move the result into `apps/desktop/` and adjust paths. This gives us the correct Tauri 2 boilerplate (which evolves between versions) while fitting the monorepo structure.

Alternatively: scaffold `apps/desktop` as a Vite + Svelte project first (`pnpm create vite`), then `pnpm tauri init` to add Tauri. This is cleaner for monorepo — avoids needing to move files.

**Final recommendation**: Option B (manual) — `pnpm create vite` → `pnpm tauri init`. Simpler, no file moving.

#### Svelte vs SvelteKit for `apps/desktop`

| Factor | Svelte (Vite) | SvelteKit |
|--------|---------------|-----------|
| Routing | None (SPA, we add our own) | Built-in file-based |
| SSR | Not needed (desktop) | Has SSR (must disable for Tauri) |
| Adapter | Not needed | Needs `@sveltejs/adapter-static` |
| Complexity | Minimal | Higher — SSR config, adapters |
| Tauri compat | Perfect | Needs `ssr: false` + static adapter |

**Recommendation**: **Plain Svelte + Vite** for `apps/desktop`. This is a desktop app — we don't need SSR, file-based routing, or adapters. A SPA with a simple client-side router (like `svelte-spa-router` or TanStack Router) is perfect. SvelteKit adds complexity with zero benefit for a Tauri shell.

---

### 3. SQLite + Drizzle in Tauri — The Critical Decision

This is the most architecturally significant decision in Fase 0. There are three approaches:

#### A. `tauri-plugin-sql` (Rust-side SQLite, JS API)

**How it works**: The official Tauri SQL plugin runs SQLite on the Rust side via `sqlx`. It exposes `execute()` and `select()` to JS through IPC.

```typescript
import Database from '@tauri-apps/plugin-sql';
const db = await Database.load('sqlite:entropia.db');
const result = await db.select('SELECT * FROM collections WHERE id = ?', [id]);
await db.execute('INSERT INTO items (id, title) VALUES (?, ?)', [id, title]);
```

**Drizzle integration**: Drizzle does NOT have a native `tauri-plugin-sql` driver. You would need to use `drizzle-orm/sqlite-proxy` — a proxy driver where you provide the execute/select callbacks:

```typescript
import { drizzle } from 'drizzle-orm/sqlite-proxy';
import Database from '@tauri-apps/plugin-sql';

const sqlite = await Database.load('sqlite:entropia.db');

const db = drizzle(async (sql, params, method) => {
  if (method === 'run') {
    await sqlite.execute(sql, params);
    return { rows: [] };
  }
  const rows = await sqlite.select(sql, params);
  return { rows };
});
```

| Pros | Cons |
|------|------|
| Official Tauri plugin, well-maintained | Every query crosses IPC bridge (Rust ↔ JS) |
| SQLite runs in Rust (fast, native) | Drizzle needs proxy driver (more glue code) |
| No Node.js dependency | Migrations must be handled separately |
| Access to Rust ecosystem for DB | Plugin API is limited (no custom functions) |

#### B. `better-sqlite3` via Node.js (Tauri sidecar or embedded)

**How it works**: Ship `better-sqlite3` as a Node.js dependency. Tauri 2 doesn't embed Node.js, so this requires either a sidecar process or using `tauri-plugin-shell` to run a Node process.

| Pros | Cons |
|------|------|
| Drizzle has native `better-sqlite3` driver | Requires bundling Node.js runtime |
| Synchronous API (fast) | Adds ~50MB to app size (Node binary) |
| Full SQLite feature access | Sidecar management complexity |
| Drizzle migrations work out of the box | Native addon — platform-specific builds |

**Verdict**: **Reject**. Bundling Node.js defeats the purpose of using Tauri (lightweight desktop app). The 50MB overhead and sidecar complexity are deal-breakers.

#### C. Rust `sqlx` / `rusqlite` directly (custom Tauri commands)

**How it works**: Write Rust commands that use `sqlx` or `rusqlite` directly. Expose typed commands to JS via `#[tauri::command]`.

```rust
#[tauri::command]
async fn get_collections(state: State<'_, DbPool>) -> Result<Vec<Collection>, String> {
    sqlx::query_as::<_, Collection>("SELECT * FROM collections")
        .fetch_all(&*state.pool)
        .await
        .map_err(|e| e.to_string())
}
```

| Pros | Cons |
|------|------|
| Full Rust power, max performance | Every query needs a Rust command |
| Type safety on Rust side | Can't use Drizzle at all |
| Can use sqlite-vec via rusqlite | Slower iteration (compile Rust for each change) |
| No proxy layer | Dual type definitions (Rust + TS) |

**Verdict**: Viable for complex queries later, but for Fase 0 this means no Drizzle (entire ORM benefit lost). Would need a different ORM on the Rust side (SeaORM, Diesel) or raw SQL.

#### D. Hybrid: `tauri-plugin-sql` + Drizzle proxy (recommended)

**How it works**: Use `tauri-plugin-sql` for the SQLite engine, Drizzle `sqlite-proxy` for the ORM layer in JS, and pre-generate migrations with `drizzle-kit generate` that are shipped as SQL files.

```
Architecture:
  JS (Drizzle sqlite-proxy) → IPC → Rust (tauri-plugin-sql / sqlx) → SQLite file
```

**Migration strategy**:
1. `drizzle-kit generate` produces `.sql` migration files at dev time
2. Migration files are bundled with the Tauri app (in `src-tauri/resources/` or `$RESOURCE` dir)
3. At app startup, run migrations via `tauri-plugin-sql`'s `execute()` — or a custom Rust command that reads migration files and applies them in order
4. A `_migrations` table tracks which migrations have been applied

```typescript
// packages/store/src/migrate.ts
export async function runMigrations(db: TauriDatabase) {
  // Read migration files from bundled resources
  // Apply each in order, tracking in _migrations table
}
```

**Why this is the best approach**:
- Drizzle's type-safe schema in TypeScript = fast iteration on data model
- `drizzle-kit generate` = versioned, reviewable SQL migration files
- `tauri-plugin-sql` = battle-tested SQLite on Rust side, no Node.js needed
- IPC overhead is negligible for a desktop app (we're not doing 1000 queries/sec)
- Later, the SAME Drizzle schema works with Cloudflare D1 (SQLite-compatible) for the sync layer
- Migration files can be tested independently of the Tauri runtime

| Pros | Cons |
|------|------|
| Type-safe schema + queries in TS | IPC overhead per query |
| Same schema for local + D1 sync | Custom migration runner needed |
| `drizzle-kit generate` for migrations | sqlite-proxy is less ergonomic than native driver |
| No Node.js required | sqlite-vec needs separate Rust integration |
| Fast dev iteration on schema | |

---

### 4. Drizzle Migrations Strategy

**`push` vs `migrate` vs `generate`:**

| Command | What it does | Use case |
|---------|-------------|----------|
| `drizzle-kit push` | Directly alters DB schema | Local dev prototyping only |
| `drizzle-kit generate` | Generates `.sql` migration files | Production — versioned migrations |
| `drizzle-kit migrate` | Applies migration files to DB | Server-side (needs DB connection) |

**For a desktop app**:
- `drizzle-kit push` is useless in production (no dev server)
- `drizzle-kit generate` at development time → produces SQL files
- Custom runtime migrator at app startup → applies SQL files to local SQLite

**Strategy**:
1. **Dev time**: `drizzle-kit generate` produces migration files in `packages/store/drizzle/`
2. **Build time**: Migration SQL files are copied into Tauri's `resources/` directory
3. **Runtime**: App startup reads migration files, checks `_drizzle_migrations` table, applies new ones
4. **Dev shortcut**: `drizzle-kit push` works locally via a small Node script that opens the SQLite file directly (for fast iteration, NOT for production)

`drizzle.config.ts` for the store package:
```typescript
import { defineConfig } from 'drizzle-kit';

export default defineConfig({
  schema: './src/schema.ts',
  out: './drizzle',
  dialect: 'sqlite',
  dbCredentials: {
    url: './dev.db'  // local dev only
  }
});
```

---

### 5. CI Setup

**Recommended**: GitHub Actions with a single Linux runner for Fase 0.

```yaml
# .github/workflows/ci.yml
name: CI
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm lint
      - run: pnpm check-types
```

**Why Linux-only for now**:
- Lint and typecheck are platform-independent
- Tauri build requires platform-specific runners (Linux + macOS + Windows) — this is Fase 1+ concern
- Running 3 platforms triples CI time and cost for zero benefit in Fase 0
- When we need to produce binaries, we add the matrix

**Turborepo CI caching**: Not needed for Fase 0. When builds get slow, add Vercel Remote Caching (one env var).

---

### 6. Design System (`packages/ui`)

#### Separate package vs co-located

| Factor | `packages/ui` (separate) | Inside `apps/desktop` |
|--------|--------------------------|------------------------|
| Reusability | Can be used by other apps/packages | Locked to desktop |
| Build pipeline | Needs own build step | Built with desktop |
| Import ergonomics | `@entropia/ui` clean imports | Relative imports |
| Storybook | Easy to add | Harder to isolate |
| For Fase 0 | Slightly more setup | Slightly less setup |

**Recommendation**: **Separate `packages/ui`** from day one. The overhead is minimal (one extra `package.json`), and it establishes the pattern that ALL shared code lives in packages. When we add Storybook or a second consumer, we're already structured correctly.

#### CSS approach

| Factor | CSS Custom Properties (tokens) | Tailwind CSS |
|--------|-------------------------------|--------------|
| Bundle size | Minimal | Adds PostCSS pipeline |
| Customization | Full control | Utility-first |
| Learning curve | Low | Medium |
| Design token support | Native | Via `tailwind.config` |
| Svelte integration | Perfect (scoped styles) | Needs PostCSS setup |

**Recommendation**: **CSS Custom Properties for tokens + Svelte scoped styles**. Tailwind is great but adds tooling complexity. For a design system with a small team, hand-crafted tokens give more control and better understanding of what's happening. Tailwind can be added later if desired — CSS variables work alongside it.

Design system structure:
```
packages/ui/
  src/
    tokens/
      colors.css
      typography.css
      spacing.css
    components/
      Button.svelte
      Input.svelte
      Card.svelte
    index.ts          ← re-exports
  package.json
```

#### Plain Svelte lib vs SvelteKit for `packages/ui`

**Plain Svelte library** — use `svelte-package` (from `@sveltejs/package`) to build the library. This gives us `.svelte` components that can be imported by `apps/desktop`. No SvelteKit needed for a component library.

---

## Approaches for Fase 0 Scope

### Approach 1: Full planned structure (6 packages) — REJECTED

Create all packages from the README: `ui`, `store`, `ai-pipeline`, `ner`, `embeddings`, `sync`.

- **Pros**: Complete structure from day one
- **Cons**: 4 empty packages with nothing in them. Over-engineering. YAGNI.
- **Effort**: Medium (lots of boilerplate, no substance)

### Approach 2: Minimal viable structure (3 packages) — RECOMMENDED

Create only what's needed for Fase 0's "done when" criteria:
- `apps/desktop` — Tauri 2 + Svelte shell
- `packages/ui` — Design system tokens + 2-3 base components
- `packages/store` — Drizzle schema + migration runner + DB connection

- **Pros**: Every package has real code. No empty shells. Easy to add packages later.
- **Cons**: None — adding a package is `mkdir + package.json` (2 minutes)
- **Effort**: Low

### Approach 3: Desktop-only (no packages) — REJECTED

Put everything in `apps/desktop`. Extract to packages later.

- **Pros**: Simplest possible start
- **Cons**: Defeats monorepo purpose. Extraction is painful. Sets bad precedent.
- **Effort**: Very Low, but creates tech debt immediately

---

## Recommendation

**Approach 2: Minimal viable structure** with these specifics:

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Monorepo tool | PNPM workspaces + Turborepo | Caching + task orchestration, trivial setup cost |
| Fase 0 packages | `apps/desktop`, `packages/ui`, `packages/store` | Only what has real code |
| Tauri scaffold | Vite + Svelte → `pnpm tauri init` | Clean monorepo integration |
| Frontend framework | Plain Svelte (NOT SvelteKit) | Desktop SPA — no SSR needed |
| SQLite strategy | `tauri-plugin-sql` + Drizzle `sqlite-proxy` | Best balance: Rust SQLite + TS type safety |
| Migrations | `drizzle-kit generate` → ship SQL files → runtime migrator | Versioned, reviewable, bundleable |
| Design system | CSS Custom Properties + Svelte scoped styles | Minimal, full control |
| UI package build | `svelte-package` | Standard Svelte library tooling |
| CI | GitHub Actions, Linux-only, lint + typecheck | Platform matrix later when building binaries |
| Additional packages | Add in their respective Fases (ai-pipeline in Fase 2, etc.) | YAGNI |

---

## Risks

1. **Drizzle `sqlite-proxy` friction**: The proxy driver is less ergonomic than native drivers. May need a thin wrapper to smooth over `tauri-plugin-sql` ↔ Drizzle bridging. Mitigated by keeping the bridge in `packages/store` behind a clean API.

2. **Migration runner reliability**: Custom migration runner for desktop app is non-trivial — needs to handle interrupted migrations, version tracking, and migration ordering. Mitigated by keeping it simple (sequential SQL files, `_drizzle_migrations` table).

3. **sqlite-vec integration**: `sqlite-vec` is a SQLite extension that needs to be loaded on the Rust side. `tauri-plugin-sql` may not support loading extensions out of the box — may need a custom Rust command or forked plugin. This is a Fase 3 concern but should be validated early.

4. **Tauri 2 Svelte template freshness**: Tauri 2 is relatively new and templates may lag behind. Using manual setup (Vite → tauri init) is more resilient to template staleness.

5. **PNPM + Tauri monorepo paths**: Tauri's `beforeDevCommand` and `frontendDist` paths need careful configuration in a monorepo context. The `src-tauri/` directory being nested in `apps/desktop/` means relative paths in `tauri.conf.json` must account for the workspace structure.

6. **Hot reload in Tauri dev**: Vite HMR works through the dev server URL (`http://localhost:5173`). In a monorepo, changes to `packages/ui` need to trigger a rebuild that Vite picks up. Turborepo's `dev` task with `persistent: true` + Vite's dependency watching should handle this, but needs validation.

---

## Ready for Proposal

**Yes** — all key technical decisions have been researched and recommended. The next step is to create a formal proposal (`sdd-propose`) that codifies these decisions and defines the implementation scope for Fase 0.

---

## Affected Areas (for implementation)

- `pnpm-workspace.yaml` — workspace definition (new file)
- `turbo.json` — task pipeline (new file)
- `package.json` — root package with scripts + turbo (new file)
- `apps/desktop/` — Tauri 2 + Svelte + Vite shell (new directory)
- `apps/desktop/src-tauri/` — Tauri Rust backend (new directory)
- `packages/ui/` — Design system with tokens + base components (new directory)
- `packages/store/` — Drizzle schema + migration runner (new directory)
- `.github/workflows/ci.yml` — CI pipeline (new file)
- `tsconfig.json` — root TypeScript config (new file)
- `.eslintrc` / `eslint.config.js` — shared ESLint config (new file)
- `.prettierrc` — shared Prettier config (new file)
