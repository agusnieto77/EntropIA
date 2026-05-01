use std::path::PathBuf;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use serde::Deserialize;

use super::types::{sanitize_entity_value, Entity, EntitySource, EntityType, NerConfig, NerEngine};
use crate::python_discovery;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn apply_windows_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

#[derive(Debug, Clone)]
pub struct SpacyPreflightReport {
    pub mode: String,
    pub python_path: Option<PathBuf>,
    pub script_path: Option<PathBuf>,
    pub model_name: Option<String>,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

impl SpacyPreflightReport {
    pub fn is_ready(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn log(&self) {
        eprintln!("[nlp/ner/spacy] Preflight mode: {}", self.mode);
        eprintln!(
            "[nlp/ner/spacy]   python: {}",
            self.python_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<not configured>".to_string())
        );
        eprintln!(
            "[nlp/ner/spacy]   script: {}",
            self.script_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<not configured>".to_string())
        );
        eprintln!(
            "[nlp/ner/spacy]   model: {}",
            self.model_name.as_deref().unwrap_or("<default>")
        );

        if self.is_ready() {
            eprintln!("[nlp/ner/spacy] Preflight OK — spaCy assets look usable.");
        } else {
            eprintln!("[nlp/ner/spacy] Preflight degraded — falling back to rule-based if spaCy is requested.");
            for issue in &self.issues {
                eprintln!("[nlp/ner/spacy]   issue: {issue}");
            }
        }

        for warning in &self.warnings {
            eprintln!("[nlp/ner/spacy]   warning: {warning}");
        }
    }
}

pub struct SpacyNerEngine {
    python_path: PathBuf,
    script_path: PathBuf,
    model_name: String,
}

#[derive(Debug, Deserialize)]
struct SpacyOutput {
    model: String,
    entities: Vec<SpacyEntity>,
}

#[derive(Debug, Deserialize)]
struct SpacyEntity {
    entity_type: String,
    value: String,
    start_offset: usize,
    end_offset: usize,
}

impl SpacyNerEngine {
    pub fn inspect_assets(config: &NerConfig) -> SpacyPreflightReport {
        let mut report = SpacyPreflightReport {
            mode: match config.engine {
                super::types::NerEngineKind::RuleBased => "rule_based",
                super::types::NerEngineKind::Onnx => "onnx",
                super::types::NerEngineKind::Hybrid => "hybrid",
                super::types::NerEngineKind::Spacy => "spacy",
            }
            .to_string(),
            python_path: config.python_path.clone(),
            script_path: config.script_path.clone(),
            model_name: config.model_name.clone(),
            issues: Vec::new(),
            warnings: Vec::new(),
        };

        match &report.python_path {
            Some(path) if path.exists() => {}
            Some(path) => report
                .issues
                .push(format!("Python interpreter not found: {}", path.display())),
            None => report
                .issues
                .push("Python path is not configured".to_string()),
        }

        match &report.script_path {
            Some(path) if path.exists() => {}
            Some(path) => report
                .issues
                .push(format!("spaCy NER script not found: {}", path.display())),
            None => report
                .issues
                .push("spaCy script path is not configured".to_string()),
        }

        if report.model_name.is_none() {
            report
                .warnings
                .push("No spaCy model configured — using es_core_news_lg by default".to_string());
        }

        report
    }

    pub fn init(config: &NerConfig) -> Result<Self, String> {
        let python_path = config
            .python_path
            .clone()
            .ok_or("Missing spaCy python_path")?;
        let script_path = config
            .script_path
            .clone()
            .ok_or("Missing spaCy script_path")?;
        let model_name = config
            .model_name
            .clone()
            .unwrap_or_else(|| "es_core_news_lg".to_string());

        if !python_path.exists() {
            return Err(format!(
                "spaCy Python interpreter not found: {}",
                python_path.display()
            ));
        }
        if !script_path.exists() {
            return Err(format!(
                "spaCy NER script not found: {}",
                script_path.display()
            ));
        }

        eprintln!(
            "[nlp/ner/spacy] Engine configured: python={}, script={}, model={}",
            python_path.display(),
            script_path.display(),
            model_name
        );

        Ok(Self {
            python_path,
            script_path,
            model_name,
        })
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }
}

impl NerEngine for SpacyNerEngine {
    fn name(&self) -> &'static str {
        "spacy"
    }

    fn extract(&self, text: &str) -> Result<Vec<Entity>, String> {
        let mut cmd = Command::new(&self.python_path);
        apply_windows_no_window(&mut cmd);
        cmd.arg(&self.script_path)
            .arg("--text")
            .arg(text)
            .arg("--model")
            .arg(&self.model_name)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd.output().map_err(|e| {
            format!(
                "Failed to spawn spaCy NER process (python={}, script={}): {e}",
                self.python_path.display(),
                self.script_path.display()
            )
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);
            return Err(format!(
                "spaCy NER script failed (exit code {exit_code}).\nPython: {}\nScript: {}\nStderr: {}",
                self.python_path.display(),
                self.script_path.display(),
                stderr.trim()
            ));
        }

        let json_str = extract_sentinel_json(&stdout);
        let parsed: SpacyOutput = serde_json::from_str(json_str).map_err(|e| {
            format!(
                "Failed to parse spaCy NER JSON: {e}\nExtracted: {}\nStderr: {}",
                json_str,
                stderr.trim()
            )
        })?;

        Ok(parsed
            .entities
            .into_iter()
            .filter_map(|entity| map_spacy_entity(entity, &parsed.model))
            .collect())
    }
}

fn map_spacy_entity(entity: SpacyEntity, model_name: &str) -> Option<Entity> {
    let entity_type = match entity.entity_type.as_str() {
        "PER" | "PERSON" => EntityType::Person,
        "LOC" | "GPE" => EntityType::Place,
        "ORG" => EntityType::Organization,
        "DATE" | "TIME" => EntityType::Date,
        "MISC" => EntityType::Misc,
        _ => return None,
    };

    let value = sanitize_entity_value(&entity.value);
    if value.is_empty() {
        return None;
    }

    Some(Entity {
        entity_type,
        value,
        start_offset: entity.start_offset,
        end_offset: entity.end_offset,
        confidence: 1.0,
        source: EntitySource::Spacy,
        model_name: Some(model_name.to_string()),
    })
}

fn extract_sentinel_json(output: &str) -> &str {
    const BEGIN: &str = "===NER_JSON_BEGIN===";
    const END: &str = "===NER_JSON_END===";

    if let Some(start_idx) = output.find(BEGIN) {
        let content_start = start_idx + BEGIN.len();
        if let Some(end_idx) = output[content_start..].find(END) {
            return output[content_start..content_start + end_idx].trim();
        }
    }

    output.trim()
}

pub fn which_python(settings_db_path: Option<&std::path::Path>) -> Option<PathBuf> {
    python_discovery::which_python_for_module(
        "nlp/ner/spacy",
        "spacy",
        "spacy+es_core_news_lg",
        "import spacy, es_core_news_lg; print('ok')",
        settings_db_path,
    )
}
