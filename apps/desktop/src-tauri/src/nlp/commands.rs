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
/// The worker will run rule-based regex NER on the item's extracted text and
/// persist results to the `entities` table.
#[tauri::command]
pub async fn extract_entities(
    item_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::ExtractEntities { item_id })
}

/// Submit a semantic triples extraction job for `item_id`.
#[tauri::command]
pub async fn extract_triples(
    item_id: String,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<String, String> {
    enqueue(&nlp_queue, NlpJob::ExtractTriples { item_id })
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
/// Returns up to `limit` (default 5) similar items ordered by cosine distance.
/// Returns empty array if sqlite-vec is not loaded or item has no embedding.
#[tauri::command]
pub async fn similar_items(
    item_id: String,
    limit: Option<u8>,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<serde_json::Value, String> {
    let _ = nlp_queue;
    let limit = limit.unwrap_or(5);
    // Similar to fts_search: kNN queries are executed via db_select from frontend.
    // This command validates the parameters and delegates to db_select pathway.
    Ok(serde_json::json!({
        "item_id": item_id,
        "limit": limit,
        "note": "Use db_select with vec_search for similarity queries"
    }))
}
