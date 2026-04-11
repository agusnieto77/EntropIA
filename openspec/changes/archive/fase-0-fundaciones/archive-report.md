# Archive Report: Fase 0 — Fundaciones

**Change**: fase-0-fundaciones
**Archived**: 2026-04-11
**Verdict**: PASS WITH WARNINGS (0 critical, 2 warnings fixed post-verify, 6 deferred)

---

## Summary

Fase 0 bootstrapped EntropIA's monorepo with a fully buildable, lintable, and type-safe foundation. The change delivered 4 new capabilities across 5 specification domains, producing 47+ files from a previously empty repository (README-only state).

### What Was Built

- **Monorepo scaffold**: PNPM workspaces + Turborepo with topological builds, parallel dev, CI caching
- **Desktop shell**: Tauri 2 + Svelte 5 SPA with Vite, window config (1280x800), HMR support
- **Data store**: SQLite via rusqlite + Drizzle sqlite-proxy, 5-table schema (collections, items, assets, notes, jobs), migration runner with idempotency tracking
- **Design system**: CSS Custom Properties token system (colors, spacing, typography, radius, shadows) + 3 base Svelte 5 components (Button, Input, Card)
- **CI pipeline**: GitHub Actions on push/PR to main, lint + typecheck, PNPM cache, ubuntu-latest

---

## Architecture Decisions (4 ADRs)

| ID      | Decision           | Choice                                       | Rationale                                                                                                                                                                                       |
| ------- | ------------------ | -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ADR-001 | SQLite access      | rusqlite + Drizzle sqlite-proxy              | Full Rust-side control (WAL, pragmas) + TS type safety via IPC. Originally spec'd as tauri-plugin-sql — evolved during implementation to custom rusqlite commands for better lifecycle control. |
| ADR-002 | Frontend framework | Plain Svelte 5 + Vite SPA                    | Desktop app has no SSR/routing needs. Minimal bundle, zero framework overhead.                                                                                                                  |
| ADR-003 | Task orchestration | PNPM workspaces + Turborepo                  | One turbo.json adds caching, topological builds, single `turbo dev` command.                                                                                                                    |
| ADR-004 | Styling approach   | CSS Custom Properties + Svelte scoped styles | Full token control. Works natively with Svelte. Tailwind can layer later.                                                                                                                       |

---

## Files Created (by package)

| Package            | Files   | Key Artifacts                                                                                                                 |
| ------------------ | ------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Root               | ~8      | package.json, pnpm-workspace.yaml, turbo.json, tsconfig.json, eslint.config.js, .prettierrc, .npmrc, .gitignore               |
| apps/desktop       | ~10     | package.json, vite.config.ts, index.html, main.ts, App.svelte, src-tauri/(Cargo.toml, tauri.conf.json, lib.rs, build.rs, db/) |
| packages/ui        | ~8      | package.json, tokens.css, Button.svelte, Input.svelte, Card.svelte, index.ts                                                  |
| packages/store     | ~7      | package.json, schema.ts, client.ts, runner.ts, index.ts, 0001_initial.sql                                                     |
| packages/config-ts | ~3      | package.json, base.json, svelte.json                                                                                          |
| .github            | 1       | workflows/ci.yml                                                                                                              |
| **Total**          | **47+** |                                                                                                                               |

---

## Implementation Stats

- **38 tasks** across 8 phases — all complete
- **5 issues fixed** during validation (version mismatches, missing deps, type errors)
- **2 post-verify fixes**: W-004 (tauri-plugin-sql dead dependency removed), W-005 (capabilities/default.json added)
- **pnpm install**: 153 packages, 0 errors
- **Typecheck**: 0 errors across 4 packages
- **Lint**: 0 errors across 3 packages

---

## Warnings Status

### Fixed (2)

| ID    | Issue                                                | Resolution                        |
| ----- | ---------------------------------------------------- | --------------------------------- |
| W-004 | tauri-plugin-sql registered but bypassed by rusqlite | Removed dead dependency           |
| W-005 | No capabilities/default.json                         | Created Tauri 2 capabilities file |

### Deferred to Fase 1 (6)

| ID    | Issue                                                | Severity | Reason Deferred                                 |
| ----- | ---------------------------------------------------- | -------- | ----------------------------------------------- |
| W-001 | tasks.md checkboxes not updated for Phases 3-8       | Low      | Bookkeeping; engram has authoritative progress  |
| W-002 | Turbo task named `typecheck` vs spec's `check-types` | Low      | Internally consistent; spec naming preference   |
| W-003 | Token CSS consolidated into single file vs 5 files   | Low      | Functionally equivalent; all categories present |
| W-006 | No drizzle.config.ts                                 | Low      | Migrations inlined — valid for Tauri bundling   |
| W-007 | ESLint Svelte plugin commented out                   | Low      | svelte-check covers type safety                 |
| W-008 | pnpm-workspace.yaml includes extra `services/*` glob | Low      | Harmless; matches planned future structure      |

---

## Engram Artifact IDs

| Artifact       | Observation ID | Topic Key                                      |
| -------------- | -------------- | ---------------------------------------------- |
| Proposal       | #7             | sdd/entropia/fase-0-fundaciones/proposal       |
| Spec           | #8             | sdd/entropia/fase-0-fundaciones/spec           |
| Design         | #9             | sdd/entropia/fase-0-fundaciones/design         |
| Tasks          | #10            | sdd/entropia/fase-0-fundaciones/tasks          |
| Verify Report  | #16            | sdd/entropia/fase-0-fundaciones/verify-report  |
| Archive Report | —              | sdd/entropia/fase-0-fundaciones/archive-report |

---

## Specs Synced to Main

All specs were NEW (no merge needed — openspec/specs/ was empty before Fase 0):

| Domain        | Action  | Location                               |
| ------------- | ------- | -------------------------------------- |
| monorepo      | Created | `openspec/specs/monorepo/spec.md`      |
| desktop-app   | Created | `openspec/specs/desktop-app/spec.md`   |
| data-store    | Created | `openspec/specs/data-store/spec.md`    |
| design-system | Created | `openspec/specs/design-system/spec.md` |
| ci            | Created | `openspec/specs/ci/spec.md`            |

---

## Next Change Recommended

### Fase 1 — MVP Documental

**Scope**: Document import, CRUD operations, viewer, basic search.

**Prerequisites met by Fase 0**:

- Monorepo structure with working build pipeline
- Tauri 2 shell with window and IPC bridge
- SQLite database with schema (collections, items, assets, notes, jobs)
- Design system with tokens and base components
- CI pipeline for quality gates

**Expected new work**:

- File import (drag & drop, file picker) → assets table
- Collection/item CRUD UI screens
- Document viewer (PDF, images)
- Full-text search via SQLite FTS5
- Automated tests (Vitest unit, basic integration)
- Address deferred Fase 0 warnings

---

## SDD Cycle Complete

Fase 0 — Fundaciones has been fully:

1. Explored (exploration.md)
2. Proposed (proposal.md)
3. Specified (5 domain specs, 20 requirements, 29 scenarios)
4. Designed (4 ADRs, 32+ file layout, interface contracts)
5. Task-broken (8 phases, 38 tasks)
6. Implemented (47+ files, all tasks complete)
7. Verified (PASS WITH WARNINGS, 0 critical)
8. Archived (this report)

The change is now the audit trail. Main specs are the source of truth.
