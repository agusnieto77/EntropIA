# Specifications: Fase 0 — Fundaciones

> **Change**: fase-0-fundaciones
> **Date**: 2026-04-11
> **Status**: Complete — ready for design
> **Domains**: monorepo, desktop-app, data-store, design-system, ci

---

## Done Criteria

1. `pnpm dev` starts the desktop app with hot reload across packages
2. DB migrates on first launch, base schema is queryable via Drizzle
3. CI passes lint + typecheck on a clean checkout

---

## Domain Specs

Detailed specs per domain are in `specs/{domain}/spec.md`. Summary below.

### Monorepo (3 requirements, 5 scenarios)

| Requirement | Strength | Scenarios |
|-------------|----------|-----------|
| Workspace Configuration | MUST | 2 — install resolves all packages; workspace:* protocol links |
| Turborepo Pipeline | MUST | 3 — topological build; parallel dev; parallel lint+typecheck |
| Root Scripts | MUST | 1 — root scripts delegate to turbo |

### Desktop App (4 requirements, 5 scenarios)

| Requirement | Strength | Scenarios |
|-------------|----------|-----------|
| App Launch | MUST | 2 — first launch renders Svelte root; dev mode uses Vite |
| Hot Reload — Local Sources | MUST | 1 — component change reflects via HMR |
| Hot Reload — Cross-Package | SHOULD | 1 — UI package change triggers rebuild |
| Window Configuration | MUST/SHOULD | 1 — title, size, decorations |

### Data Store (6 requirements, 9 scenarios)

| Requirement | Strength | Scenarios |
|-------------|----------|-----------|
| Database File Creation | MUST | 2 — first launch creates; subsequent reuses |
| IPC Bridge | MUST | 2 — select returns rows; execute writes |
| Drizzle sqlite-proxy Client | MUST | 1 — queries use IPC bridge |
| Base Schema | MUST | 2 — 5 tables defined; foreign keys |
| Migration Runner | MUST | 2 — pending applied in order; no-op if current |
| Migration Idempotency | MUST | 2 — re-run safe; interrupted recoverable |

### Design System (3 requirements, 5 scenarios)

| Requirement | Strength | Scenarios |
|-------------|----------|-----------|
| CSS Design Tokens | MUST | 2 — variables resolve; files organized by category |
| Base Components | MUST | 2 — Button uses tokens; TS types exported |
| Package Consumability | MUST | 2 — desktop imports components; CSS importable |

### CI (4 requirements, 5 scenarios)

| Requirement | Strength | Scenarios |
|-------------|----------|-----------|
| CI Triggers | MUST | 2 — push to main; pull requests |
| Quality Jobs | MUST | 3 — lint blocks; typecheck blocks; clean passes |
| Dependency Caching | SHOULD | 1 — cached PNPM store |
| Linux-Only Matrix | MUST | 1 — ubuntu-latest only |

---

## Totals

- **5 domains**, **20 requirements**, **29 scenarios**
- **Happy paths**: covered for all requirements
- **Edge cases**: interrupted migration, no-op migration, re-run safety
- **Error states**: lint failure, typecheck failure, partial migration recovery
