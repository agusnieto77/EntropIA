//! Sentiment analysis via pysentimiento Python subprocess.
//!
//! Follows the same pattern as transcription/embeddings: discover Python,
//! write input to temp JSON, spawn subprocess, parse sentinel-wrapped output.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::python_discovery::{apply_windows_no_window, which_python_for_module};
use crate::AppDbState;

const BEGIN_SENTINEL: &str = "===SENTIMENT_JSON_BEGIN===";
const END_SENTINEL: &str = "===SENTIMENT_JSON_END===";

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SentimentInput {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentTaskResult {
    pub output: Option<String>,
    pub probas: Option<std::collections::HashMap<String, f64>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentEntryResult {
    pub id: String,
    #[serde(flatten)]
    pub tasks: std::collections::HashMap<String, SentimentTaskResult>,
}

// ── Python discovery ─────────────────────────────────────────────────────────

fn which_python_for_sentiment(settings_db_path: Option<&Path>) -> Option<PathBuf> {
    which_python_for_module(
        "sentiment",
        "pysentimiento",
        "pysentimiento",
        "import pysentimiento; print('ok')",
        settings_db_path,
    )
}

// ── Script path resolution ───────────────────────────────────────────────────

fn resolve_script_path(app_handle: Option<&tauri::AppHandle>) -> Result<PathBuf, String> {
    // 1. Try Tauri resource path (production)
    if let Some(handle) = app_handle {
        use tauri::Manager;
        if let Ok(resource_path) = handle
            .path()
            .resolve("resources/scripts/sentiment.py", tauri::path::BaseDirectory::Resource)
        {
            let path = crate::path_utils::normalize_windows_path(&resource_path);
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // 2. Dev fallback: CARGO_MANIFEST_DIR/resources/scripts/
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let dev_path = PathBuf::from(&manifest_dir)
            .join("resources")
            .join("scripts")
            .join("sentiment.py");
        if dev_path.exists() {
            return Ok(dev_path);
        }

        // 3. Dev fallback: CARGO_MANIFEST_DIR/scripts/
        let dev_path2 = PathBuf::from(&manifest_dir)
            .join("scripts")
            .join("sentiment.py");
        if dev_path2.exists() {
            return Ok(dev_path2);
        }
    }

    Err("sentiment.py not found in any search path".to_string())
}

// ── Core analysis function ───────────────────────────────────────────────────

pub fn analyze_sentiment(
    entries: &[SentimentInput],
    tasks: &[String],
    app_handle: Option<&tauri::AppHandle>,
    settings_db_path: Option<&Path>,
) -> Result<Vec<SentimentEntryResult>, String> {
    let python = which_python_for_sentiment(settings_db_path)
        .ok_or_else(|| "pysentimiento not available — install it from Settings > Dependencies".to_string())?;

    let script = resolve_script_path(app_handle)?;

    // Write input to temp file
    let temp_dir = std::env::temp_dir();
    let input_path = temp_dir.join(format!("entropia_sentiment_{}.json", std::process::id()));
    {
        let mut file = std::fs::File::create(&input_path)
            .map_err(|e| format!("Failed to create temp input file: {e}"))?;
        let json = serde_json::to_vec(entries)
            .map_err(|e| format!("Failed to serialize entries: {e}"))?;
        file.write_all(&json)
            .map_err(|e| format!("Failed to write temp input file: {e}"))?;
    }

    let tasks_str = tasks.join(",");

    let mut cmd = Command::new(&python);
    apply_windows_no_window(&mut cmd);
    cmd.arg(&script)
        .arg("--input")
        .arg(&input_path)
        .arg("--tasks")
        .arg(&tasks_str)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Add cache dir if available
    if let Some(db_path) = settings_db_path {
        if let Some(app_dir) = db_path.parent() {
            let cache_dir = app_dir.join("models");
            cmd.arg("--cache-dir").arg(&cache_dir);
        }
    }

    eprintln!("[sentiment] Running: {} {} --input <temp> --tasks {tasks_str}", python.display(), script.display());

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to spawn sentiment.py: {e}"))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&input_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("sentiment.py failed (exit {}): {stderr}", output.status));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Extract JSON between sentinels
    let begin_pos = stdout
        .find(BEGIN_SENTINEL)
        .ok_or_else(|| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!("No BEGIN sentinel in sentiment.py output. stderr: {stderr}")
        })?;
    let end_pos = stdout
        .find(END_SENTINEL)
        .ok_or_else(|| "No END sentinel in sentiment.py output".to_string())?;

    let json_str = &stdout[begin_pos + BEGIN_SENTINEL.len()..end_pos].trim();

    let results: Vec<SentimentEntryResult> = serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse sentiment results: {e}"))?;

    eprintln!("[sentiment] Analyzed {} entries with tasks: {tasks_str}", results.len());

    Ok(results)
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeSentimentRequest {
    pub entries: Vec<SentimentInput>,
    pub tasks: Vec<String>,
}

#[tauri::command]
pub async fn analyze_entries_sentiment(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppDbState>,
    request: AnalyzeSentimentRequest,
) -> Result<Vec<SentimentEntryResult>, String> {
    let db_path = Some(state.db_path.clone());

    let handle = app_handle.clone();
    let entries = request.entries;
    let tasks = request.tasks;

    // Run in blocking thread to avoid holding async runtime
    tokio::task::spawn_blocking(move || {
        analyze_sentiment(&entries, &tasks, Some(&handle), db_path.as_deref())
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))?
}
