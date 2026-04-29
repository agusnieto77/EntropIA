//! PaddleOCR-VL engine — layout-aware OCR via Python subprocess.
//!
//! Calls the paddle_vl.py script which runs PaddleOCR-VL to perform
//! both layout detection and OCR in a single pass. Returns structured
//! results with text, blocks, and regions.
//!
//! Fallback chain: PaddleVL → Tesseract (if PaddleVL fails or unavailable)

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::Manager;

use crate::path_utils::normalize_windows_path;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Parsed result from the paddle_vl.py script.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // fields deserialized from Python but not all read in Rust
pub struct PaddleVlOutput {
    pub text: String,
    pub method: String,
    pub blocks: Vec<PaddleVlBlock>,
    pub regions: Vec<PaddleVlRegion>,
    pub image_width: u32,
    pub image_height: u32,
}

/// A single block from PaddleOCR-VL with text content.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // fields deserialized from Python but not all read in Rust
pub struct PaddleVlBlock {
    pub label: String,
    pub content: String,
    pub bbox: PaddleVlBbox,
    pub order: i32,
    pub group_id: i32,
}

/// Bounding box in the format returned by paddle_vl.py.
#[derive(Debug, Clone, Deserialize)]
pub struct PaddleVlBbox {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// A layout region from PaddleOCR-VL detection.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // fields deserialized from Python but not all read in Rust
pub struct PaddleVlRegion {
    pub category: String,
    pub bbox: PaddleVlBbox,
    pub confidence: f32,
}

/// Configuration for the PaddleOCR-VL engine.
#[derive(Clone)]
pub struct PaddleVlConfig {
    /// Path to the Python interpreter with paddleocr installed.
    pub python_path: PathBuf,
    /// Path to the paddle_vl.py script.
    pub script_path: PathBuf,
}

/// The PaddleOCR-VL engine — spawns Python as a child process.
///
/// Each call spawns a fresh Python process. No persistent state.
#[derive(Clone)]
pub struct PaddleVlEngine {
    config: PaddleVlConfig,
}

impl PaddleVlEngine {
    /// Validate configuration and create the engine.
    ///
    /// NOTE: Python interpreter was already validated by `which_python_for_paddle_vl()`
    /// which ran `from paddleocr import PaddleOCRVL; print('ok')` successfully.
    /// Redundant verification (e.g., `python --version`) is skipped — the
    /// discovery module already proved the interpreter works.
    pub fn init(config: PaddleVlConfig) -> Result<Self, String> {
        // Verify script exists
        if !config.script_path.exists() {
            return Err(format!(
                "PaddleVL script not found: {}",
                config.script_path.display()
            ));
        }

        eprintln!(
            "[paddle_vl] Engine configured: python={}, script={}",
            config.python_path.display(),
            config.script_path.display(),
        );

        Ok(Self { config })
    }

    /// Maximum time (in seconds) to wait for PaddleVL subprocess to complete.
    ///
    /// PaddleOCR-VL on CPU has very different timing depending on state:
    ///   - First-ever run: model downloads (~150 MB) + cold import + first inference
    ///     can take 5-15 minutes on slow connections / older CPUs.
    ///   - Models cached, cold Python: ~30-60s for import + pipeline init + first inference
    ///   - Subsequent runs (same process): would be ~5-15s, but we spawn fresh each time.
    ///
    /// We give 15 minutes (900s) of headroom. Progress is logged every 30s so the
    /// user knows the subprocess is still alive.
    const PADDLE_VL_TIMEOUT_SECS: u64 = 900;

    /// Interval (in seconds) for logging progress updates while waiting for the subprocess.
    /// At 30s intervals, a 900s timeout produces ~30 progress logs.
    const PROGRESS_LOG_INTERVAL_SECS: u64 = 30;

    /// Run PaddleOCR-VL on an image file.
    ///
    /// Spawns the Python subprocess, passes the image path,
    /// and parses the sentinel-wrapped JSON output.
    /// On timeout (15 minutes), kills the subprocess and returns an error.
    ///
    /// Sets CPU optimization env vars on the subprocess to maximize throughput:
    ///   - OMP/MKL/OPENBLAS_NUM_THREADS: parallelize matrix ops across cores
    ///   - FLAGS_use_mkldnn=1: enable Paddle's oneDNN acceleration
    ///   - HF_HUB_DISABLE_PROGRESS_BARS=1: silence noisy HF download progress
    pub fn detect(&self, image_path: &str) -> Result<PaddleVlOutput, String> {
        eprintln!("[paddle_vl] Spawning PaddleOCR-VL for: {}", image_path);

        // Determine optimal thread count: all logical cores capped at 8.
        // Going beyond 8 typically hurts due to memory bandwidth + scheduler overhead.
        let cpu_threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4)
            .to_string();

        let mut cmd = Command::new(&self.config.python_path);
        #[cfg(windows)]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.arg(&self.config.script_path)
            .arg(image_path)
            // CPU performance tuning — must be set BEFORE the Python process starts
            // because OMP/MKL libraries read these once at import time.
            .env("OMP_NUM_THREADS", &cpu_threads)
            .env("MKL_NUM_THREADS", &cpu_threads)
            .env("OPENBLAS_NUM_THREADS", &cpu_threads)
            .env("FLAGS_use_mkldnn", "1")
            .env("FLAGS_use_avx", "1")
            // Silence HuggingFace progress bars (would pollute stderr/stdout)
            .env("HF_HUB_DISABLE_PROGRESS_BARS", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .env("TRANSFORMERS_OFFLINE", "0") // allow first-run downloads
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        eprintln!("[paddle_vl] CPU threads: {cpu_threads}, MKLDNN+AVX enabled");

        let child = cmd.spawn().map_err(|e| {
            format!(
                "Failed to spawn PaddleVL process (python={}): {e}",
                self.config.python_path.display()
            )
        })?;

        eprintln!(
            "[paddle_vl] Waiting for PaddleVL (timeout: {}s, progress logs every {}s)...",
            Self::PADDLE_VL_TIMEOUT_SECS, Self::PROGRESS_LOG_INTERVAL_SECS
        );

        // Wait for the process with a timeout using polling.
        // try_wait() checks if the child has exited without blocking.
        let timeout = std::time::Duration::from_secs(Self::PADDLE_VL_TIMEOUT_SECS);
        let start = std::time::Instant::now();
        let check_interval = std::time::Duration::from_millis(500);
        let mut last_progress_log = std::time::Instant::now();

        let mut child = child;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    // Process exited — read captured stdout/stderr from the pipes.
                    // After try_wait returns Some, the process has exited but we still
                    // need to read the output. Use wait() to reap and get output.
                    // Actually, try_wait doesn't consume stdout/stderr pipes.
                    // We need to manually read from the pipes before calling wait().
                    // The simplest approach: use wait_with_output after try_wait already returned.
                    // But wait_with_output doesn't exist. Instead, we read the pipes manually.

                    // Since process has exited, all data should be available on the pipes.
                    let mut stdout_buf = Vec::new();
                    let mut stderr_buf = Vec::new();
                    if let Some(mut out) = child.stdout.take() {
                        use std::io::Read;
                        let _ = out.read_to_end(&mut stdout_buf);
                    }
                    if let Some(mut err) = child.stderr.take() {
                        use std::io::Read;
                        let _ = err.read_to_end(&mut stderr_buf);
                    }
                    // Reap the process
                    let _ = child.wait();

                    let stdout = String::from_utf8_lossy(&stdout_buf);
                    let stderr = String::from_utf8_lossy(&stderr_buf);

                    if !status.success() {
                        let exit_code = status.code().unwrap_or(-1);
                        return Err(format!(
                            "PaddleVL script failed (exit code {exit_code}).\n\
                             Python: {}\n\
                             Script: {}\n\
                             Stderr: {}\n\
                             Stdout: {}",
                            self.config.python_path.display(),
                            self.config.script_path.display(),
                            if stderr.len() > 500 { &stderr[..500] } else { &stderr },
                            if stdout.len() > 500 { &stdout[..500] } else { &stdout },
                        ));
                    }

                    // Extract JSON between sentinels
                    let json_str = extract_sentinel_json(&stdout);

                    // Check for error key in JSON
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(error) = parsed.get("error") {
                            let error_msg = error.as_str().unwrap_or("Unknown error");
                            return Err(format!("PaddleVL script reported error: {error_msg}"));
                        }
                    }

                    let result: PaddleVlOutput = serde_json::from_str(json_str).map_err(|e| {
                        let preview = if json_str.len() > 300 {
                            &json_str[..300]
                        } else {
                            json_str
                        };
                        format!(
                            "Failed to parse PaddleVL JSON: {e}\n\
                             Extracted: {preview}\n\
                             Stderr: {}",
                            if stderr.len() > 500 { &stderr[..500] } else { &stderr }
                        )
                    })?;

                    eprintln!(
                        "[paddle_vl] Complete: {} blocks, {} regions (took {:.1}s)",
                        result.blocks.len(),
                        result.regions.len(),
                        start.elapsed().as_secs_f64()
                    );

                    return Ok(result);
                }
                Ok(None) => {
                    // Process still running — check timeout
                    if start.elapsed() > timeout {
                        eprintln!(
                            "[paddle_vl] ⏰ TIMEOUT after {}s, killing PaddleVL process",
                            start.elapsed().as_secs()
                        );
                        let _ = child.kill();
                        let _ = child.wait(); // reap the process
                        return Err(format!(
                            "PaddleVL timed out after {}s. The model may still be downloading or your CPU is heavily loaded — try again later.",
                            start.elapsed().as_secs()
                        ));
                    }

                    // Periodic progress log so the user knows the subprocess is alive
                    if last_progress_log.elapsed().as_secs() >= Self::PROGRESS_LOG_INTERVAL_SECS {
                        eprintln!(
                            "[paddle_vl] ⏳ Still running... {}s elapsed (timeout at {}s)",
                            start.elapsed().as_secs(),
                            Self::PADDLE_VL_TIMEOUT_SECS
                        );
                        last_progress_log = std::time::Instant::now();
                    }

                    std::thread::sleep(check_interval);
                }
                Err(e) => {
                    return Err(format!("Failed to check PaddleVL process status: {e}"));
                }
            }
        }
    }

    /// Check if the PaddleVL engine is available.
    #[allow(dead_code)] // kept for potential future use
    pub fn is_available(&self) -> bool {
        let mut probe_cmd = Command::new(&self.config.python_path);
        #[cfg(windows)]
        {
            probe_cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let result = probe_cmd
            .args(["-c", "from paddleocr import PaddleOCRVL; print('ok')"])
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

/// Extract JSON content between `===VL_JSON_BEGIN===` and `===VL_JSON_END===`
/// sentinels. Falls back to full output if sentinels not found.
fn extract_sentinel_json(output: &str) -> &str {
    const BEGIN: &str = "===VL_JSON_BEGIN===";
    const END: &str = "===VL_JSON_END===";

    if let Some(start_idx) = output.find(BEGIN) {
        let content_start = start_idx + BEGIN.len();
        if let Some(end_idx) = output[content_start..].find(END) {
            let json_content = &output[content_start..content_start + end_idx];
            return json_content.trim();
        }
    }

    output.trim()
}

/// Score a Python candidate by how likely it is to be a dedicated PaddleOCR-VL env.
///
/// Higher score = better candidate. Used to prioritize purpose-built envs (e.g.
/// `ppocrvl-py312`, `paddle2`) over the base Conda interpreter, which tends to
/// be slower because it has more packages loaded into the import path.
fn score_python_candidate(path: &Path) -> i32 {
    let path_str = path.to_string_lossy().to_lowercase();
    let mut score = 0;

    // Strong signals: name contains paddle/ocr/vl/pp keywords
    if path_str.contains("ppocrvl") { score += 100; }
    if path_str.contains("paddle") { score += 50; }
    if path_str.contains("pp3") || path_str.contains("ppv") { score += 30; }
    if path_str.contains("ocr") { score += 20; }

    // Bonus for being in an envs/ subdirectory (dedicated env)
    if path_str.contains("\\envs\\") || path_str.contains("/envs/") {
        score += 25;
    }

    // Penalty for being the base Conda Python (no envs/ in path, root of conda dir)
    // These tend to have many unrelated packages and slower imports.
    if !path_str.contains("\\envs\\") && !path_str.contains("/envs/")
        && (path_str.contains("miniconda") || path_str.contains("anaconda"))
    {
        score -= 10;
    }

    score
}

/// Find the Python interpreter on the system that has `PaddleOCRVL` available.
///
/// Uses the shared Python candidate cache to avoid redundant filesystem scans.
/// Probes candidates sorted by their likelihood of being a dedicated PaddleOCR-VL
/// environment (scored by path heuristics).
///
/// CRITICAL: The probe verifies `PaddleOCRVL` specifically, not just `paddleocr`.
/// The `paddleocr` package can be installed without the `[doc-parser]` extra,
/// in which case `PaddleOCRVL` is missing and the subprocess would crash later.
pub fn which_python_for_paddle_vl() -> Option<PathBuf> {
    crate::python_discovery::which_python_for_module_scored(
        "paddle_vl",
        "PaddleOCRVL",
        "from paddleocr import PaddleOCRVL; print('ok')",
        &score_python_candidate,
    )
}

/// Create a PaddleVlEngine for use by the OCR worker.
///
/// Resolves the script path and Python interpreter, initializes the engine.
/// Returns None if PaddleVL is unavailable (no Python with paddleocr, or missing script).
pub fn create_paddle_vl_engine(app_handle: &tauri::AppHandle) -> Option<PaddleVlEngine> {
    // Resolve script path: try Resource directory first (production), then source (dev).
    // CRITICAL: Tauri's resolve() returns a path but doesn't verify the file exists.
    let script_path = {
        let resource_path: Option<std::path::PathBuf> = app_handle
            .path()
            .resolve("scripts/paddle_vl.py", tauri::path::BaseDirectory::Resource)
            .ok();

        // Strip Windows \\?\ prefix if present
        let clean_resource_path = resource_path.map(normalize_windows_path);

        // Check if the resource path actually exists on disk
        if let Some(ref path) = clean_resource_path {
            if path.exists() {
                path.clone()
            } else {
                eprintln!("[paddle_vl] Resource path does not exist: {}, trying dev fallback", path.display());
                let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/scripts/paddle_vl.py");
                if dev_path.exists() {
                    normalize_windows_path(dev_path)
                } else {
                    normalize_windows_path(
                        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("scripts/paddle_vl.py"),
                    )
                }
            }
        } else {
            let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources/scripts/paddle_vl.py");
            if dev_path.exists() {
                normalize_windows_path(dev_path)
            } else {
                normalize_windows_path(
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("scripts/paddle_vl.py"),
                )
            }
        }
    };

    // Find Python interpreter with paddleocr
    let python_path = match which_python_for_paddle_vl() {
        Some(p) => p,
        None => {
            eprintln!("[paddle_vl] No Python with paddleocr found — PaddleVL OCR will be unavailable.");
            return None;
        }
    };

    match PaddleVlEngine::init(PaddleVlConfig {
        python_path,
        script_path,
    }) {
        Ok(engine) => Some(engine),
        Err(e) => {
            eprintln!("[paddle_vl] ❌ Failed to create PaddleVLEngine: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sentinel_json() {
        let output = "some noise\n===VL_JSON_BEGIN==={\"text\":\"hello\"}\n===VL_JSON_END===\nmore noise";
        let extracted = extract_sentinel_json(output);
        assert_eq!(extracted, r#"{"text":"hello"}"#);
    }

    #[test]
    fn test_extract_sentinel_json_fallback() {
        let output = r#"{"text":"hello"}"#;
        let extracted = extract_sentinel_json(output);
        assert_eq!(extracted, r#"{"text":"hello"}"#);
    }
}
