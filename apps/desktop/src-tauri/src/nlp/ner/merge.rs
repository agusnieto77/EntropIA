use super::types::{sanitize_entity_value, Entity, EntitySource, EntityType};

const CORE_BETO_ONNX_BONUS_THRESHOLD: f32 = 0.05;

/// Merge Institution entities from rule-based and ONNX sources.
///
/// Rule-based institutions (colonial keywords like Cabildo, Real Audiencia)
/// take priority when they overlap with ONNX-detected organizations that
/// normalize to institutions. This preserves the domain-specific regex quality
/// for colonial document terminology.
pub fn merge_institutions(rule_based: Vec<Entity>, onnx: Vec<Entity>) -> Vec<Entity> {
    merge_entities(rule_based, onnx)
}

/// Legacy full-merge function used by Institution merging.
/// Merges entities from two sources, resolving overlaps by type
/// specificity, source quality, span completeness, and confidence.
pub fn merge_entities(rule_based: Vec<Entity>, onnx: Vec<Entity>) -> Vec<Entity> {
    let mut all = Vec::new();
    all.extend(rule_based);
    all.extend(onnx);
    all.sort_by_key(|e| (e.start_offset, e.end_offset, source_rank(&e.source)));

    let mut merged: Vec<Entity> = Vec::new();
    for candidate in all {
        if let Some(prev) = merged.last_mut() {
            if related(prev, &candidate) {
                *prev = pick_winner(prev.clone(), candidate);
                continue;
            }
        }

        merged.push(candidate);
    }

    merged
}

fn related(a: &Entity, b: &Entity) -> bool {
    overlaps(a, b) || exactish_duplicate(a, b)
}

fn overlaps(a: &Entity, b: &Entity) -> bool {
    a.start_offset < b.end_offset && b.start_offset < a.end_offset
}

fn exactish_duplicate(a: &Entity, b: &Entity) -> bool {
    let same_value = normalized_value(&a.value) == normalized_value(&b.value);
    let close_offsets = a.start_offset.abs_diff(b.start_offset) <= 2
        && a.end_offset.abs_diff(b.end_offset) <= 2;
    same_value && (close_offsets || a.entity_type == b.entity_type)
}

fn pick_winner(a: Entity, b: Entity) -> Entity {
    if is_rule_based_date(&a) {
        return a;
    }
    if is_rule_based_date(&b) {
        return b;
    }

    if prefer_onnx_core_entity(&a, &b) {
        return b;
    }
    if prefer_onnx_core_entity(&b, &a) {
        return a;
    }

    if is_rule_based_institution_like(&a, &b) {
        return a;
    }
    if is_rule_based_institution_like(&b, &a) {
        return b;
    }

    if type_specificity_rank(&b.entity_type) > type_specificity_rank(&a.entity_type) {
        return b;
    }
    if type_specificity_rank(&a.entity_type) > type_specificity_rank(&b.entity_type) {
        return a;
    }

    if span_len(&b) > span_len(&a) && b.confidence >= a.confidence {
        return b;
    }
    if span_len(&a) > span_len(&b) && a.confidence >= b.confidence {
        return a;
    }

    if b.confidence > a.confidence {
        return b;
    }
    if a.confidence > b.confidence {
        return a;
    }

    if source_rank(&b.source) > source_rank(&a.source) {
        return b;
    }

    a
}

fn is_rule_based_date(entity: &Entity) -> bool {
    entity.entity_type == EntityType::Date && entity.source == EntitySource::RuleBased
}

fn is_rule_based_institution_like(winner: &Entity, loser: &Entity) -> bool {
    winner.source == EntitySource::RuleBased
        && winner.entity_type == EntityType::Institution
        && matches!(loser.entity_type, EntityType::Institution | EntityType::Organization)
}

fn prefer_onnx_core_entity(existing: &Entity, candidate: &Entity) -> bool {
    candidate.source == EntitySource::Onnx
        && existing.source != EntitySource::Onnx
        && existing.source != EntitySource::Spacy
        && matches!(candidate.entity_type, EntityType::Person | EntityType::Place | EntityType::Organization | EntityType::Institution)
        && same_core_entity_family(&existing.entity_type, &candidate.entity_type)
        && candidate.confidence >= existing.confidence + CORE_BETO_ONNX_BONUS_THRESHOLD
}

fn same_core_entity_family(a: &EntityType, b: &EntityType) -> bool {
    match (a, b) {
        (EntityType::Person, EntityType::Person) => true,
        (EntityType::Place, EntityType::Place) => true,
        (EntityType::Organization, EntityType::Organization) => true,
        (EntityType::Institution, EntityType::Institution) => true,
        (EntityType::Organization, EntityType::Institution) => true,
        (EntityType::Institution, EntityType::Organization) => true,
        _ => false,
    }
}

fn normalized_value(value: &str) -> String {
    sanitize_entity_value(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn span_len(entity: &Entity) -> usize {
    entity.end_offset.saturating_sub(entity.start_offset)
}

fn source_rank(source: &EntitySource) -> u8 {
    match source {
        EntitySource::RuleBased => 1,
        EntitySource::Onnx => 2,
        EntitySource::Spacy => 3,
        EntitySource::Llm => 4,
    }
}

fn type_specificity_rank(entity_type: &EntityType) -> u8 {
    match entity_type {
        EntityType::Institution => 5,
        EntityType::Date => 4,
        EntityType::Person => 3,
        EntityType::Place => 3,
        EntityType::Organization => 2,
        EntityType::Misc => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn rule_based_date_beats_onnx_overlap() {
        let merged = merge_entities(
            vec![entity(EntityType::Date, "25/05/1810", 10, 20, 1.0, EntitySource::RuleBased)],
            vec![entity(EntityType::Date, "25/05/1810", 10, 20, 0.92, EntitySource::Onnx)],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, EntitySource::RuleBased);
    }

    #[test]
    fn rule_based_institution_beats_onnx_organization() {
        let merged = merge_entities(
            vec![entity(
                EntityType::Institution,
                "Cabildo de Buenos Aires",
                0,
                24,
                1.0,
                EntitySource::RuleBased,
            )],
            vec![entity(
                EntityType::Organization,
                "Cabildo de Buenos Aires",
                0,
                24,
                0.93,
                EntitySource::Onnx,
            )],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].entity_type, EntityType::Institution);
        assert_eq!(merged[0].source, EntitySource::RuleBased);
    }

    #[test]
    fn onnx_person_can_beat_lower_confidence_rule_based_match() {
        let merged = merge_entities(
            vec![entity(
                EntityType::Person,
                "Don Manuel",
                0,
                10,
                0.70,
                EntitySource::RuleBased,
            )],
            vec![entity(
                EntityType::Person,
                "Don Manuel Belgrano",
                0,
                20,
                0.91,
                EntitySource::Onnx,
            )],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, EntitySource::Onnx);
        assert_eq!(merged[0].value, "Don Manuel Belgrano");
    }

    #[test]
    fn onnx_org_has_priority_over_rule_based_org_family() {
        let merged = merge_entities(
            vec![entity(
                EntityType::Institution,
                "Real Audiencia de Charcas",
                0,
                25,
                0.80,
                EntitySource::RuleBased,
            )],
            vec![entity(
                EntityType::Organization,
                "Real Audiencia de Charcas",
                0,
                25,
                0.91,
                EntitySource::Onnx,
            )],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, EntitySource::Onnx);
    }

    #[test]
    fn exactish_duplicates_collapse_even_without_overlap() {
        let merged = merge_entities(
            vec![entity(
                EntityType::Place,
                "ciudad de Buenos Aires",
                100,
                123,
                1.0,
                EntitySource::RuleBased,
            )],
            vec![entity(
                EntityType::Place,
                "ciudad   de Buenos Aires",
                101,
                124,
                0.95,
                EntitySource::Onnx,
            )],
        );

        assert_eq!(merged.len(), 1);
    }
}
