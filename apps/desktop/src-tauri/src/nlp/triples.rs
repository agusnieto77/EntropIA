use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::text_provider;

#[derive(Debug, Clone, PartialEq)]
pub struct Triple {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

#[derive(Debug, Deserialize)]
struct TriplePayload {
    subject: String,
    predicate: String,
    object: String,
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

fn extract_triples_with_spacy(text: &str) -> Result<Vec<Triple>, String> {
    let python = crate::python_discovery::which_python_for_module(
        "nlp/triples",
        "spacy",
        "spacy+es_core_news_sm",
        "import spacy, es_core_news_sm; print('ok')",
        None,
    )
    .ok_or_else(|| "No Python with spaCy/es_core_news_sm found".to_string())?;

    let script_path = resolve_spacy_triples_script();
    if !script_path.exists() {
        return Err(format!(
            "spaCy triples script not found: {}",
            script_path.display()
        ));
    }

    let mut cmd = Command::new(&python);
    crate::python_discovery::apply_windows_no_window(&mut cmd);
    cmd.arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn spaCy triples script: {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("Failed to send text to spaCy triples script: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for spaCy triples script: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(format!(
            "spaCy triples script failed (python={}, script={}): {}",
            python.display(),
            script_path.display(),
            stderr.trim()
        ));
    }

    let json = extract_sentinel_json(&stdout);
    let parsed: Vec<TriplePayload> = serde_json::from_str(json).map_err(|e| {
        format!(
            "Failed to parse spaCy triples JSON: {e}; stdout={}, stderr={}",
            stdout.trim(),
            stderr.trim()
        )
    })?;

    Ok(parsed
        .into_iter()
        .filter_map(|t| {
            let subject = t.subject.trim().to_string();
            let predicate = t.predicate.trim().to_string();
            let object = t.object.trim().to_string();
            if subject.is_empty() || predicate.is_empty() || object.is_empty() {
                return None;
            }
            Some(Triple {
                subject,
                predicate,
                object,
            })
        })
        .collect())
}

fn extract_triples_best_effort(text: &str) -> Vec<Triple> {
    match extract_triples_with_spacy(text) {
        Ok(triples) if !triples.is_empty() => triples,
        Ok(_) => extract_triples(text),
        Err(e) => {
            eprintln!("[nlp/triples] spaCy unavailable, falling back to regex triples: {e}");
            extract_triples(text)
        }
    }
}

pub fn extract_and_store(conn: &Connection, item_id: &str) -> Result<(), String> {
    let text = text_provider::get_item_text(conn, item_id)?;
    if text.trim().is_empty() {
        conn.execute("DELETE FROM triples WHERE item_id = ?1", params![item_id])
            .map_err(|e| format!("Failed to delete old triples for empty input: {e}"))?;
        return Ok(());
    }

    let triples = extract_triples_best_effort(&text);

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

/// Extract triples for a single asset/page (asset-level processing).
///
/// Similar to `extract_and_store`, but only processes the text for the given
/// `asset_id` and stores triples with both `item_id` and `asset_id` for
/// per-page filtering in the UI.
pub fn extract_and_store_for_asset(conn: &Connection, item_id: &str, asset_id: &str) -> Result<(), String> {
    let text = text_provider::get_asset_text(conn, asset_id)?;
    if text.trim().is_empty() {
        conn.execute("DELETE FROM triples WHERE item_id = ?1 AND asset_id = ?2", params![item_id, asset_id])
            .map_err(|e| format!("Failed to delete old triples for empty asset: {e}"))?;
        return Ok(());
    }

    let triples = extract_triples_best_effort(&text);

    // Delete old triples for this specific asset only
    conn.execute("DELETE FROM triples WHERE item_id = ?1 AND asset_id = ?2", params![item_id, asset_id])
        .map_err(|e| format!("Failed to delete old triples for asset: {e}"))?;

    for triple in triples {
        conn.execute(
            r#"
            INSERT INTO triples (id, item_id, asset_id, subject, predicate, object, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                uuid_v4(),
                item_id,
                asset_id,
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

fn resolve_spacy_triples_script() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base = PathBuf::from(manifest_dir);
    let direct = base.join("scripts").join("spacy_triples.py");
    if direct.exists() {
        return direct;
    }
    base.join("resources").join("scripts").join("spacy_triples.py")
}

fn extract_sentinel_json(output: &str) -> &str {
    const BEGIN: &str = "===TRIPLES_JSON_BEGIN===";
    const END: &str = "===TRIPLES_JSON_END===";
    if let Some(start_idx) = output.find(BEGIN) {
        let content_start = start_idx + BEGIN.len();
        if let Some(end_idx) = output[content_start..].find(END) {
            return output[content_start..content_start + end_idx].trim();
        }
    }
    output.trim()
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

            CREATE TABLE transcriptions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT NOT NULL,
              language TEXT,
              duration_ms INTEGER,
              model TEXT NOT NULL,
              segments TEXT,
              confidence REAL,
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

        // With get_item_text, both extractions are concatenated ASC by created_at:
        // "Belgrano creó la Bandera. San Martín fue gobernador de Cuyo. Belgrano creó la Escarapela."
        // This produces 3 triples:
        //   1. Belgrano creó la Bandera
        //   2. San Martín fue gobernador de Cuyo
        //   3. Belgrano creó la Escarapela
        let second_run_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1",
                params!["item-rerun"],
                |row| row.get(0),
            )
            .expect("second run count");
        assert_eq!(second_run_count, 3);

        // All old triples are replaced (no stale "la Bandera" triple from old extraction alone)
        // But "la Bandera" appears in the combined text triple
        let bandera_triple_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1 AND object = ?2",
                params!["item-rerun", "la Bandera"],
                |row| row.get(0),
            )
            .expect("bandera triple check");
        assert_eq!(
            bandera_triple_count, 1,
            "la Bandera should appear once from combined text"
        );

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
                "la Bandera".to_string(),
                "la Escarapela".to_string()
            ]
        );
    }

    #[test]
    fn extract_and_store_extracts_triples_from_transcription_only_text() {
        let conn = setup_test_db();

        conn.execute(
            "INSERT INTO assets (id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                "asset-trans-tri",
                "item-trans-tri",
                "audio.mp3",
                "audio",
                1_i64
            ],
        )
        .expect("asset insert");

        // No extractions — only a transcription with rule-based sentences
        conn.execute(
            "INSERT INTO transcriptions (id, asset_id, text_content, language, duration_ms, model, segments, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params!["trans-tri-1", "asset-trans-tri", "Belgrano creó la Bandera. San Martín fue gobernador de Cuyo.", "es", 5000_i64, "base", "[]", 0.9_f64, 10_i64],
        )
        .expect("transcription insert");

        extract_and_store(&conn, "item-trans-tri").expect("extract_and_store from transcription");

        let triple_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1",
                params!["item-trans-tri"],
                |row| row.get(0),
            )
            .expect("triple count should be queryable");

        assert!(
            triple_count >= 2,
            "Triples should be extracted from transcription-only text, found {triple_count}"
        );
    }

    #[test]
    fn extract_and_store_extracts_triples_from_combined_text() {
        let conn = setup_test_db();

        conn.execute(
            "INSERT INTO assets (id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                "asset-combo-tri",
                "item-combo-tri",
                "page.png",
                "image",
                1_i64
            ],
        )
        .expect("asset insert");

        // Extraction text
        conn.execute(
            "INSERT INTO extractions (id, asset_id, text_content, method, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params!["ext-combo-tri", "asset-combo-tri", "Documento histórico.", "ocr", 0.95_f64, 5_i64],
        )
        .expect("extraction insert");

        // Transcription adds triple-bearing sentences
        conn.execute(
            "INSERT INTO transcriptions (id, asset_id, text_content, language, duration_ms, model, segments, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params!["trans-combo-tri", "asset-combo-tri", "Artigas fundó la Universidad de la República.", "es", 3000_i64, "base", "[]", 0.85_f64, 10_i64],
        )
        .expect("transcription insert");

        extract_and_store(&conn, "item-combo-tri").expect("extract_and_store from combined text");

        let triple_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1",
                params!["item-combo-tri"],
                |row| row.get(0),
            )
            .expect("triple count should be queryable");

        assert!(
            triple_count > 0,
            "Triples should be extracted from combined extraction + transcription text"
        );
    }
}
