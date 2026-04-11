import { describe, it, expect, vi, beforeEach } from 'vitest'
import { exportCollectionToJson } from './export'

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
