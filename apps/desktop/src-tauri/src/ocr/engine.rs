/// OCR engine wrapper around the `ocrs` crate.
///
/// Loads detection + recognition models from the app's bundled resources
/// and provides a synchronous `run_ocr` method suitable for `spawn_blocking`.
use image::GrayImage;
use ocrs::{ImageSource, OcrEngine as OcrsEngine, OcrEngineParams};
use rten::Model;
use tauri::path::BaseDirectory;
use tauri::Manager;

/// Wraps the `ocrs` engine with pre-loaded models.
pub struct OcrEngine {
    engine: OcrsEngine,
}

impl OcrEngine {
    /// Load detection and recognition models from the app's bundled resources.
    ///
    /// Expects the following files in the bundled resources directory:
    /// - `text-detection.rten`  — detection model
    /// - `text-recognition.rten` — recognition model
    ///
    /// Uses `resolve(..., BaseDirectory::Resource)` which works correctly
    /// both in `tauri dev` and in a bundled/installer build.
    ///
    /// # Errors
    /// Returns `Err(String)` if model files are missing or fail to load.
    pub fn load_models(app_handle: &tauri::AppHandle) -> Result<Self, String> {
        let detection_path = app_handle
            .path()
            .resolve("text-detection.rten", BaseDirectory::Resource)
            .map_err(|e| format!("Failed to resolve detection model path: {e}"))?;

        let recognition_path = app_handle
            .path()
            .resolve("text-recognition.rten", BaseDirectory::Resource)
            .map_err(|e| format!("Failed to resolve recognition model path: {e}"))?;

        let detection_model = Model::load_file(&detection_path).map_err(|e| {
            format!(
                "Failed to load detection model at {}: {e}",
                detection_path.display()
            )
        })?;
        let recognition_model = Model::load_file(&recognition_path).map_err(|e| {
            format!(
                "Failed to load recognition model at {}: {e}",
                recognition_path.display()
            )
        })?;

        let engine = OcrsEngine::new(OcrEngineParams {
            detection_model: Some(detection_model),
            recognition_model: Some(recognition_model),
            ..Default::default()
        })
        .map_err(|e| format!("Failed to initialise OCR engine: {e}"))?;

        Ok(Self { engine })
    }

    /// Run OCR inference on a pre-processed grayscale image.
    ///
    /// Converts the `GrayImage` to an RGB8 buffer (ocrs expects RGB), then
    /// prepares input and extracts all text as a single string.
    ///
    /// # Errors
    /// Returns `Err(String)` on inference failure.
    pub fn run_ocr(&self, image: GrayImage) -> Result<String, String> {
        // ocrs expects RGB input — expand single channel to 3-channel
        let (w, h) = image.dimensions();
        let rgb = image::DynamicImage::ImageLuma8(image).into_rgb8();

        let img_source = ImageSource::from_bytes(rgb.as_raw(), (w, h))
            .map_err(|e| format!("Failed to create image source ({w}x{h}): {e}"))?;

        let ocr_input = self
            .engine
            .prepare_input(img_source)
            .map_err(|e| format!("Failed to prepare OCR input: {e}"))?;

        let text = self
            .engine
            .get_text(&ocr_input)
            .map_err(|e| format!("OCR inference failed: {e}"))?;

        Ok(text)
    }
}
