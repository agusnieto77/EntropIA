import { eq, and, asc } from 'drizzle-orm'
import type { DrizzleClient } from '../types'
import { entryResults } from '../schema'

export type EntryResult = typeof entryResults.$inferSelect
export type NewEntryResult = typeof entryResults.$inferInsert

export class EntryResultRepo {
  constructor(private db: DrizzleClient) {}

  async upsert(data: Omit<NewEntryResult, 'id' | 'createdAt'>): Promise<EntryResult> {
    // Delete existing result for same entry+jobType, then insert
    await this.db
      .delete(entryResults)
      .where(
        and(eq(entryResults.entryId, data.entryId), eq(entryResults.jobType, data.jobType))
      )

    const row: EntryResult = {
      id: crypto.randomUUID(),
      entryId: data.entryId,
      jobType: data.jobType,
      result: data.result,
      createdAt: Date.now(),
    }
    await this.db.insert(entryResults).values(row)
    return row
  }

  async upsertBatch(
    rows: Omit<NewEntryResult, 'id' | 'createdAt'>[]
  ): Promise<number> {
    if (rows.length === 0) return 0
    const now = Date.now()

    // Delete existing results for the same entry+jobType pairs
    for (const row of rows) {
      await this.db
        .delete(entryResults)
        .where(
          and(eq(entryResults.entryId, row.entryId), eq(entryResults.jobType, row.jobType))
        )
    }

    const values = rows.map((r) => ({
      id: crypto.randomUUID(),
      entryId: r.entryId,
      jobType: r.jobType,
      result: r.result,
      createdAt: now,
    }))
    await this.db.insert(entryResults).values(values)
    return values.length
  }

  async findByEntry(entryId: string): Promise<EntryResult[]> {
    return this.db
      .select()
      .from(entryResults)
      .where(eq(entryResults.entryId, entryId))
      .orderBy(asc(entryResults.jobType))
  }

  async findByEntryAndType(entryId: string, jobType: string): Promise<EntryResult | null> {
    const rows = await this.db
      .select()
      .from(entryResults)
      .where(
        and(eq(entryResults.entryId, entryId), eq(entryResults.jobType, jobType))
      )
    return rows[0] ?? null
  }

  async findByEntriesAndType(entryIds: string[], jobType: string): Promise<EntryResult[]> {
    if (entryIds.length === 0) return []
    const all = await this.db.select().from(entryResults).where(eq(entryResults.jobType, jobType))
    const idSet = new Set(entryIds)
    return all.filter((r) => idSet.has(r.entryId))
  }

  async deleteByEntry(entryId: string): Promise<void> {
    await this.db.delete(entryResults).where(eq(entryResults.entryId, entryId))
  }
}
