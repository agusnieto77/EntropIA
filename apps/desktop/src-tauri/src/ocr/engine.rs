use std::fs;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone)]
pub struct OcrEngine {
    lang: String,
    data_path: Option<String>,
}

// ── OCR quality thresholds ─────────────────────────────────────────────────────

/// Images smaller than this on either axis are upscaled proportionally.
/// At <1000px, characters don't have enough pixels for reliable OCR.
const MIN_DIM_FOR_OCR: u32 = 1000;

/// Pixels lighter than this (0–255) on ALL channels are considered "background"
/// during auto-crop. Handles JPEG compression artifacts and scanner bleed-through.
/// Pure white (255) from eraser tools is well above this threshold.
const WHITE_THRESHOLD: u8 = 245;

/// Small padding (px) added around the content bounding box so we don't clip
/// characters sitting right at the edge of the detected content.
const CROP_PADDING: u32 = 10;

/// DPI fallback used when the image has no valid resolution metadata.
/// 300 DPI is standard for document OCR.
const FALLBACK_DPI: i32 = 300;

/// Block size for adaptive thresholding (must be odd).
/// 31 = 31×31 neighbourhood window. Matches OpenCV default for document OCR.
const ADAPTIVE_BLOCK_SIZE: u32 = 31;

/// Constant subtracted from mean in adaptive thresholding.
/// Positive value makes thresholding more aggressive (more black).
/// 10 is standard for document images.
const ADAPTIVE_C: i32 = 10;

/// If the percentage of "white" pixels (luminance > 230) in the auto-cropped
/// image is below this threshold, the image is considered a non-standard
/// document (dark/colored background, photo of a page, etc.) and adaptive
/// binarization is applied before OCR.
///
/// Typical scans have >70% white background. Photos of documents on
/// colored/dark surfaces usually fall well below 40%.
const WHITE_BG_THRESHOLD: f32 = 0.40;

// ── Implementation ──────────────────────────────────────────────────────────────

impl OcrEngine {
    pub fn init(lang: &str, data_path: Option<&str>) -> Result<Self, String> {
        leptess::LepTess::new(data_path, lang).map_err(|e| {
            format!(
                "Failed to initialize Tesseract (lang={}, data_path={:?}): {e}",
                lang, data_path
            )
        })?;

        Ok(Self {
            lang: lang.to_string(),
            data_path: data_path.map(String::from),
        })
    }

    pub fn run_ocr(&self, image_bytes: &[u8]) -> Result<String, String> {
        let preprocessed = preprocess_for_ocr(image_bytes)?;

        // Strategy: Try Tesseract CLI first.
        // We attempt CLI even if data_path is None by using a known default path.
        // This ensures we get the correct column layout detection (PSM 3) which
        // relies on file metadata that the in-memory API lacks.
        if let Some(tesseract_exe) = find_tesseract_exe() {
            // Use provided data_path or fallback to vcpkg default
            let effective_data_path = self
                .data_path
                .as_deref()
                .unwrap_or(r"C:\vcpkg\installed\x64-windows-static-md\share\tessdata");

            match run_tesseract_cli(
                &tesseract_exe,
                &preprocessed,
                &self.lang,
                effective_data_path,
            ) {
                Ok(text) => return Ok(text),
                Err(e) => eprintln!("[OCR] CLI failed ({e}), falling back to leptess"),
            }
        }

        // Fallback: leptess in-memory API
        let mut lt = leptess::LepTess::new(self.data_path.as_deref(), &self.lang)
            .map_err(|e| format!("Failed to create Tesseract instance: {e}"))?;

        lt.set_image_from_mem(&preprocessed)
            .map_err(|e| format!("Failed to load image into Tesseract: {e}"))?;

        // If the image has no valid DPI metadata (common after editing in external
        // tools or re-encoding), Tesseract falls back to a meager 70 DPI. This
        // produces extremely poor results — thin characters, missed words, etc.
        // Setting a 300 DPI fallback ensures correct page segmentation and font
        // size interpretation whenever metadata is missing.
        lt.set_fallback_source_resolution(FALLBACK_DPI);

        // NOTE: Tesseract uses PSM 3 (Fully Automatic Page Segmentation) by default.
        // This mode automatically detects layout, columns, and orientation.
        // The `leptess` crate v0.14 does not expose set_page_seg_mode(), so we rely
        // on the default. If we need to change PSM in the future, we'd need to either:
        //   a) Upgrade leptess to a version that exposes this API, or
        //   b) Call Tesseract CLI directly with --psm flag.
        // For now, PSM 3 default works well for most document layouts.

        lt.get_utf8_text()
            .map_err(|e| format!("OCR inference failed: {e}"))
    }
}

// ── Preprocessing ──────────────────────────────────────────────────────────────

/// Preprocess an image for OCR with four steps:
///
/// 1. **Auto-crop** — Detect the bounding box of non-white/light content and crop
///    to it. This removes blank/erased zones and margins.
///
/// 2. **Detect background type** — If the image has a non-white background (photo
///    of a document on a dark surface, yellowed paper, etc.), the white-pixel
///    ratio will be low. We use this to decide if adaptive binarization is needed.
///
/// 3. **Adaptive binarization** (conditional) — For non-white-background images,
///    apply Sauvola-style adaptive thresholding to produce a clean black-on-white
///    image. This is CRITICAL for images with dark/colored backgrounds where
///    Tesseract's built-in Otsu binarization fails completely (returns 0 chars).
///
/// 4. **Upscale** — If the image is smaller than `MIN_DIM_FOR_OCR` on either
///    axis, scale it up proportionally.
fn preprocess_for_ocr(image_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img =
        image::load_from_memory(image_bytes).map_err(|e| format!("Failed to decode image: {e}"))?;

    let cropped = auto_crop_whitespace(&img);

    let needs_binarization = has_non_white_background(&cropped);

    let processed = if needs_binarization {
        // Gaussian adaptive thresholding: matches OpenCV's ADAPTIVE_THRESH_GAUSSIAN_C.
        // This produces clean black-on-white text while preserving column gutters,
        // which is critical for Tesseract's layout detection (PSM 3).
        // Unlike Sauvola, Gaussian weighting is less aggressive and maintains
        // the spatial separation between columns.
        let gray = cropped.to_luma8();
        let binary = adaptive_threshold_gaussian(&gray, ADAPTIVE_BLOCK_SIZE, ADAPTIVE_C);
        image::DynamicImage::ImageLuma8(binary)
    } else {
        cropped
    };

    let final_img = upscale_if_needed(&processed);

    // Save debug image for visual comparison with Python preprocessing.
    // Written to the workspace root so it's easy to find.
    let debug_path = std::env::current_dir()
        .unwrap_or_default()
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(&std::path::PathBuf::from("."))
        .join("debug_rust_preprocessed.png");
    let _ = final_img.save(&debug_path);

    let mut buf = Vec::with_capacity(image_bytes.len());
    final_img
        .write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode preprocessed image as PNG: {e}"))?;

    Ok(buf)
}

/// Detect whether an image has a non-white background.
///
/// For a typical scan or screenshot of a document, >70% of pixels will be
/// near-white (background). For a photo of a document on a dark/colored
/// surface, or a yellowed/sepia document, the white-pixel ratio drops
/// dramatically. When it falls below `WHITE_BG_THRESHOLD`, we treat the
/// image as non-white-background and apply adaptive binarization.
fn has_non_white_background(img: &image::DynamicImage) -> bool {
    let gray = img.to_luma8();
    let total = gray.width() as u32 * gray.height() as u32;

    if total == 0 {
        return false;
    }

    let white_count = gray.pixels().filter(|p| p[0] > 230).count() as u32;
    let white_ratio = white_count as f32 / total as f32;

    white_ratio < WHITE_BG_THRESHOLD
}

/// Find the bounding box of all non-white content and crop to it, removing blank
/// margins and erased zones. Returns the original image unchanged if no content
/// is detected (e.g. a solid-white image).
///
/// Uses luminance (grayscale) to match OpenCV's behavior in Python tests.
fn auto_crop_whitespace(img: &image::DynamicImage) -> image::DynamicImage {
    let (width, height) = (img.width(), img.height());
    let rgba = img.to_rgba8();

    let mut x_min = width;
    let mut y_min = height;
    let mut x_max = 0u32;
    let mut y_max = 0u32;

    for y in 0..height {
        for x in 0..width {
            let pixel = rgba.get_pixel(x, y);
            // Calculate luminance to match OpenCV's grayscale conversion:
            // 0.299*R + 0.587*G + 0.114*B
            let luminance =
                0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32;

            // A pixel is "content" when luminance is darker than the threshold.
            // This matches Python's: cv2.threshold(gray, 245, 255, cv2.THRESH_BINARY_INV)
            if luminance < WHITE_THRESHOLD as f32 {
                x_min = x_min.min(x);
                y_min = y_min.min(y);
                x_max = x_max.max(x);
                y_max = y_max.max(y);
            }
        }
    }

    // No content found — return original image as-is
    if x_max <= x_min || y_max <= y_min {
        return img.clone();
    }

    // Add padding so we don't clip characters at the edge
    let x_start = x_min.saturating_sub(CROP_PADDING);
    let y_start = y_min.saturating_sub(CROP_PADDING);
    let crop_w = (x_max + 1 + CROP_PADDING)
        .saturating_sub(x_start)
        .min(width - x_start);
    let crop_h = (y_max + 1 + CROP_PADDING)
        .saturating_sub(y_start)
        .min(height - y_start);

    img.crop_imm(x_start, y_start, crop_w, crop_h)
}

/// If the image is smaller than `MIN_DIM_FOR_OCR` on either axis, upscale it
/// proportionally so Tesseract gets enough pixels per character. Uses Lanczos3
/// resampling for high-quality upscaling.
fn upscale_if_needed(img: &image::DynamicImage) -> image::DynamicImage {
    let (w, h) = (img.width(), img.height());

    if w >= MIN_DIM_FOR_OCR && h >= MIN_DIM_FOR_OCR {
        return img.clone();
    }

    let scale = (MIN_DIM_FOR_OCR as f32 / w as f32)
        .max(MIN_DIM_FOR_OCR as f32 / h as f32)
        .max(1.0);

    let new_w = (w as f32 * scale) as u32;
    let new_h = (h as f32 * scale) as u32;

    img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3)
}

/// Gaussian adaptive thresholding — matches OpenCV's ADAPTIVE_THRESH_GAUSSIAN_C.
///
/// For each pixel, computes a weighted mean of the neighbourhood where weights
/// are a Gaussian window centered on the pixel. Threshold = mean - C.
/// Pixel < threshold → black (0), else white (255).
///
/// This preserves column gutters better than Sauvola, which is critical for
/// Tesseract's layout detection (PSM 3). Block size must be odd.
fn adaptive_threshold_gaussian(
    img: &image::GrayImage,
    block_size: u32,
    c: i32,
) -> image::GrayImage {
    let (width, height) = img.dimensions();
    let mut result = image::GrayImage::new(width, height);

    // Precompute Gaussian weights for the kernel
    let radius = block_size / 2;
    let sigma = radius as f64 / 3.0; // Standard Gaussian sigma
    let mut weights = Vec::with_capacity((block_size * block_size) as usize);
    let mut weight_sum = 0.0f64;

    for dy in 0..block_size {
        for dx in 0..block_size {
            let gx = (dx as f64) - (radius as f64);
            let gy = (dy as f64) - (radius as f64);
            let w = (-((gx * gx + gy * gy) / (2.0 * sigma * sigma))).exp();
            weights.push(w);
            weight_sum += w;
        }
    }

    // Normalize weights
    for w in &mut weights {
        *w /= weight_sum;
    }

    for y in 0..height {
        for x in 0..width {
            let mut weighted_sum = 0.0f64;
            let mut wi = 0;

            let y_start = y.saturating_sub(radius) as i32;
            let y_end = (y + radius).min(height - 1) as i32;
            let x_start = x.saturating_sub(radius) as i32;
            let x_end = (x + radius).min(width - 1) as i32;

            for ky in 0..block_size as i32 {
                let wy = y_start + ky;
                if wy < 0 || wy > y_end {
                    wi += block_size as usize;
                    continue;
                }
                for kx in 0..block_size as i32 {
                    let wx = x_start + kx;
                    if wx >= 0 && wx <= x_end {
                        let pixel = img.get_pixel(wx as u32, wy as u32)[0] as f64;
                        weighted_sum += pixel * weights[wi];
                    }
                    wi += 1;
                }
            }

            let threshold = weighted_sum - (c as f64);
            let value = if (img.get_pixel(x, y)[0] as f64) < threshold {
                0u8
            } else {
                255u8
            };

            result.put_pixel(x, y, image::Luma([value]));
        }
    }

    result
}

// ── Tesseract CLI Helper ───────────────────────────────────────────────────────────

/// Find tesseract.exe in common installation paths.
fn find_tesseract_exe() -> Option<PathBuf> {
    let candidates = [
        // vcpkg default location (EntropIA's setup)
        r"C:\vcpkg\installed\x64-windows-static-md\tools\tesseract\tesseract.exe",
        // Standard Tesseract-OCR installer
        r"C:\Program Files\Tesseract-OCR\tesseract.exe",
        r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
    ];

    for path in &candidates {
        if std::path::Path::new(path).exists() {
            return Some(PathBuf::from(path));
        }
    }

    None
}

/// Run Tesseract CLI with PSM 3 (Fully Automatic).
///
/// This writes the preprocessed image to a temp PNG file and calls
/// `tesseract.exe` directly. This matches the Python/pytesseract behavior
/// that correctly detects multi-column layouts.
///
/// CLI gives Tesseract full file metadata (DPI, color profile, etc.) which
/// helps the layout detection engine. The in-memory API (`set_image_from_mem`)
/// does not provide this metadata, which can cause column interleaving.
fn run_tesseract_cli(
    tesseract_exe: &PathBuf,
    image_data: &[u8],
    lang: &str,
    data_path: &str,
) -> Result<String, String> {
    // Create temp directory for this OCR run
    let temp_dir = std::env::temp_dir().join("entropia_ocr");
    fs::create_dir_all(&temp_dir).map_err(|e| format!("Failed to create temp dir: {e}"))?;

    // Use a unique filename based on timestamp to avoid collisions
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let input_path = temp_dir.join(format!("input_{}.png", timestamp));
    let output_path = temp_dir.join(format!("output_{}", timestamp));

    // Write preprocessed image to temp file
    let mut file = fs::File::create(&input_path)
        .map_err(|e| format!("Failed to create temp image file: {e}"))?;
    file.write_all(image_data)
        .map_err(|e| format!("Failed to write temp image: {e}"))?;

    // Build CLI command: tesseract input output -l lang --oem 3 --psm 3
    let mut cmd = Command::new(tesseract_exe);
    cmd.arg(&input_path)
        .arg(&output_path)
        .arg("-l")
        .arg(lang)
        .arg("--oem")
        .arg("3") // LSTM OCR Engine (matches Python/pytesseract default)
        .arg("--psm")
        .arg("3") // Fully Automatic Page Segmentation
        .env("TESSDATA_PREFIX", data_path);

    // Execute Tesseract
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute Tesseract CLI: {e}"))?;

    // Cleanup temp files (best effort)
    let _ = fs::remove_file(&input_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(output_path.with_extension("txt"));
        return Err(format!(
            "Tesseract CLI failed (exit {}): {}",
            output.status.code().unwrap_or(-1),
            stderr
        ));
    }

    // Read output text file
    let output_file = output_path.with_extension("txt");
    let text = fs::read_to_string(&output_file)
        .map_err(|e| format!("Failed to read Tesseract output: {e}"))?;

    // Cleanup output file
    let _ = fs::remove_file(&output_file);

    Ok(text)
}
