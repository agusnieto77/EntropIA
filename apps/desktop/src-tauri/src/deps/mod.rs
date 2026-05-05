//! Dependency manager for EntropIA.
//!
//! Tracks the status of Python and Python-package dependencies required by the
//! AI pipeline (OCR, embeddings, transcription, NER). Provides probe/check,
//! install, and uv-binary management sub-modules.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::Manager;
use tokio::sync::Mutex;

pub mod checks;
pub mod install;
pub mod registry;
pub mod uv;

// Re-export checks so lib.rs can access them directly via `deps::checks`.
pub use checks::resolve_probe_python;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Identifies a single managed dependency.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DependencyId {
    Python,
    Fastembed,
    PaddleOcr,
    FasterWhisper,
    Spacy,
    SpacyModelEs,
}

/// The runtime status of a single dependency.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DependencyStatus {
    /// Status has never been checked.
    Unknown,
    /// A probe is currently running.
    Checking,
    /// Dependency is present and (optionally) at a known version.
    Installed {
        version: Option<String>,
    },
    /// Dependency was probed and was not found.
    Missing,
    /// An installation is in progress.
    Installing {
        percent: u8,
    },
    /// The last install attempt failed with this message.
    Failed(String),
}

/// Shared, async-safe map of dependency statuses.
///
/// Wrapped in `Arc<Mutex<…>>` so it can be cloned cheaply and shared between
/// the Tauri command layer and background workers.
#[derive(Clone, Debug)]
pub struct DepsState(pub Arc<Mutex<HashMap<DependencyId, DependencyStatus>>>);

impl DepsState {
    /// Create a new state map with all dependencies initialised to `Unknown`.
    pub fn new() -> Self {
        use DependencyId::*;
        let mut map = HashMap::new();
        for id in [Python, Fastembed, PaddleOcr, FasterWhisper, Spacy, SpacyModelEs] {
            map.insert(id, DependencyStatus::Unknown);
        }
        Self(Arc::new(Mutex::new(map)))
    }
}

impl Default for DepsState {
    fn default() -> Self {
        Self::new()
    }
}

/// The outcome of probing a single dependency.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DepCheckResult {
    pub id: DependencyId,
    pub status: DependencyStatus,
    pub version: Option<String>,
}

/// Result returned by `deps_get_uv_status`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UvStatusResult {
    pub uv_ready: bool,
    pub uv_path: Option<String>,
    pub uv_version: Option<String>,
    pub venv_exists: bool,
    pub venv_path: Option<String>,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Probe all registered dependencies and update the shared DepsState.
///
/// - Reads the venv Python path from app_settings via the UI DB connection.
/// - If no Python is available, returns all deps as `Missing`.
/// - Otherwise runs all probes concurrently and updates `DepsState`.
#[tauri::command]
pub async fn deps_check_all(
    state: tauri::State<'_, DepsState>,
    db: tauri::State<'_, crate::db::state::AppDbState>,
) -> Result<Vec<DepCheckResult>, String> {
    let python_path = {
        let conn = db
            .ui_conn
            .lock()
            .map_err(|e| format!("DB lock error: {e}"))?;
        checks::resolve_probe_python(&conn)
    };

    let results_map = match python_path {
        Some(python) => checks::probe_all(&python).await,
        None => {
            // No venv Python — mark every dep as Missing.
            let mut map = HashMap::new();
            for dep in registry::all_deps() {
                map.insert(dep.id.clone(), DependencyStatus::Missing);
            }
            map
        }
    };

    // Persist results into shared state.
    {
        let mut map = state.0.lock().await;
        for (id, status) in &results_map {
            map.insert(id.clone(), status.clone());
        }
    }

    let results = results_map
        .into_iter()
        .map(|(id, status)| {
            let version = match &status {
                DependencyStatus::Installed { version } => version.clone(),
                _ => None,
            };
            DepCheckResult { id, status, version }
        })
        .collect();

    Ok(results)
}

/// Install all registered dependencies into the managed venv.
///
/// - Ensures the uv binary (downloads if needed).
/// - Creates the venv (idempotent).
/// - Persists venv Python paths in app_settings.
/// - Emits `deps://progress` events per dep, `deps://complete` when done.
#[tauri::command]
pub async fn deps_install_all(
    app: tauri::AppHandle,
    state: tauri::State<'_, DepsState>,
    db: tauri::State<'_, crate::db::state::AppDbState>,
) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Error obteniendo directorio de datos de la app: {e}"))?;
    let db_path = db.db_path.clone();
    install::install_all(&app, &state, &db_path, &app_data_dir).await
}

/// Install a single dependency by id string.
///
/// - The `id` must match a `DependencyId` variant in snake_case (e.g. `"fastembed"`).
/// - Pre-flight: uv and venv must already exist.
/// - Emits `deps://progress` Installing → Installed/Failed.
#[tauri::command]
pub async fn deps_install_one(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, DepsState>,
    db: tauri::State<'_, crate::db::state::AppDbState>,
) -> Result<DepCheckResult, String> {
    // Parse the id string into a DependencyId using serde_json round-trip.
    let dep_id: DependencyId = serde_json::from_value(serde_json::Value::String(id.clone()))
        .map_err(|_| format!("ID de dependencia desconocido: '{id}'"))?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Error obteniendo directorio de datos de la app: {e}"))?;
    let db_path = db.db_path.clone();
    install::install_one(&dep_id, &app, &state, &db_path, &app_data_dir).await
}

/// Return the current status of the managed uv binary and venv.
#[tauri::command]
pub async fn deps_get_uv_status(app: tauri::AppHandle) -> Result<UvStatusResult, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Error obteniendo directorio de datos de la app: {e}"))?;

    let uv_binary = uv::UvBinary::detect(&app_data_dir);
    let uv_ready = uv_binary.is_some();
    let uv_path = uv_binary.as_ref().map(|b| b.path.to_string_lossy().into_owned());
    let uv_version = uv_binary.map(|b| b.version);

    let venv_python = install::venv_python_path(&app_data_dir);
    let venv_exists = venv_python.is_file();
    let venv_path = if venv_exists {
        Some(install::venv_path(&app_data_dir).to_string_lossy().into_owned())
    } else {
        None
    };

    Ok(UvStatusResult {
        uv_ready,
        uv_path,
        uv_version,
        venv_exists,
        venv_path,
    })
}

/// Reset the dependency manager: delete the venv, clear settings, invalidate caches.
///
/// After this, `deps_install_all` must be run again to restore Python functionality.
#[tauri::command]
pub async fn deps_reset(
    state: tauri::State<'_, DepsState>,
    db: tauri::State<'_, crate::db::state::AppDbState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Error obteniendo directorio de datos de la app: {e}"))?;

    // ── 1. Delete the venv directory ─────────────────────────────────────────
    let venv_dir = install::venv_path(&app_data_dir);
    if venv_dir.exists() {
        tokio::fs::remove_dir_all(&venv_dir)
            .await
            .map_err(|e| format!("Error eliminando entorno virtual: {e}"))?;
        eprintln!("[deps] Venv deleted: {}", venv_dir.display());
    }

    // ── 2. Delete Python-path settings from app_settings ─────────────────────
    {
        let conn = db
            .ui_conn
            .lock()
            .map_err(|e| format!("DB lock error: {e}"))?;
        let keys = [
            "deps_venv_python_path",
            "python.fastembed.path",
            "python.paddle_vl.path",
            "python.faster_whisper.path",
            "python.spacy.path",
        ];
        for key in keys {
            crate::settings::delete_setting(&conn, key)
                .map_err(|e| format!("Error eliminando configuración '{key}': {e}"))?;
        }
    }

    // ── 3. Invalidate the Python discovery probe cache ────────────────────────
    crate::python_discovery::invalidate_probe_cache();

    // ── 4. Reset DepsState to all Missing ────────────────────────────────────
    {
        use DependencyId::*;
        let mut map = state.0.lock().await;
        for id in [Python, Fastembed, PaddleOcr, FasterWhisper, Spacy, SpacyModelEs] {
            map.insert(id, DependencyStatus::Missing);
        }
    }

    eprintln!("[deps] Reset complete — all deps marked Missing");
    Ok(())
}
