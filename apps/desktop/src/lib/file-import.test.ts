import { describe, it, expect, vi, beforeEach } from 'vitest'
import {
  pickAndImportFiles,
  getAssetUrl,
  SUPPORTED_FORMATS,
  classifyFileType,
  importFilesFromPaths,
} from './file-import'

type OpenSelection = string[] | string | null

// Tauri APIs are already mocked globally in test-setup.ts

describe('classifyFileType', () => {
  it('classifies png as image', () => {
    expect(classifyFileType('photo.png')).toBe('image')
  })

  it('classifies jpg as image', () => {
    expect(classifyFileType('photo.jpg')).toBe('image')
  })

  it('classifies jpeg as image', () => {
    expect(classifyFileType('photo.jpeg')).toBe('image')
  })

  it('classifies webp as image', () => {
    expect(classifyFileType('photo.webp')).toBe('image')
  })

  it('classifies tiff as image', () => {
    expect(classifyFileType('scan.tiff')).toBe('image')
  })

  it('classifies tif as image', () => {
    expect(classifyFileType('scan.tif')).toBe('image')
  })

  it('classifies pdf as pdf', () => {
    expect(classifyFileType('document.pdf')).toBe('pdf')
  })

  it('returns null for unsupported extensions', () => {
    expect(classifyFileType('script.exe')).toBeNull()
  })

  it('handles uppercase extensions', () => {
    expect(classifyFileType('PHOTO.PNG')).toBe('image')
    expect(classifyFileType('DOC.PDF')).toBe('pdf')
  })
})

describe('SUPPORTED_FORMATS', () => {
  it('includes all image and pdf formats', () => {
    expect(SUPPORTED_FORMATS).toContain('png')
    expect(SUPPORTED_FORMATS).toContain('jpg')
    expect(SUPPORTED_FORMATS).toContain('jpeg')
    expect(SUPPORTED_FORMATS).toContain('webp')
    expect(SUPPORTED_FORMATS).toContain('tiff')
    expect(SUPPORTED_FORMATS).toContain('tif')
    expect(SUPPORTED_FORMATS).toContain('pdf')
    expect(SUPPORTED_FORMATS).toHaveLength(7)
  })
})

describe('getAssetUrl', () => {
  it('delegates to convertFileSrc', async () => {
    const { convertFileSrc } = await import('@tauri-apps/api/core')
    vi.mocked(convertFileSrc).mockReturnValue('https://asset.localhost/path/to/file.png')

    const result = getAssetUrl('/path/to/file.png')
    expect(result).toBe('https://asset.localhost/path/to/file.png')
    expect(convertFileSrc).toHaveBeenCalledWith('/path/to/file.png')
  })
})

describe('pickAndImportFiles', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns empty array when user cancels dialog', async () => {
    const { open } = await import('@tauri-apps/plugin-dialog')
    vi.mocked(open).mockResolvedValue(null)

    const result = await pickAndImportFiles('coll-1', 'item-1')
    expect(result).toEqual([])
  })

  it('copies selected files and returns ImportedFile array', async () => {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const { copyFile, mkdir } = await import('@tauri-apps/plugin-fs')
    const { appDataDir, join } = await import('@tauri-apps/api/path')

    vi.mocked(open).mockResolvedValue(['C:/photos/sunset.png'] as OpenSelection)
    vi.mocked(mkdir).mockResolvedValue(undefined)
    vi.mocked(copyFile).mockResolvedValue(undefined)
    vi.mocked(appDataDir).mockResolvedValue('/mock/app-data')
    vi.mocked(join).mockImplementation((...parts: string[]) => Promise.resolve(parts.join('/')))

    const result = await pickAndImportFiles('coll-1', 'item-1')

    expect(result).toHaveLength(1)
    expect(result[0]).toMatchObject({
      originalName: 'sunset.png',
      type: 'image',
    })
    expect(result[0]!.destPath).toContain('coll-1')
    expect(result[0]!.destPath).toContain('item-1')
    expect(mkdir).toHaveBeenCalled()
    expect(copyFile).toHaveBeenCalled()
  })
})

describe('importFilesFromPaths', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('filters unsupported files and reports them', async () => {
    const { copyFile, mkdir } = await import('@tauri-apps/plugin-fs')
    const { appDataDir, join } = await import('@tauri-apps/api/path')

    vi.mocked(mkdir).mockResolvedValue(undefined)
    vi.mocked(copyFile).mockResolvedValue(undefined)
    vi.mocked(appDataDir).mockResolvedValue('/mock/app-data')
    vi.mocked(join).mockImplementation((...parts: string[]) => Promise.resolve(parts.join('/')))

    const result = await importFilesFromPaths(
      ['C:/docs/scan.pdf', 'C:/docs/readme.docx'],
      'c1',
      'i1'
    )

    expect(result.imported).toHaveLength(1)
    expect(result.imported[0]?.originalName).toBe('scan.pdf')
    expect(result.rejected).toEqual(['readme.docx'])
  })

  it('skips duplicate source paths in the same import batch', async () => {
    const { copyFile, mkdir } = await import('@tauri-apps/plugin-fs')
    const { appDataDir, join } = await import('@tauri-apps/api/path')

    vi.mocked(mkdir).mockResolvedValue(undefined)
    vi.mocked(copyFile).mockResolvedValue(undefined)
    vi.mocked(appDataDir).mockResolvedValue('/mock/app-data')
    vi.mocked(join).mockImplementation((...parts: string[]) => Promise.resolve(parts.join('/')))

    const duplicatePath = 'C:/docs/acta.pdf'
    const result = await importFilesFromPaths([duplicatePath, duplicatePath], 'c1', 'i1')

    expect(result.imported).toHaveLength(1)
    expect(result.skippedDuplicatePaths).toBe(1)
    expect(copyFile).toHaveBeenCalledTimes(1)
  })
})
