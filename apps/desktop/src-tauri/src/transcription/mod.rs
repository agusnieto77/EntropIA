// Audio decoding module — currently unused (faster-whisper handles audio decoding internally).
// Will be re-enabled if we need audio duration/preview in Rust.
// #[allow(dead_code)]
// mod audio;
pub mod commands;
mod engine;

use engine::{TranscriptionResult, WhisperConfig, WhisperEngine};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, path::BaseDirectory};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn apply_windows_no_window(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

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
        eprintln!(
            "[transcription] Model cache dir: {}",
            model_cache_dir.display()
        );

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
/// Discovery strategy:
/// 1. If `CONDA_PREFIX` env var is set, prefer that Python (we're inside a conda env)
/// 2. Use `where` (Windows) / `which` (Unix) to discover all Python executables on PATH
/// 3. Try python3 explicitly on Unix
/// 4. Scan common Conda/Python install locations not on PATH (Windows)
/// 5. Return the first match with the required module, or None if nothing works
fn which_python() -> Option<std::path::PathBuf> {
    let module = "faster_whisper";
    let mut candidates = Vec::new();

    // 1. Conda environment — if CONDA_PREFIX is set, that Python is authoritative
    if let Ok(conda_prefix) = std::env::var("CONDA_PREFIX") {
        let conda_python = if cfg!(windows) {
            std::path::PathBuf::from(&conda_prefix).join("python.exe")
        } else {
            std::path::PathBuf::from(&conda_prefix).join("bin").join("python")
        };
        eprintln!("[transcription] CONDA_PREFIX detected: {}", conda_python.display());
        candidates.push(conda_python);
    }

    // 2. Discover Python executables on PATH via `where` (Windows) / `which` (Unix)
    let finder_cmd = if cfg!(windows) { "where" } else { "which" };
    let mut find_python_cmd = std::process::Command::new(finder_cmd);
    apply_windows_no_window(&mut find_python_cmd);
    if let Ok(output) = find_python_cmd
        .arg("python")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let path = std::path::PathBuf::from(line.trim());
                if path.is_file() && !candidates.contains(&path) {
                    candidates.push(path);
                }
            }
        }
    }

    // 3. Also try python3 explicitly (common on Linux/macOS)
    if cfg!(unix) {
        let mut find_python3_cmd = std::process::Command::new(finder_cmd);
        apply_windows_no_window(&mut find_python3_cmd);
        if let Ok(output) = find_python3_cmd
            .arg("python3")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let path = std::path::PathBuf::from(line.trim());
                    if path.is_file() && !candidates.contains(&path) {
                        candidates.push(path);
                    }
                }
            }
        }
    }

    // 4. Scan common Conda/Python install locations not on PATH
    //    (e.g. r-miniconda, miniconda3, anaconda3 under AppData or home)
    if cfg!(windows) {
        if let Ok(user_profile) = std::env::var("USERPROFILE") {
            let home = std::path::PathBuf::from(&user_profile);
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                let lad = std::path::PathBuf::from(&local_app_data);
                for dir in [
                    lad.join("r-miniconda"),           // R's embedded Conda
                    lad.join("miniconda3"),            // Miniconda in AppData\Local
                    lad.join("anaconda3"),             // Anaconda in AppData\Local
                    home.join("miniconda3"),            // Miniconda in user home
                    home.join("anaconda3"),             // Anaconda in user home
                    home.join(".conda"),                // .conda directory
                ] {
                    let python_exe = dir.join("python.exe");
                    if python_exe.is_file() && !candidates.contains(&python_exe) {
                        eprintln!("[transcription] Found Python at common location: {}", python_exe.display());
                        candidates.push(python_exe);
                    }
                    // Also check envs/ subdirectories
                    let envs_dir = dir.join("envs");
                    if envs_dir.is_dir() {
                        if let Ok(entries) = std::fs::read_dir(&envs_dir) {
                            for entry in entries.flatten() {
                                let env_python = entry.path().join("python.exe");
                                if env_python.is_file() && !candidates.contains(&env_python) {
                                    eprintln!("[transcription] Found Python in Conda env: {}", env_python.display());
                                    candidates.push(env_python);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        eprintln!("[transcription] ERROR: No Python interpreter found on this system!");
        return None;
    }

    // 5. Probe each candidate for the required module
    for candidate in &candidates {
        let mut probe_cmd = std::process::Command::new(candidate);
        apply_windows_no_window(&mut probe_cmd);
        let import_ok = probe_cmd
            .args(["-c", &format!("import {module}; print('ok')")])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        match import_ok {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim() == "ok" {
                    eprintln!(
                        "[transcription] Found Python with {module}: {}",
                        candidate.display()
                    );
                    return Some(candidate.clone());
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "[transcription] Python {} found but {module} not importable: {}",
                    candidate.display(),
                    stderr.trim()
                );
            }
            Err(e) => {
                eprintln!("[transcription] Failed to probe {}: {e}", candidate.display());
            }
        }
    }

    eprintln!(
        "[transcription] WARNING: No Python with {module} found among {} candidates",
        candidates.len()
    );
    None
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
