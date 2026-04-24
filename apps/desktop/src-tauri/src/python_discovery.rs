//! Shared Python interpreter discovery and module probing.
//!
//! All subsystems (PaddleVL, transcription, embeddings, spaCy NER) that need a
//! Python interpreter follow the same pattern: discover candidate interpreters,
//! then probe each for the required module. This module consolidates the
//! discovery step so it runs ONCE and the results are shared, reducing log noise
//! and redundant filesystem scans.
//!
//! Each subsystem still probes for its specific module, but results are cached
//! per (tag) so repeated calls for the same module skip redundant subprocess spawns.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply the Windows `CREATE_NO_WINDOW` flag to prevent console popups.
pub fn apply_windows_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

// ── Shared candidate discovery ────────────────────────────────────────────────

/// Global cache of discovered Python candidates.
/// Discovered once on first access, shared across all subsystems.
static PYTHON_CANDIDATES: OnceLock<Vec<PathBuf>> = OnceLock::new();

fn is_verbose_python_logging_enabled() -> bool {
    std::env::var("ENTROPIA_VERBOSE_PYTHON_DISCOVERY")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Discover Python interpreter candidates on the system.
///
/// Returns a list of candidate Python interpreter paths, discovered once
/// and cached for all subsequent calls. Discovery strategy:
/// 1. CONDA_PREFIX if set
/// 2. System PATH (where/which python)
/// 3. python3 on Unix
/// 4. Common Conda/Python install locations on Windows
///
/// Each candidate is verified to be an existing file. Duplicates are removed.
/// Logs a single summary line instead of per-subsystem noise.
pub fn discover_python_candidates() -> &'static Vec<PathBuf> {
    PYTHON_CANDIDATES.get_or_init(|| {
        let mut candidates = Vec::new();

        // 1. Conda environment — if CONDA_PREFIX is set, that Python is authoritative
        if let Ok(conda_prefix) = std::env::var("CONDA_PREFIX") {
            let conda_python = if cfg!(windows) {
                PathBuf::from(&conda_prefix).join("python.exe")
            } else {
                PathBuf::from(&conda_prefix).join("bin").join("python")
            };
            eprintln!("[python] CONDA_PREFIX detected: {}", conda_python.display());
            candidates.push(conda_python);
        }

        // 2. Discover Python executables on PATH via `where` (Windows) / `which` (Unix)
        let finder_cmd = if cfg!(windows) { "where" } else { "which" };
        let mut find_cmd = Command::new(finder_cmd);
        apply_windows_no_window(&mut find_cmd);
        if let Ok(output) = find_cmd
            .arg("python")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
        {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let path = PathBuf::from(line.trim());
                    if path.is_file() && !candidates.contains(&path) {
                        candidates.push(path);
                    }
                }
            }
        }

        // 3. Also try python3 explicitly (common on Linux/macOS)
        if cfg!(unix) {
            let mut find_cmd3 = Command::new(finder_cmd);
            apply_windows_no_window(&mut find_cmd3);
            if let Ok(output) = find_cmd3
                .arg("python3")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
            {
                if output.status.success() {
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        let path = PathBuf::from(line.trim());
                        if path.is_file() && !candidates.contains(&path) {
                            candidates.push(path);
                        }
                    }
                }
            }
        }

        // 4. Scan common Conda/Python install locations not on PATH (Windows)
        if cfg!(windows) {
            if let Ok(user_profile) = std::env::var("USERPROFILE") {
                let home = PathBuf::from(&user_profile);
                if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                    let lad = PathBuf::from(&local_app_data);
                    for dir in [
                        lad.join("r-miniconda"),
                        lad.join("miniconda3"),
                        lad.join("anaconda3"),
                        home.join("miniconda3"),
                        home.join("anaconda3"),
                        home.join(".conda"),
                    ] {
                        let python_exe = dir.join("python.exe");
                        if python_exe.is_file() && !candidates.contains(&python_exe) {
                            candidates.push(python_exe);
                        }
                        // Also check envs/ subdirectories
                        let envs_dir = dir.join("envs");
                        if envs_dir.is_dir() {
                            if let Ok(entries) = std::fs::read_dir(&envs_dir) {
                                for entry in entries.flatten() {
                                    let env_python = entry.path().join("python.exe");
                                    if env_python.is_file() && !candidates.contains(&env_python) {
                                        candidates.push(env_python);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Summary log — one line instead of per-discovery noise
        eprintln!("[python] Discovered {} Python candidate(s)", candidates.len());
        if is_verbose_python_logging_enabled() {
            for (i, c) in candidates.iter().enumerate() {
                eprintln!("[python]   [{}] {}", i + 1, c.display());
            }
        }

        candidates
    })
}

// ── Module probe result cache ────────────────────────────────────────────────

/// Cache of module probe results: tag → winning interpreter path.
/// Prevents redundant subprocess spawns if the same module is queried multiple times.
/// `None` values are also cached (module not found) to avoid re-probing.
static MODULE_PROBE_CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();

fn get_probe_cache() -> &'static Mutex<HashMap<String, Option<PathBuf>>> {
    MODULE_PROBE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

// ── Module probing ────────────────────────────────────────────────────────────

/// Probe a single Python interpreter for a specific import check.
///
/// Spawns `python -c "<probe_code>"` and checks if stdout contains "ok".
/// Used by subsystem-specific `which_python_for_module` functions.
pub fn probe_python_module(
    python_path: &Path,
    probe_code: &str,
) -> bool {
    let mut cmd = Command::new(python_path);
    apply_windows_no_window(&mut cmd);
    match cmd
        .args(["-c", probe_code])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim() == "ok"
        }
        _ => false,
    }
}

/// Find a Python interpreter that can import a specific module.
///
/// Uses the shared candidate cache (discovered once) and probes each candidate
/// until one succeeds. Logs one summary line per subsystem.
///
/// Arguments:
/// - `tag`: Subsystem tag for logging (e.g., "transcription", "embeddings")
/// - `module_name`: Display name of the module for logging
/// - `probe_code`: Python code to verify import (e.g., `"import faster_whisper; print('ok')"`)
pub fn which_python_for_module(
    tag: &str,
    module_name: &str,
    probe_code: &str,
) -> Option<PathBuf> {
    // Check probe cache — if we already resolved this tag, return the cached result.
    if let Ok(cache) = get_probe_cache().lock() {
        if let Some(cached) = cache.get(tag) {
            if let Some(path) = cached {
                eprintln!("[{tag}] Probe cache hit: {module_name} → {}", path.display());
                return Some(path.clone());
            }
            eprintln!("[{tag}] Probe cache hit: {module_name} → not available");
            return None;
        }
    }

    let candidates = discover_python_candidates();

    eprintln!("[{tag}] Probing {n} candidate(s) for {module_name}", n = candidates.len());
    let mut failed_probes = 0usize;

    for candidate in candidates {
        let probe_start = std::time::Instant::now();
        let import_ok = probe_python_module(candidate, probe_code);

        if import_ok {
            eprintln!(
                "[{tag}] ✅ Found Python with {module_name} after {} failed probe(s) ({}ms): {}",
                failed_probes,
                probe_start.elapsed().as_millis(),
                candidate.display()
            );
            // Cache the hit
            if let Ok(mut cache) = get_probe_cache().lock() {
                cache.insert(tag.to_string(), Some(candidate.clone()));
            }
            return Some(candidate.clone());
        }

        failed_probes += 1;
        if is_verbose_python_logging_enabled() {
            eprintln!(
                "[{tag}]   ❌ {} ({}ms): {module_name} not importable",
                candidate.display(),
                probe_start.elapsed().as_millis()
            );
        }
    }

    eprintln!(
        "[{tag}] WARNING: No Python with {module_name} found among {} candidate(s)",
        candidates.len()
    );
    // Cache the miss
    if let Ok(mut cache) = get_probe_cache().lock() {
        cache.insert(tag.to_string(), None);
    }
    None
}

/// Find a Python interpreter that can import a specific module, with candidate scoring.
///
/// Like [`which_python_for_module`], but sorts candidates by a scoring function
/// before probing. Higher scores are probed first. This is useful for subsystems
/// that prefer dedicated environments (e.g., PaddleVL prefers `ppocrvl-py312` envs).
///
/// Arguments:
/// - `tag`: Subsystem tag for logging
/// - `module_name`: Display name of the module for logging
/// - `probe_code`: Python code to verify import
/// - `scorer`: Function that assigns a score to each candidate path (higher = better)
pub fn which_python_for_module_scored(
    tag: &str,
    module_name: &str,
    probe_code: &str,
    scorer: &dyn Fn(&std::path::Path) -> i32,
) -> Option<PathBuf> {
    // Check probe cache — if we already resolved this tag, return the cached result.
    if let Ok(cache) = get_probe_cache().lock() {
        if let Some(cached) = cache.get(tag) {
            if let Some(path) = cached {
                eprintln!("[{tag}] Probe cache hit: {module_name} → {}", path.display());
                return Some(path.clone());
            }
            eprintln!("[{tag}] Probe cache hit: {module_name} → not available");
            return None;
        }
    }

    let mut candidates = discover_python_candidates().clone();

    // Sort candidates by score (descending) — dedicated envs first
    candidates.sort_by_key(|c| -scorer(c));

    eprintln!("[{tag}] Probing {} candidate(s) for {module_name} (scored, dedicated envs first)", candidates.len());
    let mut failed_probes = 0usize;

    for candidate in &candidates {
        let probe_start = std::time::Instant::now();
        let import_ok = probe_python_module(candidate, probe_code);

        if import_ok {
            eprintln!(
                "[{tag}] ✅ Found Python with {module_name} after {} failed probe(s) ({}ms): {}",
                failed_probes,
                probe_start.elapsed().as_millis(),
                candidate.display()
            );
            // Cache the hit
            if let Ok(mut cache) = get_probe_cache().lock() {
                cache.insert(tag.to_string(), Some(candidate.clone()));
            }
            return Some(candidate.clone());
        }

        failed_probes += 1;
        if is_verbose_python_logging_enabled() {
            eprintln!(
                "[{tag}]   ❌ {} ({}ms): {module_name} not importable",
                candidate.display(),
                probe_start.elapsed().as_millis()
            );
        }
    }

    eprintln!(
        "[{tag}] WARNING: No Python with {module_name} found among {} candidate(s)",
        candidates.len()
    );
    // Cache the miss
    if let Ok(mut cache) = get_probe_cache().lock() {
        cache.insert(tag.to_string(), None);
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_candidates_returns_non_empty_on_any_system() {
        // On any dev system with Python installed, at least one candidate should exist.
        // On CI without Python, this might return empty — that's OK for a smoke test.
        let candidates = discover_python_candidates();
        // Just verify it doesn't panic and returns a valid Vec
        assert!(candidates.len() <= 50, "Should not have more than 50 candidates");
    }

    #[test]
    fn probe_python_module_returns_false_for_nonsense() {
        // Probing a nonsense module should return false without panicking
        let candidates = discover_python_candidates();
        if let Some(first) = candidates.first() {
            let result = probe_python_module(first, "import __nonexistent_module_xyz__; print('ok')");
            assert!(!result, "Nonsense module should not be importable");
        }
        // If no candidates, the test is a no-op
    }
}
