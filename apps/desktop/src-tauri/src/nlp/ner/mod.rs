pub mod hybrid;
pub mod merge;
pub mod onnx;
pub mod rule_based;
pub mod spacy;
pub mod types;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::nlp::text_provider;

use self::{
    hybrid::HybridNerEngine,
    onnx::{OnnxNerEngine, OnnxPreflightReport},
    rule_based::RuleBasedNerEngine,
    spacy::{SpacyNerEngine, SpacyPreflightReport},
    types::{sanitize_entity_value, Entity, NerConfig, NerEngine, NerEngineKind},
};

const MIN_ENTITY_CONFIDENCE: f32 = 0.89;

#[allow(unused_imports)]
pub use self::types::{EntitySource, EntityType};

pub fn log_startup_status(config: &NerConfig) {
    match config.engine {
        NerEngineKind::Spacy => {
            let report: SpacyPreflightReport = SpacyNerEngine::inspect_assets(config);
            report.log();
        }
        _ => {
            let report: OnnxPreflightReport = OnnxNerEngine::inspect_assets(config);
            report.log();
        }
    }
}

pub struct NerRegistry {
    config: NerConfig,
    rule_based: RuleBasedNerEngine,
    onnx: Option<OnnxNerEngine>,
    spacy: Option<SpacyNerEngine>,
}

impl NerRegistry {
    pub fn init(config: NerConfig) -> Self {
        let onnx = match OnnxNerEngine::init(&config) {
            Ok(engine) => {
                if matches!(config.engine, NerEngineKind::Onnx | NerEngineKind::Hybrid) {
                    eprintln!(
                        "[nlp/ner] ONNX engine ready: {} — hybrid/runtime fallback remains available.",
                        engine.model_name()
                    );
                }
                Some(engine)
            }
            Err(error) => {
                if matches!(config.engine, NerEngineKind::Onnx | NerEngineKind::Hybrid) {
                    eprintln!(
                        "[nlp/ner] ONNX engine unavailable: {error} — using rule-based fallback."
                    );
                }
                None
            }
        };

        let spacy = match SpacyNerEngine::init(&config) {
            Ok(engine) => {
                if matches!(config.engine, NerEngineKind::Spacy) {
                    eprintln!(
                        "[nlp/ner] spaCy engine ready: {} — rule-based fallback remains available.",
                        engine.model_name()
                    );
                }
                Some(engine)
            }
            Err(error) => {
                if matches!(config.engine, NerEngineKind::Spacy) {
                    eprintln!(
                        "[nlp/ner] spaCy engine unavailable: {error} — using rule-based fallback."
                    );
                }
                None
            }
        };

        Self {
            config,
            rule_based: RuleBasedNerEngine::new(),
            onnx,
            spacy,
        }
    }

    pub fn extract(&self, text: &str) -> Result<Vec<Entity>, String> {
        match self.config.engine {
            NerEngineKind::RuleBased => self.rule_based.extract(text),
            NerEngineKind::Onnx => match self.onnx.as_ref() {
                Some(onnx) => onnx.extract(text),
                None => self.rule_based.extract(text),
            },
            NerEngineKind::Hybrid => HybridNerEngine::new(
                &self.rule_based,
                self.onnx.as_ref(),
                self.spacy.as_ref(),
            )
            .extract(text),
            NerEngineKind::Spacy => match self.spacy.as_ref() {
                Some(spacy) => spacy.extract(text),
                None => self.rule_based.extract(text),
            },
        }
    }

}

#[allow(dead_code)]
pub fn extract_entities(text: &str) -> Vec<Entity> {
    RuleBasedNerEngine::new()
        .extract(text)
        .expect("rule-based NER should extract entities")
}

#[derive(Clone)]
pub struct EntityExtractionBatch {
    pub text: String,
    #[allow(dead_code)] // Future: will be used for entity consolidation/review pipeline
    pub protected_entities: Vec<Entity>,
    pub entities: Vec<Entity>,
}

#[allow(dead_code)] // Future: LLM entity review pipeline (not yet wired)
#[derive(Serialize)]
struct EntityReviewCandidate<'a> {
    value: &'a str,
    #[serde(rename = "type")]
    entity_type: &'a str,
    confidence: f32,
}

#[allow(dead_code)] // Future: LLM entity review pipeline (not yet wired)
#[derive(Deserialize)]
struct ReviewedEntity {
    #[serde(default, alias = "entity", alias = "text")]
    value: String,
    #[serde(default, alias = "entity_type")]
    #[serde(rename = "type")]
    entity_type: String,
    #[serde(default)]
    confidence: Option<f32>,
}

pub fn extract_candidates_for_item(
    conn: &Connection,
    item_id: &str,
    registry: &NerRegistry,
) -> Result<EntityExtractionBatch, String> {
    let text = text_provider::get_item_text(conn, item_id)?;
    let protected_entities = load_protected_entities(conn, item_id)?;
    let entities = collect_candidate_entities(&text, &protected_entities, registry)?;

    Ok(EntityExtractionBatch {
        text,
        protected_entities,
        entities,
    })
}

pub fn extract_candidates_for_asset(
    conn: &Connection,
    item_id: &str,
    asset_id: &str,
    registry: &NerRegistry,
) -> Result<EntityExtractionBatch, String> {
    let text = text_provider::get_asset_text(conn, asset_id)?;
    let protected_entities = load_protected_entities(conn, item_id)?;
    let entities = collect_candidate_entities(&text, &protected_entities, registry)?;

    Ok(EntityExtractionBatch {
        text,
        protected_entities,
        entities,
    })
}

#[allow(dead_code)] // Future: LLM entity review pipeline (not yet wired)
pub fn serialize_review_candidates(entities: &[Entity]) -> Result<String, String> {
    let payload = entities
        .iter()
        .map(|entity| EntityReviewCandidate {
            value: entity.value.as_str(),
            entity_type: entity.entity_type.as_str(),
            confidence: entity.confidence,
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&payload)
        .map_err(|e| format!("Failed to serialize entity review candidates: {e}"))
}

#[allow(dead_code)] // Future: LLM entity review pipeline (not yet wired)
pub fn apply_llm_review(
    text: &str,
    candidate_entities: &[Entity],
    protected_entities: &[Entity],
    raw_review_json: &str,
) -> Result<Vec<Entity>, String> {
    let content = strip_markdown_fences(raw_review_json);
    let start = content.find('[').or_else(|| content.find('{'));
    let end = content.rfind(']').or_else(|| content.rfind('}'));

    let Some(start_idx) = start else {
        return Err("LLM entity review did not return JSON content".to_string());
    };
    let Some(end_idx) = end else {
        return Err("LLM entity review did not return a closed JSON payload".to_string());
    };

    let slice = &content[start_idx..=end_idx];
    let reviewed_entities = if slice.starts_with('[') {
        serde_json::from_str::<Vec<ReviewedEntity>>(slice)
            .map_err(|e| format!("Failed to parse LLM entity review array: {e}"))?
    } else {
        vec![
            serde_json::from_str::<ReviewedEntity>(slice)
                .map_err(|e| format!("Failed to parse LLM entity review object: {e}"))?,
        ]
    };

    let mut deduped_keys = std::collections::HashSet::new();
    let mut final_entities = Vec::new();

    for reviewed in reviewed_entities {
        let value = sanitize_entity_value(&reviewed.value);
        if value.is_empty() {
            continue;
        }

        let Some(entity_type) = parse_entity_type_alias(&reviewed.entity_type) else {
            continue;
        };

        let confidence = reviewed.confidence.unwrap_or(0.95).clamp(0.0, 1.0);
        let mut entity = Entity {
            entity_type,
            value,
            start_offset: 0,
            end_offset: 0,
            confidence,
            source: EntitySource::Llm,
            model_name: Some("gemma-4-E2B-it-Q4_K_M".to_string()),
        };

        if let Some(existing) = candidate_entities.iter().find(|candidate| {
            same_entity_family(&candidate.entity_type, &entity.entity_type)
                && normalize_entity_value(&candidate.value) == normalize_entity_value(&entity.value)
        }) {
            entity.start_offset = existing.start_offset;
            entity.end_offset = existing.end_offset;
            entity.confidence = entity.confidence.max(existing.confidence);
        } else if let Some((start_offset, end_offset)) = find_entity_span(text, &entity.value) {
            entity.start_offset = start_offset;
            entity.end_offset = end_offset;
        }

        if is_suppressed_by_protected(&entity, protected_entities) {
            continue;
        }

        let dedupe_key = (
            normalize_entity_value(&entity.value),
            entity.entity_type.as_str().to_string(),
        );
        if deduped_keys.insert(dedupe_key) {
            final_entities.push(entity);
        }
    }

    Ok(final_entities)
}

pub fn persist_entities_for_item(
    conn: &Connection,
    item_id: &str,
    entities: &[Entity],
) -> Result<(), String> {
    delete_automatic_entities(conn, item_id)?;
    insert_entities_for_item(conn, item_id, entities)
}

pub fn persist_entities_for_asset(
    conn: &Connection,
    item_id: &str,
    asset_id: &str,
    entities: &[Entity],
) -> Result<(), String> {
    delete_automatic_entities_for_asset(conn, item_id, asset_id)?;
    insert_entities_for_asset(conn, item_id, asset_id, entities)
}

#[allow(dead_code)]
pub fn extract_and_store(
    conn: &Connection,
    item_id: &str,
    registry: &NerRegistry,
) -> Result<(), String> {
    let batch = extract_candidates_for_item(conn, item_id, registry)?;

    if batch.text.trim().is_empty() {
        eprintln!("[nlp/ner] Extract skipped: item_id={}, no text available", item_id);
    }

    persist_entities_for_item(conn, item_id, &batch.entities)
}

/// Extract entities for a single asset/page (asset-level NER).
///
/// Similar to `extract_and_store`, but only processes the text for the given
/// `asset_id` and stores entities with both `item_id` and `asset_id` for
/// per-page filtering in the UI.
#[allow(dead_code)]
pub fn extract_and_store_for_asset(
    conn: &Connection,
    item_id: &str,
    asset_id: &str,
    registry: &NerRegistry,
) -> Result<(), String> {
    let batch = extract_candidates_for_asset(conn, item_id, asset_id, registry)?;

    if batch.text.trim().is_empty() {
        eprintln!("[nlp/ner] Asset-level extract skipped: asset_id={}, no text available", asset_id);
    }

    persist_entities_for_asset(conn, item_id, asset_id, &batch.entities)
}

fn collect_candidate_entities(
    text: &str,
    protected_entities: &[Entity],
    registry: &NerRegistry,
) -> Result<Vec<Entity>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    registry
        .extract(text)
        .map(|entities| {
            entities
                .into_iter()
                .filter(|entity| entity.confidence > MIN_ENTITY_CONFIDENCE)
                .filter(|entity| !is_suppressed_by_protected(entity, protected_entities))
                .collect::<Vec<_>>()
        })
}

fn insert_entities_for_item(conn: &Connection, item_id: &str, entities: &[Entity]) -> Result<(), String> {
    for entity in entities {
        conn.execute(
            r#"
            INSERT INTO entities (
                id, item_id, entity_type, value, start_offset, end_offset,
                confidence, source, model_name, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                uuid::Uuid::new_v4().to_string(),
                item_id,
                entity.entity_type.as_str(),
                entity.value.as_str(),
                entity.start_offset as i64,
                entity.end_offset as i64,
                entity.confidence as f64,
                entity.source.as_str(),
                entity.model_name.clone(),
                now_millis(),
            ],
        )
        .map_err(|e| format!("Failed to insert entity: {e}"))?;
    }

    Ok(())
}

fn insert_entities_for_asset(
    conn: &Connection,
    item_id: &str,
    asset_id: &str,
    entities: &[Entity],
) -> Result<(), String> {
    for entity in entities {
        conn.execute(
            r#"
            INSERT INTO entities (
                id, item_id, asset_id, entity_type, value, start_offset, end_offset,
                confidence, source, model_name, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                uuid::Uuid::new_v4().to_string(),
                item_id,
                asset_id,
                entity.entity_type.as_str(),
                entity.value.as_str(),
                entity.start_offset as i64,
                entity.end_offset as i64,
                entity.confidence as f64,
                entity.source.as_str(),
                entity.model_name.clone(),
                now_millis(),
            ],
        )
        .map_err(|e| format!("Failed to insert entity: {e}"))?;
    }

    Ok(())
}

fn delete_automatic_entities(conn: &Connection, item_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM entities WHERE item_id = ?1 AND COALESCE(source, '') NOT IN ('manual', 'manual_deleted')",
        params![item_id],
    )
    .map_err(|e| format!("Failed to delete automatic entities: {e}"))?;
    Ok(())
}

/// Delete automatic entities for a specific asset, preserving manual entities
/// and entities that belong to other assets or the item level.
fn delete_automatic_entities_for_asset(conn: &Connection, item_id: &str, asset_id: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM entities WHERE item_id = ?1 AND asset_id = ?2 AND COALESCE(source, '') NOT IN ('manual', 'manual_deleted')",
        params![item_id, asset_id],
    )
    .map_err(|e| format!("Failed to delete automatic entities for asset: {e}"))?;
    Ok(())
}

fn load_protected_entities(conn: &Connection, item_id: &str) -> Result<Vec<Entity>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT entity_type, value, start_offset, end_offset, confidence, source, model_name
            FROM entities
            WHERE item_id = ?1 AND COALESCE(source, '') IN ('manual', 'manual_deleted')
            "#,
        )
        .map_err(|e| format!("Failed to prepare protected entity query: {e}"))?;

    let rows = stmt
        .query_map(params![item_id], |row| {
            let entity_type_str: String = row.get(0)?;
            let entity_type = parse_entity_type(&entity_type_str).unwrap_or(EntityType::Misc);
            let source_str: Option<String> = row.get(5)?;
            let source = match source_str.as_deref() {
                Some("spacy") => EntitySource::Spacy,
                Some("onnx") => EntitySource::Onnx,
                _ => EntitySource::RuleBased,
            };

            Ok(Entity {
                entity_type,
                value: row.get(1)?,
                start_offset: row.get::<_, i64>(2)?.max(0) as usize,
                end_offset: row.get::<_, i64>(3)?.max(0) as usize,
                confidence: row.get::<_, f64>(4)? as f32,
                source,
                model_name: row.get(6)?,
            })
        })
        .map_err(|e| format!("Failed to query protected entities: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect protected entities: {e}"))
}

#[allow(dead_code)] // Future: used by apply_llm_review (not yet wired)
fn strip_markdown_fences(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let without_opening = trimmed
        .strip_prefix("```")
        .unwrap_or(trimmed)
        .trim_start_matches("json")
        .trim_start_matches("JSON")
        .trim();

    without_opening
        .strip_suffix("```")
        .unwrap_or(without_opening)
        .trim()
        .to_string()
}

fn parse_entity_type(value: &str) -> Option<EntityType> {
    match value {
        "person" => Some(EntityType::Person),
        "place" => Some(EntityType::Place),
        "date" => Some(EntityType::Date),
        "institution" => Some(EntityType::Institution),
        "organization" => Some(EntityType::Organization),
        "misc" => Some(EntityType::Misc),
        _ => None,
    }
}

#[allow(dead_code)] // Future: used by apply_llm_review (not yet wired)
fn parse_entity_type_alias(value: &str) -> Option<EntityType> {
    match value.trim().to_lowercase().as_str() {
        "person" | "persona" => Some(EntityType::Person),
        "place" | "location" | "lugar" => Some(EntityType::Place),
        "date" | "fecha" => Some(EntityType::Date),
        "institution" | "institucion" | "institución" => Some(EntityType::Institution),
        "organization" | "organizacion" | "organización" => Some(EntityType::Organization),
        "misc" | "other" | "otro" => Some(EntityType::Misc),
        _ => parse_entity_type(value.trim()),
    }
}

#[allow(dead_code)] // Future: used by apply_llm_review (not yet wired)
fn find_entity_span(text: &str, value: &str) -> Option<(usize, usize)> {
    let needle = value.trim();
    if needle.is_empty() {
        return None;
    }

    let haystack_lower = text.to_lowercase();
    let needle_lower = needle.to_lowercase();
    let byte_start = haystack_lower.find(&needle_lower)?;
    let byte_end = byte_start + needle_lower.len();
    Some((
        text[..byte_start].chars().count(),
        text[..byte_end].chars().count(),
    ))
}

fn is_suppressed_by_protected(candidate: &Entity, protected_entities: &[Entity]) -> bool {
    protected_entities.iter().any(|protected| {
        same_entity_family(&candidate.entity_type, &protected.entity_type)
            && (same_normalized_value(candidate, protected) || spans_overlap(candidate, protected))
    })
}

fn same_entity_family(a: &EntityType, b: &EntityType) -> bool {
    match (a, b) {
        (EntityType::Organization, EntityType::Institution)
        | (EntityType::Institution, EntityType::Organization) => true,
        _ => a == b,
    }
}

fn same_normalized_value(a: &Entity, b: &Entity) -> bool {
    normalize_entity_value(&a.value) == normalize_entity_value(&b.value)
}

fn normalize_entity_value(value: &str) -> String {
    sanitize_entity_value(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn spans_overlap(a: &Entity, b: &Entity) -> bool {
    a.start_offset < b.end_offset && b.start_offset < a.end_offset
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn setup_ner_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");

        conn.execute_batch(
            r#"
            CREATE TABLE items (
              id TEXT PRIMARY KEY,
              collection_id TEXT,
              title TEXT NOT NULL,
              metadata TEXT
            );

            CREATE TABLE assets (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              path TEXT NOT NULL,
              type TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );

            CREATE TABLE extractions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT,
              created_at INTEGER NOT NULL
            );

            CREATE TABLE transcriptions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT NOT NULL,
              language TEXT,
              duration_ms INTEGER,
              model TEXT NOT NULL,
              segments TEXT,
              confidence REAL,
              created_at INTEGER NOT NULL
            );

            CREATE TABLE entities (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              entity_type TEXT NOT NULL,
              value TEXT NOT NULL,
              start_offset INTEGER NOT NULL,
              end_offset INTEGER NOT NULL,
              confidence REAL NOT NULL,
              source TEXT,
              model_name TEXT,
              created_at INTEGER NOT NULL
            );
            "#,
        )
        .expect("NER test schema should be created");

        conn
    }

    fn seed_ner_item(conn: &Connection, item_id: &str, asset_id: &str) {
        conn.execute(
            "INSERT INTO items(id, collection_id, title, metadata) VALUES (?1, ?2, ?3, ?4)",
            params![item_id, "col-1", "Test Item", "{}"],
        )
        .expect("item insert should succeed");

        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![asset_id, item_id, "audio.mp3", "audio", 1_i64],
        )
        .expect("asset insert should succeed");
    }

    fn seed_ner_extraction(
        conn: &Connection,
        ext_id: &str,
        asset_id: &str,
        text: &str,
        created_at: i64,
    ) {
        conn.execute(
            "INSERT INTO extractions(id, asset_id, text_content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![ext_id, asset_id, text, created_at],
        )
        .expect("extraction insert should succeed");
    }

    fn seed_ner_transcription(
        conn: &Connection,
        trans_id: &str,
        asset_id: &str,
        text: &str,
        created_at: i64,
    ) {
        conn.execute(
            "INSERT INTO transcriptions(id, asset_id, text_content, language, duration_ms, model, segments, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![trans_id, asset_id, text, "es", 1000_i64, "base", "[]", 0.9_f64, created_at],
        )
        .expect("transcription insert should succeed");
    }

    fn seed_entity(
        conn: &Connection,
        id: &str,
        item_id: &str,
        entity_type: &str,
        value: &str,
        start_offset: i64,
        end_offset: i64,
        confidence: f64,
        source: &str,
    ) {
        conn.execute(
            "INSERT INTO entities(id, item_id, entity_type, value, start_offset, end_offset, confidence, source, model_name, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                item_id,
                entity_type,
                value,
                start_offset,
                end_offset,
                confidence,
                source,
                Option::<String>::None,
                1_i64
            ],
        )
        .expect("entity insert should succeed");
    }

    fn rule_based_registry() -> NerRegistry {
        NerRegistry::init(NerConfig {
            engine: NerEngineKind::RuleBased,
            model_path: None,
            tokenizer_path: None,
            python_path: None,
            script_path: None,
            model_name: None,
            max_length: 256,
            stride: 32,
            score_threshold: 0.65,
        })
    }

    #[test]
    fn hybrid_registry_falls_back_to_rule_based_when_onnx_is_unavailable() {
        let registry = NerRegistry::init(NerConfig {
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

        let entities = registry
            .extract("Don Manuel Belgrano en la ciudad de Buenos Aires")
            .expect("hybrid registry should fall back to rule-based NER");

        assert!(
            entities.iter().any(|entity| entity.entity_type == EntityType::Person),
            "fallback should still detect person entities"
        );
    }

    #[test]
    fn extract_and_store_detects_entities_from_transcription_only() {
        let conn = setup_ner_db();
        let registry = rule_based_registry();
        seed_ner_item(&conn, "item-trans-ner", "asset-trans-ner");

        seed_ner_transcription(
            &conn,
            "trans-ner-1",
            "asset-trans-ner",
            "Don Manuel Belgrano y Doña Juana Azurduy en la ciudad de Buenos Aires",
            10_i64,
        );

        extract_and_store(&conn, "item-trans-ner", &registry)
            .expect("extract_and_store should succeed");

        let entity_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1",
                params!["item-trans-ner"],
                |row| row.get(0),
            )
            .expect("entity count should be queryable");

        assert!(
            entity_count > 0,
            "NER should detect entities from transcription-only text, found {entity_count}"
        );

        let person_values: Vec<String> = conn
            .prepare("SELECT value FROM entities WHERE item_id = ?1 AND entity_type = 'person'")
            .unwrap()
            .query_map(params!["item-trans-ner"], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(
            person_values.iter().any(|v| v.contains("Manuel Belgrano")),
            "Expected 'Don Manuel Belgrano' entity from transcription text, got: {person_values:?}"
        );
    }

    #[test]
    fn extract_and_store_detects_entities_from_combined_text() {
        let conn = setup_ner_db();
        let registry = rule_based_registry();
        seed_ner_item(&conn, "item-combo-ner", "asset-combo-ner");

        seed_ner_extraction(
            &conn,
            "ext-combo-ner",
            "asset-combo-ner",
            "Documento colonial.",
            5_i64,
        );

        seed_ner_transcription(
            &conn,
            "trans-combo-ner",
            "asset-combo-ner",
            "Don San Martín fue gobernador de Cuyo.",
            10_i64,
        );

        extract_and_store(&conn, "item-combo-ner", &registry)
            .expect("extract_and_store should succeed");

        let entity_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1",
                params!["item-combo-ner"],
                |row| row.get(0),
            )
            .expect("entity count should be queryable");

        assert!(
            entity_count > 0,
            "NER should detect entities from combined extraction + transcription text"
        );
    }

    #[test]
    fn extract_and_store_deletes_old_entities_when_text_is_empty() {
        let conn = setup_ner_db();
        let registry = rule_based_registry();
        seed_ner_item(&conn, "item-empty-ner", "asset-empty-ner");

        seed_entity(
            &conn,
            "entity-stale",
            "item-empty-ner",
            "person",
            "Stale Entity",
            0,
            12,
            1.0,
            "rule_based",
        );

        extract_and_store(&conn, "item-empty-ner", &registry)
            .expect("extract_and_store should succeed");

        let entity_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1",
                params!["item-empty-ner"],
                |row| row.get(0),
            )
            .expect("entity count should be queryable");

        assert_eq!(
            entity_count, 0,
            "Old entities should be deleted when no text is available"
        );
    }

    #[test]
    fn extract_and_store_preserves_manual_entities_on_reextract() {
        let conn = setup_ner_db();
        let registry = rule_based_registry();
        seed_ner_item(&conn, "item-manual-ner", "asset-manual-ner");

        seed_ner_transcription(
            &conn,
            "trans-manual-ner",
            "asset-manual-ner",
            "Don Manuel Belgrano en la ciudad de Buenos Aires",
            10_i64,
        );

        seed_entity(
            &conn,
            "entity-manual",
            "item-manual-ner",
            "person",
            "Don Manuel Belgrano",
            0,
            20,
            1.0,
            "manual",
        );

        extract_and_store(&conn, "item-manual-ner", &registry)
            .expect("extract_and_store should preserve manual entities");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1 AND value = ?2",
                params!["item-manual-ner", "Don Manuel Belgrano"],
                |row| row.get(0),
            )
            .expect("manual entity count should be queryable");

        assert_eq!(count, 1, "manual entity should survive and suppress regenerated duplicate");
    }

    #[test]
    fn extract_and_store_respects_manual_deleted_tombstones() {
        let conn = setup_ner_db();
        let registry = rule_based_registry();
        seed_ner_item(&conn, "item-deleted-ner", "asset-deleted-ner");

        seed_ner_transcription(
            &conn,
            "trans-deleted-ner",
            "asset-deleted-ner",
            "Don Manuel Belgrano en la ciudad de Buenos Aires",
            10_i64,
        );

        seed_entity(
            &conn,
            "entity-tombstone",
            "item-deleted-ner",
            "person",
            "Don Manuel Belgrano",
            0,
            20,
            1.0,
            "manual_deleted",
        );

        extract_and_store(&conn, "item-deleted-ner", &registry)
            .expect("extract_and_store should respect tombstones");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1 AND value = ?2 AND source != 'manual_deleted'",
                params!["item-deleted-ner", "Don Manuel Belgrano"],
                |row| row.get(0),
            )
            .expect("suppressed entity count should be queryable");

        assert_eq!(count, 0, "manual_deleted tombstone should block regenerated entity");
    }

    #[test]
    fn extract_and_store_keeps_manual_entities_when_text_is_empty() {
        let conn = setup_ner_db();
        let registry = rule_based_registry();
        seed_ner_item(&conn, "item-empty-manual", "asset-empty-manual");

        seed_entity(
            &conn,
            "entity-manual-empty",
            "item-empty-manual",
            "organization",
            "SOIP",
            0,
            4,
            1.0,
            "manual",
        );

        extract_and_store(&conn, "item-empty-manual", &registry)
            .expect("extract_and_store should preserve manual rows when text is empty");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1 AND source = 'manual'",
                params!["item-empty-manual"],
                |row| row.get(0),
            )
            .expect("manual entity count should be queryable");

        assert_eq!(count, 1);
    }
}
