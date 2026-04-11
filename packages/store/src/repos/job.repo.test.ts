import { describe, it, expect, vi, beforeEach } from 'vitest'
import { JobRepo } from './job.repo'
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

describe('JobRepo', () => {
  let db: ReturnType<typeof createMockDrizzle>
  let repo: JobRepo

  beforeEach(() => {
    db = createMockDrizzle()
    repo = new JobRepo(db.db)
  })

  describe('create', () => {
    it('returns a job with pending status and correct fields', async () => {
      const now = Date.now()
      const mockJob = {
        id: 'job-1',
        type: 'ocr',
        status: 'pending',
        assetId: 'asset-1',
        result: null,
        error: null,
        createdAt: now,
        updatedAt: now,
      }

      const returningMock = vi.fn().mockResolvedValue([mockJob])
      const valuesMock = vi.fn().mockReturnValue({ returning: returningMock })
      db.mocks.insert.chain['values'] = valuesMock

      const result = await repo.create({ assetId: 'asset-1', type: 'ocr' })

      expect(result).toEqual(mockJob)
      expect(result.status).toBe('pending')
      expect(result.type).toBe('ocr')
      expect(result.assetId).toBe('asset-1')
    })

    it('creates a job with ner type', async () => {
      const now = Date.now()
      const mockJob = {
        id: 'job-2',
        type: 'ner',
        status: 'pending',
        assetId: 'asset-2',
        result: null,
        error: null,
        createdAt: now,
        updatedAt: now,
      }

      const returningMock = vi.fn().mockResolvedValue([mockJob])
      const valuesMock = vi.fn().mockReturnValue({ returning: returningMock })
      db.mocks.insert.chain['values'] = valuesMock

      const result = await repo.create({ assetId: 'asset-2', type: 'ner' })

      expect(result.type).toBe('ner')
      expect(result.status).toBe('pending')
      expect(result.id).toBe('job-2')
    })
  })

  describe('findPending', () => {
    it('returns empty array when no pending jobs exist', async () => {
      const selectResult = createChainMock([])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findPending()
      expect(result).toEqual([])
      expect(result).toHaveLength(0)
    })

    it('returns pending jobs ordered by createdAt ASC', async () => {
      const pendingJobs = [
        {
          id: 'job-old',
          type: 'ocr',
          status: 'pending',
          assetId: 'a1',
          result: null,
          error: null,
          createdAt: 100,
          updatedAt: 100,
        },
        {
          id: 'job-new',
          type: 'ocr',
          status: 'pending',
          assetId: 'a2',
          result: null,
          error: null,
          createdAt: 200,
          updatedAt: 200,
        },
      ]

      const selectResult = createChainMock(pendingJobs)
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findPending()
      expect(result).toEqual(pendingJobs)
      expect(result).toHaveLength(2)
      expect(result[0]!.id).toBe('job-old')
      expect(result[1]!.id).toBe('job-new')
    })
  })

  describe('findByAsset', () => {
    it('returns latest job for the asset', async () => {
      const latestJob = {
        id: 'job-latest',
        type: 'ocr',
        status: 'done',
        assetId: 'asset-1',
        result: null,
        error: null,
        createdAt: 300,
        updatedAt: 400,
      }

      const selectResult = createChainMock([latestJob])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findByAsset('asset-1')
      expect(result).toEqual(latestJob)
      expect(result!.assetId).toBe('asset-1')
    })

    it('returns null when no jobs exist for the asset', async () => {
      const selectResult = createChainMock([])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findByAsset('nonexistent-asset')
      expect(result).toBeNull()
    })
  })

  describe('updateStatus', () => {
    it('updates job status and returns updated job', async () => {
      const updated = {
        id: 'job-1',
        type: 'ocr',
        status: 'running',
        assetId: 'asset-1',
        result: null,
        error: null,
        createdAt: 100,
        updatedAt: 999,
      }

      const returningMock = vi.fn().mockResolvedValue([updated])
      const whereMock = vi.fn().mockReturnValue({ returning: returningMock })
      const setMock = vi.fn().mockReturnValue({ where: whereMock })
      db.mocks.update.chain['set'] = setMock

      const result = await repo.updateStatus('job-1', 'running')
      expect(result).toEqual(updated)
      expect(result.status).toBe('running')
    })

    it('updates status to error with error message', async () => {
      const updated = {
        id: 'job-2',
        type: 'ocr',
        status: 'error',
        assetId: 'asset-1',
        result: null,
        error: 'inference crashed',
        createdAt: 100,
        updatedAt: 999,
      }

      const returningMock = vi.fn().mockResolvedValue([updated])
      const whereMock = vi.fn().mockReturnValue({ returning: returningMock })
      const setMock = vi.fn().mockReturnValue({ where: whereMock })
      db.mocks.update.chain['set'] = setMock

      const result = await repo.updateStatus('job-2', 'error', 'inference crashed')
      expect(result).toEqual(updated)
      expect(result.status).toBe('error')
      expect(result.error).toBe('inference crashed')
    })
  })

  describe('updateProgress', () => {
    it('stores progress in the result JSON field', async () => {
      const updated = {
        id: 'job-1',
        type: 'ocr',
        status: 'running',
        assetId: 'asset-1',
        result: JSON.stringify({ progress: 45 }),
        error: null,
        createdAt: 100,
        updatedAt: 999,
      }

      const returningMock = vi.fn().mockResolvedValue([updated])
      const whereMock = vi.fn().mockReturnValue({ returning: returningMock })
      const setMock = vi.fn().mockReturnValue({ where: whereMock })
      db.mocks.update.chain['set'] = setMock

      const result = await repo.updateProgress('job-1', 45)
      expect(result).toEqual(updated)
      expect(JSON.parse(result.result!)).toEqual({ progress: 45 })
    })

    it('stores zero progress correctly', async () => {
      const updated = {
        id: 'job-2',
        type: 'ocr',
        status: 'running',
        assetId: 'asset-1',
        result: JSON.stringify({ progress: 0 }),
        error: null,
        createdAt: 100,
        updatedAt: 999,
      }

      const returningMock = vi.fn().mockResolvedValue([updated])
      const whereMock = vi.fn().mockReturnValue({ returning: returningMock })
      const setMock = vi.fn().mockReturnValue({ where: whereMock })
      db.mocks.update.chain['set'] = setMock

      const result = await repo.updateProgress('job-2', 0)
      expect(result).toEqual(updated)
      expect(JSON.parse(result.result!)).toEqual({ progress: 0 })
    })
  })

  describe('findById', () => {
    it('returns the job when found', async () => {
      const job = {
        id: 'job-found',
        type: 'ocr',
        status: 'done',
        assetId: 'asset-1',
        result: null,
        error: null,
        createdAt: 100,
        updatedAt: 200,
      }

      const selectResult = createChainMock([job])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findById('job-found')
      expect(result).toEqual(job)
      expect(result!.id).toBe('job-found')
    })

    it('returns null when job not found', async () => {
      const selectResult = createChainMock([])
      ;(db.db.select as ReturnType<typeof vi.fn>).mockReturnValue(selectResult.proxy)

      const result = await repo.findById('non-existent')
      expect(result).toBeNull()
    })
  })
})
