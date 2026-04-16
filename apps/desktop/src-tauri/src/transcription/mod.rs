// Audio decoding module — currently unused (faster-whisper handles audio decoding internally).
// Will be re-enabled if we need audio duration/preview in Rust.
// #[allow(dead_code)]
// mod audio;
pub mod commands;
mod engine;

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

        eprintln!(
            "[transcription] Script path: {}",
            script_path.display()
        );

        // Find Python interpreter
        let python_path = which_python();

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
                    model_dir: None, // Use faster-whisper's default cache
                })
                .expect("Failed to initialize transcription engine");

                eprintln!("[transcription] Worker ready, processing jobs.");

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
/// Tries each candidate, runs `python -c "import faster_whisper"` to verify
/// the module is importable. Falls back to the first Python found if none
/// have faster_whisper (the error from the subprocess will be clearer).
fn which_python() -> std::path::PathBuf {
    let candidates = [
        // Conda environments first — most likely to have ML packages
        r"C:\Users\agusn\miniconda3\python.exe",
        r"C:\Users\agusn\anaconda3\python.exe",
        r"C:\Users\agusn\miniconda3\envs\entropia\python.exe",
        // System Python
        "python",
        "python3",
        "python3.11",
        "python3.12",
    ];

    let mut first_found: Option<std::path::PathBuf> = None;

    for candidate in &candidates {
        let path = std::path::PathBuf::from(candidate);

        // Check if the interpreter exists and runs
        let version_ok = std::process::Command::new(&path)
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        if version_ok.is_err() {
            continue;
        }

        let version_output = version_ok.unwrap();
        if !version_output.status.success() {
            continue;
        }

        if first_found.is_none() {
            first_found = Some(path.clone());
        }

        // Verify faster_whisper is importable
        let import_ok = std::process::Command::new(&path)
            .args(["-c", "import faster_whisper; print('ok')"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        if let Ok(output) = import_ok {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim() == "ok" {
                    eprintln!(
                        "[transcription] Found Python with faster_whisper: {}",
                        path.display()
                    );
                    return path;
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "[transcription] Python {} found but faster_whisper not importable: {}",
                    path.display(),
                    stderr.trim()
                );
            }
        }
    }

    // No Python with faster_whisper found — return the first Python we found
    // so the error from the subprocess makes the problem clear.
    if let Some(path) = first_found {
        eprintln!(
            "[transcription] WARNING: No Python with faster_whisper found. Falling back to: {}",
            path.display()
        );
        path
    } else {
        eprintln!("[transcription] ERROR: No Python interpreter found on this system!");
        std::path::PathBuf::from("python")
    }
}

// ── Persistence ─────────────────────────────────────────────────────────────

/// Save a transcription result to the database.
fn save_transcription(
    conn: &rusqlite::Connection,
    asset_id: &str,
    result: &TranscriptionResult,
    model_name: &str,
) -> Result<(), String> {
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

    Ok(())
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
    save_transcription(conn, &job.asset_id, &result, "faster-whisper/base")?;

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