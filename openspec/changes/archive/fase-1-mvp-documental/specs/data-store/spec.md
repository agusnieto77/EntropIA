# Delta for Data Store

## ADDED Requirements

### Requirement: Collection Repository

`packages/store` MUST export a `CollectionRepo` class that provides: `create(name, description?)`, `findAll()`, `findById(id)`, `update(id, data)`, `delete(id)`, and `countItems(id)`. All methods MUST accept a `DrizzleClient` via constructor injection.

#### Scenario: Create and retrieve a collection

- GIVEN a `CollectionRepo` with a valid DB client
- WHEN `create({ name: 'Test' })` is called
- THEN a new row is inserted in `collections`
- AND `findAll()` returns the created collection

#### Scenario: Count items in collection

- GIVEN a collection with 5 items
- WHEN `countItems(collectionId)` is called
- THEN it returns `5`

### Requirement: Item Repository

`packages/store` MUST export an `ItemRepo` class that provides: `create(collectionId, title)`, `findByCollection(collectionId, options?)`, `findById(id)`, `update(id, data)`, `delete(id)`, and `searchByText(term, collectionId?)`.

#### Scenario: List items by collection with pagination

- GIVEN a collection with 75 items
- WHEN `findByCollection(id, { limit: 50, offset: 0 })` is called
- THEN it returns the first 50 items sorted by `created_at` descending

#### Scenario: Search items by text using LIKE

- GIVEN items with titles "Acta de cabildo" and "Carta al gobernador"
- WHEN `searchByText('cabildo')` is called
- THEN it returns only the "Acta de cabildo" item

#### Scenario: Search metadata content

- GIVEN an item with metadata `{"author": "Moreno"}`
- WHEN `searchByText('Moreno')` is called
- THEN that item is included in results

### Requirement: Asset Repository

`packages/store` MUST export an `AssetRepo` class that provides: `create(itemId, filename, mimeType, size, path)`, `findByItem(itemId)`, and `delete(id)`.

#### Scenario: Create and list assets for item

- GIVEN an `AssetRepo` with a valid DB client
- WHEN `create(itemId, 'doc.pdf', 'application/pdf', 1024, 'files/c/i/doc.pdf')` is called
- THEN `findByItem(itemId)` returns the asset

### Requirement: Note Repository

`packages/store` MUST export a `NoteRepo` class that provides: `create(itemId, content)`, `findByItem(itemId)`, `update(id, content)`, and `delete(id)`.

#### Scenario: Create and list notes for item

- GIVEN a `NoteRepo` with a valid DB client
- WHEN `create(itemId, 'Revisar fecha')` is called
- THEN `findByItem(itemId)` returns the note sorted by `created_at` descending

### Requirement: DrizzleClient Type Export

`packages/store` MUST export a `DrizzleClient` TypeScript type that represents the Drizzle instance. Repository constructors MUST accept this type for dependency injection and testability.

#### Scenario: Repos accept injected client

- GIVEN a mock `DrizzleClient` conforming to the exported type
- WHEN passed to a repository constructor
- THEN the repository operates using the mock without errors
