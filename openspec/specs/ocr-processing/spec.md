# OCR Processing Specification

## Purpose

Defines the end-to-end OCR job lifecycle: how a user triggers text extraction, how the system queues and processes jobs, and how results and errors are communicated.

## Requirements

### Requirement: OCR Job Lifecycle

The system MUST support a job state machine with states: `pending → running → done | error`. Each state transition MUST be persisted to the `jobs` table and emitted as a Tauri event.

#### Scenario: Job transitions through full lifecycle

- GIVEN an asset exists in the database
- WHEN `start_ocr_job(asset_id)` is invoked
- THEN a job row is created with status `pending` and a `job_id` is returned
- AND the job transitions to `running` when the worker picks it up
- AND the job transitions to `done` when extraction completes successfully

#### Scenario: Job transitions to error on failure

- GIVEN an OCR job is running
- WHEN the extraction engine encounters an unrecoverable error
- THEN the job status is set to `error`
- AND the error message is stored in the `jobs` table
- AND an `ocr:error` event is emitted with the `job_id` and error details

#### Scenario: Only one job runs at a time

- GIVEN one job is already in `running` state
- WHEN a second `start_ocr_job` call is made
- THEN the second job is queued with status `pending`
- AND it does not start until the first job completes or errors

---

### Requirement: PDF Native Text Extraction

The system MUST attempt to extract text from the PDF's native text layer first. If the extracted content contains fewer than 50 valid alphanumeric characters, the system MUST fall back to image-based OCR.

#### Scenario: PDF with rich native text layer

- GIVEN a born-digital PDF asset with a text layer containing ≥ 50 valid chars
- WHEN OCR processing starts for this asset
- THEN the native text layer is extracted
- AND the extraction method is recorded as `native`
- AND no OCR inference is performed

#### Scenario: PDF with sparse or absent text layer (fallback)

- GIVEN a scanned PDF asset whose native text layer has < 50 valid alphanumeric characters
- WHEN OCR processing starts
- THEN the system falls back to image-based OCR
- AND the extraction method is recorded as `ocr`

#### Scenario: PDF with zero-byte text layer

- GIVEN a PDF with an empty or missing text layer
- WHEN OCR processing starts
- THEN the system immediately proceeds to image OCR without reporting an error

---

### Requirement: Image OCR with Preprocessing

The system MUST preprocess images before sending them to the `ocrs` inference engine. The preprocessing pipeline MUST apply: grayscale conversion → adaptive threshold → resize (if needed).

#### Scenario: Image preprocessed before inference

- GIVEN a colour image asset (JPEG or PNG)
- WHEN OCR processing starts
- THEN the image is converted to grayscale
- AND an adaptive threshold is applied to binarise it
- AND the processed image is passed to `ocrs` for inference

#### Scenario: OCR returns text for preprocessed image

- GIVEN a preprocessed grayscale-and-thresholded image
- WHEN `ocrs` inference completes
- THEN the extracted text is non-empty
- AND the extraction method is recorded as `ocr`

---

### Requirement: Non-Blocking OCR Execution

The OCR job MUST execute in a background worker and MUST NOT block the Tauri UI thread or the IPC bridge. The user MUST be able to interact with the application while a job is running.

#### Scenario: UI remains interactive during OCR

- GIVEN an OCR job is in `running` state for a large PDF
- WHEN the user navigates to another collection or item
- THEN the navigation completes normally without delay
- AND OCR continues processing in the background

#### Scenario: Progress events arrive while navigating

- GIVEN an OCR job is running
- WHEN the user is on a different view
- THEN `ocr:progress` events continue to be emitted
- AND the frontend state is updated even though the ItemView is not active

---

### Requirement: Manual Trigger (No Auto-Process)

The system MUST NOT automatically start OCR when an asset is imported. OCR MUST only start when the user explicitly triggers "Extract Text" for a specific asset.

#### Scenario: Import does not trigger OCR

- GIVEN a user imports a PDF file into an item
- WHEN the import completes
- THEN no OCR job is created or queued for the new asset
- AND the asset status remains without an extraction

#### Scenario: User explicitly triggers extraction

- GIVEN an asset has no extraction
- WHEN the user clicks "Extract Text" for that asset
- THEN `start_ocr_job(asset_id)` is called
- AND a job is created and queued

---

### Requirement: Re-Processing

The system MUST allow re-running OCR on an asset that already has an extraction. The new result MUST overwrite the previous extraction.

#### Scenario: Re-run overwrites previous extraction

- GIVEN an asset already has an extraction record
- WHEN the user triggers "Extract Text" again
- THEN a new OCR job is queued
- AND upon completion the previous extraction row is replaced with the new result
- AND the `method` and `created_at` fields reflect the new run

#### Scenario: Re-run is available regardless of previous method

- GIVEN an asset was previously extracted via `native` method
- WHEN the user re-triggers extraction
- THEN the job runs the full PDF/image pipeline again
- AND the result may record `native` or `ocr` depending on quality heuristic

---

### Requirement: OCR Error Handling

Failed OCR jobs MUST store the error reason and MUST NOT leave orphan state. The UI MUST reflect the error state per asset.

#### Scenario: Error stored in jobs table

- GIVEN an OCR job fails (e.g., corrupted file, inference crash)
- WHEN the error occurs
- THEN the job row is updated to status `error`
- AND an error message is persisted in the `jobs.error` column
- AND an `ocr:error` event is emitted to the frontend

#### Scenario: Subsequent re-run allowed after error

- GIVEN an asset's last job is in `error` state
- WHEN the user clicks "Extract Text" again
- THEN a new job is created with status `pending`
- AND the previous errored job remains in history (not deleted)
