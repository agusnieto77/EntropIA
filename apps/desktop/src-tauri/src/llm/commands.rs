use tauri::State;

use super::LlmQueue;
use super::LlmJob;
use super::LlmResultEntry;
use crate::db::state::AppDbState;

#[tauri::command]
pub async fn llm_correct_ocr(
    item_id: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::CorrectOcr { item_id })?;
    Ok("queued".to_string())
}

#[tauri::command]
pub async fn llm_extract_entities(
    item_id: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::ExtractEntities { item_id })?;
    Ok("queued".to_string())
}

#[tauri::command]
pub async fn llm_extract_triples(
    item_id: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::ExtractTriples { item_id })?;
    Ok("queued".to_string())
}

#[tauri::command]
pub async fn llm_summarize(
    item_id: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::Summarize { item_id })?;
    Ok("queued".to_string())
}

#[tauri::command]
pub async fn llm_classify(
    item_id: String,
    categories: Vec<String>,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::Classify { item_id, categories })?;
    Ok("queued".to_string())
}

#[tauri::command]
pub async fn llm_ask(
    collection_id: String,
    question: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::Ask { collection_id, question })?;
    Ok("queued".to_string())
}

// ── Asset-level LLM commands ──────────────────────────────────────────────────
// These operate on a single asset/page, using get_asset_text() which avoids
// concatenating all pages and prevents context-window overflow on multi-page docs.

#[tauri::command]
pub async fn llm_correct_ocr_asset(
    asset_id: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::CorrectOcrAsset { asset_id })?;
    Ok("queued".to_string())
}

#[tauri::command]
pub async fn llm_extract_entities_asset(
    asset_id: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::ExtractEntitiesAsset { asset_id })?;
    Ok("queued".to_string())
}

#[tauri::command]
pub async fn llm_extract_triples_asset(
    asset_id: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::ExtractTriplesAsset { asset_id })?;
    Ok("queued".to_string())
}

#[tauri::command]
pub async fn llm_summarize_asset(
    asset_id: String,
    llm_queue: State<'_, LlmQueue>,
) -> Result<String, String> {
    llm_queue.submit(LlmJob::SummarizeAsset { asset_id })?;
    Ok("queued".to_string())
}

/// Retrieve all latest LLM results for a given target (item or collection).
/// Returns one result per job_type, ordered by most recent first.
#[tauri::command]
pub async fn llm_get_results(
    target_id: String,
    db: State<'_, AppDbState>,
) -> Result<Vec<LlmResultEntry>, String> {
    let conn = db.ui_conn.lock().map_err(|e| format!("DB lock error: {e}"))?;
    super::get_all_results_for_target(&conn, &target_id)
}

/// Retrieve the latest single LLM result for a target + job_type.
#[tauri::command]
pub async fn llm_get_result(
    target_id: String,
    job_type: String,
    db: State<'_, AppDbState>,
) -> Result<Option<LlmResultEntry>, String> {
    let conn = db.ui_conn.lock().map_err(|e| format!("DB lock error: {e}"))?;
    super::get_latest_result(&conn, &target_id, Some(&job_type))
}