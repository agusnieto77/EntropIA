# File Import Specification

## Purpose

Defines how users import historical document files (images and PDFs) into EntropIA, including file picker, drag & drop, file storage, and asset record creation.

## Requirements

### Requirement: File Picker Import

The system MUST allow users to open a native file picker dialog filtered to supported formats (png, jpg, webp, tiff, pdf). Multiple file selection MUST be supported. The dialog MUST use `@tauri-apps/plugin-dialog`.

#### Scenario: User picks files via dialog

- GIVEN the user is viewing a collection's item
- WHEN they click the import button and select one or more files
- THEN the native file picker opens filtered to images (png, jpg, webp, tiff) and PDFs
- AND each selected file is imported into the current item

#### Scenario: User cancels file picker

- GIVEN the file picker dialog is open
- WHEN the user clicks Cancel
- THEN no files are imported
- AND no error is shown

### Requirement: Drag and Drop Import

The system MUST accept files dragged onto the application window using Tauri's built-in `onDragDropEvent()` from `@tauri-apps/api/webview`. Dropped files MUST be validated for supported formats before import.

#### Scenario: User drags supported files onto window

- GIVEN the user is viewing a collection detail or item detail view
- WHEN they drag and drop one or more supported files onto the window
- THEN each valid file is imported into the current context (item or new item)

#### Scenario: User drags unsupported file format

- GIVEN the user drags a `.docx` file onto the window
- WHEN the drop event fires
- THEN the file is rejected
- AND a user-visible message indicates the format is not supported

### Requirement: File Storage

Imported files MUST be copied to `{appDataDir}/files/{collection_id}/{item_id}/` using `@tauri-apps/plugin-fs`. The original filename MUST be preserved. The directory structure MUST be created if it does not exist.

#### Scenario: File copied to structured directory

- GIVEN a file `acta_1810.pdf` is imported for item `item-abc` in collection `coll-123`
- WHEN the import completes
- THEN the file exists at `{appDataDir}/files/coll-123/item-abc/acta_1810.pdf`

#### Scenario: Directory created on first import

- GIVEN no files have been imported for item `item-abc`
- WHEN the first file is imported
- THEN the directory `{appDataDir}/files/coll-123/item-abc/` is created automatically

### Requirement: Asset Record Creation

Each successfully imported file MUST create an `asset` record in the database linked to the item. The asset record MUST store the original filename, MIME type, file size in bytes, and the relative path from `appDataDir`.

#### Scenario: Asset record created on import

- GIVEN a file `carta.jpg` (245KB) is imported for item `item-abc`
- WHEN the import completes
- THEN an `asset` record is inserted with `item_id = 'item-abc'`, `filename = 'carta.jpg'`, `mime_type = 'image/jpeg'`, and `size` in bytes

### Requirement: Duplicate Detection

The system MUST detect when a file with the same filename already exists in the same item's directory. The user MUST be warned and the duplicate MUST NOT be imported automatically.

#### Scenario: Duplicate filename detected

- GIVEN item `item-abc` already has an asset with filename `acta_1810.pdf`
- WHEN the user imports another file named `acta_1810.pdf`
- THEN a warning is shown indicating a duplicate exists
- AND the file is NOT imported

### Requirement: Import Progress

The system SHOULD show progress indication for files larger than 5MB during the copy operation.

#### Scenario: Large file shows progress

- GIVEN the user imports a 25MB PDF
- WHEN the copy operation is in progress
- THEN a progress indicator is visible to the user

#### Scenario: Small file imports instantly

- GIVEN the user imports a 500KB image
- WHEN the import executes
- THEN the import completes without a visible progress bar

### Requirement: Tauri Capabilities

The Tauri application MUST declare the following capability permissions for file import: `fs:allow-copy-file`, `fs:allow-create-dir`, `fs:allow-appdata-read-recursive`, `fs:allow-appdata-write-recursive`, `dialog:allow-open`.

#### Scenario: File operations succeed with correct capabilities

- GIVEN the Tauri capability configuration includes the required FS and dialog permissions
- WHEN the import flow runs
- THEN file copy and directory creation succeed without permission errors
