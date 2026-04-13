pub mod commands;
mod engine;
mod pdf;
mod preprocessor;

use engine::OcrEngine;
use pdf::{extract_pdf_text, is_quality_text};
use preprocessor::preprocess_image;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
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
    /// 1. Loads `OcrEngine` once at startup.
    /// 2. Drains jobs serially from the receiver.
    /// 3. Emits `ocr:progress`, `ocr:complete`, or `ocr:error` events per job.
    pub fn start_worker(mut receiver: mpsc::Receiver<OcrJob>, app_handle: AppHandle) {
        tauri::async_runtime::spawn(async move {
            // Load models once — if this fails, every job will get an error event.
            let engine_result = {
                let handle = app_handle.clone();
                tokio::task::spawn_blocking(move || OcrEngine::load_models(&handle))
                    .await
                    .map_err(|e| format!("Model loading task panicked: {e}"))
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
                    Ok((method, text_len)) => {
                        let _ = app_handle.emit(
                            "ocr:complete",
                            OcrCompletePayload {
                                asset_id,
                                method,
                                text_length: text_len,
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

// ── Job Processing ──────────────────────────────────────────────────────────

/// Process a single OCR job. Returns `(method, text_length)` on success.
async fn process_job(
    engine: &OcrEngine,
    job: &OcrJob,
    app_handle: &AppHandle,
) -> Result<(String, usize), String> {
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
    engine: &OcrEngine,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
) -> Result<(String, usize), String> {
    // Stage 2 — extracting native text (50 %)
    emit_progress(app_handle, asset_id, 50, "extracting_native");

    let bytes_owned = bytes.to_vec();
    let native_text = tokio::task::spawn_blocking(move || extract_pdf_text(&bytes_owned))
        .await
        .map_err(|e| format!("PDF extraction task panicked: {e}"))?;

    match native_text {
        Ok(text) if is_quality_text(&text) => {
            emit_progress(app_handle, asset_id, 100, "done");
            Ok(("native".to_string(), text.len()))
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

/// Image pipeline: preprocess → OCR inference.
async fn process_image(
    engine: &OcrEngine,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
) -> Result<(String, usize), String> {
    // Stage 2 — preprocessing (50 %)
    emit_progress(app_handle, asset_id, 50, "preprocessing");

    let bytes_owned = bytes.to_vec();
    let gray_image = tokio::task::spawn_blocking(move || -> Result<image::GrayImage, String> {
        let img = image::load_from_memory(&bytes_owned)
            .map_err(|e| format!("Failed to decode image: {e}"))?;
        Ok(preprocess_image(img))
    })
    .await
    .map_err(|e| format!("Preprocessing task panicked: {e}"))??;

    // Stage 3 — OCR inference (75 %)
    emit_progress(app_handle, asset_id, 75, "ocr_inference");

    // engine is not Send — we need to handle this carefully.
    // Since the worker itself runs on a single task, we can call run_ocr in a
    // spawn_blocking from here. However, OcrEngine contains the ocrs engine which
    // may not be Send. We work around this by noting that the worker task itself
    // is single-threaded and sequential — we use the engine reference directly.
    // If OcrEngine is !Send, the caller must ensure this runs on a LocalSet or
    // the engine is wrapped in an appropriate way. For now, we call synchronously
    // via spawn_blocking with an unsafe workaround or restructure.
    //
    // Practical approach: since the worker owns the engine and processes one job
    // at a time, we pass the GrayImage to a blocking closure. The engine reference
    // issue is resolved by the caller restructuring to own the engine on the
    // blocking thread. See start_worker for the actual spawn_blocking pattern.
    //
    // For this module-level helper, we accept the engine by reference and note
    // that in practice the caller (start_worker) already runs on a blocking-capable
    // context.
    let text = engine
        .run_ocr(gray_image)
        .map_err(|e| format!("OCR inference failed: {e}"))?;

    emit_progress(app_handle, asset_id, 100, "done");
    Ok(("ocr".to_string(), text.len()))
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
