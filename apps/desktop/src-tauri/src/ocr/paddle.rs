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

use std::fmt;
use std::path::PathBuf;

use super::postprocess;
use super::provider::{BoundingBox, OcrOutput, OcrProvider, OcrRegion};

/// PaddleOCR-based OCR provider.
///
/// Uses the `ocr-rs` crate (MNN backend) for inference. Requires model files
/// to be present at init time — returns `Err` if any model file is missing.
///
/// Thread safety: `OcrEngine` from `ocr-rs` is `Send + Sync`, so this struct
/// CAN be held in a worker thread and shared across `spawn_blocking` calls
/// without per-call creation (unlike Tesseract/LepTess which is NOT Send).
pub struct PaddleOcrProvider {
    engine: ocr_rs::OcrEngine,
    #[allow(dead_code)]
    model_dir: PathBuf,
}

impl fmt::Debug for PaddleOcrProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PaddleOcrProvider")
            .field("model_dir", &self.model_dir)
            .finish_non_exhaustive()
    }
}

impl PaddleOcrProvider {
    /// Create a new PaddleOCR provider.
    ///
    /// Validates that all required model files exist in `model_dir` and
    /// initializes the MNN inference engine:
    /// - `PP-OCRv5_mobile_det.mnn` — text detection model (shared by all languages)
    /// - `latin_PP-OCRv5_mobile_rec_infer.mnn` — recognition model for Latin scripts
    /// - `ppocr_keys_latin.txt` — character set for Latin recognition
    ///
    /// # Errors
    ///
    /// Returns `Err` if:
    /// - Any model file is missing
    /// - The MNN engine fails to initialize (unlikely with valid models)
    pub fn new(model_dir: PathBuf) -> Result<Self, String> {
        let det_path = model_dir.join("PP-OCRv5_mobile_det.mnn");
        let rec_path = model_dir.join("latin_PP-OCRv5_mobile_rec_infer.mnn");
        let dict_path = model_dir.join("ppocr_keys_latin.txt");

        // Validate model files exist before attempting to load them.
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

        Ok(Self { engine, model_dir })
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
        let img = image::load_from_memory(image_bytes)
            .map_err(|e| format!("Failed to decode image for PaddleOCR: {e}"))?;

        // 2. Run detection + recognition pipeline
        let results: Vec<ocr_rs::OcrResult_> = self
            .engine
            .recognize(&img)
            .map_err(|e| format!("PaddleOCR inference failed: {e}"))?;

        // 3. Map ocr-rs OcrResult_ → OcrRegion with bounding boxes
        let regions: Vec<OcrRegion> = results
            .into_iter()
            .map(|r| {
                let rect = r.bbox.rect;
                OcrRegion {
                    text: r.text,
                    confidence: r.confidence as f32,
                    bbox: Some(BoundingBox {
                        x: rect.left(),
                        y: rect.top(),
                        width: rect.width() as u32,
                        height: rect.height() as u32,
                    }),
                    column: None,
                }
            })
            .collect();

        // 4. Post-process: column grouping, hyphen merge, paragraph detection
        let processed = postprocess::postprocess(regions);

        // 5. Assemble final text from processed regions
        let full_text = processed
            .iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(OcrOutput {
            text: full_text,
            regions: processed,
            method: "paddle".to_string(),
        })
    }

    fn name(&self) -> &str {
        "paddle"
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
}