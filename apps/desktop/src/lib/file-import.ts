import { open } from '@tauri-apps/plugin-dialog'
import { copyFile, mkdir, remove } from '@tauri-apps/plugin-fs'
import { appDataDir, join } from '@tauri-apps/api/path'
import { convertFileSrc } from '@tauri-apps/api/core'
import { invoke } from '@tauri-apps/api/core'

const SUPPORTED_IMAGES = ['png', 'jpg', 'jpeg', 'webp', 'tiff', 'tif']
const SUPPORTED_AUDIO = ['wav', 'mp3', 'flac', 'm4a', 'aac', 'ogg']
export const SUPPORTED_FORMATS = [...SUPPORTED_IMAGES, 'pdf', ...SUPPORTED_AUDIO]

export interface ImportedFile {
  originalName: string
  destPath: string
  type: 'image' | 'pdf' | 'audio'
  size: number
}

export interface ImportFromPathsResult {
  imported: ImportedFile[]
  rejected: string[]
  skippedDuplicatePaths: number
}

/**
 * Classify a filename by its extension.
 * Returns 'image', 'pdf', 'audio', or null if unsupported.
 */
export function classifyFileType(filename: string): 'image' | 'pdf' | 'audio' | null {
  const ext = filename.split('.').pop()?.toLowerCase() ?? ''
  if (SUPPORTED_IMAGES.includes(ext)) return 'image'
  if (ext === 'pdf') return 'pdf'
  if (SUPPORTED_AUDIO.includes(ext)) return 'audio'
  return null
}

/**
 * Open a file picker dialog and return the selected file paths.
 * Does NOT copy or classify files — the caller handles that.
 */
export async function pickFiles(): Promise<string[]> {
  try {
    const selected = await open({
      multiple: true,
      filters: [
        {
          name: 'Documents',
          extensions: SUPPORTED_FORMATS,
        },
      ],
    })

    if (!selected) return []
    return Array.isArray(selected) ? selected : [selected]
  } catch (e) {
    console.error('[file-import] pickFiles error:', e)
    throw new Error(`Failed to open file dialog: ${e instanceof Error ? e.message : String(e)}`)
  }
}

/**
 * Open a file picker dialog, copy selected files into the app data directory,
 * and return metadata about imported files.
 */
export async function pickAndImportFiles(
  collectionId: string,
  itemId: string
): Promise<ImportedFile[]> {
  try {
    const selected = await open({
      multiple: true,
      filters: [
        {
          name: 'Documents',
          extensions: SUPPORTED_FORMATS,
        },
      ],
    })

    if (!selected) return []

    const files = Array.isArray(selected) ? selected : [selected]

    const result = await importFilesFromPaths(files, collectionId, itemId)
    return result.imported
  } catch (e) {
    console.error('[file-import] pickAndImportFiles error:', e)
    throw new Error(`Failed to open file dialog: ${e instanceof Error ? e.message : String(e)}`)
  }
}

/**
 * Copy a single file into the app data directory under `{collectionId}/{itemId}/`.
 * Returns the destination path.
 */
async function copyFileToItem(
  sourcePath: string,
  collectionId: string,
  itemId: string
): Promise<string> {
  const dataDir = await appDataDir()
  const destDir = await join(dataDir, 'assets', collectionId, itemId)
  await mkdir(destDir, { recursive: true })

  const name = sourcePath.split(/[/\\]/).pop() ?? 'unknown'
  const destPath = await join(destDir, `${crypto.randomUUID()}_${name}`)
  await copyFile(sourcePath, destPath)
  return destPath
}

/**
 * Classify and validate a batch of file paths.
 * Returns classified files ready to be imported and rejected filenames.
 */
export function classifyFiles(filePaths: string[]): {
  classified: { sourcePath: string; name: string; type: 'image' | 'pdf' | 'audio' }[]
  rejected: string[]
} {
  const classified: { sourcePath: string; name: string; type: 'image' | 'pdf' | 'audio' }[] = []
  const rejected: string[] = []
  const seenSourcePaths = new Set<string>()

  for (const filePath of filePaths) {
    const normalizedSource = filePath.toLowerCase()
    if (seenSourcePaths.has(normalizedSource)) {
      continue // silently skip duplicates — caller can track if needed
    }
    seenSourcePaths.add(normalizedSource)

    const name = filePath.split(/[/\\]/).pop() ?? 'unknown'
    const type = classifyFileType(name)
    if (!type) {
      rejected.push(name)
      continue
    }

    classified.push({ sourcePath: filePath, name, type })
  }

  return { classified, rejected }
}

export async function importFilesFromPaths(
  filePaths: string[],
  collectionId: string,
  itemId: string
): Promise<ImportFromPathsResult> {
  const { classified, rejected } = classifyFiles(filePaths)
  const skippedDuplicatePaths = filePaths.length - classified.length - rejected.length

  const imported: ImportedFile[] = []

  for (const file of classified) {
    const destPath = await copyFileToItem(file.sourcePath, collectionId, itemId)
    imported.push({
      originalName: file.name,
      destPath,
      type: file.type,
      size: 0,
    })
  }

  return {
    imported,
    rejected,
    skippedDuplicatePaths,
  }
}

/**
 * Import a single file: copy it to the app data directory under its own item.
 * Returns the ImportedFile metadata.
 */
export async function importSingleFile(
  sourcePath: string,
  collectionId: string,
  itemId: string
): Promise<ImportedFile> {
  const name = sourcePath.split(/[/\\]/).pop() ?? 'unknown'
  const type = classifyFileType(name)
  if (!type) {
    throw new Error(`Unsupported file format: ${name}`)
  }

  const destPath = await copyFileToItem(sourcePath, collectionId, itemId)
  return {
    originalName: name,
    destPath,
    type,
    size: 0,
  }
}

/**
 * Convert a native file path to a URL that can be used in the webview.
 */
export function getAssetUrl(nativePath: string): string {
  return convertFileSrc(nativePath)
}

/**
 * Delete an asset file from the filesystem.
 *
 * - If the file does not exist (ENOENT/not-found), logs a warning and returns
 *   successfully — the DB cleanup should still proceed.
 * - If a permission error or other filesystem error occurs, throws so the
 *   caller can abort the deletion flow.
 */
export async function deleteAssetFile(nativePath: string): Promise<void> {
  try {
    await remove(nativePath)
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e)
    // ENOENT / NotFound — file already gone, continue with DB cleanup
    if (
      message.includes('ENOENT') ||
      message.includes('not found') ||
      message.includes('NotFound')
    ) {
      console.warn('[file-import] Asset file not found, continuing with DB cleanup:', nativePath)
      return
    }
    // Permission error or other FS error — abort
    throw new Error(`Failed to delete asset file: ${message}`)
  }
}

// ---------------------------------------------------------------------------
// PDF Thumbnails
// ---------------------------------------------------------------------------

/**
 * Generate or retrieve a cached thumbnail for the first page of a PDF.
 *
 * Calls the Rust `generate_pdf_thumbnail` command, which renders the first
 * page at 400px width and caches the PNG at `{app_data_dir}/thumbnails/{asset_id}.png`.
 * If a cached thumbnail already exists, the cached path is returned immediately.
 *
 * Returns a webview-accessible URL via `convertFileSrc`.
 */
export async function generatePdfThumbnail(
  assetPath: string,
  assetId: string
): Promise<string> {
  const nativePath: string = await invoke('generate_pdf_thumbnail', {
    assetPath,
    assetId,
  })
  return convertFileSrc(nativePath)
}

/**
 * Delete a cached PDF thumbnail for an asset.
 *
 * Should be called when a PDF asset is deleted to clean up the thumbnail cache.
 * Silently succeeds even if the thumbnail doesn't exist.
 */
export async function deletePdfThumbnail(assetId: string): Promise<void> {
  await invoke('delete_pdf_thumbnail', { assetId })
}
