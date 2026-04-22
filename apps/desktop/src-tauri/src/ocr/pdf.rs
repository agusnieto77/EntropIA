//! PDF text extraction and page rendering for OCR fallback.
//!
//! Two extraction strategies:
//! 1. **Native text** — `extract_pdf_text()` extracts embedded text via `pdf-extract`.
//!    Fast and accurate for text-based PDFs. Quality-checked with `is_quality_text()`.
//! 2. **Page rendering** — `render_pdf_page_to_image()` renders a PDF page as PNG
//!    bitmap via `pdfium-render`, enabling OCR fallback for scanned/image-based PDFs.
//!
//! For multi-page PDFs, `pdf_page_count()` returns the total number of pages,
//! and `render_pdf_page_to_image()` accepts any page index (not just page 0).

use pdfium_render::prelude::*;
use std::io::Cursor;

/// Extract text from the native text layer of a PDF byte slice.
/// Returns the raw extracted text or an error message.
pub fn extract_pdf_text(bytes: &[u8]) -> Result<String, String> {
    pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| format!("PDF text extraction failed: {e}"))
}

/// Returns `true` if the text contains at least `MIN_ALPHANUM_CHARS` valid
/// UTF-8 alphanumeric characters. Used to decide whether native PDF text is
/// rich enough or we should fall back to OCR.
pub fn is_quality_text(text: &str) -> bool {
    const MIN_ALPHANUM_CHARS: usize = 50;
    text.chars().filter(|c| c.is_alphanumeric()).count() >= MIN_ALPHANUM_CHARS
}

/// Get the number of pages in a PDF document.
///
/// Used by the multi-page OCR pipeline to know how many pages to process.
pub fn pdf_page_count(bytes: &[u8]) -> Result<usize, String> {
    let pdfium = Pdfium::default();
    let document = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| format!("Failed to load PDF for page count: {e}"))?;
    Ok(document.pages().len().into())
}

/// Render a single PDF page to PNG bytes, suitable for OCR processing.
///
/// Uses `pdfium-render` to rasterize the page at 300 DPI equivalent
/// (target width ~2550px for letter-size). Returns raw PNG bytes that
/// can be fed directly to `OcrProvider::recognize()`.
///
/// # Arguments
/// * `bytes` — Raw PDF file bytes
/// * `page_index` — Zero-based page index (0 = first page)
///
/// # Errors
/// Returns `Err` if:
/// - Pdfium fails to initialize
/// - PDF cannot be loaded
/// - Page index is out of bounds
/// - Rendering or encoding fails
pub fn render_pdf_page_to_image(bytes: &[u8], page_index: usize) -> Result<Vec<u8>, String> {
    let pdfium = Pdfium::default();
    let document = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| format!("Failed to load PDF: {e}"))?;

    let pages = document.pages();
    let page_count: usize = pages.len().into();

    if page_index >= page_count {
        return Err(format!(
            "Page index {} out of bounds (PDF has {} pages)",
            page_index, page_count
        ));
    }

    let page_idx: PdfPageIndex = PdfPageIndex::from(page_index as u16);
    let page = pages
        .get(page_idx)
        .map_err(|e| format!("Failed to get page {page_index} from PDF: {e}"))?;

    // Render at 300 DPI equivalent. A typical letter-size page is 8.5" × 11"
    // which at 300 DPI gives 2550 × 3300 pixels.
    let render_config = PdfRenderConfig::new()
        .set_target_width(2550)
        .rotate_if_landscape(PdfPageRenderRotation::Degrees90, true);

    let bitmap = page
        .render_with_config(&render_config)
        .map_err(|e| format!("Failed to render PDF page {page_index}: {e}"))?;

    // Convert to image::DynamicImage, then encode as PNG
    let dynamic_image = bitmap.as_image();

    let mut png_bytes = Vec::new();
    dynamic_image
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode rendered page as PNG: {e}"))?;

    Ok(png_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_not_quality() {
        assert!(!is_quality_text(""));
    }

    #[test]
    fn short_garbled_text_is_not_quality() {
        let garbled = "!@#$%^&*()_+-=[]{}|;':\",./<>? abc 123";
        assert!(!is_quality_text(garbled));
    }

    #[test]
    fn normal_text_is_quality() {
        let text = "This is a perfectly normal paragraph of text that contains well over fifty alphanumeric characters and should pass the quality heuristic with ease.";
        assert!(is_quality_text(text));
    }

    /// pdf_page_count requires the pdfium native library which may not be
    /// available in unit test environments. Marked as ignored.
    #[test]
    #[ignore]
    fn pdf_page_count_invalid_bytes() {
        // Invalid PDF bytes should return an error, not panic
        let result = pdf_page_count(b"not a pdf");
        assert!(result.is_err(), "Expected error for invalid PDF bytes");
    }
}