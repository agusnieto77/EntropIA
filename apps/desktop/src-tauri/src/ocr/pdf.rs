/// PDF native text extraction and quality heuristic.

/// Extracts text from the native text layer of a PDF byte slice.
/// Returns the extracted text or an error message.
pub fn extract_pdf_text(bytes: &[u8]) -> Result<String, String> {
    pdf_extract::extract_text_from_mem(bytes)
        .map_err(|e| format!("PDF text extraction failed: {e}"))
}

/// Returns `true` if the text contains at least 50 valid UTF-8 alphanumeric characters.
/// Used to decide whether native PDF text is rich enough or we should fall back to OCR.
pub fn is_quality_text(text: &str) -> bool {
    let alphanum_count = text.chars().filter(|c| c.is_alphanumeric()).count();
    alphanum_count >= 50
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
        // Less than 50 alphanumeric characters — lots of symbols, few real chars
        let garbled = "!@#$%^&*()_+-=[]{}|;':\",./<>? abc 123";
        assert!(!is_quality_text(garbled));
    }

    #[test]
    fn normal_text_is_quality() {
        let text = "This is a perfectly normal paragraph of text that contains well over fifty alphanumeric characters and should pass the quality heuristic with ease.";
        assert!(is_quality_text(text));
    }
}
