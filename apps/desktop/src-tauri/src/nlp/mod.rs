pub mod commands;
pub mod embeddings;
pub mod fts;
pub mod ner;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

// ── Event payloads ───────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct NlpProgressPayload {
    pub item_id: String,
    pub job: String,
    pub pct: u8,
}

#[derive(Clone, Serialize)]
pub struct NlpCompletePayload {
    pub item_id: String,
    pub job: String,
}

#[derive(Clone, Serialize)]
pub struct NlpErrorPayload {
    pub item_id: String,
    pub job: String,
    pub error: String,
}

// ── Job & Queue ──────────────────────────────────────────────────────────────

/// A single NLP work unit submitted to the background worker.
pub enum NlpJob {
    IndexFts { item_id: String },
    ComputeEmbedding { item_id: String },
    ExtractEntities { item_id: String },
}

/// Handle for submitting NLP jobs to the background worker.
///
/// Managed as Tauri state — NLP commands grab this via `State<NlpQueue>`.
pub struct NlpQueue {
    sender: mpsc::Sender<NlpJob>,
}

impl NlpQueue {
    /// Create a new queue and return `(NlpQueue, Receiver)`.
    pub fn new() -> (Self, mpsc::Receiver<NlpJob>) {
        let (sender, receiver) = mpsc::channel::<NlpJob>(64);
        (Self { sender }, receiver)
    }

    /// Submit a job to the queue. Returns immediately.
    pub fn submit(&self, job: NlpJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("Failed to enqueue NLP job: {e}"))
    }

    /// Spawn the background worker loop on the Tokio runtime.
    ///
    /// The worker drains jobs serially and emits `nlp:progress`, `nlp:complete`,
    /// or `nlp:error` events per job.
    pub fn start_worker(
        db_path: std::path::PathBuf,
        mut receiver: mpsc::Receiver<NlpJob>,
        app_handle: AppHandle,
    ) {
        tokio::spawn(async move {
            // Open a dedicated SQLite connection for the NLP worker.
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => {
                    let _ = c.execute_batch(
                        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;",
                    );
                    // Load sqlite-vec extension for vec0 virtual table support.
                    if let Err(e) = sqlite_vec::load(&c) {
                        // Non-fatal: embeddings will be degraded, FTS5/NER continue.
                        eprintln!("[nlp] sqlite-vec load failed: {e} — embedding jobs will be skipped");
                    }
                    c
                }
                Err(e) => {
                    eprintln!("[nlp] Failed to open worker DB connection: {e}");
                    return;
                }
            };

            while let Some(job) = receiver.recv().await {
                match job {
                    NlpJob::IndexFts { item_id } => {
                        emit_progress(&app_handle, &item_id, "fts", 10);
                        let result = tokio::task::block_in_place(|| {
                            // Fetch item data from DB and index into FTS5.
                            fts::index_item_from_db(&conn, &item_id)
                        });
                        match result {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "fts", 100);
                                emit_complete(&app_handle, &item_id, "fts");
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "fts", &e),
                        }
                    }
                    NlpJob::ComputeEmbedding { item_id } => {
                        emit_progress(&app_handle, &item_id, "embed", 10);
                        let result = tokio::task::block_in_place(|| {
                            embeddings::compute_and_store(&conn, &item_id)
                        });
                        match result {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "embed", 100);
                                emit_complete(&app_handle, &item_id, "embed");
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "embed", &e),
                        }
                    }
                    NlpJob::ExtractEntities { item_id } => {
                        emit_progress(&app_handle, &item_id, "ner", 10);
                        let result = tokio::task::block_in_place(|| {
                            ner::extract_and_store(&conn, &item_id)
                        });
                        match result {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "ner", 100);
                                emit_complete(&app_handle, &item_id, "ner");
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "ner", &e),
                        }
                    }
                }
            }
        });
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn emit_progress(app_handle: &AppHandle, item_id: &str, job: &str, pct: u8) {
    let _ = app_handle.emit(
        "nlp:progress",
        NlpProgressPayload {
            item_id: item_id.to_string(),
            job: job.to_string(),
            pct,
        },
    );
}

fn emit_complete(app_handle: &AppHandle, item_id: &str, job: &str) {
    let _ = app_handle.emit(
        "nlp:complete",
        NlpCompletePayload {
            item_id: item_id.to_string(),
            job: job.to_string(),
        },
    );
}

fn emit_error(app_handle: &AppHandle, item_id: &str, job: &str, error: &str) {
    let _ = app_handle.emit(
        "nlp:error",
        NlpErrorPayload {
            item_id: item_id.to_string(),
            job: job.to_string(),
            error: error.to_string(),
        },
    );
}
