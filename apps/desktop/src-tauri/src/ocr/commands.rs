/// Tauri IPC commands for OCR operations.

use super::{update_extraction_text, OcrQueue};
use crate::db::state::AppDbState;
use crate::nlp::{enqueue_entity_refresh_for_item, lookup_item_id_for_asset, NlpQueue};
use tauri::State;

/// Submit an OCR extraction job to the background worker queue.
///
/// Returns immediately with `Ok("queued")`. The worker will process the job
/// asynchronously and emit `ocr:progress`, `ocr:complete`, or `ocr:error` events.
///
/// # Arguments
/// * `asset_id`   — unique ID of the asset in the database
/// * `asset_path` — absolute filesystem path to the asset file
/// * `asset_type` — `"pdf"` or `"image"`
/// * `mode`       — `"light"` (plain PaddleOCR/Tesseract, default) or `"high"` (PaddleVL)
/// * `ocr_queue`  — managed state injected by Tauri
#[tauri::command]
pub async fn extract_text(
    asset_id: String,
    asset_path: String,
    asset_type: String,
    mode: Option<String>,
    ocr_queue: State<'_, OcrQueue>,
) -> Result<String, String> {
    let ocr_mode = match mode.as_deref() {
        Some("high") => super::OcrMode::High,
        _ => super::OcrMode::Light, // default to light
    };

    let job = super::OcrJob {
        asset_id,
        asset_path,
        asset_type,
        mode: ocr_mode,
    };

    ocr_queue.submit(job)?;
    Ok("queued".to_string())
}

/// Update the text_content of the latest extraction for an asset.
///
/// This allows users to manually correct OCR output and persist the correction.
/// The original extraction metadata (id, created_at, method, confidence) is preserved.
#[tauri::command]
pub async fn update_extraction_text_cmd(
    asset_id: String,
    text_content: String,
    db: State<'_, AppDbState>,
    nlp_queue: State<'_, NlpQueue>,
) -> Result<(), String> {
    let conn = db.ui_conn.lock().map_err(|e| format!("DB lock poisoned: {e}"))?;
    update_extraction_text(&conn, &asset_id, &text_content)?;

    if let Some(item_id) = lookup_item_id_for_asset(&conn, &asset_id)? {
        enqueue_entity_refresh_for_item(&nlp_queue, &item_id)?;
        eprintln!(
            "[nlp/ner] Auto-enqueued ExtractEntities after OCR text update: asset_id={}, item_id={}",
            asset_id,
            item_id
        );
    }

    Ok(())
}
