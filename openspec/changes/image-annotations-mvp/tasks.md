# Tasks: Image Annotations MVP

## Phase 1: Data Layer

- [x] 1.1 Add `0007_annotations` SQL in `packages/store/src/runner.ts` with `annotations` table plus `asset_id` and `(asset_id, page)` indexes. Depends: none. Complexity: Medium.
- [x] 1.2 Extend `packages/store/src/schema.ts` with the typed `annotations` table (`assetId`, `page`, `kind`, `color`, `x`, `y`, `width`, `height`, timestamps). Depends: 1.1. Complexity: Medium.
- [x] 1.3 Create `packages/store/src/repos/annotation.repo.ts` with annotation types and CRUD methods (`create`, `findByAsset`, `replaceForAssetPage`, `delete`) following `NoteRepo` patterns. Depends: 1.2. Complexity: Complex.
- [x] 1.4 Wire the repo through `packages/store/src/repos/store.ts` and `packages/store/src/index.ts`, then extend `packages/store/src/repos/asset.repo.ts` cascade cleanup to remove annotations. Depends: 1.3. Complexity: Medium.

## Phase 2: UI Components

- [x] 2.1 Expand `packages/ui/src/components/DocumentViewer/DocumentViewer.types.ts` with annotation models, toolbar state, and change/select callbacks. Depends: 1.4. Complexity: Medium.
- [x] 2.2 Create `packages/ui/src/components/AnnotationToolbar/AnnotationToolbar.svelte` for select/rectangle/underline tools, four preset colors, and delete-selected action with dark-theme styling. Depends: 2.1. Complexity: Medium.
- [x] 2.3 Refactor `packages/ui/src/components/DocumentViewer/DocumentViewer.svelte` into a measured image stage that renders the toolbar and an absolute SVG overlay only for image assets. Depends: 2.1, 2.2. Complexity: Complex.
- [x] 2.4 Implement pointer interactions in `packages/ui/src/components/DocumentViewer/DocumentViewer.svelte` for draft rectangle/underline creation, topmost selection, recolor, delete, and empty-space deselect. Depends: 2.3. Complexity: Complex.

## Phase 3: Integration

- [x] 3.1 Update `apps/desktop/src/views/ItemView.svelte` to own `annotations`, `selectedAnnotationId`, `annotationTool`, `annotationColor`, and `annotationSaveError` state per selected asset. Depends: 1.4, 2.4. Complexity: Complex.
- [x] 3.2 Wire annotation load/reload in `apps/desktop/src/views/ItemView.svelte` via `store.annotations.findByAsset(asset.id, 1)` and clear/flush state on asset changes. Depends: 3.1. Complexity: Medium.
- [x] 3.3 Add coordinate normalization/denormalization and rendered-image bounds measurement between `ItemView.svelte` and `DocumentViewer.svelte`, keeping overlay inert until bounds are valid. Depends: 2.3, 3.1. Complexity: Complex.
- [x] 3.4 Implement debounced persistence in `apps/desktop/src/views/ItemView.svelte` with `replaceForAssetPage(asset.id, 1, annotations)`, optimistic local edits, and retry-friendly error handling. Depends: 3.2, 3.3. Complexity: Complex.
- [x] 3.5 Preserve PDF view-only behavior in `packages/ui/src/components/DocumentViewer/DocumentViewer.svelte` and `apps/desktop/src/views/ItemView.svelte` by hiding or disabling annotation controls for PDFs. Depends: 2.3, 3.1. Complexity: Medium.

## Phase 4: Testing

- [x] 4.1 Add `packages/store/src/repos/annotation.repo.test.ts` for CRUD, asset-scoped isolation, `page = 1` persistence, and replace semantics. Depends: 1.4. Complexity: Medium.
- [x] 4.2 Update `packages/store/src/repos/asset.repo.test.ts` and `packages/store/src/repos/store.test.ts` for migration wiring and cascade deletion of annotations. Depends: 1.4. Complexity: Medium.
- [x] 4.3 Extend `packages/ui/src/components/DocumentViewer/__tests__/DocumentViewer.test.ts` for toolbar visibility, draw/select/recolor/delete flows, overlay alignment after resize, and PDF inactivity. Depends: 2.4, 3.5. Complexity: Complex.
- [x] 4.4 Extend `apps/desktop/src/views/ItemView.test.ts` with mocked annotation load/save, asset-switch rehydration, debounce timing, and non-blocking save failure handling. Depends: 3.4. Complexity: Complex.
