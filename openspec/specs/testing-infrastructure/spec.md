# Testing Infrastructure Specification

## Purpose

Defines the Vitest testing setup across the EntropIA monorepo, enabling Strict TDD for all new code in Fase 1 and beyond.

## Requirements

### Requirement: Monorepo Test Runner

Vitest MUST be installed as a root devDependency. A `vitest.workspace.ts` at the repository root MUST define all testable projects (`packages/store`, `packages/ui`, `apps/desktop`).

#### Scenario: Run all tests from root

- GIVEN `vitest.workspace.ts` exists at the repo root
- WHEN the user runs `pnpm test` from the repo root
- THEN Vitest discovers and runs tests in `packages/store`, `packages/ui`, and `apps/desktop`
- AND the exit code is 0 when all tests pass

#### Scenario: Run tests for a single package

- GIVEN a developer is working on `packages/store`
- WHEN they run `pnpm test` inside `packages/store`
- THEN only the store package tests execute

### Requirement: Per-Package Configuration

Each testable package MUST have its own `vitest.config.ts` that specifies the appropriate test environment.

#### Scenario: Store package uses node environment

- GIVEN `packages/store/vitest.config.ts` exists
- WHEN store tests run
- THEN the test environment is `node` (no DOM APIs)

#### Scenario: UI package uses happy-dom environment

- GIVEN `packages/ui/vitest.config.ts` exists
- WHEN UI component tests run
- THEN the test environment is `happy-dom`

#### Scenario: Desktop app uses happy-dom environment

- GIVEN `apps/desktop/vitest.config.ts` exists
- WHEN desktop tests run
- THEN the test environment is `happy-dom`
- AND Tauri API modules are mockable

### Requirement: Turborepo Integration

The root `turbo.json` MUST define a `test` pipeline task so that `pnpm test` at the root runs all package tests via Turborepo's task runner.

#### Scenario: Turborepo runs tests in dependency order

- GIVEN `turbo.json` has a `test` task defined
- WHEN `pnpm test` runs at the root
- THEN Turborepo executes test tasks respecting package dependency order
- AND caches results when inputs have not changed

### Requirement: Store Test Mocking Strategy

Store package tests MUST mock the `DbClient` interface (not Tauri `invoke` directly). A mock `DbClient` that records calls and returns configurable results MUST be usable in all repository tests.

#### Scenario: Repository test uses mock DbClient

- GIVEN a mock `DbClient` with `execute` and `select` stubs
- WHEN a repository method is called in a test
- THEN the repository delegates to the mock `DbClient`
- AND no Tauri runtime is required

#### Scenario: Mock DbClient returns configured data

- GIVEN `select` is stubbed to return `[{ id: '1', name: 'Test' }]`
- WHEN `CollectionRepo.findAll()` is called
- THEN it returns the stubbed data

### Requirement: Component Test Utilities

Component tests in `packages/ui` and `apps/desktop` MUST use `@testing-library/svelte` for rendering and querying. Tauri API modules (`@tauri-apps/api/core`, `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-fs`) MUST be mocked at the module level via `vi.mock()`.

#### Scenario: Svelte component renders in test

- GIVEN a Svelte component from `packages/ui`
- WHEN rendered with `@testing-library/svelte` `render()`
- THEN the component mounts in `happy-dom`
- AND DOM queries (getByText, getByRole) work correctly

#### Scenario: Tauri APIs are mocked in component tests

- GIVEN a component that calls `convertFileSrc()`
- WHEN tested with `vi.mock('@tauri-apps/api/core')`
- THEN the mock returns a test-safe URL
- AND no Tauri runtime error occurs

### Requirement: Coverage Reporting

Vitest MUST support generating coverage reports via `vitest --coverage`. Coverage SHOULD be available per-package.

#### Scenario: Coverage report generated

- GIVEN tests exist for `packages/store`
- WHEN `pnpm test -- --coverage` runs
- THEN a coverage report is generated showing line, branch, and function coverage
