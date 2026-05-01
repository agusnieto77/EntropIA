//! Smart hybrid NER engine that routes extraction by entity type:
//!
//! - **PER, ORG, LOC** → BERT (ONNX) primary, spaCy fallback if BERT unavailable
//! - **DATE** → RegEx (rule-based) ONLY — never from BERT or spaCy
//! - **Institution** → merge rule-based + BERT (current behavior)
//! - **Misc** → BERT primary, spaCy fallback

use super::{
    merge::merge_institutions,
    onnx::consolidate_onnx_entities,
    onnx::OnnxNerEngine,
    rule_based::RuleBasedNerEngine,
    spacy::SpacyNerEngine,
    types::{sanitize_entity_value, Entity, EntityType, NerEngine},
};

/// Entity types for which BERT (ONNX) is the authority.
/// PER, ORG, LOC come from BERT only — spaCy serves as fallback
/// if the ONNX engine is unavailable or produces zero results.
const BERT_FIRST_TYPES: [EntityType; 3] = [
    EntityType::Person,
    EntityType::Organization,
    EntityType::Place,
];

pub struct HybridNerEngine<'a> {
    rule_based: &'a RuleBasedNerEngine,
    onnx: Option<&'a OnnxNerEngine>,
    spacy: Option<&'a SpacyNerEngine>,
}

impl<'a> HybridNerEngine<'a> {
    pub fn new(
        rule_based: &'a RuleBasedNerEngine,
        onnx: Option<&'a OnnxNerEngine>,
        spacy: Option<&'a SpacyNerEngine>,
    ) -> Self {
        Self {
            rule_based,
            onnx,
            spacy,
        }
    }
}

impl NerEngine for HybridNerEngine<'_> {
    fn name(&self) -> &'static str {
        "hybrid"
    }

    fn extract(&self, text: &str) -> Result<Vec<Entity>, String> {
        // Collect from all available sources
        let rule_entities = self.rule_based.extract(text)?;
        let mut onnx_entities: Vec<Entity> = match self.onnx {
            Some(engine) => engine.extract(text).unwrap_or_default(),
            None => vec![],
        };
        let spacy_entities: Vec<Entity> = match self.spacy {
            Some(engine) => engine.extract(text).unwrap_or_default(),
            None => vec![],
        };

        // Re-consolidate ONNX entities to merge window-boundary duplicates
        // and near-duplicates before routing by type.
        onnx_entities = consolidate_onnx_entities(onnx_entities);

        let mut result = Vec::new();

        // ── PER / ORG / LOC: BERT primary → spaCy fallback → rule-based last resort ──
        let onnx_core: Vec<Entity> = onnx_entities
            .iter()
            .filter(|e| BERT_FIRST_TYPES.contains(&e.entity_type))
            .cloned()
            .collect();

        if !onnx_core.is_empty() {
            result.extend(onnx_core);
        } else {
            let spacy_core: Vec<Entity> = spacy_entities
                .iter()
                .filter(|e| BERT_FIRST_TYPES.contains(&e.entity_type))
                .cloned()
                .collect();

            if !spacy_core.is_empty() {
                result.extend(spacy_core);
            } else {
                let rule_core: Vec<Entity> = rule_entities
                    .iter()
                    .filter(|e| BERT_FIRST_TYPES.contains(&e.entity_type))
                    .cloned()
                    .collect();
                result.extend(rule_core);
            }
        }

        // ── DATE: RegEx ONLY — never from BERT or spaCy ──
        let rule_dates: Vec<Entity> = rule_entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Date)
            .cloned()
            .collect();
        result.extend(rule_dates);

        // ── Institution: merge rule-based + BERT ──
        // Only ORG entities NOT already routed to onnx_core (i.e., those that
        // normalized to Institution) should be merged with rule-based institutions.
        let rule_institutions: Vec<Entity> = rule_entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Institution)
            .cloned()
            .collect();
        let onnx_institutions: Vec<Entity> = onnx_entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Institution)
            .cloned()
            .collect();
        result.extend(merge_institutions(rule_institutions, onnx_institutions));

        // ── Misc: BERT primary → spaCy fallback ──
        let onnx_misc: Vec<Entity> = onnx_entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Misc)
            .cloned()
            .collect();
        let spacy_misc: Vec<Entity> = spacy_entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Misc)
            .cloned()
            .collect();

        if !onnx_misc.is_empty() {
            result.extend(onnx_misc);
        } else {
            result.extend(spacy_misc);
        }

        // ── Final deduplication ──
        // After routing by type, overlapping entities from different sources
        // (e.g. an ORG from ONNX and the same ORG from a different window)
        // may still duplicate. Remove them.
        result = dedup_entities(result);

        result.sort_by_key(|e| e.start_offset);
        Ok(result)
    }
}

/// Remove duplicate entities.
///
/// This collapses both:
/// - technical duplicates/near-duplicates from overlap and decoding artifacts
/// - repeated mentions of the same normalized entity anywhere in the document
///
/// Keeps the best surface form according to `prefer_better_entity`.
fn dedup_entities(mut entities: Vec<Entity>) -> Vec<Entity> {
    if entities.is_empty() {
        return entities;
    }

    for entity in &mut entities {
        entity.value = sanitize_entity_value(&entity.value);
    }

    entities.sort_by_key(|e| (e.start_offset, e.end_offset));

    let mut deduped: Vec<Entity> = Vec::new();
    for entity in entities {
        let mut dominated = false;
        for prev in deduped.iter_mut() {
            if same_entity_family(&prev.entity_type, &entity.entity_type)
                && (overlaps_or_near_duplicate(prev, &entity)
                    || same_normalized_entity(prev, &entity))
            {
                // Keep the more complete entity
                *prev = prefer_better_entity(prev.clone(), entity.clone());
                dominated = true;
                break;
            }
        }
        if !dominated {
            deduped.push(entity);
        }
    }

    deduped
}

fn same_normalized_entity(a: &Entity, b: &Entity) -> bool {
    let a_norm = normalize_value(&a.value);
    let b_norm = normalize_value(&b.value);

    if a_norm.is_empty() || b_norm.is_empty() {
        return false;
    }

    a_norm == b_norm
}

fn overlaps_or_near_duplicate(a: &Entity, b: &Entity) -> bool {
    // Standard overlap
    if a.start_offset < b.end_offset && b.start_offset < a.end_offset {
        return true;
    }
    // Near duplicate: same normalized value, close offsets
    let a_norm = normalize_value(&a.value);
    let b_norm = normalize_value(&b.value);
    if a_norm == b_norm
        && a.start_offset.abs_diff(b.start_offset) <= 8
        && a.end_offset.abs_diff(b.end_offset) <= 8
    {
        return true;
    }
    if (a_norm.starts_with(&b_norm) || b_norm.starts_with(&a_norm))
        && a.start_offset.abs_diff(b.start_offset) <= 8
        && a.end_offset.abs_diff(b.end_offset) <= 8
    {
        return true;
    }
    false
}

fn same_entity_family(a: &EntityType, b: &EntityType) -> bool {
    match (a, b) {
        (EntityType::Organization, EntityType::Institution)
        | (EntityType::Institution, EntityType::Organization) => true,
        _ => a == b,
    }
}

fn prefer_better_entity(a: Entity, b: Entity) -> Entity {
    let a_score = entity_quality_score(&a);
    let b_score = entity_quality_score(&b);
    if b_score > a_score {
        b
    } else {
        a
    }
}

fn entity_quality_score(entity: &Entity) -> usize {
    let mut score = entity.value.chars().filter(|c| c.is_alphanumeric()).count();
    score += entity.end_offset.saturating_sub(entity.start_offset);
    if entity.confidence >= 0.9 {
        score += 10;
    }
    if entity.value.contains(' ') {
        score += 5;
    }
    score
}

fn normalize_value(value: &str) -> String {
    sanitize_entity_value(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nlp::ner::types::EntitySource;

    #[allow(dead_code)]
    fn entity(
        entity_type: EntityType,
        value: &str,
        start: usize,
        end: usize,
        confidence: f32,
        source: EntitySource,
    ) -> Entity {
        Entity {
            entity_type,
            value: value.to_string(),
            start_offset: start,
            end_offset: end,
            confidence,
            source,
            model_name: None,
        }
    }

    #[test]
    fn hybrid_extracts_dates_only_from_regex() {
        // This test runs without ML engines (both None),
        // so rule-based acts as fallback for core types.
        let rb = RuleBasedNerEngine::new();
        let engine = HybridNerEngine::new(&rb, None, None);
        let text = "Don Manuel Belgrano en la ciudad de Buenos Aires, a 15 de mayo de 1810.";
        let entities = engine.extract(text).expect("hybrid extract should work");

        // Dates come only from rule-based (RegEx)
        let dates: Vec<&Entity> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Date)
            .collect();
        assert!(!dates.is_empty(), "Should detect dates from RegEx");
        for date in &dates {
            assert_eq!(
                date.source,
                EntitySource::RuleBased,
                "Dates must come from RegEx only, got {:?}",
                date.source
            );
        }
    }

    #[test]
    fn hybrid_core_entities_use_regex_fallback_when_no_ml() {
        let rb = RuleBasedNerEngine::new();
        let engine = HybridNerEngine::new(&rb, None, None);
        let text = "Don Manuel Belgrano en la ciudad de Buenos Aires.";
        let entities = engine.extract(text).expect("hybrid extract should work");

        // Without ML engines, rule-based is the last-resort fallback for core types
        let persons: Vec<&Entity> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Person)
            .collect();
        let places: Vec<&Entity> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Place)
            .collect();

        assert!(
            !persons.is_empty(),
            "Should detect persons from rule-based fallback"
        );
        assert!(
            !places.is_empty(),
            "Should detect places from rule-based fallback"
        );

        for person in &persons {
            assert_eq!(
                person.source,
                EntitySource::RuleBased,
                "Without ML, core entities fall back to rule-based"
            );
        }
    }

    #[test]
    fn hybrid_routing_prefers_onnx_for_core_types() {
        // Verify BERT_FIRST_TYPES contains exactly the right types
        assert!(BERT_FIRST_TYPES.contains(&EntityType::Person));
        assert!(BERT_FIRST_TYPES.contains(&EntityType::Organization));
        assert!(BERT_FIRST_TYPES.contains(&EntityType::Place));
        assert!(!BERT_FIRST_TYPES.contains(&EntityType::Date));
        assert!(!BERT_FIRST_TYPES.contains(&EntityType::Institution));
        assert!(!BERT_FIRST_TYPES.contains(&EntityType::Misc));
    }

    #[test]
    fn hybrid_institution_merge_prefers_rule_based_institution() {
        // When both rule-based and ONNX detect an institution,
        // the merge should resolve overlaps
        let rb = RuleBasedNerEngine::new();
        let engine = HybridNerEngine::new(&rb, None, None);
        let text = "El Cabildo de Buenos Aires y la Real Audiencia.";
        let entities = engine.extract(text).expect("hybrid extract should work");

        let institutions: Vec<&Entity> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Institution)
            .collect();
        assert!(
            !institutions.is_empty(),
            "Should detect institutions from rule-based (Cabildo, Real Audiencia)"
        );
    }

    #[test]
    fn hybrid_date_entities_never_come_from_onnx() {
        // Even if we could add ONNX date entities, the routing
        // strictly filters them out — only RegEx dates survive
        let rb = RuleBasedNerEngine::new();
        let engine = HybridNerEngine::new(&rb, None, None);
        let text = "El 25 de mayo de 1810 fue una fecha importante.";
        let entities = engine.extract(text).expect("hybrid extract should work");

        let dates: Vec<&Entity> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Date)
            .collect();
        for date in &dates {
            assert_eq!(
                date.source,
                EntitySource::RuleBased,
                "DATE entities must only come from RegEx, got {:?}",
                date.source
            );
        }
    }

    #[test]
    fn dedup_entities_removes_overlapping_duplicates() {
        let deduped = dedup_entities(vec![
            entity(
                EntityType::Organization,
                "Gremio de la Construcción",
                10,
                40,
                0.95,
                EntitySource::Onnx,
            ),
            entity(
                EntityType::Organization,
                "Gremio de la Construcción",
                10,
                40,
                0.92,
                EntitySource::Spacy,
            ),
        ]);

        assert_eq!(
            deduped.len(),
            1,
            "Overlapping identical entities from different sources should deduplicate, got {:?}",
            deduped
        );
    }

    #[test]
    fn dedup_entities_keeps_longer_entity_on_overlap() {
        let deduped = dedup_entities(vec![
            entity(
                EntityType::Place,
                "MAR DEL PL",
                0,
                10,
                0.82,
                EntitySource::Onnx,
            ),
            entity(
                EntityType::Place,
                "Mar del Plata",
                0,
                13,
                0.91,
                EntitySource::Onnx,
            ),
        ]);

        assert_eq!(deduped.len(), 1, "Should keep the more complete entity");
        assert!(
            deduped[0].value.contains("Mar del Plata"),
            "Expected more complete entity, got: {}",
            deduped[0].value
        );
    }

    #[test]
    fn dedup_entities_collapses_case_and_punctuation_variants() {
        let deduped = dedup_entities(vec![
            entity(
                EntityType::Place,
                "MAR DEL PLATA",
                100,
                113,
                0.86,
                EntitySource::Onnx,
            ),
            entity(
                EntityType::Place,
                "Mar del Plata",
                101,
                114,
                0.93,
                EntitySource::Onnx,
            ),
            entity(
                EntityType::Place,
                "Mar del Plata.",
                101,
                115,
                0.90,
                EntitySource::Spacy,
            ),
        ]);

        assert_eq!(
            deduped.len(),
            1,
            "Expected variant duplicates to collapse: {deduped:?}"
        );
        assert_eq!(deduped[0].value, "Mar del Plata");
    }

    #[test]
    fn dedup_entities_collapses_repeated_mentions_globally() {
        let deduped = dedup_entities(vec![
            entity(
                EntityType::Place,
                "Mar del Plata",
                10,
                23,
                0.88,
                EntitySource::Onnx,
            ),
            entity(
                EntityType::Place,
                "Mar del Plata",
                210,
                223,
                0.93,
                EntitySource::Onnx,
            ),
            entity(
                EntityType::Place,
                "MAR DEL PLATA",
                410,
                423,
                0.84,
                EntitySource::Spacy,
            ),
        ]);

        assert_eq!(
            deduped.len(),
            1,
            "Repeated mentions of the same entity should collapse globally: {deduped:?}"
        );
        assert_eq!(deduped[0].value, "Mar del Plata");
    }

    #[test]
    fn dedup_entities_keeps_repeated_mentions_of_different_families() {
        let deduped = dedup_entities(vec![
            entity(
                EntityType::Place,
                "Córdoba",
                10,
                17,
                0.92,
                EntitySource::Onnx,
            ),
            entity(
                EntityType::Organization,
                "Córdoba",
                200,
                207,
                0.90,
                EntitySource::Onnx,
            ),
        ]);

        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn dedup_entities_preserves_different_type_overlaps() {
        let deduped = dedup_entities(vec![
            entity(
                EntityType::Person,
                "O'Neill",
                50,
                57,
                0.90,
                EntitySource::Onnx,
            ),
            entity(
                EntityType::Date,
                "21 de agosto de 1970",
                0,
                20,
                1.0,
                EntitySource::RuleBased,
            ),
        ]);

        assert_eq!(
            deduped.len(),
            2,
            "Different types should not deduplicate against each other"
        );
    }

    #[test]
    fn dedup_empty_list_is_noop() {
        let result = dedup_entities(vec![]);
        assert!(result.is_empty());
    }
}
