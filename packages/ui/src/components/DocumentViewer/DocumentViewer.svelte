<script lang="ts">
  import AnnotationToolbar from '../AnnotationToolbar/AnnotationToolbar.svelte'
  import AudioPlayer from '../AudioPlayer/AudioPlayer.svelte'
  import type { DocumentViewerProps } from './DocumentViewer.types'
  import type { AnnotationTool, EditTool, ViewerAnnotation } from './DocumentViewer.types'

  let {
    path: _path,
    type,
    assetUrl,
    annotations = [],
    selectedAnnotationId = null,
    annotationTool = 'select',
    annotationColor = 'var(--color-accent)',
    editTool = 'none',
    canUndo = false,
    onAnnotationsChange = () => {},
    onSelectedAnnotationIdChange = () => {},
    onAnnotationToolChange = () => {},
    onAnnotationColorChange = () => {},
    onEditSelect = () => {},
    onEditToolChange = () => {},
    onRotateLeft = () => {},
    onRotateRight = () => {},
    onUndo = () => {},
    onDimensionsChange = () => {},
  }: DocumentViewerProps = $props()

  const presetColors = [
    { value: 'var(--color-accent)', label: 'Accent' },
    { value: 'var(--color-success)', label: 'Success' },
    { value: 'var(--color-warning)', label: 'Warning' },
    { value: 'var(--color-danger)', label: 'Danger' },
  ]
  const MIN_DRAW_PX = 6
  const UNDERLINE_STROKE_PX = 2
  const UNDERLINE_HITBOX_NORMALIZED = 0.02
  const MIN_ZOOM = 0.5
  const MAX_ZOOM = 4.0
  const ZOOM_STEP = 0.25

  // PDF state
  let currentPage = $state(1)
  let totalPages = $state(0)
  let pdfZoom = $state(1.0)
  let loading = $state(false)
  let error = $state<string | null>(null)

  let canvasEl: HTMLCanvasElement | undefined = $state()
  let imgEl: HTMLImageElement | undefined = $state()
  let containerEl: HTMLElement | undefined = $state()
  let pdfDoc: any = null

  // Image geometry — natural (intrinsic) dimensions of the source file
  let naturalW = $state(0)
  let naturalH = $state(0)

  // Container inner dimensions (content area, padding excluded)
  let containerW = $state(0)
  let containerH = $state(0)

  // Image zoom level (1.0 = fit-to-container)
  let imageZoom = $state(1.0)

  let draft = $state<{
    startX: number
    startY: number
    currentX: number
    currentY: number
    kind: Exclude<AnnotationTool, 'select'>
  } | null>(null)

  // Edit draft: temporary rectangle drawn while crop/erase is active
  let editDraft = $state<{
    startX: number
    startY: number
    currentX: number
    currentY: number
  } | null>(null)

  // ── Derived geometry ──────────────────────────────────────────────
  // fitScale: the scale that makes the image fit inside the container
  const fitScale = $derived(
    naturalW > 0 && naturalH > 0 && containerW > 0 && containerH > 0
      ? Math.min(containerW / naturalW, containerH / naturalH)
      : 1
  )

  // Display dimensions: what the image (and SVG overlay) measure on screen
  const displayW = $derived(Math.round(naturalW * fitScale * imageZoom))
  const displayH = $derived(Math.round(naturalH * fitScale * imageZoom))

  const hasRenderableBounds = $derived(naturalW > 0 && naturalH > 0)

  const canZoomIn = $derived(imageZoom < MAX_ZOOM)
  const canZoomOut = $derived(imageZoom > MIN_ZOOM)

  const canGoPrev = $derived(currentPage > 1)
  const canGoNext = $derived(currentPage < totalPages)
  const canPdfZoomIn = $derived(pdfZoom < 3.0)
  const canPdfZoomOut = $derived(pdfZoom > 0.5)

  const overlayCursor = $derived(
    editTool !== 'none'
      ? 'crosshair'
      : annotationTool === 'select'
        ? 'default'
        : 'crosshair'
  )

  // ── Helpers ────────────────────────────────────────────────────────
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

  /** Convert normalized [0,1] → natural-image pixels (SVG viewBox space) */
  function px(value: number, axis: 'x' | 'y') {
    const dimension = axis === 'x' ? naturalW : naturalH
    return String(Math.round(value * dimension))
  }

  /** Convert a viewport PointerEvent to normalized [0,1] coordinates.
   *  Uses getBoundingClientRect which accounts for CSS transforms, so this
   *  works correctly at any zoom level. */
  function getNormalizedPoint(event: PointerEvent) {
    const target = event.currentTarget as SVGSVGElement
    const rect = target.getBoundingClientRect()
    if (rect.width === 0 || rect.height === 0) return null

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
    // Minimum size in display pixels, converted to normalized coords via display dimensions
    const minNormW = MIN_DRAW_PX / Math.max(displayW, 1)
    const minNormH = MIN_DRAW_PX / Math.max(displayH, 1)
    if (kind === 'underline') {
      return box.width >= minNormW
    }
    return box.width >= minNormW && box.height >= minNormH
  }

  // ── Edit draft helpers ───────────────────────────────────────────────
  function toEditBox(d: NonNullable<typeof editDraft>) {
    const x = Math.min(d.startX, d.currentX)
    const y = Math.min(d.startY, d.currentY)
    return {
      x: round(x),
      y: round(y),
      width: round(Math.abs(d.currentX - d.startX)),
      height: round(Math.abs(d.currentY - d.startY)),
    }
  }

  // ── Measurement ────────────────────────────────────────────────────
  function measureImage() {
    if (!imgEl) return
    // Skip measurement if the image hasn't loaded yet (e.g. after {#key} re-creation).
    // This preserves the previous dimensions until onload fires, avoiding a flash
    // where the overlay disappears because naturalWidth/naturalHeight are 0.
    if (!imgEl.complete || imgEl.naturalWidth === 0) return
    naturalW = imgEl.naturalWidth
    naturalH = imgEl.naturalHeight
    onDimensionsChange({ width: naturalW, height: naturalH })
  }

  function measureContainer() {
    if (!containerEl) return
    const style = getComputedStyle(containerEl)
    const padX = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight)
    const padY = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom)
    containerW = containerEl.clientWidth - padX
    containerH = containerEl.clientHeight - padY
  }

  // ── Handlers ────────────────────────────────────────────────────────
  function handleToolbarToolChange(tool: AnnotationTool) {
    onAnnotationToolChange(tool)
    // Reset edit tool when switching to annotation mode
    if (tool !== 'select') {
      onEditToolChange('none')
    }
    if (tool !== annotationTool) {
      draft = null
    }
  }

  function handleToolbarColorChange(color: string) {
    onAnnotationColorChange(color)
    if (!selectedAnnotationId) return
    onAnnotationsChange(
      annotations.map((a) =>
        a.id === selectedAnnotationId ? { ...a, color, updatedAt: Date.now() } : a
      )
    )
  }

  function handleDeleteSelected() {
    if (!selectedAnnotationId) return
    onAnnotationsChange(annotations.filter((a) => a.id !== selectedAnnotationId))
    onSelectedAnnotationIdChange(null)
  }

  function handleOverlayPointerDown(event: PointerEvent) {
    if (!hasRenderableBounds || event.button !== 0) return
    const point = getNormalizedPoint(event)
    if (!point) return

    // Edit mode: start drawing an edit selection rectangle
    if (editTool !== 'none') {
      editDraft = {
        startX: point.x,
        startY: point.y,
        currentX: point.x,
        currentY: point.y,
      }
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
    if (editDraft) {
      const point = getNormalizedPoint(event)
      if (!point) return
      editDraft = {
        ...editDraft,
        currentX: point.x,
        currentY: point.y,
      }
      return
    }

    if (!draft) return
    const point = getNormalizedPoint(event)
    if (!point) return
    draft = {
      ...draft,
      currentX: point.x,
      currentY: draft.kind === 'underline' ? draft.startY : point.y,
    }
  }

  function finishDraft() {
    // Handle edit draft completion
    if (editDraft) {
      const box = toEditBox(editDraft)
      editDraft = null
      const minSize = MIN_DRAW_PX / Math.max(displayW, 1)
      const minHeight = MIN_DRAW_PX / Math.max(displayH, 1)
      if (box.width < minSize || box.height < minHeight) return
      onEditSelect({ x: box.x, y: box.y, width: box.width, height: box.height })
      return
    }

    if (!draft) return
    const kind = draft.kind

    if (kind === 'underline') {
      const x = round(Math.min(draft.startX, draft.currentX))
      const width = round(Math.abs(draft.currentX - draft.startX))
      const y = round(clamp01(draft.startY - UNDERLINE_HITBOX_NORMALIZED / 2))
      const minWidth = MIN_DRAW_PX / Math.max(displayW, 1)
      const clampedWidth = round(Math.min(width, 1 - x))

      draft = null
      if (clampedWidth < minWidth) return

      onAnnotationsChange([
        ...annotations,
        createLocalAnnotation('underline', x, y, clampedWidth, UNDERLINE_HITBOX_NORMALIZED),
      ])
      onSelectedAnnotationIdChange(null)
      return
    }

    const box = toDraftBox(draft)
    draft = null
    if (!meetsMinimumSize(box, kind)) return

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
    if (event.key !== 'Enter' && event.key !== ' ') return
    event.preventDefault()
    event.stopPropagation()
    handleShapeClick(annotationId)
  }

  // ── Zoom (image) ──────────────────────────────────────────────────
  function imageZoomIn() {
    if (canZoomIn) imageZoom = Math.min(MAX_ZOOM, imageZoom + ZOOM_STEP)
  }
  function imageZoomOut() {
    if (canZoomOut) imageZoom = Math.max(MIN_ZOOM, imageZoom - ZOOM_STEP)
  }

  // ── PDF ─────────────────────────────────────────────────────────────
  function resetViewerState() {
    loading = false
    error = null
    currentPage = 1
    totalPages = 0
    pdfZoom = 1.0
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
      const viewport = page.getViewport({ scale: pdfZoom })
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
  function pdfZoomIn() {
    if (canPdfZoomIn) {
      pdfZoom = Math.min(3.0, pdfZoom + 0.25)
      renderPage()
    }
  }
  function pdfZoomOut() {
    if (canPdfZoomOut) {
      pdfZoom = Math.max(0.5, pdfZoom - 0.25)
      renderPage()
    }
  }

  // ── Effects ────────────────────────────────────────────────────────
  // Reset image zoom when the asset URL changes (crop, rotate, erase)
  $effect(() => {
    // Depend on assetUrl so zoom resets on every edit
    const _url = assetUrl
    if (type === 'image') {
      imageZoom = 1.0
    }
  })

  $effect(() => {
    if (type !== 'image') {
      naturalW = 0
      naturalH = 0
      return
    }
    if (!imgEl) return // Image element not mounted yet; don't zero dimensions
    measureImage()
    const obs = new ResizeObserver(() => measureImage())
    obs.observe(imgEl)
    return () => obs.disconnect()
  })

  $effect(() => {
    if (type !== 'image' || !containerEl) return
    measureContainer()
    const obs = new ResizeObserver(() => measureContainer())
    obs.observe(containerEl)
    return () => obs.disconnect()
  })

  $effect(() => {
    if (type !== 'pdf') {
      resetViewerState()
      return
    }
    activatePdfMode()
    void loadPdf()
  })

  // ── Draft rendering ─────────────────────────────────────────────────
  const draftBox = $derived(draft ? toDraftBox(draft) : null)
  const draftUnderline = $derived(
    draft?.kind === 'underline'
      ? {
          x1: Math.min(draft.startX, draft.currentX),
          x2: Math.max(draft.startX, draft.currentX),
          y: draft.startY,
        }
      : null
  )
</script>

<div class="document-viewer">
  {#if type === 'image'}
    <!-- svelte-ignore a11y_no_static_element_interactions — overlay needs pointer events -->
    <div class="document-viewer__image-container" bind:this={containerEl}>
      <div class="document-viewer__toolbar-anchor">
        <AnnotationToolbar
          tool={annotationTool}
          editTool={editTool}
          color={annotationColor}
          hasSelection={selectedAnnotationId !== null}
          canUndo={canUndo}
          colors={presetColors}
          onToolChange={handleToolbarToolChange}
          onEditToolChange={onEditToolChange}
          onColorChange={handleToolbarColorChange}
          onDeleteSelected={handleDeleteSelected}
          onRotateLeft={onRotateLeft}
          onRotateRight={onRotateRight}
          onUndo={onUndo}
        />
      </div>

      <div class="document-viewer__image-stage">
        {#key assetUrl}
        <img
          bind:this={imgEl}
          src={assetUrl}
          alt="Document"
          class="document-viewer__image"
          style={`width:${displayW}px;height:${displayH}px;`}
          onload={measureImage}
        />
      {/key}

        {#if hasRenderableBounds}
          <svg
            class="document-viewer__overlay"
            data-testid="annotation-overlay"
            role="application"
            aria-label="Image annotation overlay"
            width={displayW}
            height={displayH}
            viewBox={`0 0 ${naturalW} ${naturalH}`}
            style={`--overlay-cursor: ${overlayCursor}`}
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
                  vector-effect="non-scaling-stroke"
                  style={editTool !== 'none' ? 'pointer-events:none' : ''}
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
                <g style={editTool !== 'none' ? 'pointer-events:none' : ''}>
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
                    stroke-width={UNDERLINE_STROKE_PX}
                    stroke-linecap="round"
                    vector-effect="non-scaling-stroke"
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

            {#if draftBox && draft?.kind === 'rectangle'}
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
                vector-effect="non-scaling-stroke"
              />
            {/if}

            {#if draftUnderline}
              <line
                x1={px(draftUnderline.x1, 'x')}
                y1={px(draftUnderline.y, 'y')}
                x2={px(draftUnderline.x2, 'x')}
                y2={px(draftUnderline.y, 'y')}
                stroke={annotationColor}
                stroke-width={UNDERLINE_STROKE_PX}
                stroke-dasharray="6 4"
                stroke-linecap="round"
                vector-effect="non-scaling-stroke"
              />
            {/if}

            {#if editDraft}
              {@const ebox = toEditBox(editDraft)}
              {@const isCrop = editTool === 'crop'}
              {@const editColor = isCrop ? 'var(--color-success, #16a34a)' : 'var(--color-danger, #dc2626)'}
              {@const editLabel = isCrop ? 'Crop region' : 'Erase region'}
              <!-- Dim area outside selection -->
              {#if ebox.width > 0.001 && ebox.height > 0.001}
                <rect
                  x={0}
                  y={0}
                  width={naturalW}
                  height={px(ebox.y, 'y')}
                  fill="rgba(0,0,0,0.35)"
                />
                <rect
                  x={0}
                  y={px(ebox.y, 'y')}
                  width={px(ebox.x, 'x')}
                  height={px(ebox.height, 'y')}
                  fill="rgba(0,0,0,0.35)"
                />
                <rect
                  x={px(ebox.x + ebox.width, 'x')}
                  y={px(ebox.y, 'y')}
                  width={naturalW - px(ebox.x + ebox.width, 'x')}
                  height={px(ebox.height, 'y')}
                  fill="rgba(0,0,0,0.35)"
                />
                <rect
                  x={0}
                  y={px(ebox.y + ebox.height, 'y')}
                  width={naturalW}
                  height={naturalH - px(ebox.y + ebox.height, 'y')}
                  fill="rgba(0,0,0,0.35)"
                />
              {/if}
              <rect
                data-testid="edit-selection-rect"
                x={px(ebox.x, 'x')}
                y={px(ebox.y, 'y')}
                width={px(ebox.width, 'x')}
                height={px(ebox.height, 'y')}
                fill={isCrop ? 'rgba(22,163,74,0.08)' : 'rgba(220,38,38,0.08)'}
                stroke={editColor}
                stroke-width="2"
                stroke-dasharray="8 4"
                vector-effect="non-scaling-stroke"
                role="img"
                aria-label={editLabel}
              />
            {/if}
          </svg>
        {/if}
      </div>
    </div>

    <div class="document-viewer__controls" data-testid="image-controls">
      <button
        type="button"
        class="document-viewer__btn"
        data-testid="image-zoom-out"
        disabled={!canZoomOut}
        onclick={imageZoomOut}
        aria-label="Zoom out"
      >
        &minus;
      </button>
      <span class="document-viewer__zoom-info" data-testid="image-zoom-info">
        {Math.round(imageZoom * 100)}%
      </span>
      <button
        type="button"
        class="document-viewer__btn"
        data-testid="image-zoom-in"
        disabled={!canZoomIn}
        onclick={imageZoomIn}
        aria-label="Zoom in"
      >
        +
      </button>
    </div>
  {:else if type === 'audio'}
    <AudioPlayer src={assetUrl} />
  {:else}
    {#if loading}
      <div class="document-viewer__loading" data-testid="pdf-loading">
        <span class="document-viewer__spinner" aria-hidden="true"></span>
        <span>Loading PDF...</span>
      </div>
    {/if}

    {#if error}
      <div class="document-viewer__error" data-testid="pdf-error" role="alert">{error}</div>
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
        aria-label="Previous page">&#8249;</button
      >
      <span class="document-viewer__page-info" data-testid="pdf-page-info"
        >{currentPage} / {totalPages}</span
      >
      <button
        type="button"
        class="document-viewer__btn"
        data-testid="pdf-next"
        disabled={!canGoNext}
        onclick={nextPage}
        aria-label="Next page">&#8250;</button
      >
      <span class="document-viewer__separator"></span>
      <button
        type="button"
        class="document-viewer__btn"
        data-testid="pdf-zoom-out"
        disabled={!canPdfZoomOut}
        onclick={pdfZoomOut}
        aria-label="Zoom out">&minus;</button
      >
      <span class="document-viewer__zoom-info" data-testid="pdf-zoom-info"
        >{Math.round(pdfZoom * 100)}%</span
      >
      <button
        type="button"
        class="document-viewer__btn"
        data-testid="pdf-zoom-in"
        disabled={!canPdfZoomIn}
        onclick={pdfZoomIn}
        aria-label="Zoom in">+</button
      >
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
    overflow: auto;
    padding: var(--space-4);
    position: relative;
  }

  .document-viewer__toolbar-anchor {
    position: sticky;
    top: 0;
    z-index: 3;
    display: flex;
    justify-content: flex-end;
    align-items: flex-start;
    gap: var(--space-2);
    pointer-events: none;
    padding: 0 var(--space-2) var(--space-2) 0;
  }

  .document-viewer__image-stage {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin: auto;
  }

  .document-viewer__image {
    display: block;
    flex-shrink: 0;
  }

  .document-viewer__overlay {
    position: absolute;
    inset: 0;
    cursor: var(--overlay-cursor, crosshair);
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
