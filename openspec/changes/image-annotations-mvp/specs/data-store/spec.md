# Delta for data-store

## ADDED Requirements

### Requirement: Annotation Persistence Contract

The data store MUST persist annotations through the same SQLite/store boundary used for metadata. Each persisted annotation MUST include asset scope, annotation type, color, normalized geometry, and `page`.

#### Scenario: Save and reload annotations for one asset

- GIVEN an asset has annotations saved through the store layer
- WHEN the asset is loaded again
- THEN those annotations are returned with the same type, color, geometry, and page values

#### Scenario: Annotation records stay asset-scoped

- GIVEN two assets belong to the same item and only one has annotations
- WHEN annotations are queried for each asset
- THEN only the annotated asset returns records

### Requirement: Forward-Compatible Page Scope

The data store MUST include a `page` field for annotation records to preserve forward compatibility. Image annotations MUST use page `1` for this MVP.

#### Scenario: Image annotations persist with page 1

- GIVEN an image annotation is saved
- WHEN the persisted record is read back
- THEN its `page` value is `1`

#### Scenario: Page does not change asset scoping

- GIVEN annotations are stored for an image asset with page `1`
- WHEN that asset is reopened
- THEN the same annotations are restored for that asset without multi-page behavior
