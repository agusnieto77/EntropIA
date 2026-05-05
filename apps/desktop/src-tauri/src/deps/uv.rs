//! uv binary management for the dependency manager.
//!
//! uv is the fast Python package installer used to install deps into the
//! managed venv. This module resolves a pinned uv binary from bundled Tauri
//! resources first, then falls back to development resources, a legacy managed
//! copy under app-data, and finally the system `PATH`.

use std::path::{Path, PathBuf};

use tauri::Manager;
use tokio::process::Command;

use crate::path_utils::normalize_windows_path;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The pinned uv version used by the dependency manager.
pub const UV_VERSION: &str = "0.6.14";

const UV_DOWNLOAD_URL_WINDOWS_X86_64: &str = concat!(
    "https://github.com/astral-sh/uv/releases/download/",
    "0.6.14",
    "/uv-x86_64-pc-windows-msvc.zip"
);

const UV_DOWNLOAD_URL_WINDOWS_AARCH64: &str = concat!(
    "https://github.com/astral-sh/uv/releases/download/",
    "0.6.14",
    "/uv-aarch64-pc-windows-msvc.zip"
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowsUvArch {
    X86_64,
    Aarch64,
}

impl WindowsUvArch {
    fn resource_dir(self) -> &'static str {
        match self {
            Self::X86_64 => "windows-x86_64",
            Self::Aarch64 => "windows-aarch64",
        }
    }

    fn download_url(self) -> &'static str {
        match self {
            Self::X86_64 => UV_DOWNLOAD_URL_WINDOWS_X86_64,
            Self::Aarch64 => UV_DOWNLOAD_URL_WINDOWS_AARCH64,
        }
    }
}

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

/// Returns the directory where the versioned uv binary lives.
///
/// Example: `<app_data_dir>/tools/uv-0.6.14/`
pub fn uv_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("tools").join(format!("uv-{UV_VERSION}"))
}

/// Returns the full path to the uv executable.
///
/// Example: `<app_data_dir>/tools/uv-0.6.14/uv.exe`
pub fn uv_exe_path(app_data_dir: &Path) -> PathBuf {
    uv_dir(app_data_dir).join("uv.exe")
}

fn preferred_windows_arches() -> Vec<WindowsUvArch> {
    let mut arches = Vec::new();

    for key in ["PROCESSOR_ARCHITEW6432", "PROCESSOR_ARCHITECTURE"] {
        let Some(value) = std::env::var_os(key) else {
            continue;
        };
        let Some(arch) = parse_windows_arch_value(&value.to_string_lossy()) else {
            continue;
        };
        if !arches.contains(&arch) {
            arches.push(arch);
        }
    }

    #[cfg(target_arch = "aarch64")]
    if !arches.contains(&WindowsUvArch::Aarch64) {
        arches.push(WindowsUvArch::Aarch64);
    }

    #[cfg(target_arch = "x86_64")]
    if !arches.contains(&WindowsUvArch::X86_64) {
        arches.push(WindowsUvArch::X86_64);
    }

    if arches.is_empty() {
        arches.push(WindowsUvArch::X86_64);
    }

    arches
}

fn parse_windows_arch_value(value: &str) -> Option<WindowsUvArch> {
    match value.trim().to_ascii_uppercase().as_str() {
        "AMD64" | "X86_64" | "X64" => Some(WindowsUvArch::X86_64),
        "ARM64" | "AARCH64" => Some(WindowsUvArch::Aarch64),
        _ => None,
    }
}

fn bundled_uv_path(app_handle: &tauri::AppHandle) -> Option<PathBuf> {
    for arch in preferred_windows_arches() {
        let resource_rel = format!("resources/tools/uv/{}/uv.exe", arch.resource_dir());
        let Some(resolved) = app_handle
            .path()
            .resolve(&resource_rel, tauri::path::BaseDirectory::Resource)
            .ok()
            .map(normalize_windows_path)
        else {
            continue;
        };

        if resolved.exists() {
            return Some(resolved);
        }
    }

    None
}

fn dev_uv_path() -> Option<PathBuf> {
    for arch in preferred_windows_arches() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("tools")
            .join("uv")
            .join(arch.resource_dir())
            .join("uv.exe");

        if path.exists() {
            return Some(normalize_windows_path(path));
        }
    }

    None
}

fn resolve_system_uv_path() -> Option<PathBuf> {
    let mut cmd = std::process::Command::new("where.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as StdCommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd
        .arg("uv")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .map(normalize_windows_path)
}

fn version_from_output(output: &std::process::Output) -> Option<String> {
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version_str = stdout.trim();
    if !version_str.contains(UV_VERSION) {
        eprintln!(
            "[deps/uv] version mismatch: expected {UV_VERSION}, got {version_str:?}"
        );
        return None;
    }

    Some(UV_VERSION.to_string())
}

fn probe_uv_command(mut cmd: std::process::Command, path: PathBuf) -> Option<UvBinary> {
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

    let version = version_from_output(&output)?;
    Some(UvBinary { path, version })
}

fn detect_file(exe: &Path) -> Option<UvBinary> {
    if !exe.is_file() {
        return None;
    }

    probe_uv_command(std::process::Command::new(exe), exe.to_path_buf())
}

fn detect_on_path() -> Option<UvBinary> {
    let resolved = resolve_system_uv_path().unwrap_or_else(|| PathBuf::from("uv"));
    let command_path = if resolved.is_file() {
        resolved.clone()
    } else {
        PathBuf::from("uv")
    };
    probe_uv_command(std::process::Command::new(command_path), resolved)
}

// ---------------------------------------------------------------------------
// UvBinary impl
// ---------------------------------------------------------------------------

impl UvBinary {
    /// Detect a valid, version-matching uv binary using the full resolution order:
    /// bundled resource → dev fallback → managed app-data copy → system PATH.
    pub fn detect(app_handle: Option<&tauri::AppHandle>, app_data_dir: &Path) -> Option<UvBinary> {
        app_handle
            .and_then(bundled_uv_path)
            .and_then(|path| detect_file(&path))
            .or_else(|| dev_uv_path().and_then(|path| detect_file(&path)))
            .or_else(|| detect_file(&uv_exe_path(app_data_dir)))
            .or_else(detect_on_path)
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

/// Download the pinned uv binary into the managed app-data tools directory.
///
/// Resolution order elsewhere now prefers bundled resources; download remains a
/// fallback when no bundled/dev/system uv is available.
pub async fn download(
    app_data_dir: &Path,
    on_progress: impl Fn(u8, &str) + Send + 'static,
) -> Result<UvBinary, String> {
    use std::io::{Read as _, Write as _};

    let target_arch = preferred_windows_arches()
        .into_iter()
        .next()
        .unwrap_or(WindowsUvArch::X86_64);

    let dir = uv_dir(app_data_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Error creando directorio para uv: {e}"))?;

    on_progress(0, "Descargando uv…");

    let mut response = reqwest::get(target_arch.download_url())
        .await
        .map_err(|e| format!("Error descargando uv: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Error descargando uv: respuesta HTTP {}",
            response.status()
        ));
    }

    let content_length: Option<u64> = response.content_length();
    let tmp_zip_path = dir.join("uv-download.zip.tmp");

    {
        let mut file = std::fs::File::create(&tmp_zip_path)
            .map_err(|e| format!("Error creando archivo temporal: {e}"))?;

        let mut downloaded: u64 = 0;
        let mut last_reported_pct: u8 = 0;

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

    let exe_path = uv_exe_path(app_data_dir);

    let extract_result = (|| -> Result<(), String> {
        let zip_file = std::fs::File::open(&tmp_zip_path)
            .map_err(|e| format!("Error abriendo ZIP: {e}"))?;
        let mut archive = zip::ZipArchive::new(zip_file)
            .map_err(|e| format!("Error extrayendo uv: {e}"))?;

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

        std::fs::write(&exe_path, &buf).map_err(|e| format!("Error extrayendo uv: {e}"))?;

        Ok(())
    })();

    let _ = std::fs::remove_file(&tmp_zip_path);

    extract_result?;

    on_progress(95, "Verificando uv…");

    let binary = detect_file(&exe_path).ok_or_else(|| {
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

    #[test]
    fn test_parse_windows_arch_value() {
        assert_eq!(
            parse_windows_arch_value("AMD64"),
            Some(WindowsUvArch::X86_64)
        );
        assert_eq!(
            parse_windows_arch_value("arm64"),
            Some(WindowsUvArch::Aarch64)
        );
        assert_eq!(parse_windows_arch_value("mips"), None);
    }

    #[test]
    fn test_preferred_windows_arches_never_empty() {
        assert!(
            !preferred_windows_arches().is_empty(),
            "preferred_windows_arches should always return at least one supported arch"
        );
    }
}
