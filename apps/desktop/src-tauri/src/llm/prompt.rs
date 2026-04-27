/// Prompt templates for each LLM task. All prompts use Gemma's chat format.

fn gemma_prompt(instruction: &str) -> String {
    format!(
        "<start_of_turn>user\n{instruction}<end_of_turn>\n<start_of_turn>model\n"
    )
}

pub fn ocr_correction(text: &str) -> String {
    gemma_prompt(&format!(
        r#"Sos un especialista en transcripción de documentos históricos. El siguiente texto fue extraído por OCR de un documento impreso y contiene errores.

Tu tarea:
1. Corregí errores de OCR: sustituciones de caracteres, espacios faltantes, palabras garabateadas, letras mal leídas.
2. Unificá líneas rotas: mergeá líneas que fueron divididas por el layout en columnas o guiones en oraciones y párrafos completos. NO conserves saltos de línea que provienen del layout en columnas — reconstruí el flujo de lectura natural.
3. Ignorá los cortes de columnas de impresión: el texto viene de layouts multi-columna. Mergeá el texto de diferentes columnas en un orden de lectura coherente.
4. Preservá el idioma, estilo y terminología histórica originales. No modernices ni interpretes.

Devolvé SOLO el texto corregido y unificado con saltos de párrafo apropiados. No agregues explicaciones.

Texto OCR:
{text}"#
    ))
}

pub fn extract_entities(text: &str) -> String {
    gemma_prompt(&format!(
        r#"Extraé entidades nombradas de este texto de documento histórico. Devolvé un array JSON donde cada elemento tiene: "value" (el texto de la entidad), "type" (uno de: person, place, date, organization, institution, misc), "confidence" (0.0 a 1.0).

Solo extraé entidades de las que estés seguro. Para fechas, usá el formato original del texto. Respondé en el mismo idioma que el texto original (por defecto, español).

Devolvé SOLO el array JSON, sin explicaciones.

Texto:
{text}"#
    ))
}

pub fn extract_triples(text: &str) -> String {
    gemma_prompt(&format!(
        r#"Extraé triples semánticos (sujeto-predicado-objeto) de este texto de documento histórico. Devolvé un array JSON donde cada elemento tiene: "subject", "predicate", "object".

Enfocate en relaciones fácticas: quién hizo qué, quién está relacionado con quién, qué pasó dónde y cuándo. Usá los términos exactos del texto. Respondé en el mismo idioma que el texto original (por defecto, español).

Devolvé SOLO el array JSON, sin explicaciones.

Texto:
{text}"#
    ))
}

pub fn summarize(text: &str) -> String {
    gemma_prompt(&format!(
        r#"Resumí este texto de documento histórico en un ÚNICO párrafo conciso. El resumen debe:
- Tener entre 10 y 15 líneas
- Preservar nombres propios, fechas, lugares y eventos clave
- Estar escrito en el mismo idioma que el texto original (por defecto, español)
- SIEMPRE terminar con una oración completa que termine en punto

NO superes las 15 líneas. NO cortes a mitad de frase.

Texto:
{text}"#
    ))
}

pub fn classify(text: &str, categories: &[String]) -> String {
    let cats = categories.join(", ");
    gemma_prompt(&format!(
        r#"Clasificá este documento histórico en una o más de estas categorías: {cats}

Devolvé un array JSON de objetos con: "category" (de la lista arriba), "confidence" (0.0 a 1.0). Respondé en el mismo idioma que el texto original (por defecto, español).

Devolvé SOLO el array JSON, sin explicaciones.

Texto:
{text}"#
    ))
}

pub fn question_answer(question: &str, context: &str) -> String {
    gemma_prompt(&format!(
        r#"Respondé la siguiente pregunta basándote SOLO en los fragmentos de documento provistos. Si la respuesta no se puede determinar del contexto, decilo explícitamente. Respondé en el mismo idioma que la pregunta (por defecto, español).

Contexto:
{context}

Pregunta: {question}"#
    ))
}
