import { eq, desc, sql, inArray } from 'drizzle-orm'
import type { DrizzleClient, DbClient } from '../types'
import { collections, items, assets, notes, jobs, extractions, entities, triples } from '../schema'

export type Collection = typeof collections.$inferSelect
export type NewCollection = typeof collections.$inferInsert

export class CollectionRepo {
  constructor(
    private db: DrizzleClient,
    private rawClient?: DbClient
  ) {}

  async create(data: Omit<NewCollection, 'id' | 'createdAt' | 'updatedAt'>): Promise<Collection> {
    const now = Date.now()
    const createdCollection: Collection = {
      id: crypto.randomUUID(),
      name: data.name,
      description: data.description ?? null,
      createdAt: now,
      updatedAt: now,
    }

    if (this.rawClient) {
      await this.rawClient.execute(
        'INSERT INTO collections (id, name, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?)',
        [
          createdCollection.id,
          createdCollection.name,
          createdCollection.description,
          createdCollection.createdAt,
          createdCollection.updatedAt,
        ]
      )
    } else {
      await this.db.insert(collections).values(createdCollection)
    }

    return createdCollection
  }

  async findAll(): Promise<Collection[]> {
    return this.db.select().from(collections).orderBy(desc(collections.updatedAt))
  }

  async findById(id: string): Promise<Collection | null> {
    const rows = await this.db.select().from(collections).where(eq(collections.id, id))

    return rows[0] ?? null
  }

  async update(
    id: string,
    data: Partial<Pick<NewCollection, 'name' | 'description'>>
  ): Promise<Collection> {
    const rows = await this.db
      .update(collections)
      .set({ ...data, updatedAt: Date.now() })
      .where(eq(collections.id, id))
      .returning()

    return rows[0]!
  }

  async delete(id: string): Promise<void> {
    // Get item IDs in this collection
    const itemRows = await this.db
      .select({ id: items.id })
      .from(items)
      .where(eq(items.collectionId, id))
    const itemIds = itemRows.map((r) => r.id)

    if (itemIds.length > 0) {
      // Get asset IDs for these items
      const assetRows = await this.db
        .select({ id: assets.id })
        .from(assets)
        .where(inArray(assets.itemId, itemIds))
      const assetIds = assetRows.map((r) => r.id)

      // Delete leaves first: jobs & extractions depend on assets
      if (assetIds.length > 0) {
        await this.db.delete(jobs).where(inArray(jobs.assetId, assetIds))
        await this.db.delete(extractions).where(inArray(extractions.assetId, assetIds))
      }

      // Delete item-level dependents
      await this.db.delete(entities).where(inArray(entities.itemId, itemIds))
      await this.db.delete(triples).where(inArray(triples.itemId, itemIds))
      await this.db.delete(notes).where(inArray(notes.itemId, itemIds))
      await this.db.delete(assets).where(inArray(assets.itemId, itemIds))
      await this.db.delete(items).where(inArray(items.id, itemIds))
    }

    await this.db.delete(collections).where(eq(collections.id, id))
  }

  async countItems(id: string): Promise<number> {
    const rows = await this.db
      .select({ count: sql<number>`count(*)` })
      .from(items)
      .where(eq(items.collectionId, id))

    return rows[0]?.count ?? 0
  }
}
