/// Tauri IPC commands for OCR operations.

use super::{OcrJob, OcrQueue};
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
/// * `ocr_queue`  — managed state injected by Tauri
#[tauri::command]
pub async fn extract_text(
    asset_id: String,
    asset_path: String,
    asset_type: String,
    ocr_queue: State<'_, OcrQueue>,
) -> Result<String, String> {
    let job = OcrJob {
        asset_id,
        asset_path,
        asset_type,
    };

    ocr_queue.submit(job)?;
    Ok("queued".to_string())
}
