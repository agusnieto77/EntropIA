import { eq } from 'drizzle-orm'
import type { DrizzleClient, DbClient } from '../types'
import { assets } from '../schema'

export type Asset = typeof assets.$inferSelect
export type NewAsset = typeof assets.$inferInsert

export class AssetRepo {
  constructor(
    private db: DrizzleClient,
    private rawClient?: DbClient
  ) {}

  async create(data: Omit<NewAsset, 'id' | 'createdAt'>): Promise<Asset> {
    const createdAsset: Asset = {
      id: crypto.randomUUID(),
      itemId: data.itemId,
      path: data.path,
      type: data.type,
      size: data.size ?? null,
      createdAt: Date.now(),
    }

    if (this.rawClient) {
      // Validate that the parent item exists before inserting (FK constraint)
      const itemExists = await this.rawClient.select('SELECT id FROM items WHERE id = ?', [
        createdAsset.itemId,
      ])
      if (itemExists.length === 0) {
        throw new Error(`Cannot create asset: item "${createdAsset.itemId}" does not exist`)
      }

      await this.rawClient.execute(
        'INSERT INTO assets (id, item_id, path, type, size, created_at) VALUES (?, ?, ?, ?, ?, ?)',
        [
          createdAsset.id,
          createdAsset.itemId,
          createdAsset.path,
          createdAsset.type,
          createdAsset.size,
          createdAsset.createdAt,
        ]
      )
    } else {
      await this.db.insert(assets).values(createdAsset)
    }

    return createdAsset
  }

  async findByItem(itemId: string): Promise<Asset[]> {
    return this.db.select().from(assets).where(eq(assets.itemId, itemId))
  }

  async findById(id: string): Promise<Asset | null> {
    const rows = await this.db.select().from(assets).where(eq(assets.id, id))

    return rows[0] ?? null
  }

  async delete(id: string): Promise<void> {
    await this.db.delete(assets).where(eq(assets.id, id))
  }
}
