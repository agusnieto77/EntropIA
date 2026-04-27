//! LLM Vision Sidecar — spawns a separate process for mmproj inference.
//!
//! The sidecar (`llm-sidecar`) loads Gemma + mmproj in an isolated process.
//! This avoids STATUS_STACK_BUFFER_OVERRUN caused by a conflict between mtmd
//! and other native libraries (pdfium, onnxruntime, tesseract) in the Tauri
//! process. The sidecar communicates via JSON on stdin/stdout with sentinel
//! markers, following the same pattern as transcribe.py and paddle_vl.py.

use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::io::{BufRead, BufReader, Write as IoWrite};

use serde::{Deserialize, Serialize};

// ── Sentinel markers (must match sidecar) ──────────────────────────────────

const JSON_BEGIN: &str = "===LLM_JSON_BEGIN===";
const JSON_END: &str = "===LLM_JSON_END===";
const INPUT_MARKER: &str = ">>>LLM<<<";

// ── Response type from sidecar ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SidecarResponse {
    pub status: String,
    pub result: Option<String>,
    pub error: Option<String>,
}

// ── Command type sent to sidecar ────────────────────────────────────────────

#[derive(Serialize)]
#[serde(tag = "cmd")]
enum LlmCmd {
    #[serde(rename = "generate")]
    Generate { prompt: String, max_tokens: i32 },
    #[serde(rename = "generate_with_image")]
    GenerateWithImage { image_path: String, prompt: String, max_tokens: i32 },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "shutdown")]
    Shutdown,
}

// ── Sidecar manager ────────────────────────────────────────────────────────

pub struct SidecarManager {
    /// Path to the llm-sidecar binary.
    sidecar_path: PathBuf,
    /// Path to the Gemma model file.
    model_path: PathBuf,
    /// Path to the mmproj file (if available).
    mmproj_path: Option<PathBuf>,
    /// Whether the sidecar process is currently alive.
    alive: Arc<AtomicBool>,
}

impl SidecarManager {
    /// Create a new sidecar manager. The sidecar is NOT started until
    /// `start()` is called.
    pub fn new(sidecar_path: PathBuf, model_path: PathBuf, mmproj_path: Option<PathBuf>) -> Self {
        Self {
            sidecar_path,
            model_path,
            mmproj_path,
            alive: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether the sidecar process is currently alive.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Start the sidecar process and wait for it to signal readiness.
    pub fn start(&self) -> Result<SidecarHandle, String> {
        if !self.sidecar_path.exists() {
            return Err(format!(
                "Sidecar binary not found: {}",
                self.sidecar_path.display()
            ));
        }

        let mut cmd = std::process::Command::new(&self.sidecar_path);
        cmd.arg(&self.model_path);
        if let Some(ref mmproj) = self.mmproj_path {
            cmd.arg(mmproj);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        eprintln!("[sidecar] Spawning: {:?}", cmd);

        let mut child = cmd.spawn()
            .map_err(|e| format!("Failed to spawn sidecar: {e}"))?;

        let stdout = child.stdout.take()
            .ok_or("Failed to get sidecar stdout")?;
        let mut reader = BufReader::new(stdout);

        // Wait for readiness signal (pong)
        let response = read_sidecar_response(&mut reader)
            .map_err(|e| format!("Sidecar readiness check failed: {e}"))?;

        if response.status != "pong" && response.status != "ok" {
            let _ = child.kill();
            return Err(format!("Sidecar unexpected response: {:?}", response));
        }

        self.alive.store(true, Ordering::Relaxed);
        eprintln!("[sidecar] Process started and ready");

        Ok(SidecarHandle {
            child,
            reader,
            alive: self.alive.clone(),
        })
    }
}

// ── Sidecar handle (owns the child process) ────────────────────────────────

pub struct SidecarHandle {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
    alive: Arc<AtomicBool>,
}

impl SidecarHandle {
    /// Send a generate command and wait for the response.
    pub fn generate(&mut self, prompt: &str, max_tokens: i32) -> Result<String, String> {
        self.send_command(LlmCmd::Generate { prompt: prompt.into(), max_tokens })?;
        let response = self.read_response()?;
        match response.status.as_str() {
            "ok" => response.result.ok_or_else(|| "Sidecar returned ok but no result".to_string()),
            "error" => Err(response.error.unwrap_or_else(|| "Unknown sidecar error".into())),
            other => Err(format!("Sidecar returned unexpected status: {other}")),
        }
    }

    /// Send a generate_with_image command and wait for the response.
    pub fn generate_with_image(&mut self, image_path: &str, prompt: &str, max_tokens: i32) -> Result<String, String> {
        self.send_command(LlmCmd::GenerateWithImage {
            image_path: image_path.into(),
            prompt: prompt.into(),
            max_tokens,
        })?;
        let response = self.read_response()?;
        match response.status.as_str() {
            "ok" => response.result.ok_or_else(|| "Sidecar returned ok but no result".to_string()),
            "error" => Err(response.error.unwrap_or_else(|| "Unknown sidecar error".into())),
            other => Err(format!("Sidecar returned unexpected status: {other}")),
        }
    }

    /// Send a ping to check if the sidecar is still alive.
    pub fn ping(&mut self) -> Result<bool, String> {
        self.send_command(LlmCmd::Ping)?;
        let response = self.read_response()?;
        Ok(response.status == "pong")
    }

    /// Tell the sidecar to shut down gracefully.
    pub fn shutdown(&mut self) -> Result<(), String> {
        if self.send_command(LlmCmd::Shutdown).is_ok() {
            let _ = self.child.wait();
        } else {
            let _ = self.child.kill();
        }
        self.alive.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn send_command(&mut self, cmd: LlmCmd) -> Result<(), String> {
        let json = serde_json::to_string(&cmd)
            .map_err(|e| format!("JSON serialize: {e}"))?;
        let line = format!("{INPUT_MARKER}{json}\n");
        if let Some(stdin) = self.child.stdin.as_mut() {
            stdin.write_all(line.as_bytes())
                .map_err(|e| format!("Write to sidecar stdin: {e}"))?;
            stdin.flush()
                .map_err(|e| format!("Flush sidecar stdin: {e}"))?;
        } else {
            return Err("Sidecar stdin not available".into());
        }
        Ok(())
    }

    fn read_response(&mut self) -> Result<SidecarResponse, String> {
        read_sidecar_response(&mut self.reader)
    }
}

impl Drop for SidecarHandle {
    fn drop(&mut self) {
        if self.child.stdin.is_some() {
            let _ = self.send_command(LlmCmd::Shutdown);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.alive.store(false, Ordering::Relaxed);
    }
}

// ── IPC helpers ─────────────────────────────────────────────────────────────

fn read_sidecar_response(reader: &mut BufReader<std::process::ChildStdout>) -> Result<SidecarResponse, String> {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return Err("Sidecar process exited (EOF)".into()),
            Ok(_) => {}
            Err(e) => return Err(format!("Sidecar read error: {e}")),
        }

        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        if let Some(json_str) = extract_json(trimmed) {
            match serde_json::from_str::<SidecarResponse>(json_str) {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    eprintln!("[sidecar] Failed to parse response JSON: {e}");
                    eprintln!("[sidecar] Raw: {json_str}");
                    continue;
                }
            }
        }

        // Non-JSON lines are logged but skipped
        if !trimmed.starts_with("===") {
            eprintln!("[sidecar] (stderr) {trimmed}");
        }
    }
}

fn extract_json(line: &str) -> Option<&str> {
    let begin = line.find(JSON_BEGIN)?;
    let after_begin = begin + JSON_BEGIN.len();
    let end = line[after_begin..].find(JSON_END)?;
    Some(&line[after_begin..after_begin + end])
}

// ── Sidecar binary discovery ───────────────────────────────────────────────

/// Find the sidecar binary using a 3-tier resolution strategy:
/// 1. Next to the current executable (production — bundled by Tauri)
/// 2. Via CARGO_MANIFEST_DIR → project root → tools/llm-sidecar/target/{release,debug} (dev)
/// 3. System PATH (fallback)
///
/// Release binaries are preferred over debug (no CRT debug assertions).
/// This mirrors the resolution pattern used for Tesseract tessdata, pdfium,
/// Python scripts, and model files throughout the app.
pub fn find_sidecar_binary() -> Option<PathBuf> {
    // 1. Next to current executable (production/bundled by Tauri)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            #[cfg(target_os = "windows")]
            let sidecar = dir.join("llm-sidecar.exe");
            #[cfg(not(target_os = "windows"))]
            let sidecar = dir.join("llm-sidecar");
            if sidecar.exists() {
                eprintln!("[sidecar] Found binary next to executable: {}", sidecar.display());
                return Some(sidecar);
            }
        }
    }

    // 2. Via CARGO_MANIFEST_DIR → workspace root → tools/llm-sidecar/target/{release,debug}
    //    CARGO_MANIFEST_DIR points to apps/desktop/src-tauri; workspace root is 3 levels up.
    //    Prefer release over debug — release builds don't have CRT debug assertions.
    {
        let mut manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // Navigate up to workspace root:
        // apps/desktop/src-tauri → apps/desktop → apps → <workspace_root>
        for _ in 0..3 {
            if !manifest_dir.pop() {
                break;
            }
        }
        let tools_dir = manifest_dir
            .join("tools")
            .join("llm-sidecar")
            .join("target");

        // Prefer release (no debug CRT assertions)
        for profile in ["release", "debug"] {
            let bin_path = tools_dir.join(profile);
            #[cfg(target_os = "windows")]
            let sidecar = bin_path.join("llm-sidecar.exe");
            #[cfg(not(target_os = "windows"))]
            let sidecar = bin_path.join("llm-sidecar");
            if sidecar.exists() {
                eprintln!("[sidecar] Found binary via CARGO_MANIFEST_DIR ({profile}): {}", sidecar.display());
                return Some(sidecar);
            }
        }
    }

    // 3. System PATH (fallback — if user installed llm-sidecar globally)
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(';') {
            let candidate = PathBuf::from(dir);
            #[cfg(target_os = "windows")]
            let sidecar = candidate.join("llm-sidecar.exe");
            #[cfg(not(target_os = "windows"))]
            let sidecar = candidate.join("llm-sidecar");
            if sidecar.exists() {
                eprintln!("[sidecar] Found binary in PATH: {}", sidecar.display());
                return Some(sidecar);
            }
        }
    }

    eprintln!("[sidecar] Binary not found in any search path");
    None
}