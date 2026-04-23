/// Prompt templates for each LLM task. All prompts use Gemma's chat format.

fn gemma_prompt(instruction: &str) -> String {
    format!(
        "<start_of_turn>user\n{instruction}<end_of_turn>\n<start_of_turn>model\n"
    )
}

pub fn ocr_correction(text: &str) -> String {
    gemma_prompt(&format!(
        r#"You are a specialist in historical document transcription. The following text was extracted via OCR from a degraded historical document and contains errors.

Correct the OCR errors while preserving the original language, style, and historical terminology. Do not modernize or interpret the text. Only fix clear OCR mistakes (character substitutions, missing spaces, garbled words).

Return ONLY the corrected text, nothing else.

OCR text:
{text}"#
    ))
}

pub fn extract_entities(text: &str) -> String {
    gemma_prompt(&format!(
        r#"Extract named entities from this historical document text. Return a JSON array where each element has: "value" (the entity text), "type" (one of: person, place, date, organization, institution, misc), "confidence" (0.0 to 1.0).

Only extract entities you are confident about. For dates, use the original format found in the text.

Return ONLY the JSON array, no explanation.

Text:
{text}"#
    ))
}

pub fn extract_triples(text: &str) -> String {
    gemma_prompt(&format!(
        r#"Extract semantic triples (subject-predicate-object) from this historical document text. Return a JSON array where each element has: "subject", "predicate", "object".

Focus on factual relationships: who did what, who is related to whom, what happened where and when. Use the exact terms from the text.

Return ONLY the JSON array, no explanation.

Text:
{text}"#
    ))
}

pub fn summarize(text: &str) -> String {
    gemma_prompt(&format!(
        r#"Summarize this historical document text in 2-3 paragraphs. Preserve key names, dates, places, and events. Write the summary in the same language as the source text.

Text:
{text}"#
    ))
}

pub fn classify(text: &str, categories: &[String]) -> String {
    let cats = categories.join(", ");
    gemma_prompt(&format!(
        r#"Classify this historical document into one or more of these categories: {cats}

Return a JSON array of objects with: "category" (from the list above), "confidence" (0.0 to 1.0).

Return ONLY the JSON array, no explanation.

Text:
{text}"#
    ))
}

pub fn question_answer(question: &str, context: &str) -> String {
    gemma_prompt(&format!(
        r#"Answer the following question based ONLY on the provided document excerpts. If the answer cannot be determined from the context, say so explicitly. Write the answer in the same language as the question.

Context:
{context}

Question: {question}"#
    ))
}
