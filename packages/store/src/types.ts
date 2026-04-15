import type { SqliteRemoteDatabase } from 'drizzle-orm/sqlite-proxy'

/**
 * Low-level database client interface that wraps Tauri IPC calls.
 * Used by both the migration runner and the Drizzle sqlite-proxy adapter.
 */
export interface DbClient {
  /** Execute a write query (INSERT, UPDATE, DELETE, DDL). */
  execute(sql: string, params?: unknown[]): Promise<{ rowsAffected: number }>
  /** Execute multiple SQL statements atomically within a transaction. */
  executeBatch(sql: string): Promise<void>
  /** Execute a read query (SELECT) and return typed rows. */
  select<T = Record<string, unknown>>(sql: string, params?: unknown[]): Promise<T[]>
}

/**
 * Drizzle ORM client type for sqlite-proxy mode.
 * Used by repository classes for typed query building.
 */
export type DrizzleClient = SqliteRemoteDatabase
