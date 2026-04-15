# Proposal: image-annotations-mvp

## Intent

Add lightweight visual markup to image assets in the item detail view so users can preserve evidentiary regions without external tools.

## Scope

### In Scope

- Overlay toolbar for image assets with rectangle, underline, four preset colors, and delete-selected annotation.
- Asset-scoped annotation persistence in SQLite with normalized `0..1` coordinates and a `page` field reserved for future PDF support.
- `ItemView` load/save flow plus `DocumentViewer` SVG overlay that tracks the real rendered image bounds under `object-fit: contain`.

### Out of Scope

- PDF annotation UI, text-layer semantic underline, and multi-page workflows.
- Freehand drawing, export/sharing, collaboration, undo/redo, or bulk operations.

## Capabilities

### New Capabilities

- `image-annotations`: create, render, recolor, persist, and delete visual annotations on image assets.

### Modified Capabilities

- `document-viewer`: image mode gains overlay, selection, and toolbar behavior while PDF mode stays read-only.
- `data-store`: schema and repo surface gain asset-scoped `annotations` persistence with forward-compatible `page` scoping.

## Approach

Add inline migration `0007_annotations` in `packages/store/src/runner.ts`, extend the Drizzle schema, and introduce `AnnotationRepo` following `NoteRepo`. `ItemView` owns annotation state like notes, reloading on asset change and debounce-saving edits. `DocumentViewer` receives annotations plus callbacks, measures the displayed image box, and renders SVG shapes from normalized coordinates so resize and zoom remain stable.

## Affected Areas

| Area                                                                    | Impact   | Description                             |
| ----------------------------------------------------------------------- | -------- | --------------------------------------- |
| `packages/store/src/runner.ts`                                          | Modified | Register inline `annotations` migration |
| `packages/store/src/schema.ts`                                          | Modified | Add `annotations` table                 |
| `packages/store/src/repos/annotation.repo.ts`                           | New      | CRUD for asset annotations              |
| `packages/store/src/repos/store.ts`                                     | Modified | Expose `annotations` repo               |
| `packages/ui/src/components/AnnotationToolbar/AnnotationToolbar.svelte` | New      | Toolbar UI                              |
| `packages/ui/src/components/DocumentViewer/DocumentViewer.svelte`       | Modified | SVG overlay + interaction layer         |
| `packages/ui/src/components/DocumentViewer/DocumentViewer.types.ts`     | Modified | Annotation props/events                 |
| `apps/desktop/src/views/ItemView.svelte`                                | Modified | Asset-scoped load/save integration      |

## Risks

| Risk                                     | Likelihood | Mitigation                                                                 |
| ---------------------------------------- | ---------- | -------------------------------------------------------------------------- |
| Overlay drift from `object-fit: contain` | Med        | Map pointer/render coordinates against measured rendered-image bounds only |
| Save churn while dragging                | Med        | Keep local optimistic state and debounce persistence                       |
| Future PDF page mismatch                 | Low        | Store `page` now and fix image MVP to page `1`                             |

## Rollback Plan

Remove toolbar and overlay wiring, delete `AnnotationRepo` usage, and stop registering migration `0007_annotations` for fresh installs. Existing databases can safely leave the new table unused or drop it in a follow-up rollback migration.

## Dependencies

- Existing `NoteRepo` CRUD pattern and store export structure.
- Existing `ItemView` asset-selection flow and `DocumentViewer` image rendering.

## Success Criteria

- [ ] Users can add rectangle and underline annotations to image assets.
- [ ] Selected annotations can change color and be deleted individually.
- [ ] Reopening an item restores annotations in the correct positions after resize.
- [ ] PDFs remain viewable with annotation controls inactive for this MVP.
