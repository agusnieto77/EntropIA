/// FTS5 indexing and search helpers.
///
/// All operations use raw SQL against the `fts_items` virtual table.
/// FTS5 contentless tables (`content=''`) require explicit INSERT/DELETE —
/// there is no automatic sync with the source table.
use rusqlite::{params, Connection};

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct FtsRow {
    pub item_id: String,
    pub title: String,
    pub rank: f64,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Fetch item data from DB and index it into `fts_items`.
///
/// Retrieves title, metadata, and extracted text for `item_id`, then upserts
/// into the FTS5 virtual table.
pub fn index_item_from_db(conn: &Connection, item_id: &str) -> Result<(), String> {
    // Fetch item title + metadata
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT title, COALESCE(metadata, '') FROM items WHERE id = ?1",
            params![item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let (title, metadata) = match row {
        Some(r) => r,
        None => return Ok(()), // Item not found — not an error
    };

    // Fetch extracted text (concatenate all extractions)
    let extracted_text: String = {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT COALESCE(e.text_content, '')
                FROM extractions e
                JOIN assets a ON e.asset_id = a.id
                WHERE a.item_id = ?1
                ORDER BY e.created_at ASC
                "#,
            )
            .map_err(|e| format!("Failed to prepare extraction query: {e}"))?;

        let texts: Vec<String> = stmt
            .query_map(params![item_id], |row| row.get(0))
            .map_err(|e| format!("Failed to query extractions: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        texts.join(" ")
    };

    fts_index_item(conn, item_id, &title, &metadata, &extracted_text)
}

/// Upsert a document into `fts_items`.
///
/// FTS5 contentless tables don't support UPDATE — we delete then re-insert.
pub fn fts_index_item(
    conn: &Connection,
    item_id: &str,
    title: &str,
    metadata: &str,
    extracted_text: &str,
) -> Result<(), String> {
    // Delete existing entry (no-op if not present)
    conn.execute("DELETE FROM fts_items WHERE item_id = ?1", params![item_id])
        .map_err(|e| format!("FTS5 delete failed: {e}"))?;

    // Insert fresh entry
    conn.execute(
        "INSERT INTO fts_items(item_id, title, metadata, extracted_text) VALUES (?1, ?2, ?3, ?4)",
        params![item_id, title, metadata, extracted_text],
    )
    .map_err(|e| format!("FTS5 insert failed: {e}"))?;

    Ok(())
}

/// Search `fts_items` using a FTS5 MATCH expression.
///
/// `query` must already be sanitized by the caller (use `sanitize_fts5_query`).
/// Results are ordered by BM25 rank (most relevant first).
///
/// If `collection_id` is provided, results are filtered via a JOIN to `items`.
pub fn fts_search(
    conn: &Connection,
    query: &str,
    collection_id: Option<&str>,
) -> Result<Vec<FtsRow>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let sanitized = sanitize_fts5_query(query);
    if sanitized.is_empty() {
        return Ok(vec![]);
    }

    let rows = if let Some(cid) = collection_id {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT f.item_id, f.title, bm25(fts_items) AS rank
                FROM fts_items f
                JOIN items i ON f.item_id = i.id
                WHERE f.fts_items MATCH ?1
                  AND i.collection_id = ?2
                ORDER BY rank
                "#,
            )
            .map_err(|e| format!("Failed to prepare FTS5 search: {e}"))?;

        let mapped = stmt
            .query_map(params![sanitized.as_str(), cid], |row| {
                Ok(FtsRow {
                    item_id: row.get(0)?,
                    title: row.get(1)?,
                    rank: row.get(2)?,
                })
            })
            .map_err(|e| format!("FTS5 search failed: {e}"))?;

        let mut collected = Vec::new();
        for row in mapped {
            if let Ok(row) = row {
                collected.push(row);
            }
        }
        collected
    } else {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT item_id, title, bm25(fts_items) AS rank
                FROM fts_items
                WHERE fts_items MATCH ?1
                ORDER BY rank
                "#,
            )
            .map_err(|e| format!("Failed to prepare FTS5 search: {e}"))?;

        let mapped = stmt
            .query_map(params![sanitized.as_str()], |row| {
                Ok(FtsRow {
                    item_id: row.get(0)?,
                    title: row.get(1)?,
                    rank: row.get(2)?,
                })
            })
            .map_err(|e| format!("FTS5 search failed: {e}"))?;

        let mut collected = Vec::new();
        for row in mapped {
            if let Ok(row) = row {
                collected.push(row);
            }
        }
        collected
    };

    Ok(rows)
}

/// Sanitize a raw user query to be safe for FTS5 MATCH.
///
/// Strips FTS5 operators (AND, OR, NOT, NEAR, *, -, ^) and special chars
/// (quotes, parentheses). Collapses whitespace and trims.
pub fn sanitize_fts5_query(raw: &str) -> String {
    // Remove FTS5 special characters
    let cleaned = raw
        .replace('"', "")
        .replace('(', "")
        .replace(')', "")
        .replace('*', "")
        .replace('-', " ")
        .replace('^', "");

    // Remove FTS5 boolean operators (case-insensitive, whole word)
    let mut words: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| {
            let up = w.to_ascii_uppercase();
            !matches!(up.as_str(), "AND" | "OR" | "NOT" | "NEAR")
        })
        .collect();

    // Deduplicate consecutive identical words
    words.dedup();

    words.join(" ")
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_fts5_query ──────────────────────────────────────────────────

    #[test]
    fn plain_text_passes_through() {
        assert_eq!(sanitize_fts5_query("buenos aires"), "buenos aires");
    }

    #[test]
    fn removes_boolean_and() {
        assert_eq!(sanitize_fts5_query("foo AND bar"), "foo bar");
    }

    #[test]
    fn removes_boolean_or() {
        assert_eq!(sanitize_fts5_query("foo OR bar"), "foo bar");
    }

    #[test]
    fn removes_boolean_not() {
        assert_eq!(sanitize_fts5_query("foo NOT bar"), "foo bar");
    }

    #[test]
    fn removes_near_operator() {
        assert_eq!(sanitize_fts5_query("foo NEAR bar"), "foo bar");
    }

    #[test]
    fn removes_asterisk() {
        assert_eq!(sanitize_fts5_query("histo*"), "histo");
    }

    #[test]
    fn removes_quotes() {
        assert_eq!(sanitize_fts5_query(r#""exact phrase""#), "exact phrase");
    }

    #[test]
    fn removes_parentheses() {
        assert_eq!(sanitize_fts5_query("(foo OR bar)"), "foo bar");
    }

    #[test]
    fn removes_hyphen() {
        assert_eq!(sanitize_fts5_query("foo-bar"), "foo bar");
    }

    #[test]
    fn removes_caret() {
        assert_eq!(sanitize_fts5_query("foo^bar"), "foobar");
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(sanitize_fts5_query(""), "");
    }

    #[test]
    fn only_operators_returns_empty() {
        assert_eq!(sanitize_fts5_query("AND OR NOT"), "");
    }

    #[test]
    fn mixed_case_operators_removed() {
        assert_eq!(sanitize_fts5_query("foo and bar"), "foo and bar"); // lowercase 'and' is a word, not operator
        assert_eq!(sanitize_fts5_query("foo AND bar"), "foo bar");
    }

    #[test]
    fn collapses_extra_whitespace() {
        assert_eq!(sanitize_fts5_query("  foo   bar  "), "foo bar");
    }

    // ── In-memory FTS5 tests ─────────────────────────────────────────────────

    fn setup_fts_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB failed");
        conn.execute_batch(
            r#"
            CREATE VIRTUAL TABLE fts_items USING fts5(
                item_id UNINDEXED,
                title,
                metadata,
                extracted_text,
                tokenize = 'unicode61 remove_diacritics 1',
                content = ''
            );
            "#,
        )
        .expect("FTS5 table creation failed");
        conn
    }

    #[test]
    fn fts_index_and_search_basic() {
        let conn = setup_fts_db();
        fts_index_item(
            &conn,
            "item-1",
            "Historia Colonial",
            "",
            "Buenos Aires 1810",
        )
        .expect("index failed");

        let results = fts_search(&conn, "colonial", None).expect("search failed");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item_id, "item-1");
    }

    #[test]
    fn fts_search_returns_empty_for_no_match() {
        let conn = setup_fts_db();
        fts_index_item(&conn, "item-1", "Historia Colonial", "", "Buenos Aires")
            .expect("index failed");

        let results = fts_search(&conn, "azteca", None).expect("search failed");
        assert!(results.is_empty());
    }

    #[test]
    fn fts_search_empty_query_returns_empty() {
        let conn = setup_fts_db();
        let results = fts_search(&conn, "", None).expect("search failed");
        assert!(results.is_empty());
    }

    #[test]
    fn fts_index_upsert_replaces_previous_entry() {
        let conn = setup_fts_db();
        fts_index_item(&conn, "item-1", "Título Original", "", "texto viejo")
            .expect("first index failed");
        fts_index_item(&conn, "item-1", "Título Actualizado", "", "texto nuevo")
            .expect("second index failed");

        let old_results = fts_search(&conn, "viejo", None).expect("search failed");
        assert!(old_results.is_empty(), "Old content should be replaced");

        let new_results = fts_search(&conn, "nuevo", None).expect("search failed");
        assert_eq!(new_results.len(), 1);
        assert_eq!(new_results[0].title, "Título Actualizado");
    }

    #[test]
    fn fts_search_ranks_by_relevance() {
        let conn = setup_fts_db();
        fts_index_item(&conn, "item-1", "Historia", "", "guerra guerra guerra")
            .expect("index 1 failed");
        fts_index_item(&conn, "item-2", "Historia", "", "guerra").expect("index 2 failed");

        let results = fts_search(&conn, "guerra", None).expect("search failed");
        assert_eq!(results.len(), 2);
        // item-1 has higher term frequency — should rank first (lower BM25 = more relevant in SQLite)
        assert_eq!(results[0].item_id, "item-1");
    }
}
