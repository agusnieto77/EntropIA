import { describe, it, expect, vi, beforeEach } from 'vitest'
import { AssetRepo } from './asset.repo'
import type { DrizzleClient } from '../types'

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
  const deleteMock = createChainMock([])

  const db = {
    select: vi.fn().mockReturnValue(selectMock.proxy),
    insert: vi.fn().mockReturnValue(insertMock.proxy),
    delete: vi.fn().mockReturnValue(deleteMock.proxy),
  } as unknown as DrizzleClient

  return {
    db,
    mocks: {
      select: selectMock,
      insert: insertMock,
      delete: deleteMock,
    },
  }
}

describe('AssetRepo', () => {
  let db: ReturnType<typeof createMockDrizzle>
  let repo: AssetRepo

  beforeEach(() => {
    db = createMockDrizzle()
    repo = new AssetRepo(db.db)
  })

  describe('create', () => {
    it('returns an asset with generated id and timestamp', async () => {
      const now = Date.now()
      const mockAsset = {
        id: 'asset-1',
        itemId: 'item-1',
        path: '/data/files/paper.pdf',
        type: 'pdf',
        size: 1024,
        createdAt: now,
      }

      const returningMock = vi.fn().mockResolvedValue([mockAsset])
      const valuesMock = vi.fn().mockReturnValue({ returning: returningMock })
      db.mocks.insert.chain['values'] = valuesMock

      const result = await repo.create({
        itemId: 'item-1',
        path: '/data/files/paper.pdf',
        type: 'pdf',
        size: 1024,
      })

      expect(result).toEqual(mockAsset)
      expect(result.id).toBe('asset-1')
      expect(result.path).toBe('/data/files/paper.pdf')
      expect(result.type).toBe('pdf')
      expect(result.size).toBe(1024)
    })

    it('creates asset without size (optional field)', async () => {
      const mockAsset = {
        id: 'asset-2',
        itemId: 'item-1',
        path: '/data/files/photo.jpg',
        type: 'image',
        size: null,
        createdAt: 100,
      }

      const returningMock = vi.fn().mockResolvedValue([mockAsset])
      const valuesMock = vi.fn().mockReturnValue({ returning: returningMock })
      db.mocks.insert.chain['values'] = valuesMock

      const result = await repo.create({
        itemId: 'item-1',
        path: '/data/files/photo.jpg',
        type: 'image',
      })

      expect(result).toEqual(mockAsset)
      expect(result.size).toBeNull()
    })
  })

  describe('findByItem', () => {
    it('returns empty array when item has no assets', async () => {
      const result = await repo.findByItem('no-assets-item')
      expect(result).toEqual([])
    })

    it('returns assets for a specific item', async () => {
      const assets = [
        { id: 'a1', itemId: 'item-1', path: '/a.pdf', type: 'pdf', size: 100, createdAt: 10 },
        { id: 'a2', itemId: 'item-1', path: '/b.jpg', type: 'image', size: 200, createdAt: 20 },
      ]

      const selectResult = createChainMock(assets)
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findByItem('item-1')
      expect(result).toEqual(assets)
      expect(result).toHaveLength(2)
      expect(result[0]!.type).toBe('pdf')
      expect(result[1]!.type).toBe('image')
    })
  })

  describe('findById', () => {
    it('returns null when asset not found', async () => {
      const selectResult = createChainMock([])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findById('non-existent')
      expect(result).toBeNull()
    })

    it('returns the asset when found', async () => {
      const asset = {
        id: 'found-1',
        itemId: 'item-1',
        path: '/doc.pdf',
        type: 'pdf',
        size: 512,
        createdAt: 1,
      }
      const selectResult = createChainMock([asset])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findById('found-1')
      expect(result).toEqual(asset)
      expect(result!.id).toBe('found-1')
      expect(result!.size).toBe(512)
    })
  })

  describe('delete', () => {
    it('completes without error', async () => {
      await expect(repo.delete('del-1')).resolves.toBeUndefined()
    })
  })
})
