<script lang="ts">
  import { onMount } from 'svelte'
  import type { DocumentViewerProps } from './DocumentViewer.types'

  let { path: _path, type, assetUrl }: DocumentViewerProps = $props()

  // PDF state
  let currentPage = $state(1)
  let totalPages = $state(0)
  let zoom = $state(1.0)
  let loading = $state(type === 'pdf')
  let error = $state<string | null>(null)

  let canvasEl: HTMLCanvasElement | undefined = $state()
  let pdfDoc: any = null

  const canGoPrev = $derived(currentPage > 1)
  const canGoNext = $derived(currentPage < totalPages)
  const canZoomIn = $derived(zoom < 3.0)
  const canZoomOut = $derived(zoom > 0.5)

  async function loadPdf() {
    try {
      loading = true
      error = null
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

  onMount(() => {
    if (type === 'pdf') {
      loadPdf()
    }
  })
</script>

<div class="document-viewer">
  {#if type === 'image'}
    <div class="document-viewer__image-container">
      <img src={assetUrl} alt="Document" class="document-viewer__image" />
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

  .document-viewer__image {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
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
