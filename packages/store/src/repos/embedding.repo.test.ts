import { describe, it, expect, beforeEach } from 'vitest'
import { EmbeddingRepo } from './embedding.repo'
import type { DbClient } from '../types'

// Mock DbClient that uses an in-memory store for fallback embeddings table
function createMockDbClient() {
  const store: Record<string, string> = {}
  const executedSql: string[] = []

  return {
    store,
    _executedSql: executedSql,

    async execute(sql: string, params?: unknown[]) {
      executedSql.push(sql)
      const sqlUpper = sql.trim().toUpperCase()

      if (sqlUpper.startsWith('CREATE TABLE')) {
        // Idempotent table creation
        return { rowsAffected: 0 }
      }

      if (sqlUpper.startsWith('INSERT OR REPLACE')) {
        const itemId = params?.[0] as string
        const embedding = params?.[1] as string
        store[itemId] = embedding
        return { rowsAffected: 1 }
      }

      if (sqlUpper.startsWith('DELETE')) {
        const itemId = params?.[0] as string
        delete store[itemId]
        return { rowsAffected: 1 }
      }

      return { rowsAffected: 0 }
    },

    async select<T>(sql: string, params?: unknown[]): Promise<T[]> {
      executedSql.push(sql)
      const sqlUpper = sql.trim().toUpperCase()

      if (sqlUpper.includes('SELECT') && sqlUpper.includes('FROM')) {
        // knnSearch query: WHERE item_id != ?
        if (sqlUpper.includes('WHERE') && sqlUpper.includes('!=')) {
          const excludeId = params?.[0] as string
          return Object.entries(store)
            .filter(([id]) => id !== excludeId)
            .map(([id, embedding]) => ({ item_id: id, embedding })) as T[]
        }
        // getEmbedding query: WHERE item_id = ?
        if (sqlUpper.includes('WHERE')) {
          const itemId = params?.[0] as string
          if (store[itemId]) {
            return [{ item_id: itemId, embedding: store[itemId] }] as T[]
          }
          return []
        }
      }

      return []
    },
  } satisfies DbClient & { store: Record<string, string>; _executedSql: string[] }
}

describe('EmbeddingRepo', () => {
  let client: ReturnType<typeof createMockDbClient>
  let repo: EmbeddingRepo

  beforeEach(async () => {
    client = createMockDbClient()
    repo = new EmbeddingRepo(client)
    await repo.initialize()
  })

  describe('storeEmbedding', () => {
    it('stores an embedding vector for an item', async () => {
      const embedding = new Array(384).fill(0.1)
      await repo.storeEmbedding('item-1', embedding)

      const result = await repo.getEmbedding('item-1')
      expect(result).not.toBeNull()
      expect(result).toHaveLength(384)
    })

    it('stores different embeddings for different items', async () => {
      const emb1 = new Array(384).fill(0.1)
      const emb2 = new Array(384).fill(0.9)

      await repo.storeEmbedding('item-1', emb1)
      await repo.storeEmbedding('item-2', emb2)

      const r1 = await repo.getEmbedding('item-1')
      const r2 = await repo.getEmbedding('item-2')

      expect(r1![0]).toBeCloseTo(0.1)
      expect(r2![0]).toBeCloseTo(0.9)
    })
  })

  describe('getEmbedding', () => {
    it('returns null when no embedding exists for item', async () => {
      const result = await repo.getEmbedding('non-existent-item')
      expect(result).toBeNull()
    })

    it('returns the stored embedding as number array', async () => {
      const embedding = [0.1, 0.2, 0.3]
      await repo.storeEmbedding('item-1', embedding)

      const result = await repo.getEmbedding('item-1')
      expect(result).not.toBeNull()
      expect(result![0]).toBeCloseTo(0.1)
      expect(result![1]).toBeCloseTo(0.2)
      expect(result![2]).toBeCloseTo(0.3)
    })
  })

  describe('deleteEmbedding', () => {
    it('removes embedding for the given item', async () => {
      await repo.storeEmbedding('item-1', [0.1, 0.2, 0.3])
      await repo.deleteEmbedding('item-1')

      const result = await repo.getEmbedding('item-1')
      expect(result).toBeNull()
    })

    it('resolves without error when deleting non-existent item', async () => {
      await expect(repo.deleteEmbedding('ghost-item')).resolves.toBeUndefined()
    })
  })

  describe('storeEmbedding — update existing', () => {
    it('replaces an existing embedding with a new one', async () => {
      const original = new Array(384).fill(0.1)
      const updated = new Array(384).fill(0.9)

      await repo.storeEmbedding('item-1', original)
      await repo.storeEmbedding('item-1', updated)

      const result = await repo.getEmbedding('item-1')
      expect(result).not.toBeNull()
      // Should have the updated value, not the original
      expect(result![0]).toBeCloseTo(0.9)
    })
  })

  describe('knnSearch', () => {
    it('returns empty array when item has no embedding', async () => {
      const results = await repo.knnSearch('nonexistent-item')
      expect(results).toEqual([])
    })

    it('returns similar items sorted by distance (ascending)', async () => {
      // item-A: [1, 0] — pointing right
      // item-B: [0, 1] — pointing up (max distance from item-A)
      // item-C: [0.9, 0.1] — close to item-A
      await repo.storeEmbedding('item-A', [1, 0])
      await repo.storeEmbedding('item-B', [0, 1])
      await repo.storeEmbedding('item-C', [0.9, 0.1])

      const results = await repo.knnSearch('item-A', 5)

      expect(results).toHaveLength(2) // item-B and item-C (not item-A itself)
      // item-C should be closer to item-A than item-B
      expect(results[0]!.itemId).toBe('item-C')
      expect(results[1]!.itemId).toBe('item-B')
      // Distances must be ascending (item-C < item-B)
      expect(results[0]!.distance).toBeLessThan(results[1]!.distance)
    })

    it('respects the limit parameter', async () => {
      await repo.storeEmbedding('item-origin', [1, 0, 0])
      await repo.storeEmbedding('item-1', [0.9, 0.1, 0])
      await repo.storeEmbedding('item-2', [0.8, 0.2, 0])
      await repo.storeEmbedding('item-3', [0.7, 0.3, 0])

      const results = await repo.knnSearch('item-origin', 2)

      expect(results).toHaveLength(2)
    })
  })
})
