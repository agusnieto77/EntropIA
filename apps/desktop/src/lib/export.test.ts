import { describe, it, expect, vi, beforeEach } from 'vitest'
import { exportCollectionToJson, buildCollectionExportData, exportCollectionById } from './export'

type ExportStoreMock = {
  collections: { findById: ReturnType<typeof vi.fn> }
  items: { findByCollection: ReturnType<typeof vi.fn> }
  assets: { findByItem: ReturnType<typeof vi.fn> }
  notes: { findByItem: ReturnType<typeof vi.fn> }
}

type ExportStoreInput = Parameters<typeof exportCollectionById>[0]

function asExportStore(store: ExportStoreMock): ExportStoreInput {
  return store as unknown as ExportStoreInput
}

describe('exportCollectionToJson', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns null when user cancels save dialog', async () => {
    const { save } = await import('@tauri-apps/plugin-dialog')
    vi.mocked(save).mockResolvedValue(null)

    const result = await exportCollectionToJson({ name: 'Test' }, 'test.json')
    expect(result).toBeNull()
  })

  it('writes JSON file and returns path when user selects location', async () => {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const { writeFile } = await import('@tauri-apps/plugin-fs')
    vi.mocked(save).mockResolvedValue('/exports/test.json')
    vi.mocked(writeFile).mockResolvedValue(undefined)

    const data = { name: 'My Collection', items: [{ id: '1' }] }
    const result = await exportCollectionToJson(data, 'my-collection.json')

    expect(result).toBe('/exports/test.json')
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: 'my-collection.json',
      })
    )
    expect(writeFile).toHaveBeenCalledWith('/exports/test.json', expect.any(Uint8Array))
  })

  it('serializes data as pretty-printed JSON', async () => {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const { writeFile } = await import('@tauri-apps/plugin-fs')
    vi.mocked(save).mockResolvedValue('/out/data.json')
    vi.mocked(writeFile).mockResolvedValue(undefined)

    const data = { hello: 'world' }
    await exportCollectionToJson(data, 'data.json')

    const writtenBytes = vi.mocked(writeFile).mock.calls[0]![1] as Uint8Array
    const writtenStr = new TextDecoder().decode(writtenBytes)
    expect(JSON.parse(writtenStr)).toEqual(data)
    // Pretty-printed = has newlines
    expect(writtenStr).toContain('\n')
  })
})

describe('buildCollectionExportData', () => {
  it('builds versioned payload with relative asset paths', () => {
    const collection = {
      id: 'c1',
      name: 'Archivo Municipal',
      description: 'docs',
      createdAt: 1,
      updatedAt: 2,
    }
    const items = [
      { id: 'i1', title: 'Acta 1', collectionId: 'c1', metadata: null, createdAt: 3, updatedAt: 4 },
    ]

    const payload = buildCollectionExportData(
      collection,
      items,
      {
        i1: [
          {
            id: 'a1',
            itemId: 'i1',
            path: '/mock/app-data/assets/c1/i1/file.pdf',
            type: 'pdf',
            size: 123,
            createdAt: 5,
          },
        ],
      },
      {
        i1: [
          {
            id: 'n1',
            itemId: 'i1',
            content: 'nota',
            createdAt: 6,
            updatedAt: 7,
          },
        ],
      }
    )

    expect(payload.version).toBe(1)
    expect(Array.isArray(payload.items)).toBe(true)
    expect(payload.items).toHaveLength(1)
    expect(payload.items[0]?.assets[0]?.path).toBe('assets/c1/i1/file.pdf')
    expect(payload.items[0]?.notes[0]?.content).toBe('nota')
  })
})

describe('exportCollectionById', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns null when collection does not exist', async () => {
    const store: ExportStoreMock = {
      collections: { findById: vi.fn().mockResolvedValue(null) },
      items: { findByCollection: vi.fn() },
      assets: { findByItem: vi.fn() },
      notes: { findByItem: vi.fn() },
    }

    const result = await exportCollectionById(asExportStore(store), 'missing')
    expect(result).toBeNull()
  })

  it('exports full collection payload with default filename', async () => {
    const { save } = await import('@tauri-apps/plugin-dialog')
    const { writeFile } = await import('@tauri-apps/plugin-fs')
    vi.mocked(save).mockResolvedValue('/exports/Archivo Municipal.json')
    vi.mocked(writeFile).mockResolvedValue(undefined)

    const collection = {
      id: 'c1',
      name: 'Archivo Municipal',
      description: null,
      createdAt: 1,
      updatedAt: 2,
    }
    const item = {
      id: 'i1',
      title: 'Acta',
      collectionId: 'c1',
      metadata: null,
      createdAt: 3,
      updatedAt: 4,
    }

    const store: ExportStoreMock = {
      collections: { findById: vi.fn().mockResolvedValue(collection) },
      items: { findByCollection: vi.fn().mockResolvedValue([item]) },
      assets: {
        findByItem: vi.fn().mockResolvedValue([
          {
            id: 'a1',
            itemId: 'i1',
            path: '/mock/app-data/assets/c1/i1/acta.pdf',
            type: 'pdf',
            size: 10,
            createdAt: 5,
          },
        ]),
      },
      notes: {
        findByItem: vi
          .fn()
          .mockResolvedValue([
            { id: 'n1', itemId: 'i1', content: 'nota', createdAt: 6, updatedAt: 7 },
          ]),
      },
    }

    const path = await exportCollectionById(asExportStore(store), 'c1')

    expect(path).toBe('/exports/Archivo Municipal.json')
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({
        defaultPath: 'Archivo Municipal.json',
      })
    )
    expect(writeFile).toHaveBeenCalledTimes(1)
  })
})
