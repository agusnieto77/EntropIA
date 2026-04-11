# Delta for CI

## ADDED Requirements

### Requirement: Test Job

The CI pipeline MUST run `pnpm test` as a job in the workflow. The test job MUST fail the pipeline if any test fails.

#### Scenario: Tests pass in CI

- GIVEN all Vitest tests pass
- WHEN the CI pipeline runs the test job
- THEN the test job succeeds
- AND the overall pipeline status includes the test result

#### Scenario: Test failure blocks merge

- GIVEN a PR introduces a failing test
- WHEN the CI pipeline runs
- THEN the test job fails
- AND the overall pipeline status is "failed"

## MODIFIED Requirements

### Requirement: Quality Jobs

The CI pipeline MUST run lint (ESLint), typecheck (`tsc` + `svelte-check`), and unit tests (Vitest). All jobs MUST fail fast — any single failure MUST fail the entire pipeline.
(Previously: Unit tests ran only "when test scripts exist" — now Vitest is always present and MUST run.)

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
