import { open } from '@tauri-apps/plugin-dialog'
import { copyFile, mkdir } from '@tauri-apps/plugin-fs'
import { appDataDir, join } from '@tauri-apps/api/path'
import { convertFileSrc } from '@tauri-apps/api/core'

const SUPPORTED_IMAGES = ['png', 'jpg', 'jpeg', 'webp', 'tiff', 'tif']
export const SUPPORTED_FORMATS = [...SUPPORTED_IMAGES, 'pdf']

export interface ImportedFile {
  originalName: string
  destPath: string
  type: 'image' | 'pdf'
  size: number
}

/**
 * Classify a filename by its extension.
 * Returns 'image', 'pdf', or null if unsupported.
 */
export function classifyFileType(filename: string): 'image' | 'pdf' | null {
  const ext = filename.split('.').pop()?.toLowerCase() ?? ''
  if (SUPPORTED_IMAGES.includes(ext)) return 'image'
  if (ext === 'pdf') return 'pdf'
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
  const dataDir = await appDataDir()
  const destDir = await join(dataDir, 'assets', collectionId, itemId)
  await mkdir(destDir, { recursive: true })

  const imported: ImportedFile[] = []

  for (const filePath of files) {
    const name = filePath.split(/[/\\]/).pop() ?? 'unknown'
    const type = classifyFileType(name)
    if (!type) continue

    const destPath = await join(destDir, `${crypto.randomUUID()}_${name}`)
    await copyFile(filePath, destPath)

    imported.push({
      originalName: name,
      destPath,
      type,
      size: 0, // Size not available from dialog; consumer can stat if needed
    })
  }

  return imported
}

/**
 * Convert a native file path to a URL that can be used in the webview.
 */
export function getAssetUrl(nativePath: string): string {
  return convertFileSrc(nativePath)
}
