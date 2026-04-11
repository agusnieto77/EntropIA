# OCR UX Specification

## Purpose

Defines the user-facing interface elements for OCR: the trigger button, progress indicator, extracted text panel, per-asset status badge, and the constraint that the UI remains interactive during background OCR.

## Requirements

### Requirement: Extract Text Button

`ItemView` MUST display an "Extract Text" button for each asset. The button MUST be disabled while a job for that asset is in `pending` or `running` state. The button MUST be enabled otherwise (`done`, `error`, or no job).

#### Scenario: Button enabled when no extraction exists

- GIVEN an asset with no OCR job history
- WHEN `ItemView` renders that asset
- THEN the "Extract Text" button is visible and enabled

#### Scenario: Button disabled while job running

- GIVEN an asset whose latest job has status `running`
- WHEN `ItemView` renders
- THEN the "Extract Text" button is disabled
- AND a tooltip or label indicates extraction is in progress

#### Scenario: Button re-enabled after job completes

- GIVEN an asset whose job transitions from `running` to `done`
- WHEN the `ocr:complete` event is received
- THEN the "Extract Text" button becomes enabled again

#### Scenario: Button re-enabled after job errors

- GIVEN an asset whose job transitions to `error`
- WHEN the `ocr:error` event is received
- THEN the "Extract Text" button becomes enabled to allow retry

---

### Requirement: Per-Asset Progress Indicator

`ItemView` MUST show a progress indicator (0–100%) for the asset whose job is `running`. The indicator MUST update in real time as `ocr:progress` events arrive.

#### Scenario: Progress indicator appears when job starts

- GIVEN the user clicks "Extract Text"
- WHEN the job transitions to `running`
- THEN a progress indicator is displayed for that asset
- AND it shows `0%` initially

#### Scenario: Progress updates as events arrive

- GIVEN a job is running and `ocr:progress` events are emitted with `{ job_id, progress: 45 }`
- WHEN the event arrives at the frontend
- THEN the progress indicator for that asset updates to `45%`

#### Scenario: Progress indicator disappears on completion

- GIVEN a job completes and `ocr:complete` is emitted
- WHEN the event arrives
- THEN the progress indicator is hidden
- AND the extracted text panel becomes visible

---

### Requirement: Extracted Text Panel

`ItemView` MUST render a collapsible read-only text panel for each asset that has a completed extraction. The panel MUST display the full `text_content` returned by `ExtractionRepo.findByAsset`.

#### Scenario: Panel renders after successful extraction

- GIVEN an asset with status `done` and a non-empty `text_content`
- WHEN `ItemView` renders
- THEN a collapsible text panel is visible for that asset
- AND it displays the extracted text as read-only

#### Scenario: Panel is collapsed by default

- GIVEN an asset with a completed extraction
- WHEN `ItemView` first renders
- THEN the text panel is collapsed (not showing full content)
- AND the user can click to expand it

#### Scenario: Panel not shown when no extraction exists

- GIVEN an asset with no extraction
- WHEN `ItemView` renders
- THEN no text panel is rendered for that asset

#### Scenario: Panel shows error message on error state

- GIVEN an asset whose last job is in `error` state
- WHEN `ItemView` renders
- THEN an error message is shown instead of the text panel
- AND the message includes an indication that extraction failed

---

### Requirement: Per-Asset Status Badge

The asset list within `ItemView` MUST display a status badge for each asset. The badge MUST reflect the current OCR state: `none` | `pending` | `running` | `done` | `error`.

#### Scenario: Badge shows correct state per asset

- GIVEN three assets: one with no job, one with status `running`, one with status `done`
- WHEN `ItemView` renders the asset list
- THEN the first asset shows no badge (or a neutral indicator)
- AND the second shows a `running` badge
- AND the third shows a `done` badge

#### Scenario: Badge updates without page reload

- GIVEN an asset badge shows `pending`
- WHEN the `ocr:progress` event arrives changing status to `running`
- THEN the badge updates to `running` reactively

---

### Requirement: Background Operation — UI Interactivity

The application MUST remain fully interactive while an OCR job runs. Navigation, search, and other data operations MUST NOT be blocked or delayed by the background worker.

#### Scenario: Navigation works during OCR

- GIVEN an OCR job is running for an asset
- WHEN the user navigates to the Collections List view
- THEN the navigation completes immediately
- AND the OCR job continues running in the background

#### Scenario: Other IPC commands succeed during OCR

- GIVEN an OCR job is running (worker using `worker_conn`)
- WHEN the frontend issues a `select` IPC command (using `ui_conn`)
- THEN the query returns results without waiting for OCR to finish
