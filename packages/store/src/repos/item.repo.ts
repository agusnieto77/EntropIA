import { eq, and, like, or, desc } from 'drizzle-orm'
import type { DrizzleClient, DbClient } from '../types'
import { items } from '../schema'
import { FtsRepo, type FtsResult } from './fts.repo'

export type Item = typeof items.$inferSelect
export type NewItem = typeof items.$inferInsert

export class ItemRepo {
  private ftsRepo: FtsRepo | null

  constructor(
    private db: DrizzleClient,
    rawClient?: DbClient
  ) {
    this.ftsRepo = rawClient ? new FtsRepo(rawClient) : null
  }

  async create(data: Omit<NewItem, 'id' | 'createdAt' | 'updatedAt'>): Promise<Item> {
    const now = Date.now()
    const rows = await this.db
      .insert(items)
      .values({
        id: crypto.randomUUID(),
        ...data,
        createdAt: now,
        updatedAt: now,
      })
      .returning()

    return rows[0]!
  }

  async findByCollection(collectionId: string): Promise<Item[]> {
    return this.db
      .select()
      .from(items)
      .where(eq(items.collectionId, collectionId))
      .orderBy(desc(items.updatedAt))
  }

  async findById(id: string): Promise<Item | null> {
    const rows = await this.db.select().from(items).where(eq(items.id, id))

    return rows[0] ?? null
  }

  async update(id: string, data: Partial<Pick<NewItem, 'title' | 'metadata'>>): Promise<Item> {
    const rows = await this.db
      .update(items)
      .set({ ...data, updatedAt: Date.now() })
      .where(eq(items.id, id))
      .returning()

    return rows[0]!
  }

  async delete(id: string): Promise<void> {
    await this.db.delete(items).where(eq(items.id, id))
  }

  /**
   * Search items by text.
   * - If a rawClient was provided (FTS5 available), delegates to searchByFts5.
   * - Otherwise falls back to SQL LIKE on title and metadata.
   */
  async searchByText(collectionId: string, query: string): Promise<Item[]> {
    const pattern = `%${query}%`
    return this.db
      .select()
      .from(items)
      .where(
        and(
          eq(items.collectionId, collectionId),
          or(like(items.title, pattern), like(items.metadata, pattern))
        )
      )
      .orderBy(desc(items.updatedAt))
  }

  /**
   * FTS5-based search. Returns FtsResult[] with itemId and rank.
   * Requires a rawClient (DbClient) to be provided at construction time.
   * Returns empty array if no rawClient or empty query.
   */
  async searchByFts5(query: string, _collectionId?: string): Promise<FtsResult[]> {
    if (!this.ftsRepo || !query.trim()) return []
    return this.ftsRepo.search(query, 50)
  }
}
