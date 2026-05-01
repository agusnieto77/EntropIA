//! Tesseract OCR provider — wraps the existing `OcrEngine` pipeline.
//!
//! Implements the `OcrProvider` trait using Tesseract (via leptess in-memory
//! API or CLI fallback). This is the fallback provider used when PaddleOCR
//! models are unavailable.
//!
//! The preprocessing and CLI logic lives in `engine.rs`; this module only
//! adapts that pipeline to the `OcrProvider` trait interface.

use super::engine::OcrEngine;
use super::provider::{OcrOutput, OcrProvider, OcrRegion};

/// Tesseract-based OCR provider.
///
/// Holds an `OcrEngine` config (lang + data_path). The `OcrEngine` struct
/// only contains `String`/`Option<String>` fields — the actual LepTess
/// instance is created per-call inside `run_ocr()`, so this is safe for
/// `Send + Sync`.
#[derive(Debug)]
pub struct TesseractProvider {
    engine: OcrEngine,
}

impl TesseractProvider {
    /// Initialize a Tesseract provider.
    ///
    /// Validates that the tessdata files are available by attempting to create
    /// a `LepTess` instance. If tessdata is missing, returns an `Err` with
    /// a diagnostic message.
    ///
    /// # Arguments
    /// * `lang` — Tesseract language string (e.g. `"spa+eng"`)
    /// * `data_path` — Path to the tessdata directory. `None` uses Tesseract's
    ///   compiled-in default, which typically doesn't work on Windows.
    pub fn init(lang: &str, data_path: Option<&str>) -> Result<Self, String> {
        // Validate that tessdata is available by trying to create a LepTess instance.
        // OcrEngine::init does this validation internally.
        let engine = OcrEngine::init(lang, data_path)?;

        Ok(Self { engine })
    }
}

impl OcrProvider for TesseractProvider {
    fn recognize(&self, image_bytes: &[u8]) -> Result<OcrOutput, String> {
        // Delegate to the existing OcrEngine pipeline.
        // This runs the full preprocessing + CLI/leptess fallback chain.
        let text = self.engine.run_ocr(image_bytes)?;

        Ok(OcrOutput {
            text: text.clone(),
            regions: vec![OcrRegion {
                text,
                confidence: 0.0,
                bbox: None,
                column: None,
            }],
            method: "tesseract".to_string(),
        })
    }

    fn name(&self) -> &str {
        "tesseract"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tesseract_init_missing_tessdata() {
        // In a test environment, tessdata is not available, so init should fail.
        let result = TesseractProvider::init("spa+eng", None);
        assert!(result.is_err(), "Expected init to fail without tessdata");
        assert!(
            result
                .unwrap_err()
                .contains("Failed to initialize Tesseract"),
            "Error should mention Tesseract init failure"
        );
    }
}
