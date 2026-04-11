import { save } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'

/**
 * Export data as a JSON file via the native save dialog.
 * Returns the chosen file path, or null if the user cancelled.
 */
export async function exportCollectionToJson(
  data: object,
  defaultName: string
): Promise<string | null> {
  const filePath = await save({
    defaultPath: defaultName,
    filters: [
      {
        name: 'JSON',
        extensions: ['json'],
      },
    ],
  })

  if (!filePath) return null

  const json = JSON.stringify(data, null, 2)
  const bytes = new TextEncoder().encode(json)
  await writeFile(filePath, bytes)

  return filePath
}
