//! PaddleOCR engine provider.
//!
//! This module is feature-gated behind `#[cfg(feature = "paddle-ocr")]`.
//! When enabled, it provides `PaddleOcrProvider` which uses the `ocr-rs` crate
//! for OCR inference with bounding-box output and post-processing.
//!
//! PP-OCRv5 detection + latin recognition covers Spanish, English, and 80+
//! additional languages. The detection model finds text regions first, then
//! recognition decodes each region — fundamentally better than Tesseract for
//! complex layouts (multi-column, rotated text, photos of documents).
//!
//! ## Orientation correction
//!
//! If the PP-LCNet document orientation model (`PP-LCNet_x1_0_doc_ori.mnn`)
//! is present in the model directory, the provider automatically detects and
//! corrects document rotation before OCR. This handles 0°, 90°, 180°, 270°
//! rotations common in scanned documents and phone photos.
//!
//! If the orientation model is missing, the provider proceeds without rotation
//! correction — existing behavior is preserved (graceful degradation).

use std::fmt;
use std::path::PathBuf;

use super::provider::{BoundingBox, OcrOutput, OcrProvider, OcrRegion};

/// Filename for the PP-LCNet document orientation model.
///
/// This is a small (~2.5MB) neural network that classifies document orientation
/// into 4 classes: 0°, 90°, 180°, 270°. When present, the OCR pipeline
/// automatically rotates the image to upright before detection+recognition.
const ORI_MODEL_FILENAME: &str = "PP-LCNet_x1_0_doc_ori.mnn";

/// Minimum confidence threshold for orientation classification.
///
/// Below this threshold, we assume the classifier is uncertain and skip
/// rotation correction — better to OCR an upright image with no rotation
/// than to incorrectly rotate one that's already correct.
const ORI_CONFIDENCE_THRESHOLD: f32 = 0.7;

/// PaddleOCR-based OCR provider with optional orientation correction.
///
/// Uses the `ocr-rs` crate (MNN backend) for inference. Requires detection and
/// recognition model files at init time — returns `Err` if any is missing.
///
/// The orientation model is **optional**: if `PP-LCNet_x1_0_doc_ori.mnn` is
/// present in the model directory, it's loaded and used to auto-rotate images
/// before OCR; if missing, OCR proceeds without rotation correction.
///
/// Thread safety: `OcrEngine` and `OriModel` from `ocr-rs` are `Send + Sync`,
/// so this struct CAN be held in a worker thread and shared across
/// `spawn_blocking` calls without per-call creation.
pub struct PaddleOcrProvider {
    engine: ocr_rs::OcrEngine,
    /// Optional PP-LCNet document orientation model.
    /// When present, images are classified and rotated before OCR.
    ori_model: Option<ocr_rs::OriModel>,
    #[allow(dead_code)]
    model_dir: PathBuf,
}

impl fmt::Debug for PaddleOcrProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PaddleOcrProvider")
            .field("model_dir", &self.model_dir)
            .field("has_ori_model", &self.ori_model.is_some())
            .finish_non_exhaustive()
    }
}

impl PaddleOcrProvider {
    /// Create a new PaddleOCR provider with optional orientation correction.
    ///
    /// Validates that all required model files exist in `model_dir` and
    /// initializes the MNN inference engine:
    /// - `PP-OCRv5_mobile_det.mnn` — text detection model (shared by all languages)
    /// - `latin_PP-OCRv5_mobile_rec_infer.mnn` — recognition model for Latin scripts
    /// - `ppocr_keys_latin.txt` — character set for Latin recognition
    ///
    /// Also attempts to load the optional orientation model:
    /// - `PP-LCNet_x1_0_doc_ori.mnn` — document orientation classifier (4-class)
    ///
    /// If the orientation model is missing, the provider proceeds without
    /// rotation correction (graceful degradation).
    ///
    /// # Errors
    ///
    /// Returns `Err` if any **required** model file is missing.
    /// Returns `Err` if the MNN engine fails to initialize (unlikely with valid models).
    /// Returns `Ok` even if the orientation model is missing (logged as warning).
    pub fn new(model_dir: PathBuf) -> Result<Self, String> {
        let det_path = model_dir.join("PP-OCRv5_mobile_det.mnn");
        let rec_path = model_dir.join("latin_PP-OCRv5_mobile_rec_infer.mnn");
        let dict_path = model_dir.join("ppocr_keys_latin.txt");

        // Validate required model files exist before attempting to load them.
        // This gives clear error messages ("model not found: /path/to/file")
        // instead of cryptic MNN init failures.
        for p in [&det_path, &rec_path, &dict_path] {
            if !p.exists() {
                return Err(format!("PaddleOCR model not found: {}", p.display()));
            }
        }

        let engine = ocr_rs::OcrEngine::new(
            det_path.to_str().ok_or_else(|| {
                format!("Invalid det model path: {}", det_path.display())
            })?,
            rec_path.to_str().ok_or_else(|| {
                format!("Invalid rec model path: {}", rec_path.display())
            })?,
            dict_path.to_str().ok_or_else(|| {
                format!("Invalid dict path: {}", dict_path.display())
            })?,
            None, // Use default OcrEngineConfig
        )
        .map_err(|e| format!("PaddleOCR engine init failed: {e}"))?;

        // Attempt to load optional orientation model.
        // If missing, we proceed without rotation correction — not a fatal error.
        let ori_model = {
            let ori_path = model_dir.join(ORI_MODEL_FILENAME);
            if !ori_path.exists() {
                eprintln!(
                    "[OCR] Orientation model not found at {} — rotation correction disabled",
                    ori_path.display()
                );
                None
            } else {
                match ocr_rs::OriModel::from_file(&ori_path, None) {
                    Ok(m) => {
                        eprintln!("[OCR] ✅ Orientation model loaded — automatic rotation correction enabled");
                        Some(m)
                    }
                    Err(e) => {
                        eprintln!(
                            "[OCR] ⚠️ Orientation model found at {} but failed to load: {e} — rotation correction disabled",
                            ori_path.display()
                        );
                        None
                    }
                }
            }
        };

        Ok(Self {
            engine,
            ori_model,
            model_dir,
        })
    }

    /// Returns the model directory path for diagnostics.
    #[allow(dead_code)]
    pub fn model_dir(&self) -> &std::path::Path {
        &self.model_dir
    }
}

impl OcrProvider for PaddleOcrProvider {
    fn recognize(&self, image_bytes: &[u8]) -> Result<OcrOutput, String> {
        // 1. Decode image from raw bytes
        let mut img = image::load_from_memory(image_bytes)
            .map_err(|e| format!("Failed to decode image for PaddleOCR: {e}"))?;

        // 2. Orientation correction (if model is loaded)
        //    Classify the image orientation and rotate to upright if needed.
        //    This handles documents scanned or photographed at 90°/180°/270°.
        if let Some(ref ori) = self.ori_model {
            match ori.classify(&img) {
                Ok(result) => {
                    if result.is_valid(ORI_CONFIDENCE_THRESHOLD) && result.angle != 0 {
                        eprintln!(
                            "[OCR] Orientation correction: rotating {}° (confidence: {:.2}%)",
                            result.angle,
                            result.confidence * 100.0
                        );
                        img = rotate_image(&img, result.angle);
                    } else if result.angle != 0 {
                        eprintln!(
                            "[OCR] Orientation detected {}° but confidence too low ({:.2}% < {:.0}%) — skipping rotation",
                            result.angle,
                            result.confidence * 100.0,
                            ORI_CONFIDENCE_THRESHOLD * 100.0
                        );
                    }
                    // angle == 0 means upright — no rotation needed, nothing to log
                }
                Err(e) => {
                    eprintln!("[OCR] Orientation classification failed: {e} — proceeding without rotation");
                }
            }
        }

        // 3. Run detection + recognition pipeline on the (possibly rotated) image
        let results: Vec<ocr_rs::OcrResult_> = self
            .engine
            .recognize(&img)
            .map_err(|e| format!("PaddleOCR inference failed: {e}"))?;

        // 4. Map ocr-rs OcrResult_ → OcrRegion with bounding boxes
        let regions: Vec<OcrRegion> = results
            .into_iter()
            .map(|r| {
                let rect = r.bbox.rect;
                OcrRegion {
                    text: r.text,
                    confidence: r.confidence,
                    bbox: Some(BoundingBox {
                        x: rect.left(),
                        y: rect.top(),
                        width: rect.width(),
                        height: rect.height(),
                    }),
                    column: None,
                }
            })
            .collect();

        // 5. Assemble final text from regions in PaddleOCR's natural order
        //    (top-to-bottom, left-to-right within rows).
        //    Post-processing is bypassed — the heuristic column grouping was
        //    mixing lines from different columns. PaddleOCR's detection model
        //    already returns regions in correct reading order for typical documents.
        let full_text = regions
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(OcrOutput {
            text: full_text,
            regions,
            method: "paddle".to_string(),
        })
    }

    /// Region-level OCR: decode image, apply orientation correction, crop to
    /// the region's bounding box (with padding), then run detection+recognition
    /// on just the crop. Adjusts OCR bbox coordinates back to full-image space.
    fn recognize_region(
        &self,
        image_bytes: &[u8],
        region: &crate::layout::region::LayoutRegion,
    ) -> Result<OcrOutput, String> {
        // 1. Decode image from raw bytes
        let mut img = image::load_from_memory(image_bytes)
            .map_err(|e| format!("Failed to decode image for PaddleOCR region: {e}"))?;

        // 2. Orientation correction (if model is loaded) — same as recognize()
        if let Some(ref ori) = self.ori_model {
            match ori.classify(&img) {
                Ok(result) => {
                    if result.is_valid(ORI_CONFIDENCE_THRESHOLD) && result.angle != 0 {
                        eprintln!(
                            "[OCR] Orientation correction: rotating {}° (confidence: {:.2}%)",
                            result.angle,
                            result.confidence * 100.0
                        );
                        img = rotate_image(&img, result.angle);
                    } else if result.angle != 0 {
                        eprintln!(
                            "[OCR] Orientation detected {}° but confidence too low — skipping rotation",
                            result.angle,
                        );
                    }
                }
                Err(e) => {
                    eprintln!("[OCR] Orientation classification failed: {e} — proceeding without rotation");
                }
            }
        }

        // 3. Crop to region bbox with proportional padding, clamped to image bounds.
        //    3% horizontal padding (left/right) avoids cutting off characters at box edges.
        //    3% vertical padding (top/bottom) provides similar margin proportionally.
        let pad_x = (region.bbox.width as f32 * 0.03).ceil() as i32;
        let pad_y = (region.bbox.height as f32 * 0.03).ceil() as i32;
        let img_w = img.width() as i32;
        let img_h = img.height() as i32;

        let crop_x = (region.bbox.x - pad_x).max(0) as u32;
        let crop_y = (region.bbox.y - pad_y).max(0) as u32;
        let crop_right = (region.bbox.x + region.bbox.width + pad_x).min(img_w) as u32;
        let crop_bottom = (region.bbox.y + region.bbox.height + pad_y).min(img_h) as u32;

        let crop_w = crop_right.saturating_sub(crop_x);
        let crop_h = crop_bottom.saturating_sub(crop_y);

        if crop_w == 0 || crop_h == 0 {
            // Region is out of bounds — fall back to full image recognition
            eprintln!(
                "[OCR] Region crop is empty (bbox {:?} on {}x{} image) — falling back to full image",
                region.bbox,
                img.width(),
                img.height()
            );
            return self.recognize(image_bytes);
        }

        let crop = img.crop_imm(crop_x, crop_y, crop_w, crop_h);

        // 4. Run detection + recognition directly on the DynamicImage crop
        let results: Vec<ocr_rs::OcrResult_> = self
            .engine
            .recognize(&crop)
            .map_err(|e| format!("PaddleOCR region inference failed: {e}"))?;

        // 5. Adjust bbox coordinates back to original image space.
        //    OCR output bboxes are relative to the crop; add the crop origin offset
        //    to convert them to full-image coordinates.
        let offset_x = crop_x as i32;
        let offset_y = crop_y as i32;

        let ocr_regions: Vec<OcrRegion> = results
            .into_iter()
            .map(|r| {
                let rect = r.bbox.rect;
                OcrRegion {
                    text: r.text,
                    confidence: r.confidence,
                    bbox: Some(BoundingBox {
                        x: rect.left() + offset_x,
                        y: rect.top() + offset_y,
                        width: rect.width(),
                        height: rect.height(),
                    }),
                    column: None,
                }
            })
            .collect();

        // 6. Assemble text from OCR regions
        let full_text = ocr_regions
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(OcrOutput {
            text: full_text,
            regions: ocr_regions,
            method: "paddle".to_string(),
        })
    }

    fn name(&self) -> &str {
        "paddle"
    }
}

/// Rotate a `DynamicImage` by the given angle in degrees.
///
/// Supports 90, 180, 270 (no-op for 0). Any other angle returns the image unchanged.
/// Uses `image::DynamicImage` rotation methods which are lossless 90° increment rotations.
fn rotate_image(img: &image::DynamicImage, angle: i32) -> image::DynamicImage {
    match angle {
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => {
            eprintln!("[OCR] Unexpected orientation angle {angle}° — not rotating");
            img.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paddle_provider_missing_models() {
        // With a nonexistent directory, init should fail with a clear error.
        let result = PaddleOcrProvider::new(PathBuf::from("/nonexistent/path"));
        assert!(result.is_err(), "Expected error for missing models");
        let err = result.unwrap_err();
        assert!(
            err.contains("PaddleOCR model not found"),
            "Error should mention missing model, got: {err}"
        );
    }

    /// Integration test: load real PP-OCRv5 models and recognize text from an image.
    /// This test is ignored by default because it requires:
    /// 1. PP-OCRv5 model files in the resources directory
    /// 2. A test image to process
    /// Run with: cargo test --features paddle-ocr -- --ignored paddle_integration
    #[test]
    #[ignore]
    fn test_paddle_provider_integration() {
        // Resolve model directory — same logic as resolve_paddle_model_dir in mod.rs
        let model_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("models")
            .join("ocr");

        let provider = PaddleOcrProvider::new(model_dir)
            .expect("PaddleOCR init failed — are model files in resources/models/ocr/?");

        eprintln!("[test] PaddleOCR provider initialized: {}", provider.name());

        // Load a test image — use the project's existing test image
        // CARGO_MANIFEST_DIR = .../apps/desktop/src-tauri
        // We need workspace root which is 3 levels up
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()  // apps/desktop
            .and_then(|p| p.parent()) // apps
            .and_then(|p| p.parent()) // workspace root (POSITRON/EntropIA)
            .expect("no workspace root");
        let test_image_path = workspace_root.join("rust_style_binary.png");

        if !test_image_path.exists() {
            eprintln!("[test] Skipping — test image not found at {:?}", test_image_path);
            return;
        }

        let image_bytes = std::fs::read(&test_image_path)
            .expect("Failed to read test image");

        eprintln!("[test] Running PaddleOCR on {:?} ({} bytes)...", test_image_path, image_bytes.len());

        let output = provider.recognize(&image_bytes)
            .expect("PaddleOCR recognition failed");

        eprintln!("[test] Method: {}", output.method);
        eprintln!("[test] Text length: {} chars", output.text.len());
        eprintln!("[test] Regions: {}", output.regions.len());
        eprintln!("[test] First 200 chars of text:\n{}", &output.text.chars().take(200).collect::<String>());

        // Basic assertions — we just need SOME text back
        assert!(!output.text.is_empty(), "OCR should produce non-empty text");
        assert!(!output.regions.is_empty(), "OCR should produce at least one region");
        assert_eq!(output.method, "paddle", "Method should be 'paddle'");

        // Verify bounding boxes are present (PaddleOCR provides them)
        let regions_with_bbox: Vec<_> = output.regions.iter()
            .filter(|r| r.bbox.is_some())
            .collect();
        assert!(
            !regions_with_bbox.is_empty(),
            "PaddleOCR should provide bounding boxes for detected regions"
        );

        eprintln!("[test] ✅ PaddleOCR integration test passed!");
    }

    #[test]
    fn test_rotate_image_90() {
        let img = image::DynamicImage::new_rgb8(200, 100); // landscape
        let rotated = rotate_image(&img, 90);
        // 90° rotation: 200x100 → 100x200
        assert_eq!(rotated.width(), 100);
        assert_eq!(rotated.height(), 200);
    }

    #[test]
    fn test_rotate_image_180() {
        let img = image::DynamicImage::new_rgb8(200, 100);
        let rotated = rotate_image(&img, 180);
        // 180° rotation: dimensions unchanged
        assert_eq!(rotated.width(), 200);
        assert_eq!(rotated.height(), 100);
    }

    #[test]
    fn test_rotate_image_270() {
        let img = image::DynamicImage::new_rgb8(200, 100); // landscape
        let rotated = rotate_image(&img, 270);
        // 270° rotation: 200x100 → 100x200
        assert_eq!(rotated.width(), 100);
        assert_eq!(rotated.height(), 200);
    }

    #[test]
    fn test_rotate_image_0_noop() {
        let img = image::DynamicImage::new_rgb8(200, 100);
        let rotated = rotate_image(&img, 0);
        // 0° should return image unchanged (unexpected angle path)
        assert_eq!(rotated.width(), 200);
        assert_eq!(rotated.height(), 100);
    }

    #[test]
    fn test_paddle_provider_without_orientation_model() {
        // PaddleOcrProvider should init successfully even without the orientation model.
        // The orientation model is optional — if missing, orientation correction is skipped.
        // This test uses a nonexistent dir, so it should fail on the required models,
        // but the error should be about det/rec/dict — NOT about the orientation model.
        let result = PaddleOcrProvider::new(PathBuf::from("/nonexistent/path"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        // The error should mention "PaddleOCR model not found" (for det/rec/dict)
        // and NOT mention "Orientation" at all (the ori model is optional)
        assert!(err.contains("PaddleOCR model not found"), "Expected model not found error, got: {err}");
        assert!(!err.contains("Orientation"), "Orientation model failure should NOT be a fatal error, got: {err}");
    }
}