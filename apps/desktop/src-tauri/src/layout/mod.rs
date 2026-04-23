//! Layout detection module — DocLayout-YOLO background worker.
//!
//! Follows the same job-queue pattern as the transcription module:
//! 1. Frontend calls Tauri command → submits job to mpsc channel → returns "queued"
//! 2. Worker thread drains jobs serially, emits progress/complete/error events
//! 3. Frontend listens to events via `LayoutStore` → updates UI reactively
//! 4. DB stores results in `layouts` table for persistence between sessions

pub mod commands;
pub mod engine;
pub mod reading_order;
pub mod region;

use engine::{DocLayoutEngine, LayoutConfig};
use region::LayoutResult;
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

// ── Public helper ───────────────────────────────────────────────────────────

/// Create a `DocLayoutEngine` for use by other modules (e.g., OCR worker).
///
/// Resolves the script path and Python interpreter, initializes the engine.
/// Returns `None` if layout detection is unavailable (no Python with
/// `doclayout_yolo`, missing script, or engine init failure).
///
/// This is intended to be called during app setup — it probes for Python
/// and may take a few seconds on the first call.
pub fn create_layout_engine(app_handle: &AppHandle) -> Option<DocLayoutEngine> {
    // Resolve script path: try Resource directory first (production), then source (dev).
    // CRITICAL: Tauri's resolve() returns a path but doesn't verify the file exists.
    // In dev mode, new resources may not be copied to target/debug/ yet.
    // We must check existence and fall back to the source path.
    let script_path = {
        let resource_path = app_handle
            .path()
            .resolve("scripts/layout_detect.py", BaseDirectory::Resource)
            .ok();

        // Strip Windows \\?\ prefix if present (same pattern as resolve_paddle_model_dir)
        let clean_resource_path = resource_path.map(|p| {
            let s = p.to_string_lossy().into_owned();
            if s.starts_with(r"\\?\") {
                std::path::PathBuf::from(&s[4..])
            } else {
                p
            }
        });

        // Check if the resource path actually exists on disk
        if let Some(ref path) = clean_resource_path {
            if path.exists() {
                path.clone()
            } else {
                eprintln!("[layout] Resource path does not exist: {}, trying dev fallback", path.display());
                // Dev fallback: CARGO_MANIFEST_DIR/resources/scripts/layout_detect.py
                let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/scripts/layout_detect.py");
                if dev_path.exists() {
                    dev_path
                } else {
                    // Last resort: try the scripts/ directory directly
                    let scripts_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("scripts/layout_detect.py");
                    scripts_path
                }
            }
        } else {
            // resolve() failed entirely — use dev fallback
            let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/scripts/layout_detect.py");
            if dev_path.exists() {
                dev_path
            } else {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/layout_detect.py")
            }
        }
    };

    eprintln!("[layout] Script path: {}", script_path.display());

    // Find Python interpreter with doclayout_yolo
    let python_path = match engine::which_python_for_layout() {
        Some(p) => p,
        None => {
            eprintln!("[layout] No Python with doclayout_yolo found — layout-aware OCR will be unavailable.");
            return None;
        }
    };

    // Resolve model cache directory inside app data
    let model_cache_dir = match app_handle.path().app_data_dir() {
        Ok(dir) => {
            let cache_dir = dir.join("hf_cache_layout");
            std::fs::create_dir_all(&cache_dir).unwrap_or_else(|e| {
                eprintln!("[layout] Warning: could not create model cache dir {}: {e}", cache_dir.display());
            });
            Some(cache_dir)
        }
        Err(e) => {
            eprintln!("[layout] Failed to get app data dir for OCR layout engine: {e}");
            None
        }
    };

    match DocLayoutEngine::init(LayoutConfig {
        python_path,
        script_path,
        model_cache_dir,
    }) {
        Ok(engine) => {
            eprintln!("[layout] ✅ DocLayoutEngine created for OCR worker");
            Some(engine)
        }
        Err(e) => {
            eprintln!("[layout] ❌ Failed to create DocLayoutEngine for OCR worker: {e}");
            None
        }
    }
}

// ── Event payloads ──────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct LayoutProgressPayload {
    pub asset_id: String,
    pub pct: u8,
    pub stage: String,
}

#[derive(Clone, Serialize)]
pub struct LayoutCompletePayload {
    pub asset_id: String,
    pub regions_count: usize,
    pub model: String,
    /// JSON-serialized layout regions for frontend visualization.
    pub regions_json: String,
}

#[derive(Clone, Serialize)]
pub struct LayoutErrorPayload {
    pub asset_id: String,
    pub error: String,
}

// ── Job & Queue ──────────────────────────────────────────────────────────────

/// A single layout detection work unit submitted to the background worker.
pub struct LayoutJob {
    pub asset_id: String,
    pub asset_path: String,
}

/// Handle for submitting jobs to the background layout detection worker.
///
/// Managed as Tauri state — the `extract_layout` command grabs this via
/// `State<LayoutQueue>`.
pub struct LayoutQueue {
    sender: tokio::sync::mpsc::Sender<LayoutJob>,
}

impl LayoutQueue {
    /// Create a new queue and return `(LayoutQueue, Receiver)`.
    pub fn new() -> (Self, tokio::sync::mpsc::Receiver<LayoutJob>) {
        let (sender, receiver) = tokio::sync::mpsc::channel::<LayoutJob>(16);
        (Self { sender }, receiver)
    }

    /// Submit a job to the queue. Returns immediately.
    pub fn submit(&self, job: LayoutJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("Failed to enqueue layout job: {e}"))
    }

    /// Spawn the background worker loop on a dedicated thread.
    ///
    /// Each detection call spawns a Python subprocess, so no persistent
    /// model state is held. The thread just drains the queue and spawns
    /// processes sequentially.
    pub fn start_worker(
        db_path: std::path::PathBuf,
        mut receiver: tokio::sync::mpsc::Receiver<LayoutJob>,
        app_handle: AppHandle,
    ) {
        // Resolve script path: try Resource directory first (production), then source (dev).
        // CRITICAL: Tauri's resolve() returns a path but doesn't verify the file exists.
        // In dev mode, new resources may not be copied to target/debug/ yet.
        // We must check existence and fall back to the source path.
        let script_path = {
            let resource_path = app_handle
                .path()
                .resolve("scripts/layout_detect.py", BaseDirectory::Resource)
                .ok();

            // Strip Windows \\?\ prefix if present (same pattern as resolve_paddle_model_dir)
            let clean_resource_path = resource_path.map(|p| {
                let s = p.to_string_lossy().into_owned();
                if s.starts_with(r"\\?\") {
                    std::path::PathBuf::from(&s[4..])
                } else {
                    p
                }
            });

            // Check if the resource path actually exists on disk
            if let Some(ref path) = clean_resource_path {
                if path.exists() {
                    path.clone()
                } else {
                    eprintln!("[layout] Resource path does not exist: {}, trying dev fallback", path.display());
                    // Dev fallback: CARGO_MANIFEST_DIR/resources/scripts/layout_detect.py
                    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("resources/scripts/layout_detect.py");
                    if dev_path.exists() {
                        dev_path
                    } else {
                        // Last resort: try the scripts/ directory directly
                        let scripts_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("scripts/layout_detect.py");
                        scripts_path
                    }
                }
            } else {
                // resolve() failed entirely — use dev fallback
                let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/scripts/layout_detect.py");
                if dev_path.exists() {
                    dev_path
                } else {
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("scripts/layout_detect.py")
                }
            }
        };

        eprintln!("[layout] Script path: {}", script_path.display());

        // Find Python interpreter with doclayout_yolo
        let python_path = match engine::which_python_for_layout() {
            Some(p) => p,
            None => {
                eprintln!("[layout] No Python with doclayout_yolo found — worker will report errors for all jobs.");
                std::thread::Builder::new()
                    .name("layout-worker".to_string())
                    .stack_size(8 * 1024 * 1024)
                    .spawn(move || {
                        while let Some(job) = receiver.blocking_recv() {
                            let _ = app_handle.emit(
                                "layout:error",
                                LayoutErrorPayload {
                                    asset_id: job.asset_id,
                                    error: "No Python interpreter with doclayout_yolo found. Please install Python and run: pip install doclayout-yolo".to_string(),
                                },
                            );
                        }
                    })
                    .expect("Failed to spawn layout worker thread (no-python fallback)");
                return;
            }
        };

        // Resolve model cache directory inside app data (avoids HuggingFace symlink
        // issues on Windows — same pattern as transcription module)
        let model_cache_dir = app_handle
            .path()
            .app_data_dir()
            .expect("Failed to get app data dir for model cache")
            .join("hf_cache_layout");
        std::fs::create_dir_all(&model_cache_dir).unwrap_or_else(|e| {
            eprintln!(
                "[layout] Warning: could not create model cache dir {}: {e}",
                model_cache_dir.display()
            );
        });
        eprintln!(
            "[layout] Model cache dir: {}",
            model_cache_dir.display()
        );

        std::thread::Builder::new()
            .name("layout-worker".to_string())
            .stack_size(8 * 1024 * 1024) // 8 MB — subprocess only, no heavy stack needed
            .spawn(move || {
                let engine = DocLayoutEngine::init(LayoutConfig {
                    python_path: python_path,
                    script_path,
                    model_cache_dir: Some(model_cache_dir),
                });

                let engine = match engine {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("[layout] Failed to initialize layout engine: {e}");
                        while let Some(job) = receiver.blocking_recv() {
                            let _ = app_handle.emit(
                                "layout:error",
                                LayoutErrorPayload {
                                    asset_id: job.asset_id,
                                    error: format!("Layout engine initialization failed: {e}"),
                                },
                            );
                        }
                        return;
                    }
                };

                eprintln!("[layout] Worker ready, processing jobs.");

                // ── Open dedicated DB connection ─────────────────────────────
                let conn = match rusqlite::Connection::open(&db_path) {
                    Ok(c) => {
                        let _ = c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;");
                        c
                    }
                    Err(e) => {
                        eprintln!("[layout] Failed to open DB connection: {e}");
                        while let Some(job) = receiver.blocking_recv() {
                            let _ = app_handle.emit(
                                "layout:error",
                                LayoutErrorPayload {
                                    asset_id: job.asset_id,
                                    error: format!("DB connection failed: {e}"),
                                },
                            );
                        }
                        return;
                    }
                };

                // ── Main work loop ───────────────────────────────────────────
                while let Some(job) = receiver.blocking_recv() {
                    let asset_id = job.asset_id.clone();
                    let result = process_job(&engine, &conn, &job, &app_handle);

                    match result {
                        Ok(layout_result) => {
                            let regions_json = serde_json::to_string(&layout_result.regions)
                                .unwrap_or_else(|_| "[]".to_string());
                            let _ = app_handle.emit(
                                "layout:complete",
                                LayoutCompletePayload {
                                    asset_id: asset_id.clone(),
                                    regions_count: layout_result.regions.len(),
                                    model: layout_result.model.clone(),
                                    regions_json,
                                },
                            );
                        }
                        Err(err) => {
                            eprintln!("[layout] Error for {asset_id}: {err}");
                            let _ = app_handle.emit(
                                "layout:error",
                                LayoutErrorPayload {
                                    asset_id,
                                    error: err,
                                },
                            );
                        }
                    }
                }
            })
            .expect("Failed to spawn layout worker thread");
    }
}

// ── Persistence ──────────────────────────────────────────────────────────────

/// Save a layout detection result to the database.
fn save_layout(
    conn: &rusqlite::Connection,
    asset_id: &str,
    result: &LayoutResult,
    model_name: &str,
) -> Result<(), String> {
    // Serialize regions as JSON
    let regions_json = serde_json::to_string(&result.regions)
        .map_err(|e| format!("Failed to serialize layout regions: {e}"))?;

    // Delete existing layout for this asset (upsert semantics)
    conn.execute("DELETE FROM layouts WHERE asset_id = ?1", [asset_id])
        .map_err(|e| format!("Failed to delete existing layout: {e}"))?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO layouts(id, asset_id, regions, model, image_width, image_height, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            asset_id,
            regions_json,
            model_name,
            result.image_width as i64,
            result.image_height as i64,
            now,
        ],
    )
    .map_err(|e| format!("Failed to insert layout: {e}"))?;

    Ok(())
}

/// Look up a previously saved layout result for an asset.
///
/// Returns the most recent layout detection result for the given `asset_id`,
/// or `None` if no layout has been detected for that asset yet.
pub fn lookup_layout_for_asset(
    conn: &rusqlite::Connection,
    asset_id: &str,
) -> Option<LayoutResult> {
    let mut stmt = conn
        .prepare(
            "SELECT regions, model, image_width, image_height FROM layouts WHERE asset_id = ?1 ORDER BY created_at DESC LIMIT 1",
        )
        .ok()?;

    let result = stmt.query_row([asset_id], |row| {
        let regions_json: String = row.get(0)?;
        let model: String = row.get(1)?;
        let image_width: i64 = row.get(2)?;
        let image_height: i64 = row.get(3)?;
        Ok((regions_json, model, image_width, image_height))
    });

    let (regions_json, model, image_width, image_height) = result.ok()?;
    let regions: Vec<crate::layout::region::LayoutRegion> =
        serde_json::from_str(&regions_json).ok()?;

    Some(LayoutResult {
        regions,
        model,
        image_width: image_width as u32,
        image_height: image_height as u32,
    })
}

// ── Job Processing ────────────────────────────────────────────────────────────

/// Process a single layout detection job.
fn process_job(
    engine: &DocLayoutEngine,
    conn: &rusqlite::Connection,
    job: &LayoutJob,
    app_handle: &AppHandle,
) -> Result<LayoutResult, String> {
    emit_progress(app_handle, &job.asset_id, 10, "reading");

    eprintln!("[layout] Detecting layout for: {}", job.asset_path);
    emit_progress(app_handle, &job.asset_id, 30, "detecting");

    let mut layout_result = engine.detect(&job.asset_path)?;

    emit_progress(app_handle, &job.asset_id, 60, "ordering");

    // Compute reading order
    reading_order::compute_reading_order(&mut layout_result.regions, layout_result.image_width);

    emit_progress(app_handle, &job.asset_id, 80, "saving");

    // Persist to SQLite
    save_layout(conn, &job.asset_id, &layout_result, &layout_result.model)?;

    emit_progress(app_handle, &job.asset_id, 100, "done");

    Ok(layout_result)
}

/// Emit a `layout:progress` event to the frontend.
fn emit_progress(app_handle: &AppHandle, asset_id: &str, pct: u8, stage: &str) {
    let _ = app_handle.emit(
        "layout:progress",
        LayoutProgressPayload {
            asset_id: asset_id.to_string(),
            pct,
            stage: stage.to_string(),
        },
    );
}