/// Rule-based Named Entity Recognition for historical Spanish documents.
///
/// Uses `once_cell::sync::Lazy` to compile regex patterns once at startup.
/// Supports PERSON, PLACE, DATE (numeric + written), and INSTITUTION entity types.
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};

// ── Entity types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub entity_type: EntityType,
    pub value: String,
    pub start_offset: usize,
    pub end_offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EntityType {
    Person,
    Place,
    Date,
    Institution,
}

impl EntityType {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Person => "person",
            EntityType::Place => "place",
            EntityType::Date => "date",
            EntityType::Institution => "institution",
        }
    }
}

// ── Compiled patterns (once_cell) ────────────────────────────────────────────

static PERSON_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?:Don|Doña|Dr\.?|Fray|Sor|Fr\.)\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+(?:\s+[A-ZÁÉÍÓÚÑ][a-záéíóúñ]+)*",
    )
    .expect("PERSON_RE failed to compile")
});

static PLACE_RE: Lazy<Regex> = Lazy::new(|| {
    // PLACE colonial forms:
    // - prepositional: ciudad|villa|pueblo|provincia de <Topónimo...>
    // - marker forms: río|sierra <Topónimo...>
    // Toponym tokens accept Title Case and common lowercase connectors (de/del/la/las/los/y).
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

// ── Public API ───────────────────────────────────────────────────────────────

/// Extract named entities from `text`.
///
/// Applies all patterns in order. Overlapping matches are allowed — the
/// historian can review duplicates via the UI.
pub fn extract_entities(text: &str) -> Vec<Entity> {
    let mut entities = Vec::new();

    collect_matches(&PERSON_RE, text, EntityType::Person, &mut entities);
    collect_matches(&PLACE_RE, text, EntityType::Place, &mut entities);
    collect_matches(&DATE_WRITTEN_RE, text, EntityType::Date, &mut entities);
    collect_matches(&DATE_NUMERIC_RE, text, EntityType::Date, &mut entities);
    collect_matches(
        &INSTITUTION_RE,
        text,
        EntityType::Institution,
        &mut entities,
    );

    // Sort by start offset for deterministic output
    entities.sort_by_key(|e| e.start_offset);
    entities
}

/// Fetch the extracted text for `item_id`, run NER, and persist results to DB.
pub fn extract_and_store(conn: &Connection, item_id: &str) -> Result<(), String> {
    // Fetch extracted text for the item (latest extraction)
    let text: Option<String> = conn
        .query_row(
            r#"
            SELECT e.text_content
            FROM extractions e
            JOIN assets a ON e.asset_id = a.id
            WHERE a.item_id = ?1
            ORDER BY e.created_at DESC
            LIMIT 1
            "#,
            params![item_id],
            |row| row.get(0),
        )
        .ok();

    let text = match text {
        Some(t) if !t.is_empty() => t,
        _ => return Ok(()), // Nothing to index — not an error
    };

    let entities = extract_entities(&text);

    // Delete previous entities for this item, then insert fresh batch
    conn.execute("DELETE FROM entities WHERE item_id = ?1", params![item_id])
        .map_err(|e| format!("Failed to delete old entities: {e}"))?;

    for entity in &entities {
        let id = uuid_v4();
        conn.execute(
            r#"
            INSERT INTO entities (id, item_id, entity_type, value, start_offset, end_offset, confidence, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                id,
                item_id,
                entity.entity_type.as_str(),
                entity.value,
                entity.start_offset as i64,
                entity.end_offset as i64,
                1.0_f64, // rule-based: deterministic confidence
                now_millis(),
            ],
        )
        .map_err(|e| format!("Failed to insert entity: {e}"))?;
    }

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn collect_matches(re: &Regex, text: &str, entity_type: EntityType, out: &mut Vec<Entity>) {
    for m in re.find_iter(text) {
        out.push(Entity {
            entity_type: entity_type.clone(),
            value: m.as_str().to_string(),
            start_offset: m.start(),
            end_offset: m.end(),
        });
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // Minimal UUID-like ID — uses process random + timestamp for uniqueness.
    // Production apps should use the `uuid` crate, but we avoid adding a dep here.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!(
        "{:016x}-{:08x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros(),
        nanos,
    )
}

fn now_millis() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Fixture 1 — formal colonial letter with titles, places, dates, institutions
    const FIXTURE_COLONIAL: &str = r#"
En la ciudad de Buenos Aires, a 15 de mayo de 1810, Don Manuel Belgrano,
secretario de la Real Audiencia, y Doña Juana Azurduy, representante de la
villa de Potosí, se reunieron en el Cabildo para tratar el asunto.
El Convento San Francisco y la Universidad de Córdoba enviaron delegados.
La fecha límite era el 25/05/1810.
"#;

    // Fixture 2 — ecclesiastical document
    const FIXTURE_ECCLESIASTICAL: &str = r#"
Fray Bartolomé de las Casas presentó su informe ante la Iglesia Catedral
de la ciudad de Sevilla el 3 de junio de 1542.
El Dr. Juan de Zumárraga, obispo de la sierra Nevada, firmó el documento.
"#;

    // Fixture 3 — administrative record with multiple dates
    const FIXTURE_ADMINISTRATIVE: &str = r#"
El Cabildo de la villa de Montevideo registró, el 12 de octubre de 1820,
el acuerdo firmado por Don José Artigas y el representante de la provincia de
Entre Ríos. La Audiencia Real emitió su resolución el 01/11/1820.
"#;

    #[test]
    fn fixture_colonial_detects_person() {
        let entities = extract_entities(FIXTURE_COLONIAL);
        let persons: Vec<_> = entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Person)
            .collect();
        assert!(
            !persons.is_empty(),
            "Expected at least one PERSON in colonial fixture"
        );
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
        assert!(
            !places.is_empty(),
            "Expected at least one PLACE in colonial fixture"
        );

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
        assert!(
            !dates.is_empty(),
            "Expected at least one DATE in colonial fixture"
        );
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
        let has_institution = entities
            .iter()
            .any(|e| e.entity_type == EntityType::Institution);
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
        assert!(
            persons.len() >= 2,
            "Expected ≥2 persons in ecclesiastical fixture"
        );
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
