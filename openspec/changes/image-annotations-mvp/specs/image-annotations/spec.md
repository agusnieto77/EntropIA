# image-annotations Specification

## Purpose

Defines image-first authoring and editing of lightweight visual annotations in the item detail view.

## Requirements

### Requirement: Image Annotation Authoring

The system MUST allow rectangle highlight and underline annotations on image assets. Each annotation MUST be asset-scoped, MUST use normalized `0..1` coordinates, and MUST persist `page = 1` for image assets.

#### Scenario: Create a rectangle highlight

- GIVEN an image asset is open in the detail view
- WHEN the user draws with the rectangle tool
- THEN a semi-transparent rectangle annotation is created on that asset

#### Scenario: Create an underline annotation

- GIVEN an image asset is open in the detail view
- WHEN the user draws with the underline tool
- THEN a horizontal underline annotation is created with normalized geometry

### Requirement: Annotation Editing

The system MUST allow users to select a single annotation, change its color, and delete it individually. The toolbar SHOULD stay discreet and coherent with the dark UI.

#### Scenario: Recolor a selected annotation

- GIVEN an annotation is selected on an image asset
- WHEN the user chooses another color
- THEN only that annotation changes color

#### Scenario: Delete a selected annotation

- GIVEN an annotation is selected on an image asset
- WHEN the user deletes it
- THEN that annotation is removed from the viewer state
