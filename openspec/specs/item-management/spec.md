# Item Management Specification

## Purpose

Defines CRUD operations for items within a collection, including metadata editing and item listing with pagination.

## Requirements

### Requirement: Create Item

The system MUST allow users to create an item inside a collection. Title is REQUIRED. The system MUST generate a unique ID, set timestamps, and link the item to its parent collection.

#### Scenario: Create item with title

- GIVEN the user is viewing collection "Archivo Municipal"
- WHEN they create a new item with title "Acta de cabildo 1810-05-25"
- THEN an item is created linked to that collection
- AND it appears in the collection's item list

#### Scenario: Reject empty title

- GIVEN the create item form is open
- WHEN the user submits without entering a title
- THEN the item is NOT created
- AND a validation error is shown

### Requirement: List Items in Collection

The system MUST display items within a collection sorted by `created_at` descending. The list MUST be paginated at 50 items per page. Each item MUST show its title, asset count, and a thumbnail of its first asset (if any).

#### Scenario: Items listed with pagination

- GIVEN a collection has 75 items
- WHEN the user views the collection detail
- THEN the first 50 items are shown sorted by newest first
- AND a control to load the next page is available

#### Scenario: Item shows asset thumbnail

- GIVEN an item has 3 assets, the first being an image
- WHEN the item is displayed in the list
- THEN a thumbnail of the first asset is shown

#### Scenario: Item with no assets

- GIVEN an item has zero assets
- WHEN it is displayed in the list
- THEN a placeholder icon is shown instead of a thumbnail

### Requirement: Edit Item Metadata

The system MUST allow users to edit an item's title and metadata. Metadata MUST be presented as editable key-value pairs via a UI editor. The metadata MUST be stored as a JSON object in the `metadata` column.

#### Scenario: Edit item title

- GIVEN an item with title "Untitled"
- WHEN the user changes the title to "Carta al gobernador"
- THEN the title is updated in the database
- AND the updated title is reflected in the UI

#### Scenario: Add metadata key-value pair

- GIVEN an item with no metadata
- WHEN the user adds key "date" with value "1810-05-25"
- THEN the metadata is saved as `{"date": "1810-05-25"}`

#### Scenario: Edit existing metadata

- GIVEN an item with metadata `{"date": "1810-05-25"}`
- WHEN the user changes the value of "date" to "1810-05-26"
- THEN the metadata updates to `{"date": "1810-05-26"}`

#### Scenario: Remove metadata key

- GIVEN an item with metadata `{"date": "1810-05-25", "author": "Moreno"}`
- WHEN the user removes the "author" key
- THEN the metadata updates to `{"date": "1810-05-25"}`

### Requirement: Delete Item

The system MUST allow users to delete an item. If the item has assets, a confirmation dialog MUST be shown. Deleting an item MUST cascade-delete its assets and notes. Asset files on disk SHOULD be removed.

#### Scenario: Delete item with assets requires confirmation

- GIVEN an item with 2 assets
- WHEN the user initiates deletion
- THEN a confirmation dialog warns about the assets
- AND deletion proceeds only on confirmation

#### Scenario: Delete item with no assets

- GIVEN an item with zero assets
- WHEN the user deletes it
- THEN the item is removed without confirmation
