# Specification: image-annotations-mvp

## Capabilities

### New Capabilities

- `image-annotations`: create, recolor, delete, and restore asset-scoped annotations for image assets.

### Modified Capabilities

- `document-viewer`: render a discreet overlay toolbar and aligned annotation layer for images while keeping PDF annotation controls inactive.
- `data-store`: persist annotations in the SQLite/store layer with normalized geometry and forward-compatible `page` scoping.

## Non-Goals / Out of Scope

- PDF annotation authoring, semantic text underlines, and multi-page editing workflows.
- Freehand drawing, export/sharing, collaboration, undo/redo, and bulk annotation operations.

## Acceptance Criteria

- Users can add rectangle highlight and underline annotations on image assets in the document detail view.
- Users can change annotation color and delete a single selected annotation.
- Closing and reopening the same asset restores annotations in the correct positions after resize.
- Annotation data is stored through the same SQLite/store path used for metadata persistence.
- PDF assets remain view-only for this MVP, with annotation controls inactive.
- The toolbar remains visually discreet and coherent with the dark UI.

## Specification

### image-annotations (new)

#### Requirement: Image Annotation Authoring

The system MUST allow rectangle highlight and underline annotations on image assets. Each annotation MUST be scoped to the selected asset, MUST use normalized coordinates in the `0..1` range, and MUST persist `page = 1` for image assets.

#### Scenario: Create a rectangle highlight

- GIVEN an image asset is open in the detail view
- WHEN the user chooses the rectangle tool and drags over the image
- THEN a semi-transparent rectangle annotation is created on that asset

#### Scenario: Create an underline annotation

- GIVEN an image asset is open in the detail view
- WHEN the user chooses the underline tool and draws across the image
- THEN a horizontal underline annotation is created with normalized geometry

#### Requirement: Annotation Editing

The system MUST allow a user to select an annotation, change its color, and delete it individually. The annotation toolbar SHOULD remain discreet and visually coherent with the dark UI.

#### Scenario: Recolor and delete a selected annotation

- GIVEN an image asset already has an annotation selected
- WHEN the user changes its color or deletes it
- THEN only that annotation is updated or removed

### document-viewer (modified)

#### Requirement: Image Annotation Overlay Alignment

The viewer MUST render the annotation toolbar over image assets and MUST map normalized annotation geometry to the current rendered image bounds so overlays remain aligned after fit, resize, or reopen.

#### Scenario: Restore annotations after reopening and resize

- GIVEN an image asset has saved annotations
- WHEN the user reopens the asset after the viewer size changed
- THEN the annotations render in the correct visual positions on the image

#### Requirement: PDF Annotation Inactivity

When the selected asset is a PDF, the viewer MUST remain view-only and annotation controls MUST NOT create, update, or delete annotations. Controls MAY be hidden or shown disabled.

#### Scenario: PDF stays view-only

- GIVEN the selected asset is a PDF
- WHEN the user opens the document viewer
- THEN annotation creation and editing remain inactive

### data-store (modified)

#### Requirement: Annotation Persistence Contract

The data store MUST persist annotations through the same SQLite/store boundary used for metadata. Persisted records MUST include asset scope, annotation type, color, normalized geometry, and `page`.

#### Scenario: Save and restore annotations for one asset

- GIVEN an asset has one or more saved annotations
- WHEN the asset is loaded again from the store layer
- THEN all saved annotations for that asset are returned with their persisted properties

#### Scenario: Asset-scoped isolation

- GIVEN two assets belong to the same item and only one has annotations
- WHEN the viewer loads each asset
- THEN annotations are returned only for the asset they belong to
