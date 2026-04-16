use std::io::Cursor;

#[derive(Clone)]
pub struct OcrEngine {
    lang: String,
    data_path: Option<String>,
}

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
        let img = image::load_from_memory(image_bytes)
            .map_err(|e| format!("Failed to decode image: {e}"))?;

        let mut tiff_buf = Vec::new();
        img.write_to(
            &mut Cursor::new(&mut tiff_buf),
            image::ImageFormat::Tiff.into(),
        )
        .map_err(|e| format!("Failed to encode image to TIFF: {e}"))?;

        let mut lt = leptess::LepTess::new(self.data_path.as_deref(), &self.lang)
            .map_err(|e| format!("Failed to create Tesseract instance: {e}"))?;

        lt.set_image_from_mem(&tiff_buf)
            .map_err(|e| format!("Failed to load image into Tesseract: {e}"))?;

        lt.get_utf8_text()
            .map_err(|e| format!("OCR inference failed: {e}"))
    }
}
