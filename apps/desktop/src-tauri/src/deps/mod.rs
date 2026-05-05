//! Dependency manager for EntropIA.
//!
//! Tracks the status of Python and Python-package dependencies required by the
//! AI pipeline (OCR, embeddings, transcription, NER). Provides probe/check,
//! install, and uv-binary management sub-modules.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
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
#[derive(Debug, Default)]
pub struct DepsStateData {
    pub statuses: HashMap<DependencyId, DependencyStatus>,
    pub cached_probe_python: Option<PathBuf>,
    pub cached_probe_results: Option<HashMap<DependencyId, DependencyStatus>>,
    pub probe_in_flight: bool,
    pub probe_generation: u64,
}

#[derive(Clone, Debug)]
pub struct DepsState(pub Arc<Mutex<DepsStateData>>);

fn default_dependency_statuses() -> HashMap<DependencyId, DependencyStatus> {
    use DependencyId::*;

    let mut map = HashMap::new();
    for id in [Python, Fastembed, PaddleOcr, FasterWhisper, Spacy, SpacyModelEs] {
        map.insert(id, DependencyStatus::Unknown);
    }
    map
}

fn missing_dependency_statuses() -> HashMap<DependencyId, DependencyStatus> {
    registry::all_deps()
        .into_iter()
        .map(|dep| (dep.id.clone(), DependencyStatus::Missing))
        .collect()
}

fn dep_results_from_map(
    results_map: HashMap<DependencyId, DependencyStatus>,
) -> Vec<DepCheckResult> {
    registry::all_deps()
        .iter()
        .filter_map(|dep| {
            results_map.get(&dep.id).cloned().map(|status| {
                let version = match &status {
                    DependencyStatus::Installed { version } => version.clone(),
                    _ => None,
                };
                DepCheckResult {
                    id: dep.id.clone(),
                    status,
                    version,
                }
            })
        })
        .collect()
}

impl DepsState {
    /// Create a new state map with all dependencies initialised to `Unknown`.
    pub fn new() -> Self {
        Self(
            Arc::new(Mutex::new(DepsStateData {
                statuses: default_dependency_statuses(),
                cached_probe_python: None,
                cached_probe_results: None,
                probe_in_flight: false,
                probe_generation: 0,
            })),
        )
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

pub fn should_invalidate_cache_for_setting(key: &str) -> bool {
    matches!(
        key,
        "deps_venv_python_path"
            | "python.fastembed.path"
            | "python.paddle_vl.path"
            | "python.faster_whisper.path"
            | "python.spacy.path"
    )
}

pub async fn invalidate_probe_cache(state: &DepsState) {
    let mut data = state.0.lock().await;
    data.cached_probe_python = None;
    data.cached_probe_results = None;
    data.probe_in_flight = false;
    data.probe_generation = data.probe_generation.saturating_add(1);
    drop(data);
    checks::invalidate_resolved_probe_python_log();
}

pub async fn cache_current_statuses(state: &DepsState, probe_python: Option<PathBuf>) {
    let mut data = state.0.lock().await;
    data.cached_probe_python = probe_python;
    data.cached_probe_results = Some(data.statuses.clone());
    data.probe_in_flight = false;
}

async fn finish_probe_attempt(
    state: &DepsState,
    probe_generation: u64,
    probe_python: Option<PathBuf>,
    results: Option<HashMap<DependencyId, DependencyStatus>>,
) {
    let mut data = state.0.lock().await;
    if data.probe_generation != probe_generation {
        return;
    }
    data.cached_probe_python = probe_python;
    data.cached_probe_results = results.clone();
    data.probe_in_flight = false;
    if let Some(results_map) = results {
        for (id, status) in results_map {
            data.statuses.insert(id, status);
        }
    }
}

pub async fn probe_all_once(
    state: &DepsState,
    db: &crate::db::state::AppDbState,
) -> Result<HashMap<DependencyId, DependencyStatus>, String> {
    loop {
        let probe_generation = {
            let mut data = state.0.lock().await;

            if let Some(results) = &data.cached_probe_results {
                return Ok(results.clone());
            }

            if data.probe_in_flight {
                None
            } else {
                data.probe_in_flight = true;
                for dep in registry::all_deps() {
                    data.statuses
                        .insert(dep.id.clone(), DependencyStatus::Checking);
                }
                Some(data.probe_generation)
            }
        };

        if let Some(probe_generation) = probe_generation {
            let probe_settings = {
                let conn = db
                    .ui_conn
                    .lock()
                    .map_err(|err| format!("DB lock error: {err}"));

                conn.map(|guard| checks::load_probe_python_settings(&guard))
            };

            let probe_settings = if let Ok(settings) = probe_settings {
                settings
            } else {
                finish_probe_attempt(state, probe_generation, None, None).await;
                return Err(
                    probe_settings
                        .err()
                        .unwrap_or_else(|| "DB lock error".to_string()),
                );
            };

            let python_path = checks::resolve_probe_python_async(
                probe_settings,
                checks::ProbePythonMode::DependencyManager,
            )
            .await?;

            let results_map = match python_path.clone() {
                Some(python) => checks::probe_all(&python).await,
                None => missing_dependency_statuses(),
            };

            finish_probe_attempt(state, probe_generation, python_path, Some(results_map.clone()))
                .await;

            return Ok(results_map);
        }

        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

fn all_critical_installed(results: &HashMap<DependencyId, DependencyStatus>) -> bool {
    registry::all_deps()
        .iter()
        .filter(|dep| dep.critical)
        .all(|dep| {
            matches!(
                results.get(&dep.id),
                Some(DependencyStatus::Installed { .. })
            )
        })
}

pub fn emit_probe_complete(
    app: &tauri::AppHandle,
    results: &HashMap<DependencyId, DependencyStatus>,
) -> Result<(), String> {
    let payload = install::DepsCompletePayload {
        results: dep_results_from_map(results.clone()),
        all_critical_installed: all_critical_installed(results),
    };

    app.emit("deps://complete", payload)
        .map_err(|error| format!("Failed to emit dependency completion event: {error}"))
}

/// Probe all registered dependencies and update the shared DepsState.
///
/// - Reads the venv Python path from app_settings via the UI DB connection.
/// - If no Python is available, returns all deps as `Missing`.
/// - Otherwise runs all probes concurrently and updates `DepsState`.
#[tauri::command]
pub async fn deps_check_all(
    app: tauri::AppHandle,
    state: tauri::State<'_, DepsState>,
    db: tauri::State<'_, crate::db::state::AppDbState>,
) -> Result<Vec<DepCheckResult>, String> {
    let results_map = probe_all_once(state.inner(), db.inner()).await?;
    emit_probe_complete(&app, &results_map)?;
    Ok(dep_results_from_map(results_map))
}

#[tauri::command]
pub async fn deps_get_cached_statuses(
    state: tauri::State<'_, DepsState>,
) -> Result<Vec<DepCheckResult>, String> {
    let data = state.0.lock().await;
    Ok(dep_results_from_map(data.statuses.clone()))
}

/// Install all registered dependencies into the managed venv.
///
/// - Ensures the uv binary (bundled/dev/system fallback, downloads only if needed).
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

    let uv_binary = uv::UvBinary::detect(Some(&app), &app_data_dir);
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
    invalidate_probe_cache(state.inner()).await;

    // ── 4. Reset DepsState to all Missing and refresh cache ──────────────────
    {
        use DependencyId::*;
        let mut map = state.0.lock().await;
        for id in [Python, Fastembed, PaddleOcr, FasterWhisper, Spacy, SpacyModelEs] {
            map.statuses.insert(id, DependencyStatus::Missing);
        }
    }
    cache_current_statuses(state.inner(), None).await;

    eprintln!("[deps] Reset complete — all deps marked Missing");
    Ok(())
}
