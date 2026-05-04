//! uv binary management for the dependency manager.
//!
//! uv is the fast Python package installer used to install deps into the
//! managed venv. This module knows how to locate, version-check, and (in a
//! future phase) download the uv binary for the current platform.

use std::path::{Path, PathBuf};

use tokio::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The pinned uv version used by the dependency manager.
pub const UV_VERSION: &str = "0.6.14";

/// Download URL template for the Windows x86_64 uv zip.
pub const UV_DOWNLOAD_URL: &str = concat!(
    "https://github.com/astral-sh/uv/releases/download/",
    "0.6.14",
    "/uv-x86_64-pc-windows-msvc.zip"
);

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A located, version-verified uv binary ready to run.
pub struct UvBinary {
    pub path: PathBuf,
    pub version: String,
}

/// The current availability state of the uv binary.
pub enum UvStatus {
    /// Binary is present and matches the expected version.
    Ready(UvBinary),
    /// Binary not found at the expected path.
    NotFound,
    /// A download is in progress.
    Downloading { percent: u8 },
    /// Download or verification failed.
    Failed(String),
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the directory where the versioned uv binary lives.
///
/// Example: `<app_data_dir>/tools/uv-0.6.14/`
pub fn uv_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir
        .join("tools")
        .join(format!("uv-{UV_VERSION}"))
}

/// Returns the full path to the uv executable.
///
/// Example: `<app_data_dir>/tools/uv-0.6.14/uv.exe`
pub fn uv_exe_path(app_data_dir: &Path) -> PathBuf {
    uv_dir(app_data_dir).join("uv.exe")
}

// ---------------------------------------------------------------------------
// UvBinary impl
// ---------------------------------------------------------------------------

impl UvBinary {
    /// Detect whether a valid, version-matching uv binary exists at the
    /// expected path. Returns `None` if the file is absent, the subprocess
    /// fails, or the version string doesn't match `UV_VERSION`.
    pub fn detect(app_data_dir: &Path) -> Option<UvBinary> {
        let exe = uv_exe_path(app_data_dir);
        if !exe.is_file() {
            return None;
        }

        // Run `uv --version` synchronously — this is called from non-async
        // contexts (startup detection) so we use std::process::Command here.
        let mut cmd = std::process::Command::new(&exe);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as StdCommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let output = cmd
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        // `uv --version` prints e.g. "uv 0.6.14 (abc1234 2025-01-01)"
        let stdout = String::from_utf8_lossy(&output.stdout);
        let version_str = stdout.trim();
        if !version_str.contains(UV_VERSION) {
            eprintln!(
                "[deps/uv] version mismatch: expected {UV_VERSION}, got {version_str:?}"
            );
            return None;
        }

        Some(UvBinary {
            path: exe,
            version: UV_VERSION.to_string(),
        })
    }

    /// Build a tokio `Command` pre-configured with `CREATE_NO_WINDOW` on
    /// Windows. Callers add args before spawning.
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.path);
        #[cfg(windows)]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }
}

// ---------------------------------------------------------------------------
// Download
// ---------------------------------------------------------------------------

/// Download the pinned uv binary for Windows x86_64.
///
/// Steps:
/// 1. Create the versioned uv directory.
/// 2. Stream-download the ZIP from `UV_DOWNLOAD_URL`, reporting progress via
///    `on_progress(percent, message)` roughly every 5 % or every 1 MB.
/// 3. Extract `uv.exe` from the ZIP root and write it to `uv_exe_path`.
/// 4. Verify the installed binary matches `UV_VERSION`.
pub async fn download(
    app_data_dir: &Path,
    on_progress: impl Fn(u8, &str) + Send + 'static,
) -> Result<UvBinary, String> {
    use std::io::{Read as _, Write as _};

    let dir = uv_dir(app_data_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Error creando directorio para uv: {e}"))?;

    // ── 1. Stream download ───────────────────────────────────────────────────
    on_progress(0, "Descargando uv…");

    let mut response = reqwest::get(UV_DOWNLOAD_URL)
        .await
        .map_err(|e| format!("Error descargando uv: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Error descargando uv: respuesta HTTP {}",
            response.status()
        ));
    }

    let content_length: Option<u64> = response.content_length();

    // Write into a temp file inside the uv dir.
    let tmp_zip_path = dir.join("uv-download.zip.tmp");

    {
        let mut file = std::fs::File::create(&tmp_zip_path)
            .map_err(|e| format!("Error creando archivo temporal: {e}"))?;

        let mut downloaded: u64 = 0;
        let mut last_reported_pct: u8 = 0;

        // `response.chunk()` polls the stream one chunk at a time without
        // requiring the `futures-util` crate for `StreamExt`.
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("Error descargando uv: {e}"))?
        {
            file.write_all(&chunk)
                .map_err(|e| format!("Error escribiendo archivo temporal: {e}"))?;
            downloaded += chunk.len() as u64;

            if let Some(total) = content_length {
                let pct = ((downloaded * 100) / total).min(99) as u8;
                // Report every ~5 % or every 1 MB to avoid flooding.
                let mb_boundary = (downloaded / (1024 * 1024))
                    != ((downloaded - chunk.len() as u64) / (1024 * 1024));
                if pct >= last_reported_pct + 5 || mb_boundary {
                    last_reported_pct = pct;
                    on_progress(pct, &format!("Descargando uv… {pct}%"));
                }
            }
        }
    }

    on_progress(90, "Extrayendo uv…");

    // ── 2. Extract uv.exe from ZIP ───────────────────────────────────────────
    let exe_path = uv_exe_path(app_data_dir);

    let extract_result = (|| -> Result<(), String> {
        let zip_file = std::fs::File::open(&tmp_zip_path)
            .map_err(|e| format!("Error abriendo ZIP: {e}"))?;
        let mut archive = zip::ZipArchive::new(zip_file)
            .map_err(|e| format!("Error extrayendo uv: {e}"))?;

        // Find the uv.exe entry — it may be at the root or inside a directory.
        let entry_index = (0..archive.len())
            .find(|&i| {
                archive
                    .by_index(i)
                    .map(|f| {
                        let name = f.name().to_ascii_lowercase();
                        name == "uv.exe" || name.ends_with("/uv.exe")
                    })
                    .unwrap_or(false)
            })
            .ok_or_else(|| "Error extrayendo uv: uv.exe no encontrado en el ZIP".to_string())?;

        let mut entry = archive
            .by_index(entry_index)
            .map_err(|e| format!("Error extrayendo uv: {e}"))?;

        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| format!("Error extrayendo uv: {e}"))?;

        std::fs::write(&exe_path, &buf)
            .map_err(|e| format!("Error extrayendo uv: {e}"))?;

        Ok(())
    })();

    // Always remove the temp file, whether extraction succeeded or not.
    let _ = std::fs::remove_file(&tmp_zip_path);

    extract_result?;

    on_progress(95, "Verificando uv…");

    // ── 3. Verify the installed binary ───────────────────────────────────────
    let binary = UvBinary::detect(app_data_dir).ok_or_else(|| {
        // Binary failed version check — remove it so the next attempt starts fresh.
        let _ = std::fs::remove_file(&exe_path);
        "Versión incorrecta de uv".to_string()
    })?;

    on_progress(100, "uv listo");
    Ok(binary)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uv_exe_path_contains_version() {
        let base = Path::new("/some/app/data");
        let exe = uv_exe_path(base);
        let exe_str = exe.to_string_lossy();
        assert!(
            exe_str.contains(UV_VERSION),
            "uv exe path should contain the version string '{UV_VERSION}', got: {exe_str}"
        );
        assert!(
            exe_str.ends_with("uv.exe"),
            "uv exe path should end with 'uv.exe', got: {exe_str}"
        );
    }

    #[test]
    fn test_uv_dir_is_parent_of_exe() {
        let base = Path::new("/some/app/data");
        let dir = uv_dir(base);
        let exe = uv_exe_path(base);
        assert_eq!(
            exe.parent().unwrap(),
            dir,
            "uv_exe_path parent should equal uv_dir"
        );
    }
}
