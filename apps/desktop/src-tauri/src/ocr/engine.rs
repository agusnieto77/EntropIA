use std::io::Cursor;

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

/// Block radius for adaptive thresholding (Sauvola-style via imageproc).
/// A radius of 15 = 31×31 neighbourhood window. Larger values smooth out
/// local variations more aggressively; smaller values preserve fine detail
/// but may be noisier. 15 works well for typical document photos.
const ADAPTIVE_BLOCK_RADIUS: u32 = 15;

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

        // Use PSM 3 (Fully Automatic) - Tesseract detects layout automatically.
        // For multi-column documents, this reads column-by-column (not human reading order),
        // but provides excellent text recognition quality (~90%+ accuracy).
        // The extracted text is fully searchable even if column order differs from human reading.
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
        // Adaptive binarization: convert to grayscale, then Sauvola threshold.
        // This produces a clean black-on-white image from dark/colored backgrounds.
        let gray = cropped.to_luma8();
        let binary = imageproc::contrast::adaptive_threshold(&gray, ADAPTIVE_BLOCK_RADIUS);
        image::DynamicImage::ImageLuma8(binary)
    } else {
        cropped
    };

    let final_img = upscale_if_needed(&processed);

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
            // A pixel is "content" when at least one channel is darker than the
            // white threshold. This catches colored text, grey scan artifacts,
            // and avoids false positives from pure-white/erased zones.
            if pixel[0] < WHITE_THRESHOLD
                || pixel[1] < WHITE_THRESHOLD
                || pixel[2] < WHITE_THRESHOLD
            {
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
