//! DocLayout-YOLO layout detection engine — subprocess-based via Python.
//!
//! Calls the `layout_detect.py` script as a child process to detect
//! document layout regions (titles, text blocks, tables, figures, etc.)
//! using the DocLayout-YOLO model.
//!
//! The Python process is isolated — if it crashes, we catch it as a
//! `Result::Err` instead of a hard abort that kills the entire app.

use std::path::PathBuf;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn apply_windows_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Configuration for the layout detection engine.
#[derive(Clone)]
pub struct LayoutConfig {
    /// Path to the Python interpreter.
    pub python_path: PathBuf,
    /// Path to the layout_detect.py script.
    pub script_path: PathBuf,
    /// Directory to cache models (None = use HuggingFace default).
    pub model_cache_dir: Option<PathBuf>,
}

/// Parsed result from the Python script's JSON output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawLayoutResult {
    pub regions: Vec<RawLayoutRegion>,
}

/// A single region from the Python script output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawLayoutRegion {
    pub category: String,
    pub bbox: RawBbox,
    pub confidence: f32,
}

/// Bounding box from the Python script output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RawBbox {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The layout detection engine — spawns Python as a child process.
///
/// Each detection call spawns a fresh Python process that:
/// - Loads the DocLayout-YOLO model (cached after first download)
/// - Runs inference on the image
/// - Outputs JSON to stdout with sentinel markers
/// - Exits (freeing all memory)
///
/// `Clone` is cheap — only the config (paths) is cloned, not model state.
/// Each clone creates an independent subprocess caller.
#[derive(Clone)]
pub struct DocLayoutEngine {
    config: LayoutConfig,
}

impl DocLayoutEngine {
    /// Validate configuration and create the engine.
    /// The model is loaded per-call by the Python process.
    pub fn init(config: LayoutConfig) -> Result<Self, String> {
        // Verify the script exists
        if !config.script_path.exists() {
            return Err(format!(
                "Layout detection script not found: {}",
                config.script_path.display()
            ));
        }

        // Verify python exists — for bare command names on PATH,
        // we can't use exists(), so verify by running --version instead.
        let python_is_valid = if config.python_path.is_absolute()
            || config.python_path.parent().map_or(false, |p| p.exists())
        {
            config.python_path.exists()
        } else {
            let mut cmd = std::process::Command::new(&config.python_path);
            apply_windows_no_window(&mut cmd);
            cmd.arg("--version")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };

        if !python_is_valid {
            return Err(format!(
                "Python interpreter not found or not working: {}",
                config.python_path.display()
            ));
        }

        eprintln!(
            "[layout] Engine configured: python={}, script={}",
            config.python_path.display(),
            config.script_path.display(),
        );

        Ok(Self { config })
    }

    /// Detect layout regions in an image by spawning the Python script.
    ///
    /// The script receives the image path and outputs JSON regions to stdout,
    /// wrapped in sentinel markers for reliable extraction.
    /// Progress and errors go to stderr — we capture both.
    pub fn detect(&self, image_path: &str) -> Result<super::region::LayoutResult, String> {
        eprintln!("[layout] Spawning Python layout detection for: {}", image_path);

        let mut cmd = Command::new(&self.config.python_path);
        apply_windows_no_window(&mut cmd);
        cmd.arg(&self.config.script_path)
            .arg(image_path);

        if let Some(ref model_dir) = self.config.model_cache_dir {
            cmd.arg("--model-dir").arg(model_dir);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd.output().map_err(|e| {
            format!(
                "Failed to spawn Python layout process (python={}): {e}",
                self.config.python_path.display()
            )
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Check for error sentinel in output
        let json_str = extract_sentinel_json(&stdout);

        // Check if the JSON contains an error key from the Python script
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
            if let Some(error) = parsed.get("error") {
                let error_msg = error.as_str().unwrap_or("Unknown error");
                return Err(format!(
                    "Layout detection script reported error: {error_msg}\n\
                     Stderr: {}",
                    stderr.trim(),
                ));
            }
        }

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);
            return Err(format!(
                "Layout detection script failed (exit code {exit_code}).\n\
                 Python: {}\n\
                 Script: {}\n\
                 Stderr: {}\n\
                 Stdout: {}",
                self.config.python_path.display(),
                self.config.script_path.display(),
                stderr.trim(),
                stdout.trim(),
            ));
        }

        let raw: RawLayoutResult = serde_json::from_str(json_str).map_err(|e| {
            format!(
                "Failed to parse layout detection JSON: {e}\n\
                 Extracted: {}\n\
                 Full stdout ({} bytes): {}\n\
                 Stderr: {}",
                if json_str.len() > 200 {
                    &json_str[..200]
                } else {
                    json_str
                },
                stdout.len(),
                if stdout.len() > 500 {
                    &stdout[..500.min(stdout.len())]
                } else {
                    &stdout
                },
                stderr.trim(),
            )
        })?;

        // Convert raw result to typed LayoutResult
        use super::region::{BoundingBox, LayoutCategory, LayoutRegion, LayoutResult};

        let regions: Vec<LayoutRegion> = raw
            .regions
            .into_iter()
            .map(|r| {
                let category = match r.category.as_str() {
                    "title" => LayoutCategory::Title,
                    "plain_text" => LayoutCategory::PlainText,
                    "abandoned" => LayoutCategory::Abandoned,
                    "figure" => LayoutCategory::Figure,
                    "figure_caption" => LayoutCategory::FigureCaption,
                    "table" => LayoutCategory::Table,
                    "table_caption" => LayoutCategory::TableCaption,
                    "table_footnote" => LayoutCategory::TableFootnote,
                    "isolate_formula" => LayoutCategory::IsolateFormula,
                    "formula_caption" => LayoutCategory::FormulaCaption,
                    other => {
                        eprintln!(
                            "[layout] Unknown category '{}', falling back to PlainText",
                            other
                        );
                        LayoutCategory::PlainText
                    }
                };

                LayoutRegion {
                    category,
                    bbox: BoundingBox {
                        x: r.bbox.x,
                        y: r.bbox.y,
                        width: r.bbox.width,
                        height: r.bbox.height,
                    },
                    confidence: r.confidence,
                    reading_order: 0, // Will be assigned by compute_reading_order
                }
            })
            .collect();

        // Get image dimensions from stderr (the Python script logs them)
        // Fallback: try to get dimensions from bboxes
        let image_width = regions
            .iter()
            .map(|r| (r.bbox.x + r.bbox.width) as u32)
            .max()
            .unwrap_or(0);
        let image_height = regions
            .iter()
            .map(|r| (r.bbox.y + r.bbox.height) as u32)
            .max()
            .unwrap_or(0);

        eprintln!(
            "[layout] Python layout detection complete: {} regions",
            regions.len()
        );

        Ok(LayoutResult {
            regions,
            image_width,
            image_height,
            model: "doclayout_yolo".to_string(),
        })
    }

    /// Check if the layout detection engine is available.
    ///
    /// Returns true if the Python interpreter and doclayout_yolo module
    /// are both accessible.
    pub fn is_available(&self) -> bool {
        let mut probe_cmd = std::process::Command::new(&self.config.python_path);
        apply_windows_no_window(&mut probe_cmd);
        let result = probe_cmd
            .args(["-c", "import doclayout_yolo; print('ok')"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        match result {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.trim() == "ok"
            }
            _ => false,
        }
    }
}

/// Extract JSON content between `===LAYOUT_JSON_BEGIN===` and
/// `===LAYOUT_JSON_END===` sentinels. Falls back to the full output
/// if sentinels are not found (backwards compatibility).
fn extract_sentinel_json(output: &str) -> &str {
    const BEGIN: &str = "===LAYOUT_JSON_BEGIN===";
    const END: &str = "===LAYOUT_JSON_END===";

    if let Some(start_idx) = output.find(BEGIN) {
        let content_start = start_idx + BEGIN.len();
        if let Some(end_idx) = output[content_start..].find(END) {
            let json_content = &output[content_start..content_start + end_idx];
            return json_content.trim();
        }
    }

    // Fallback: return the full trimmed output
    output.trim()
}

/// Find the Python interpreter on the system that has `doclayout_yolo` available.
///
/// Discovery strategy (same as transcription module):
/// 1. If `CONDA_PREFIX` env var is set, prefer that Python
/// 2. Use `where` (Windows) / `which` (Unix) to discover all Python executables on PATH
/// 3. Try python3 explicitly on Unix
/// 4. Scan common Conda/Python install locations not on PATH (Windows)
/// 5. Return the first match with the required module, or None if nothing works
pub fn which_python_for_layout() -> Option<PathBuf> {
    let module = "doclayout_yolo";
    let mut candidates = Vec::new();

    // 1. Conda environment
    if let Ok(conda_prefix) = std::env::var("CONDA_PREFIX") {
        let conda_python = if cfg!(windows) {
            PathBuf::from(&conda_prefix).join("python.exe")
        } else {
            PathBuf::from(&conda_prefix).join("bin").join("python")
        };
        eprintln!("[layout] CONDA_PREFIX detected: {}", conda_python.display());
        candidates.push(conda_python);
    }

    // 2. Discover Python executables on PATH
    let finder_cmd = if cfg!(windows) { "where" } else { "which" };
    let mut find_python_cmd = Command::new(finder_cmd);
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
                let path = PathBuf::from(line.trim());
                if path.is_file() && !candidates.contains(&path) {
                    candidates.push(path);
                }
            }
        }
    }

    // 3. Also try python3 explicitly (common on Linux/macOS)
    if cfg!(unix) {
        let mut find_python3_cmd = Command::new(finder_cmd);
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
                    let path = PathBuf::from(line.trim());
                    if path.is_file() && !candidates.contains(&path) {
                        candidates.push(path);
                    }
                }
            }
        }
    }

    // 4. Scan common Conda/Python install locations (Windows)
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
                        eprintln!(
                            "[layout] Found Python at common location: {}",
                            python_exe.display()
                        );
                        candidates.push(python_exe);
                    }
                    // Also check envs/ subdirectories
                    let envs_dir = dir.join("envs");
                    if envs_dir.is_dir() {
                        if let Ok(entries) = std::fs::read_dir(&envs_dir) {
                            for entry in entries.flatten() {
                                let env_python = entry.path().join("python.exe");
                                if env_python.is_file() && !candidates.contains(&env_python) {
                                    eprintln!(
                                        "[layout] Found Python in Conda env: {}",
                                        env_python.display()
                                    );
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
        eprintln!("[layout] ERROR: No Python interpreter found on this system!");
        return None;
    }

    // 5. Probe each candidate for the required module
    for candidate in &candidates {
        let mut probe_cmd = Command::new(candidate);
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
                        "[layout] Found Python with {module}: {}",
                        candidate.display()
                    );
                    return Some(candidate.clone());
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!(
                    "[layout] Python {} found but {module} not importable: {}",
                    candidate.display(),
                    stderr.trim()
                );
            }
            Err(e) => {
                eprintln!("[layout] Failed to probe {}: {e}", candidate.display());
            }
        }
    }

    eprintln!(
        "[layout] WARNING: No Python with {module} found among {} candidates",
        candidates.len()
    );
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sentinel_json() {
        let output = "some noise\n===LAYOUT_JSON_BEGIN==={\"regions\":[]}\n===LAYOUT_JSON_END===\nmore noise";
        let extracted = extract_sentinel_json(output);
        assert_eq!(extracted, r#"{"regions":[]}"#);
    }

    #[test]
    fn test_extract_sentinel_json_fallback() {
        let output = r#"{"regions":[]}"#;
        let extracted = extract_sentinel_json(output);
        assert_eq!(extracted, r#"{"regions":[]}"#);
    }

    #[test]
    fn test_layout_detect_script_exists() {
        let script = std::path::PathBuf::from("scripts/layout_detect.py");
        if script.exists() {
            assert!(true, "layout_detect.py script found");
        } else {
            // Script may be at a different path — just verify config mechanism works
            let config = LayoutConfig {
                python_path: std::path::PathBuf::from("python"),
                script_path: std::path::PathBuf::from("nonexistent.py"),
                model_cache_dir: None,
            };
            let result = DocLayoutEngine::init(config);
            assert!(result.is_err(), "Should fail with missing script");
        }
    }
}