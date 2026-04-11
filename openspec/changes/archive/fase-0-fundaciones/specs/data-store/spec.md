# Data Store Specification

## Purpose

Defines the SQLite database lifecycle, IPC bridge, Drizzle ORM integration, schema, and migration runner that form the persistence layer of EntropIA.

## Requirements

### Requirement: Database File Creation

The SQLite database file MUST be created automatically in the Tauri `appDataDir` on the application's first launch. Subsequent launches MUST reuse the existing file.

#### Scenario: First launch creates database

- GIVEN the application has never been launched on this machine
- WHEN the app starts for the first time
- THEN a SQLite database file is created inside `appDataDir`
- AND the file persists after the app closes

#### Scenario: Subsequent launch reuses database

- GIVEN a database file already exists in `appDataDir`
- WHEN the app starts again
- THEN the existing database is opened without data loss

### Requirement: IPC Bridge

The Tauri backend MUST expose `execute` and `select` commands via IPC, allowing the JavaScript frontend to run SQL statements against the SQLite database. `execute` is for write operations (INSERT, UPDATE, DELETE, DDL). `select` is for read operations (SELECT).

#### Scenario: Select returns rows

- GIVEN the database contains rows in the `collections` table
- WHEN the frontend invokes the `select` IPC command with a SELECT query
- THEN the command returns an array of row objects

#### Scenario: Execute runs write operations

- GIVEN a valid INSERT statement and parameters
- WHEN the frontend invokes the `execute` IPC command
- THEN the row is inserted into the database
- AND the command returns the number of affected rows

### Requirement: Drizzle sqlite-proxy Client

`packages/store` MUST export a Drizzle client configured with the `sqlite-proxy` adapter. The proxy callbacks MUST delegate to the Tauri IPC bridge for all SQL execution.

#### Scenario: Drizzle query uses IPC bridge

- GIVEN a Drizzle client initialized with `sqlite-proxy`
- WHEN a type-safe query is executed (e.g., `db.select().from(collections)`)
- THEN the generated SQL is sent through the IPC bridge
- AND results are returned as typed objects matching the Drizzle schema

### Requirement: Base Schema

`packages/store` MUST define Drizzle schema tables for: `collections`, `items`, `assets`, `notes`, and `jobs`. All tables MUST use `TEXT` primary keys and include `created_at` timestamps.

#### Scenario: Schema defines all base tables

- GIVEN the Drizzle schema file in `packages/store`
- WHEN the schema is inspected
- THEN tables `collections`, `items`, `assets`, `notes`, and `jobs` are defined
- AND each table has a `TEXT` primary key column named `id`
- AND each table has a `created_at` column

#### Scenario: Foreign key relationships

- GIVEN the schema defines `items.collection_id`, `assets.item_id`, `notes.item_id`, and `jobs.asset_id`
- WHEN these columns are inspected
- THEN each references the `id` column of its parent table

### Requirement: Migration Runner

The app MUST apply pending SQL migration files in sequential order on startup. Migrations MUST be generated at dev time by `drizzle-kit generate` and bundled with the application.

#### Scenario: Pending migrations applied on startup

- GIVEN 3 migration SQL files exist and 1 has already been applied
- WHEN the app starts
- THEN the 2 unapplied migrations are executed in filename order
- AND each is recorded in the migrations tracking table

#### Scenario: No pending migrations is a no-op

- GIVEN all migration files have already been applied
- WHEN the app starts
- THEN no SQL is executed and startup completes normally

### Requirement: Migration Idempotency

The migration runner MUST be safe to re-run. It MUST track applied migrations in a `_drizzle_migrations` table and MUST NOT re-apply already-applied migrations.

#### Scenario: Re-running migrations is safe

- GIVEN all migrations have been applied
- WHEN the migration runner executes again (e.g., on next app start)
- THEN no migrations are re-applied
- AND no errors occur

#### Scenario: Interrupted migration is recoverable

- GIVEN a migration was partially applied (app crashed mid-migration)
- WHEN the app restarts and the migration runner executes
- THEN the runner SHOULD detect the incomplete state
- AND either complete or re-apply the failed migration safely
