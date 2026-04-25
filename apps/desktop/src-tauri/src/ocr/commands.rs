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

/// Generate a thumbnail PNG for the first page of a PDF.
///
/// Returns the filesystem path to the cached thumbnail. The frontend should
/// use `convertFileSrc()` to turn this path into a webview-accessible URL.
///
/// Thumbnails are cached at `{app_data_dir}/thumbnails/{asset_id}.png`.
/// If a cached thumbnail already exists, the cached path is returned immediately
/// without re-rendering.
#[tauri::command]
pub async fn generate_pdf_thumbnail(
    asset_path: String,
    asset_id: String,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    use std::io::Write;
    use tauri::Manager;

    // Ensure Pdfium DLL path is initialized before any PDF operations.
    // This is a no-op if already called by the OCR worker; safe to call multiple times.
    super::pdf::init_pdfium_path(&app_handle);

    // Resolve thumbnails directory
    let app_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {e}"))?;

    let thumb_dir = app_dir.join("thumbnails");
    std::fs::create_dir_all(&thumb_dir)
        .map_err(|e| format!("Failed to create thumbnails directory: {e}"))?;

    let thumb_path = thumb_dir.join(format!("{asset_id}.png"));

    // Return cached thumbnail immediately if it exists
    if thumb_path.exists() {
        // Strip Windows \\?\ prefix if present
        let path_str = thumb_path.to_string_lossy().into_owned();
        let clean = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str).to_string();
        return Ok(clean);
    }

    // Read PDF and render thumbnail in a blocking task
    // (pdfium is CPU-intensive and must not block the async runtime)
    let result_path = tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&asset_path)
            .map_err(|e| format!("Failed to read PDF file: {e}"))?;

        let png_data = super::pdf::render_pdf_thumbnail(&bytes)?;

        // Write thumbnail to disk
        let mut file = std::fs::File::create(&thumb_path)
            .map_err(|e| format!("Failed to create thumbnail file: {e}"))?;
        file.write_all(&png_data)
            .map_err(|e| format!("Failed to write thumbnail data: {e}"))?;

        // Strip Windows \\?\ prefix if present
        let path_str = thumb_path.to_string_lossy().into_owned();
        let clean = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str).to_string();
        Ok::<String, String>(clean)
    })
    .await
    .map_err(|e| format!("Thumbnail generation task panicked: {e}"))??;

    Ok(result_path)
}

/// Delete a cached PDF thumbnail for an asset.
///
/// Called when a PDF asset is deleted to clean up the thumbnail cache.
/// Returns `Ok(())` even if the file doesn't exist (ENOENT is OK).
#[tauri::command]
pub async fn delete_pdf_thumbnail(
    asset_id: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Manager;

    let app_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {e}"))?;

    let thumb_path = app_dir.join("thumbnails").join(format!("{asset_id}.png"));

    if thumb_path.exists() {
        std::fs::remove_file(&thumb_path)
            .map_err(|e| format!("Failed to delete thumbnail: {e}"))?;
    }

    Ok(())
}
