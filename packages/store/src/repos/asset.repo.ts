import { eq } from 'drizzle-orm'
import type { DrizzleClient } from '../types'
import { assets } from '../schema'

export type Asset = typeof assets.$inferSelect
export type NewAsset = typeof assets.$inferInsert

export class AssetRepo {
  constructor(private db: DrizzleClient) {}

  async create(data: Omit<NewAsset, 'id' | 'createdAt'>): Promise<Asset> {
    const rows = await this.db
      .insert(assets)
      .values({
        id: crypto.randomUUID(),
        ...data,
        createdAt: Date.now(),
      })
      .returning()

    return rows[0]!
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
