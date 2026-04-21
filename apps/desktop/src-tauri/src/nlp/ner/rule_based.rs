use once_cell::sync::Lazy;
use regex::Regex;

use super::types::{sanitize_entity_value, Entity, EntitySource, EntityType, NerEngine};

pub struct RuleBasedNerEngine;

impl RuleBasedNerEngine {
    pub fn new() -> Self {
        Self
    }
}

impl NerEngine for RuleBasedNerEngine {
    fn name(&self) -> &'static str {
        "rule_based"
    }

    fn extract(&self, text: &str) -> Result<Vec<Entity>, String> {
        let mut entities = Vec::new();

        collect_matches(&PERSON_RE, text, EntityType::Person, &mut entities);
        collect_matches(&PLACE_RE, text, EntityType::Place, &mut entities);
        collect_matches(&DATE_WRITTEN_RE, text, EntityType::Date, &mut entities);
        collect_matches(&DATE_NUMERIC_RE, text, EntityType::Date, &mut entities);
        collect_matches(&INSTITUTION_RE, text, EntityType::Institution, &mut entities);

        entities.sort_by_key(|e| e.start_offset);
        Ok(entities)
    }
}

fn collect_matches(re: &Regex, text: &str, entity_type: EntityType, out: &mut Vec<Entity>) {
    for m in re.find_iter(text) {
        let value = sanitize_entity_value(m.as_str());
        if value.is_empty() {
            continue;
        }
        out.push(Entity {
            entity_type: entity_type.clone(),
            value,
            start_offset: m.start(),
            end_offset: m.end(),
            confidence: 1.0,
            source: EntitySource::RuleBased,
            model_name: None,
        });
    }
}

static PERSON_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:Don|Doña|Dr\.?|Fray|Sor|Fr\.)\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+(?:\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)*",
    )
    .expect("PERSON_RE failed to compile")
});

static PLACE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:(?:(?:ciudad|villa|pueblo|provincia)\s+de\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+(?:\s+(?:de|del|la|las|los|y)\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+|\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)*)|(?:(?:río|sierra)\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+(?:\s+(?:de|del|la|las|los|y)\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+|\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)*))")
        .expect("PLACE_RE failed to compile")
});

static DATE_WRITTEN_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b\d{1,2}\s+de\s+[a-záéíóúñ]+\s+de\s+\d{4}\b")
        .expect("DATE_WRITTEN_RE failed to compile")
});

static DATE_NUMERIC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b\d{1,2}/\d{1,2}/\d{4}\b").expect("DATE_NUMERIC_RE failed to compile")
});

static INSTITUTION_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:Real|Cabildo|Iglesia|Convento|Universidad|Audiencia)(?:\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)*",
    )
    .expect("INSTITUTION_RE failed to compile")
});

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_COLONIAL: &str = r#"
En la ciudad de Buenos Aires, a 15 de mayo de 1810, Don Manuel Belgrano,
secretario de la Real Audiencia, y Doña Juana Azurduy, representante de la
villa de Potosí, se reunieron en el Cabildo para tratar el asunto.
El Convento San Francisco y la Universidad de Córdoba enviaron delegados.
La fecha límite era el 25/05/1810.
"#;

    const FIXTURE_ECCLESIASTICAL: &str = r#"
Fray Bartolomé de las Casas presentó su informe ante la Iglesia Catedral
de la ciudad de Sevilla el 3 de junio de 1542.
El Dr. Juan de Zumárraga, obispo de la sierra Nevada, firmó el documento.
"#;

    const FIXTURE_ADMINISTRATIVE: &str = r#"
El Cabildo de la villa de Montevideo registró, el 12 de octubre de 1820,
el acuerdo firmado por Don José Artigas y el representante de la provincia de
Entre Ríos. La Audiencia Real emitió su resolución el 01/11/1820.
"#;

    fn extract_entities(text: &str) -> Vec<Entity> {
        RuleBasedNerEngine::new().extract(text).expect("rule-based NER should work")
    }

    #[test]
    fn fixture_colonial_detects_person() {
        let entities = extract_entities(FIXTURE_COLONIAL);
        let persons: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Person)
            .collect();
        assert!(!persons.is_empty(), "Expected at least one PERSON in colonial fixture");
        let values: Vec<&str> = persons.iter().map(|e| e.value.as_str()).collect();
        assert!(
            values.iter().any(|v| v.contains("Manuel Belgrano")),
            "Expected 'Don Manuel Belgrano' to be detected"
        );
    }

    #[test]
    fn fixture_colonial_detects_place() {
        let entities = extract_entities(FIXTURE_COLONIAL);
        let places: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Place)
            .collect();
        assert!(!places.is_empty(), "Expected at least one PLACE in colonial fixture");

        let values: Vec<&str> = places.iter().map(|e| e.value.as_str()).collect();
        assert!(
            values.iter().any(|v| *v == "ciudad de Buenos Aires"),
            "Expected 'ciudad de Buenos Aires' to be detected"
        );
        assert!(
            values.iter().any(|v| *v == "villa de Potosí"),
            "Expected 'villa de Potosí' to be detected"
        );
    }

    #[test]
    fn fixture_colonial_detects_date_written() {
        let entities = extract_entities(FIXTURE_COLONIAL);
        let dates: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Date)
            .collect();
        assert!(!dates.is_empty(), "Expected at least one DATE in colonial fixture");
        let values: Vec<&str> = dates.iter().map(|e| e.value.as_str()).collect();
        assert!(
            values.iter().any(|v| v.contains("15 de mayo de 1810")),
            "Expected '15 de mayo de 1810' to be detected"
        );
    }

    #[test]
    fn fixture_colonial_detects_date_numeric() {
        let entities = extract_entities(FIXTURE_COLONIAL);
        let dates: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Date)
            .collect();
        let values: Vec<&str> = dates.iter().map(|e| e.value.as_str()).collect();
        assert!(
            values.iter().any(|v| *v == "25/05/1810"),
            "Expected '25/05/1810' to be detected as numeric date"
        );
    }

    #[test]
    fn fixture_colonial_detects_institution() {
        let entities = extract_entities(FIXTURE_COLONIAL);
        let institutions: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Institution)
            .collect();
        assert!(
            !institutions.is_empty(),
            "Expected at least one INSTITUTION in colonial fixture"
        );
    }

    #[test]
    fn fixture_colonial_detects_all_four_entity_types() {
        let entities = extract_entities(FIXTURE_COLONIAL);
        let has_person = entities.iter().any(|e| e.entity_type == EntityType::Person);
        let has_place = entities.iter().any(|e| e.entity_type == EntityType::Place);
        let has_date = entities.iter().any(|e| e.entity_type == EntityType::Date);
        let has_institution = entities.iter().any(|e| e.entity_type == EntityType::Institution);
        assert!(has_person, "Missing PERSON entities");
        assert!(has_place, "Missing PLACE entities");
        assert!(has_date, "Missing DATE entities");
        assert!(has_institution, "Missing INSTITUTION entities");
    }

    #[test]
    fn fixture_ecclesiastical_detects_fray_and_doctor() {
        let entities = extract_entities(FIXTURE_ECCLESIASTICAL);
        let persons: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Person)
            .collect();
        assert!(persons.len() >= 2, "Expected ≥2 persons in ecclesiastical fixture");
        let values: Vec<&str> = persons.iter().map(|e| e.value.as_str()).collect();
        assert!(
            values.iter().any(|v| v.contains("Bartolomé")),
            "Expected Fray Bartolomé to be detected"
        );
    }

    #[test]
    fn fixture_administrative_detects_multiple_dates() {
        let entities = extract_entities(FIXTURE_ADMINISTRATIVE);
        let dates: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Date)
            .collect();
        assert!(
            dates.len() >= 2,
            "Expected ≥2 dates in administrative fixture, got {}",
            dates.len()
        );
    }

    #[test]
    fn empty_text_returns_no_entities() {
        let entities = extract_entities("");
        assert!(entities.is_empty());
    }

    #[test]
    fn entities_sorted_by_start_offset() {
        let entities = extract_entities(FIXTURE_COLONIAL);
        for i in 1..entities.len() {
            assert!(
                entities[i].start_offset >= entities[i - 1].start_offset,
                "Entities not sorted by start_offset at index {i}"
            );
        }
    }

    #[test]
    fn detects_colonial_prepositional_place_forms() {
        let text = "En la ciudad de Buenos Aires y la villa de Potosí, con visitas de la provincia de Entre Ríos.";
        let places: Vec<String> = extract_entities(text)
            .into_iter()
            .filter(|e| e.entity_type == EntityType::Place)
            .map(|e| e.value)
            .collect();

        assert!(places.iter().any(|v| v == "ciudad de Buenos Aires"));
        assert!(places.iter().any(|v| v == "villa de Potosí"));
        assert!(places.iter().any(|v| v == "provincia de Entre Ríos"));
    }

    #[test]
    fn avoids_false_positive_for_non_toponym_phrase() {
        let text = "El pueblo de los vecinos solicitó audiencia.";
        let places: Vec<String> = extract_entities(text)
            .into_iter()
            .filter(|e| e.entity_type == EntityType::Place)
            .map(|e| e.value)
            .collect();

        assert!(places.is_empty(), "unexpected PLACE entities: {places:?}");
    }
}
