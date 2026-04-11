import { save } from '@tauri-apps/plugin-dialog'
import { writeFile } from '@tauri-apps/plugin-fs'
import type { Asset, Collection, Item, Note, StoreApi } from '@entropia/store'

type ExportAsset = {
  filename: string
  type: string
  size: number | null
  path: string
}

type ExportNote = {
  content: string
  createdAt: number
  updatedAt: number
}

type ExportItem = {
  id: string
  title: string
  metadata: string | null
  createdAt: number
  updatedAt: number
  assets: ExportAsset[]
  notes: ExportNote[]
}

export interface CollectionExportPayload {
  version: number
  exportedAt: string
  collection: {
    id: string
    name: string
    description: string | null
    createdAt: number
    updatedAt: number
  }
  items: ExportItem[]
}

function toRelativeAssetPath(path: string): string {
  const normalized = path.replace(/\\/g, '/')
  const marker = '/assets/'
  const markerIndex = normalized.lastIndexOf(marker)
  if (markerIndex >= 0) {
    return normalized.slice(markerIndex + 1)
  }
  return normalized
}

function getFilename(path: string): string {
  return path.split(/[/\\]/).pop() ?? path
}

export function buildCollectionExportData(
  collection: Collection,
  items: Item[],
  assetsByItemId: Record<string, Asset[]>,
  notesByItemId: Record<string, Note[]>
): CollectionExportPayload {
  return {
    version: 1,
    exportedAt: new Date().toISOString(),
    collection: {
      id: collection.id,
      name: collection.name,
      description: collection.description,
      createdAt: collection.createdAt,
      updatedAt: collection.updatedAt,
    },
    items: items.map((item) => ({
      id: item.id,
      title: item.title,
      metadata: item.metadata,
      createdAt: item.createdAt,
      updatedAt: item.updatedAt,
      assets: (assetsByItemId[item.id] ?? []).map((asset) => ({
        filename: getFilename(asset.path),
        type: asset.type,
        size: asset.size ?? null,
        path: toRelativeAssetPath(asset.path),
      })),
      notes: (notesByItemId[item.id] ?? []).map((note) => ({
        content: note.content,
        createdAt: note.createdAt,
        updatedAt: note.updatedAt,
      })),
    })),
  }
}

export async function exportCollectionById(
  store: StoreApi,
  collectionId: string
): Promise<string | null> {
  const collection = await store.collections.findById(collectionId)
  if (!collection) return null

  const items = await store.items.findByCollection(collectionId)

  const assetsByItemId: Record<string, Asset[]> = {}
  const notesByItemId: Record<string, Note[]> = {}

  for (const item of items) {
    const [assets, notes] = await Promise.all([
      store.assets.findByItem(item.id),
      store.notes.findByItem(item.id),
    ])
    assetsByItemId[item.id] = assets
    notesByItemId[item.id] = notes
  }

  const payload = buildCollectionExportData(collection, items, assetsByItemId, notesByItemId)
  return exportCollectionToJson(payload, `${collection.name}.json`)
}

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
