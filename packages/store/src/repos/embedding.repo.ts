import type { DbClient } from '../types'

/**
 * EmbeddingRepo — manages float32 embedding vectors for items.
 *
 * In production (Tauri runtime), uses the vec0 virtual table via sqlite-vec.
 * In test environments (where sqlite-vec is unavailable), falls back to a
 * regular `embeddings_fallback` table that stores vectors as JSON blobs.
 *
 * The `initialize()` method auto-detects which mode to use.
 */
export class EmbeddingRepo {
  private useVec0 = false

  constructor(private client: DbClient) {}

  /**
   * Initialize the embedding storage. Tries vec0 first, falls back to JSON table.
   * Must be called before any other method.
   */
  async initialize(): Promise<void> {
    try {
      // Try to create/access the vec0 virtual table
      await this.client.execute(
        `CREATE VIRTUAL TABLE IF NOT EXISTS vec_items USING vec0(item_id TEXT PRIMARY KEY, embedding FLOAT[384])`
      )
      this.useVec0 = true
    } catch {
      // sqlite-vec not available — use fallback JSON table
      await this.client.execute(
        `CREATE TABLE IF NOT EXISTS embeddings_fallback (item_id TEXT PRIMARY KEY, embedding TEXT NOT NULL)`
      )
      this.useVec0 = false
    }
  }

  /**
   * Store (upsert) an embedding vector for an item.
   */
  async storeEmbedding(itemId: string, embedding: number[]): Promise<void> {
    if (this.useVec0) {
      // vec0 INSERT OR REPLACE with float32 blob
      await this.client.execute(
        `INSERT OR REPLACE INTO vec_items(item_id, embedding) VALUES (?, ?)`,
        [itemId, JSON.stringify(embedding)]
      )
    } else {
      // Fallback: store as JSON string
      await this.client.execute(
        `INSERT OR REPLACE INTO embeddings_fallback(item_id, embedding) VALUES (?, ?)`,
        [itemId, JSON.stringify(embedding)]
      )
    }
  }

  /**
   * Retrieve the embedding vector for an item. Returns null if not found.
   */
  async getEmbedding(itemId: string): Promise<number[] | null> {
    const table = this.useVec0 ? 'vec_items' : 'embeddings_fallback'
    const rows = await this.client.select<{ item_id: string; embedding: string }>(
      `SELECT item_id, embedding FROM ${table} WHERE item_id = ?`,
      [itemId]
    )

    if (rows.length === 0) return null

    const raw = rows[0]!.embedding
    try {
      return JSON.parse(raw) as number[]
    } catch {
      return null
    }
  }

  /**
   * Delete the embedding for an item.
   */
  async deleteEmbedding(itemId: string): Promise<void> {
    const table = this.useVec0 ? 'vec_items' : 'embeddings_fallback'
    await this.client.execute(`DELETE FROM ${table} WHERE item_id = ?`, [itemId])
  }
}
