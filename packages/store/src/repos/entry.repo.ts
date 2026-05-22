import { eq, asc } from 'drizzle-orm'
import type { DrizzleClient } from '../types'
import { entries } from '../schema'

export type Entry = typeof entries.$inferSelect
export type NewEntry = typeof entries.$inferInsert

export class EntryRepo {
  constructor(private db: DrizzleClient) {}

  async create(data: Omit<NewEntry, 'id' | 'createdAt'>): Promise<Entry> {
    const entry: Entry = {
      id: crypto.randomUUID(),
      itemId: data.itemId,
      content: data.content,
      attributes: data.attributes ?? null,
      sortIndex: data.sortIndex ?? 0,
      createdAt: Date.now(),
    }
    await this.db.insert(entries).values(entry)
    return entry
  }

  async createBatch(rows: Omit<NewEntry, 'id' | 'createdAt'>[]): Promise<number> {
    if (rows.length === 0) return 0
    const now = Date.now()
    const values = rows.map((r, i) => ({
      id: crypto.randomUUID(),
      itemId: r.itemId,
      content: r.content,
      attributes: r.attributes ?? null,
      sortIndex: r.sortIndex ?? i,
      createdAt: now,
    }))
    await this.db.insert(entries).values(values)
    return values.length
  }

  async findByItem(itemId: string): Promise<Entry[]> {
    return this.db
      .select()
      .from(entries)
      .where(eq(entries.itemId, itemId))
      .orderBy(asc(entries.sortIndex))
  }

  async findById(id: string): Promise<Entry | null> {
    const rows = await this.db.select().from(entries).where(eq(entries.id, id))
    return rows[0] ?? null
  }

  async countByItem(itemId: string): Promise<number> {
    const rows = await this.db
      .select()
      .from(entries)
      .where(eq(entries.itemId, itemId))
    return rows.length
  }

  async delete(id: string): Promise<void> {
    await this.db.delete(entries).where(eq(entries.id, id))
  }

  async deleteByItem(itemId: string): Promise<void> {
    await this.db.delete(entries).where(eq(entries.itemId, itemId))
  }
}
