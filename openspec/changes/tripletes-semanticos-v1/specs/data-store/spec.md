# Delta for data-store

## ADDED Requirements

### Requirement: Triples Table Migration

The data store MUST include a versioned migration that creates a `triples` table linked to `items` and optimized for per-item reads.

#### Scenario: Migration creates triples table and index

- GIVEN pending migrations include the triples migration
- WHEN migrations run on startup
- THEN a `triples` table exists with `id`, `item_id`, `subject`, `predicate`, `object`, and `created_at`
- AND an index on `triples(item_id)` exists

#### Scenario: Triples migration is safe on re-run

- GIVEN the triples migration was already applied
- WHEN migrations run again
- THEN the migration is not re-applied
- AND startup completes without schema errors

### Requirement: Triple Repository Contract

`packages/store` MUST expose a `TripleRepo` with `findByItemId(itemId)` and `replaceByItemId(itemId, triples)` for per-item persistence.

#### Scenario: List triples by item

- GIVEN triples exist for item A and item B
- WHEN `findByItemId(itemA)` is called
- THEN only triples belonging to item A are returned

#### Scenario: Replace triples atomically by item

- GIVEN item A has existing triples
- WHEN `replaceByItemId(itemA, newTriples)` is called
- THEN previous triples for item A are removed and `newTriples` are persisted
- AND triples for other items remain unchanged
