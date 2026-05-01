use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use ndarray::{Array2, ArrayViewD};
use ort::{
    inputs,
    session::{builder::GraphOptimizationLevel, Session},
    value::TensorRef,
};
use serde::Deserialize;
use tokenizers::{Encoding, Tokenizer};

use super::types::{sanitize_entity_value, Entity, EntitySource, EntityType, NerConfig, NerEngine};

static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct OnnxPreflightReport {
    pub mode: String,
    pub model_path: Option<PathBuf>,
    pub tokenizer_path: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub runtime_path: Option<PathBuf>,
    pub runtime_env_path: Option<String>,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

impl OnnxPreflightReport {
    pub fn is_ready(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn log(&self) {
        eprintln!("[nlp/ner] Preflight mode: {}", self.mode);
        eprintln!(
            "[nlp/ner]   model: {}",
            display_path(self.model_path.as_deref())
        );
        eprintln!(
            "[nlp/ner]   tokenizer: {}",
            display_path(self.tokenizer_path.as_deref())
        );
        eprintln!(
            "[nlp/ner]   config: {}",
            display_path(self.config_path.as_deref())
        );

        if let Some(runtime_env_path) = &self.runtime_env_path {
            eprintln!("[nlp/ner]   runtime: ORT_DYLIB_PATH={runtime_env_path}");
        } else {
            eprintln!(
                "[nlp/ner]   runtime: {}",
                display_path(self.runtime_path.as_deref())
            );
        }

        if self.is_ready() {
            eprintln!("[nlp/ner] Preflight OK — ONNX assets look usable.");
        } else {
            eprintln!(
                "[nlp/ner] Preflight degraded — falling back to rule-based if ONNX is requested."
            );
            for issue in &self.issues {
                eprintln!("[nlp/ner]   issue: {issue}");
            }
        }

        for warning in &self.warnings {
            eprintln!("[nlp/ner]   warning: {warning}");
        }
    }
}

pub struct OnnxNerEngine {
    model_name: String,
    max_length: usize,
    stride: usize,
    score_threshold: f32,
    labels: Vec<String>,
    tokenizer: Mutex<Tokenizer>,
    session: Mutex<Session>,
}

#[derive(Debug, Deserialize)]
struct HuggingFaceModelConfig {
    id2label: Option<HashMap<String, String>>,
    #[serde(rename = "_name_or_path")]
    name_or_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BioTag {
    Begin,
    Inside,
}

#[derive(Debug, Clone)]
struct DecodedTag {
    bio: BioTag,
    entity_type: EntityType,
}

#[derive(Debug)]
struct OpenEntity {
    entity_type: EntityType,
    local_start: usize,
    local_end: usize,
    scores: Vec<f32>,
    pieces: Vec<String>,
}

#[derive(Debug, Clone)]
struct TokenPrediction {
    bio: BioTag,
    entity_type: EntityType,
    start: usize,
    end: usize,
    score: f32,
    piece: String,
}

struct Logits {
    values: Vec<f32>,
    token_count: usize,
    label_count: usize,
}

const CORE_ENTITY_THRESHOLD: f32 = 0.30;
const NON_CORE_ENTITY_THRESHOLD: f32 = 0.65;

impl OnnxNerEngine {
    pub fn inspect_assets(config: &NerConfig) -> OnnxPreflightReport {
        let mode = match config.engine {
            super::types::NerEngineKind::RuleBased => "rule_based",
            super::types::NerEngineKind::Onnx => "onnx",
            super::types::NerEngineKind::Hybrid => "hybrid",
            super::types::NerEngineKind::Spacy => "spacy",
        }
        .to_string();

        let mut report = OnnxPreflightReport {
            mode,
            model_path: config.model_path.clone(),
            tokenizer_path: config.tokenizer_path.clone(),
            config_path: config
                .model_path
                .as_ref()
                .and_then(|path| path.parent())
                .map(|dir| dir.join("config.json")),
            runtime_path: None,
            runtime_env_path: std::env::var("ORT_DYLIB_PATH").ok(),
            issues: Vec::new(),
            warnings: Vec::new(),
        };

        if let Some(model_path) = &report.model_path {
            if !model_path.exists() {
                report
                    .issues
                    .push(format!("Missing model.onnx at {}", model_path.display()));
            }
        } else {
            report
                .issues
                .push("Model path is not configured".to_string());
        }

        if let Some(tokenizer_path) = &report.tokenizer_path {
            if !tokenizer_path.exists() {
                report.issues.push(format!(
                    "Missing tokenizer.json at {}",
                    tokenizer_path.display()
                ));
            }
        } else {
            report
                .issues
                .push("Tokenizer path is not configured".to_string());
        }

        if let Some(config_path) = &report.config_path {
            if !config_path.exists() {
                report.warnings.push(format!(
                    "Optional config.json not found at {} — label metadata will use defaults",
                    config_path.display()
                ));
            }
        }

        if report.runtime_env_path.is_none() {
            if let Some(model_dir) = config.model_path.as_ref().and_then(|path| path.parent()) {
                report.runtime_path = find_ort_dylib(model_dir);
                if report.runtime_path.is_none() {
                    report.issues.push(format!(
                        "No ONNX Runtime library found near {} — place onnxruntime.dll/libonnxruntime.* there or set ORT_DYLIB_PATH",
                        model_dir.display()
                    ));
                }
            }
        }

        report
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn init(config: &NerConfig) -> Result<Self, String> {
        let model_path = config
            .model_path
            .as_ref()
            .ok_or("Missing ONNX model_path")?;
        let tokenizer_path = config
            .tokenizer_path
            .as_ref()
            .ok_or("Missing tokenizer_path")?;

        if !model_path.exists() {
            return Err(format!("ONNX model not found: {}", model_path.display()));
        }

        if !tokenizer_path.exists() {
            return Err(format!("Tokenizer not found: {}", tokenizer_path.display()));
        }

        let model_dir = model_path.parent().ok_or_else(|| {
            format!(
                "ONNX model has no parent directory: {}",
                model_path.display()
            )
        })?;

        ORT_INIT
            .get_or_init(|| initialize_ort(model_dir.to_path_buf()))
            .clone()?;

        let tokenizer = Tokenizer::from_file(tokenizer_path).map_err(|e| {
            format!(
                "Failed to load tokenizer from {}: {e}",
                tokenizer_path.display()
            )
        })?;

        let model_name = resolve_model_name(model_path);
        let labels = load_labels(model_dir);

        let session = Session::builder()
            .map_err(|e| format!("Failed to create ORT session builder: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| format!("Failed to configure ORT optimization level: {e}"))?
            .commit_from_file(model_path)
            .map_err(|e| format!("Failed to load ONNX model {}: {e}", model_path.display()))?;

        Ok(Self {
            model_name,
            max_length: config.max_length.max(16),
            stride: config.stride.max(8),
            score_threshold: config.score_threshold.clamp(0.0, 1.0),
            labels,
            tokenizer: Mutex::new(tokenizer),
            session: Mutex::new(session),
        })
    }

    fn extract_chunk(&self, chunk: &str, base_offset: usize) -> Result<Vec<Entity>, String> {
        let encoding = {
            let tokenizer = self
                .tokenizer
                .lock()
                .map_err(|_| "Tokenizer mutex poisoned".to_string())?;
            tokenizer
                .encode(chunk, true)
                .map_err(|e| format!("Failed to tokenize text chunk: {e}"))?
        };

        if encoding.get_ids().is_empty() {
            return Ok(vec![]);
        }

        if encoding.get_ids().len() > self.max_length {
            return self.extract_chunk_windows(chunk, base_offset, &encoding);
        }

        let token_count = encoding.get_ids().len();
        let input_ids = array_from_u32(&encoding.get_ids()[..token_count])?;
        let attention_mask = array_from_u32(&encoding.get_attention_mask()[..token_count])?;
        let type_ids = array_from_u32(&encoding.get_type_ids()[..token_count])?;

        let logits = self.run_inference(&input_ids, &attention_mask, &type_ids)?;
        decode_entities_from_parts(
            chunk,
            base_offset,
            &logits,
            &self.labels,
            &encoding.get_offsets()[..token_count],
            &encoding.get_special_tokens_mask()[..token_count],
            &encoding.get_tokens()[..token_count],
            self.score_threshold,
            &self.model_name,
        )
    }

    fn extract_semantic_chunks(&self, text: &str) -> Result<Vec<Entity>, String> {
        let chunks = self.build_chunks(text)?;
        let mut entities = Vec::new();

        for (start, end) in chunks {
            let Some(chunk) = text.get(start..end) else {
                continue;
            };
            let mut chunk_entities = self.extract_chunk(chunk, start)?;
            entities.append(&mut chunk_entities);
        }

        Ok(entities)
    }

    fn build_chunks(&self, text: &str) -> Result<Vec<(usize, usize)>, String> {
        let max_tokens = self.max_length.saturating_sub(2).max(32);
        let tokenizer = self
            .tokenizer
            .lock()
            .map_err(|_| "Tokenizer mutex poisoned".to_string())?;

        let mut chunks = Vec::new();
        for (start, end) in paragraph_spans(text) {
            let Some(span_text) = text.get(start..end) else {
                continue;
            };
            let token_count = count_tokens_without_specials(&tokenizer, span_text)?;
            if token_count <= max_tokens {
                chunks.push((start, end));
            } else {
                chunks.extend(split_long_span(text, start, end, &tokenizer, max_tokens)?);
            }
        }

        if chunks.is_empty() && !text.trim().is_empty() {
            return Ok(vec![(0, text.len())]);
        }

        Ok(chunks)
    }

    fn extract_chunk_windows(
        &self,
        chunk: &str,
        base_offset: usize,
        encoding: &Encoding,
    ) -> Result<Vec<Entity>, String> {
        let offsets = encoding.get_offsets();
        let total_tokens = encoding.get_ids().len();
        let mut entities = Vec::new();
        let mut start_token = 0usize;
        let step = self.max_length.saturating_sub(self.stride).max(1);
        let mut window_index = 0usize;

        while start_token < total_tokens {
            let end_token = (start_token + self.max_length).min(total_tokens);
            let start_byte = offsets.get(start_token).map(|offset| offset.0).unwrap_or(0);
            let end_byte = offsets
                .get(end_token.saturating_sub(1))
                .map(|offset| offset.1)
                .unwrap_or(chunk.len());

            if start_byte >= end_byte || end_byte > chunk.len() {
                break;
            }

            let Some(window_chunk) = chunk.get(start_byte..end_byte) else {
                break;
            };

            eprintln!(
                "[nlp/ner] ONNX token window: base_offset={}, window={}, tokens={}..{}, bytes={}..{}",
                base_offset,
                window_index,
                start_token,
                end_token,
                start_byte,
                end_byte
            );

            let ids = &encoding.get_ids()[start_token..end_token];
            let attention = &encoding.get_attention_mask()[start_token..end_token];
            let type_ids = &encoding.get_type_ids()[start_token..end_token];
            let logits = self.run_inference(
                &array_from_u32(ids)?,
                &array_from_u32(attention)?,
                &array_from_u32(type_ids)?,
            )?;

            let window_offsets = offsets[start_token..end_token]
                .iter()
                .map(|(start, end)| {
                    (
                        start.saturating_sub(start_byte),
                        end.saturating_sub(start_byte),
                    )
                })
                .collect::<Vec<_>>();
            let window_special =
                encoding.get_special_tokens_mask()[start_token..end_token].to_vec();
            let window_tokens = encoding.get_tokens()[start_token..end_token].to_vec();

            let mut window_entities = decode_entities_from_parts(
                window_chunk,
                base_offset + start_byte,
                &logits,
                &self.labels,
                &window_offsets,
                &window_special,
                &window_tokens,
                self.score_threshold,
                &self.model_name,
            )?;
            prune_window_edge_entities(
                &mut window_entities,
                base_offset + start_byte,
                base_offset + end_byte,
                start_token == 0,
                end_token == total_tokens,
            );
            entities.append(&mut window_entities);

            if end_token == total_tokens {
                break;
            }

            start_token = start_token.saturating_add(step);
            window_index += 1;
        }

        Ok(consolidate_onnx_entities(entities))
    }

    fn run_inference(
        &self,
        input_ids: &Array2<i64>,
        attention_mask: &Array2<i64>,
        type_ids: &Array2<i64>,
    ) -> Result<Logits, String> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| "ONNX session mutex poisoned".to_string())?;

        let outputs = match session.inputs.len() {
            2 => session
                .run(inputs![
                    TensorRef::from_array_view(input_ids)
                        .map_err(|e| format!("Failed to create input_ids tensor: {e}"))?,
                    TensorRef::from_array_view(attention_mask)
                        .map_err(|e| format!("Failed to create attention_mask tensor: {e}"))?,
                ])
                .map_err(|e| format!("ONNX inference failed: {e}"))?,
            3 => session
                .run(inputs![
                    TensorRef::from_array_view(input_ids)
                        .map_err(|e| format!("Failed to create input_ids tensor: {e}"))?,
                    TensorRef::from_array_view(attention_mask)
                        .map_err(|e| format!("Failed to create attention_mask tensor: {e}"))?,
                    TensorRef::from_array_view(type_ids)
                        .map_err(|e| format!("Failed to create token_type_ids tensor: {e}"))?,
                ])
                .map_err(|e| format!("ONNX inference failed: {e}"))?,
            count => {
                return Err(format!(
                    "Unsupported ONNX input count for token classification: expected 2 or 3 inputs, got {count}"
                ))
            }
        };

        let logits = outputs[0]
            .try_extract_array::<f32>()
            .map_err(|e| format!("Failed to extract ONNX logits: {e}"))?;

        let shape = logits.shape();
        if shape.len() != 3 {
            return Err(format!(
                "Unexpected ONNX logits shape for token classification: {shape:?}"
            ));
        }

        Ok(Logits {
            values: logits.iter().copied().collect(),
            token_count: shape[1],
            label_count: shape[2],
        })
    }
}

fn display_path(path: Option<&Path>) -> String {
    path.map(|p| p.display().to_string())
        .unwrap_or_else(|| "<not configured>".to_string())
}

impl NerEngine for OnnxNerEngine {
    fn name(&self) -> &'static str {
        "onnx"
    }

    fn extract(&self, text: &str) -> Result<Vec<Entity>, String> {
        if text.trim().is_empty() {
            return Ok(vec![]);
        }

        let entities = self.extract_semantic_chunks(text)?;

        Ok(consolidate_onnx_entities(entities))
    }
}

fn paragraph_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut paragraph_start: Option<usize> = None;
    let mut line_start = 0usize;

    while line_start <= text.len() {
        let rel_end = text[line_start..].find('\n');
        let line_end = rel_end.map(|idx| line_start + idx).unwrap_or(text.len());
        let line = &text[line_start..line_end];

        if line.trim().is_empty() {
            if let Some(start) = paragraph_start.take() {
                spans.push((start, line_start));
            }
        } else if paragraph_start.is_none() {
            paragraph_start = Some(line_start);
        }

        if let Some(rel) = rel_end {
            line_start = line_end + text[line_end..].chars().next().unwrap_or('\n').len_utf8();
            if rel == 0 && line_start > text.len() {
                break;
            }
        } else {
            break;
        }
    }

    if let Some(start) = paragraph_start {
        spans.push((start, text.len()));
    }

    spans
}

fn split_long_span(
    text: &str,
    start: usize,
    end: usize,
    tokenizer: &Tokenizer,
    max_tokens: usize,
) -> Result<Vec<(usize, usize)>, String> {
    let Some(chunk) = text.get(start..end) else {
        return Ok(vec![(start, end)]);
    };
    let pieces = token_piece_spans(chunk);
    if pieces.is_empty() {
        return Ok(vec![(start, end)]);
    }

    let mut spans = Vec::new();
    let mut current_start = 0usize;
    let mut current_end = 0usize;
    let mut current_tokens = 0usize;
    let mut has_current = false;

    for (piece_start, piece_end) in pieces {
        let Some(piece_text) = chunk.get(piece_start..piece_end) else {
            continue;
        };
        let piece_tokens = count_tokens_without_specials(tokenizer, piece_text)?;

        if has_current && current_tokens + piece_tokens > max_tokens {
            spans.push((start + current_start, start + current_end));
            has_current = false;
            current_tokens = 0;
        }

        if !has_current {
            current_start = piece_start;
            current_end = piece_end;
            current_tokens = piece_tokens;
            has_current = true;
        } else {
            current_end = piece_end;
            current_tokens += piece_tokens;
        }
    }

    if has_current {
        spans.push((start + current_start, start + current_end));
    }

    Ok(spans)
}

fn token_piece_spans(chunk: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut idx = 0usize;

    while idx < chunk.len() {
        while idx < chunk.len() {
            let Some(ch) = chunk[idx..].chars().next() else {
                break;
            };
            if !ch.is_whitespace() {
                break;
            }
            idx += ch.len_utf8();
        }

        if idx >= chunk.len() {
            break;
        }

        let piece_start = idx;
        while idx < chunk.len() {
            let Some(ch) = chunk[idx..].chars().next() else {
                break;
            };
            if ch.is_whitespace() {
                break;
            }
            idx += ch.len_utf8();
        }

        while idx < chunk.len() {
            let Some(ch) = chunk[idx..].chars().next() else {
                break;
            };
            if !ch.is_whitespace() {
                break;
            }
            idx += ch.len_utf8();
        }

        spans.push((piece_start, idx));
    }

    spans
}

fn count_tokens_without_specials(tokenizer: &Tokenizer, text: &str) -> Result<usize, String> {
    tokenizer
        .encode(text, false)
        .map(|encoding| encoding.get_ids().len())
        .map_err(|e| format!("Failed to tokenize text chunk for semantic chunking: {e}"))
}

fn array_from_u32(values: &[u32]) -> Result<Array2<i64>, String> {
    Array2::from_shape_vec(
        (1, values.len()),
        values.iter().map(|v| *v as i64).collect(),
    )
    .map_err(|e| format!("Failed to build ONNX input tensor: {e}"))
}

fn initialize_ort(model_dir: PathBuf) -> Result<(), String> {
    if std::env::var_os("ORT_DYLIB_PATH").is_some() {
        ort::init()
            .commit()
            .map_err(|e| format!("Failed to initialize ORT from ORT_DYLIB_PATH: {e}"))?;
        return Ok(());
    }

    let dylib_path = find_ort_dylib(&model_dir).ok_or_else(|| {
        format!(
            "No ONNX Runtime dynamic library found near model directory {}. Expected onnxruntime.dll / libonnxruntime.* or set ORT_DYLIB_PATH.",
            model_dir.display()
        )
    })?;

    ort::init_from(dylib_path.display().to_string())
        .commit()
        .map_err(|e| {
            format!(
                "Failed to initialize ORT from {}: {e}",
                dylib_path.display()
            )
        })?;

    Ok(())
}

fn find_ort_dylib(model_dir: &Path) -> Option<PathBuf> {
    let candidates = runtime_candidates(model_dir);
    candidates.into_iter().find(|path| path.exists())
}

fn runtime_candidates(model_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut push_names = |base: &Path| {
        for name in runtime_file_names() {
            candidates.push(base.join(name));
        }
    };

    push_names(model_dir);
    if let Some(parent) = model_dir.parent() {
        push_names(parent);
    }

    candidates
}

fn runtime_file_names() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &["onnxruntime.dll"]
    }

    #[cfg(target_os = "linux")]
    {
        &["libonnxruntime.so", "libonnxruntime.so.1"]
    }

    #[cfg(target_os = "macos")]
    {
        &["libonnxruntime.dylib"]
    }
}

fn resolve_model_name(model_path: &Path) -> String {
    let model_dir = model_path.parent().unwrap_or_else(|| Path::new("."));
    let config_path = model_dir.join("config.json");
    if let Ok(raw) = fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<HuggingFaceModelConfig>(&raw) {
            if let Some(name) = config.name_or_path {
                if !name.trim().is_empty() {
                    return name;
                }
            }
        }
    }

    model_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("onnx-ner")
        .to_string()
}

fn load_labels(model_dir: &Path) -> Vec<String> {
    let config_path = model_dir.join("config.json");
    if let Ok(raw) = fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<HuggingFaceModelConfig>(&raw) {
            if let Some(id2label) = config.id2label {
                let mut pairs: Vec<(usize, String)> = id2label
                    .into_iter()
                    .filter_map(|(idx, label)| idx.parse::<usize>().ok().map(|i| (i, label)))
                    .collect();
                pairs.sort_by_key(|(idx, _)| *idx);
                if !pairs.is_empty() {
                    return pairs.into_iter().map(|(_, label)| label).collect();
                }
            }
        }
    }

    default_labels()
}

fn default_labels() -> Vec<String> {
    vec![
        "B-LOC".to_string(),
        "B-MISC".to_string(),
        "B-ORG".to_string(),
        "B-PER".to_string(),
        "I-LOC".to_string(),
        "I-MISC".to_string(),
        "I-ORG".to_string(),
        "I-PER".to_string(),
        "O".to_string(),
    ]
}

fn decode_entities_from_parts(
    chunk: &str,
    base_offset: usize,
    logits: &Logits,
    labels: &[String],
    offsets: &[(usize, usize)],
    special_tokens: &[u32],
    tokens: &[String],
    score_threshold: f32,
    model_name: &str,
) -> Result<Vec<Entity>, String> {
    let label_count = logits.label_count;
    if label_count == 0 {
        return Ok(vec![]);
    }

    let effective_labels = if labels.len() == label_count {
        labels.to_vec()
    } else if label_count == 9 {
        default_labels()
    } else {
        (0..label_count).map(|idx| format!("LABEL_{idx}")).collect()
    };

    let logits = ArrayViewD::from_shape(
        ndarray::IxDyn(&[1, logits.token_count, label_count]),
        &logits.values,
    )
    .map_err(|e| format!("Failed to view ONNX logits with expected shape: {e}"))?;

    let mut token_predictions = Vec::new();

    for token_idx in 0..logits.shape()[1]
        .min(offsets.len())
        .min(special_tokens.len())
    {
        let (start, end) = offsets[token_idx];
        let piece = tokens
            .get(token_idx)
            .map(String::as_str)
            .unwrap_or_default();
        if special_tokens[token_idx] != 0 || start >= end {
            continue;
        }

        let (label, confidence) = best_label(&logits, token_idx, &effective_labels);

        let Some(tag) = parse_bio_label(&label) else {
            continue;
        };

        let min_confidence = entity_threshold(&tag.entity_type, score_threshold);
        if confidence < min_confidence {
            continue;
        }

        token_predictions.push(TokenPrediction {
            bio: tag.bio,
            entity_type: tag.entity_type.clone(),
            start,
            end,
            score: confidence,
            piece: piece.to_string(),
        });
    }

    Ok(aggregate_simple_entities(
        chunk,
        base_offset,
        token_predictions,
        model_name,
    ))
}

fn best_label(logits: &ArrayViewD<'_, f32>, token_idx: usize, labels: &[String]) -> (String, f32) {
    let mut best_idx = 0usize;
    let mut best_logit = f32::NEG_INFINITY;
    let mut logit_row = Vec::with_capacity(labels.len());

    for label_idx in 0..labels.len() {
        let value = logits[[0, token_idx, label_idx]];
        logit_row.push(value);
        if value > best_logit {
            best_logit = value;
            best_idx = label_idx;
        }
    }

    (
        labels[best_idx].clone(),
        softmax_confidence(&logit_row, best_idx),
    )
}

fn softmax_confidence(logits: &[f32], best_idx: usize) -> f32 {
    if logits.is_empty() || best_idx >= logits.len() {
        return 0.0;
    }

    let max_logit = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let denom = logits
        .iter()
        .map(|logit| (*logit - max_logit).exp())
        .sum::<f32>();

    if denom <= f32::EPSILON {
        0.0
    } else {
        (logits[best_idx] - max_logit).exp() / denom
    }
}

fn entity_threshold(entity_type: &EntityType, configured_threshold: f32) -> f32 {
    match entity_type {
        EntityType::Person
        | EntityType::Place
        | EntityType::Organization
        | EntityType::Institution => configured_threshold.min(CORE_ENTITY_THRESHOLD),
        _ => configured_threshold.max(NON_CORE_ENTITY_THRESHOLD),
    }
}

fn parse_bio_label(label: &str) -> Option<DecodedTag> {
    if label == "O" {
        return None;
    }

    let (prefix, raw_type) = label.split_once('-')?;
    let bio = match prefix {
        "B" => BioTag::Begin,
        "I" => BioTag::Inside,
        _ => return None,
    };

    let entity_type = match raw_type {
        "PER" => EntityType::Person,
        "LOC" => EntityType::Place,
        "ORG" => EntityType::Organization,
        "MISC" => EntityType::Misc,
        "DATE" => EntityType::Date,
        "INST" => EntityType::Institution,
        _ => return None,
    };

    Some(DecodedTag { bio, entity_type })
}

/// Aggregate token predictions into entities, respecting BIO tagging:
///
/// - **B-XXX**: Always flush any open entity and start a new one.
/// - **I-XXX**: Extend the open entity if the type matches; otherwise, start a
///   new entity (marks a broken sequence or type mismatch).
///
/// Subword continuation pieces (## prefix) are always attached to the open
/// entity regardless of gap. Word-start markers (▁, Ġ) are treated as word
/// boundaries that CAN extend an open I-tagged entity (since ▁Man ▁del ▁Plata
/// inside a B-LOC…I-LOC…I-LOC span is valid).
fn aggregate_simple_entities(
    chunk: &str,
    base_offset: usize,
    token_predictions: Vec<TokenPrediction>,
    model_name: &str,
) -> Vec<Entity> {
    let mut entities = Vec::new();
    let mut open: Option<OpenEntity> = None;

    for token in token_predictions {
        match token.bio {
            BioTag::Begin => {
                // BIO is the authority, but token classifiers often emit a fresh B-XXX
                // in the middle of the same surface entity on subword fragments or
                // punctuation-heavy abbreviations. Repair those cases before flushing.
                if let Some(current) = &mut open {
                    if current.entity_type == token.entity_type
                        && should_repair_begin_as_continuation(chunk, current, &token)
                    {
                        current.local_end = current.local_end.max(token.end);
                        current.scores.push(token.score);
                        current.pieces.push(token.piece);
                        continue;
                    }
                }

                flush_entity(&mut entities, &mut open, chunk, base_offset, model_name);
                open = Some(OpenEntity {
                    entity_type: token.entity_type,
                    local_start: token.start,
                    local_end: token.end,
                    scores: vec![token.score],
                    pieces: vec![token.piece],
                });
            }
            BioTag::Inside => {
                // I-XXX extends the open entity only when the type matches.
                // If there's a type mismatch or no open entity, treat it as a
                // broken sequence and start fresh (the model emitted I without B).
                if let Some(current) = &mut open {
                    if current.entity_type == token.entity_type {
                        // Same type — extend the open entity.
                        current.local_end = current.local_end.max(token.end);
                        current.scores.push(token.score);
                        current.pieces.push(token.piece);
                    } else {
                        // Type mismatch — flush and restart. A transition like
                        // B-LOC → I-PER is malformed; treat I-PER as a new entity.
                        flush_entity(&mut entities, &mut open, chunk, base_offset, model_name);
                        open = Some(OpenEntity {
                            entity_type: token.entity_type,
                            local_start: token.start,
                            local_end: token.end,
                            scores: vec![token.score],
                            pieces: vec![token.piece],
                        });
                    }
                } else {
                    // Orphan I-tag without a preceding B-tag. Start a new entity.
                    open = Some(OpenEntity {
                        entity_type: token.entity_type,
                        local_start: token.start,
                        local_end: token.end,
                        scores: vec![token.score],
                        pieces: vec![token.piece],
                    });
                }
            }
        }
    }

    flush_entity(&mut entities, &mut open, chunk, base_offset, model_name);
    entities
}

fn should_repair_begin_as_continuation(
    chunk: &str,
    current: &OpenEntity,
    token: &TokenPrediction,
) -> bool {
    if current.entity_type != token.entity_type {
        return false;
    }

    if is_continuation_piece(&token.piece) || token.start <= current.local_end {
        return true;
    }

    let gap = chunk
        .get(current.local_end..token.start)
        .unwrap_or_default();
    if !gap.chars().all(is_entity_joiner) {
        return false;
    }

    let normalized = normalize_piece_text(&token.piece);
    let mut chars = normalized.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    // Repair only likely tokenizer/model glitches, not genuine new B-tags.
    first.is_lowercase() || normalized.chars().count() == 1
}

fn normalize_piece_text(piece: &str) -> &str {
    if is_continuation_piece(piece) {
        piece.trim_start_matches("##")
    } else if is_word_start_piece(piece) {
        piece.trim_start_matches('▁').trim_start_matches('Ġ')
    } else {
        piece
    }
}

fn prune_window_edge_entities(
    entities: &mut Vec<Entity>,
    window_start_offset: usize,
    window_end_offset: usize,
    is_first_window: bool,
    is_last_window: bool,
) {
    entities.retain(|entity| {
        let touches_left_edge = !is_first_window && entity.start_offset <= window_start_offset;
        let touches_right_edge = !is_last_window && entity.end_offset >= window_end_offset;
        !(touches_left_edge || touches_right_edge)
    });
}

fn flush_entity(
    entities: &mut Vec<Entity>,
    open: &mut Option<OpenEntity>,
    chunk: &str,
    base_offset: usize,
    model_name: &str,
) {
    let Some(current) = open.take() else {
        return;
    };

    let (aligned_start, aligned_end) =
        align_entity_span_to_source(chunk, current.local_start, current.local_end);

    let span_value = chunk
        .get(aligned_start..aligned_end)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| reconstruct_from_pieces(&current.pieces));

    let reconstructed = reconstruct_from_pieces(&current.pieces);
    let value = choose_best_surface_form(&span_value, &reconstructed);

    let sanitized = sanitize_entity_value(&value);
    let value = sanitized.trim();
    if value.is_empty() || should_drop_entity_value(value) {
        return;
    }

    let entity_type = normalize_entity_type(current.entity_type, value);

    let confidence = if current.scores.is_empty() {
        0.0
    } else {
        current.scores.iter().sum::<f32>() / current.scores.len() as f32
    };

    entities.push(Entity {
        entity_type,
        value: value.to_string(),
        start_offset: base_offset + aligned_start,
        end_offset: base_offset + aligned_end,
        confidence,
        source: EntitySource::Onnx,
        model_name: Some(model_name.to_string()),
    });
}

fn align_entity_span_to_source(chunk: &str, start: usize, end: usize) -> (usize, usize) {
    if start >= end || start >= chunk.len() {
        return (start.min(chunk.len()), end.min(chunk.len()));
    }

    let mut aligned_start = start;
    let mut aligned_end = end.min(chunk.len());

    while aligned_start > 0 {
        let Some(prev_char) = chunk[..aligned_start].chars().next_back() else {
            break;
        };
        if !is_entity_token_char(prev_char) {
            break;
        }
        aligned_start -= prev_char.len_utf8();
    }

    while aligned_end < chunk.len() {
        let Some(next_char) = chunk[aligned_end..].chars().next() else {
            break;
        };
        if !is_entity_token_char(next_char) {
            break;
        }
        aligned_end += next_char.len_utf8();
    }

    (aligned_start, aligned_end)
}

fn is_entity_token_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '.' | '/' | '&' | '\'' | '’' | '-' | 'º' | 'ª')
}

/// Check whether a token can extend the current open entity span across a
/// character gap. Only subword continuation pieces (## prefix) are allowed to
/// bridge arbitrary gaps because they're never independent words — they must
/// attach to the preceding token. Word-start markers (▁, Ġ) and plain tokens
/// require an explicit joiner character in the gap to extend.
///
/// NOTE: Not currently used in `aggregate_simple_entities` (BIO logic handles
/// extension directly), but retained for potential future use in window-level
/// consolidation where span reconstruction across gaps is needed.
#[allow(dead_code)]
fn can_extend_entity(chunk: &str, current_end: usize, next_start: usize, piece: &str) -> bool {
    if is_continuation_piece(piece) {
        return true;
    }

    if next_start <= current_end {
        return true;
    }

    chunk
        .get(current_end..next_start)
        .is_some_and(|gap| gap.chars().all(is_entity_joiner))
}

#[allow(dead_code)]
fn is_entity_joiner(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '-' | '.' | '\'' | '’' | '"')
}

/// Returns true for subword continuation pieces (## prefix).
/// These glue to the previous token without a space.
fn is_continuation_piece(piece: &str) -> bool {
    piece.starts_with("##")
}

/// Returns true for SentencePiece / GPT-2 word-start markers (▁, Ġ).
/// These mark the beginning of a new word — a space should precede them
/// (unless they're the very first token in the reconstruction).
fn is_word_start_piece(piece: &str) -> bool {
    piece.starts_with('▁') || piece.starts_with('Ġ')
}

/// Returns true for any special subword marker (continuation or word-start).
/// Used for debugging and filtering; prefer `is_continuation_piece` or
/// `is_word_start_piece` for semantic decisions.
#[allow(dead_code)]
fn is_subword_marked(piece: &str) -> bool {
    is_continuation_piece(piece) || is_word_start_piece(piece)
}

fn reconstruct_from_pieces(pieces: &[String]) -> String {
    let mut out = String::new();
    let mut first_token = true;

    for piece in pieces {
        if piece == "[UNK]" {
            continue;
        }

        // Determine the normalized form (strip all subword markers)
        // and whether this piece starts a new word.
        let (normalized, starts_new_word) = if is_continuation_piece(piece) {
            // ##grano → continuation, glue to previous, no space
            (piece.trim_start_matches("##"), false)
        } else if is_word_start_piece(piece) {
            // ▁Manuel or ĠManuel → new word, add space unless first
            let stripped = piece.trim_start_matches('▁').trim_start_matches('Ġ');
            (stripped, true)
        } else {
            // Plain token like "Manuel" — also starts a new word
            (piece.as_str(), true)
        };

        if normalized.is_empty() {
            continue;
        }

        // All punctuation-only tokens are appended without a space
        let is_punct = normalized.chars().all(|ch| !ch.is_alphanumeric());

        if first_token || is_continuation_piece(piece) || is_punct {
            out.push_str(normalized);
            first_token = false;
        } else if starts_new_word {
            out.push(' ');
            out.push_str(normalized);
        } else {
            // Should not reach here, but safe fallback
            out.push_str(normalized);
        }
    }

    out
}

fn choose_best_surface_form(span_value: &str, reconstructed: &str) -> String {
    let span_clean = span_value.trim();
    let reconstructed_clean = reconstructed.trim();

    let span_score = surface_form_score(span_clean);
    let reconstructed_score = surface_form_score(reconstructed_clean);

    if reconstructed_score > span_score {
        reconstructed_clean.to_string()
    } else {
        span_clean.to_string()
    }
}

fn surface_form_score(value: &str) -> usize {
    if value.is_empty() {
        return 0;
    }

    if contains_suspicious_surface_artifacts(value) {
        return 0;
    }

    let mut score = value.chars().filter(|ch| ch.is_alphanumeric()).count();
    if !value.contains("[UNK]") {
        score += 50;
    }
    if value.contains(' ') {
        score += 10;
    }
    if value.chars().last().is_some_and(|ch| ch.is_alphanumeric()) {
        score += 5;
    }

    score
}

fn contains_suspicious_surface_artifacts(value: &str) -> bool {
    value.contains('�') || value.contains('Ã') || value.contains('Â') || value.contains("â€")
}

fn should_drop_entity_value(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.eq_ignore_ascii_case("[UNK]")
        || trimmed.contains("[UNK]")
        || trimmed.chars().all(|ch| !ch.is_alphanumeric())
        || trimmed.chars().filter(|ch| ch.is_alphanumeric()).count() <= 1
}

pub fn consolidate_onnx_entities(mut entities: Vec<Entity>) -> Vec<Entity> {
    entities.sort_by_key(|entity| (entity.start_offset, entity.end_offset));

    let mut consolidated: Vec<Entity> = Vec::new();
    for entity in entities {
        if let Some(prev) = consolidated.last_mut() {
            if same_family(&prev.entity_type, &entity.entity_type)
                && (overlaps(prev, &entity) || is_near_duplicate(prev, &entity))
            {
                *prev = prefer_more_complete_entity(prev.clone(), entity);
                continue;
            }
        }

        consolidated.push(entity);
    }

    consolidated
}

fn same_family(a: &EntityType, b: &EntityType) -> bool {
    match (a, b) {
        (EntityType::Organization, EntityType::Institution)
        | (EntityType::Institution, EntityType::Organization) => true,
        _ => a == b,
    }
}

fn overlaps(a: &Entity, b: &Entity) -> bool {
    a.start_offset < b.end_offset && b.start_offset < a.end_offset
}

fn is_near_duplicate(a: &Entity, b: &Entity) -> bool {
    let a_norm = normalize_entity_value(&a.value);
    let b_norm = normalize_entity_value(&b.value);
    let close_offsets =
        a.start_offset.abs_diff(b.start_offset) <= 24 || a.end_offset.abs_diff(b.end_offset) <= 24;

    close_offsets
        && (a_norm == b_norm
            || a_norm.starts_with(&b_norm)
            || b_norm.starts_with(&a_norm)
            || a_norm.contains(&b_norm)
            || b_norm.contains(&a_norm))
}

fn prefer_more_complete_entity(a: Entity, b: Entity) -> Entity {
    let a_score = completion_score(&a);
    let b_score = completion_score(&b);

    if b_score > a_score {
        b
    } else {
        a
    }
}

fn completion_score(entity: &Entity) -> usize {
    let mut score = surface_form_score(&entity.value);
    score += entity.end_offset.saturating_sub(entity.start_offset);
    if entity.confidence >= 0.9 {
        score += 5;
    }
    score
}

fn normalize_entity_value(value: &str) -> String {
    sanitize_entity_value(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn normalize_entity_type(entity_type: EntityType, value: &str) -> EntityType {
    match entity_type {
        EntityType::Organization if looks_like_institution(value) => EntityType::Institution,
        other => other,
    }
}

fn looks_like_institution(value: &str) -> bool {
    let normalized = value
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim();
    institution_keywords().iter().any(|keyword| {
        normalized
            .get(..keyword.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(keyword))
    })
}

fn institution_keywords() -> &'static [&'static str] {
    &[
        "Real",
        "Cabildo",
        "Iglesia",
        "Convento",
        "Universidad",
        "Audiencia",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nlp::ner::types::NerEngineKind;

    #[test]
    fn parse_bio_labels_for_supported_entity_types() {
        assert!(matches!(
            parse_bio_label("B-PER"),
            Some(DecodedTag {
                bio: BioTag::Begin,
                entity_type: EntityType::Person
            })
        ));
        assert!(matches!(
            parse_bio_label("I-PER"),
            Some(DecodedTag {
                bio: BioTag::Inside,
                entity_type: EntityType::Person
            })
        ));
        assert!(matches!(
            parse_bio_label("I-LOC"),
            Some(DecodedTag {
                bio: BioTag::Inside,
                entity_type: EntityType::Place
            })
        ));
        assert!(matches!(
            parse_bio_label("B-ORG"),
            Some(DecodedTag {
                bio: BioTag::Begin,
                entity_type: EntityType::Organization,
            })
        ));
        assert!(matches!(
            parse_bio_label("B-MISC"),
            Some(DecodedTag {
                bio: BioTag::Begin,
                entity_type: EntityType::Misc,
            })
        ));
        assert!(matches!(
            parse_bio_label("B-DATE"),
            Some(DecodedTag {
                bio: BioTag::Begin,
                entity_type: EntityType::Date,
            })
        ));
    }

    #[test]
    fn choose_best_surface_form_prefers_clean_source_span_over_corrupted_reconstruction() {
        let span = "Según ella";
        let reconstructed = "Seg�n ella";

        assert_eq!(choose_best_surface_form(span, reconstructed), "Según ella");
    }

    #[test]
    fn parse_bio_label_rejects_invalid_prefixes() {
        assert!(parse_bio_label("O").is_none());
        assert!(parse_bio_label("E-PER").is_none());
        assert!(parse_bio_label("PER").is_none());
        assert!(parse_bio_label("").is_none());
    }

    #[test]
    fn bio_begin_always_starts_new_entity() {
        // Two B-PER tokens should create TWO separate entities, not merge
        let entities = aggregate_simple_entities(
            "Juan Carlos",
            0,
            vec![
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Person,
                    start: 0,
                    end: 4,
                    score: 0.95,
                    piece: "Juan".to_string(),
                },
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Person,
                    start: 5,
                    end: 12,
                    score: 0.90,
                    piece: "Carlos".to_string(),
                },
            ],
            "ner",
        );

        assert_eq!(
            entities.len(),
            2,
            "B-PER followed by B-PER should create two separate entities, got {:?}",
            entities
        );
    }

    #[test]
    fn bio_inside_extends_matching_entity() {
        // B-PER followed by I-PER should merge into one entity
        let entities = aggregate_simple_entities(
            "Manuel Belgrano",
            0,
            vec![
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Person,
                    start: 0,
                    end: 6,
                    score: 0.97,
                    piece: "Manuel".to_string(),
                },
                TokenPrediction {
                    bio: BioTag::Inside,
                    entity_type: EntityType::Person,
                    start: 7,
                    end: 15,
                    score: 0.94,
                    piece: "Belgrano".to_string(),
                },
            ],
            "ner",
        );

        assert_eq!(
            entities.len(),
            1,
            "B-PER + I-PER should merge into one entity"
        );
        assert_eq!(entities[0].value, "Manuel Belgrano");
    }

    #[test]
    fn noisy_begin_tag_on_subword_is_repaired_into_same_entity() {
        let entities = aggregate_simple_entities(
            "Conservas Baltar S.A.I.C",
            0,
            vec![
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Organization,
                    start: 0,
                    end: 9,
                    score: 0.95,
                    piece: "Conserva".to_string(),
                },
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Organization,
                    start: 8,
                    end: 24,
                    score: 0.93,
                    piece: "##s".to_string(),
                },
            ],
            "ner",
        );

        assert_eq!(entities.len(), 1);
        assert!(entities[0].value.contains("Conservas"));
    }

    #[test]
    fn noisy_begin_tag_on_lowercase_fragment_is_repaired() {
        let entities = aggregate_simple_entities(
            "Agrupación 1 de Mayo",
            0,
            vec![
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Organization,
                    start: 0,
                    end: 3,
                    score: 0.92,
                    piece: "Agr".to_string(),
                },
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Organization,
                    start: 3,
                    end: 20,
                    score: 0.91,
                    piece: "upación".to_string(),
                },
            ],
            "ner",
        );

        assert_eq!(entities.len(), 1);
        assert!(entities[0].value.contains("Agrupación"));
    }

    #[test]
    fn bio_inside_mismatch_flushes_and_starts_new() {
        // B-LOC followed by I-PER is malformed — I-PER starts its own entity
        let entities = aggregate_simple_entities(
            "Buenos Aires Manuel",
            0,
            vec![
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Place,
                    start: 0,
                    end: 12,
                    score: 0.95,
                    piece: "Buenos Aires".to_string(),
                },
                TokenPrediction {
                    bio: BioTag::Inside,
                    entity_type: EntityType::Person,
                    start: 13,
                    end: 20,
                    score: 0.90,
                    piece: "Manuel".to_string(),
                },
            ],
            "ner",
        );

        assert_eq!(
            entities.len(),
            2,
            "B-LOC + I-PER mismatch should produce two entities"
        );
        assert_eq!(entities[0].entity_type, EntityType::Place);
        assert_eq!(entities[1].entity_type, EntityType::Person);
    }

    #[test]
    fn orphan_inside_tag_creates_standalone_entity() {
        // I-PER without a preceding B-PER starts a new entity
        let entities = aggregate_simple_entities(
            "Belgrano",
            10,
            vec![TokenPrediction {
                bio: BioTag::Inside,
                entity_type: EntityType::Person,
                start: 0,
                end: 8,
                score: 0.85,
                piece: "Belgrano".to_string(),
            }],
            "ner",
        );

        assert_eq!(
            entities.len(),
            1,
            "Orphan I-PER should still create an entity"
        );
        assert_eq!(entities[0].entity_type, EntityType::Person);
    }

    #[test]
    fn prune_window_edge_entities_drops_partial_entities_at_overlap_edges() {
        let mut entities = vec![
            Entity {
                entity_type: EntityType::Place,
                value: "DEL PL".to_string(),
                start_offset: 100,
                end_offset: 106,
                confidence: 0.88,
                source: EntitySource::Onnx,
                model_name: Some("ner".to_string()),
            },
            Entity {
                entity_type: EntityType::Place,
                value: "Mar del Plata".to_string(),
                start_offset: 120,
                end_offset: 133,
                confidence: 0.92,
                source: EntitySource::Onnx,
                model_name: Some("ner".to_string()),
            },
            Entity {
                entity_type: EntityType::Organization,
                value: "Cuerpo de Dele".to_string(),
                start_offset: 180,
                end_offset: 194,
                confidence: 0.80,
                source: EntitySource::Onnx,
                model_name: Some("ner".to_string()),
            },
        ];

        prune_window_edge_entities(&mut entities, 100, 194, false, false);

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].value, "Mar del Plata");
    }

    #[test]
    fn prune_window_edge_entities_keeps_edges_on_first_or_last_window() {
        let mut entities = vec![
            Entity {
                entity_type: EntityType::Place,
                value: "Mar del Plata".to_string(),
                start_offset: 0,
                end_offset: 13,
                confidence: 0.92,
                source: EntitySource::Onnx,
                model_name: Some("ner".to_string()),
            },
            Entity {
                entity_type: EntityType::Organization,
                value: "SOIP".to_string(),
                start_offset: 200,
                end_offset: 204,
                confidence: 0.89,
                source: EntitySource::Onnx,
                model_name: Some("ner".to_string()),
            },
        ];

        prune_window_edge_entities(&mut entities, 0, 204, true, true);

        assert_eq!(entities.len(), 2);
    }

    #[test]
    fn align_entity_span_to_source_preserves_full_numeric_suffix() {
        let text = "Córdoba 1338";
        let (start, end) = align_entity_span_to_source(text, 0, 11);
        assert_eq!(text.get(start..end), Some("Córdoba 1338"));
    }

    #[test]
    fn paragraph_spans_split_on_blank_lines() {
        let text = "Uno\nlinea\n\nDos\n\n  \nTres";
        let spans = paragraph_spans(text);
        let values = spans
            .into_iter()
            .filter_map(|(start, end)| text.get(start..end))
            .collect::<Vec<_>>();

        assert_eq!(values, vec!["Uno\nlinea\n", "Dos\n", "Tres"]);
    }

    #[test]
    fn token_piece_spans_follow_non_whitespace_plus_trailing_whitespace() {
        let chunk = "Conservas Baltar S.A.I.C  \nSOIP";
        let values = token_piece_spans(chunk)
            .into_iter()
            .filter_map(|(start, end)| chunk.get(start..end))
            .collect::<Vec<_>>();

        assert_eq!(values, vec!["Conservas ", "Baltar ", "S.A.I.C  \n", "SOIP"]);
    }

    #[test]
    fn organization_entities_with_colonial_titles_become_institutions() {
        assert_eq!(
            normalize_entity_type(EntityType::Organization, "Cabildo de Buenos Aires"),
            EntityType::Institution
        );
        assert_eq!(
            normalize_entity_type(EntityType::Organization, "Universidad de Córdoba"),
            EntityType::Institution
        );
        assert_eq!(
            normalize_entity_type(EntityType::Organization, "Compañía Mercantil"),
            EntityType::Organization
        );
    }

    #[test]
    fn institution_heuristic_does_not_panic_on_utf8_boundaries() {
        assert!(!looks_like_institution("Dirección General de Escuelas"));
        assert!(looks_like_institution("Universidad de Córdoba"));
        assert!(!looks_like_institution("Órgano Mercantil"));
    }

    #[test]
    fn can_extend_entity_across_spaces_and_subwords() {
        let chunk = "Manuel Bel##grano";
        // ## continuation always extends
        assert!(can_extend_entity(chunk, 10, 10, "##grano"));
        // Plain word with gap containing only whitespace — extend if joiner
        assert!(can_extend_entity(chunk, 6, 7, "Bel"));
        // Gap with non-joiner characters — don't extend
        assert!(!can_extend_entity(chunk, 6, 9, "Bel"));
    }

    #[test]
    fn can_extend_entity_distinguishes_word_starts_from_continuations() {
        let chunk = "San Martín";
        // ## pieces are continuations — always extend
        assert!(can_extend_entity(chunk, 3, 4, "##Martín"));
        // ▁ is a word-start, not a continuation — needs joiner gap to extend
        assert!(can_extend_entity(chunk, 3, 4, "▁Martín"));
        // Ġ is a word-start, same behavior as ▁
        assert!(can_extend_entity(chunk, 3, 4, "ĠMartín"));
        // Plain word — same as ▁/Ġ, needs joiner gap
        assert!(can_extend_entity(chunk, 3, 4, "Martín"));
        // Gap with non-joiner (punctuation) — don't extend ▁-marked word
        assert!(!can_extend_entity("foo,bar", 3, 5, "▁bar"));
    }

    #[test]
    fn reconstruct_from_wordpieces_merges_subwords() {
        let value = reconstruct_from_pieces(&[
            "Manuel".to_string(),
            "Bel".to_string(),
            "##grano".to_string(),
        ]);

        assert_eq!(value, "Manuel Belgrano");
    }

    #[test]
    fn reconstruct_from_pieces_sentencepiece_word_starts() {
        // ▁ marks word-start — should add space before (except first)
        let value = reconstruct_from_pieces(&[
            "▁Mar".to_string(),
            "▁del".to_string(),
            "▁Plata".to_string(),
        ]);

        assert_eq!(value, "Mar del Plata");
    }

    #[test]
    fn reconstruct_from_pieces_gpt2_bpe_word_starts() {
        // Ġ marks word-start (space in GPT-2 BPE) — same as ▁
        let value = reconstruct_from_pieces(&[
            "ĠDon".to_string(),
            "ĠManuel".to_string(),
            "ĠBelgrano".to_string(),
        ]);

        assert_eq!(value, "Don Manuel Belgrano");
    }

    #[test]
    fn reconstruct_from_mixed_wordpieces_and_word_starts() {
        // Real model output: ▁ for word start, ## for subword continuation
        let value = reconstruct_from_pieces(&[
            "▁Real".to_string(),
            "▁Audiencia".to_string(),
            "▁de".to_string(),
            "▁Char".to_string(),
            "##cas".to_string(),
        ]);

        assert_eq!(value, "Real Audiencia de Charcas");
    }

    #[test]
    fn reconstruct_from_punctuation_only_tokens() {
        // Punctuation tokens should append without extra spaces
        let value = reconstruct_from_pieces(&["▁Cabildo".to_string(), ",".to_string()]);

        // The comma should not get a space before it when appended after a word
        // Actually, in TokenPrediction land the punctuation token's offsets
        // determine where it goes, but in reconstruction we just concatenate.
        // Since "," is all non-alphanumeric, it gets appended directly.
        assert_eq!(value, "Cabildo,");
    }

    #[test]
    fn aggregate_simple_entities_merges_same_family_tokens_into_one_span() {
        let entities = aggregate_simple_entities(
            "Wilson Sons y Cía.",
            0,
            vec![
                TokenPrediction {
                    bio: BioTag::Begin,
                    entity_type: EntityType::Person,
                    start: 0,
                    end: 6,
                    score: 0.95,
                    piece: "W".to_string(),
                },
                TokenPrediction {
                    bio: BioTag::Inside,
                    entity_type: EntityType::Person,
                    start: 0,
                    end: 6,
                    score: 0.91,
                    piece: "##ilson".to_string(),
                },
                TokenPrediction {
                    bio: BioTag::Inside,
                    entity_type: EntityType::Person,
                    start: 7,
                    end: 11,
                    score: 0.93,
                    piece: "Son".to_string(),
                },
                TokenPrediction {
                    bio: BioTag::Inside,
                    entity_type: EntityType::Person,
                    start: 7,
                    end: 11,
                    score: 0.89,
                    piece: "##s".to_string(),
                },
            ],
            "ner",
        );

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].value, "Wilson Sons");
    }

    #[test]
    fn consolidate_onnx_entities_prefers_more_complete_overlap() {
        let entities = consolidate_onnx_entities(vec![
            Entity {
                entity_type: EntityType::Place,
                value: "MAR DEL PL".to_string(),
                start_offset: 0,
                end_offset: 10,
                confidence: 0.82,
                source: EntitySource::Onnx,
                model_name: Some("ner".to_string()),
            },
            Entity {
                entity_type: EntityType::Place,
                value: "Mar del Plata".to_string(),
                start_offset: 0,
                end_offset: 13,
                confidence: 0.91,
                source: EntitySource::Onnx,
                model_name: Some("ner".to_string()),
            },
        ]);

        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].value, "Mar del Plata");
    }

    #[test]
    fn softmax_confidence_is_normalized_probability() {
        let confidence = softmax_confidence(&[1.0, 3.0, 0.5], 1);
        assert!(
            confidence > 0.75,
            "expected high confidence, got {confidence}"
        );
        assert!(confidence < 1.0, "softmax confidence should stay below 1");
    }

    #[test]
    fn load_labels_falls_back_to_conll_order_without_config() {
        let labels = load_labels(Path::new("definitely-missing-dir"));
        assert_eq!(labels.len(), 9);
        assert_eq!(labels.last().map(String::as_str), Some("O"));
    }

    #[test]
    fn inspect_assets_reports_missing_required_files() {
        let report = OnnxNerEngine::inspect_assets(&NerConfig {
            engine: NerEngineKind::Hybrid,
            model_path: None,
            tokenizer_path: None,
            python_path: None,
            script_path: None,
            model_name: None,
            max_length: 256,
            stride: 32,
            score_threshold: 0.65,
        });

        assert!(!report.is_ready());
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("Model path")));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.contains("Tokenizer path")));
    }
}
