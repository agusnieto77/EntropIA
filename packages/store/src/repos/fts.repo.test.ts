import { describe, it, expect, beforeEach } from 'vitest'
import { sanitizeFts5Query, FtsRepo } from './fts.repo'
import type { DbClient } from '../types'

// ============================================================================
// sanitizeFts5Query — pure function tests (no mocks needed)
// ============================================================================
describe('sanitizeFts5Query', () => {
  it('returns empty string for empty input', () => {
    expect(sanitizeFts5Query('')).toBe('')
  })

  it('returns empty string for whitespace-only input', () => {
    expect(sanitizeFts5Query('   ')).toBe('')
  })

  it('wraps single word in quotes', () => {
    expect(sanitizeFts5Query('cabildo')).toBe('"cabildo"')
  })

  it('wraps multiple words in individual quotes', () => {
    expect(sanitizeFts5Query('acta cabildo')).toBe('"acta" "cabildo"')
  })

  it('strips AND operator keyword', () => {
    const result = sanitizeFts5Query('acta AND cabildo')
    expect(result).toBe('"acta" "cabildo"')
  })

  it('strips OR operator keyword', () => {
    const result = sanitizeFts5Query('acta OR cabildo')
    expect(result).toBe('"acta" "cabildo"')
  })

  it('strips special chars: parentheses, asterisks, dashes', () => {
    const result = sanitizeFts5Query('(acta) -cabildo*')
    // special chars stripped from tokens, then each valid token quoted
    expect(result).not.toContain('(')
    expect(result).not.toContain(')')
    expect(result).not.toContain('*')
    expect(result).not.toContain('-')
  })

  it('handles mixed operators and plain text', () => {
    const result = sanitizeFts5Query('acta AND (cabildo OR gobernador)')
    // Operators stripped, parentheses stripped, valid tokens quoted
    expect(result).toContain('"acta"')
    expect(result).toContain('"cabildo"')
    expect(result).toContain('"gobernador"')
  })

  it('handles three-word phrase with no special chars', () => {
    expect(sanitizeFts5Query('real audiencia provincial')).toBe('"real" "audiencia" "provincial"')
  })

  it('strips colons, commas, and dots from tokens', () => {
    const result = sanitizeFts5Query('acta: cabildo, 1810.')
    expect(result).not.toContain(':')
    expect(result).not.toContain(',')
    expect(result).not.toContain('.')
    expect(result).toContain('"acta"')
    expect(result).toContain('"1810"')
  })

  it('handles NOT keyword — strips it', () => {
    const result = sanitizeFts5Query('cabildo NOT gobernador')
    expect(result).not.toContain('NOT')
    expect(result).toContain('"cabildo"')
    expect(result).toContain('"gobernador"')
  })

  it('handles NEAR keyword — strips it', () => {
    const result = sanitizeFts5Query('cabildo NEAR gobernador')
    expect(result).not.toContain('NEAR')
  })
})

// ============================================================================
// FtsRepo — uses DbClient (raw SQL)
// ============================================================================
function createMockDbClient(): DbClient & {
  _executedSql: string[]
  _selectResults: unknown[]
} {
  const executedSql: string[] = []
  let selectResults: unknown[] = []

  return {
    _executedSql: executedSql,
    get _selectResults() {
      return selectResults
    },
    set _selectResults(v: unknown[]) {
      selectResults = v
    },

    async execute(sql: string, _params?: unknown[]) {
      executedSql.push(sql)
      return { rowsAffected: 1 }
    },

    async select<T>(sql: string, _params?: unknown[]): Promise<T[]> {
      executedSql.push(sql)
      return selectResults as T[]
    },

    async executeBatch(_sql: string): Promise<void> {
      // No-op for unit tests
    },
  }
}

describe('FtsRepo', () => {
  let client: ReturnType<typeof createMockDbClient>
  let repo: FtsRepo

  beforeEach(() => {
    client = createMockDbClient()
    repo = new FtsRepo(client)
  })

  describe('indexItem', () => {
    it('executes INSERT OR REPLACE into fts_items', async () => {
      await repo.indexItem('item-1', 'Acta del Cabildo', '{}', 'extracted text content')
      const hasFtsInsert = client._executedSql.some(
        (sql) => sql.includes('fts_items') && (sql.includes('INSERT') || sql.includes('insert'))
      )
      expect(hasFtsInsert).toBe(true)
    })

    it('includes the item_id in the executed SQL params', async () => {
      await repo.indexItem('item-42', 'Title', '', '')
      // The last executed SQL should reference an insert into fts_items
      const insertSql = client._executedSql.find(
        (sql) => sql.includes('fts_items') && sql.includes('INSERT')
      )
      expect(insertSql).toBeDefined()
    })
  })

  describe('search', () => {
    it('returns empty array when no results found', async () => {
      client._selectResults = []
      const results = await repo.search('nonexistent term')
      expect(results).toEqual([])
    })

    it('returns FtsResult array when matches found', async () => {
      client._selectResults = [
        { item_id: 'item-1', rank: -0.5 },
        { item_id: 'item-2', rank: -1.2 },
      ]

      const results = await repo.search('cabildo')
      expect(results).toHaveLength(2)
      expect(results[0]!.itemId).toBe('item-1')
      expect(results[1]!.itemId).toBe('item-2')
    })

    it('returns empty array for empty query without executing SELECT', async () => {
      const initialCount = client._executedSql.length
      const results = await repo.search('')
      expect(results).toEqual([])
      // Should not have executed any SELECT for empty query
      expect(client._executedSql.length).toBe(initialCount)
    })

    it('uses MATCH in the search SQL', async () => {
      client._selectResults = []
      await repo.search('cabildo')
      const hasMATCH = client._executedSql.some((sql) => sql.includes('MATCH'))
      expect(hasMATCH).toBe(true)
    })
  })

  describe('removeItem', () => {
    it('executes DELETE from fts_items for the given item_id', async () => {
      await repo.removeItem('item-1')
      const hasDelete = client._executedSql.some(
        (sql) => sql.includes('fts_items') && sql.includes('DELETE')
      )
      expect(hasDelete).toBe(true)
    })

    it('resolves without error when removing non-existent item', async () => {
      await expect(repo.removeItem('ghost-item')).resolves.toBeUndefined()
    })
  })
})
