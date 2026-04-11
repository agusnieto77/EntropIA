import { eq, and } from 'drizzle-orm'
import type { DrizzleClient } from '../types'
import { entities } from '../schema'

export type Entity = typeof entities.$inferSelect
export type NewEntity = {
  itemId: string
  entityType: string
  value: string
  startOffset?: number
  endOffset?: number
  confidence?: number
  createdAt: number
}

export type EntityType = 'person' | 'place' | 'date' | 'institution' | 'custom'

export class EntityRepo {
  constructor(private db: DrizzleClient) {}

  async findByItemId(itemId: string): Promise<Entity[]> {
    return this.db.select().from(entities).where(eq(entities.itemId, itemId))
  }

  async findByItemIdAndType(itemId: string, type: EntityType): Promise<Entity[]> {
    return this.db
      .select()
      .from(entities)
      .where(and(eq(entities.itemId, itemId), eq(entities.entityType, type)))
  }

  async create(data: NewEntity): Promise<Entity> {
    const rows = await this.db
      .insert(entities)
      .values({
        id: crypto.randomUUID(),
        itemId: data.itemId,
        entityType: data.entityType,
        value: data.value,
        startOffset: data.startOffset ?? 0,
        endOffset: data.endOffset ?? 0,
        confidence: data.confidence ?? 1.0,
        createdAt: data.createdAt,
      })
      .returning()

    return rows[0]!
  }

  async deleteByItemId(itemId: string): Promise<void> {
    await this.db.delete(entities).where(eq(entities.itemId, itemId))
  }
}
