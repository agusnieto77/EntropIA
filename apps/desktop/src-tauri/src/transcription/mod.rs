// Audio decoding module — currently unused (faster-whisper handles audio decoding internally).
// Will be re-enabled if we need audio duration/preview in Rust.
// #[allow(dead_code)]
// mod audio;
pub mod commands;
mod engine;

use crate::nlp::{lookup_item_id_for_asset, NlpJob, NlpQueue};
use engine::{TranscriptionResult, WhisperConfig, WhisperEngine};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, path::BaseDirectory};

// ── Event payloads ──────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct TranscriptionProgressPayload {
    pub asset_id: String,
    pub pct: u8,
    pub stage: String,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionCompletePayload {
    pub asset_id: String,
    pub text: String,
    pub language: String,
    pub duration_ms: u64,
    pub segments_count: usize,
}

#[derive(Clone, Serialize)]
pub struct TranscriptionErrorPayload {
    pub asset_id: String,
    pub error: String,
}

// ── Job & Queue ─────────────────────────────────────────────────────────────

/// A single transcription work unit submitted to the background worker.
pub struct TranscriptionJob {
    pub asset_id: String,
    pub asset_path: String,
}

/// Handle for submitting jobs to the background transcription worker.
///
/// Managed as Tauri state — the `transcribe_audio` command grabs this via
/// `State<TranscriptionQueue>`.
pub struct TranscriptionQueue {
    sender: tokio::sync::mpsc::Sender<TranscriptionJob>,
}

impl TranscriptionQueue {
    /// Create a new queue and return `(TranscriptionQueue, Receiver)`.
    pub fn new() -> (Self, tokio::sync::mpsc::Receiver<TranscriptionJob>) {
        let (sender, receiver) = tokio::sync::mpsc::channel::<TranscriptionJob>(64);
        (Self { sender }, receiver)
    }

    /// Submit a job to the queue. Returns immediately.
    pub fn submit(&self, job: TranscriptionJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("Failed to enqueue transcription job: {e}"))
    }

    /// Spawn the background worker loop on a dedicated thread.
    ///
    /// Each transcription call spawns a Python subprocess, so no persistent
    /// WhisperContext is held. The thread just drains the queue and spawns
    /// processes sequentially.
    pub fn start_worker(
        db_path: std::path::PathBuf,
        mut receiver: tokio::sync::mpsc::Receiver<TranscriptionJob>,
        app_handle: AppHandle,
    ) {
        // Resolve script path: try Resource directory first (production), then source (dev)
        let script_path = app_handle
            .path()
            .resolve("scripts/transcribe.py", BaseDirectory::Resource)
            .unwrap_or_else(|_| {
                // Dev fallback: look relative to the src-tauri directory
                let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/scripts/transcribe.py");
                if dev_path.exists() {
                    dev_path
                } else {
                    // Last resort
                    std::path::PathBuf::from("scripts/transcribe.py")
                }
            });

        // Find Python interpreter
        let python_path = match which_python() {
            Some(p) => p,
            None => {
                eprintln!("[transcription] No Python with faster_whisper found — worker will report errors for all jobs.");
                std::thread::Builder::new()
                    .name("transcription-worker".to_string())
                    .stack_size(8 * 1024 * 1024)
                    .spawn(move || {
                        while let Some(job) = receiver.blocking_recv() {
                            let _ = app_handle.emit(
                                "transcription:error",
                                TranscriptionErrorPayload {
                                    asset_id: job.asset_id,
                                    error: "No Python interpreter with faster_whisper found. Please install Python and run: pip install faster-whisper".to_string(),
                                },
                            );
                        }
                    })
                    .expect("Failed to spawn transcription worker thread (no-python fallback)");
                return;
            }
        };

        // Resolve model cache directory inside app data (avoids HuggingFace symlink
        // issues on Windows — WinError 448 "untrusted mount point" on reparse points)
        let model_cache_dir = app_handle
            .path()
            .app_data_dir()
            .expect("Failed to get app data dir for model cache")
            .join("hf_cache");
        std::fs::create_dir_all(&model_cache_dir).unwrap_or_else(|e| {
            eprintln!("[transcription] Warning: could not create model cache dir {}: {e}", model_cache_dir.display());
        });
        std::thread::Builder::new()
            .name("transcription-worker".to_string())
            .stack_size(8 * 1024 * 1024) // 8 MB — subprocess only, no heavy stack needed
            .spawn(move || {
                let engine = WhisperEngine::init(WhisperConfig {
                    python_path: python_path,
                    script_path,
                    model_size: "base".to_string(),
                    language: "es".to_string(),
                    compute_type: "int8".to_string(),
                    model_dir: Some(model_cache_dir),
                });

                let engine = match engine {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("[transcription] Failed to initialize transcription engine: {e}");
                        while let Some(job) = receiver.blocking_recv() {
                            let _ = app_handle.emit(
                                "transcription:error",
                                TranscriptionErrorPayload {
                                    asset_id: job.asset_id,
                                    error: format!("Transcription engine initialization failed: {e}"),
                                },
                            );
                        }
                        return;
                    }
                };

                // ── Open dedicated DB connection ────────────────────────────
                let conn = match rusqlite::Connection::open(&db_path) {
                    Ok(c) => {
                        let _ = c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;");
                        c
                    }
                    Err(e) => {
                        eprintln!("[transcription] Failed to open DB connection: {e}");
                        while let Some(job) = receiver.blocking_recv() {
                            let _ = app_handle.emit(
                                "transcription:error",
                                TranscriptionErrorPayload {
                                    asset_id: job.asset_id,
                                    error: format!("DB connection failed: {e}"),
                                },
                            );
                        }
                        return;
                    }
                };

                // ── Main work loop ──────────────────────────────────────────
                while let Some(job) = receiver.blocking_recv() {
                    let asset_id = job.asset_id.clone();
                    let result = process_job(&engine, &conn, &job, &app_handle);

                    match result {
                        Ok(transcription) => {
                            let _ = app_handle.emit(
                                "transcription:complete",
                                TranscriptionCompletePayload {
                                    asset_id: asset_id.clone(),
                                    text: transcription.text.clone(),
                                    language: transcription.language.clone(),
                                    duration_ms: transcription.duration_ms,
                                    segments_count: transcription.segments.len(),
                                },
                            );
                        }
                        Err(err) => {
                            eprintln!("[transcription] Error for {asset_id}: {err}");
                            let _ = app_handle.emit(
                                "transcription:error",
                                TranscriptionErrorPayload {
                                    asset_id,
                                    error: err,
                                },
                            );
                        }
                    }
                }
            })
            .expect("Failed to spawn transcription worker thread");
    }
}

/// Find the Python interpreter on the system that has `faster_whisper` available.
///
/// Uses the shared Python candidate cache to avoid redundant filesystem scans
/// and log noise. Probes each candidate for the `faster_whisper` module.
fn which_python() -> Option<std::path::PathBuf> {
    crate::python_discovery::which_python_for_module(
        "transcription",
        "faster_whisper",
        "import faster_whisper; print('ok')",
    )
}

// ── Persistence ─────────────────────────────────────────────────────────────

/// Save a transcription result to the database.
fn save_transcription(
    conn: &rusqlite::Connection,
    asset_id: &str,
    result: &TranscriptionResult,
    model_name: &str,
) -> Result<Option<String>, String> {
    // Serialize segments as JSON (using the same Segment struct)
    let segments_json = serde_json::to_string(&result.segments)
        .map_err(|e| format!("Failed to serialize segments: {e}"))?;

    // Delete existing transcription for this asset (upsert semantics)
    conn.execute(
        "DELETE FROM transcriptions WHERE asset_id = ?1",
        [asset_id],
    )
    .map_err(|e| format!("Failed to delete existing transcription: {e}"))?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO transcriptions(id, asset_id, text_content, language, duration_ms, model, segments, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            asset_id,
            result.text,
            result.language,
            result.duration_ms as i64,
            model_name,
            segments_json,
            None::<f64>, // confidence — not provided by faster-whisper directly
            now,
        ],
    )
    .map_err(|e| format!("Failed to insert transcription: {e}"))?;

    lookup_item_id_for_asset(conn, asset_id)
}

// ── Job Processing ──────────────────────────────────────────────────────────

/// Process a single transcription job.
fn process_job(
    engine: &WhisperEngine,
    conn: &rusqlite::Connection,
    job: &TranscriptionJob,
    app_handle: &AppHandle,
) -> Result<TranscriptionResult, String> {
    emit_progress(app_handle, &job.asset_id, 10, "reading");

    // Stage 1 — the Python script handles both decoding and transcription
    eprintln!("[transcription] Transcribing: {}", job.asset_path);
    emit_progress(app_handle, &job.asset_id, 30, "transcribing");

    let result = engine
        .transcribe(&job.asset_path, 0)?; // duration_ms comes from Python output

    emit_progress(app_handle, &job.asset_id, 80, "saving");

    // Stage 2 — persist to SQLite
    if let Some(item_id) = save_transcription(conn, &job.asset_id, &result, "faster-whisper/base")? {
        // Asset-level NER + triples: only re-extract for the transcribed asset,
        // not the entire item. Avoids reprocessing unchanged pages.
        let nlp_queue = app_handle.state::<NlpQueue>();
        if let Err(e) = nlp_queue.submit(NlpJob::ExtractEntitiesForAsset {
            item_id: item_id.clone(),
            asset_id: job.asset_id.clone(),
        }) {
            eprintln!("[nlp] Failed to auto-enqueue ExtractEntitiesForAsset after transcription save: {e}");
        } else {
            eprintln!(
                "[nlp] Auto-enqueued ExtractEntitiesForAsset after transcription save: asset_id={}, item_id={}",
                job.asset_id, item_id
            );
        }
        if let Err(e) = nlp_queue.submit(NlpJob::ExtractTriplesForAsset {
            item_id: item_id.clone(),
            asset_id: job.asset_id.clone(),
        }) {
            eprintln!("[nlp] Failed to auto-enqueue ExtractTriplesForAsset after transcription save: {e}");
        }
    }

    emit_progress(app_handle, &job.asset_id, 100, "done");

    Ok(result)
}

/// Emit a `transcription:progress` event to the frontend.
fn emit_progress(app_handle: &AppHandle, asset_id: &str, pct: u8, stage: &str) {
    let _ = app_handle.emit(
        "transcription:progress",
        TranscriptionProgressPayload {
            asset_id: asset_id.to_string(),
            pct,
            stage: stage.to_string(),
        },
    );
}
