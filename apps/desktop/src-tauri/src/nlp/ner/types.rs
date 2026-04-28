use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityType {
    Person,
    Place,
    Date,
    Institution,
    Organization,
    Misc,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Place => "place",
            Self::Date => "date",
            Self::Institution => "organization",
            Self::Organization => "organization",
            Self::Misc => "misc",
        }
    }
}

pub fn sanitize_entity_value(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch: char| {
            if ch.is_alphanumeric() {
                return false;
            }
            matches!(
                ch,
                '"'
                    | '\''
                    | '“'
                    | '”'
                    | '‘'
                    | '’'
                    | '-'
                    | '–'
                    | '—'
                    | '«'
                    | '»'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | ':'
                    | ';'
                    | ','
                    | '.'
                    | '!'
                    | '?'
                    | '/'
                    | '\\'
                    | '|'
            ) || ch.is_whitespace()
        })
        .trim()
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntitySource {
    RuleBased,
    Onnx,
    Spacy,
    Llm,
}

impl EntitySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RuleBased => "rule_based",
            Self::Onnx => "onnx",
            Self::Spacy => "spacy",
            Self::Llm => "llm",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub entity_type: EntityType,
    pub value: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub confidence: f32,
    pub source: EntitySource,
    pub model_name: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NerEngineKind {
    RuleBased,
    Onnx,
    Hybrid,
    Spacy,
}

#[derive(Debug, Clone)]
pub struct NerConfig {
    pub engine: NerEngineKind,
    pub model_path: Option<PathBuf>,
    pub tokenizer_path: Option<PathBuf>,
    pub python_path: Option<PathBuf>,
    pub script_path: Option<PathBuf>,
    pub model_name: Option<String>,
    pub max_length: usize,
    pub stride: usize,
    pub score_threshold: f32,
}

#[allow(dead_code)]
pub trait NerEngine: Send + Sync {
    fn name(&self) -> &'static str;
    fn extract(&self, text: &str) -> Result<Vec<Entity>, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn institution_serializes_as_organization() {
        assert_eq!(EntityType::Institution.as_str(), "organization");
    }

    #[test]
    fn sanitize_entity_value_trims_edge_symbols() {
        assert_eq!(sanitize_entity_value("\"Conservas Baltar S.A.I.C.\""), "Conservas Baltar S.A.I.C");
        assert_eq!(sanitize_entity_value("REBELION JUVENIL. —"), "REBELION JUVENIL");
        assert_eq!(sanitize_entity_value("  - M. I. A. -  "), "M. I. A");
    }
}
