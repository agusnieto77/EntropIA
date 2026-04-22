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
}