//! OCR provider trait and shared output types.
//!
//! Defines the `OcrProvider` trait that decouples OCR logic from the worker,
//! and the `OcrOutput`, `OcrRegion`, `BoundingBox` structs used by all providers
//! to return structured recognition results.

use serde::Serialize;

/// Bounding box from OCR detection.
///
/// Represents a rectangle in pixel coordinates within the source image.
/// Origin (0,0) is the top-left corner of the image.
#[derive(Debug, Clone, Serialize)]
pub struct BoundingBox {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Single OCR region with optional bounding box and column assignment.
///
/// Each region represents a line or block of text recognized by the OCR engine.
/// The `column` field is assigned during post-processing to indicate which
/// column the region belongs to (0 = leftmost).
#[derive(Debug, Clone, Serialize)]
pub struct OcrRegion {
    pub text: String,
    pub confidence: f32,
    pub bbox: Option<BoundingBox>,
    pub column: Option<usize>,
}

/// Unified OCR output from any provider.
///
/// Contains the full recognized text, structured regions with bounding boxes,
/// and the method used to produce the result (e.g. "paddle" or "tesseract").
#[derive(Debug, Clone, Serialize)]
pub struct OcrOutput {
    pub text: String,
    pub regions: Vec<OcrRegion>,
    pub method: String,
}

/// Provider trait — sync, called inside `spawn_blocking`.
///
/// Each OCR engine implements this trait. The worker holds a `Box<dyn OcrProvider>`
/// and calls `recognize` for each job. Init-time fallback selects which provider
/// to use; once selected, all jobs go through the same provider.
pub trait OcrProvider: Send + Sync {
    /// Recognize text from raw image bytes.
    ///
    /// The bytes can be any format supported by `image::load_from_memory`
    /// (PNG, JPEG, TIFF, etc.). Returns structured output with regions and
    /// bounding boxes, or an error message on failure.
    fn recognize(&self, image_bytes: &[u8]) -> Result<OcrOutput, String>;

    /// Recognize text from a cropped region of an image.
    ///
    /// Default implementation crops the image to the region's bounding box
    /// (with padding) and calls `recognize()` on the crop. Providers that
    /// support region-level optimization can override this.
    fn recognize_region(
        &self,
        image_bytes: &[u8],
        region: &crate::layout::region::LayoutRegion,
    ) -> Result<OcrOutput, String> {
        // Default: fall back to full-image recognition
        let _ = region; // suppress unused warning
        self.recognize(image_bytes)
    }

    /// Short identifier for the provider (e.g. "paddle", "tesseract").
    fn name(&self) -> &str;
}