//! Venv creation and package installation for the dependency manager.
//!
//! Uses the managed uv binary to create an isolated Python 3.11 venv and
//! install each registered dependency into it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::io::AsyncBufReadExt as _;
use tokio::process::Command;

use super::{DepCheckResult, DependencyId, DependencyStatus, DepsState};
use crate::deps::checks::{probe_one, ProbePythonMode};
use crate::deps::registry::{all_deps_in_install_order, find_dep, DependencySpec};
use crate::deps::uv::{self, UvBinary};

#[cfg(test)]
const SPACY_MODEL_ES_VERSION: &str = "3.8.0";
const SPACY_MODEL_ES_WHEEL_URL: &str =
    "https://github.com/explosion/spacy-models/releases/download/es_core_news_sm-3.8.0/es_core_news_sm-3.8.0-py3-none-any.whl";

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the directory where the managed venv lives.
///
/// Example: `<app_data_dir>/venv/entropia-env`
pub fn venv_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("venv").join("entropia-env")
}

/// Returns the path to the Python interpreter inside the managed venv.
///
/// Example: `<app_data_dir>/venv/entropia-env/Scripts/python.exe`
pub fn venv_python_path(app_data_dir: &Path) -> PathBuf {
    venv_path(app_data_dir).join("Scripts").join("python.exe")
}

// ---------------------------------------------------------------------------
// Venv creation
// ---------------------------------------------------------------------------

/// Create the managed venv using `uv venv <venv_path> --python 3.11`.
///
/// Returns the path to the venv's `python.exe`. If the venv already exists
/// (the python interpreter file is present) this is a no-op.
pub async fn create_venv(uv: &UvBinary, app_data_dir: &Path) -> Result<PathBuf, String> {
    let python_path = venv_python_path(app_data_dir);

    // Already exists — nothing to do.
    if python_path.is_file() {
        return Ok(python_path);
    }

    let venv = venv_path(app_data_dir);
    let venv_str = venv.to_string_lossy().into_owned();

    let output = uv
        .command()
        .args(["venv", &venv_str, "--python", "3.11", "--seed"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Error creando entorno virtual: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Error creando entorno virtual: {stderr}"));
    }

    if !python_path.is_file() {
        return Err(
            "Error creando entorno virtual: python.exe no encontrado después de uv venv"
                .to_string(),
        );
    }

    Ok(python_path)
}

// ---------------------------------------------------------------------------
// Persist venv paths to app_settings
// ---------------------------------------------------------------------------

/// Write all Python-path settings into `app_settings` so that every subsystem
/// (embeddings, OCR, transcription, NER) can find the managed interpreter.
pub fn persist_venv_paths(
    conn: &rusqlite::Connection,
    python_path: &Path,
) -> Result<(), String> {
    let path_str = python_path.to_string_lossy();

    let keys = [
        "deps_venv_python_path",
        "python.fastembed.path",
        "python.paddle_vl.path",
        "python.faster_whisper.path",
        "python.spacy.path",
    ];

    for key in keys {
        crate::settings::set_setting(conn, key, &path_str)
            .map_err(|e| format!("Error guardando ruta Python en configuración ({key}): {e}"))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Install a single package
// ---------------------------------------------------------------------------

/// Install one dependency into the managed venv.
///
/// - Deps with `pip_spec`: `uv pip install <spec> --python <venv_python>`
/// - `SpacyModelEs`: `uv pip install <exact-wheel-url> --python <venv_python>`
/// - `Python` (no pip_spec, managed by uv): immediate `Ok(())`
///
/// Streams stderr line-by-line, calling `on_output(line)` for each line.
/// On non-zero exit returns `Err` with the last few stderr lines.
pub async fn install_package(
    uv: &UvBinary,
    dep: &DependencySpec,
    venv_python: &Path,
    on_output: impl Fn(&str) + Send + 'static,
) -> Result<(), String> {
    if dep.id == DependencyId::Python {
        // Python itself is managed by `uv venv` — nothing to install.
        return Ok(());
    }

    let spec = managed_install_spec(dep)
        .ok_or_else(|| format!("Sin spec de instalación para {}", dep.display_name))?;

    let python_str = venv_python.to_string_lossy().into_owned();
    let mut cmd = uv.command();
    cmd.args(["pip", "install", spec, "--python", &python_str])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    run_and_stream(&mut cmd, dep.display_name, on_output).await
}

fn managed_install_spec(dep: &DependencySpec) -> Option<&'static str> {
    match dep.id {
        DependencyId::Python => None,
        // Install the exact wheel version compatible with our managed spaCy 3.8 flow.
        DependencyId::SpacyModelEs => Some(SPACY_MODEL_ES_WHEEL_URL),
        _ => dep.pip_spec,
    }
}

async fn ensure_managed_prerequisites_installed(
    uv: &UvBinary,
    dep: &DependencySpec,
    venv_python: &Path,
) -> Result<(), String> {
    for prerequisite_id in dep.managed_prerequisites {
        let prerequisite = find_dep(prerequisite_id)
            .ok_or_else(|| format!("Prerequisito desconocido para {}: {prerequisite_id:?}", dep.display_name))?;

        let prerequisite_status = probe_one(prerequisite, venv_python).await;
        if matches!(prerequisite_status, DependencyStatus::Installed { .. }) {
            continue;
        }

        let display_name = prerequisite.display_name;
        install_package(uv, prerequisite, venv_python, move |line| {
            eprintln!("[deps/install] [{display_name}] {line}");
        })
        .await?;

        let post_install_status = probe_one(prerequisite, venv_python).await;
        if !matches!(post_install_status, DependencyStatus::Installed { .. }) {
            return Err(format!(
                "No se pudo confirmar el prerequisito {} dentro del venv administrado",
                prerequisite.display_name
            ));
        }
    }

    Ok(())
}

async fn update_dependency_status(
    app: &tauri::AppHandle,
    state: &DepsState,
    id: &DependencyId,
    status: DependencyStatus,
) {
    {
        let mut map = state.0.lock().await;
        map.statuses.insert(id.clone(), status.clone());
    }

    let _ = app.emit(
        "deps://progress",
        DepsProgressPayload {
            id: id.clone(),
            status,
        },
    );
}

fn managed_install_plan(dep: &'static DependencySpec) -> Vec<&'static DependencySpec> {
    let mut plan = Vec::new();
    let mut seen = std::collections::HashSet::new();
    collect_managed_install_plan(dep, &mut seen, &mut plan);
    plan
}

fn collect_managed_install_plan(
    dep: &'static DependencySpec,
    seen: &mut std::collections::HashSet<DependencyId>,
    plan: &mut Vec<&'static DependencySpec>,
) {
    if !seen.insert(dep.id.clone()) {
        return;
    }

    for prerequisite_id in dep.managed_prerequisites {
        if let Some(prerequisite) = find_dep(prerequisite_id) {
            collect_managed_install_plan(prerequisite, seen, plan);
        }
    }

    plan.push(dep);
}

/// Helper: spawn `cmd`, stream stderr lines via `on_output`, return `Err` on
/// non-zero exit with the last few lines of stderr as the message.
async fn run_and_stream(
    cmd: &mut Command,
    display_name: &str,
    on_output: impl Fn(&str) + Send + 'static,
) -> Result<(), String> {
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Error iniciando instalación de {display_name}: {e}"))?;

    // Collect stderr lines for error reporting.
    let mut last_lines: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    const TAIL: usize = 10;

    if let Some(stderr) = child.stderr.take() {
        let mut reader = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            on_output(&line);
            if last_lines.len() >= TAIL {
                last_lines.pop_front();
            }
            last_lines.push_back(line);
        }
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Error esperando proceso de {display_name}: {e}"))?;

    if !status.success() {
        let tail = last_lines
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("Error instalando {display_name}: {tail}"));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Event payloads
// ---------------------------------------------------------------------------

/// Emitted on `deps://progress` after each dep status change.
#[derive(Clone, Serialize, Deserialize)]
pub struct DepsProgressPayload {
    pub id: DependencyId,
    pub status: DependencyStatus,
}

/// Emitted on `deps://uv_progress` during uv binary download.
#[derive(Clone, Serialize, Deserialize)]
pub struct DepsUvProgressPayload {
    pub percent: u8,
    pub message: String,
}

/// Emitted on `deps://complete` when the full install run finishes.
#[derive(Clone, Serialize, Deserialize)]
pub struct DepsCompletePayload {
    pub results: Vec<DepCheckResult>,
    pub all_critical_installed: bool,
}

// ---------------------------------------------------------------------------
// Install all dependencies
// ---------------------------------------------------------------------------

/// Orchestrate a full dependency install run.
///
/// 1. Ensure the uv binary (detect → download if missing).
/// 2. Create the venv (idempotent).
/// 3. Persist venv paths in app_settings.
/// 4. Loop over `all_deps()` in registry order, skipping Python (handled by
///    uv venv). Install each, emit `deps://progress` events, continue on
///    failure.
/// 5. Emit `deps://complete`.
///
/// Always returns `Ok(())` — partial failures are reported via events.
pub async fn install_all(
    app: &tauri::AppHandle,
    state: &DepsState,
    db_path: &Path,
    app_data_dir: &Path,
) -> Result<(), String> {
    super::invalidate_probe_cache(state).await;

    // ── 1. Ensure uv ────────────────────────────────────────────────────────
    let uv = ensure_uv(app, app_data_dir).await?;

    // ── 2. Create venv & update Python status ───────────────────────────────
    {
        let mut map = state.0.lock().await;
        map.statuses.insert(
            DependencyId::Python,
            DependencyStatus::Installing { percent: 0 },
        );
    }
    let _ = app.emit(
        "deps://progress",
        DepsProgressPayload {
            id: DependencyId::Python,
            status: DependencyStatus::Installing { percent: 0 },
        },
    );

    let venv_python = match create_venv(&uv, app_data_dir).await {
        Ok(p) => {
            let status = DependencyStatus::Installed { version: Some("3.11".to_string()) };
            {
                let mut map = state.0.lock().await;
                map.statuses.insert(DependencyId::Python, status.clone());
            }
            let _ = app.emit(
                "deps://progress",
                DepsProgressPayload {
                    id: DependencyId::Python,
                    status,
                },
            );
            p
        }
        Err(e) => {
            let status = DependencyStatus::Failed(e.clone());
            {
                let mut map = state.0.lock().await;
                map.statuses.insert(DependencyId::Python, status.clone());
            }
            let _ = app.emit(
                "deps://progress",
                DepsProgressPayload {
                    id: DependencyId::Python,
                    status,
                },
            );
            return Err(e);
        }
    };

    // ── 3. Persist venv paths ────────────────────────────────────────────────
    {
        let conn = rusqlite::Connection::open(db_path)
            .map_err(|e| format!("Error abriendo base de datos para settings: {e}"))?;
        persist_venv_paths(&conn, &venv_python)
            .map_err(|e| format!("Error guardando rutas de venv: {e}"))?;
    }

    // ── 4. Install each package ──────────────────────────────────────────────
    let mut results: Vec<DepCheckResult> = Vec::new();

    // Add Python result.
    results.push(DepCheckResult {
        id: DependencyId::Python,
        status: DependencyStatus::Installed { version: Some("3.11".to_string()) },
        version: Some("3.11".to_string()),
    });

    for dep in all_deps_in_install_order() {
        if dep.id == DependencyId::Python {
            continue; // Already handled above.
        }

        // Mark as installing.
        let installing = DependencyStatus::Installing { percent: 0 };
        {
            let mut map = state.0.lock().await;
            map.statuses.insert(dep.id.clone(), installing.clone());
        }
        let _ = app.emit(
            "deps://progress",
            DepsProgressPayload {
                id: dep.id.clone(),
                status: installing,
            },
        );

        // Clone handles for the closure (on_output captures dep.display_name).
        let display_name = dep.display_name;
        let install_result = install_package(
            &uv,
            dep,
            &venv_python,
            move |line| {
                eprintln!("[deps/install] [{display_name}] {line}");
            },
        )
        .await;

        let final_status = match install_result {
            Ok(()) => DependencyStatus::Installed { version: None },
            Err(msg) => {
                eprintln!("[deps/install] failed {}: {msg}", dep.display_name);
                DependencyStatus::Failed(msg)
            }
        };

        {
            let mut map = state.0.lock().await;
            map.statuses.insert(dep.id.clone(), final_status.clone());
        }
        let _ = app.emit(
            "deps://progress",
            DepsProgressPayload {
                id: dep.id.clone(),
                status: final_status.clone(),
            },
        );

        results.push(DepCheckResult {
            id: dep.id.clone(),
            status: final_status,
            version: None,
        });
    }

    // ── 5. Emit complete ─────────────────────────────────────────────────────
    let all_critical_installed = results.iter().all(|r| {
        let dep = find_dep(&r.id);
        let critical = dep.map(|d| d.critical).unwrap_or(false);
        if critical {
            matches!(r.status, DependencyStatus::Installed { .. })
        } else {
            true
        }
    });

    let _ = app.emit(
        "deps://complete",
        DepsCompletePayload {
            results,
            all_critical_installed,
        },
    );

    crate::python_discovery::invalidate_probe_cache();
    super::cache_current_statuses(state, Some(venv_python)).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Install one dependency
// ---------------------------------------------------------------------------

/// Install a single dependency by id.
///
/// - Rejects `DependencyId::Python` (managed by uv).
/// - Pre-flight: ensures uv + venv exist (returns `Err` if not).
/// - Emits `deps://progress` Installing → Installed/Failed.
/// - Re-probes the dep after install and returns the `DepCheckResult`.
pub async fn install_one(
    id: &DependencyId,
    app: &tauri::AppHandle,
    state: &DepsState,
    db_path: &Path,
    app_data_dir: &Path,
) -> Result<DepCheckResult, String> {
    super::invalidate_probe_cache(state).await;

    if *id == DependencyId::Python {
        return Err(
            "Python es gestionado por uv, no se puede instalar individualmente".to_string(),
        );
    }

    // Pre-flight: uv must already be present.
    let uv = uv::UvBinary::detect(Some(app), app_data_dir).ok_or_else(|| {
        "uv no está disponible. Verificá los recursos bundled o ejecutá la instalación completa primero."
            .to_string()
    })?;

    // Ensure the managed venv exists before installing a single dependency.
    let existing_venv_python = venv_python_path(app_data_dir);
    let venv_python = if existing_venv_python.is_file() {
        existing_venv_python
    } else {
        let status = DependencyStatus::Installing { percent: 0 };
        {
            let mut map = state.0.lock().await;
            map.statuses.insert(DependencyId::Python, status.clone());
        }
        let _ = app.emit(
            "deps://progress",
            DepsProgressPayload {
                id: DependencyId::Python,
                status,
            },
        );

        let created = create_venv(&uv, app_data_dir).await?;

        {
            let conn = rusqlite::Connection::open(db_path)
                .map_err(|e| format!("Error abriendo base de datos para settings: {e}"))?;
            persist_venv_paths(&conn, &created)
                .map_err(|e| format!("Error guardando rutas de venv: {e}"))?;
        }

        let status = DependencyStatus::Installed {
            version: Some("3.11".to_string()),
        };
        {
            let mut map = state.0.lock().await;
            map.statuses.insert(DependencyId::Python, status.clone());
        }
        let _ = app.emit(
            "deps://progress",
            DepsProgressPayload {
                id: DependencyId::Python,
                status,
            },
        );

        created
    };

    let dep = find_dep(id)
        .ok_or_else(|| format!("Dependencia desconocida: {id:?}"))?;

    let install_plan = managed_install_plan(dep);

    // Emit Installing.
    let installing = DependencyStatus::Installing { percent: 0 };
    update_dependency_status(app, state, id, installing).await;

    for planned_dep in &install_plan[..install_plan.len().saturating_sub(1)] {
        ensure_managed_prerequisites_installed(&uv, planned_dep, &venv_python).await?;

        let prerequisite_status = probe_one(planned_dep, &venv_python).await;
        if matches!(prerequisite_status, DependencyStatus::Installed { .. }) {
            update_dependency_status(app, state, &planned_dep.id, prerequisite_status).await;
            continue;
        }

        update_dependency_status(
            app,
            state,
            &planned_dep.id,
            DependencyStatus::Installing { percent: 0 },
        )
        .await;

        let display_name = planned_dep.display_name;
        install_package(&uv, planned_dep, &venv_python, move |line| {
            eprintln!("[deps/install] [{display_name}] {line}");
        })
        .await?;

        let verified = probe_one(planned_dep, &venv_python).await;
        if !matches!(verified, DependencyStatus::Installed { .. }) {
            update_dependency_status(
                app,
                state,
                &planned_dep.id,
                DependencyStatus::Failed(format!(
                    "No se pudo confirmar {} dentro del venv administrado después de instalarlo",
                    planned_dep.display_name
                )),
            )
            .await;
            return Err(format!(
                "No se pudo confirmar {} dentro del venv administrado después de instalarlo",
                planned_dep.display_name
            ));
        }

        update_dependency_status(app, state, &planned_dep.id, verified).await;
    }

    let display_name = dep.display_name;
    let install_result = install_package(&uv, dep, &venv_python, move |line| {
        eprintln!("[deps/install] [{display_name}] {line}");
    })
    .await;

    if let Err(ref msg) = install_result {
        let status = DependencyStatus::Failed(msg.clone());
        update_dependency_status(app, state, id, status).await;
        return Err(msg.clone());
    }

    // Re-probe to get accurate installed status.
    // Read python path from settings if venv path has been persisted; fall
    // back to the path we already know.
    let probe_settings = rusqlite::Connection::open(db_path)
        .ok()
        .map(|conn| crate::deps::checks::load_probe_python_settings(&conn))
        .unwrap_or_default();
    let probe_python = crate::deps::checks::resolve_probe_python_async(
        probe_settings,
        ProbePythonMode::DependencyManager,
    )
        .await?
        .unwrap_or(venv_python);

    let probed_status = probe_one(dep, &probe_python).await;

    update_dependency_status(app, state, id, probed_status.clone()).await;

    let version = match &probed_status {
        DependencyStatus::Installed { version } => version.clone(),
        _ => None,
    };

    crate::python_discovery::invalidate_probe_cache();
    super::cache_current_statuses(state, Some(probe_python.clone())).await;

    Ok(DepCheckResult {
        id: id.clone(),
        status: probed_status,
        version,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Ensure a valid uv binary is available: detect it, or download it.
/// Emits `deps://uv_progress` events during download.
async fn ensure_uv(app: &tauri::AppHandle, app_data_dir: &Path) -> Result<UvBinary, String> {
    if let Some(uv) = uv::UvBinary::detect(Some(app), app_data_dir) {
        return Ok(uv);
    }

    let app_clone = app.clone();
    uv::download(app_data_dir, move |percent, message| {
        let _ = app_clone.emit(
            "deps://uv_progress",
            DepsUvProgressPayload {
                percent,
                message: message.to_string(),
            },
        );
    })
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps::registry::find_dep;

    #[test]
    fn test_venv_path_structure() {
        let base = Path::new("/some/app/data");
        let venv = venv_path(base);
        assert!(
            venv.to_string_lossy().contains("entropia-env"),
            "venv path should contain 'entropia-env'"
        );
    }

    #[test]
    fn test_venv_python_path_ends_with_exe() {
        let base = Path::new("/some/app/data");
        let python = venv_python_path(base);
        assert!(
            python.to_string_lossy().ends_with("python.exe"),
            "venv python path should end with 'python.exe'"
        );
        assert!(
            python.to_string_lossy().contains("Scripts"),
            "venv python path should go through Scripts/"
        );
    }

    #[test]
    fn test_persist_venv_paths_writes_all_keys() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory DB");
        conn.execute_batch(
            "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .expect("create table");

        let python_path = Path::new("/fake/venv/Scripts/python.exe");
        persist_venv_paths(&conn, python_path).expect("persist should succeed");

        let keys = [
            "deps_venv_python_path",
            "python.fastembed.path",
            "python.paddle_vl.path",
            "python.faster_whisper.path",
            "python.spacy.path",
        ];
        for key in keys {
            let value: String = conn
                .query_row(
                    "SELECT value FROM app_settings WHERE key = ?1",
                    rusqlite::params![key],
                    |row| row.get(0),
                )
                .expect(&format!("key '{key}' should be present"));
            assert_eq!(
                value,
                python_path.to_string_lossy().as_ref(),
                "key '{key}' should store the python path"
            );
        }
    }

    #[test]
    fn test_managed_install_plan_keeps_spacy_before_model() {
        let spacy_model = find_dep(&DependencyId::SpacyModelEs).expect("spacy model present");
        let plan = managed_install_plan(spacy_model)
            .into_iter()
            .map(|dep| dep.id.clone())
            .collect::<Vec<_>>();

        assert_eq!(plan, vec![DependencyId::Spacy, DependencyId::SpacyModelEs]);
    }

    #[test]
    fn test_managed_install_spec_uses_exact_spacy_model_wheel() {
        let spacy_model = find_dep(&DependencyId::SpacyModelEs).expect("spacy model present");

        assert_eq!(managed_install_spec(spacy_model), Some(SPACY_MODEL_ES_WHEEL_URL));
        assert!(SPACY_MODEL_ES_WHEEL_URL.contains("es_core_news_sm-3.8.0"));
        assert!(SPACY_MODEL_ES_WHEEL_URL.ends_with("-py3-none-any.whl"));
    }

    #[test]
    fn test_managed_install_spec_preserves_regular_pip_specs() {
        let spacy = find_dep(&DependencyId::Spacy).expect("spacy dep present");

        assert_eq!(managed_install_spec(spacy), spacy.pip_spec);
    }

    #[test]
    fn test_spacy_model_version_constant_matches_wheel_url() {
        assert!(SPACY_MODEL_ES_WHEEL_URL.contains(SPACY_MODEL_ES_VERSION));
    }
}
