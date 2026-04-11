# Tasks: Fase 0 — Fundaciones

## Phase 1: Monorepo Scaffold

- [x] 1.1 Create root `package.json` (`private:true`, `packageManager: pnpm@9.x`, scripts delegating to `turbo run <task>` for dev/build/lint/check-types/test) — [monorepo/Root Scripts]
- [x] 1.2 Create `pnpm-workspace.yaml` with `apps/*` + `packages/*` globs — [monorepo/Workspace Configuration]
- [x] 1.3 Create `turbo.json` with tasks: `build` (dependsOn `^build`), `dev` (persistent, no cache), `lint`, `check-types`, `test` — [monorepo/Turborepo Pipeline]
- [x] 1.4 Create root `tsconfig.json`: `target ES2022`, `module ESNext`, `strict`, `moduleResolution bundler` — [design ADR-002]
- [x] 1.5 Create `.npmrc` with `shamefully-hoist=false`, `strict-peer-dependencies=false` — [monorepo/Workspace Configuration]
- [x] 1.6 Update `.gitignore` — add `node_modules/`, `dist/`, `target/`, `.turbo/`, `*.db` patterns

## Phase 2: Tooling & Shared Config

- [x] 2.1 Create `eslint.config.js` (flat config): `@typescript-eslint`, `eslint-plugin-svelte`, ignores for `dist/`, `target/`, `.turbo/` — [ci/Quality Jobs]
- [x] 2.2 Create `.prettierrc`: semi:false, single quotes, trailing commas, Svelte plugin — [design File Changes/Root]
- [x] 2.3 Create `packages/config-ts/` — shared tsconfig package with base.json and svelte.json — [ci/Quality Jobs]

## Phase 3: packages/ui — Design Tokens & Components

- [ ] 3.1 Create `packages/ui/package.json`: name `@entropia/ui`, `svelte` field, exports map (`./tokens.css` → `src/tokens/index.css`, `.` → `src/index.ts`) — [design-system/Package Consumability]
- [ ] 3.2 Create `packages/ui/tsconfig.json` extending root — [design File Changes/packages/ui]
- [ ] 3.3 Create `packages/ui/src/tokens/colors.css`: `:root` with `--color-primary`, `--color-surface`, `--color-text`, `--color-border`, semantic palette — [design-system/CSS Design Tokens]
- [ ] 3.4 Create `packages/ui/src/tokens/spacing.css`: `--spacing-xs` (4px) through `--spacing-3xl` (4px scale) — [design-system/CSS Design Tokens]
- [ ] 3.5 Create `packages/ui/src/tokens/typography.css`: `--font-family`, `--font-size-sm/md/lg`, `--font-weight-*`, `--line-height-*` — [design-system/CSS Design Tokens]
- [ ] 3.6 Create `packages/ui/src/tokens/radius.css`: `--radius-sm`, `--radius-md`, `--radius-lg` — [design-system/CSS Design Tokens]
- [ ] 3.7 Create `packages/ui/src/tokens/index.css`: barrel `@import` of all token files — [design-system/CSS Design Tokens]
- [ ] 3.8 Create `packages/ui/src/components/Button.svelte`: props `variant` (primary|secondary|ghost), `size` (sm|md|lg), `disabled`. Token-styled. — [design-system/Base Components]
- [ ] 3.9 Create `packages/ui/src/components/Input.svelte`: props `type`, `placeholder`, `value`, `disabled`. Token-styled. — [design-system/Base Components]
- [ ] 3.10 Create `packages/ui/src/components/Card.svelte`: props `padding` (none|sm|md|lg). Slots: default, header, footer. Token-styled. — [design-system/Base Components]
- [ ] 3.11 Create `packages/ui/src/index.ts`: re-export Button, Input, Card + token CSS path string — [design-system/Package Consumability]

## Phase 4: packages/store — Schema, Client & Migrations

- [ ] 4.1 Create `packages/store/package.json`: name `@entropia/store`, deps `drizzle-orm`, `@tauri-apps/plugin-sql`, devDeps `drizzle-kit` — [data-store/Drizzle sqlite-proxy Client]
- [ ] 4.2 Create `packages/store/tsconfig.json` extending root — [design File Changes/packages/store]
- [ ] 4.3 Create `packages/store/src/schema.ts`: Drizzle tables `collections`, `items`, `assets`, `notes`, `jobs` — TEXT PKs, `created_at`/`updated_at`, foreign keys per design contract — [data-store/Base Schema]
- [ ] 4.4 Create `packages/store/src/client.ts`: Drizzle `sqlite-proxy` instance wrapping `@tauri-apps/plugin-sql` Database.load("sqlite:entropia.db") — [data-store/Drizzle sqlite-proxy Client, data-store/IPC Bridge]
- [ ] 4.5 Create `packages/store/src/runner.ts`: migration runner — creates `_drizzle_migrations` table, reads `migrations/*.sql`, applies pending in filename order, records each — [data-store/Migration Runner, data-store/Migration Idempotency]
- [ ] 4.6 Create `packages/store/drizzle.config.ts`: schema path `./src/schema.ts`, out `./drizzle`, dialect `sqlite` — [data-store/Migration Runner]
- [ ] 4.7 Generate initial migration: run `drizzle-kit generate` → `packages/store/drizzle/0000_initial.sql` (5 tables DDL) — [data-store/Base Schema]
- [ ] 4.8 Create `packages/store/src/index.ts`: export `db`, `schema`, `initialize()` (loads DB + runs migrations) — [data-store/Drizzle sqlite-proxy Client]

## Phase 5: apps/desktop — Tauri 2 + Svelte Shell

- [ ] 5.1 Create `apps/desktop/package.json`: deps `@entropia/ui: workspace:*`, `@entropia/store: workspace:*`, `@tauri-apps/api`, `@tauri-apps/cli`, `svelte`, `vite`, `@sveltejs/vite-plugin-svelte` — [desktop-app/App Launch, monorepo/Workspace Configuration]
- [ ] 5.2 Create `apps/desktop/vite.config.ts`: Svelte plugin, `server.port: 5173`, `server.strictPort: true` — [desktop-app/Hot Reload — Local Sources]
- [ ] 5.3 Create `apps/desktop/tsconfig.json` extending root, include `src/**` — [design File Changes/apps/desktop]
- [ ] 5.4 Create `apps/desktop/index.html`: minimal HTML shell mounting `#app`, `<script type="module" src="/src/main.ts">` — [desktop-app/App Launch]
- [ ] 5.5 Create `apps/desktop/src/main.ts`: import `@entropia/ui/tokens.css`, mount Svelte App, call `initialize()` from `@entropia/store` — [desktop-app/App Launch, data-store/Database File Creation]
- [ ] 5.6 Create `apps/desktop/src/App.svelte`: root component importing Button, Card from `@entropia/ui` — renders smoke-test UI — [desktop-app/App Launch, design-system/Package Consumability]

## Phase 6: Tauri Rust Backend

- [ ] 6.1 Scaffold `apps/desktop/src-tauri/` via `cargo init` or manual: `Cargo.toml` with `tauri` (v2), `tauri-plugin-sql` (sqlite feature), `serde`, `serde_json` — [data-store/IPC Bridge]
- [ ] 6.2 Create `apps/desktop/src-tauri/tauri.conf.json`: `devUrl: http://localhost:5173`, `frontendDist: ../dist`, window 1024×768 default, 800×600 min, title "EntropIA" — [desktop-app/Window Configuration]
- [ ] 6.3 Create `apps/desktop/src-tauri/src/lib.rs`: register `tauri-plugin-sql`, Tauri Builder setup with `.plugin(tauri_plugin_sql::Builder::new().build())` — [data-store/IPC Bridge]
- [ ] 6.4 Create `apps/desktop/src-tauri/capabilities/default.json`: allow `sql` plugin IPC + window defaults — [data-store/IPC Bridge]
- [ ] 6.5 Create `apps/desktop/src-tauri/build.rs`: standard Tauri build script — [desktop-app/App Launch]

## Phase 7: CI Pipeline

- [ ] 7.1 Create `.github/workflows/ci.yml`: trigger on push `main` + PRs to `main`, `ubuntu-latest`, PNPM cache via `pnpm/action-setup`, steps: install → `turbo run lint check-types test` — [ci/CI Triggers, ci/Quality Jobs, ci/Dependency Caching, ci/Linux-Only Matrix]
- [ ] 7.2 Add `lint` and `check-types` scripts to each package.json (ui, store, desktop) — [ci/Quality Jobs]

## Phase 8: Integration & Validation

- [ ] 8.1 Run `pnpm install` from root — verify single lockfile, all 3 packages linked — [monorepo/Workspace Configuration]
- [ ] 8.2 Run `pnpm dev` — verify Tauri window opens, Svelte renders, title "EntropIA", 1024×768 — [desktop-app/App Launch, desktop-app/Window Configuration]
- [ ] 8.3 Verify DB: check `entropia.db` created in appDataDir, `_drizzle_migrations` populated, 5 tables exist — [data-store/Database File Creation, data-store/Migration Runner]
- [ ] 8.4 Modify `apps/desktop/src/App.svelte` — verify HMR updates within 2s, no full reload — [desktop-app/Hot Reload — Local Sources]
- [ ] 8.5 Run `pnpm lint` and `pnpm check-types` — verify clean pass across all packages — [ci/Quality Jobs]
