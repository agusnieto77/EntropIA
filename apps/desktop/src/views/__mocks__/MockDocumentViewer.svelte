<script lang="ts">
  let {
    type = 'image',
    annotations = [],
    selectedAnnotationId = null,
    annotationTool = 'select',
    annotationColor = 'var(--color-accent)',
    onAnnotationsChange = () => {},
    onSelectedAnnotationIdChange = () => {},
    onAnnotationToolChange = () => {},
    onAnnotationColorChange = () => {},
  } = $props()

  function createDraftAnnotation() {
    return {
      id: crypto.randomUUID(),
      assetId: '',
      page: 1,
      kind: 'rectangle',
      color: annotationColor,
      x: 0.1,
      y: 0.2,
      width: 0.3,
      height: 0.4,
      createdAt: 1,
      updatedAt: 1,
    }
  }
</script>

<div data-testid="mock-document-viewer">
  <p data-testid="viewer-type">{type}</p>
  <p data-testid="viewer-annotation-count">{annotations.length}</p>
  <p data-testid="viewer-selected-annotation">{selectedAnnotationId ?? 'none'}</p>
  <p data-testid="viewer-annotation-tool">{annotationTool}</p>
  <p data-testid="viewer-annotation-color">{annotationColor}</p>

  <button
    type="button"
    onclick={() => onAnnotationsChange([...annotations, createDraftAnnotation()])}
  >
    Add annotation
  </button>
  <button
    type="button"
    onclick={() => onSelectedAnnotationIdChange(annotations[0]?.id ?? 'missing-annotation')}
  >
    Select annotation
  </button>
  <button type="button" onclick={() => onAnnotationToolChange('rectangle')}>Rectangle tool</button>
  <button type="button" onclick={() => onAnnotationColorChange('var(--color-warning)')}>
    Warning color
  </button>
</div>
