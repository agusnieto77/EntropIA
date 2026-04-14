import { describe, it, expect, vi, beforeEach } from 'vitest'
import { CollectionRepo } from './collection.repo'
import type { DrizzleClient } from '../types'

/**
 * CollectionRepo tests.
 * Strategy: mock the DrizzleClient's Drizzle query-builder chain.
 * We build a chainable mock that captures method calls and returns
 * predictable data, then assert the repo returns the correct shape.
 */

// Helper: create a chainable mock that resolves with the given value
function createChainMock(resolveValue: unknown = []) {
  const chain: Record<string, ReturnType<typeof vi.fn>> = {}

  const createProxy = (): unknown =>
    new Proxy(() => {}, {
      apply: () => (resolveValue instanceof Promise ? resolveValue : Promise.resolve(resolveValue)),
      get: (_target, prop) => {
        if (prop === 'then') {
          // Make the proxy thenable so `await chain` resolves
          return (resolve: (v: unknown) => void) => resolve(resolveValue)
        }
        if (!chain[prop as string]) {
          chain[prop as string] = vi.fn().mockReturnValue(createProxy())
        }
        return chain[prop as string]
      },
    })

  return { proxy: createProxy(), chain }
}

function createMockDrizzle() {
  const selectMock = createChainMock([])
  const insertMock = createChainMock([])
  const updateMock = createChainMock([])
  const deleteMock = createChainMock([])

  const db = {
    select: vi.fn().mockReturnValue(selectMock.proxy),
    insert: vi.fn().mockReturnValue(insertMock.proxy),
    update: vi.fn().mockReturnValue(updateMock.proxy),
    delete: vi.fn().mockReturnValue(deleteMock.proxy),
  } as unknown as DrizzleClient

  return {
    db,
    mocks: {
      select: selectMock,
      insert: insertMock,
      update: updateMock,
      delete: deleteMock,
    },
  }
}

describe('CollectionRepo', () => {
  let db: ReturnType<typeof createMockDrizzle>
  let repo: CollectionRepo

  beforeEach(() => {
    db = createMockDrizzle()
    repo = new CollectionRepo(db.db)
  })

  describe('create', () => {
    it('returns a collection with generated id and timestamps', async () => {
      const valuesMock = vi.fn().mockResolvedValue(undefined)
      db.mocks.insert.chain['values'] = valuesMock

      const result = await repo.create({
        name: 'Research Papers',
        description: 'My papers',
      })

      expect(result.name).toBe('Research Papers')
      expect(result.description).toBe('My papers')
      expect(result.id).toBeTruthy()
      expect(typeof result.id).toBe('string')
      expect(result.createdAt).toBeTypeOf('number')
      expect(result.updatedAt).toBeTypeOf('number')
      expect(result.updatedAt).toBe(result.createdAt)

      expect(valuesMock).toHaveBeenCalledTimes(1)
      const insertedCollection = valuesMock.mock.calls[0]![0]
      expect(insertedCollection).toEqual(result)
    })

    it('generates unique ids for different collections', async () => {
      const ids: string[] = []
      const valuesMock = vi.fn().mockImplementation(async () => {
        // Extract the id from the call to values()
        const data = valuesMock.mock.calls[valuesMock.mock.calls.length - 1]![0]
        ids.push(data.id)
      })
      db.mocks.insert.chain['values'] = valuesMock

      await repo.create({ name: 'Collection A' })
      await repo.create({ name: 'Collection B' })

      expect(ids).toHaveLength(2)
      expect(ids[0]).not.toBe(ids[1])
      // IDs should be non-empty strings
      expect(ids[0]!.length).toBeGreaterThan(0)
      expect(ids[1]!.length).toBeGreaterThan(0)
    })
  })

  describe('findAll', () => {
    it('returns empty array when no collections exist', async () => {
      // Default mock resolves with []
      const result = await repo.findAll()
      expect(result).toEqual([])
    })

    it('returns collections sorted by updated_at desc', async () => {
      const collections = [
        { id: '1', name: 'Newer', description: null, createdAt: 100, updatedAt: 200 },
        { id: '2', name: 'Older', description: null, createdAt: 50, updatedAt: 100 },
      ]

      // Reconfigure select mock to return collections
      const selectResult = createChainMock(collections)
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findAll()
      expect(result).toEqual(collections)
      expect(result).toHaveLength(2)
      expect(result[0]!.name).toBe('Newer')
    })
  })

  describe('findById', () => {
    it('returns null when collection not found', async () => {
      const selectResult = createChainMock([])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findById('non-existent')
      expect(result).toBeNull()
    })

    it('returns the collection when found', async () => {
      const collection = { id: 'abc', name: 'Found', description: null, createdAt: 1, updatedAt: 2 }
      const selectResult = createChainMock([collection])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findById('abc')
      expect(result).toEqual(collection)
      expect(result!.id).toBe('abc')
    })
  })

  describe('update', () => {
    it('returns updated collection with new updatedAt', async () => {
      const updated = {
        id: 'u1',
        name: 'Updated Name',
        description: null,
        createdAt: 100,
        updatedAt: 999,
      }

      const returningMock = vi.fn().mockResolvedValue([updated])
      const whereMock = vi.fn().mockReturnValue({ returning: returningMock })
      const setMock = vi.fn().mockReturnValue({ where: whereMock })
      db.mocks.update.chain['set'] = setMock

      const result = await repo.update('u1', { name: 'Updated Name' })
      expect(result).toEqual(updated)
      expect(result.name).toBe('Updated Name')
    })
  })

  describe('delete', () => {
    it('completes without error', async () => {
      // delete().where() resolves
      await expect(repo.delete('del-1')).resolves.toBeUndefined()
    })
  })

  describe('countItems', () => {
    it('returns 0 when collection has no items', async () => {
      const selectResult = createChainMock([{ count: 0 }])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.countItems('empty-col')
      expect(result).toBe(0)
    })

    it('returns correct count when collection has items', async () => {
      const selectResult = createChainMock([{ count: 5 }])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.countItems('col-with-items')
      expect(result).toBe(5)
    })
  })
})
