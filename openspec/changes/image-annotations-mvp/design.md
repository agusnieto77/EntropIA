# Design: image-annotations-mvp

## Technical Approach

Add image-only annotations through the existing Svelte + store layering: `ItemView` owns persisted annotation state, `DocumentViewer` becomes an interactive image stage, and the store gains an `annotations` table/repo. Geometry is stored normalized to the rendered image box (`0..1`), so reopen, resize, and dark-theme layout changes rehydrate without recomputing from natural pixels. PDF rendering stays read-only and does not attach annotation handlers.

## Architecture Decisions

| Decision          | Options                                 | Choice / Rationale                                                                                                                                     |
| ----------------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Annotation schema | JSON blob vs flattened columns          | **Flattened columns**: `asset_id`, `page`, `kind`, `color`, `x`, `y`, `width`, `height`, timestamps. Easier validation, filtering, and Drizzle typing. |
| Save model        | Per-pointer CRUD vs page-scoped replace | **Debounced replace per asset/page** from `ItemView`. Dragging updates local state optimistically; persistence writes the final list, reducing churn.  |
| Overlay tech      | SVG vs canvas                           | **SVG** for MVP. Native hit-testing, selection styling, and keyboard focus are simpler than canvas redraw/state bookkeeping.                           |

## Data Flow

```text
SQLite annotations ← AnnotationRepo.replaceForAssetPage()
        ↑                         ↑
asset change / reopen      debounced optimistic save
        ↑                         ↑
     ItemView  ← onChange / onSelect →  DocumentViewer
                                      (image-only SVG overlay)
```

## Interaction Model

`DocumentViewer` gets `annotations`, `selectedAnnotationId`, `annotationTool`, `annotationColor`, and callbacks. Internal state is a tiny machine: `idle/selecting`, `drawing-rect`, `drawing-underline`. On `pointerdown`, if a drawing tool is active inside image bounds, it creates a draft; `pointermove` updates normalized geometry; `pointerup` commits only if the draft clears a minimum size threshold, otherwise cancels. In select mode, hit-tested SVG shapes select the topmost annotation; clicking empty space clears selection. Color buttons update the selected annotation immediately, otherwise they set the default color for the next draft. Delete removes only the selected annotation.

Underline uses the same bounding-box contract as rectangles: drag defines `x`, `y`, `width`, `height`, but rendering clamps to a horizontal `<line>` centered within that box, which keeps storage uniform while preserving a reliable hit area.

## Overlay Rendering Strategy

Wrap the `<img>` in a relative stage and render an absolutely positioned `<svg>` over the measured image element, not the scroll container. `bind:this={imgEl}` plus a `ResizeObserver` on the image/stage keep `clientWidth`/`clientHeight` current under `max-width`, `max-height`, and `object-fit: contain`. Normalized geometry maps as `px = normalized * renderedDimension`; reverse mapping clamps pointer coordinates to `0..1`. If the image has not loaded or bounds are zero, the overlay stays inert and the base image still renders.

## Persistence and Store Updates

`ItemView.svelte` adds `annotations`, `selectedAnnotationId`, `annotationTool`, `annotationColor`, `annotationSaveTimer`, and `annotationSaveError`. On selected asset change it clears selection, flushes any pending save for the previous image asset, then loads `store.annotations.findByAsset(asset.id, 1)`. Viewer edits update local state immediately and schedule `replaceForAssetPage(asset.id, 1, annotations)` after ~400–500 ms. Failures surface as non-blocking UI error text while keeping local edits visible for retry.

Store work:

- `packages/store/src/runner.ts`: add migration `0007_annotations`.
- `packages/store/src/schema.ts`: add `annotations` table and indexes on `asset_id` and `(asset_id, page)`.
- `packages/store/src/repos/annotation.repo.ts`: `findByAsset`, `replaceForAssetPage`.
- `packages/store/src/repos/store.ts` and `src/index.ts`: wire/export `annotations`.
- `packages/store/src/repos/asset.repo.ts`: extend cascade delete to remove annotations explicitly with other asset dependents.

## Interfaces / Contracts

```ts
type AnnotationKind = 'rectangle' | 'underline'
interface Annotation {
  id: string
  assetId: string
  page: number
  kind: AnnotationKind
  color: string
  x: number
  y: number
  width: number
  height: number
  createdAt: number
  updatedAt: number
}
```

## Error Handling, Accessibility, UX

Persistence errors never block viewing: annotations remain in local state, saves retry on the next edit, and reopen falls back to the last successful DB snapshot. Invalid geometry is clamped before render/save. The toolbar uses existing dark-theme tokens (`--color-surface`, `--color-surface-raised`, `--color-border`, `--color-accent`) as a discreet floating panel over the image. Active tool/color buttons use `aria-pressed`; delete is keyboard reachable; selected annotations get a higher-contrast outline; PDF mode hides or disables annotation controls and keeps current PDF controls unchanged.

## File Changes

| File                                                                    | Action | Description                             |
| ----------------------------------------------------------------------- | ------ | --------------------------------------- |
| `packages/store/src/runner.ts`                                          | Modify | Register `0007_annotations` migration   |
| `packages/store/src/schema.ts`                                          | Modify | Add annotations table                   |
| `packages/store/src/repos/annotation.repo.ts`                           | Create | Asset/page annotation persistence       |
| `packages/store/src/repos/store.ts`                                     | Modify | Expose `annotations` repo               |
| `packages/store/src/index.ts`                                           | Modify | Export annotation types/repo            |
| `packages/store/src/repos/asset.repo.ts`                                | Modify | Delete annotations during asset cascade |
| `packages/ui/src/components/AnnotationToolbar/AnnotationToolbar.svelte` | Create | Dark floating toolbar                   |
| `packages/ui/src/components/DocumentViewer/*`                           | Modify | Overlay, pointer logic, props/tests     |
| `apps/desktop/src/views/ItemView.svelte`                                | Modify | Load, optimistic edit, debounce save    |

## Testing Strategy

| Layer       | What to Test                                           | Approach                             |
| ----------- | ------------------------------------------------------ | ------------------------------------ |
| Unit        | Repo replace/load, geometry clamp/mapping              | Vitest repo + helper tests           |
| Component   | Tool switching, draw/select/delete, PDF inactive state | `DocumentViewer.test.ts`             |
| Integration | Asset-change rehydrate and debounced persistence       | `ItemView.test.ts` with mocked store |

## Migration / Rollout

Migration `0007_annotations` is additive only; no backfill. Image assets always save with `page = 1`, reserving page scoping for future PDF support.

## Open Questions

- [ ] None blocking for MVP.
