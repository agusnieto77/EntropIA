pub mod commands;
pub mod postprocess;
pub mod provider;
pub mod tesseract;

#[cfg(feature = "paddle-ocr")]
pub mod paddle;

mod engine;
mod pdf;

use provider::OcrProvider;
use pdf::{extract_pdf_text, is_quality_text};
use crate::nlp::{enqueue_entity_refresh_for_item, lookup_item_id_for_asset, NlpQueue};
use serde::Serialize;
use std::sync::Arc;
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
    /// 2. Loads the OCR provider once at startup (PaddleOCR → Tesseract fallback).
    /// 3. Drains jobs serially from the receiver.
    /// 4. Saves extracted text to DB, then emits events per job.
    pub fn start_worker(
        db_path: std::path::PathBuf,
        mut receiver: mpsc::Receiver<OcrJob>,
        app_handle: AppHandle,
    ) {
        tauri::async_runtime::spawn(async move {
            // ── Provider initialization with fallback chain ───────────────
            //
            // Try PaddleOCR first (if compiled with `paddle-ocr` feature).
            // If PaddleOCR models are not found (e.g. first run without downloading),
            // fall back to Tesseract. If both fail, drain the queue with errors.
            let provider: Arc<dyn OcrProvider> = {
                let mut chosen: Option<Arc<dyn OcrProvider>> = None;

                // Step 1: Try PaddleOCR (primary engine)
                #[cfg(feature = "paddle-ocr")]
                {
                    let model_dir = resolve_paddle_model_dir(&app_handle);
                    eprintln!("[OCR] Attempting PaddleOCR init from: {}", model_dir.display());
                    match paddle::PaddleOcrProvider::new(model_dir) {
                        Ok(p) => {
                            eprintln!("[OCR] ✅ PaddleOCR initialized successfully — using as primary engine");
                            chosen = Some(Arc::new(p) as Arc<dyn OcrProvider>);
                        }
                        Err(e) => {
                            eprintln!("[OCR] ❌ PaddleOCR unavailable ({e}), trying Tesseract fallback");
                        }
                    }
                }

                // Step 2: Try Tesseract (fallback engine)
                if chosen.is_none() {
                    let tessdata_path = resolve_tessdata_dir(&app_handle);
                    eprintln!("[OCR] Attempting Tesseract init with tessdata: {}", tessdata_path.as_deref().unwrap_or("(default)"));
                    match tesseract::TesseractProvider::init("spa+eng", tessdata_path.as_deref()) {
                        Ok(t) => {
                            eprintln!("[OCR] ✅ Tesseract initialized — using as fallback engine");
                            chosen = Some(Arc::new(t) as Arc<dyn OcrProvider>);
                        }
                        Err(e) => {
                            eprintln!("[OCR] ❌ Tesseract also unavailable ({e})");
                        }
                    }
                }

                match chosen {
                    Some(p) => p,
                    None => {
                        eprintln!("[OCR] 🚨 No OCR provider available — draining queue with errors");
                        while let Some(job) = receiver.recv().await {
                            let _ = app_handle.emit(
                                "ocr:error",
                                OcrErrorPayload {
                                    asset_id: job.asset_id,
                                    error: "No OCR engine available (PaddleOCR and Tesseract both failed to load)".to_string(),
                                },
                            );
                        }
                        return;
                    }
                }
            };

            eprintln!("[OCR] Using provider: {}", provider.name());

            // Main work loop — serial, one job at a time
            while let Some(job) = receiver.recv().await {
                let asset_id = job.asset_id.clone();
                let result = process_job(&provider, &job, &app_handle).await;

                match result {
                    Ok(output) => {
                        let method = output.method.clone();
                        let text_content = output.text.clone();
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

// ── Model directory resolution ──────────────────────────────────────────────

/// Resolve the PaddleOCR model directory.
///
/// In production (bundled Tauri app), uses `BaseDirectory::Resource`.
/// In dev mode, falls back to `CARGO_MANIFEST_DIR` so models can be loaded
/// from the project's `resources/models/ocr/` directory.
#[cfg(feature = "paddle-ocr")]
fn resolve_paddle_model_dir(app_handle: &AppHandle) -> std::path::PathBuf {
    // Try Tauri resource path first (production)
    if let Ok(path) = app_handle
        .path()
        .resolve("resources/models/ocr", BaseDirectory::Resource)
    {
        // Strip Windows \\?\ prefix if present (Tesseract compatibility pattern)
        let mut s = path.to_string_lossy().into_owned();
        if s.starts_with(r"\\?\") {
            s = s[4..].to_string();
        }
        let clean_path = std::path::PathBuf::from(s);
        if clean_path.exists() {
            return clean_path;
        }
    }

    // Dev fallback: CARGO_MANIFEST_DIR/resources/models/ocr
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev_path = std::path::PathBuf::from(manifest_dir)
            .join("resources")
            .join("models")
            .join("ocr");
        if dev_path.exists() {
            return dev_path;
        }
    }

    // Last resort: relative path
    std::path::PathBuf::from("resources/models/ocr")
}

/// Resolve the Tesseract tessdata directory.
///
/// Same pattern as PaddleOCR: Tauri resource path → CARGO_MANIFEST_DIR fallback.
fn resolve_tessdata_dir(app_handle: &AppHandle) -> Option<String> {
    // Try Tauri resource path first (production)
    if let Ok(path) = app_handle
        .path()
        .resolve("resources/tessdata", BaseDirectory::Resource)
    {
        // Strip Windows \\?\ prefix — Tesseract's C API does NOT understand it
        let mut s = path.to_string_lossy().into_owned();
        if s.starts_with(r"\\?\") {
            s = s[4..].to_string();
        }
        let clean_path = std::path::PathBuf::from(&s);
        if clean_path.exists() {
            return Some(s);
        }
    }

    // Dev fallback: CARGO_MANIFEST_DIR/resources/tessdata
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev_path = std::path::PathBuf::from(manifest_dir)
            .join("resources")
            .join("tessdata");
        if dev_path.exists() {
            return Some(dev_path.to_string_lossy().into_owned());
        }
    }

    // Fallback to vcpkg default (works on the dev machine)
    Some(r"C:\vcpkg\installed\x64-windows-static-md\share\tessdata".to_string())
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

/// Process a single OCR job using any OcrProvider.
///
/// Returns `OcrOutput` on success, which includes the recognized text,
/// structured regions (with bounding boxes for PaddleOCR), and the method name.
async fn process_job(
    provider: &Arc<dyn OcrProvider>,
    job: &OcrJob,
    app_handle: &AppHandle,
) -> Result<provider::OcrOutput, String> {
    let asset_id = job.asset_id.clone();

    // Stage 1 — reading file (25 %)
    emit_progress(app_handle, &asset_id, 25, "reading");

    let file_bytes = tokio::fs::read(&job.asset_path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", job.asset_path))?;

    match job.asset_type.as_str() {
        "pdf" => process_pdf(provider, &file_bytes, &asset_id, app_handle).await,
        _ => process_image(provider, &file_bytes, &asset_id, app_handle).await,
    }
}

/// PDF pipeline: try native text first, fall back to OCR via the provider.
async fn process_pdf(
    provider: &Arc<dyn OcrProvider>,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
) -> Result<provider::OcrOutput, String> {
    // Stage 2 — extracting native text (50 %)
    emit_progress(app_handle, asset_id, 50, "extracting_native");

    let bytes_owned = bytes.to_vec();
    let native_text = tokio::task::spawn_blocking(move || extract_pdf_text(&bytes_owned))
        .await
        .map_err(|e| format!("PDF extraction task panicked: {e}"))?;

    match native_text {
        Ok(text) if is_quality_text(&text) => {
            emit_progress(app_handle, asset_id, 100, "done");
            Ok(provider::OcrOutput {
                text: text.clone(),
                regions: vec![provider::OcrRegion {
                    text,
                    confidence: 0.0,
                    bbox: None,
                    column: None,
                }],
                method: "native".to_string(),
            })
        }
        _ => {
            // Native text failed quality check — render first page as image and OCR it.
            eprintln!("[pdf] Native text failed quality check, falling back to PDF→image→OCR");

            // Stage 3 — rendering PDF page as image (75 %)
            emit_progress(app_handle, asset_id, 75, "rendering_pdf_page");

            let pdf_bytes = bytes.to_vec();
            let page_image = tokio::task::spawn_blocking(move || {
                pdf::render_pdf_page_to_image(&pdf_bytes, 0) // First page only
            })
            .await
            .map_err(|e| format!("PDF render task panicked: {e}"))?
            .map_err(|e| format!("PDF page rendering failed: {e}"))?;

            // Stage 4 — OCR the rendered page image (85 %)
            emit_progress(app_handle, asset_id, 85, "ocr_fallback");

            let provider_clone = Arc::clone(provider);
            let page_image_owned = page_image; // Move into the closure
            let output = tokio::task::spawn_blocking(move || {
                provider_clone.recognize(&page_image_owned)
            })
            .await
            .map_err(|e| format!("OCR fallback task panicked: {e}"))?
            .map_err(|e| format!("OCR fallback failed: {e}"))?;

            // Override method to indicate this came from PDF→image→OCR
            let result = provider::OcrOutput {
                method: format!("pdf_{}", output.method),
                ..output
            };

            emit_progress(app_handle, asset_id, 100, "done");
            Ok(result)
        }
    }
}

/// Image pipeline: OCR inference via the active provider.
async fn process_image(
    provider: &Arc<dyn OcrProvider>,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
) -> Result<provider::OcrOutput, String> {
    emit_progress(app_handle, asset_id, 50, "ocr_inference");

    // Clone the Arc so the closure owns its own reference.
    // This avoids raw pointers and satisfies Send bounds.
    let provider_clone = Arc::clone(provider);
    let bytes_owned = bytes.to_vec();

    let output = tokio::task::spawn_blocking(move || {
        provider_clone.recognize(&bytes_owned)
    })
    .await
    .map_err(|e| format!("OCR task panicked: {e}"))?
    .map_err(|e| format!("OCR inference failed: {e}"))?;

    emit_progress(app_handle, asset_id, 100, "done");
    Ok(output)
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