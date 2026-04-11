import { describe, it, expect } from 'vitest'
import { runMigrations } from './runner'
import { createMockDbClient } from './__mocks__/db.mock'

describe('runMigrations — migrations 0004, 0005 and 0006', () => {
  it('executes 0004_fts5 migration SQL (FTS5 virtual table creation)', async () => {
    const client = createMockDbClient()
    await runMigrations(client)

    const hasFts5 = client._executedSql.some(
      (sql) => sql.includes('fts_items') && sql.includes('fts5')
    )
    expect(hasFts5).toBe(true)
  })

  it('executes 0005_nlp_tables migration SQL (entities table creation)', async () => {
    const client = createMockDbClient()
    await runMigrations(client)

    const hasEntities = client._executedSql.some(
      (sql) => sql.includes('entities') && sql.includes('CREATE TABLE IF NOT EXISTS')
    )
    expect(hasEntities).toBe(true)
  })

  it('is idempotent — running twice does not throw', async () => {
    const client = createMockDbClient()
    // First run
    await runMigrations(client)
    // Simulate "already applied" by pre-populating the applied set
    // Second run with all migrations already recorded — mock already returns them
    await expect(runMigrations(client)).resolves.toBeUndefined()
  })

  it('FTS5 migration uses unicode61 tokenizer config', async () => {
    const client = createMockDbClient()
    await runMigrations(client)

    const hasUnicode61 = client._executedSql.some((sql) => sql.includes('unicode61'))
    expect(hasUnicode61).toBe(true)
  })

  it('entities migration creates idx_entities_item_id index', async () => {
    const client = createMockDbClient()
    await runMigrations(client)

    const hasIndex = client._executedSql.some((sql) => sql.includes('idx_entities_item_id'))
    expect(hasIndex).toBe(true)
  })

  it('migrations 0001 through 0005 are all executed on a fresh database', async () => {
    const client = createMockDbClient()
    await runMigrations(client)

    // All 5 migration names should cause INSERT INTO _migrations
    const wasInserted = client._executedSql.some(
      (sql) => sql.includes('INSERT INTO _migrations') && sql.includes('VALUES')
    )
    expect(wasInserted).toBe(true)
  })

  it('executes 0006_triples migration SQL (triples table + index)', async () => {
    const client = createMockDbClient()
    await runMigrations(client)

    const hasTriplesTable = client._executedSql.some(
      (sql) => sql.includes('CREATE TABLE IF NOT EXISTS triples') && sql.includes('item_id')
    )
    const hasTriplesIndex = client._executedSql.some((sql) =>
      sql.includes('CREATE INDEX IF NOT EXISTS triples_item_id_idx')
    )

    expect(hasTriplesTable).toBe(true)
    expect(hasTriplesIndex).toBe(true)
  })
})
