<script lang="ts">
  import AnnotationToolbar from '../AnnotationToolbar/AnnotationToolbar.svelte'
  import type { DocumentViewerProps } from './DocumentViewer.types'
  import type { AnnotationTool, ViewerAnnotation } from './DocumentViewer.types'

  let {
    path: _path,
    type,
    assetUrl,
    annotations = [],
    selectedAnnotationId = null,
    annotationTool = 'select',
    annotationColor = 'var(--color-accent)',
    onAnnotationsChange = () => {},
    onSelectedAnnotationIdChange = () => {},
    onAnnotationToolChange = () => {},
    onAnnotationColorChange = () => {},
  }: DocumentViewerProps = $props()

  const presetColors = [
    { value: 'var(--color-accent)', label: 'Accent' },
    { value: 'var(--color-success)', label: 'Success' },
    { value: 'var(--color-warning)', label: 'Warning' },
    { value: 'var(--color-danger)', label: 'Danger' },
  ]
  const MIN_DRAW_PX = 6

  // PDF state
  let currentPage = $state(1)
  let totalPages = $state(0)
  let zoom = $state(1.0)
  let loading = $state(false)
  let error = $state<string | null>(null)

  let canvasEl: HTMLCanvasElement | undefined = $state()
  let imgEl: HTMLImageElement | undefined = $state()
  let pdfDoc: any = null
  let imageBounds = $state({ width: 0, height: 0 })
  let draft = $state<{
    startX: number
    startY: number
    currentX: number
    currentY: number
    kind: Exclude<AnnotationTool, 'select'>
  } | null>(null)

  const canGoPrev = $derived(currentPage > 1)
  const canGoNext = $derived(currentPage < totalPages)
  const canZoomIn = $derived(zoom < 3.0)
  const canZoomOut = $derived(zoom > 0.5)
  const hasRenderableBounds = $derived(imageBounds.width > 0 && imageBounds.height > 0)

  function clamp01(value: number) {
    return Math.max(0, Math.min(1, value))
  }

  function round(value: number) {
    return Number(value.toFixed(4))
  }

  function createLocalAnnotation(
    nextTool: Exclude<AnnotationTool, 'select'>,
    x: number,
    y: number,
    width: number,
    height: number
  ): ViewerAnnotation {
    const now = Date.now()
    return {
      id: crypto.randomUUID(),
      assetId: '',
      page: 1,
      kind: nextTool,
      color: annotationColor,
      x,
      y,
      width,
      height,
      createdAt: now,
      updatedAt: now,
    }
  }

  function measureImage() {
    imageBounds = {
      width: imgEl?.clientWidth ?? 0,
      height: imgEl?.clientHeight ?? 0,
    }
  }

  function getNormalizedPoint(event: PointerEvent) {
    const target = event.currentTarget as SVGSVGElement
    const rect = target.getBoundingClientRect()
    if (rect.width === 0 || rect.height === 0) {
      return null
    }

    return {
      x: clamp01((event.clientX - rect.left) / rect.width),
      y: clamp01((event.clientY - rect.top) / rect.height),
    }
  }

  function toDraftBox(currentDraft: NonNullable<typeof draft>) {
    const x = Math.min(currentDraft.startX, currentDraft.currentX)
    const y = Math.min(currentDraft.startY, currentDraft.currentY)
    return {
      x: round(x),
      y: round(y),
      width: round(Math.abs(currentDraft.currentX - currentDraft.startX)),
      height: round(Math.abs(currentDraft.currentY - currentDraft.startY)),
    }
  }

  function meetsMinimumSize(box: { width: number; height: number }, kind: AnnotationTool) {
    const minWidth = MIN_DRAW_PX / Math.max(imageBounds.width, 1)
    const minHeight = MIN_DRAW_PX / Math.max(imageBounds.height, 1)
    if (kind === 'underline') {
      return box.width >= minWidth
    }
    return box.width >= minWidth && box.height >= minHeight
  }

  function px(value: number, axis: 'x' | 'y') {
    const dimension = axis === 'x' ? imageBounds.width : imageBounds.height
    return String(Math.round(value * dimension))
  }

  function handleToolbarToolChange(tool: AnnotationTool) {
    onAnnotationToolChange(tool)
    if (tool !== annotationTool) {
      draft = null
    }
  }

  function handleToolbarColorChange(color: string) {
    onAnnotationColorChange(color)

    if (!selectedAnnotationId) {
      return
    }

    onAnnotationsChange(
      annotations.map((annotation) =>
        annotation.id === selectedAnnotationId
          ? { ...annotation, color, updatedAt: Date.now() }
          : annotation
      )
    )
  }

  function handleDeleteSelected() {
    if (!selectedAnnotationId) {
      return
    }

    onAnnotationsChange(annotations.filter((annotation) => annotation.id !== selectedAnnotationId))
    onSelectedAnnotationIdChange(null)
  }

  function handleOverlayPointerDown(event: PointerEvent) {
    if (!hasRenderableBounds || event.button !== 0) {
      return
    }

    const point = getNormalizedPoint(event)
    if (!point) {
      return
    }

    if (annotationTool === 'select') {
      onSelectedAnnotationIdChange(null)
      return
    }

    draft = {
      startX: point.x,
      startY: point.y,
      currentX: point.x,
      currentY: point.y,
      kind: annotationTool,
    }
  }

  function handleOverlayPointerMove(event: PointerEvent) {
    if (!draft) {
      return
    }

    const point = getNormalizedPoint(event)
    if (!point) {
      return
    }

    draft = {
      ...draft,
      currentX: point.x,
      currentY: point.y,
    }
  }

  function finishDraft() {
    if (!draft) {
      return
    }

    const box = toDraftBox(draft)
    const kind = draft.kind
    draft = null

    if (!meetsMinimumSize(box, kind)) {
      return
    }

    onAnnotationsChange([
      ...annotations,
      createLocalAnnotation(kind, box.x, box.y, box.width, box.height),
    ])
    onSelectedAnnotationIdChange(null)
  }

  function handleShapeClick(annotationId: string) {
    onSelectedAnnotationIdChange(annotationId)
  }

  function handleShapePointerDown(event: PointerEvent, annotationId: string) {
    event.stopPropagation()
    handleShapeClick(annotationId)
  }

  function handleShapeKeydown(event: KeyboardEvent, annotationId: string) {
    if (event.key !== 'Enter' && event.key !== ' ') {
      return
    }

    event.preventDefault()
    event.stopPropagation()
    handleShapeClick(annotationId)
  }

  $effect(() => {
    if (type !== 'image' || !imgEl) {
      imageBounds = { width: 0, height: 0 }
      return
    }

    measureImage()
    const observer = new ResizeObserver(() => measureImage())
    observer.observe(imgEl)

    return () => observer.disconnect()
  })

  function resetViewerState() {
    loading = false
    error = null
    currentPage = 1
    totalPages = 0
    zoom = 1.0
    pdfDoc = null
  }

  function activatePdfMode() {
    loading = true
    error = null
  }

  async function loadPdf() {
    try {
      activatePdfMode()
      const pdfjs = await import('pdfjs-dist')
      pdfjs.GlobalWorkerOptions.workerSrc = new URL(
        'pdfjs-dist/build/pdf.worker.min.mjs',
        import.meta.url
      ).href
      const loadingTask = pdfjs.getDocument(assetUrl)
      pdfDoc = await loadingTask.promise
      totalPages = pdfDoc.numPages
      await renderPage()
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to load PDF'
    } finally {
      loading = false
    }
  }

  async function renderPage() {
    if (!pdfDoc || !canvasEl) return
    try {
      const page = await pdfDoc.getPage(currentPage)
      const viewport = page.getViewport({ scale: zoom })
      const context = canvasEl.getContext('2d')
      if (!context) return
      canvasEl.width = viewport.width
      canvasEl.height = viewport.height
      await page.render({ canvasContext: context, viewport }).promise
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to render page'
    }
  }

  function prevPage() {
    if (canGoPrev) {
      currentPage--
      renderPage()
    }
  }

  function nextPage() {
    if (canGoNext) {
      currentPage++
      renderPage()
    }
  }

  function zoomIn() {
    if (canZoomIn) {
      zoom = Math.min(3.0, zoom + 0.25)
      renderPage()
    }
  }

  function zoomOut() {
    if (canZoomOut) {
      zoom = Math.max(0.5, zoom - 0.25)
      renderPage()
    }
  }

  $effect(() => {
    if (type !== 'pdf') {
      resetViewerState()
      return
    }

    activatePdfMode()
    void loadPdf()
  })

  const draftBox = $derived(draft ? toDraftBox(draft) : null)
</script>

<div class="document-viewer">
  {#if type === 'image'}
    <div class="document-viewer__image-container">
      <div class="document-viewer__image-stage">
        <AnnotationToolbar
          tool={annotationTool}
          color={annotationColor}
          hasSelection={selectedAnnotationId !== null}
          colors={presetColors}
          onToolChange={handleToolbarToolChange}
          onColorChange={handleToolbarColorChange}
          onDeleteSelected={handleDeleteSelected}
        />

        <img
          bind:this={imgEl}
          src={assetUrl}
          alt="Document"
          class="document-viewer__image"
          onload={measureImage}
        />

        {#if hasRenderableBounds}
          <svg
            class="document-viewer__overlay"
            data-testid="annotation-overlay"
            role="application"
            aria-label="Image annotation overlay"
            width={imageBounds.width}
            height={imageBounds.height}
            viewBox={`0 0 ${imageBounds.width} ${imageBounds.height}`}
            onpointerdown={handleOverlayPointerDown}
            onpointermove={handleOverlayPointerMove}
            onpointerup={finishDraft}
            onpointerleave={finishDraft}
          >
            {#each annotations as annotation (annotation.id)}
              {#if annotation.kind === 'rectangle'}
                <rect
                  data-testid={`annotation-shape-${annotation.id}`}
                  x={px(annotation.x, 'x')}
                  y={px(annotation.y, 'y')}
                  width={px(annotation.width, 'x')}
                  height={px(annotation.height, 'y')}
                  fill={annotation.color}
                  fill-opacity="0.2"
                  stroke={annotation.id === selectedAnnotationId
                    ? 'var(--color-text-primary)'
                    : annotation.color}
                  stroke-width={annotation.id === selectedAnnotationId ? 2 : 1.5}
                  role="button"
                  tabindex="-1"
                  aria-label={`Select annotation ${annotation.id}`}
                  onclick={(event) => {
                    event.stopPropagation()
                    handleShapeClick(annotation.id)
                  }}
                  onkeydown={(event) => handleShapeKeydown(event, annotation.id)}
                  onpointerdown={(event) => handleShapePointerDown(event, annotation.id)}
                />
              {:else}
                <g>
                  <rect
                    data-testid={`annotation-hitbox-${annotation.id}`}
                    x={px(annotation.x, 'x')}
                    y={px(annotation.y, 'y')}
                    width={px(annotation.width, 'x')}
                    height={px(annotation.height, 'y')}
                    fill="transparent"
                    role="button"
                    tabindex="-1"
                    aria-label={`Select annotation ${annotation.id}`}
                    onclick={(event) => {
                      event.stopPropagation()
                      handleShapeClick(annotation.id)
                    }}
                    onkeydown={(event) => handleShapeKeydown(event, annotation.id)}
                    onpointerdown={(event) => handleShapePointerDown(event, annotation.id)}
                  />
                  <line
                    data-testid={`annotation-shape-${annotation.id}`}
                    x1={px(annotation.x, 'x')}
                    y1={px(annotation.y + annotation.height / 2, 'y')}
                    x2={px(annotation.x + annotation.width, 'x')}
                    y2={px(annotation.y + annotation.height / 2, 'y')}
                    stroke={annotation.id === selectedAnnotationId
                      ? 'var(--color-text-primary)'
                      : annotation.color}
                    stroke-width={Math.max(
                      3,
                      Math.round(annotation.height * imageBounds.height) || 3
                    )}
                    stroke-linecap="round"
                    role="button"
                    tabindex="-1"
                    aria-label={`Select annotation ${annotation.id}`}
                    onclick={(event) => {
                      event.stopPropagation()
                      handleShapeClick(annotation.id)
                    }}
                    onkeydown={(event) => handleShapeKeydown(event, annotation.id)}
                    onpointerdown={(event) => handleShapePointerDown(event, annotation.id)}
                  />
                </g>
              {/if}
            {/each}

            {#if draftBox}
              {#if draft?.kind === 'rectangle'}
                <rect
                  x={px(draftBox.x, 'x')}
                  y={px(draftBox.y, 'y')}
                  width={px(draftBox.width, 'x')}
                  height={px(draftBox.height, 'y')}
                  fill={annotationColor}
                  fill-opacity="0.14"
                  stroke={annotationColor}
                  stroke-dasharray="6 4"
                  stroke-width="1.5"
                />
              {:else}
                <line
                  x1={px(draftBox.x, 'x')}
                  y1={px(draftBox.y + draftBox.height / 2, 'y')}
                  x2={px(draftBox.x + draftBox.width, 'x')}
                  y2={px(draftBox.y + draftBox.height / 2, 'y')}
                  stroke={annotationColor}
                  stroke-width={Math.max(3, Math.round(draftBox.height * imageBounds.height) || 3)}
                  stroke-dasharray="6 4"
                  stroke-linecap="round"
                />
              {/if}
            {/if}
          </svg>
        {/if}
      </div>
    </div>
  {:else}
    {#if loading}
      <div class="document-viewer__loading" data-testid="pdf-loading">
        <span class="document-viewer__spinner" aria-hidden="true"></span>
        <span>Loading PDF...</span>
      </div>
    {/if}

    {#if error}
      <div class="document-viewer__error" data-testid="pdf-error" role="alert">
        {error}
      </div>
    {/if}

    <div class="document-viewer__canvas-container">
      <canvas bind:this={canvasEl} data-testid="pdf-canvas"></canvas>
    </div>

    <div class="document-viewer__controls" data-testid="pdf-controls">
      <button
        type="button"
        class="document-viewer__btn"
        data-testid="pdf-prev"
        disabled={!canGoPrev}
        onclick={prevPage}
        aria-label="Previous page"
      >
        &#8249;
      </button>

      <span class="document-viewer__page-info" data-testid="pdf-page-info">
        {currentPage} / {totalPages}
      </span>

      <button
        type="button"
        class="document-viewer__btn"
        data-testid="pdf-next"
        disabled={!canGoNext}
        onclick={nextPage}
        aria-label="Next page"
      >
        &#8250;
      </button>

      <span class="document-viewer__separator"></span>

      <button
        type="button"
        class="document-viewer__btn"
        data-testid="pdf-zoom-out"
        disabled={!canZoomOut}
        onclick={zoomOut}
        aria-label="Zoom out"
      >
        &minus;
      </button>

      <span class="document-viewer__zoom-info" data-testid="pdf-zoom-info">
        {Math.round(zoom * 100)}%
      </span>

      <button
        type="button"
        class="document-viewer__btn"
        data-testid="pdf-zoom-in"
        disabled={!canZoomIn}
        onclick={zoomIn}
        aria-label="Zoom in"
      >
        +
      </button>
    </div>
  {/if}
</div>

<style>
  .document-viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
    background-color: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  .document-viewer__image-container {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: auto;
    padding: var(--space-4);
  }

  .document-viewer__image-stage {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    max-width: 100%;
    max-height: 100%;
  }

  .document-viewer__image {
    display: block;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  .document-viewer__overlay {
    position: absolute;
    inset: 0;
    cursor: crosshair;
  }

  .document-viewer__canvas-container {
    flex: 1;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    overflow: auto;
    padding: var(--space-4);
  }

  .document-viewer__loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-6);
    color: var(--color-text-secondary);
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
  }

  .document-viewer__spinner {
    width: 16px;
    height: 16px;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: var(--radius-full);
    animation: spin 0.6s linear infinite;
  }

  .document-viewer__error {
    padding: var(--space-4);
    color: var(--color-danger);
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
    text-align: center;
  }

  .document-viewer__controls {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-4);
    background-color: var(--color-surface);
    border-top: 1px solid var(--color-border);
  }

  .document-viewer__btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background-color: transparent;
    color: var(--color-text-primary);
    cursor: pointer;
    font-size: var(--font-size-lg);
    line-height: 1;
    transition:
      background-color 0.15s ease,
      border-color 0.15s ease;
  }

  .document-viewer__btn:hover:not(:disabled) {
    background-color: var(--color-surface-raised);
    border-color: var(--color-text-muted);
  }

  .document-viewer__btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .document-viewer__page-info,
  .document-viewer__zoom-info {
    font-family: var(--font-mono);
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    min-width: 60px;
    text-align: center;
  }

  .document-viewer__separator {
    width: 1px;
    height: 20px;
    background-color: var(--color-border);
    margin: 0 var(--space-2);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
