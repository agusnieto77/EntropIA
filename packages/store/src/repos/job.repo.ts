import { eq, asc, desc } from 'drizzle-orm'
import type { DrizzleClient } from '../types'
import { jobs } from '../schema'

export type Job = typeof jobs.$inferSelect
export type NewJob = typeof jobs.$inferInsert

export class JobRepo {
  constructor(private db: DrizzleClient) {}

  async create(data: { assetId: string; type: string }): Promise<Job> {
    const now = Date.now()
    const rows = await this.db
      .insert(jobs)
      .values({
        id: crypto.randomUUID(),
        type: data.type,
        status: 'pending',
        assetId: data.assetId,
        createdAt: now,
        updatedAt: now,
      })
      .returning()

    return rows[0]!
  }

  async findById(id: string): Promise<Job | null> {
    const rows = await this.db.select().from(jobs).where(eq(jobs.id, id))
    return rows[0] ?? null
  }

  async findPending(): Promise<Job[]> {
    return this.db
      .select()
      .from(jobs)
      .where(eq(jobs.status, 'pending'))
      .orderBy(asc(jobs.createdAt))
  }

  async findByAsset(assetId: string): Promise<Job | null> {
    const rows = await this.db
      .select()
      .from(jobs)
      .where(eq(jobs.assetId, assetId))
      .orderBy(desc(jobs.createdAt))
      .limit(1)

    return rows[0] ?? null
  }

  async updateStatus(id: string, status: string, error?: string): Promise<Job> {
    const rows = await this.db
      .update(jobs)
      .set({
        status,
        ...(error !== undefined ? { error } : {}),
        updatedAt: Date.now(),
      })
      .where(eq(jobs.id, id))
      .returning()

    return rows[0]!
  }

  /**
   * Store progress (0–100) in the `result` JSON field.
   * The `jobs` table has no dedicated `progress` column,
   * so we encode it as `{ "progress": N }` in `result`.
   */
  async updateProgress(id: string, progress: number): Promise<Job> {
    const rows = await this.db
      .update(jobs)
      .set({
        result: JSON.stringify({ progress }),
        updatedAt: Date.now(),
      })
      .where(eq(jobs.id, id))
      .returning()

    return rows[0]!
  }
}
