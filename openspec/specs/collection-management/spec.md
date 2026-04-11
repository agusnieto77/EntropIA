# Collection Management Specification

## Purpose

Defines CRUD operations for collections — the top-level organizational unit for grouping historical document items.

## Requirements

### Requirement: Create Collection

The system MUST allow users to create a collection. A name is REQUIRED. A description is OPTIONAL. The system MUST generate a unique ID and set `created_at` and `updated_at` timestamps.

#### Scenario: Create collection with name only

- GIVEN the user is on the collections list view
- WHEN they submit a new collection with name "Archivo Municipal"
- THEN a collection is created with that name and an empty description
- AND the collection appears in the list

#### Scenario: Create collection with name and description

- GIVEN the user opens the create collection form
- WHEN they enter name "Archivo Municipal" and description "Documentos del cabildo 1800-1850"
- THEN the collection is created with both fields populated

#### Scenario: Reject empty name

- GIVEN the create collection form is open
- WHEN the user submits without entering a name
- THEN the collection is NOT created
- AND a validation error is shown indicating name is required

### Requirement: List Collections

The system MUST display all collections sorted by `updated_at` descending (most recently updated first). Each collection MUST show its name, description (if any), item count, and last updated date.

#### Scenario: Collections listed in order

- GIVEN three collections exist with different `updated_at` values
- WHEN the user views the collections list
- THEN collections are displayed sorted by most recently updated first

#### Scenario: Item count badge shown

- GIVEN a collection has 15 items
- WHEN the user views the collections list
- THEN that collection displays an item count badge showing "15"

#### Scenario: Empty state

- GIVEN no collections exist
- WHEN the user views the collections list
- THEN an empty state message is shown with guidance to create a collection

### Requirement: Update Collection

The system MUST allow users to rename a collection or update its description. The `updated_at` timestamp MUST be refreshed on update.

#### Scenario: Rename collection

- GIVEN a collection named "Archivo Municipal" exists
- WHEN the user renames it to "Archivo Histórico Municipal"
- THEN the name is updated
- AND `updated_at` reflects the current time

### Requirement: Delete Collection

The system MUST allow users to delete a collection. If the collection contains items, a confirmation dialog MUST be shown before deletion. Deleting a collection MUST cascade-delete all items, assets, and notes within it. Asset files on disk SHOULD also be removed.

#### Scenario: Delete empty collection

- GIVEN a collection with zero items
- WHEN the user deletes it
- THEN the collection is removed from the database
- AND it disappears from the list

#### Scenario: Delete collection with items requires confirmation

- GIVEN a collection with 5 items
- WHEN the user initiates deletion
- THEN a confirmation dialog warns that 5 items will be deleted
- AND deletion only proceeds if the user confirms

#### Scenario: User cancels deletion

- GIVEN the confirmation dialog is shown
- WHEN the user cancels
- THEN the collection and its items remain intact
