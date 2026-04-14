import { eq, desc, sql } from 'drizzle-orm'
import type { DrizzleClient, DbClient } from '../types'
import { collections, items } from '../schema'

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
