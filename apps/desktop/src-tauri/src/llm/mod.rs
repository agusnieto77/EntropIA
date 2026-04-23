pub mod commands;
pub mod engine;
pub mod prompt;

use std::path::PathBuf;

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use crate::nlp::text_provider;

use self::engine::{LlmConfig, LlmEngine};

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
        }
    }

    fn item_or_collection_id(&self) -> &str {
        match self {
            LlmJob::CorrectOcr { item_id }
            | LlmJob::ExtractEntities { item_id }
            | LlmJob::ExtractTriples { item_id }
            | LlmJob::Summarize { item_id }
            | LlmJob::Classify { item_id, .. } => item_id,
            LlmJob::Ask { collection_id, .. } => collection_id,
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
// Queue
// ---------------------------------------------------------------------------

pub struct LlmQueue {
    sender: mpsc::Sender<LlmJob>,
}

impl LlmQueue {
    pub fn new() -> (Self, mpsc::Receiver<LlmJob>) {
        let (sender, receiver) = mpsc::channel::<LlmJob>(64);
        (Self { sender }, receiver)
    }

    pub fn submit(&self, job: LlmJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("LLM queue full or closed: {e}"))
    }

    pub fn start_worker(
        db_path: PathBuf,
        mut receiver: mpsc::Receiver<LlmJob>,
        app_handle: AppHandle,
    ) {
        tauri::async_runtime::spawn(async move {
            const MODEL_FILENAME: &str = "gemma-4-E2B-it-Q4_K_M.gguf";

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

            let config = LlmConfig {
                model_path: model_path.clone(),
                n_ctx: 4096,
                n_threads: None,
                seed: 1234,
            };

            // Initialize engine (optional — degrades gracefully)
            let engine = match tokio::task::spawn_blocking(move || LlmEngine::init(config)).await {
                Ok(Ok(engine)) => {
                    eprintln!("[llm] Engine ready: {}", model_path.display());
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

            // Main worker loop
            while let Some(job) = receiver.recv().await {
                let job_name = job.job_name();
                let id = job.item_or_collection_id().to_string();

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
                    process_job(engine, &conn, &job)
                });

                match result {
                    Ok(output) => {
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
        LlmJob::CorrectOcr { .. } => 2048,
        LlmJob::ExtractEntities { .. } => 1024,
        LlmJob::ExtractTriples { .. } => 1024,
        LlmJob::Summarize { .. } => 512,
        LlmJob::Classify { .. } => 256,
        LlmJob::Ask { .. } => 512,
    }
}

fn process_job(engine: &LlmEngine, conn: &rusqlite::Connection, job: &LlmJob) -> Result<String, String> {
    match job {
        LlmJob::CorrectOcr { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for OCR correction".to_string());
            }
            let p = prompt::ocr_correction(&text);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ExtractEntities { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for entity extraction".to_string());
            }
            let p = prompt::extract_entities(&text);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ExtractTriples { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for triple extraction".to_string());
            }
            let p = prompt::extract_triples(&text);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::Summarize { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for summarization".to_string());
            }
            let p = prompt::summarize(&text);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::Classify { item_id, categories } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for classification".to_string());
            }
            let p = prompt::classify(&text, categories);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::Ask { collection_id, question } => {
            // Gather context from FTS search over the collection
            let context = gather_collection_context(conn, collection_id, question)?;
            if context.is_empty() {
                return Err("No relevant documents found for this question".to_string());
            }
            let p = prompt::question_answer(question, &context);
            engine.generate(&p, max_tokens_for(job))
        }
    }
}

/// Gathers relevant text snippets from a collection using FTS search.
fn gather_collection_context(
    conn: &rusqlite::Connection,
    collection_id: &str,
    question: &str,
) -> Result<String, String> {
    // Search FTS index for relevant items in this collection, take top 5
    let mut stmt = conn
        .prepare(
            "SELECT i.title, e.text_content
             FROM fts_items f
             JOIN items i ON i.id = f.item_id
             LEFT JOIN extractions e ON e.item_id = i.id
             WHERE f.fts_items MATCH ?1 AND i.collection_id = ?2
             ORDER BY rank
             LIMIT 5",
        )
        .map_err(|e| format!("FTS query failed: {e}"))?;

    let rows = stmt
        .query_map(params![question, collection_id], |row| {
            let title: String = row.get(0)?;
            let text: Option<String> = row.get(1)?;
            Ok((title, text.unwrap_or_default()))
        })
        .map_err(|e| format!("FTS query failed: {e}"))?;

    let mut context = String::new();
    for row in rows {
        if let Ok((title, text)) = row {
            if !text.is_empty() {
                context.push_str(&format!("--- {} ---\n{}\n\n", title, text));
            }
        }
    }

    Ok(context)
}
