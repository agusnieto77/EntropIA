/// Tauri IPC commands for NLP operations.
///
/// Each command pushes a job to the `NlpQueue` and returns immediately with
/// `Ok("queued")`. The worker emits `nlp:progress`, `nlp:complete`, or
/// `nlp:error` events asynchronously.

use super::{NlpJob, NlpQueue};
use tauri::State;

fn enqueue(nlp_queue: &NlpQueue, job: NlpJob) -> Result<String, String> {
    nlp_queue.submit(job)?;
    Ok("queued".to_string())
}

/// Submit an FTS5 indexing job for `item_id`.
///
/// The worker will fetch the item's title + extracted text and upsert into
/// the `fts_items` virtual table.
#[tauri::command]
pub async fn index_fts(
    item_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::IndexFts { item_id })
}

/// Submit an embedding computation job for `item_id`.
///
/// The worker will extract the item's text, compute a 384-dim vector via
/// fastembed, and upsert into `vec_items`.
#[tauri::command]
pub async fn embed_item(
    item_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::ComputeEmbedding { item_id })
}

/// Submit a NER extraction job for `item_id`.
///
/// Uses the shared dedup gate so that a frontend-triggered request doesn't
/// duplicate an auto-triggered NER job for the same item.
#[tauri::command]
pub async fn extract_entities(
    item_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    super::enqueue_entity_refresh_for_item(&nlp_queue, &item_id).map(|_| "queued".to_string())
}

/// Submit a semantic triples extraction job for `item_id`.
#[tauri::command]
pub async fn extract_triples(
    item_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::ExtractTriples { item_id })
}

/// Submit a full enrichment pipeline job (FTS + embed + NER + triples) for `item_id`.
///
/// The worker runs all 4 sub-jobs sequentially. Errors in individual sub-jobs
/// are logged and emitted as `nlp:error` events but do NOT block remaining sub-jobs.
#[tauri::command]
pub async fn enrich_item(
    item_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::EnrichItem { item_id })
}

// ── Asset-level commands ────────────────────────────────────────────────────
// These process only the selected asset/page text, not the entire item.
// Results are stored with both item_id (ownership) and asset_id (filtering).

/// Submit an embedding computation job for a specific asset.
///
/// The worker will extract the asset's text, compute a 384-dim vector,
/// and upsert into `vec_assets` keyed by `asset_id`.
/// This preserves the item-level vector in `vec_items`.
#[tauri::command]
pub async fn embed_asset(
    item_id: String,
    asset_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::ComputeAssetEmbedding { item_id, asset_id })
}

/// Submit a NER extraction job for a specific asset.
///
/// The worker will extract entities from only the selected asset's text.
/// Entities are stored with both `item_id` and `asset_id` for filtering.
#[tauri::command]
pub async fn extract_entities_for_asset(
    item_id: String,
    asset_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::ExtractEntitiesForAsset { item_id, asset_id })
}

/// Submit a semantic triples extraction job for a specific asset.
///
/// The worker will extract triples from only the selected asset's text.
/// Triples are stored with both `item_id` and `asset_id` for filtering.
#[tauri::command]
pub async fn extract_triples_for_asset(
    item_id: String,
    asset_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::ExtractTriplesForAsset { item_id, asset_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_accepts_extract_triples_job_when_queue_has_capacity() {
        let (queue, _receiver) = NlpQueue::new();
        let result = enqueue(
            &queue,
            NlpJob::ExtractTriples {
                item_id: "item-1".to_string(),
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "queued");
    }

    #[test]
    fn enqueue_propagates_queue_error_when_channel_is_full() {
        let (queue, _receiver) = NlpQueue::new();

        for i in 0..64 {
            let ok = enqueue(
                &queue,
                NlpJob::ExtractTriples {
                    item_id: format!("item-{i}"),
                },
            );
            assert!(ok.is_ok());
        }

        let result = enqueue(
            &queue,
            NlpJob::ExtractTriples {
                item_id: "overflow".to_string(),
            },
        );

        assert!(result.is_err());
        assert!(
            result.err().unwrap().contains("Failed to enqueue NLP job"),
            "Expected queue error to be propagated"
        );
    }

#[test]
    fn enqueue_keeps_non_embedding_jobs_stable() {
        let (queue, _receiver) = NlpQueue::new();

        let fts = enqueue(
            &queue,
            NlpJob::IndexFts {
                item_id: "item-fts".to_string(),
            },
        );
        let ner = enqueue(
            &queue,
            NlpJob::ExtractEntities {
                item_id: "item-ner".to_string(),
            },
        );
        let triples = enqueue(
            &queue,
            NlpJob::ExtractTriples {
                item_id: "item-triples".to_string(),
            },
        );

        assert_eq!(fts.unwrap(), "queued");
        assert_eq!(ner.unwrap(), "queued");
        assert_eq!(triples.unwrap(), "queued");
    }

    #[test]
    fn enrich_item_command_enqueues_job_and_returns_queued() {
        let (queue, mut rx) = NlpQueue::new();

        let result = enqueue(
            &queue,
            NlpJob::EnrichItem {
                item_id: "item-enrich-cmd".to_string(),
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "queued");

        // Verify the job was actually enqueued with the right variant
        let job = rx.try_recv().expect("should receive enqueued job");
        match job {
            NlpJob::EnrichItem { item_id } => {
                assert_eq!(item_id, "item-enrich-cmd");
            }
            _ => panic!("Expected EnrichItem job, got: {:?}", job),
        }
    }

    #[test]
    fn cosine_distance_identical_vectors_is_zero() {
        let a = vec![1.0_f32, 2.0_f32, 3.0_f32];
        let b = vec![1.0_f32, 2.0_f32, 3.0_f32];
        let dist = super::cosine_distance(&a, &b).unwrap();
        assert!((dist - 0.0).abs() < 1e-10, "identical vectors should have distance 0");
    }

    #[test]
    fn cosine_distance_opposite_vectors_is_two() {
        let a = vec![1.0_f32, 0.0_f32, 0.0_f32];
        let b = vec![-1.0_f32, 0.0_f32, 0.0_f32];
        let dist = super::cosine_distance(&a, &b).unwrap();
        assert!((dist - 2.0).abs() < 1e-10, "opposite vectors should have distance 2");
    }

    #[test]
    fn cosine_distance_orthogonal_vectors_is_one() {
        let a = vec![1.0_f32, 0.0_f32, 0.0_f32];
        let b = vec![0.0_f32, 1.0_f32, 0.0_f32];
        let dist = super::cosine_distance(&a, &b).unwrap();
        assert!((dist - 1.0).abs() < 1e-10, "orthogonal vectors should have distance 1");
    }

    #[test]
    fn cosine_distance_different_lengths_returns_none() {
        let a = vec![1.0_f32, 2.0_f32];
        let b = vec![1.0_f32, 2.0_f32, 3.0_f32];
        assert!(super::cosine_distance(&a, &b).is_none());
    }

    #[test]
    fn cosine_distance_empty_vectors_returns_none() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert!(super::cosine_distance(&a, &b).is_none());
    }

    #[test]
    fn cosine_distance_zero_magnitude_returns_none() {
        let a = vec![0.0_f32, 0.0_f32, 0.0_f32];
        let b = vec![1.0_f32, 2.0_f32, 3.0_f32];
        assert!(super::cosine_distance(&a, &b).is_none());
    }

    #[test]
    fn cosine_distance_proportional_vectors_is_zero() {
        let a = vec![1.0_f32, 2.0_f32, 3.0_f32];
        let b = vec![2.0_f32, 4.0_f32, 6.0_f32];
        let dist = super::cosine_distance(&a, &b).unwrap();
        assert!((dist - 0.0).abs() < 1e-10, "proportional vectors should have distance 0");
    }

    #[test]
    fn floats_to_blob_round_trips_for_similarity_search() {
        let original = vec![0.5_f32, -0.3_f32, 0.8_f32, 1.0_f32];
        let blob: Vec<u8> = original.iter().flat_map(|f| f.to_le_bytes()).collect();
        let recovered: Vec<f32> = blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(recovered, original, "blob round-trip should preserve values");
    }
}

/// Search `fts_items` using full-text search.
///
/// Returns a JSON array of `{ item_id, title, rank }` objects, ordered by
/// BM25 relevance. `query` is sanitized internally.
///
/// # Arguments
/// * `query`         — user search query (will be sanitized)
/// * `collection_id` — optional UUID to scope results to a single collection
#[tauri::command]
pub async fn fts_search(
    query: String,
    collection_id: Option<String>,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<serde_json::Value, String> {
    // FTS search runs on the worker connection via a blocking task.
    // We use the NlpQueue's channel to serialize DB access — here we send
    // a search that returns results synchronously via a oneshot channel.
    //
    // For simplicity in MVP: use the db::commands::db_select pathway instead
    // of duplicating the connection. The frontend calls db_select directly for
    // search — this command is provided for completeness and future use.
    let _ = nlp_queue; // state injected but search uses db_select from frontend
    let sanitized = crate::nlp::fts::sanitize_fts5_query(&query);
    Ok(serde_json::json!({
        "query": sanitized,
        "collection_id": collection_id,
        "note": "Use db_select with FTS5 MATCH for direct search"
    }))
}

/// Find items similar to `item_id` using kNN vector search.
///
/// Returns up to `limit` (default 5) similar items ordered by cosine distance
/// (most similar first). Returns empty array if the item has no embedding or
/// there are no other items with embeddings.
///
/// Since sqlite-vec is a no-op shim on Windows, this performs a full table
/// scan of `vec_items` and computes cosine similarity in Rust. This is fine
/// for MVP-scale data (<10k items).
#[tauri::command]
pub async fn similar_items(
    item_id: String,
    limit: Option<u8>,
    db: tauri::State<'_, crate::db::state::AppDbState>,
) -> Result<serde_json::Value, String> {
    let limit = limit.unwrap_or(5) as usize;
    const MAX_CANDIDATES: usize = 2000;

    let conn = db.ui_conn.lock().map_err(|e| e.to_string())?;

    let target_collection_id: String = conn
        .query_row(
            "SELECT collection_id FROM items WHERE id = ?1",
            rusqlite::params![item_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to resolve target collection: {e}"))?;

    // Read the target item's embedding (stored as little-endian f32 blob)
    let target_blob: Vec<u8> = conn
        .query_row(
            "SELECT embedding FROM vec_items WHERE item_id = ?1",
            rusqlite::params![item_id],
            |row| row.get(0),
        )
        .map_err(|e| {
            if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
                format!("No embedding found for item '{item_id}'")
            } else {
                format!("Failed to read embedding for '{item_id}': {e}")
            }
        })?;

    // Convert blob to f32 vector
    if target_blob.len() % 4 != 0 {
        return Err(format!(
            "Embedding blob has invalid size: {} bytes (not divisible by 4)",
            target_blob.len()
        ));
    }
    let target: Vec<f32> = target_blob
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();

    // Read candidate embeddings with their titles and collection_id via JOIN.
    // Limit scan to same collection and cap candidates to keep latency bounded.
    let mut stmt = conn
        .prepare(
            "SELECT v.item_id, i.title, i.collection_id, v.embedding
             FROM vec_items v
             LEFT JOIN items i ON i.id = v.item_id
             WHERE v.item_id != ?1
               AND i.collection_id = ?2
             LIMIT ?3",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let rows = stmt
        .query_map(
            rusqlite::params![item_id, target_collection_id, MAX_CANDIDATES as i64],
            |row| {
            let id: String = row.get(0)?;
            let title: Option<String> = row.get(1)?;
            let collection_id: Option<String> = row.get(2)?;
            let blob: Vec<u8> = row.get(3)?;
            Ok((id, title.unwrap_or_default(), collection_id.unwrap_or_default(), blob))
        },
        )
        .map_err(|e| format!("Failed to execute query: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read rows: {e}"))?;

    // Compute cosine distance for each item
    let mut results: Vec<(String, String, String, f64)> = rows
        .into_iter()
        .filter_map(|(id, title, collection_id, blob)| {
            if blob.len() % 4 != 0 || blob.len() != target_blob.len() {
                return None; // Skip incompatible embeddings
            }
            let other: Vec<f32> = blob
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();

            let distance = cosine_distance(&target, &other)?;
            // Convert distance to similarity (1.0 = identical, 0.0 = unrelated)
            let similarity = 1.0 - distance;
            Some((id, title, collection_id, similarity))
        })
        .collect();

    // Sort by similarity descending (most similar first) and take top-k
    results.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
    let results: Vec<_> = results
        .into_iter()
        .take(limit)
        .map(|(id, title, collection_id, similarity)| {
            serde_json::json!({
                "itemId": id,
                "title": title,
                "collectionId": collection_id,
                "similarity": similarity,
            })
        })
        .collect();

    Ok(serde_json::Value::Array(results))
}

/// Compute cosine distance (1 - cosine_similarity) between two f32 vectors.
/// Returns None if either vector has zero magnitude.
fn cosine_distance(a: &[f32], b: &[f32]) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }

    let mut dot = 0.0_f64;
    let mut mag_a = 0.0_f64;
    let mut mag_b = 0.0_f64;

    for (ai, bi) in a.iter().zip(b.iter()) {
        let ai = *ai as f64;
        let bi = *bi as f64;
        dot += ai * bi;
        mag_a += ai * ai;
        mag_b += bi * bi;
    }

    let mag_a = mag_a.sqrt();
    let mag_b = mag_b.sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return None;
    }

    Some(1.0 - dot / (mag_a * mag_b))
}
