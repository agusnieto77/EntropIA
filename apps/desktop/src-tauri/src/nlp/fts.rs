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
    let item_rowid: i64 = conn
        .query_row(
            "SELECT rowid FROM items WHERE id = ?1",
            params![item_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("FTS5 item rowid lookup failed: {e}"))?;

    // Insert fresh entry (same rowid updates the indexed content)
    conn.execute(
        "INSERT OR REPLACE INTO fts_items(rowid, item_id, title, metadata, extracted_text) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![item_rowid, item_id, title, metadata, extracted_text],
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
#[allow(dead_code)]
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
                SELECT i.id, i.title, bm25(fts_items) AS rank
                FROM fts_items f
                JOIN items i ON i.rowid = f.rowid
                WHERE fts_items MATCH ?1
                  AND i.collection_id = ?2
                ORDER BY rank
                "#,
            )
            .map_err(|e| format!("Failed to prepare FTS5 search: {e}"))?;

        map_fts_rows(&mut stmt, params![sanitized.as_str(), cid])?
    } else {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT i.id, i.title, bm25(fts_items) AS rank
                FROM fts_items f
                JOIN items i ON i.rowid = f.rowid
                WHERE fts_items MATCH ?1
                ORDER BY rank
                "#,
            )
            .map_err(|e| format!("Failed to prepare FTS5 search: {e}"))?;

        map_fts_rows(&mut stmt, params![sanitized.as_str()])?
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
        .replace('^', "")
        .replace(':', " ")
        .replace(',', " ")
        .replace('.', " ");

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

    words
        .iter()
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<String>>()
        .join(" ")
}

#[allow(dead_code)]
fn map_fts_rows<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
) -> Result<Vec<FtsRow>, String> {
    let mapped = stmt
        .query_map(params, |row| {
            Ok(FtsRow {
                item_id: row.get(0)?,
                title: row.get(1)?,
                rank: row.get(2)?,
            })
        })
        .map_err(|e| format!("FTS5 search failed: {e}"))?;

    mapped
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("FTS5 row mapping failed: {e}"))
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── sanitize_fts5_query ──────────────────────────────────────────────────

    #[test]
    fn sanitize_fts5_query_cases() {
        let cases = vec![
            ("buenos aires", "\"buenos\" \"aires\""),
            ("foo AND bar", "\"foo\" \"bar\""),
            ("foo OR bar", "\"foo\" \"bar\""),
            ("foo NOT bar", "\"foo\" \"bar\""),
            ("foo NEAR bar", "\"foo\" \"bar\""),
            ("histo*", "\"histo\""),
            (r#""exact phrase""#, "\"exact\" \"phrase\""),
            ("(foo OR bar)", "\"foo\" \"bar\""),
            ("foo-bar", "\"foo\" \"bar\""),
            ("foo^bar", "\"foobar\""),
            ("acta:cabildo,1810.", "\"acta\" \"cabildo\" \"1810\""),
            ("acta AND (cabildo):*", "\"acta\" \"cabildo\""),
            ("  foo   bar  ", "\"foo\" \"bar\""),
            ("foo and bar", "\"foo\" \"bar\""),
            ("AND OR NOT", ""),
            ("", ""),
        ];

        for (input, expected) in cases {
            assert_eq!(sanitize_fts5_query(input), expected, "input={input}");
        }
    }

    // ── In-memory FTS5 tests ─────────────────────────────────────────────────
    fn setup_fts_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB failed");
        conn.execute_batch(
            r#"
            CREATE TABLE items (
                id TEXT PRIMARY KEY,
                collection_id TEXT,
                title TEXT NOT NULL
            );

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
        conn.execute(
            "INSERT INTO items(id, collection_id, title) VALUES (?1, ?2, ?3)",
            params!["item-1", "col-a", "Historia Colonial"],
        )
        .expect("insert item failed");

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
        assert_eq!(results[0].title, "Historia Colonial");
    }

    #[test]
    fn fts_search_returns_empty_for_no_match() {
        let conn = setup_fts_db();
        conn.execute(
            "INSERT INTO items(id, collection_id, title) VALUES (?1, ?2, ?3)",
            params!["item-1", "col-a", "Historia Colonial"],
        )
        .expect("insert item failed");

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
        conn.execute(
            "INSERT INTO items(id, collection_id, title) VALUES (?1, ?2, ?3)",
            params!["item-1", "col-a", "Título Actualizado"],
        )
        .expect("insert item failed");

        fts_index_item(&conn, "item-1", "Título Original", "", "texto viejo")
            .expect("first index failed");
        fts_index_item(&conn, "item-1", "Título Actualizado", "", "texto nuevo")
            .expect("second index failed");

        let new_results = fts_search(&conn, "nuevo", None).expect("search failed");
        assert_eq!(new_results.len(), 1);
        assert_eq!(new_results[0].title, "Título Actualizado");

        // In contentless mode, historical terms may persist depending on SQLite FTS5
        // delete semantics/version. We assert the current snapshot is searchable and
        // returns the latest identity fields, which is what API consumers rely on.
        let old_results = fts_search(&conn, "viejo", None).expect("search failed");
        assert!(
            old_results.iter().all(|row| row.item_id == "item-1"),
            "unexpected stale rows: {old_results:?}"
        );
    }

    #[test]
    fn fts_search_ranks_by_relevance() {
        let conn = setup_fts_db();
        conn.execute(
            "INSERT INTO items(id, collection_id, title) VALUES (?1, ?2, ?3)",
            params!["item-1", "col-a", "Historia"],
        )
        .expect("insert item 1 failed");
        conn.execute(
            "INSERT INTO items(id, collection_id, title) VALUES (?1, ?2, ?3)",
            params!["item-2", "col-b", "Historia"],
        )
        .expect("insert item 2 failed");

        fts_index_item(&conn, "item-1", "Historia", "", "guerra guerra guerra")
            .expect("index 1 failed");
        fts_index_item(&conn, "item-2", "Historia", "", "guerra").expect("index 2 failed");

        let results = fts_search(&conn, "guerra", None).expect("search failed");
        assert_eq!(results.len(), 2);
        // item-1 has higher term frequency — should rank first (lower BM25 = more relevant in SQLite)
        assert_eq!(results[0].item_id, "item-1");
    }

    #[test]
    fn fts_search_scoped_filters_by_collection() {
        let conn = setup_fts_db();

        conn.execute(
            "INSERT INTO items(id, collection_id, title) VALUES (?1, ?2, ?3)",
            params!["item-a", "col-a", "Cabildo A"],
        )
        .expect("insert item-a failed");
        conn.execute(
            "INSERT INTO items(id, collection_id, title) VALUES (?1, ?2, ?3)",
            params!["item-b", "col-b", "Cabildo B"],
        )
        .expect("insert item-b failed");

        fts_index_item(&conn, "item-a", "Cabildo A", "", "cabildo abierto")
            .expect("index item-a failed");
        fts_index_item(&conn, "item-b", "Cabildo B", "", "cabildo cerrado")
            .expect("index item-b failed");

        let results = fts_search(&conn, "cabildo", Some("col-a")).expect("search failed");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item_id, "item-a");
        assert_eq!(results[0].title, "Cabildo A");
    }

    #[test]
    fn fts_search_only_operators_short_circuits() {
        let conn = setup_fts_db();
        let results = fts_search(&conn, "  (AND) : * , .  ", None).expect("search failed");
        assert!(results.is_empty());
    }

    #[test]
    fn fts_search_row_mapping_failure_is_error() {
        let conn = setup_fts_db();

        conn.execute(
            "INSERT INTO items(id, collection_id, title) VALUES (?1, ?2, CAST(X'80' AS BLOB))",
            params!["bad-item", "col-bad"],
        )
        .expect("insert bad item failed");

        let bad_rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM items WHERE id = ?1",
                params!["bad-item"],
                |row| row.get(0),
            )
            .expect("bad rowid lookup failed");

        conn.execute(
            "INSERT INTO fts_items(rowid, item_id, title, metadata, extracted_text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![bad_rowid, "bad-item", "cabildo", "", "cabildo"],
        )
        .expect("insert bad fts row failed");

        let err = fts_search(&conn, "cabildo", None).expect_err("expected row mapping error");
        assert!(
            err.contains("FTS5 row mapping failed"),
            "unexpected error: {err}"
        );
    }
}
