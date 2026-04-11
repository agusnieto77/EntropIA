import { eq, desc, sql } from 'drizzle-orm'
import type { DrizzleClient } from '../types'
import { collections, items } from '../schema'

export type Collection = typeof collections.$inferSelect
export type NewCollection = typeof collections.$inferInsert

export class CollectionRepo {
  constructor(private db: DrizzleClient) {}

  async create(data: Omit<NewCollection, 'id' | 'createdAt' | 'updatedAt'>): Promise<Collection> {
    const now = Date.now()
    const rows = await this.db
      .insert(collections)
      .values({
        id: crypto.randomUUID(),
        ...data,
        createdAt: now,
        updatedAt: now,
      })
      .returning()

    return rows[0]!
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
