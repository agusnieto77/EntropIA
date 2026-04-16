/// Embedding computation using fastembed `all-MiniLM-L6-v2` (384 dimensions).
///
/// `EmbeddingEngine` is initialized once and reused. If the model fails to load,
/// `compute_and_store` logs a warning and returns `Ok(())` — embeddings are
/// degraded silently; FTS5 and NER continue unaffected.
use rusqlite::{params, Connection};

use super::text_provider;

// ── Public API ───────────────────────────────────────────────────────────────

/// Compute embedding for item's text (extractions + transcriptions) and store it in `vec_items`.
///
/// Graceful fallback: if fastembed or sqlite-vec is unavailable, logs a warning
/// and returns `Ok(())` without modifying the database.
pub fn compute_and_store(conn: &Connection, item_id: &str) -> Result<(), String> {
    // Fetch concatenated text from both extractions and transcriptions
    let text = text_provider::get_item_text(conn, item_id)?;
    if text.trim().is_empty() {
        return Ok(()); // Nothing to embed — not an error
    }

    // Attempt to compute embedding via fastembed
    let vector = match embed_text(&text) {
        Ok(v) => v,
        Err(e) => {
            // Non-fatal degradation
            eprintln!("{}", embedding_degradation_log(item_id, &e));
            return Ok(());
        }
    };

    // Attempt to upsert into vec_items (requires sqlite-vec loaded)
    let blob = floats_to_blob(&vector);
    upsert_vec_item(conn, item_id, &blob)
}

/// Embed a single text string and return a 384-dimensional float vector.
///
/// Returns `Err` if fastembed fails; callers should treat this as a non-fatal
/// degradation.
pub fn embed_text(text: &str) -> Result<Vec<f32>, String> {
    #[cfg(not(feature = "embeddings"))]
    {
        let _ = text;
        return Err("Embeddings feature is disabled at compile time".to_string());
    }

    #[cfg(feature = "embeddings")]
    {
        #[cfg(windows)]
        use fastembed_shim::{EmbeddingModel, InitOptions, TextEmbedding};
        #[cfg(not(windows))]
        use fastembed_upstream::{EmbeddingModel, InitOptions, TextEmbedding};

        let mut model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(false),
        )
        .map_err(|e| format!("Failed to load fastembed model: {e}"))?;

        let embeddings = model
            .embed(vec![text.to_string()], None)
            .map_err(|e| format!("fastembed embedding failed: {e}"))?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "fastembed returned empty embeddings".to_string())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Serialize `Vec<f32>` to little-endian bytes for sqlite-vec BLOB storage.
fn floats_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn embedding_degradation_log(item_id: &str, reason: &str) -> String {
    format!("[nlp/embeddings] Skipping embedding for {item_id}: {reason}")
}

fn upsert_vec_item(conn: &Connection, item_id: &str, blob: &[u8]) -> Result<(), String> {
    let result = conn.execute(
        "INSERT OR REPLACE INTO vec_items(item_id, embedding) VALUES (?1, ?2)",
        params![item_id, blob],
    );

    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            // vec0 table might not exist if sqlite-vec wasn't loaded — degrade gracefully
            eprintln!("[nlp/embeddings] vec_items upsert failed for {item_id}: {e}");
            Ok(())
        }
    }
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_to_blob_produces_correct_byte_count() {
        let v = vec![1.0_f32, 2.0_f32, 3.0_f32];
        let blob = floats_to_blob(&v);
        assert_eq!(blob.len(), 3 * 4, "Each f32 should produce 4 bytes");
    }

    #[test]
    fn floats_to_blob_round_trips_correctly() {
        let original = vec![1.5_f32, -0.5_f32, 100.0_f32];
        let blob = floats_to_blob(&original);
        let recovered: Vec<f32> = blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(recovered, original);
    }

    #[test]
    fn empty_vec_produces_empty_blob() {
        let blob = floats_to_blob(&[]);
        assert!(blob.is_empty());
    }

    #[test]
    fn upsert_vec_item_degrades_gracefully_when_vec_items_table_missing() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        let result = upsert_vec_item(&conn, "item-1", &[1, 2, 3, 4]);
        assert!(
            result.is_ok(),
            "missing vec_items table must not fail embedding pipeline"
        );
    }

    #[test]
    fn upsert_vec_item_writes_when_vec_items_table_exists() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute(
            "CREATE TABLE vec_items(item_id TEXT PRIMARY KEY, embedding BLOB NOT NULL)",
            [],
        )
        .expect("vec_items table should be created");

        let result = upsert_vec_item(&conn, "item-1", &[1, 2, 3, 4]);
        assert!(result.is_ok(), "upsert should pass when table exists");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vec_items WHERE item_id = 'item-1'",
                [],
                |row| row.get(0),
            )
            .expect("row count query should succeed");
        assert_eq!(count, 1);
    }

    #[test]
    fn embedding_degradation_log_includes_item_id_and_reason() {
        let message =
            embedding_degradation_log("item-42", "Embeddings feature is disabled at compile time");
        assert!(
            message.contains("item-42"),
            "log message must include item id for operational diagnosis"
        );
        assert!(
            message.contains("Embeddings feature is disabled at compile time"),
            "log message must include degradation reason"
        );
    }

    #[test]
    fn embedding_degradation_log_keeps_expected_prefix_for_grepability() {
        let message = embedding_degradation_log("item-99", "fastembed embedding failed");
        assert!(
            message.starts_with("[nlp/embeddings] Skipping embedding for "),
            "log message prefix should remain stable for observability tooling"
        );
    }

    #[cfg(not(feature = "embeddings"))]
    #[test]
    fn compute_and_store_degrades_gracefully_when_feature_is_disabled() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(
            "
            CREATE TABLE items (
              id TEXT PRIMARY KEY,
              collection_id TEXT,
              title TEXT NOT NULL,
              metadata TEXT
            );
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
              text_content TEXT,
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
            ",
        )
        .expect("schema should be created");

        conn.execute(
            "INSERT INTO items(id, collection_id, title, metadata) VALUES (?1, ?2, ?3, ?4)",
            params!["item-1", "col-1", "Title", "{}"],
        )
        .expect("item should be inserted");

        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["asset-1", "item-1", "asset.txt", "txt", 1_i64],
        )
        .expect("asset should be inserted");
        conn.execute(
            "INSERT INTO extractions(id, asset_id, text_content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["ext-1", "asset-1", "texto para embedding", 2_i64],
        )
        .expect("extraction should be inserted");

        let result = compute_and_store(&conn, "item-1");
        assert!(
            result.is_ok(),
            "feature-disabled embeddings path must degrade non-fatally"
        );
    }
}
