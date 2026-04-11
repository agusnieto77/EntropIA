# Proposal: Fase 0 — Fundaciones

## Intent

Bootstrap EntropIA's monorepo with the minimum viable structure to support all future phases. Without this foundation — Tauri shell, type-safe DB layer, design tokens, CI pipeline — no feature work can begin. Fase 0 produces a buildable, lintable, testable skeleton that every subsequent Fase extends.

## Scope

### In Scope
- PNPM workspace + Turborepo configuration (root)
- `apps/desktop` — Tauri 2 + Svelte + Vite shell (window opens, renders "hello")
- `packages/ui` — CSS Custom Properties tokens + 2–3 base Svelte components
- `packages/store` — Drizzle `sqlite-proxy` bridge over `tauri-plugin-sql`, schema stub, migration runner
- Shared tooling: ESLint, Prettier, root `tsconfig.json`
- GitHub Actions CI: lint + typecheck (Linux-only)

### Out of Scope
- `packages/ai-pipeline`, `packages/ner`, `packages/embeddings`, `packages/sync` (Fases 2–4)
- Multi-platform CI matrix / binary builds (Fase 1+)
- Storybook for `packages/ui` (future enhancement)
- `sqlite-vec` extension loading (Fase 3, but validate path exists)
- Cloudflare D1 sync layer (Fase 4)
- Application routing / actual UI screens

## Capabilities

### New Capabilities
- `monorepo-scaffold`: PNPM workspaces + Turborepo task pipeline + shared tooling
- `tauri-shell`: Tauri 2 desktop window with Svelte + Vite frontend
- `design-tokens`: CSS Custom Properties token system + base Svelte components
- `sqlite-store`: Drizzle sqlite-proxy over tauri-plugin-sql + migration runner

### Modified Capabilities
None — greenfield project, no existing specs.

## Approach

1. **Root scaffold**: `pnpm init`, `pnpm-workspace.yaml`, `turbo.json`, shared ESLint/Prettier/tsconfig
2. **`apps/desktop`**: `pnpm create vite` (Svelte template) → `pnpm tauri init` inside it. Configure `tauri.conf.json` paths for monorepo. Add `tauri-plugin-sql` to Cargo deps.
3. **`packages/ui`**: `svelte-package` build. CSS token files (`colors.css`, `typography.css`, `spacing.css`) + `Button`, `Input`, `Card` components with scoped styles.
4. **`packages/store`**: Drizzle schema stub (`collections`, `items` tables). `sqlite-proxy` wrapper that bridges to `tauri-plugin-sql`. `drizzle-kit generate` config. Custom migration runner that reads SQL files from Tauri `$RESOURCE` dir.
5. **CI**: GitHub Actions workflow — checkout, pnpm install, lint, check-types.
6. **Validation**: `turbo run build` succeeds. Tauri dev server opens a window. `packages/ui` components render in desktop. Store connects to SQLite.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `pnpm-workspace.yaml` | New | Workspace definition |
| `turbo.json` | New | Task pipeline (build, dev, lint, check-types, test) |
| `package.json` (root) | New | Scripts + turbo devDep |
| `tsconfig.json` (root) | New | Shared TS config |
| `eslint.config.js` | New | Shared lint rules |
| `.prettierrc` | New | Code formatting |
| `apps/desktop/` | New | Tauri 2 + Svelte + Vite shell |
| `apps/desktop/src-tauri/` | New | Rust backend with tauri-plugin-sql |
| `packages/ui/` | New | Design tokens + base components |
| `packages/store/` | New | Drizzle schema, proxy bridge, migrations |
| `.github/workflows/ci.yml` | New | CI pipeline |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Drizzle `sqlite-proxy` ergonomics — verbose callback bridge, less docs | Med | Encapsulate in `packages/store/src/db.ts` behind clean API; test bridge in isolation |
| Custom migration runner — interrupted migrations, ordering bugs | Med | Keep runner simple (sequential SQL, `_drizzle_migrations` table); write integration tests early |
| `sqlite-vec` extension path — may need forked plugin or custom Rust cmd | Low | Validate `tauri-plugin-sql` extension loading exists; document findings for Fase 3 |
| Tauri monorepo paths — `tauri.conf.json` relative paths break easily | Med | Pin `frontendDist` + `devUrl` in config; add to CI smoke test |
| Cross-package hot reload — `packages/ui` changes not triggering desktop rebuild | Med | Turborepo `persistent` dev + Vite dep watching; validate in dev workflow before merging |

## Rollback Plan

Fase 0 is greenfield — rollback = delete generated directories and config files. Since there's no existing code to break:
1. `git revert` the Fase 0 branch/commits
2. Repository returns to README-only state
3. No data migrations to undo, no user data at risk

## Dependencies

- **Node.js ≥ 22** (LTS)
- **PNPM ≥ 9**
- **Rust toolchain** (stable, for Tauri 2 build)
- **Tauri 2 CLI** (`@tauri-apps/cli`)
- **Tauri prerequisites** (platform-specific: WebView2 on Windows, webkit2gtk on Linux)

## Success Criteria

- [ ] `pnpm install` resolves all workspace dependencies without errors
- [ ] `turbo run build` completes successfully across all 3 packages
- [ ] `turbo run lint` and `turbo run check-types` pass with zero errors
- [ ] `pnpm tauri dev` opens a native window rendering the Svelte app
- [ ] `packages/ui` components render correctly in `apps/desktop`
- [ ] `packages/store` connects to SQLite via `tauri-plugin-sql` and runs a migration
- [ ] GitHub Actions CI passes on push (lint + typecheck)
- [ ] Cross-package change in `packages/ui` triggers rebuild in `apps/desktop` during dev
