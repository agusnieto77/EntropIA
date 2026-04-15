# Delta for document-viewer

## ADDED Requirements

### Requirement: Image Annotation Overlay Alignment

The viewer MUST render an annotation layer and discreet toolbar for image assets. The viewer MUST map normalized annotation geometry to the current rendered image bounds so annotations remain aligned after fit, resize, and reopen.

#### Scenario: Overlay stays aligned after resize

- GIVEN an image asset has saved normalized annotations
- WHEN the viewer size changes and the image is re-fitted
- THEN annotations remain visually aligned with the same image regions

#### Scenario: Toolbar appears only for image annotation mode

- GIVEN an image asset is selected
- WHEN the viewer enters annotation mode
- THEN the toolbar is visible over the image without replacing the metadata panel

### Requirement: PDF Annotation Inactivity

When the selected asset is a PDF, the viewer MUST remain view-only and annotation controls MUST NOT create, update, or delete annotations. Controls MAY be hidden or disabled.

#### Scenario: PDF remains read-only

- GIVEN the selected asset is a PDF
- WHEN the document viewer loads
- THEN annotation authoring controls are inactive

#### Scenario: Existing image annotations do not bleed into PDF mode

- GIVEN the user previously annotated an image asset in the same item
- WHEN they switch to a PDF asset
- THEN no editable annotation overlay is shown for the PDF
