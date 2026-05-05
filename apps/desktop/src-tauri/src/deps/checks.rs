//! Probe/check system for managed Python dependencies.
//!
//! Each dependency has a short Python one-liner (`probe_code`) that prints
//! `"ok"` when the dependency is importable. This module runs those probes
//! asynchronously and maps the results to `DependencyStatus` values.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use super::{DependencyId, DependencyStatus};
use crate::deps::registry::all_deps;

const PROBE_FASTEMBED: &str = "import fastembed; print('ok')";
const PROBE_PADDLE_VL: &str = "from paddleocr import PaddleOCRVL; print('ok')";
const PROBE_FASTER_WHISPER: &str = "import faster_whisper; print('ok')";
const PROBE_SPACY_ES: &str = "import spacy, es_core_news_sm; print('ok')";
const RUNTIME_PYTHON_KEYS: &[&str] = &[
    "python.fastembed.path",
    "python.paddle_vl.path",
    "python.faster_whisper.path",
    "python.spacy.path",
];

// ---------------------------------------------------------------------------
// Per-dependency probe
// ---------------------------------------------------------------------------

/// Probe a single dependency by running its `probe_code` with `python_path`.
///
/// - Spawns `python_path -c "<probe_code>"` with a 10 s per-probe timeout.
/// - stdout contains `"ok"` → `Installed { version: None }`
/// - Non-zero exit, timeout, or spawn error → `Missing`
pub async fn probe_one(dep: &crate::deps::registry::DependencySpec, python_path: &Path) -> DependencyStatus {
    let mut cmd = Command::new(python_path);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.args(["-c", dep.probe_code])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let probe_result = timeout(Duration::from_secs(10), cmd.output()).await;

    match probe_result {
        Ok(Ok(output)) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.trim().contains("ok") {
                DependencyStatus::Installed { version: None }
            } else {
                DependencyStatus::Missing
            }
        }
        // Non-zero exit or I/O error from spawn
        Ok(_) => DependencyStatus::Missing,
        // Timeout
        Err(_) => {
            eprintln!(
                "[deps/checks] probe timed out for '{}' using {}",
                dep.display_name,
                python_path.display()
            );
            DependencyStatus::Missing
        }
    }
}

// ---------------------------------------------------------------------------
// Probe all dependencies
// ---------------------------------------------------------------------------

/// Probe all registered dependencies concurrently and return a status map.
///
/// - Runs all probes in parallel using `tokio::task::JoinSet`.
/// - Applies a 15 s global timeout over the entire set.
/// - Dependencies that haven't finished when the global timeout fires are
///   marked `Unknown` (not yet checked).
pub async fn probe_all(python_path: &Path) -> HashMap<DependencyId, DependencyStatus> {
    let deps = all_deps();
    let python_path = python_path.to_path_buf();

    // Spawn one task per dependency.
    let mut join_set: tokio::task::JoinSet<(DependencyId, DependencyStatus)> =
        tokio::task::JoinSet::new();

    for dep in deps {
        // SAFETY: DependencySpec is &'static so borrowing id/probe_code is fine.
        let id = dep.id.clone();
        let probe_code = dep.probe_code;
        let display_name = dep.display_name;
        let python = python_path.clone();

        join_set.spawn(async move {
            let mut cmd = Command::new(&python);
            #[cfg(windows)]
            {
                cmd.creation_flags(CREATE_NO_WINDOW);
            }
            cmd.args(["-c", probe_code])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());

            let result = timeout(Duration::from_secs(10), cmd.output()).await;
            let status = match result {
                Ok(Ok(output)) if output.status.success() => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if stdout.trim().contains("ok") {
                        DependencyStatus::Installed { version: None }
                    } else {
                        DependencyStatus::Missing
                    }
                }
                Ok(_) => DependencyStatus::Missing,
                Err(_) => {
                    eprintln!("[deps/checks] probe timed out for '{display_name}'");
                    DependencyStatus::Missing
                }
            };
            (id, status)
        });
    }

    // Collect results with a 15 s global timeout.
    let mut results: HashMap<DependencyId, DependencyStatus> = HashMap::new();

    let collect_all = async {
        while let Some(join_result) = join_set.join_next().await {
            match join_result {
                Ok((id, status)) => {
                    results.insert(id, status);
                }
                Err(e) => {
                    eprintln!("[deps/checks] probe task panicked: {e}");
                }
            }
        }
    };

    match timeout(Duration::from_secs(15), collect_all).await {
        Ok(()) => {}
        Err(_) => {
            eprintln!("[deps/checks] global probe timeout (15 s) — marking remaining deps Unknown");
            // Abort any tasks still running.
            join_set.abort_all();
            // Any dep not yet inserted stays Unknown (default for missing keys).
        }
    }

    // Ensure every registered dep has an entry — default to Unknown if we
    // didn't get a result (e.g. was still in flight when timeout hit).
    for dep in all_deps() {
        results
            .entry(dep.id.clone())
            .or_insert(DependencyStatus::Unknown);
    }

    results
}

// ---------------------------------------------------------------------------
// Python path resolution
// ---------------------------------------------------------------------------

/// Resolve the Python interpreter path to use for probing.
///
/// Reads `deps_venv_python_path` from `app_settings` via an open rusqlite
/// connection. If that managed venv path is missing, it falls back to
/// discovering a runtime Python that satisfies the same critical capabilities
/// used by the app at runtime (`fastembed` + `PaddleOCRVL`). Optional
/// capabilities (`faster_whisper`, `spaCy + es_core_news_sm`) are used only to
/// prefer a richer runtime when multiple candidates satisfy the critical set.
pub fn resolve_probe_python(conn: &rusqlite::Connection) -> Option<PathBuf> {
    if let Some(raw) = crate::settings::get_setting(conn, "deps_venv_python_path") {
        let path = PathBuf::from(&raw);
        if path.is_file() {
            return Some(path);
        }
    }

    resolve_runtime_python(conn)
}

fn resolve_runtime_python(conn: &rusqlite::Connection) -> Option<PathBuf> {
    let mut candidates = persisted_runtime_candidates(conn);

    for candidate in crate::python_discovery::discover_python_candidates() {
        if candidate.is_file() && !candidates.contains(candidate) {
            candidates.push(candidate.clone());
        }
    }

    let mut best_match: Option<(PathBuf, usize)> = None;

    for candidate in candidates {
        let capabilities = probe_runtime_capabilities(&candidate);
        if !(capabilities.has_fastembed && capabilities.has_paddle_vl) {
            continue;
        }

        let optional_score = usize::from(capabilities.has_faster_whisper)
            + usize::from(capabilities.has_spacy);

        match &best_match {
            Some((_, best_score)) if *best_score >= optional_score => {}
            _ => {
                best_match = Some((candidate, optional_score));
            }
        }
    }

    if let Some((path, optional_score)) = best_match {
        eprintln!(
            "[deps/checks] Using runtime Python fallback (critical deps OK, optional score={}): {}",
            optional_score,
            path.display()
        );
        return Some(path);
    }

    None
}

fn persisted_runtime_candidates(conn: &rusqlite::Connection) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    for key in RUNTIME_PYTHON_KEYS {
        let Some(raw) = crate::settings::get_setting(conn, key) else {
            continue;
        };

        let path = PathBuf::from(raw);
        if path.is_file() && !candidates.contains(&path) {
            candidates.push(path);
        }
    }

    candidates
}

#[derive(Debug, Clone, Copy, Default)]
struct RuntimeCapabilities {
    has_fastembed: bool,
    has_paddle_vl: bool,
    has_faster_whisper: bool,
    has_spacy: bool,
}

fn probe_runtime_capabilities(path: &Path) -> RuntimeCapabilities {
    RuntimeCapabilities {
        has_fastembed: crate::python_discovery::probe_python_module(path, PROBE_FASTEMBED),
        has_paddle_vl: crate::python_discovery::probe_python_module(path, PROBE_PADDLE_VL),
        has_faster_whisper: crate::python_discovery::probe_python_module(
            path,
            PROBE_FASTER_WHISPER,
        ),
        has_spacy: crate::python_discovery::probe_python_module(path, PROBE_SPACY_ES),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_conn() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory SQLite");
        conn.execute_batch(
            "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .expect("create app_settings");
        conn
    }

    #[test]
    fn test_resolve_probe_python_prefers_existing_managed_path() {
        let conn = in_memory_conn();
        let current_exe = std::env::current_exe().expect("current exe path");
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
            rusqlite::params!["deps_venv_python_path", current_exe.to_string_lossy().as_ref()],
        )
        .expect("insert managed python path");

        let result = resolve_probe_python(&conn);
        assert!(
            result.as_ref() == Some(&current_exe),
            "Expected managed venv path to be preferred when present"
        );
    }

    #[test]
    fn test_resolve_probe_python_with_stale_managed_path_does_not_panic() {
        let conn = in_memory_conn();
        // Insert a path that does not exist on disk
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES ('deps_venv_python_path', '/nonexistent/path/python.exe')",
            [],
        )
        .expect("insert setting");

        let result = resolve_probe_python(&conn);
        assert!(
            result.as_ref().map(|path| path.is_file()).unwrap_or(true),
            "A stale managed path should either fall back to a valid runtime or return None"
        );
    }

    #[test]
    fn test_persisted_runtime_candidates_ignore_missing_and_duplicate_paths() {
        let conn = in_memory_conn();
        let current_exe = std::env::current_exe().expect("current exe path");

        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
            rusqlite::params!["python.fastembed.path", current_exe.to_string_lossy().as_ref()],
        )
        .expect("insert fastembed path");
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
            rusqlite::params!["python.paddle_vl.path", current_exe.to_string_lossy().as_ref()],
        )
        .expect("insert duplicate path");
        conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)",
            rusqlite::params!["python.spacy.path", "/nonexistent/path/python.exe"],
        )
        .expect("insert stale path");

        let candidates = persisted_runtime_candidates(&conn);
        assert_eq!(candidates, vec![current_exe]);
    }
}
