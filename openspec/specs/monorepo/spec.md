# Monorepo Specification

## Purpose

Defines the PNPM workspace and Turborepo configuration that enables multi-package development, task orchestration, and build caching for the EntropIA monorepo.

## Requirements

### Requirement: Workspace Configuration

The root `package.json` MUST declare workspace packages via `pnpm-workspace.yaml` covering `apps/*` and `packages/*`. The root `package.json` MUST be `private: true` and declare a `packageManager` field pinning the PNPM version.

#### Scenario: Workspace resolves all packages

- GIVEN the monorepo root with `pnpm-workspace.yaml` listing `apps/*` and `packages/*`
- WHEN `pnpm install` is run from the root
- THEN all packages in `apps/desktop`, `packages/ui`, and `packages/store` are linked
- AND a single `pnpm-lock.yaml` is generated at the root

#### Scenario: Inter-package dependencies use workspace protocol

- GIVEN `apps/desktop` depends on `@entropia/ui` and `@entropia/store`
- WHEN dependencies are declared with `workspace:*` protocol
- THEN PNPM resolves them to the local packages without publishing

### Requirement: Turborepo Pipeline

`turbo.json` MUST define pipelines for `build`, `dev`, `lint`, `check-types`, and `test`. The `build` pipeline MUST depend on `^build` (topological). The `dev` pipeline MUST be `persistent: true` and uncached. The `test` pipeline MUST be defined with `dependsOn: ["^build"]` and cache outputs in `coverage/**`.

#### Scenario: Build respects dependency order

- GIVEN `apps/desktop` depends on `packages/ui` and `packages/store`
- WHEN `turbo run build` is executed
- THEN `packages/ui` and `packages/store` build BEFORE `apps/desktop`

#### Scenario: Dev runs all packages in parallel

- GIVEN all three packages define a `dev` script
- WHEN `pnpm dev` (which runs `turbo run dev`) is executed from the root
- THEN all `dev` tasks start concurrently in a single terminal session

#### Scenario: Lint and typecheck run without build dependency

- GIVEN the `lint` and `check-types` pipelines have no `dependsOn` build requirement
- WHEN `turbo run lint check-types` is executed
- THEN both tasks run in parallel across all packages

### Requirement: Root Scripts

The root `package.json` MUST expose scripts: `dev`, `build`, `lint`, `check-types`, and `test` — each delegating to `turbo run <task>`.

#### Scenario: Root scripts delegate to Turborepo

- GIVEN the root `package.json` has `"dev": "turbo run dev"`
- WHEN `pnpm dev` is run from the root
- THEN Turborepo orchestrates the `dev` task across all packages
