# NLP UX Specification

## Purpose

Defines the user interface for the NLP analysis panel in `ItemView`, covering the three analysis actions, their progress states, the Similar Items section, and the EntityViewer integration.

## Requirements

### Requirement: Analysis Panel in ItemView

`ItemView.svelte` MUST include a collapsible "Analysis" panel in the right column, positioned below the existing extraction panel. The panel MUST follow the same collapsible pattern used by the extraction panel (header with toggle chevron, body that expands/collapses). The panel MUST only be visible when the item has at least one asset.

#### Scenario: Analysis panel renders in ItemView

- GIVEN a user opens an item that has at least one asset
- WHEN the ItemView loads
- THEN an "Analysis" collapsible panel is visible in the right column below the extraction panel

#### Scenario: Analysis panel hidden when no asset

- GIVEN an item has no assets
- WHEN the ItemView loads
- THEN the Analysis panel is not rendered

#### Scenario: Panel collapses and expands

- GIVEN the Analysis panel is expanded
- WHEN the user clicks the panel header
- THEN the panel body collapses
- AND clicking again expands it

---

### Requirement: Three Analysis Action Buttons

The Analysis panel MUST contain three buttons: "Full-Text Index", "Generate Embeddings", and "Extract Entities". Each button MUST trigger its corresponding Tauri command: `index_item_fts`, `embed_item`, and `extract_entities` respectively. Buttons MUST be disabled while their corresponding action is running.

#### Scenario: Full-Text Index button triggers FTS indexing

- GIVEN the Analysis panel is visible
- WHEN the user clicks "Full-Text Index"
- THEN `index_item_fts` is invoked with the current `item_id`
- AND the button is disabled while the command runs

#### Scenario: Generate Embeddings button triggers embedding

- GIVEN the Analysis panel is visible
- WHEN the user clicks "Generate Embeddings"
- THEN `embed_item` is invoked with the current `item_id`
- AND the button is disabled while the command runs

#### Scenario: Extract Entities button triggers NER

- GIVEN the Analysis panel is visible
- WHEN the user clicks "Extract Entities"
- THEN `extract_entities` is invoked with the current `item_id`
- AND the button is disabled while the command runs

---

### Requirement: Action Status Badges

Each analysis action button MUST show a status badge alongside it. The badge MUST reflect the current state: `idle` (no action taken), `running` (command in progress), `done` (completed successfully), or `error` (command failed). Status MUST persist within the session for the current item.

#### Scenario: Idle state on initial render

- GIVEN the Analysis panel is opened for an item that has never been analyzed
- WHEN the panel renders
- THEN all three status badges show `idle`

#### Scenario: Running badge shown while command executes

- GIVEN the user clicks "Generate Embeddings"
- WHEN the `embed_item` command is in flight
- THEN the Embeddings button badge shows `running`
- AND the button is disabled

#### Scenario: Done badge shown on success

- GIVEN "Extract Entities" is running
- WHEN `extract_entities` completes successfully
- THEN the Entities button badge transitions to `done`
- AND the button is re-enabled

#### Scenario: Error badge shown on failure

- GIVEN "Full-Text Index" is running
- WHEN `index_item_fts` returns an error
- THEN the FTS button badge transitions to `error`
- AND the button is re-enabled

---

### Requirement: Similar Items Section

The Analysis panel MUST contain a "Similar Items" sub-section that displays 3–5 linked item cards when the current item has embeddings. Each card MUST show the linked item's title and collection name. Clicking a card MUST navigate to that item.

#### Scenario: Similar items displayed when embeddings exist

- GIVEN the current item has a vector in `vec_items`
- WHEN the Analysis panel is expanded
- THEN the Similar Items section shows up to 5 item cards
- AND each card displays the item title and its collection name

#### Scenario: Similar items hidden when no embedding

- GIVEN the current item has no entry in `vec_items`
- WHEN the Analysis panel is expanded
- THEN the Similar Items section shows an empty state ("Generate embeddings to discover similar documents")

#### Scenario: Clicking similar item navigates

- GIVEN the Similar Items section shows item B as a similar item
- WHEN the user clicks item B's card
- THEN the app navigates to item B's detail view

---

### Requirement: Entities Section in Analysis Panel

The Analysis panel MUST render the `EntityViewer` component below the Similar Items section. The component MUST be populated with the entities fetched for the current item. The section MUST update automatically after "Extract Entities" completes.

#### Scenario: EntityViewer populated after extraction

- GIVEN the user clicks "Extract Entities" and the command completes
- WHEN the Analysis panel is visible
- THEN the EntityViewer section renders the newly detected entities grouped by type

#### Scenario: EntityViewer empty state before first extraction

- GIVEN an item has never had entities extracted
- WHEN the Analysis panel renders
- THEN the EntityViewer shows the empty state message

#### Scenario: Highlight event scrolls text viewer

- GIVEN the EntityViewer shows a PERSON entity
- WHEN the user clicks that entity
- THEN a `highlight` event propagates to the extraction text viewer
- AND the relevant text range is visually highlighted
