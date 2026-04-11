# Semantic Triples Specification

## Purpose

Define v1 behavior for extracting and handling semantic triples (Subject | Predicate | Object) per item, without graph features.

## Requirements

### Requirement: Per-Item Triple Extraction

The system MUST provide an asynchronous backend capability to extract semantic triples for a single `item_id` from that item's extracted text.

#### Scenario: Extract triples for one item

- GIVEN an item with extracted text
- WHEN triple extraction is requested for that `item_id`
- THEN the system returns zero or more triples with `subject`, `predicate`, and `object`
- AND no data from other items is included

#### Scenario: Item without extracted text

- GIVEN an item without extracted text
- WHEN triple extraction is requested
- THEN the system completes successfully with an empty triple result

### Requirement: Functional Idempotency by Item

Re-running extraction for the same `item_id` MUST functionally replace that item's previous triples with the latest extraction result.

#### Scenario: Re-extraction replaces previous result set

- GIVEN an item already has stored triples
- WHEN extraction is executed again for the same `item_id`
- THEN previous triples for that item are replaced
- AND the resulting stored set reflects only the latest extraction run
