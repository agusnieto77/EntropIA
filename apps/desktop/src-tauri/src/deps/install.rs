//! Venv creation and package installation for the dependency manager.
//!
//! Uses the managed uv binary to create an isolated Python 3.11 venv and
//! install each registered dependency into it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::io::AsyncBufReadExt as _;
use tokio::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use super::{DepCheckResult, DependencyId, DependencyStatus, DepsState};
use crate::deps::checks::probe_one;
use crate::deps::registry::{all_deps, find_dep, DependencySpec};
use crate::deps::uv::{self, UvBinary};

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
        .args(["venv", &venv_str, "--python", "3.11"])
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
        "python.embed.path",
        "python.paddle_vl.path",
        "python.transcription.path",
        "python.spacy_ner.path",
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
/// - `SpacyModelEs` (no pip_spec): `python -m spacy download es_core_news_sm`
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
    match dep.id {
        DependencyId::Python => {
            // Python itself is managed by `uv venv` — nothing to install.
            return Ok(());
        }
        DependencyId::SpacyModelEs => {
            // spaCy model is downloaded via `python -m spacy download`.
            let mut cmd = Command::new(venv_python);
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.args(["-m", "spacy", "download", "es_core_news_sm"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            return run_and_stream(&mut cmd, dep.display_name, on_output).await;
        }
        _ => {
            let spec = dep
                .pip_spec
                .ok_or_else(|| format!("Sin pip_spec para {}", dep.display_name))?;

            let python_str = venv_python.to_string_lossy().into_owned();
            let mut cmd = uv.command();
            cmd.args(["pip", "install", spec, "--python", &python_str])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            return run_and_stream(&mut cmd, dep.display_name, on_output).await;
        }
    }
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
    // ── 1. Ensure uv ────────────────────────────────────────────────────────
    let uv = ensure_uv(app, app_data_dir).await?;

    // ── 2. Create venv & update Python status ───────────────────────────────
    {
        let mut map = state.0.lock().await;
        map.insert(
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
                map.insert(DependencyId::Python, status.clone());
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
                map.insert(DependencyId::Python, status.clone());
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

    for dep in all_deps() {
        if dep.id == DependencyId::Python {
            continue; // Already handled above.
        }

        // Mark as installing.
        let installing = DependencyStatus::Installing { percent: 0 };
        {
            let mut map = state.0.lock().await;
            map.insert(dep.id.clone(), installing.clone());
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
            map.insert(dep.id.clone(), final_status.clone());
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
    if *id == DependencyId::Python {
        return Err(
            "Python es gestionado por uv, no se puede instalar individualmente".to_string(),
        );
    }

    // Pre-flight: uv must already be present.
    let uv = uv::UvBinary::detect(app_data_dir)
        .ok_or_else(|| "uv no está disponible. Ejecutá la instalación completa primero.".to_string())?;

    // Pre-flight: venv python must exist.
    let venv_python = venv_python_path(app_data_dir);
    if !venv_python.is_file() {
        return Err(
            "El entorno virtual no existe. Ejecutá la instalación completa primero.".to_string(),
        );
    }

    let dep = find_dep(id)
        .ok_or_else(|| format!("Dependencia desconocida: {id:?}"))?;

    // Emit Installing.
    let installing = DependencyStatus::Installing { percent: 0 };
    {
        let mut map = state.0.lock().await;
        map.insert(id.clone(), installing.clone());
    }
    let _ = app.emit(
        "deps://progress",
        DepsProgressPayload {
            id: id.clone(),
            status: installing,
        },
    );

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

    if let Err(ref msg) = install_result {
        let status = DependencyStatus::Failed(msg.clone());
        {
            let mut map = state.0.lock().await;
            map.insert(id.clone(), status.clone());
        }
        let _ = app.emit(
            "deps://progress",
            DepsProgressPayload {
                id: id.clone(),
                status,
            },
        );
        return Err(msg.clone());
    }

    // Re-probe to get accurate installed status.
    // Read python path from settings if venv path has been persisted; fall
    // back to the path we already know.
    let probe_python = {
        let conn = rusqlite::Connection::open(db_path).ok();
        conn.and_then(|c| crate::deps::checks::resolve_probe_python(&c))
            .unwrap_or(venv_python)
    };

    let probed_status = probe_one(dep, &probe_python).await;

    {
        let mut map = state.0.lock().await;
        map.insert(id.clone(), probed_status.clone());
    }
    let _ = app.emit(
        "deps://progress",
        DepsProgressPayload {
            id: id.clone(),
            status: probed_status.clone(),
        },
    );

    let version = match &probed_status {
        DependencyStatus::Installed { version } => version.clone(),
        _ => None,
    };

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
    if let Some(uv) = uv::UvBinary::detect(app_data_dir) {
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
            "python.embed.path",
            "python.paddle_vl.path",
            "python.transcription.path",
            "python.spacy_ner.path",
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
}
