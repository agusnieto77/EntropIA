import { open } from '@tauri-apps/plugin-dialog'
import { copyFile, mkdir, remove } from '@tauri-apps/plugin-fs'
import { appDataDir, join } from '@tauri-apps/api/path'
import { convertFileSrc } from '@tauri-apps/api/core'

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

export async function importFilesFromPaths(
  filePaths: string[],
  collectionId: string,
  itemId: string
): Promise<ImportFromPathsResult> {
  const dataDir = await appDataDir()
  const destDir = await join(dataDir, 'assets', collectionId, itemId)
  await mkdir(destDir, { recursive: true })

  const imported: ImportedFile[] = []
  const rejected: string[] = []
  const seenSourcePaths = new Set<string>()
  let skippedDuplicatePaths = 0

  for (const filePath of filePaths) {
    const normalizedSource = filePath.toLowerCase()
    if (seenSourcePaths.has(normalizedSource)) {
      skippedDuplicatePaths++
      continue
    }
    seenSourcePaths.add(normalizedSource)

    const name = filePath.split(/[/\\]/).pop() ?? 'unknown'
    const type = classifyFileType(name)
    if (!type) {
      rejected.push(name)
      continue
    }

    const destPath = await join(destDir, `${crypto.randomUUID()}_${name}`)
    await copyFile(filePath, destPath)

    imported.push({
      originalName: name,
      destPath,
      type,
      size: 0, // Size not available from dialog; consumer can stat if needed
    })
  }

  return {
    imported,
    rejected,
    skippedDuplicatePaths,
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
