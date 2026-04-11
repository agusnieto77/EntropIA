/// Embedding computation using fastembed `all-MiniLM-L6-v2` (384 dimensions).
///
/// `EmbeddingEngine` is initialized once and reused. If the model fails to load,
/// `compute_and_store` logs a warning and returns `Ok(())` — embeddings are
/// degraded silently; FTS5 and NER continue unaffected.
use rusqlite::{params, Connection};

// ── Public API ───────────────────────────────────────────────────────────────

/// Compute embedding for item's extracted text and store it in `vec_items`.
///
/// Graceful fallback: if fastembed or sqlite-vec is unavailable, logs a warning
/// and returns `Ok(())` without modifying the database.
pub fn compute_and_store(conn: &Connection, item_id: &str) -> Result<(), String> {
    // Fetch extracted text for the item (latest extraction)
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
        Some(t) if !t.is_empty() => t,
        _ => return Ok(()), // Nothing to embed — not an error
    };

    // Attempt to compute embedding via fastembed
    let vector = match embed_text(&text) {
        Ok(v) => v,
        Err(e) => {
            // Non-fatal degradation
            eprintln!("[nlp/embeddings] Skipping embedding for {item_id}: {e}");
            return Ok(());
        }
    };

    // Attempt to upsert into vec_items (requires sqlite-vec loaded)
    let blob = floats_to_blob(&vector);
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
        use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

        let model = TextEmbedding::try_new(
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
}
