use std::fs;
use std::path::PathBuf;
use std::process::Command;

const OCR_MODELS: &[(&str, &str)] = &[
    ("text-detection.rten", "https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten"),
    ("text-recognition.rten", "https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten"),
];

fn main() {
    // Ensure OCR model files are present in the resources directory before building.
    // This runs during `cargo build` / `cargo tauri build` and downloads the models
    // if they are missing, so the bundler can include them.
    ensure_ocr_models();

    tauri_build::build()
}

fn ensure_ocr_models() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let resources_dir = PathBuf::from(&manifest_dir).join("resources");

    if !resources_dir.exists() {
        fs::create_dir_all(&resources_dir).ok();
    }

    let mut all_present = true;

    for (filename, url) in OCR_MODELS {
        let target = resources_dir.join(filename);
        if target.exists() {
            let size = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
            println!(
                "cargo:warning=[OK] {} already exists ({:.1} MB)",
                filename,
                size as f64 / 1_048_576.0
            );
            continue;
        }

        all_present = false;
        println!("cargo:warning=[..] Downloading {} from S3...", filename);

        // Try PowerShell first (Windows), then curl (Unix)
        let success = if cfg!(windows) {
            let status = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!(
                        "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{}' -OutFile '{}' -UseBasicParsing",
                        url,
                        target.display()
                    ),
                ])
                .status();
            status.map(|s| s.success()).unwrap_or(false)
        } else {
            let status = Command::new("curl")
                .args(["-fsSL", "-o", target.to_str().unwrap(), url])
                .status();
            status.map(|s| s.success()).unwrap_or(false)
        };

        if success && target.exists() {
            let size = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
            println!(
                "cargo:warning=[OK] {} downloaded ({:.1} MB)",
                filename,
                size as f64 / 1_048_576.0
            );
        } else {
            // Clean up partial download
            let _ = fs::remove_file(&target);
            println!(
                "cargo:warning=[!!] Failed to download {}. OCR engine will not work.",
                filename
            );
        }
    }

    if !all_present {
        println!("cargo:warning=");
        println!("cargo:warning=OCR models were downloaded. Re-run build if models were missing.");
        println!("cargo:warning=");
    }
}
