pub mod commands;
pub mod engine;
pub mod prompt;
pub mod sidecar;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::params;
use rusqlite::OptionalExtension;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::nlp::text_provider;

use self::engine::{LlmConfig, LlmEngine};
use self::sidecar::{SidecarManager, SidecarHandle};

// ---------------------------------------------------------------------------
// Job definition
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LlmJob {
    CorrectOcr { item_id: String },
    ExtractEntities { item_id: String },
    ExtractTriples { item_id: String },
    Summarize { item_id: String },
    Classify { item_id: String, categories: Vec<String> },
    Ask { collection_id: String, question: String },
    // Asset-level variants — operate on a single asset/page instead of the whole item.
    // These use get_asset_text() which only fetches text for the specified asset,
    // avoiding context-window overflow on multi-page documents.
    CorrectOcrAsset { asset_id: String },
    ExtractEntitiesAsset { asset_id: String },
    ExtractTriplesAsset { asset_id: String },
    SummarizeAsset { asset_id: String },
}

impl LlmJob {
    fn job_name(&self) -> &'static str {
        match self {
            LlmJob::CorrectOcr { .. } => "correct_ocr",
            LlmJob::ExtractEntities { .. } => "extract_entities",
            LlmJob::ExtractTriples { .. } => "extract_triples",
            LlmJob::Summarize { .. } => "summarize",
            LlmJob::Classify { .. } => "classify",
            LlmJob::Ask { .. } => "ask",
            LlmJob::CorrectOcrAsset { .. } => "correct_ocr",
            LlmJob::ExtractEntitiesAsset { .. } => "extract_entities",
            LlmJob::ExtractTriplesAsset { .. } => "extract_triples",
            LlmJob::SummarizeAsset { .. } => "summarize",
        }
    }

    /// Returns the ID used as the event/persistence target.
    /// For asset-level jobs, this is the asset_id; for item-level, the item_id.
    fn target_id(&self) -> &str {
        match self {
            LlmJob::CorrectOcr { item_id }
            | LlmJob::ExtractEntities { item_id }
            | LlmJob::ExtractTriples { item_id }
            | LlmJob::Summarize { item_id }
            | LlmJob::Classify { item_id, .. } => item_id,
            LlmJob::Ask { collection_id, .. } => collection_id,
            LlmJob::CorrectOcrAsset { asset_id }
            | LlmJob::ExtractEntitiesAsset { asset_id }
            | LlmJob::ExtractTriplesAsset { asset_id }
            | LlmJob::SummarizeAsset { asset_id } => asset_id,
        }
    }

}

// ---------------------------------------------------------------------------
// Event payloads
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct LlmProgressPayload {
    pub id: String,
    pub job: String,
    pub pct: u8,
}

#[derive(Clone, Serialize)]
pub struct LlmCompletePayload {
    pub id: String,
    pub job: String,
    pub result: String,
}

#[derive(Clone, Serialize)]
pub struct LlmErrorPayload {
    pub id: String,
    pub job: String,
    pub error: String,
}

fn emit_progress(app_handle: &AppHandle, id: &str, job: &str, pct: u8) {
    let _ = app_handle.emit(
        "llm:progress",
        LlmProgressPayload {
            id: id.to_string(),
            job: job.to_string(),
            pct,
        },
    );
}

fn emit_complete(app_handle: &AppHandle, id: &str, job: &str, result: &str) {
    let _ = app_handle.emit(
        "llm:complete",
        LlmCompletePayload {
            id: id.to_string(),
            job: job.to_string(),
            result: result.to_string(),
        },
    );
}

fn emit_error(app_handle: &AppHandle, id: &str, job: &str, error: &str) {
    let _ = app_handle.emit(
        "llm:error",
        LlmErrorPayload {
            id: id.to_string(),
            job: job.to_string(),
            error: error.to_string(),
        },
    );
}

// ---------------------------------------------------------------------------
// Result retrieval (for UI hydration after page reload)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct LlmResultEntry {
    pub target_id: String,
    pub job_type: String,
    pub result: String,
    pub created_at: i64,
}

/// Fetch the latest LLM result for a given target (item or collection) and
/// optional job type. Returns `None` if no result is found.
pub fn get_latest_result(
    conn: &rusqlite::Connection,
    target_id: &str,
    job_type: Option<&str>,
) -> Result<Option<LlmResultEntry>, String> {
    let row = if let Some(jt) = job_type {
        conn.query_row(
            "SELECT target_id, job_type, result, created_at
             FROM llm_results
             WHERE target_id = ?1 AND job_type = ?2
             ORDER BY created_at DESC LIMIT 1",
            params![target_id, jt],
            |row| {
                Ok(LlmResultEntry {
                    target_id: row.get(0)?,
                    job_type: row.get(1)?,
                    result: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
    } else {
        conn.query_row(
            "SELECT target_id, job_type, result, created_at
             FROM llm_results
             WHERE target_id = ?1
             ORDER BY created_at DESC LIMIT 1",
            params![target_id],
            |row| {
                Ok(LlmResultEntry {
                    target_id: row.get(0)?,
                    job_type: row.get(1)?,
                    result: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
    };

    match row {
        Ok(entry) => Ok(Some(entry)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to query llm_results: {e}")),
    }
}

/// Fetch all latest LLM results for a given target (one per job_type).
pub fn get_all_results_for_target(
    conn: &rusqlite::Connection,
    target_id: &str,
) -> Result<Vec<LlmResultEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT target_id, job_type, result, created_at
             FROM llm_results
             WHERE target_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| format!("Failed to prepare llm_results query: {e}"))?;

    let rows = stmt
        .query_map(params![target_id], |row| {
            Ok(LlmResultEntry {
                target_id: row.get(0)?,
                job_type: row.get(1)?,
                result: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| format!("Failed to query llm_results: {e}"))?;

    let mut results = Vec::new();
    let mut seen_job_types = std::collections::HashSet::new();
    for row in rows {
        if let Ok(entry) = row {
            // Keep only the latest result per job_type (DESC order means first is latest)
            if seen_job_types.insert(entry.job_type.clone()) {
                results.push(entry);
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Persist an LLM result to the database. Uses INSERT OR REPLACE so the
/// latest result per (target, job_type) pair is always kept.
fn persist_result(
    conn: &rusqlite::Connection,
    target_id: &str,
    job_type: &str,
    result: &str,
) -> Result<(), String> {
    let id = format!("llr-{target_id}-{job_type}");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    conn.execute(
        "INSERT OR REPLACE INTO llm_results (id, target_id, job_type, result, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, target_id, job_type, result, now],
    )
    .map_err(|e| format!("Failed to persist LLM result: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

pub struct LlmQueue {
    sender: mpsc::Sender<LlmJob>,
    /// Shared flag set to `true` after the LLM engine initializes successfully.
    available: Arc<AtomicBool>,
    /// Shared flag set to `true` after multimodal (vision) support is confirmed.
    multimodal: Arc<AtomicBool>,
}

impl LlmQueue {
    pub fn new() -> (Self, mpsc::Receiver<LlmJob>) {
        let (sender, receiver) = mpsc::channel::<LlmJob>(64);
        let available = Arc::new(AtomicBool::new(false));
        let multimodal = Arc::new(AtomicBool::new(false));
        (
            Self {
                sender,
                available: available.clone(),
                multimodal: multimodal.clone(),
            },
            receiver,
        )
    }

    pub fn submit(&self, job: LlmJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("LLM queue full or closed: {e}"))
    }

    /// Returns `true` if the LLM engine has been loaded successfully.
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }

    /// Returns `true` if the LLM engine supports multimodal (vision) input.
    pub fn is_multimodal(&self) -> bool {
        self.multimodal.load(Ordering::Relaxed)
    }

    /// Returns a clone of the availability flag for sharing with the worker.
    /// Used to signal engine readiness from the worker back to the main state.
    pub fn available_flag(&self) -> Arc<AtomicBool> {
        self.available.clone()
    }

    /// Returns a clone of the multimodal flag for sharing with the worker.
    pub fn multimodal_flag(&self) -> Arc<AtomicBool> {
        self.multimodal.clone()
    }

    pub fn start_worker(
        db_path: PathBuf,
        mut receiver: mpsc::Receiver<LlmJob>,
        app_handle: AppHandle,
        available: Arc<AtomicBool>,
        multimodal: Arc<AtomicBool>,
    ) {
        tauri::async_runtime::spawn(async move {
            const MODEL_FILENAME: &str = "gemma-4-E2B-it-Q4_K_M.gguf";
            // Common mmproj filenames — first match wins
            const MMPROJ_FILENAME: &str = "mmproj-BF16.gguf";

            // Search for model in multiple locations (first match wins)
            let app_models_dir = db_path
                .parent()
                .expect("db_path should have a parent")
                .join("models");
            std::fs::create_dir_all(&app_models_dir).ok();

            let search_paths = [
                // 1. App data dir: {app_data_dir}/models/
                app_models_dir.join(MODEL_FILENAME),
                // 2. Project root (dev convenience) — handles bartowski prefix too
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(MODEL_FILENAME),
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(format!("google_{MODEL_FILENAME}")),
            ];

            let model_path = search_paths
                .iter()
                .find(|p| p.exists())
                .cloned()
                .unwrap_or_else(|| app_models_dir.join(MODEL_FILENAME));

            // Search for mmproj (multimodal projection) — DISABLED in-process due to
            // STATUS_STACK_BUFFER_OVERRUN conflict with pdfium/ort/tesseract.
            // Kept for detection/logging; sidecar approach will use this path.
            let mmproj_search_paths = [
                app_models_dir.join(MMPROJ_FILENAME),
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(MMPROJ_FILENAME),
            ];

            let mmproj_path = mmproj_search_paths
                .iter()
                .find(|p| p.exists())
                .cloned();

            if let Some(ref mmp) = mmproj_path {
                eprintln!(
                    "[llm] Found mmproj: {} — vision DISABLED in-process (sidecar needed)",
                    mmp.display()
                );
            } else {
                eprintln!("[llm] No mmproj found — vision not available");
            }

            let config = LlmConfig {
                model_path: model_path.clone(),
                n_ctx: 4096,
                n_threads: None,
                seed: 1234,
                mmproj_path: mmproj_path.clone(),
            };

            // Initialize engine (optional — degrades gracefully)
            let engine = match tokio::task::spawn_blocking(move || LlmEngine::init(config)).await {
                Ok(Ok(engine)) => {
                    eprintln!("[llm] Engine ready: {}", model_path.display());
                    available.store(true, Ordering::Relaxed);
                    Some(engine)
                }
                Ok(Err(e)) => {
                    eprintln!(
                        "[llm] Engine unavailable: {e} — LLM jobs will degrade gracefully. \
                         Place a GGUF model at: {}",
                        model_path.display()
                    );
                    None
                }
                Err(e) => {
                    eprintln!("[llm] Engine init panicked: {e}");
                    None
                }
            };

            // Start sidecar for vision (multimodal) support.
            // The sidecar loads Gemma + mmproj in an isolated process, avoiding the
            // STATUS_STACK_BUFFER_OVERRUN crash that mmproj causes inside Tauri's
            // process (conflict with pdfium/ort/tesseract).
            let mut sidecar: Option<SidecarHandle> = None;
            if engine.is_some() {
                let sidecar_bin = sidecar::find_sidecar_binary();
                match sidecar_bin {
                    Some(bin) => {
                        if let Some(ref mmp) = mmproj_path {
                            if mmp.exists() {
                                eprintln!("[llm] Starting sidecar for vision support...");
                                let model = model_path.clone();
                                let mmproj = mmp.clone();
                                match tokio::task::block_in_place(|| {
                                    let manager = SidecarManager::new(bin, model, Some(mmproj));
                                    manager.start()
                                }) {
                                    Ok(handle) => {
                                        eprintln!("[llm] Sidecar ready — vision enabled");
                                        sidecar = Some(handle);
                                        multimodal.store(true, Ordering::Relaxed);
                                    }
                                    Err(e) => {
                                        eprintln!("[llm] Sidecar failed: {e} — text-only mode");
                                    }
                                }
                            } else {
                                eprintln!("[llm] mmproj file missing — vision not available");
                            }
                        } else {
                            eprintln!("[llm] No mmproj configured — vision not available");
                        }
                    }
                    None => {
                        eprintln!("[llm] No sidecar binary found — vision not available");
                    }
                }
            }

            // Open dedicated DB connection for the worker
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => {
                    let _ =
                        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;");
                    c
                }
                Err(e) => {
                    eprintln!("[llm] Failed to open worker DB connection: {e}");
                    return;
                }
            };

            // Ensure llm_results table exists (idempotent)
            if let Err(e) = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS llm_results (
                    id TEXT PRIMARY KEY,
                    target_id TEXT NOT NULL,
                    job_type TEXT NOT NULL,
                    result TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_llm_results_target ON llm_results(target_id);",
            ) {
                eprintln!("[llm] Warning: could not create llm_results table: {e}");
            }

            // Main worker loop
            while let Some(job) = receiver.recv().await {
                let job_name = job.job_name();
                let id = job.target_id().to_string();

                let engine = match &engine {
                    Some(e) => e,
                    None => {
                        emit_error(
                            &app_handle,
                            &id,
                            job_name,
                            "LLM engine not available. Place a GGUF model in the models/ directory.",
                        );
                        continue;
                    }
                };

                emit_progress(&app_handle, &id, job_name, 10);

                let result = tokio::task::block_in_place(|| {
                    process_job(engine, sidecar.as_mut(), &conn, &job)
                });

                match result {
                    Ok(output) => {
                        // Persist result to database (non-fatal if it fails)
                        if let Err(e) = persist_result(&conn, &id, job_name, &output) {
                            eprintln!("[llm] Warning: failed to persist result for {id}/{job_name}: {e}");
                        }

                        emit_progress(&app_handle, &id, job_name, 100);
                        emit_complete(&app_handle, &id, job_name, &output);
                    }
                    Err(e) => {
                        emit_error(&app_handle, &id, job_name, &e);
                    }
                }
            }

            eprintln!("[llm] Worker loop ended — channel closed.");
        });
    }
}

// ---------------------------------------------------------------------------
// Job processing
// ---------------------------------------------------------------------------

/// Max tokens for generation per job type.
fn max_tokens_for(job: &LlmJob) -> i32 {
    match job {
        LlmJob::CorrectOcr { .. } | LlmJob::CorrectOcrAsset { .. } => 2048,
        LlmJob::ExtractEntities { .. } | LlmJob::ExtractEntitiesAsset { .. } => 1024,
        LlmJob::ExtractTriples { .. } | LlmJob::ExtractTriplesAsset { .. } => 1024,
        LlmJob::Summarize { .. } | LlmJob::SummarizeAsset { .. } => 512,
        LlmJob::Classify { .. } => 256,
        LlmJob::Ask { .. } => 512,
    }
}

/// Truncate text to the last sentence boundary (period, exclamation, question mark)
/// so it doesn't cut mid-sentence. Used for summaries that get truncated by token limits.
fn truncate_to_sentence_boundary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // If the text already ends with a sentence-ending punctuation, it's fine.
    if trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?') || trimmed.ends_with('。') || trimmed.ends_with('！') {
        return trimmed.to_string();
    }

    // Find the last sentence-ending punctuation and truncate there.
    // Search backwards for . ! ? 。 ！
    let sentence_enders = ['.', '!', '?', '。', '！'];
    if let Some(pos) = trimmed.rfind(sentence_enders) {
        // Include the punctuation character
        trimmed[..=pos].to_string()
    } else {
        // No sentence boundary found at all — return as-is (better than nothing)
        trimmed.to_string()
}
}

// ---------------------------------------------------------------------------
// Text truncation for context safety
// ---------------------------------------------------------------------------

/// Conservative characters-per-token estimate for Latin-script text.
/// Gemma tokenizer averages ~3.5 chars/token for English/Spanish; using 3.0
/// provides a safety margin for multi-byte characters and template overhead.
const CHARS_PER_TOKEN_ESTIMATE: usize = 3;

/// Tokens reserved for prompt template instructions and formatting.
/// Each prompt wraps the text in instruction text (~50-150 tokens for Gemma
/// chat format markers + task instructions).
const TEMPLATE_OVERHEAD_TOKENS: i32 = 128;

/// Truncate text so that the resulting prompt + max_tokens fits within n_ctx.
/// Uses a conservative heuristic and cuts at sentence boundaries when possible.
fn truncate_text_for_context(n_ctx: u32, max_tokens: i32, text: &str) -> String {
    let budget_tokens = (n_ctx as i32) - max_tokens - TEMPLATE_OVERHEAD_TOKENS;
    if budget_tokens <= 0 {
        // Extremely small context — return first ~500 chars as a last resort
        return text.chars().take(500).collect();
    }
    let budget_chars = budget_tokens as usize * CHARS_PER_TOKEN_ESTIMATE;
    let text_chars = text.chars().count();
    if text_chars <= budget_chars {
        return text.to_string();
    }

    // Collect chars up to budget, then try to cut at the last sentence boundary
    let truncated: String = text.chars().take(budget_chars).collect();
    if let Some(pos) = truncated.rfind(|c: char| c == '.' || c == '\n' || c == '！' || c == '。') {
        // Keep up to and including the sentence boundary char
        truncated[..=pos].to_string()
    } else {
        truncated
    }
}

// ---------------------------------------------------------------------------
// Context gathering for Ask (FTS-based)
// ---------------------------------------------------------------------------

/// Maximum context size in characters for Ask queries (~2000 tokens budget).
const MAX_ASK_CONTEXT_CHARS: usize = 6000;

/// Maximum characters per individual document snippet (~400 tokens).
const MAX_SNIPPET_CHARS: usize = 1200;

fn process_job(engine: &LlmEngine, sidecar: Option<&mut SidecarHandle>, conn: &rusqlite::Connection, job: &LlmJob) -> Result<String, String> {
    let n_ctx = engine.n_ctx();

    match job {
        LlmJob::CorrectOcr { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for OCR correction".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::ocr_correction(&truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ExtractEntities { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for entity extraction".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::extract_entities(&truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ExtractTriples { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for triple extraction".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::extract_triples(&truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::Summarize { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for summarization".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::summarize(&truncated);
            let result = engine.generate(&p, max_tokens_for(job))?;
            Ok(truncate_to_sentence_boundary(&result))
        }

        LlmJob::Classify { item_id, categories } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for classification".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::classify(&truncated, categories);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::Ask { collection_id, question } => {
            // Gather context from FTS search over the collection
            let context = gather_collection_context(conn, collection_id, question)?;
            if context.is_empty() {
                return Err("No relevant documents found for this question".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &context);
            let p = prompt::question_answer(question, &truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        // ── Asset-level variants (single page/asset, avoids context overflow) ──

        LlmJob::CorrectOcrAsset { asset_id } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for OCR correction on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::ocr_correction(&truncated);

            // Try vision via sidecar if available and image exists.
            // Falls back to text-only if sidecar fails or no image path.
            if let Some(sc) = sidecar {
                match get_asset_image_path(conn, asset_id) {
                    Ok(Some(image_path)) => {
                        match sc.generate_with_image(&image_path.to_string_lossy(), &p, max_tokens_for(job)) {
                            Ok(result) => return Ok(result),
                            Err(e) => {
                                eprintln!("[llm] Sidecar vision failed, falling back to text-only: {e}");
                            }
                        }
                    }
                    Ok(None) => {
                        // No image path (e.g. audio/PDF) — use text-only
                    }
                    Err(e) => {
                        eprintln!("[llm] Could not resolve image path for {asset_id}: {e}");
                    }
                }
            }

            // Text-only fallback (no sidecar, no image, or sidecar failure)
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ExtractEntitiesAsset { asset_id } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for entity extraction on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::extract_entities(&truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ExtractTriplesAsset { asset_id } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for triple extraction on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::extract_triples(&truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::SummarizeAsset { asset_id } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for summarization on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::summarize(&truncated);
            let result = engine.generate(&p, max_tokens_for(job))?;
            Ok(truncate_to_sentence_boundary(&result))
        }
    }
}

/// Gathers relevant text snippets from a collection using FTS search.
///
/// Uses the existing `sanitize_fts5_query` to safely handle natural-language
/// questions, and retrieves full text via `text_provider::get_item_text`
/// instead of a broken LEFT JOIN on extrations.
fn gather_collection_context(
    conn: &rusqlite::Connection,
    collection_id: &str,
    question: &str,
) -> Result<String, String> {
    // Sanitize the question for FTS5 — natural-language queries contain
    // operators and noise that break FTS MATCH.
    let fts_query = crate::nlp::fts::sanitize_fts5_query(question);
    if fts_query.is_empty() {
        return Ok(String::new());
    }

    // Find matching item IDs via FTS (top 5 by relevance)
    let item_ids: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT i.id
                 FROM fts_items f
                 JOIN items i ON i.rowid = f.rowid
                 WHERE fts_items MATCH ?1 AND i.collection_id = ?2
                 ORDER BY rank
                 LIMIT 5",
            )
            .map_err(|e| format!("FTS query prepare failed: {e}"))?;

        let rows = stmt
            .query_map(params![fts_query, collection_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| format!("FTS query failed: {e}"))?;

        rows.filter_map(|r| r.ok()).collect()
    };

    if item_ids.is_empty() {
        return Ok(String::new());
    }

    // For each matching item, retrieve full text via text_provider
    let mut context = String::new();
    for item_id in &item_ids {
        let title: String = conn
            .query_row(
                "SELECT title FROM items WHERE id = ?1",
                params![item_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "Unknown".to_string());

        let text = text_provider::get_item_text(conn, item_id).unwrap_or_default();
        if !text.is_empty() {
            // Truncate each snippet to stay within budget
            let display_text: String = if text.chars().count() > MAX_SNIPPET_CHARS {
                text.chars().take(MAX_SNIPPET_CHARS).collect()
            } else {
                text.clone()
            };

            let snippet = format!("--- {} ---\n{}\n\n", title, display_text);
            if context.len() + snippet.len() > MAX_ASK_CONTEXT_CHARS {
                // Budget exceeded — add what fits and stop
                let remaining = MAX_ASK_CONTEXT_CHARS.saturating_sub(context.len());
                if remaining > 0 {
                    context.push_str(&snippet[..remaining.min(snippet.len())]);
                }
                break;
            }
            context.push_str(&snippet);
        }
    }

    Ok(context)
}

/// Look up the image/pdf file path for an asset from the database.
/// Returns Ok(Some(path)) if the asset exists and is an image type,
/// Ok(None) if the asset exists but is audio/PDF (no direct image),
/// Err if the asset doesn't exist or the query fails.
fn get_asset_image_path(conn: &rusqlite::Connection, asset_id: &str) -> Result<Option<PathBuf>, String> {
    let path_str: String = conn
        .query_row(
            "SELECT path FROM assets WHERE id = ?1",
            params![asset_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("Failed to query asset path: {e}"))?
        .unwrap_or_default();

    if path_str.is_empty() {
        return Ok(None);
    }

    let path = PathBuf::from(&path_str);

    // Only return path for visual assets (image/pdf). Audio has no image.
    let asset_type: String = conn
        .query_row(
            "SELECT type FROM assets WHERE id = ?1",
            params![asset_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("Failed to query asset type: {e}"))?
        .unwrap_or_default();

    if asset_type == "audio" {
        return Ok(None);
    }

    // For PDFs, we'd need to render a page — not supported yet.
    // Only serve direct image files for now.
    if asset_type == "pdf" {
        // PDFs need page rendering which we don't do here yet.
        // Return None to fall back to text-only correction.
        return Ok(None);
    }

    Ok(Some(path))
}
