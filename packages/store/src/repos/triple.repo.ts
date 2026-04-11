import { eq } from 'drizzle-orm'
import type { DrizzleClient } from '../types'
import { triples } from '../schema'

export type Triple = typeof triples.$inferSelect

export type NewTriple = {
  subject: string
  predicate: string
  object: string
}

export class TripleRepo {
  constructor(private db: DrizzleClient) {}

  async findByItemId(itemId: string): Promise<Triple[]> {
    return this.db.select().from(triples).where(eq(triples.itemId, itemId))
  }

  async replaceByItemId(itemId: string, rows: NewTriple[]): Promise<void> {
    await this.db.delete(triples).where(eq(triples.itemId, itemId))
    if (rows.length === 0) return

    await this.db.insert(triples).values(
      rows.map((row) => ({
        id: crypto.randomUUID(),
        itemId,
        subject: row.subject,
        predicate: row.predicate,
        object: row.object,
        createdAt: Date.now(),
      }))
    )
  }
}
