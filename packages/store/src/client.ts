import { invoke } from '@tauri-apps/api/core'
import { drizzle } from 'drizzle-orm/sqlite-proxy'
import type { DbClient } from './types'

/**
 * Creates a low-level database client that bridges TypeScript ↔ Rust via Tauri IPC.
 * Every SQL call is forwarded to the Rust backend through `invoke`.
 */
export const createDbClient = (): DbClient => ({
  async execute(sql, params = []) {
    return invoke<{ rowsAffected: number }>('db_execute', { sql, params })
  },
  async select<T = Record<string, unknown>>(sql: string, params: unknown[] = []) {
    return invoke<T[]>('db_select', { sql, params })
  },
})

/**
 * Creates a Drizzle ORM instance in sqlite-proxy mode.
 * Delegates all SQL execution to the provided DbClient (Tauri IPC).
 *
 * - `run` method: used for INSERT/UPDATE/DELETE — executes via client.execute
 * - `all`/`get`/`values` methods: used for SELECT — executes via client.select
 *   and maps rows to value arrays (Drizzle expects `{ rows: unknown[][] }`)
 */
export const createDrizzleClient = (client: DbClient) =>
  drizzle(async (sql, params, method) => {
    if (method === 'run') {
      await client.execute(sql, params as unknown[])
      return { rows: [] }
    }
    const rows = await client.select<Record<string, unknown>>(sql, params as unknown[])
    return { rows: rows.map((row) => Object.values(row)) }
  })
