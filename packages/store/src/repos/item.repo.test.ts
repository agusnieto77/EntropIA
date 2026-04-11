import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ItemRepo } from './item.repo'
import type { DrizzleClient } from '../types'
import type { DbClient } from '../types'

// Helper: create a chainable mock that resolves with the given value
function createChainMock(resolveValue: unknown = []) {
  const chain: Record<string, ReturnType<typeof vi.fn>> = {}

  const createProxy = (): unknown =>
    new Proxy(() => {}, {
      apply: () => (resolveValue instanceof Promise ? resolveValue : Promise.resolve(resolveValue)),
      get: (_target, prop) => {
        if (prop === 'then') {
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

describe('ItemRepo', () => {
  let db: ReturnType<typeof createMockDrizzle>
  let repo: ItemRepo

  beforeEach(() => {
    db = createMockDrizzle()
    repo = new ItemRepo(db.db)
  })

  describe('create', () => {
    it('returns an item with generated id, timestamps and correct fields', async () => {
      const now = Date.now()
      const mockItem = {
        id: 'item-1',
        title: 'Test Document',
        collectionId: 'col-1',
        metadata: null,
        createdAt: now,
        updatedAt: now,
      }

      const returningMock = vi.fn().mockResolvedValue([mockItem])
      const valuesMock = vi.fn().mockReturnValue({ returning: returningMock })
      db.mocks.insert.chain['values'] = valuesMock

      const result = await repo.create({
        title: 'Test Document',
        collectionId: 'col-1',
      })

      expect(result).toEqual(mockItem)
      expect(result.id).toBe('item-1')
      expect(result.title).toBe('Test Document')
      expect(result.collectionId).toBe('col-1')
    })

    it('includes metadata when provided', async () => {
      const meta = JSON.stringify({ author: 'Jane' })
      const mockItem = {
        id: 'item-2',
        title: 'With Metadata',
        collectionId: 'col-1',
        metadata: meta,
        createdAt: 100,
        updatedAt: 100,
      }

      const returningMock = vi.fn().mockResolvedValue([mockItem])
      const valuesMock = vi.fn().mockReturnValue({ returning: returningMock })
      db.mocks.insert.chain['values'] = valuesMock

      const result = await repo.create({
        title: 'With Metadata',
        collectionId: 'col-1',
        metadata: meta,
      })

      expect(result.metadata).toBe(meta)
    })
  })

  describe('findByCollection', () => {
    it('returns empty array when collection has no items', async () => {
      const result = await repo.findByCollection('empty-col')
      expect(result).toEqual([])
    })

    it('returns items for a specific collection', async () => {
      const items = [
        {
          id: 'i1',
          title: 'Doc A',
          collectionId: 'col-1',
          metadata: null,
          createdAt: 100,
          updatedAt: 200,
        },
        {
          id: 'i2',
          title: 'Doc B',
          collectionId: 'col-1',
          metadata: null,
          createdAt: 150,
          updatedAt: 250,
        },
      ]

      const selectResult = createChainMock(items)
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findByCollection('col-1')
      expect(result).toEqual(items)
      expect(result).toHaveLength(2)
    })
  })

  describe('findById', () => {
    it('returns null when item not found', async () => {
      const selectResult = createChainMock([])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findById('non-existent')
      expect(result).toBeNull()
    })

    it('returns the item when found', async () => {
      const item = {
        id: 'found-1',
        title: 'Found Item',
        collectionId: 'col-1',
        metadata: null,
        createdAt: 1,
        updatedAt: 2,
      }
      const selectResult = createChainMock([item])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findById('found-1')
      expect(result).toEqual(item)
      expect(result!.id).toBe('found-1')
    })
  })

  describe('update', () => {
    it('returns updated item with new title and updatedAt', async () => {
      const updated = {
        id: 'u1',
        title: 'New Title',
        collectionId: 'col-1',
        metadata: null,
        createdAt: 100,
        updatedAt: 999,
      }

      const returningMock = vi.fn().mockResolvedValue([updated])
      const whereMock = vi.fn().mockReturnValue({ returning: returningMock })
      const setMock = vi.fn().mockReturnValue({ where: whereMock })
      db.mocks.update.chain['set'] = setMock

      const result = await repo.update('u1', { title: 'New Title' })
      expect(result).toEqual(updated)
      expect(result.title).toBe('New Title')
    })

    it('updates metadata field', async () => {
      const newMeta = JSON.stringify({ tags: ['important'] })
      const updated = {
        id: 'u2',
        title: 'Same',
        collectionId: 'col-1',
        metadata: newMeta,
        createdAt: 100,
        updatedAt: 999,
      }

      const returningMock = vi.fn().mockResolvedValue([updated])
      const whereMock = vi.fn().mockReturnValue({ returning: returningMock })
      const setMock = vi.fn().mockReturnValue({ where: whereMock })
      db.mocks.update.chain['set'] = setMock

      const result = await repo.update('u2', { metadata: newMeta })
      expect(result.metadata).toBe(newMeta)
    })
  })

  describe('delete', () => {
    it('completes without error', async () => {
      await expect(repo.delete('del-1')).resolves.toBeUndefined()
    })
  })

  describe('searchByText', () => {
    it('returns empty when no matches found', async () => {
      const selectResult = createChainMock([])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.searchByText('col-1', 'nonexistent')
      expect(result).toEqual([])
    })

    it('returns matching items for the collection', async () => {
      const matchingItems = [
        {
          id: 'i1',
          title: 'Machine Learning Paper',
          collectionId: 'col-1',
          metadata: null,
          createdAt: 100,
          updatedAt: 200,
        },
      ]

      const selectResult = createChainMock(matchingItems)
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.searchByText('col-1', 'machine')
      expect(result).toEqual(matchingItems)
      expect(result).toHaveLength(1)
      expect(result[0]!.title).toBe('Machine Learning Paper')
    })

    it('returns items matching metadata field', async () => {
      const matchingItems = [
        {
          id: 'i2',
          title: 'Untitled Document',
          collectionId: 'col-1',
          metadata: JSON.stringify({ author: 'Darwin', year: '1859' }),
          createdAt: 100,
          updatedAt: 200,
        },
      ]

      const selectResult = createChainMock(matchingItems)
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.searchByText('col-1', 'Darwin')
      expect(result).toHaveLength(1)
      expect(result[0]!.metadata).toContain('Darwin')
    })
  })

  describe('searchByFts5', () => {
    it('returns FtsResult[] with itemId and rank from FTS5 search', async () => {
      const rawClient = {
        execute: vi.fn().mockResolvedValue({ rowsAffected: 0 }),
        select: vi.fn().mockResolvedValue([
          { item_id: 'item-1', rank: -0.5 },
          { item_id: 'item-2', rank: -1.2 },
        ]),
      } as unknown as DbClient

      const repo2 = new ItemRepo(db.db, rawClient)
      const results = await repo2.searchByFts5('cabildo')
      expect(results).toHaveLength(2)
      expect(results[0]!.itemId).toBe('item-1')
      expect(results[1]!.itemId).toBe('item-2')
    })

    it('returns empty array when FTS5 finds no matches', async () => {
      const rawClient = {
        execute: vi.fn().mockResolvedValue({ rowsAffected: 0 }),
        select: vi.fn().mockResolvedValue([]),
      } as unknown as DbClient

      const repo2 = new ItemRepo(db.db, rawClient)
      const results = await repo2.searchByFts5('xyznonexistentterm')
      expect(results).toEqual([])
    })

    it('falls back to LIKE search when rawClient is not provided', async () => {
      const matchingItems = [
        {
          id: 'i1',
          title: 'Acta de cabildo',
          collectionId: 'col-1',
          metadata: null,
          createdAt: 100,
          updatedAt: 200,
        },
      ]

      const selectResult = createChainMock(matchingItems)
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      // No rawClient — uses LIKE fallback
      const result = await repo.searchByText('col-1', 'cabildo')
      expect(result).toHaveLength(1)
      expect(result[0]!.title).toBe('Acta de cabildo')
    })

    it('returns empty for empty query in FTS5 path', async () => {
      const rawClient = {
        execute: vi.fn().mockResolvedValue({ rowsAffected: 0 }),
        select: vi.fn().mockResolvedValue([]),
      } as unknown as DbClient

      const repo2 = new ItemRepo(db.db, rawClient)
      const results = await repo2.searchByFts5('')
      expect(results).toEqual([])
    })
  })

  describe('searchByText with FTS5 integration', () => {
    it('uses FTS5 results when rawClient is provided and FTS5 returns matches', async () => {
      // FTS5 returns specific item IDs
      const rawSelectMock = vi.fn().mockResolvedValue([
        { item_id: 'item-fts-1', rank: -0.5 },
        { item_id: 'item-fts-2', rank: -1.0 },
      ])
      const rawClient = {
        execute: vi.fn().mockResolvedValue({ rowsAffected: 0 }),
        select: rawSelectMock,
      } as unknown as DbClient

      const ftsItem1 = {
        id: 'item-fts-1',
        title: 'Acta notarial de cabildo',
        collectionId: 'col-1',
        metadata: null,
        createdAt: 100,
        updatedAt: 200,
      }
      const ftsItem2 = {
        id: 'item-fts-2',
        title: 'Documento de cabildo',
        collectionId: 'col-1',
        metadata: null,
        createdAt: 50,
        updatedAt: 150,
      }

      // Drizzle mock returns the two items (for the follow-up findByIds query)
      const selectResult = createChainMock([ftsItem1, ftsItem2])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const repo2 = new ItemRepo(db.db, rawClient)
      const results = await repo2.searchByText('col-1', 'cabildo')

      // FTS5 was called (rawClient.select was invoked)
      expect(rawSelectMock).toHaveBeenCalled()
      // Results contain both FTS5-matched items
      expect(results).toHaveLength(2)
    })

    it('falls back to LIKE when FTS5 returns no results', async () => {
      // FTS5 returns nothing
      const rawSelectMock = vi.fn().mockResolvedValue([])
      const rawClient = {
        execute: vi.fn().mockResolvedValue({ rowsAffected: 0 }),
        select: rawSelectMock,
      } as unknown as DbClient

      const likeItems = [
        {
          id: 'like-1',
          title: 'Rare Document',
          collectionId: 'col-1',
          metadata: null,
          createdAt: 100,
          updatedAt: 200,
        },
      ]
      const selectResult = createChainMock(likeItems)
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const repo2 = new ItemRepo(db.db, rawClient)
      const results = await repo2.searchByText('col-1', 'rare')

      // FTS5 was tried (rawClient.select was invoked)
      expect(rawSelectMock).toHaveBeenCalled()
      // FTS5 returned nothing, so Drizzle LIKE fallback was used
      expect(results).toHaveLength(1)
      expect(results[0]!.id).toBe('like-1')
    })
  })
})
