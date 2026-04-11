# Document Viewer Specification

## Purpose

Defines inline viewing of imported historical documents (images and PDFs) within the item detail view.

## Requirements

### Requirement: Image Rendering

Images MUST render inline using `convertFileSrc()` from `@tauri-apps/api/core` to convert native file paths to webview-safe URLs. Supported formats: PNG, JPG, WEBP, TIFF.

#### Scenario: Image displays inline

- GIVEN an item has an image asset `carta.jpg` stored on disk
- WHEN the user opens the item detail view
- THEN the image renders inline via a webview-safe `asset://` URL
- AND no `file://` protocol errors occur

#### Scenario: High-resolution image scales to fit

- GIVEN an image is 4000x3000 pixels
- WHEN it renders in the viewer
- THEN it scales to fit the available panel width
- AND the user can see the full image without horizontal scrolling

### Requirement: PDF Rendering

PDFs MUST render via `pdfjs-dist` (Mozilla PDF.js). The viewer MUST support page navigation (next/previous/go-to-page) and zoom (in/out/fit-to-width). The PDF worker MUST be lazy-loaded.

#### Scenario: PDF displays first page on open

- GIVEN an item has a PDF asset `acta.pdf` with 10 pages
- WHEN the user opens the item detail view
- THEN the first page of the PDF renders in the viewer

#### Scenario: Navigate between pages

- GIVEN a PDF with 10 pages is open, currently showing page 1
- WHEN the user clicks "Next page"
- THEN page 2 renders in the viewer
- AND a page indicator shows "2 / 10"

#### Scenario: Zoom controls

- GIVEN a PDF is displayed at default zoom level
- WHEN the user clicks "Zoom in"
- THEN the PDF renders at a larger scale
- AND "Zoom out" returns to the previous scale

### Requirement: Viewer Layout

The document viewer MUST open in a panel alongside the item metadata — NOT in a modal. The viewer panel and the metadata/notes panel MUST be simultaneously visible.

#### Scenario: Side-by-side layout

- GIVEN the user opens an item with assets
- WHEN the item detail view loads
- THEN the document viewer panel is visible alongside the metadata panel
- AND both panels are usable without switching

### Requirement: Loading State

The viewer MUST show a loading indicator while the document (especially PDF) is being rendered.

#### Scenario: PDF shows loading state

- GIVEN a PDF asset is selected for viewing
- WHEN the PDF is loading and rendering
- THEN a loading indicator is visible in the viewer panel
- AND the indicator disappears once the first page renders

### Requirement: Error State

The viewer MUST show an error state if the asset file is not found on disk or cannot be rendered.

#### Scenario: File not found on disk

- GIVEN an asset record exists but the file was deleted from disk
- WHEN the viewer attempts to display it
- THEN an error message is shown indicating the file could not be found

#### Scenario: Corrupted PDF

- GIVEN a PDF file is corrupted
- WHEN `pdfjs-dist` fails to parse it
- THEN an error message is shown indicating the file cannot be rendered
