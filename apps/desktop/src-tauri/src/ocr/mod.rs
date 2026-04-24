pub mod commands;
pub mod postprocess;
pub mod provider;
pub mod tesseract;

#[cfg(feature = "paddle-ocr")]
pub mod paddle;

mod engine;
mod pdf;
pub mod paddle_vl;
pub mod layout_onnx;
pub mod reading_order;

// Dev-only visualization helpers for debugging layout detection.
// Compiled in debug builds only; the call site is also gated by cfg!(debug_assertions).
#[cfg(debug_assertions)]
mod debug_viz;

use provider::{LayoutCategory, OcrProvider};
use pdf::{extract_pdf_text, is_quality_text, pdf_page_count};
use paddle_vl::PaddleVlEngine;
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
    pub mode: OcrMode,
}

/// OCR processing mode.
///
/// - `Light`: Plain PaddleOCR/Tesseract OCR only — no layout detection, no Python subprocess.
/// - `High`: PaddleOCR-VL Python subprocess only. Slower but layout-aware extraction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum OcrMode {
    #[default]
    Light, // Plain OCR (PaddleOCR/Tesseract)
    High,  // PaddleOCR-VL only
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
        paddle_vl_engine: Option<PaddleVlEngine>,
        layout_engine: Option<Arc<layout_onnx::OnnxLayoutEngine>>,
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
            if paddle_vl_engine.is_some() {
                eprintln!("[OCR] ✅ PaddleOCR-VL available — will use for OCRH (High mode)");
            } else {
                eprintln!("[OCR] PaddleOCR-VL not available — OCRH will fall back to plain OCR");
            }
            if layout_engine.is_some() {
                eprintln!("[OCR] Native layout engine (ONNX) available — currently unused (OCRL is plain OCR)");
            }

            // Main work loop — serial, one job at a time
            while let Some(job) = receiver.recv().await {
                let asset_id = job.asset_id.clone();
                let result = process_job(&provider, &job, &app_handle, paddle_vl_engine.as_ref(), layout_engine.as_ref()).await;

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

// ── Layout-Aware Text Formatting ──────────────────────────────────────────

/// Format text based on the layout category of the region.
///
/// Returns `None` for categories that should be skipped (Figure, Header, Footer, Abandoned).
/// Returns `Some(formatted_text)` for categories that contribute to the output.
///
/// NOTE: Currently unused in production code (layout-aware pipeline removed from Light mode).
/// Kept for potential future use and tested in unit tests.
#[allow(dead_code)]
fn format_region_text(category: &LayoutCategory, text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    match category {
        LayoutCategory::Title => Some(format!("## {trimmed}")),
        LayoutCategory::PlainText => Some(trimmed.to_string()),
        LayoutCategory::Table => Some(format!("---\n{trimmed}\n---")),
        LayoutCategory::Figure => None, // Skip figures in text output
        LayoutCategory::Caption => Some(trimmed.to_string()),
        LayoutCategory::Footnote => Some(format!("Note: {trimmed}")),
        LayoutCategory::Header => None, // Skip headers (typically noise)
        LayoutCategory::Footer => None, // Skip footers (page numbers, etc.)
        LayoutCategory::Code => Some(format!("```\n{trimmed}\n```")),
        LayoutCategory::Reference => Some(trimmed.to_string()),
        LayoutCategory::Abandoned => None, // Skip abandoned content
    }
}

/// Maximum image dimension (longest side, in pixels) to feed into PaddleVL.
///
/// PaddleOCR-VL is a vision-language model that runs a VLM on the full image.
/// Inference time scales roughly with pixel count — on CPU, a 2200×2575 image
/// (5.67 MP) takes 15+ minutes, while a 1500×1756 image (2.63 MP) takes ~4 min.
///
/// We downscale images larger than this threshold (in either dimension) before
/// passing to PaddleVL. The aspect ratio is preserved. At 1000px longest side,
/// OCR accuracy on typical document images (scanned newspapers, book pages,
/// forms) may start to degrade for very small fonts — monitor results and
/// bump this up to 1500-2000 if needed.
const PADDLE_VL_MAX_DIMENSION: u32 = 1000;

/// Maximum total pixel count before triggering downscale.
///
/// Belt-and-suspenders check alongside PADDLE_VL_MAX_DIMENSION. Consistent with
/// MAX_DIMENSION=1000: a 1000×1000 square image is exactly at the limit.
/// Anything larger by area (e.g. 1100×1100 = 1.21 MP) triggers downscale.
/// We trigger downscale if EITHER condition is met.
const PADDLE_VL_MAX_PIXELS: u32 = 1_000_000; // 1 megapixel

/// Downscale an image if it exceeds PaddleVL's comfort zone.
///
/// Returns the (possibly reduced) image bytes as PNG. If no downscale is
/// needed (image fits comfortably within PADDLE_VL_MAX_DIMENSION and
/// PADDLE_VL_MAX_PIXELS), returns the original bytes verbatim.
///
/// On decode or re-encode failure, returns the original bytes — we never
/// want to block OCR because of a resize issue.
fn maybe_downscale_for_paddlevl(bytes: &[u8]) -> Vec<u8> {
    let img = match image::load_from_memory(bytes) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("[OCRH] Could not decode image for downscale check: {e}. Using original bytes.");
            return bytes.to_vec();
        }
    };

    let (w, h) = (img.width(), img.height());
    let total_pixels = (w as u64) * (h as u64);
    let max_dim = w.max(h);

    let exceeds_dim = max_dim > PADDLE_VL_MAX_DIMENSION;
    let exceeds_pixels = total_pixels > PADDLE_VL_MAX_PIXELS as u64;

    if !exceeds_dim && !exceeds_pixels {
        eprintln!(
            "[OCRH] Image size {}x{} ({:.2} MP) OK, no downscale needed",
            w, h, total_pixels as f64 / 1_000_000.0
        );
        return bytes.to_vec();
    }

    // Compute target size: scale the longest side down to PADDLE_VL_MAX_DIMENSION
    // while preserving aspect ratio. This also addresses the PADDLE_VL_MAX_PIXELS
    // case because reducing the longest side also reduces total pixels.
    let scale = PADDLE_VL_MAX_DIMENSION as f32 / max_dim as f32;
    let new_w = ((w as f32) * scale).round().max(1.0) as u32;
    let new_h = ((h as f32) * scale).round().max(1.0) as u32;

    eprintln!(
        "[OCRH] Downscaling {}x{} ({:.2} MP) → {}x{} ({:.2} MP) for PaddleVL",
        w, h, total_pixels as f64 / 1_000_000.0,
        new_w, new_h, (new_w as u64 * new_h as u64) as f64 / 1_000_000.0
    );

    // Triangle filter = good balance of quality vs speed for document images
    let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Triangle);

    // Re-encode as PNG (lossless, preserves text sharpness)
    let mut out = Vec::with_capacity(bytes.len() / 2);
    if let Err(e) = resized.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png) {
        eprintln!("[OCRH] Re-encode failed: {e}. Using original bytes.");
        return bytes.to_vec();
    }

    out
}

/// Decide whether the layout detection result is trustworthy, or whether we
/// should bypass per-region OCR and use the provider directly on the full image.
///
/// Heuristics (any triggers bypass):
///   1. Zero regions detected — nothing to work with
///   2. Only 1 region detected — layout model likely failed to segment the page;
///      a single "region" spanning most of the image is basically "OCR the whole
///      thing" anyway, but per-region cropping often loses text at the edges.
///   3. A single region covers > 60% of the image area — the layout model treated
///      the whole page as one block, which means we lose nothing by bypassing it
///      and OCR'ing the full image directly (and we gain the provider's internal
///      post-processing that we'd otherwise miss).
///
/// When bypassed, the caller falls through to `provider.recognize(full_image)`
/// which typically produces much better results on newspaper clippings, forms,
/// and other documents where PP-DocLayout-L gives sparse/wrong regions.
///
/// NOTE: Currently unused in production code (layout-aware pipeline removed from Light mode).
/// Kept for potential future use and tested in unit tests.
#[allow(dead_code)]
fn should_bypass_layout(
    regions: &[provider::LayoutRegion],
    image_width: u32,
    image_height: u32,
) -> bool {
    if regions.len() < 2 {
        if regions.is_empty() {
            eprintln!("[OCRL] Bypassing layout: 0 regions detected");
        } else {
            eprintln!("[OCRL] Bypassing layout: only 1 region detected (insufficient segmentation)");
        }
        return true;
    }

    // Check if any single region dominates the image (> 60% of total area)
    let total_area = (image_width as u64) * (image_height as u64);
    if total_area == 0 {
        return false;
    }
    for region in regions {
        let region_area = (region.bbox.width as u64) * (region.bbox.height as u64);
        let ratio = (region_area as f64) / (total_area as f64);
        if ratio > 0.60 {
            eprintln!(
                "[OCRL] Bypassing layout: region {:?} covers {:.1}% of the image (>60% threshold)",
                region.label, ratio * 100.0
            );
            return true;
        }
    }

    false
}

/// Crops a layout-detected region from the image with a fixed 5px margin
/// on all sides (top, bottom, left, right).
///
/// The layout model (PP-DocLayout-L) produces tight bounding boxes that
/// closely match text boundaries, so minimal margin is needed.
///
/// Clamps to image bounds. Returns `None` if the cropped region is too small
/// (< 10px on either axis) — anything smaller cannot produce useful OCR.
///
/// NOTE: Currently unused in production code (layout-aware pipeline removed from Light mode).
/// Kept for potential future use and tested in unit tests.
#[allow(dead_code)]
fn crop_region(
    image: &image::DynamicImage,
    bbox: &provider::BoundingBox,
) -> Option<image::DynamicImage> {
    let (img_w, img_h) = (image.width() as i32, image.height() as i32);
    const MARGIN: i32 = 5;

    let x1 = (bbox.x - MARGIN).max(0);
    let y1 = (bbox.y - MARGIN).max(0);
    let x2 = (bbox.x + bbox.width as i32 + MARGIN).min(img_w);
    let y2 = (bbox.y + bbox.height as i32 + MARGIN).min(img_h);

    let crop_w = (x2 - x1) as u32;
    let crop_h = (y2 - y1) as u32;

    // Skip regions that are too small — cannot produce useful OCR.
    if crop_w < 10 || crop_h < 10 {
        eprintln!(
            "[ocr] Skipping region too small: {}x{} at ({},{})",
            crop_w, crop_h, x1, y1
        );
        return None;
    }

    Some(image.crop_imm(x1 as u32, y1 as u32, crop_w, crop_h))
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
    paddle_vl_engine: Option<&PaddleVlEngine>,
    _layout_engine: Option<&Arc<layout_onnx::OnnxLayoutEngine>>,
) -> Result<provider::OcrOutput, String> {
    let asset_id = job.asset_id.clone();

    // Stage 1 — reading file (25 %)
    emit_progress(app_handle, &asset_id, 25, "reading");

    let file_bytes = tokio::fs::read(&job.asset_path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", job.asset_path))?;

    match job.asset_type.as_str() {
        "pdf" => process_pdf(provider, &file_bytes, &asset_id, app_handle, paddle_vl_engine, &job.mode).await,
        _ => process_image(provider, &file_bytes, &asset_id, app_handle, paddle_vl_engine, &job.mode).await,
    }
}

/// PDF pipeline: try native text first, fall back to page-by-page OCR.
///
/// For text-based PDFs, the native text layer is extracted and quality-checked.
/// If it's insufficient (scanned PDFs, images), every page is rendered and OCR'd,
/// then the results are concatenated with page separators.
async fn process_pdf(
    provider: &Arc<dyn OcrProvider>,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
    paddle_vl_engine: Option<&PaddleVlEngine>,
    mode: &OcrMode,
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
            // Native text failed quality check — render ALL pages and OCR them.
            eprintln!("[pdf] Native text failed quality check, falling back to multi-page PDF→image→OCR");

            // Get page count in a blocking task (pdfium interaction)
            let pdf_bytes_for_count = bytes.to_vec();
            let page_count = tokio::task::spawn_blocking(move || {
                pdf_page_count(&pdf_bytes_for_count)
            })
            .await
            .map_err(|e| format!("PDF page count task panicked: {e}"))?
            .map_err(|e| format!("Failed to get PDF page count: {e}"))?;

            eprintln!("[pdf] Processing {page_count} page(s) via OCR fallback");

            let mut all_text = String::new();
            let mut all_regions: Vec<provider::OcrRegion> = Vec::new();
            let mut method_suffix = String::new();

            for page_idx in 0..page_count {
                // Progress: 60% base + (page_idx / page_count) * 35% range
                let pct = 60 + ((page_idx as u8 * 35) / page_count.max(1) as u8);
                emit_progress(app_handle, asset_id, pct.min(95), &format!("ocr_page_{}", page_idx + 1));

                // Render this page
                let pdf_bytes_for_render = bytes.to_vec();
                let page_image = tokio::task::spawn_blocking(move || {
                    pdf::render_pdf_page_to_image(&pdf_bytes_for_render, page_idx)
                })
                .await
                .map_err(|e| format!("PDF render task panicked: {e}"))?
                .map_err(|e| format!("PDF page {} rendering failed: {e}", page_idx + 1))?;

                // OCR this page — mode-aware pipeline
                let provider_clone = Arc::clone(provider);
                let engine_clone = paddle_vl_engine.cloned();
                let mode_clone = mode.clone();

                let output = tokio::task::spawn_blocking(move || {
                    match mode_clone {
                        OcrMode::Light => {
                            // Light mode: plain OCR, no layout detection
                            provider_clone
                                .recognize(&page_image)
                                .map_err(|e| format!("OCR page {} failed: {e}", page_idx + 1))
                        }
                        OcrMode::High => {
                            // High mode: try PaddleVL, fall back to plain OCR
                            if let Some(engine) = engine_clone {
                                // Downscale large PDF page renders before PaddleVL (same reason as images)
                                let vl_bytes = maybe_downscale_for_paddlevl(&page_image);

                                let temp_path = std::env::temp_dir().join(format!(
                                    "entropia_paddlevl_pdf_{}_{}.png",
                                    page_idx,
                                    uuid::Uuid::new_v4()
                                ));

                                if let Err(e) = std::fs::write(&temp_path, &vl_bytes) {
                                    eprintln!("[OCRH] Failed to write temp file for PaddleVL on PDF page {}: {e}. Falling back to plain OCR.", page_idx + 1);
                                    return provider_clone
                                        .recognize(&page_image)
                                        .map_err(|e| format!("OCR page {} failed: {e}", page_idx + 1));
                                }

                                let temp_path_str = match temp_path.to_str() {
                                    Some(s) => s.to_string(),
                                    None => {
                                        eprintln!("[OCRH] Invalid temp path for PaddleVL on PDF page {}. Falling back.", page_idx + 1);
                                        return provider_clone
                                            .recognize(&page_image)
                                            .map_err(|e| format!("OCR page {} failed: {e}", page_idx + 1));
                                    }
                                };

                                let vl_result = engine.detect(&temp_path_str);
                                let _ = std::fs::remove_file(&temp_path);

                                match vl_result {
                                    Ok(vl_result) => {
                                        Ok(provider::OcrOutput {
                                            text: vl_result.text,
                                            regions: vl_result.regions.into_iter().map(|r| provider::OcrRegion {
                                                text: String::new(),
                                                confidence: r.confidence,
                                                bbox: Some(provider::BoundingBox {
                                                    x: r.bbox.x,
                                                    y: r.bbox.y,
                                                    width: r.bbox.width as u32,
                                                    height: r.bbox.height as u32,
                                                }),
                                                column: None,
                                            }).collect(),
                                            method: vl_result.method,
                                        })
                                    }
                                    Err(e) => {
                                        eprintln!("[OCRH] PaddleVL failed for PDF page {}: {e}. Falling back to plain OCR.", page_idx + 1);
                                        provider_clone
                                            .recognize(&page_image)
                                            .map_err(|e| format!("OCR page {} failed: {e}", page_idx + 1))
                                    }
                                }
                            } else {
                                // No PaddleVL — plain OCR
                                provider_clone
                                    .recognize(&page_image)
                                    .map_err(|e| format!("OCR page {} failed: {e}", page_idx + 1))
                            }
                        }
                    }
                })
                .await
                .map_err(|e| format!("OCR page {} task panicked: {e}", page_idx + 1))??;

                // Track method for reporting
                if method_suffix.is_empty() {
                    method_suffix = format!("pdf_{}", output.method);
                }

                // Accumulate results with page separators
                if !all_text.is_empty() {
                    all_text.push_str("\n\n---\n\n"); // Page separator
                }
                all_text.push_str(&output.text);
                all_regions.extend(output.regions);
            }

            let method = if !all_text.is_empty() {
                method_suffix
            } else {
                "pdf_unknown".to_string()
            };

            emit_progress(app_handle, asset_id, 100, "done");
            Ok(provider::OcrOutput {
                text: all_text,
                regions: all_regions,
                method,
            })
        }
    }
}

/// Image pipeline: mode-aware OCR with progressive fallback.
///
/// **Light mode** (OCRL): Plain PaddleOCR/Tesseract OCR on the full image. No layout detection.
/// Fast and simple.
///
/// **High mode** (OCRH): PaddleVL Python subprocess only. Slower but more accurate layout awareness.
async fn process_image(
    provider: &Arc<dyn OcrProvider>,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
    paddle_vl_engine: Option<&PaddleVlEngine>,
    mode: &OcrMode,
) -> Result<provider::OcrOutput, String> {
    match mode {
        OcrMode::Light => process_image_light(provider, bytes, asset_id, app_handle).await,
        OcrMode::High => process_image_high(provider, bytes, asset_id, app_handle, paddle_vl_engine).await,
    }
}

/// Light mode: plain PaddleOCR/Tesseract OCR — no layout detection.
///
/// Runs the provider's `recognize()` directly on the full image. Fast and simple.
/// Layout-aware processing is available via High mode (PaddleVL).
async fn process_image_light(
    provider: &Arc<dyn OcrProvider>,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
) -> Result<provider::OcrOutput, String> {
    emit_progress(app_handle, asset_id, 50, "ocr_inference");

    // Plain OCR — no layout detection, just run the provider on the full image
    let provider_clone = Arc::clone(provider);
    let bytes_owned = bytes.to_vec();

    let mut output = tokio::task::spawn_blocking(move || {
        provider_clone.recognize(&bytes_owned)
    })
    .await
    .map_err(|e| format!("OCR task panicked: {e}"))?
    .map_err(|e| format!("OCR inference failed: {e}"))?;

    // Reorder regions by reading order (columns left-to-right, top-to-bottom)
    // This matches the algorithm in orden_lectura.py
    if output.regions.len() >= 2 && output.regions.iter().any(|r| r.bbox.is_some()) {
        // Decode image to get dimensions for reading order computation
        if let Ok(img) = image::load_from_memory(bytes) {
            let (img_w, img_h) = (img.width(), img.height());
            output.regions = reading_order::reorder_ocr_regions(&output.regions, img_w, img_h);
            // Rebuild text from reordered regions
            output.text = output.regions.iter()
                .map(|r| r.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
        }
        // If image decode fails, keep original order — don't fail OCR for this
    }

    // Dev-only: save debug visualization of detected OCR lines
    #[cfg(debug_assertions)]
    {
        if !output.regions.is_empty() {
            let method = output.method.clone();
            let regions_debug = output.regions.clone();
            let bytes_debug = bytes.to_vec();
            let aid = asset_id.to_string();
            // Best-effort — don't fail OCR if debug viz fails
            let _ = debug_viz::save_ocr_lines_debug(&bytes_debug, &regions_debug, &method, &aid);
        }
    }

    emit_progress(app_handle, asset_id, 100, "done");
    Ok(output)
}

/// High mode: PaddleVL Python subprocess only.
///
/// Runs PaddleOCR-VL (layout + OCR in one pass) via Python subprocess.
/// Falls back to plain OCR if PaddleVL is unavailable or fails.
async fn process_image_high(
    provider: &Arc<dyn OcrProvider>,
    bytes: &[u8],
    asset_id: &str,
    app_handle: &AppHandle,
    paddle_vl_engine: Option<&PaddleVlEngine>,
) -> Result<provider::OcrOutput, String> {
    emit_progress(app_handle, asset_id, 50, "ocr_inference");

    // Try PaddleVL (Python subprocess) if available
    if let Some(engine) = paddle_vl_engine {
        emit_progress(app_handle, asset_id, 55, "paddlevl_detection");

        let engine_clone = engine.clone();
        let provider_clone = Arc::clone(provider);
        let bytes_owned = bytes.to_vec();
        let asset_id_owned = asset_id.to_string();

        let output = tokio::task::spawn_blocking(move || {
            // Downscale large images before feeding to PaddleVL — inference time
            // scales with pixel count, and CPUs can take 10x longer on 2200x2575
            // images vs 1500x1756. The resized image is still sharp enough for
            // accurate OCR on typical document scans.
            let vl_bytes = maybe_downscale_for_paddlevl(&bytes_owned);

            // Write bytes to a temp file for PaddleVL subprocess
            let temp_path = std::env::temp_dir().join(format!(
                "entropia_paddlevl_{}.png",
                uuid::Uuid::new_v4()
            ));

            if let Err(e) = std::fs::write(&temp_path, &vl_bytes) {
                eprintln!(
                    "[OCRH] Failed to write temp file for PaddleVL for {asset_id_owned}: {e}. \
                     Falling back to plain OCR."
                );
                return provider_clone
                    .recognize(&bytes_owned)
                    .map_err(|e| format!("OCR inference failed: {e}"));
            }

            let temp_path_str = match temp_path.to_str() {
                Some(s) => s.to_string(),
                None => {
                    eprintln!(
                        "[OCRH] Invalid temp path for PaddleVL for {asset_id_owned}. \
                         Falling back to plain OCR."
                    );
                    return provider_clone
                        .recognize(&bytes_owned)
                        .map_err(|e| format!("OCR inference failed: {e}"));
                }
            };

            // Run PaddleVL detection via Python subprocess
            let vl_result = engine_clone.detect(&temp_path_str);
            let _ = std::fs::remove_file(&temp_path); // best-effort cleanup

            match vl_result {
                Ok(vl_output) => {
                    eprintln!("[OCRH] PaddleVL detected {} blocks for {asset_id_owned}", vl_output.blocks.len());
                    Ok(provider::OcrOutput {
                        text: vl_output.text,
                        regions: vl_output.regions.into_iter().map(|r| provider::OcrRegion {
                            text: String::new(),
                            confidence: r.confidence,
                            bbox: Some(provider::BoundingBox {
                                x: r.bbox.x,
                                y: r.bbox.y,
                                width: r.bbox.width as u32,
                                height: r.bbox.height as u32,
                            }),
                            column: None,
                        }).collect(),
                        method: vl_output.method,
                    })
                }
                Err(e) => {
                    eprintln!(
                        "[OCRH] PaddleVL failed for {asset_id_owned}: {e}. \
                         Falling back to plain OCR."
                    );
                    provider_clone
                        .recognize(&bytes_owned)
                        .map_err(|e| format!("OCR inference failed: {e}"))
                }
            }
        })
        .await
        .map_err(|e| format!("OCR task panicked: {e}"))??;

        emit_progress(app_handle, asset_id, 100, "done");
        return Ok(output);
    }

    // No PaddleVL — plain OCR
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::provider::{BoundingBox, LayoutCategory};

    #[test]
    fn test_format_region_text_title() {
        let result = format_region_text(&LayoutCategory::Title, "Introduction");
        assert_eq!(result, Some("## Introduction".to_string()));
    }

    #[test]
    fn test_format_region_text_plain_text() {
        let result = format_region_text(&LayoutCategory::PlainText, "Some body text");
        assert_eq!(result, Some("Some body text".to_string()));
    }

    #[test]
    fn test_format_region_text_table() {
        let result = format_region_text(&LayoutCategory::Table, "col1 | col2\na | b");
        assert_eq!(result, Some("---\ncol1 | col2\na | b\n---".to_string()));
    }

    #[test]
    fn test_format_region_text_figure_skipped() {
        let result = format_region_text(&LayoutCategory::Figure, "image description");
        assert_eq!(result, None);
    }

    #[test]
    fn test_format_region_text_caption() {
        let result = format_region_text(&LayoutCategory::Caption, "Figure 1: Diagram");
        assert_eq!(result, Some("Figure 1: Diagram".to_string()));
    }

    #[test]
    fn test_format_region_text_footnote() {
        let result = format_region_text(&LayoutCategory::Footnote, "See reference 1");
        assert_eq!(result, Some("Note: See reference 1".to_string()));
    }

    #[test]
    fn test_format_region_text_header_skipped() {
        let result = format_region_text(&LayoutCategory::Header, "Page 1");
        assert_eq!(result, None);
    }

    #[test]
    fn test_format_region_text_footer_skipped() {
        let result = format_region_text(&LayoutCategory::Footer, "Page 1");
        assert_eq!(result, None);
    }

    #[test]
    fn test_format_region_text_code() {
        let result = format_region_text(&LayoutCategory::Code, "fn main() {}");
        assert_eq!(result, Some("```\nfn main() {}\n```".to_string()));
    }

    #[test]
    fn test_format_region_text_reference() {
        let result = format_region_text(&LayoutCategory::Reference, "[1] Smith 2024");
        assert_eq!(result, Some("[1] Smith 2024".to_string()));
    }

    #[test]
    fn test_format_region_text_abandoned_skipped() {
        let result = format_region_text(&LayoutCategory::Abandoned, "seal content");
        assert_eq!(result, None);
    }

    #[test]
    fn test_format_region_text_empty_skipped() {
        let result = format_region_text(&LayoutCategory::PlainText, "   ");
        assert_eq!(result, None);
    }

    #[test]
    fn test_crop_region_basic() {
        // Create a 200x200 white image
        let img = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(200, 200, image::Rgba([255, 255, 255, 255]))
        );

        let bbox = BoundingBox { x: 50, y: 50, width: 100, height: 100 };
        let cropped = crop_region(&img, &bbox);

        assert!(cropped.is_some(), "Crop should succeed for valid bbox");
        let cropped = cropped.unwrap();
        // Should be larger than 100x100 due to 15% padding (now 30px on each side)
        // Total expected: 100 + 30 + 30 = 160 (or clamped to image bounds)
        assert!(cropped.width() >= 100, "Cropped width should be at least 100, got {}", cropped.width());
        assert!(cropped.height() >= 100, "Cropped height should be at least 100, got {}", cropped.height());
    }

    #[test]
    fn test_crop_region_at_edge() {
        let img = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(200, 200, image::Rgba([255, 255, 255, 255]))
        );

        // Region near the top-left corner — padding should be clamped
        let bbox = BoundingBox { x: 0, y: 0, width: 50, height: 50 };
        let cropped = crop_region(&img, &bbox);

        assert!(cropped.is_some(), "Crop at edge should succeed");
    }

    #[test]
    fn test_crop_region_too_small() {
        // A region that after 5px margin on each side is still < 10px
        // should be skipped — too small for useful OCR.
        let _tiny_img = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(15, 15, image::Rgba([255, 255, 255, 255]))
        );
        // 3x3 region + 5px margin each side in a 15x15 image:
        //   x1 = max(5-5, 0) = 0, x2 = min(5+3+5, 15) = 13 → width = 13
        // That's >= 10, so we need an even smaller image:
        let micro_img = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]))
        );
        // 2x2 region + 5px margin clamped to 8x8 image:
        //   x1 = max(3-5, 0) = 0, x2 = min(3+2+5, 8) = 8 → width = 8
        //   8 < 10 → skipped
        let bbox = BoundingBox { x: 3, y: 3, width: 2, height: 2 };
        let cropped = crop_region(&micro_img, &bbox);

        assert!(cropped.is_none(), "Region that crops to <10px after margin should be skipped");
    }

    #[test]
    fn test_crop_region_margin_is_5px() {
        // Verify the margin is exactly 5px on each side.
        let img = image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(500, 500, image::Rgba([255, 255, 255, 255]))
        );

        // 50x50 region at center — 5px margin each side → 60x60 crop
        let bbox = BoundingBox { x: 200, y: 200, width: 50, height: 50 };
        let cropped = crop_region(&img, &bbox).expect("crop should succeed");

        assert_eq!(cropped.width(), 60, "50px + 5px + 5px = 60px width");
        assert_eq!(cropped.height(), 60, "50px + 5px + 5px = 60px height");
    }
}