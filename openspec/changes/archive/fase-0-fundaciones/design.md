# Design: Fase 0 — Fundaciones

## Technical Approach

Bootstrap a 3-package PNPM + Turborepo monorepo (`apps/desktop`, `packages/ui`, `packages/store`) with shared tooling. Tauri 2 shell consumes UI tokens via workspace protocol. SQLite runs Rust-side via `tauri-plugin-sql`; Drizzle `sqlite-proxy` provides type-safe TS queries over IPC. Migrations are pre-generated SQL files applied at startup. CI validates lint + typecheck on Linux.

## Architecture Decisions

| ID | Decision | Choice | Alternatives Rejected | Rationale |
|----|----------|--------|-----------------------|-----------|
| ADR-001 | SQLite access strategy | `tauri-plugin-sql` + Drizzle `sqlite-proxy` | `better-sqlite3` (requires Node.js, +50MB), pure Rust sqlx (loses Drizzle ORM benefit) | Rust-side SQLite + TS type safety. IPC overhead negligible for desktop. Same Drizzle schema reusable for D1 sync (Fase 4). |
| ADR-002 | Frontend framework | Plain Svelte + Vite SPA | SvelteKit (SSR must be disabled, needs static adapter, adds complexity for zero desktop benefit) | Desktop app has no SSR/routing needs. Minimal bundle, zero framework overhead. |
| ADR-003 | Task orchestration | PNPM workspaces + Turborepo | Bare PNPM workspaces (no caching, manual parallel tasks) | One `turbo.json` file adds caching, topological builds, and single `turbo dev` command. Trivial setup cost. |
| ADR-004 | Styling approach | CSS Custom Properties tokens + Svelte scoped styles | Tailwind CSS (adds PostCSS pipeline, utility-first is overkill for small token set) | Full control over token system. Works natively with Svelte scoped styles. Tailwind can layer on top later if needed. |

## Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│  apps/desktop (Tauri 2 + Svelte + Vite)                     │
│                                                             │
│  Svelte Components ◄── @entropia/ui (workspace:*)           │
│       │                                                     │
│       ▼                                                     │
│  Drizzle Client ◄── @entropia/store (workspace:*)           │
│  (sqlite-proxy)                                             │
│       │                                                     │
│       ▼  invoke("plugin:sql|execute/select")                │
├───────┼─────────────────────────────────────────────────────┤
│  IPC  │  (Tauri command bridge)                             │
├───────┼─────────────────────────────────────────────────────┤
│       ▼                                                     │
│  tauri-plugin-sql (Rust/sqlx) ──► SQLite file               │
│                                   ($appDataDir/entropia.db) │
└─────────────────────────────────────────────────────────────┘
```

**Migration flow** (startup):
```
App start → store.initialize() → read _drizzle_migrations → diff vs bundled SQL files → execute pending → done
```

## File Changes

### Root
| File | Action | Description |
|------|--------|-------------|
| `package.json` | Create | `private:true`, workspace scripts delegating to turbo, `packageManager` field |
| `pnpm-workspace.yaml` | Create | `apps/*` + `packages/*` globs |
| `turbo.json` | Create | Tasks: build (^build), dev (persistent), lint, check-types (^build), test |
| `tsconfig.json` | Create | Base config: `target: ES2022`, `module: ESNext`, strict, paths |
| `eslint.config.js` | Create | Flat config, `@typescript-eslint`, `eslint-plugin-svelte` |
| `.prettierrc` | Create | Tabs, single quotes, trailing commas |

### `apps/desktop/`
| File | Action | Description |
|------|--------|-------------|
| `package.json` | Create | Deps: `@entropia/ui`, `@entropia/store`, `@tauri-apps/api`, `@tauri-apps/plugin-sql` |
| `vite.config.ts` | Create | Svelte plugin, resolve alias for `@entropia/*` pointing to workspace packages |
| `tsconfig.json` | Create | Extends root, include `src/**` |
| `src-tauri/tauri.conf.json` | Create | `devUrl: localhost:5173`, `frontendDist: ../dist`, window 1024×768, title "EntropIA" |
| `src-tauri/Cargo.toml` | Create | `tauri`, `tauri-plugin-sql` with `sqlite` feature |
| `src-tauri/src/lib.rs` | Create | Register `tauri-plugin-sql`, manage app setup |
| `src/main.ts` | Create | Mount Svelte app, import tokens CSS, call `store.initialize()` |
| `src/App.svelte` | Create | Root component importing UI components |

### `packages/ui/`
| File | Action | Description |
|------|--------|-------------|
| `package.json` | Create | Name `@entropia/ui`, `svelte-package` build, exports map |
| `src/tokens/colors.css` | Create | `--color-primary`, `--color-surface`, `--color-text`, semantic palette |
| `src/tokens/spacing.css` | Create | `--spacing-xs` through `--spacing-3xl` (4px scale) |
| `src/tokens/typography.css` | Create | `--font-family`, `--font-size-*`, `--font-weight-*`, `--line-height-*` |
| `src/tokens/radius.css` | Create | `--radius-sm`, `--radius-md`, `--radius-lg` |
| `src/tokens/index.css` | Create | Barrel import of all token files on `:root` |
| `src/components/Button.svelte` | Create | Props: `variant`, `size`, `disabled`. Slots: default. Token-styled. |
| `src/components/Input.svelte` | Create | Props: `type`, `placeholder`, `value`, `disabled`. Token-styled. |
| `src/components/Card.svelte` | Create | Props: `padding`. Slots: default, header, footer. Token-styled. |
| `src/index.ts` | Create | Re-exports components + token CSS path |

### `packages/store/`
| File | Action | Description |
|------|--------|-------------|
| `package.json` | Create | Name `@entropia/store`, deps: `drizzle-orm`, `@tauri-apps/plugin-sql` |
| `src/schema.ts` | Create | Drizzle tables: `collections`, `items`, `assets`, `notes`, `jobs` |
| `src/client.ts` | Create | `sqlite-proxy` drizzle instance wrapping `tauri-plugin-sql` IPC |
| `src/runner.ts` | Create | Migration runner: reads SQL files, tracks in `_drizzle_migrations` |
| `src/index.ts` | Create | Exports: `db`, `schema`, `initialize()` |
| `drizzle.config.ts` | Create | Schema path, output `./drizzle`, dialect `sqlite` |
| `drizzle/0000_initial.sql` | Create | Generated DDL for base schema (5 tables) |

### CI
| File | Action | Description |
|------|--------|-------------|
| `.github/workflows/ci.yml` | Create | Triggers: push main + PRs. Jobs: lint + typecheck (parallel). PNPM cache. |

## Interfaces / Contracts

### Drizzle sqlite-proxy bridge (`packages/store/src/client.ts`)
```typescript
import { drizzle } from "drizzle-orm/sqlite-proxy";
import Database from "@tauri-apps/plugin-sql";
import * as schema from "./schema";

let sqlite: Database;

export async function initialize() {
  sqlite = await Database.load("sqlite:entropia.db");
  await runMigrations(sqlite);
}

export const db = drizzle(
  async (sql, params, method) => {
    if (method === "run") {
      await sqlite.execute(sql, params);
      return { rows: [] };
    }
    const rows = await sqlite.select<Record<string, unknown>[]>(sql, params);
    return { rows: rows.map((r) => Object.values(r)) };
  },
  { schema }
);
```

### Base schema (`packages/store/src/schema.ts`)
```typescript
import { sqliteTable, text, integer } from "drizzle-orm/sqlite-core";
import { sql } from "drizzle-orm";

const id = () => text("id").primaryKey();
const createdAt = () => text("created_at").default(sql`(datetime('now'))`).notNull();
const updatedAt = () => text("updated_at").default(sql`(datetime('now'))`).notNull();

export const collections = sqliteTable("collections", {
  id: id(), name: text("name").notNull(), description: text("description"),
  createdAt: createdAt(), updatedAt: updatedAt(),
});

export const items = sqliteTable("items", {
  id: id(), title: text("title").notNull(),
  collectionId: text("collection_id").references(() => collections.id).notNull(),
  metadata: text("metadata", { mode: "json" }),
  createdAt: createdAt(), updatedAt: updatedAt(),
});

export const assets = sqliteTable("assets", {
  id: id(), itemId: text("item_id").references(() => items.id).notNull(),
  path: text("path").notNull(), type: text("type").notNull(),
  createdAt: createdAt(),
});

export const notes = sqliteTable("notes", {
  id: id(), itemId: text("item_id").references(() => items.id).notNull(),
  content: text("content").notNull(),
  createdAt: createdAt(), updatedAt: updatedAt(),
});

export const jobs = sqliteTable("jobs", {
  id: id(), type: text("type").notNull(),
  status: text("status", { enum: ["pending","running","done","error"] }).notNull().default("pending"),
  assetId: text("asset_id").references(() => assets.id),
  result: text("result", { mode: "json" }), error: text("error"),
  createdAt: createdAt(), updatedAt: updatedAt(),
});
```

### Migration runner (`packages/store/src/runner.ts`)
```typescript
// Reads SQL files from bundled resources (naming: NNNN_name.sql)
// Creates _drizzle_migrations table if not exists
// Applies pending migrations in filename order
// Each migration wrapped in transaction for atomicity
```

### UI component props (`packages/ui`)
```typescript
// Button.svelte props
type ButtonVariant = "primary" | "secondary" | "ghost";
type ButtonSize = "sm" | "md" | "lg";
interface ButtonProps { variant?: ButtonVariant; size?: ButtonSize; disabled?: boolean; }

// Input.svelte props
interface InputProps { type?: "text" | "password" | "email"; placeholder?: string; value?: string; disabled?: boolean; }

// Card.svelte — uses slots: default, header, footer
interface CardProps { padding?: "none" | "sm" | "md" | "lg"; }
```

## Testing Strategy

| Layer | What to Test | Approach |
|-------|-------------|----------|
| Unit | Drizzle schema exports, migration file ordering | Vitest (add in `packages/store`) |
| Integration | sqlite-proxy ↔ tauri-plugin-sql bridge | Manual validation via `tauri dev` (automated in Fase 1) |
| E2E | Window opens, renders UI, DB initializes | Manual smoke test; Tauri WebDriver in Fase 1 |
| CI | Lint + typecheck pass across all packages | GitHub Actions, `turbo run lint check-types` |

Storybook for `packages/ui` is **deferred to Fase 1** — Fase 0 validates components render inside desktop.

## Migration / Rollout

Greenfield project — no data migration. Rollback = `git revert`.

Migration files for the DB schema are generated via `drizzle-kit generate` and committed to the repo. The runtime runner applies them sequentially at app startup.

## Open Questions

- [ ] `tauri-plugin-sql` `select()` returns `Record<string, unknown>[]` — verify Drizzle `sqlite-proxy` expects rows as arrays of values (`unknown[][]`) vs objects. May need `Object.values()` mapping.
- [ ] Validate `svelte-package` output works as direct Vite dependency via workspace protocol without a separate build step (source imports vs pre-built dist).
