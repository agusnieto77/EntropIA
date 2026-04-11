use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq)]
pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

static TRIPLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^\s*(?P<subject>[^.,;:]+?)\s+(?P<predicate>es|fue|era|firm[oó]|fund[oó]|cre[oó]|dirig[ií]a|lider[oó]|naci[oó]\s+en|muri[oó]\s+en)\s+(?P<object>[^.,;:]+?)\s*$",
    )
    .expect("TRIPLE_RE failed to compile")
});

pub fn extract_triples(text: &str) -> Vec<Triple> {
    text.split(['.', ';', '\n'])
        .filter_map(|sentence| {
            let sentence = sentence.trim();
            if sentence.is_empty() {
                return None;
            }

            let caps = TRIPLE_RE.captures(sentence)?;
            let subject = caps.name("subject")?.as_str().trim();
            let predicate = caps.name("predicate")?.as_str().trim();
            let object = caps.name("object")?.as_str().trim();

            if subject.is_empty() || object.is_empty() {
                return None;
            }

            Some(Triple {
                subject: subject.to_string(),
                predicate: predicate.to_string(),
                object: object.to_string(),
            })
        })
        .collect()
}

pub fn extract_and_store(conn: &Connection, item_id: &str) -> Result<(), String> {
    let text: Option<String> = conn
        .query_row(
            r#"
            SELECT e.text_content
            FROM extractions e
            JOIN assets a ON e.asset_id = a.id
            WHERE a.item_id = ?1
            ORDER BY e.created_at DESC
            LIMIT 1
            "#,
            params![item_id],
            |row| row.get(0),
        )
        .ok();

    let text = match text {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            conn.execute("DELETE FROM triples WHERE item_id = ?1", params![item_id])
                .map_err(|e| format!("Failed to delete old triples for empty input: {e}"))?;
            return Ok(());
        }
    };

    let triples = extract_triples(&text);

    conn.execute("DELETE FROM triples WHERE item_id = ?1", params![item_id])
        .map_err(|e| format!("Failed to delete old triples: {e}"))?;

    for triple in triples {
        conn.execute(
            r#"
            INSERT INTO triples (id, item_id, subject, predicate, object, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                uuid_v4(),
                item_id,
                triple.subject,
                triple.predicate,
                triple.object,
                now_millis(),
            ],
        )
        .map_err(|e| format!("Failed to insert triple: {e}"))?;
    }

    Ok(())
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!(
        "{:016x}-{:08x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros(),
        nanos,
    )
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");

        conn.execute_batch(
            r#"
            CREATE TABLE assets (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              path TEXT NOT NULL,
              type TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );

            CREATE TABLE extractions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT NOT NULL,
              method TEXT NOT NULL,
              confidence REAL NOT NULL,
              created_at INTEGER NOT NULL
            );

            CREATE TABLE triples (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              subject TEXT NOT NULL,
              predicate TEXT NOT NULL,
              object TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );
            "#,
        )
        .expect("schema should be created");

        conn
    }

    #[test]
    fn extract_triples_returns_matches_for_rule_based_sentences() {
        let text = "Belgrano creó la Bandera. San Martín fue gobernador de Cuyo.";

        let triples = extract_triples(text);

        assert_eq!(triples.len(), 2);
        assert_eq!(triples[0].subject, "Belgrano");
        assert_eq!(triples[0].predicate.to_lowercase(), "creó");
        assert_eq!(triples[0].object, "la Bandera");
    }

    #[test]
    fn extract_triples_returns_empty_for_empty_text() {
        let triples = extract_triples("");
        assert!(triples.is_empty());
    }

    #[test]
    fn extract_triples_returns_empty_when_no_patterns_match() {
        let text = "Texto descriptivo sin predicado rule-based reconocido";
        let triples = extract_triples(text);
        assert!(triples.is_empty());
    }

    #[test]
    fn extract_and_store_returns_ok_and_keeps_item_without_extracted_text_empty() {
        let conn = setup_test_db();

        conn.execute(
            "INSERT INTO assets (id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["asset-1", "item-empty", "asset.pdf", "pdf", 1_i64],
        )
        .expect("asset insert");

        conn.execute(
            "INSERT INTO triples (id, item_id, subject, predicate, object, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["triple-old", "item-empty", "old-s", "old-p", "old-o", 1_i64],
        )
        .expect("seed old triple");

        let result = extract_and_store(&conn, "item-empty");
        assert!(result.is_ok());

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1",
                params!["item-empty"],
                |row| row.get(0),
            )
            .expect("count triples");

        assert_eq!(count, 0);
    }

    #[test]
    fn extract_and_store_reextract_non_empty_replaces_previous_non_empty_result_set() {
        let conn = setup_test_db();

        conn.execute(
            "INSERT INTO assets (id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["asset-reextract", "item-rerun", "asset.pdf", "pdf", 1_i64],
        )
        .expect("asset insert");

        conn.execute(
            "INSERT INTO extractions (id, asset_id, text_content, method, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "ext-old",
                "asset-reextract",
                "Belgrano creó la Bandera.",
                "ocr",
                0.95_f64,
                1_i64
            ],
        )
        .expect("old extraction insert");

        extract_and_store(&conn, "item-rerun").expect("first extraction run");

        let first_run_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1",
                params!["item-rerun"],
                |row| row.get(0),
            )
            .expect("first run count");
        assert_eq!(first_run_count, 1);

        conn.execute(
            "INSERT INTO extractions (id, asset_id, text_content, method, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "ext-new",
                "asset-reextract",
                "San Martín fue gobernador de Cuyo. Belgrano creó la Escarapela.",
                "ocr",
                0.99_f64,
                2_i64
            ],
        )
        .expect("new extraction insert");

        extract_and_store(&conn, "item-rerun").expect("second extraction run");

        let second_run_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1",
                params!["item-rerun"],
                |row| row.get(0),
            )
            .expect("second run count");
        assert_eq!(second_run_count, 2);

        let old_triple_remaining: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1 AND object = ?2",
                params!["item-rerun", "la Bandera"],
                |row| row.get(0),
            )
            .expect("old triple check");
        assert_eq!(old_triple_remaining, 0);

        let new_objects: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT object FROM triples WHERE item_id = ?1 ORDER BY object ASC")
                .expect("prepare object query");

            stmt.query_map(params!["item-rerun"], |row| row.get::<_, String>(0))
                .expect("query objects")
                .collect::<Result<Vec<_>, _>>()
                .expect("collect objects")
        };

        assert_eq!(
            new_objects,
            vec![
                "gobernador de Cuyo".to_string(),
                "la Escarapela".to_string()
            ]
        );
    }
}
