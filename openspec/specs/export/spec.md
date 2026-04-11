# Export Specification

## Purpose

Defines JSON export of collection data for backup and interoperability.

## Requirements

### Requirement: Export Collection to JSON

The system MUST allow users to export a collection as a JSON file. The export MUST include: collection metadata, all items with their metadata, relative asset paths, and note content. The export MUST NOT include binary file contents.

#### Scenario: Full collection export

- GIVEN a collection with 3 items, each with assets and notes
- WHEN the user triggers export
- THEN a JSON file is generated containing the collection, all items, asset references (relative paths), and notes

#### Scenario: Export structure

- GIVEN a collection is exported
- WHEN the JSON file is inspected
- THEN it contains a `version` field, `exportedAt` timestamp, `collection` object, and `items` array
- AND each item contains `assets` (with filename, type, size) and `notes` (with content, timestamp)

#### Scenario: Asset paths are relative

- GIVEN an asset stored at `{appDataDir}/files/coll-123/item-abc/acta.pdf`
- WHEN the collection is exported
- THEN the asset path in JSON is relative (e.g., `files/coll-123/item-abc/acta.pdf`)
- AND no absolute system paths are included

### Requirement: Save Dialog

The system MUST present a native save dialog (via `@tauri-apps/plugin-dialog`) for the user to choose the export file destination. The default filename SHOULD be `{collection-name}.json`.

#### Scenario: Save dialog with default filename

- GIVEN the user exports collection "Archivo Municipal"
- WHEN the save dialog opens
- THEN the default filename is "Archivo Municipal.json"

#### Scenario: User cancels export

- GIVEN the save dialog is open
- WHEN the user cancels
- THEN no file is written
- AND no error is shown

### Requirement: Export Excludes Binaries

The export MUST NOT include binary file contents (images, PDFs). Only metadata and paths are included.

#### Scenario: No binary data in export

- GIVEN a collection with image and PDF assets
- WHEN the collection is exported as JSON
- THEN the JSON file size is proportional to metadata only
- AND no base64-encoded file contents are present
