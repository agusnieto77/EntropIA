pub mod commands;
mod engine;
mod pdf;

use engine::OcrEngine;
use pdf::{extract_pdf_text, is_quality_text};
use crate::nlp::{enqueue_entity_refresh_for_item, lookup_item_id_for_asset, NlpQueue};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, path::BaseDirectory};
use tokio::sync::mpsc;

// ── Event payloads ──────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct OcrProgressPayload {
    pub asset_id: String,
    pub pct: u8,
    pub stage: String,
}

#[derive(Clone, Serialize)]
pub struct OcrCompletePayload {
    pub asset_id: String,
    pub method: String,
    pub text_length: usize,
    pub text_content: String,
}

#[derive(Clone, Serialize)]
pub struct OcrErrorPayload {
    pub asset_id: String,
    pub error: String,
}

// ── Job & Queue ─────────────────────────────────────────────────────────────

/// A single OCR work unit submitted to the background worker.
pub struct OcrJob {
    pub asset_id: String,
    pub asset_path: String,
    pub asset_type: String, // "pdf" | "image"
}

/// Handle for submitting jobs to the background OCR worker.
///
/// Managed as Tauri state — the `extract_text` command grabs this via `State<OcrQueue>`.
pub struct OcrQueue {
    sender: mpsc::Sender<OcrJob>,
}

impl OcrQueue {
    /// Create a new queue and return `(OcrQueue, Receiver)`.
    ///
    /// The caller is responsible for passing the receiver to [`start_worker`].
    pub fn new() -> (Self, mpsc::Receiver<OcrJob>) {
        // Bounded channel — 64 pending jobs should be more than enough for a
        // single-user desktop app. `try_send` will fail gracefully if full.
        let (sender, receiver) = mpsc::channel::<OcrJob>(64);
        (Self { sender }, receiver)
    }

    /// Submit a job to the queue. Returns immediately.
    pub fn submit(&self, job: OcrJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("Failed to enqueue OCR job: {e}"))
    }

    /// Spawn the background worker loop on the Tokio runtime.
    ///
    /// The worker:
    /// 1. Opens its own SQLite connection for persisting extractions.
    /// 2. Loads `OcrEngine` once at startup.
    /// 3. Drains jobs serially from the receiver.
    /// 4. Saves extracted text to DB, then emits events per job.
    pub fn start_worker(
        db_path: std::path::PathBuf,
        mut receiver: mpsc::Receiver<OcrJob>,
        app_handle: AppHandle,
    ) {
        tauri::async_runtime::spawn(async move {
            // Initialize Tesseract engine — if this fails, every job will get an error event.
            let engine_result = {
                // In Tesseract 5.x, the datapath is the tessdata directory ITSELF
                // (the directory containing .traineddata files), NOT its parent.
                // The old 3.x/4.x convention of passing the parent was changed.
                //
                // Tauri bundles "resources/tessdata/" under {exe_dir}/resources/tessdata/.
                // BaseDirectory::Resource points to {exe_dir}, so we resolve the
                // full relative path to get the actual tessdata directory.
                let tesseract_datapath = app_handle
                    .path()
                    .resolve("resources/tessdata", BaseDirectory::Resource)
                    .map(|p| {
                        // Tauri on Windows may return paths with the \\?\ extended-length
                        // prefix (e.g. \\?\C:\Users\…). Tesseract's C API does NOT
                        // understand this prefix and fails with TessInitError{-1}.
                        // Strip it so Tesseract gets a plain drive-letter path.
                        let mut s = p.to_string_lossy().into_owned();
                        if s.starts_with(r"\\?\") {
                            s = s[4..].to_string();
                        }
                        s
                    })
                    .ok();
                tokio::task::spawn_blocking(move || OcrEngine::init("spa+eng", tesseract_datapath.as_deref()))
                    .await
                    .map_err(|e| format!("Engine init task panicked: {e}"))
                    .and_then(|r| r)
            };

            let engine = match engine_result {
                Ok(e) => e,
                Err(load_err) => {
                    // Cannot proceed — drain queue and report errors
                    while let Some(job) = receiver.recv().await {
                        let _ = app_handle.emit(
                            "ocr:error",
                            OcrErrorPayload {
                                asset_id: job.asset_id,
                                error: format!("OCR engine failed to load: {load_err}"),
                            },
                        );
                    }
                    return;
                }
            };

            // Main work loop — serial, one job at a time
            while let Some(job) = receiver.recv().await {
                let asset_id = job.asset_id.clone();
                let result = process_job(&engine, &job, &app_handle).await;

                match result {
                    Ok((method, text_content)) => {
                        let aid = asset_id.clone();
                        let method_clone = method.clone();
                        let db_path_clone = db_path.clone();
                        let text_for_save = text_content.clone();

                        // Persist extraction to SQLite — open a fresh connection inside
                        // spawn_blocking because rusqlite::Connection is not Send.
                        let save_result = tokio::task::spawn_blocking(move || {
                            let conn = rusqlite::Connection::open(&db_path_clone)
                                .map_err(|e| format!("Failed to open save connection: {e}"))?;
                            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
                                .map_err(|e| format!("Failed to configure pragmas: {e}"))?;
                            save_extraction(&conn, &aid, &text_for_save, &method_clone)?;
                            lookup_item_id_for_asset(&conn, &aid)
                        })
                        .await
                        .map_err(|e| format!("Save task panicked: {e}"))
                        .and_then(|r| r);

                        if let Err(e) = &save_result {
                            eprintln!("[ocr] Failed to save extraction for {asset_id}: {e}");
                            // Still emit complete — text is in memory even if DB save failed
                        } else if let Ok(Some(item_id)) = &save_result {
                            if let Err(e) = enqueue_entity_refresh_for_item(&app_handle.state::<NlpQueue>(), item_id) {
                                eprintln!("[nlp/ner] Failed to auto-enqueue ExtractEntities after OCR save for item {item_id}: {e}");
                            } else {
                                eprintln!(
                                    "[nlp/ner] Auto-enqueued ExtractEntities after OCR save: asset_id={}, item_id={}",
                                    asset_id,
                                    item_id
                                );
                            }
                        }

                        let _ = app_handle.emit(
                            "ocr:complete",
                            OcrCompletePayload {
                                asset_id,
                                method,
                                text_length: text_content.len(),
                                text_content,
                            },
                        );
                    }
                    Err(err) => {
                        let _ = app_handle.emit(
                            "ocr:error",
                            OcrErrorPayload {
                                asset_id,
                                error: err,
                            },
                        );
                    }
                }
            }
        });
    }
}

// ── Persistence ─────────────────────────────────────────────────────────────

/// Upsert an extraction row for the given asset_id.
///
/// Deletes any existing extractions for the asset, then inserts a new row.
/// This matches the frontend `ExtractionRepo.upsert` semantics.
fn save_extraction(
    conn: &rusqlite::Connection,
    asset_id: &str,
    text_content: &str,
    method: &str,
) -> Result<(), String> {
    // Delete existing extractions for this asset
    conn.execute(
        "DELETE FROM extractions WHERE asset_id = ?1",
        [asset_id],
    )
    .map_err(|e| format!("Failed to delete existing extractions: {e}"))?;

    // Insert new extraction
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO extractions(id, asset_id, text_content, method, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, asset_id, text_content, method, None::<f64>, now],
    )
    .map_err(|e| format!("Failed to insert extraction: {e}"))?;

    Ok(())
}

/// Update only the text_content of the latest extraction for an asset.
/// Preserves id, created_at, method, and confidence.
/// Returns `Ok(())` even if no extraction exists (no-op).
fn update_extraction_text(
    conn: &rusqlite::Connection,
    asset_id: &str,
    text_content: &str,
) -> Result<(), String> {
    // Find the latest extraction for this asset
    let mut stmt = conn
        .prepare("SELECT id FROM extractions WHERE asset_id = ?1 ORDER BY created_at DESC LIMIT 1")
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let extraction_id: Result<String, _> = stmt.query_row([asset_id], |row| row.get(0));

    drop(stmt); // release borrow before execute

    match extraction_id {
        Ok(id) => {
            conn.execute(
                "UPDATE extractions SET text_content = ?1 WHERE id = ?2",
                rusqlite::params![text_content, id],
            )
            .map_err(|e| format!("Failed to update extraction text: {e}"))?;
            Ok(())
        }
        Err(_) => Ok(()), // no extraction exists — no-op
    }
}

// ── Job Processing ──────────────────────────────────────────────────────────

/// Process a single OCR job. Returns `(method, text_content)` on success.
async fn process_job(
    engine: &OcrEngine,
    job: &OcrJob,
    app_handle: &AppHandle,
) -> Result<(String, String), String> {
    let asset_id = job.asset_id.clone();

    // Stage 1 — reading file (25 %)
    emit_progress(app_handle, &asset_id, 25, "reading");

    let file_bytes = tokio::fs::read(&job.asset_path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", job.asset_path))?;

    match job.asset_type.as_str() {
        "pdf" => process_pdf(engine, &file_bytes, &asset_id, app_handle).await,
        _ => process_image(engine, &file_bytes, &asset_id, app_handle).await,
    }
}

/// PDF pipeline: try native text first, fall back to image OCR.
async fn process_pdf(
    _engine: &OcrEngine,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
) -> Result<(String, String), String> {
    // Stage 2 — extracting native text (50 %)
    emit_progress(app_handle, asset_id, 50, "extracting_native");

    let bytes_owned = bytes.to_vec();
    let native_text = tokio::task::spawn_blocking(move || extract_pdf_text(&bytes_owned))
        .await
        .map_err(|e| format!("PDF extraction task panicked: {e}"))?;

    match native_text {
        Ok(text) if is_quality_text(&text) => {
            emit_progress(app_handle, asset_id, 100, "done");
            Ok(("native".to_string(), text))
        }
        _ => {
            // Fallback — render first page as image and OCR it.
            // NOTE: Full PDF-to-image rendering requires a crate like `pdfium-render`.
            // For Fase 2, we return an error explaining the limitation.
            // A future phase (Fase 2.5) will add pdfium-render for scanned PDF → image.
            // TODO: Implement PDF page rendering for OCR fallback (Fase 2.5)
            Err("PDF native text extraction failed quality check and PDF-to-image rendering is not yet implemented (Fase 2.5)".to_string())
        }
    }
}

/// Image pipeline: OCR inference via Tesseract.
async fn process_image(
    engine: &OcrEngine,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
) -> Result<(String, String), String> {
    emit_progress(app_handle, asset_id, 50, "ocr_inference");

    let engine_clone = engine.clone();
    let bytes_owned = bytes.to_vec();

    let text = tokio::task::spawn_blocking(move || engine_clone.run_ocr(&bytes_owned))
        .await
        .map_err(|e| format!("OCR task panicked: {e}"))?
        .map_err(|e| format!("OCR inference failed: {e}"))?;

    emit_progress(app_handle, asset_id, 100, "done");
    Ok(("ocr".to_string(), text))
}

/// Emit an `ocr:progress` event to the frontend.
fn emit_progress(app_handle: &AppHandle, asset_id: &str, pct: u8, stage: &str) {
    let _ = app_handle.emit(
        "ocr:progress",
        OcrProgressPayload {
            asset_id: asset_id.to_string(),
            pct,
            stage: stage.to_string(),
        },
    );
}
