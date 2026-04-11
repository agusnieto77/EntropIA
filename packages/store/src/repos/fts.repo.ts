import type { DbClient } from '../types'

export interface FtsResult {
  itemId: string
  rank: number
}

// FTS5 operator keywords to strip from user queries
const FTS5_OPERATORS = /\b(AND|OR|NOT|NEAR)\b/g
// Special characters to strip from individual tokens
const FTS5_SPECIAL_CHARS = /[()"\-*^:,./\\]/g

/**
 * Sanitize a raw user query for safe use in FTS5 MATCH expressions.
 * - Removes FTS5 operator keywords (AND, OR, NOT, NEAR)
 * - Strips special characters that are FTS5 operators
 * - Wraps each remaining token in double quotes
 * - Returns empty string for empty/whitespace-only input
 */
export function sanitizeFts5Query(raw: string): string {
  if (!raw.trim()) return ''

  // Remove operator keywords
  const withoutOps = raw.replace(FTS5_OPERATORS, ' ')

  // Split on whitespace, strip special chars from each token, filter empties
  const tokens = withoutOps
    .split(/\s+/)
    .map((token) => token.replace(FTS5_SPECIAL_CHARS, ''))
    .filter((token) => token.length > 0)

  if (tokens.length === 0) return ''

  return tokens.map((t) => `"${t}"`).join(' ')
}

export class FtsRepo {
  constructor(private client: DbClient) {}

  /**
   * Insert or replace an item's indexed fields in fts_items.
   * Uses raw SQL — FTS5 virtual tables don't work with Drizzle ORM builders.
   */
  async indexItem(
    itemId: string,
    title: string,
    metadata: string,
    extractedText: string
  ): Promise<void> {
    // First delete existing row, then insert (manual upsert for FTS5 contentless tables)
    await this.client.execute(`DELETE FROM fts_items WHERE item_id = ?`, [itemId])
    await this.client.execute(
      `INSERT INTO fts_items(item_id, title, metadata, extracted_text) VALUES (?, ?, ?, ?)`,
      [itemId, title, metadata, extractedText]
    )
  }

  /**
   * Search fts_items using FTS5 MATCH. Returns ranked results.
   * Returns empty array for empty/whitespace query (no DB call).
   */
  async search(query: string, limit = 20): Promise<FtsResult[]> {
    const safeQuery = sanitizeFts5Query(query)
    if (!safeQuery) return []

    const rows = await this.client.select<{ item_id: string; rank: number }>(
      `SELECT item_id, rank FROM fts_items WHERE fts_items MATCH ? ORDER BY rank LIMIT ?`,
      [safeQuery, limit]
    )

    return rows.map((row) => ({
      itemId: row.item_id,
      rank: row.rank,
    }))
  }

  /**
   * Remove an item's row from fts_items.
   */
  async removeItem(itemId: string): Promise<void> {
    await this.client.execute(`DELETE FROM fts_items WHERE item_id = ?`, [itemId])
  }
}
