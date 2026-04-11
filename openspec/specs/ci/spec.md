# CI Specification

## Purpose

Defines the GitHub Actions continuous integration pipeline that validates code quality on every push and pull request.

## Requirements

### Requirement: CI Triggers

The CI workflow MUST trigger on pushes to `main` and on all pull requests targeting `main`.

#### Scenario: Push to main triggers CI

- GIVEN a commit is pushed to the `main` branch
- WHEN GitHub Actions processes the event
- THEN the CI workflow runs

#### Scenario: Pull request triggers CI

- GIVEN a pull request is opened or updated targeting `main`
- WHEN GitHub Actions processes the event
- THEN the CI workflow runs

### Requirement: Quality Jobs

The CI pipeline MUST run lint (ESLint), typecheck (`tsc` + `svelte-check`), and unit tests (Vitest). All jobs MUST fail fast — any single failure MUST fail the entire pipeline.

#### Scenario: Lint failure blocks merge

- GIVEN a PR introduces an ESLint violation
- WHEN the CI pipeline runs
- THEN the lint job fails
- AND the overall pipeline status is "failed"

#### Scenario: Typecheck failure blocks merge

- GIVEN a PR introduces a TypeScript or Svelte type error
- WHEN the CI pipeline runs
- THEN the typecheck job fails
- AND the overall pipeline status is "failed"

#### Scenario: Test failure blocks merge

- GIVEN a PR introduces a failing Vitest test
- WHEN the CI pipeline runs
- THEN the test job fails
- AND the overall pipeline status is "failed"

#### Scenario: All checks pass on clean code

- GIVEN a PR with no lint errors, no type errors, and all tests passing
- WHEN the CI pipeline runs
- THEN all jobs (lint, typecheck, test) succeed
- AND the overall pipeline status is "passed"

### Requirement: Test Job

The CI pipeline MUST run `pnpm test` as a job in the workflow. The test job MUST fail the pipeline if any test fails.

#### Scenario: Tests pass in CI

- GIVEN all Vitest tests pass
- WHEN the CI pipeline runs the test job
- THEN the test job succeeds
- AND the overall pipeline status includes the test result

#### Scenario: Test failure blocks merge (standalone)

- GIVEN a PR introduces a failing test
- WHEN the CI pipeline runs
- THEN the test job fails
- AND the overall pipeline status is "failed"

### Requirement: Dependency Caching

The CI pipeline SHOULD use PNPM store caching to avoid re-downloading dependencies on every run.

#### Scenario: Second run uses cached dependencies

- GIVEN a previous CI run cached the PNPM store
- WHEN a new CI run starts with the same lockfile
- THEN `pnpm install` completes faster using the cached store

### Requirement: Linux-Only Matrix

The CI pipeline MUST run on `ubuntu-latest` only for Fase 0. Platform matrix (macOS, Windows) MAY be added in later phases when binary builds are needed.

#### Scenario: CI runs on Linux

- GIVEN the workflow is configured with `runs-on: ubuntu-latest`
- WHEN the CI pipeline executes
- THEN all jobs run on a Linux runner
