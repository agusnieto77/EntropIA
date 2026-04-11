# Delta for nlp-ux

## ADDED Requirements

### Requirement: Semantic Triples Analysis Action

The Analysis panel in `ItemView` MUST provide a dedicated action to run semantic triple extraction for the current item through an asynchronous command.

#### Scenario: User runs triple extraction from Analysis panel

- GIVEN an item with at least one asset and extracted text
- WHEN the user triggers semantic triple extraction
- THEN the app invokes the triple extraction command with the current `item_id`
- AND the action reflects running and completion states

#### Scenario: Extraction command fails

- GIVEN the user triggers semantic triple extraction
- WHEN the backend command returns an error
- THEN the action state changes to error
- AND the UI remains responsive for retry

### Requirement: Semantic Triples List Rendering

After extraction, the Analysis panel MUST display a simple list of triples for the current item using Subject | Predicate | Object columns or fields.

#### Scenario: Render triples after successful extraction

- GIVEN triple extraction completes successfully
- WHEN triples are loaded for the current `item_id`
- THEN the Analysis panel shows each triple with Subject, Predicate, and Object

#### Scenario: Empty-state rendering for no triples

- GIVEN extraction yields no triples for the current item
- WHEN the triples section is rendered
- THEN the UI shows an explicit empty state
- AND no graph, cross-document, or sync behavior is shown
