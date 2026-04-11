# NER Entities Specification

## Purpose

Defines rule-based named-entity recognition for historical Spanish texts, entity storage, and entity display in the EntropIA frontend.

## Requirements

### Requirement: Extract Entities Command

The system MUST expose an `extract_entities` Tauri command that accepts an `item_id`, reads the item's latest extraction text, runs regex-based NER, stores detected entities in the `entities` table, and returns the list of found entities. If no extraction exists for the item, the command MUST return an empty array without error.

#### Scenario: Extract entities from item with extraction

- GIVEN an item has extracted text containing known entity patterns
- WHEN `extract_entities({ item_id })` is invoked
- THEN entities are detected and inserted into `entities`
- AND the command returns the list of entities

#### Scenario: No extraction available returns empty

- GIVEN an item has no extraction record
- WHEN `extract_entities({ item_id })` is invoked
- THEN an empty array is returned
- AND no rows are written to `entities`
- AND no error is thrown

#### Scenario: Re-extract clears previous entities

- GIVEN an item already has 5 entity rows in `entities`
- WHEN `extract_entities({ item_id })` is invoked again
- THEN all previous entities for that item are deleted first
- AND the new set of entities replaces them

---

### Requirement: Entity Types and Patterns

The NER engine MUST detect at minimum four entity types using Rust regex rules over the extracted text:

| Type          | Trigger patterns                                                              |
| ------------- | ----------------------------------------------------------------------------- |
| `PERSON`      | Prefixes: Don, Doña, Dr, Fray, Sor, Sr, Sra                                   |
| `PLACE`       | Geographic markers: ciudad de, villa de, provincia de, río, sierra            |
| `DATE`        | Formats: `dd/mm/yyyy`, `dd de <month> de <year>`, Spanish written month names |
| `INSTITUTION` | Keywords: Real, Cabildo, Iglesia, Convento, Universidad, Audiencia            |

#### Scenario: Person entity detected

- GIVEN extracted text contains "Don Juan de Garay llegó"
- WHEN `extract_entities` runs
- THEN an entity of type `PERSON` with value `"Don Juan de Garay"` is detected

#### Scenario: Place entity detected

- GIVEN extracted text contains "ciudad de Buenos Aires"
- WHEN `extract_entities` runs
- THEN an entity of type `PLACE` with value `"ciudad de Buenos Aires"` is detected

#### Scenario: Date entity detected (numeric)

- GIVEN extracted text contains "el 15/05/1810"
- WHEN `extract_entities` runs
- THEN an entity of type `DATE` with value `"15/05/1810"` is detected

#### Scenario: Date entity detected (written)

- GIVEN extracted text contains "a los doce días del mes de mayo de mil ochocientos diez"
- WHEN `extract_entities` runs
- THEN an entity of type `DATE` is detected for the written date phrase

#### Scenario: Institution entity detected

- GIVEN extracted text contains "el Cabildo de Córdoba resolvió"
- WHEN `extract_entities` runs
- THEN an entity of type `INSTITUTION` with value `"Cabildo de Córdoba"` is detected

#### Scenario: No matching entities returns empty array

- GIVEN extracted text contains no recognizable entity patterns
- WHEN `extract_entities` runs
- THEN an empty array is returned
- AND no rows are written to `entities`

---

### Requirement: Entities Table

The system MUST store detected entities in an `entities` table with columns: `id TEXT PRIMARY KEY`, `item_id TEXT NOT NULL REFERENCES items(id)`, `entity_type TEXT NOT NULL`, `value TEXT NOT NULL`, `start_offset INTEGER NOT NULL`, `end_offset INTEGER NOT NULL`, `confidence REAL NOT NULL`. The table MUST have an index on `item_id`.

#### Scenario: Entity row contains all required fields

- GIVEN an entity is extracted from item-1
- WHEN the row is inserted into `entities`
- THEN it has non-null values for `id`, `item_id`, `entity_type`, `value`, `start_offset`, `end_offset`, `confidence`

#### Scenario: Foreign key enforced on item_id

- GIVEN a non-existent `item_id`
- WHEN an entity row is inserted referencing that id
- THEN a foreign key constraint error is raised

---

### Requirement: Entity Repository

`packages/store` MUST export an `EntityRepo` class that provides `findByItemId(itemId)`, `create(data)`, and `deleteByItemId(itemId)`. All methods MUST accept a `DrizzleClient` via constructor injection.

#### Scenario: Create and retrieve entities for item

- GIVEN an `EntityRepo` with a valid DB client
- WHEN `create({ item_id, entity_type: 'PERSON', value: 'Don José', start_offset: 0, end_offset: 8, confidence: 0.9 })` is called
- THEN `findByItemId(item_id)` returns the created entity

#### Scenario: deleteByItemId removes all entities for item

- GIVEN an item has 3 entity rows
- WHEN `deleteByItemId(item_id)` is called
- THEN `findByItemId(item_id)` returns an empty array

---

### Requirement: EntityViewer Component

`packages/ui` MUST export an `EntityViewer.svelte` component that accepts an `entities` prop (array of entity objects) and renders them grouped by `entity_type`. Each entity MUST display its `value` and clicking it MUST emit a `highlight` event with `{ start_offset, end_offset }`.

#### Scenario: Entities rendered grouped by type

- GIVEN `EntityViewer` receives 4 entities (2 PERSON, 1 PLACE, 1 DATE)
- WHEN the component renders
- THEN entities are grouped into sections: "PERSON", "PLACE", "DATE"
- AND each section lists its entity values

#### Scenario: Click emits highlight event

- GIVEN an entity with `start_offset: 10, end_offset: 25` is displayed
- WHEN the user clicks that entity
- THEN a `highlight` event is dispatched with `{ start_offset: 10, end_offset: 25 }`

#### Scenario: Empty entities shows empty state

- GIVEN `EntityViewer` receives an empty array
- WHEN the component renders
- THEN an empty state message is shown (e.g., "No entities detected")
