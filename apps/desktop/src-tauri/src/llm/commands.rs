use serde::Serialize;
use tauri::State;

use super::LlmQueue;
use super::LlmJob;

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
