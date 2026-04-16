<script lang="ts">
  import { getStore } from '$lib/db'
  import { getAssetUrl } from '$lib/file-import'
  import { OcrStore, extractText } from '$lib/ocr'
  import { TranscriptionStore, transcribeAudio } from '$lib/transcription'
  import {
    NlpStore,
    indexFts,
    enrichItem,
    embedItem,
    extractEntities,
    extractTriples,
    similarItems as fetchSimilarItems,
  } from '$lib/nlp'
  import {
    DocumentViewer,
    MetadataEditor,
    NoteEditor,
    Button,
    Card,
    EntityViewer,
  } from '@entropia/ui'
  import { onMount, onDestroy } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { invoke } from '@tauri-apps/api/core'
  import type {
    Item,
    Asset,
    Note,
    Annotation as StoreAnnotation,
    AnnotationKind as StoreAnnotationKind,
  } from '@entropia/store'
  import type {
    Entity,
    ViewerAnnotation,
    AnnotationKind as ViewerAnnotationKind,
  } from '@entropia/ui'
  import { TranscriptionRepo } from '@entropia/store'

  let { itemId, collectionId }: { itemId: string; collectionId: string } = $props()

  let item = $state<Item | null>(null)
  let assets = $state<Asset[]>([])
  let notes = $state<Note[]>([])
  let loading = $state(true)
  let error = $state<string | null>(null)
  let selectedAssetIndex = $state(0)
  let savingMetadata = $state(false)
  let annotations = $state<ViewerAnnotation[]>([])
  let selectedAnnotationId = $state<string | null>(null)
  let annotationTool = $state<'select' | 'rectangle' | 'underline'>('select')
  let annotationColor = $state('var(--color-accent)')
  let annotationSaveError = $state<string | null>(null)
  let annotationSaveTimer: ReturnType<typeof setTimeout> | null = null
  let pendingAnnotationSave: {
    assetId: string
    annotations: ViewerAnnotation[]
  } | null = null

  // OCR state — plain TS class, updated via Tauri events
  const ocrStore = new OcrStore({
    onComplete: (assetId) => {
      // After OCR extraction completes, auto-trigger full enrichment for this item
      void enrichItem(itemId).catch(() => {
        // Non-fatal: enrichment failure doesn't block the UI
      })
      void assetId // suppress unused warning (assetId belongs to an asset of itemId)
    },
  })
  // Reactive tick counter: incremented on every OCR event to force Svelte re-evaluation
  let ocrTick = $state(0)
  // Edited text per asset — tracks user corrections to OCR output
  let ocrEditedText = $state(new Map<string, string>())
  // Debounce timers per asset for persisting edits to DB
  let ocrPersistTimers = $state(new Map<string, ReturnType<typeof setTimeout>>())

  // Transcription state — mirrors OcrStore pattern for audio assets
  const transcriptionStore = new TranscriptionStore({
    onComplete: (assetId) => {
      // After transcription completes, auto-trigger full enrichment
      void enrichItem(itemId).catch(() => {})
      void assetId
    },
  })
  let transcriptionTick = $state(0)
  let transEditedText = $state(new Map<string, string>())
  let transPersistTimers = $state(new Map<string, ReturnType<typeof setTimeout>>())

  /** Schedule a debounced persist of edited text to the DB (500ms after last keystroke). */
  function schedulePersist(assetId: string, text: string) {
    // Cancel any pending timer for this asset
    const existing = ocrPersistTimers.get(assetId)
    if (existing) clearTimeout(existing)

    // Schedule new persist
    const timer = setTimeout(async () => {
      try {
        await invoke('update_extraction_text_cmd', { assetId, textContent: text })
        // Re-enrich so search reflects the corrected text
        await enrichItem(itemId).catch(() => {})
      } catch (e) {
        console.error('[ItemView] Failed to persist OCR correction:', e)
      }
      ocrPersistTimers.delete(assetId)
    }, 500)

    ocrPersistTimers.set(assetId, timer)
  }

  /** Schedule a debounced persist of edited transcription text to the DB. */
  function scheduleTranscriptionPersist(assetId: string, text: string) {
    const existing = transPersistTimers.get(assetId)
    if (existing) clearTimeout(existing)

    const timer = setTimeout(async () => {
      try {
        await invoke('update_transcription_text_cmd', { assetId, textContent: text })
        await enrichItem(itemId).catch(() => {})
      } catch (e) {
        console.error('[ItemView] Failed to persist transcription correction:', e)
      }
      transPersistTimers.delete(assetId)
    }, 500)

    transPersistTimers.set(assetId, timer)
  }

  // NLP state — mirrors OcrStore pattern
  const nlpStore = new NlpStore()
  let nlpTick = $state(0)
  let entities = $state<Entity[]>([])
  let similarItemIds = $state<string[]>([])
  let triples = $state<Array<{ subject: string; predicate: string; object: string }>>([])
  let analysisOpen = $state(false)

  let metadataValue = $derived<Record<string, string>>(
    item?.metadata ? parseMetadataRecord(item.metadata) : {}
  )

  let selectedAsset = $derived(assets[selectedAssetIndex] ?? null)

  let viewerSrc = $derived(selectedAsset ? getAssetUrl(selectedAsset.path) : '')

  let viewerType = $derived<'image' | 'pdf' | 'audio'>(
    selectedAsset?.type === 'pdf' ? 'pdf' : selectedAsset?.type === 'audio' ? 'audio' : 'image'
  )

  function clampNormalized(value: number) {
    return Math.max(0, Math.min(1, value))
  }

  function normalizeAnnotationsForAsset(
    asset: Asset,
    nextAnnotations: ViewerAnnotation[]
  ): ViewerAnnotation[] {
    return nextAnnotations.map((annotation) => {
      const now = Date.now()
      return {
        ...annotation,
        id: annotation.id || crypto.randomUUID(),
        assetId: asset.id,
        page: 1,
        color: annotation.color,
        x: clampNormalized(annotation.x),
        y: clampNormalized(annotation.y),
        width: clampNormalized(annotation.width),
        height: clampNormalized(annotation.height),
        createdAt: annotation.createdAt || now,
        updatedAt: now,
      }
    })
  }

  async function persistAnnotations(assetId: string, nextAnnotations: ViewerAnnotation[]) {
    try {
      const inputs = nextAnnotations.map((a) => ({
        kind: a.kind as StoreAnnotationKind,
        color: a.color,
        x: a.x,
        y: a.y,
        width: a.width,
        height: a.height,
      }))
      await getStore().annotations.replaceForAssetPage(assetId, 1, inputs)
      annotationSaveError = null
    } catch {
      annotationSaveError = 'Failed to save annotations. Changes remain local until retry.'
    }
  }

  function clearAnnotationSaveTimer() {
    if (annotationSaveTimer) {
      clearTimeout(annotationSaveTimer)
      annotationSaveTimer = null
    }
  }

  async function flushPendingAnnotationSave() {
    clearAnnotationSaveTimer()

    if (!pendingAnnotationSave) {
      return
    }

    const saveJob = pendingAnnotationSave
    pendingAnnotationSave = null
    await persistAnnotations(saveJob.assetId, saveJob.annotations)
  }

  function scheduleAnnotationPersist(assetId: string, nextAnnotations: ViewerAnnotation[]) {
    clearAnnotationSaveTimer()
    pendingAnnotationSave = {
      assetId,
      annotations: nextAnnotations,
    }

    annotationSaveTimer = setTimeout(async () => {
      const saveJob = pendingAnnotationSave
      pendingAnnotationSave = null
      annotationSaveTimer = null

      if (!saveJob) {
        return
      }

      await persistAnnotations(saveJob.assetId, saveJob.annotations)
    }, 500)
  }

  function handleAnnotationsChange(nextAnnotations: ViewerAnnotation[]) {
    if (!selectedAsset || selectedAsset.type !== 'image') {
      return
    }

    annotations = normalizeAnnotationsForAsset(selectedAsset, nextAnnotations)
    annotationSaveError = null
    scheduleAnnotationPersist(selectedAsset.id, annotations)
  }

  function handleSelectedAnnotationIdChange(annotationId: string | null) {
    selectedAnnotationId = annotationId
  }

  function handleAnnotationToolChange(tool: 'select' | 'rectangle' | 'underline') {
    annotationTool = tool
  }

  function handleAnnotationColorChange(color: string) {
    annotationColor = color
  }

  function parseMetadataRecord(json: string): Record<string, string> {
    try {
      const obj = JSON.parse(json)
      const record: Record<string, string> = {}
      for (const [key, value] of Object.entries(obj)) {
        record[key] = String(value)
      }
      return record
    } catch {
      return {}
    }
  }

  let metadataSaveTimer: ReturnType<typeof setTimeout> | null = null

  async function handleExtractText(asset: Asset) {
    ocrStore._updateState(asset.id, { status: 'pending', progress: 0 })
    ocrTick++
    try {
      await extractText(asset.id, asset.path, asset.type)
    } catch (e) {
      ocrStore._updateState(asset.id, {
        status: 'error',
        error: e instanceof Error ? e.message : 'Extraction failed',
      })
      ocrTick++
    }
  }

  async function handleTranscribeAudio(asset: Asset) {
    transcriptionStore._updateState(asset.id, { status: 'pending', progress: 0 })
    transcriptionTick++
    try {
      await transcribeAudio(asset.id, asset.path)
    } catch (e) {
      transcriptionStore._updateState(asset.id, {
        status: 'error',
        error: e instanceof Error ? e.message : 'Transcription failed',
      })
      transcriptionTick++
    }
  }

  /** Load existing extraction text for all assets on mount (persistence between sessions). */
  async function loadExistingExtractions() {
    const store = getStore()
    for (const asset of assets) {
      const extraction = await store.extractions.findByAsset(asset.id)
      if (extraction) {
        ocrStore._updateState(asset.id, {
          status: 'done',
          progress: 100,
          textLength: extraction.textContent.length,
          method: extraction.method,
          textContent: extraction.textContent,
        })
        ocrTick++
      }
    }
  }

  /** Load existing transcriptions for all audio assets on mount. */
  async function loadExistingTranscriptions() {
    const store = getStore()
    for (const asset of assets) {
      if (asset.type !== 'audio') continue
      const transcription = await store.transcriptions.findByAsset(asset.id)
      if (transcription) {
        transcriptionStore._updateState(asset.id, {
          status: 'done',
          progress: 100,
          text: transcription.textContent,
          language: transcription.language ?? undefined,
          durationMs: transcription.durationMs ?? undefined,
          segmentsCount: transcription.segments
            ? TranscriptionRepo.parseSegments(transcription.segments).length
            : 0,
        })
        transcriptionTick++
      }
    }
  }

  function getOcrState(assetId: string) {
    // Depend on ocrTick to trigger Svelte reactivity when events arrive
    void ocrTick
    return ocrStore.getState(assetId)
  }

  function getTranscriptionState(assetId: string) {
    void transcriptionTick
    return transcriptionStore.getState(assetId)
  }

  function getNlpState() {
    void nlpTick
    return nlpStore.getState(itemId)
  }

  async function handleIndexFts() {
    nlpStore._setJobStatus(itemId, 'fts', 'pending')
    nlpTick++
    try {
      await indexFts(itemId)
    } catch (e) {
      nlpStore._setJobStatus(itemId, 'fts', 'error', e instanceof Error ? e.message : 'Failed')
      nlpTick++
    }
  }

  async function handleEmbedItem() {
    nlpStore._setJobStatus(itemId, 'embed', 'pending')
    nlpTick++
    try {
      await embedItem(itemId)
    } catch (e) {
      nlpStore._setJobStatus(itemId, 'embed', 'error', e instanceof Error ? e.message : 'Failed')
      nlpTick++
    }
  }

  async function handleExtractEntities() {
    nlpStore._setJobStatus(itemId, 'ner', 'pending')
    nlpTick++
    try {
      await extractEntities(itemId)
    } catch (e) {
      nlpStore._setJobStatus(itemId, 'ner', 'error', e instanceof Error ? e.message : 'Failed')
      nlpTick++
    }
  }

  async function handleExtractTriples() {
    nlpStore._setJobStatus(itemId, 'triples', 'pending')
    nlpTick++
    try {
      await extractTriples(itemId)
    } catch (e) {
      nlpStore._setJobStatus(itemId, 'triples', 'error', e instanceof Error ? e.message : 'Failed')
      nlpTick++
    }
  }

  async function loadEntities() {
    try {
      const store = getStore()
      entities = (await store.entities.findByItemId(itemId)) as Entity[]
    } catch {
      // Non-fatal: entities panel shows empty state
    }
  }

  async function loadSimilarItems() {
    try {
      const results = await fetchSimilarItems(itemId, 5)
      similarItemIds = results.map((r: { itemId: string }) => r.itemId)
    } catch {
      similarItemIds = []
    }
  }

  async function loadTriples() {
    try {
      const store = getStore()
      triples = await store.triples.findByItemId(itemId)
    } catch {
      triples = []
    }
  }

  function handleMetadataChange(metadata: Record<string, string>) {
    if (metadataSaveTimer) clearTimeout(metadataSaveTimer)
    metadataSaveTimer = setTimeout(async () => {
      if (!item) return
      try {
        savingMetadata = true
        const store = getStore()
        await store.items.update(item.id, { metadata: JSON.stringify(metadata) })
      } catch (e) {
        error = e instanceof Error ? e.message : 'Failed to save metadata'
      } finally {
        savingMetadata = false
      }
    }, 1000)
  }

  async function handleSaveNote(content: string) {
    try {
      error = null
      const store = getStore()
      await store.notes.create({ itemId, content })
      notes = await store.notes.findByItem(itemId)
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to save note'
    }
  }

  async function handleDeleteNote(noteId: string) {
    try {
      error = null
      const store = getStore()
      await store.notes.delete(noteId)
      notes = await store.notes.findByItem(itemId)
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to delete note'
    }
  }

  // Note editing state
  let editingNoteId = $state<string | null>(null)
  let editContent = $state('')

  function handleEditNote(note: Note) {
    editingNoteId = note.id
    editContent = note.content
  }

  async function handleSaveEdit(noteId: string) {
    if (!editContent.trim()) return
    try {
      error = null
      const store = getStore()
      await store.notes.update(noteId, editContent)
      notes = await store.notes.findByItem(itemId)
      editingNoteId = null
      editContent = ''
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to update note'
    }
  }

  function handleCancelEdit() {
    editingNoteId = null
    editContent = ''
  }

  async function loadData() {
    try {
      loading = true
      error = null
      const store = getStore()
      const [loadedItem, loadedAssets, loadedNotes] = await Promise.all([
        store.items.findById(itemId),
        store.assets.findByItem(itemId),
        store.notes.findByItem(itemId),
      ])
      item = loadedItem
      assets = loadedAssets
      notes = loadedNotes
      // Load existing extraction text for persistence between sessions
      await loadExistingExtractions()
      await loadExistingTranscriptions()
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load item'
    } finally {
      loading = false
    }
  }

  $effect(() => {
    const asset = selectedAsset
    const currentAssetId = asset?.id ?? null

    selectedAnnotationId = null
    annotationTool = 'select'

    if (pendingAnnotationSave && pendingAnnotationSave.assetId !== currentAssetId) {
      void flushPendingAnnotationSave()
    }

    if (!asset || asset.type !== 'image') {
      annotations = []
      annotationSaveError = null
      return
    }

    let cancelled = false

    void (async () => {
      try {
        annotationSaveError = null
        const loadedAnnotations = await getStore().annotations.findByAsset(asset.id, 1)
        if (!cancelled && selectedAsset?.id === asset.id) {
          annotations = loadedAnnotations.map((a) => ({
            ...a,
            kind: a.kind as ViewerAnnotationKind,
          }))
        }
      } catch {
        if (!cancelled) {
          annotations = []
          annotationSaveError = 'Failed to load annotations for this asset.'
        }
      }
    })()

    return () => {
      cancelled = true
    }
  })

  onMount(() => {
    loadData()
    ocrStore
      .startListening((eventName, callback) =>
        listen(eventName, callback).then((unlisten) => {
          // Wrap unlisten to also trigger reactivity tick
          return () => {
            unlisten()
          }
        })
      )
      .then(() => {
        // Patch each event to also bump ocrTick for Svelte reactivity
        const origUpdate = ocrStore._updateState.bind(ocrStore)
        ocrStore._updateState = (assetId, partial) => {
          origUpdate(assetId, partial)
          ocrTick++
        }
      })

    nlpStore
      .startListening((eventName, callback) =>
        listen(eventName, callback).then((unlisten) => () => unlisten())
      )
      .then(() => {
        const origSet = nlpStore._setJobStatus.bind(nlpStore)
        nlpStore._setJobStatus = (id, job, status, err) => {
          origSet(id, job, status, err)
          nlpTick++
          // After NER completes, reload entities from DB
          if (job === 'ner' && status === 'done' && id === itemId) {
            loadEntities()
          }
          if (job === 'triples' && status === 'done' && id === itemId) {
            loadTriples()
          }
        }
      })

    transcriptionStore
      .startListening((eventName, callback) =>
        listen(eventName, callback).then((unlisten) => () => unlisten())
      )
      .then(() => {
        const origUpdate = transcriptionStore._updateState.bind(transcriptionStore)
        transcriptionStore._updateState = (assetId, partial) => {
          origUpdate(assetId, partial)
          transcriptionTick++
        }
      })

    return () => {
      if (metadataSaveTimer) clearTimeout(metadataSaveTimer)
    }
  })

  onDestroy(() => {
    ocrStore.stopListening()
    nlpStore.stopListening()
    transcriptionStore.stopListening()
    // Clear any pending debounce timers to avoid stale persist after unmount
    for (const timer of ocrPersistTimers.values()) {
      clearTimeout(timer)
    }
    ocrPersistTimers.clear()
    for (const timer of transPersistTimers.values()) {
      clearTimeout(timer)
    }
    transPersistTimers.clear()
    clearAnnotationSaveTimer()
  })
</script>

{#if loading}
  <p class="status">Loading...</p>
{:else if error && !item}
  <p class="error">{error}</p>
{:else if item}
  <div class="item-view">
    <div class="left-panel">
      {#if selectedAsset}
        <DocumentViewer
          path={selectedAsset.path}
          assetUrl={viewerSrc}
          type={viewerType}
          {annotations}
          {selectedAnnotationId}
          {annotationTool}
          {annotationColor}
          onAnnotationsChange={handleAnnotationsChange}
          onSelectedAnnotationIdChange={handleSelectedAnnotationIdChange}
          onAnnotationToolChange={handleAnnotationToolChange}
          onAnnotationColorChange={handleAnnotationColorChange}
        />

        {#if annotationSaveError}
          <p class="error">{annotationSaveError}</p>
        {/if}
      {:else}
        <div class="empty-viewer">
          <p>No assets attached to this item.</p>
        </div>
      {/if}

      {#if assets.length > 1}
        <div class="asset-list">
          {#each assets as asset, i (asset.id)}
            {@const ocr = getOcrState(asset.id)}
            <button
              class="asset-thumb"
              class:active={i === selectedAssetIndex}
              onclick={() => (selectedAssetIndex = i)}
            >
              {asset.path.split(/[/\\]/).pop() ?? 'Asset'}
              {#if ocr.status !== 'idle'}
                <span class="ocr-badge ocr-badge--{ocr.status}">{ocr.status}</span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="right-panel">
      <h2 class="item-title">{item.title}</h2>

      {#if error}
        <p class="error">{error}</p>
      {/if}

      <section class="section">
        <h3>
          Metadata {#if savingMetadata}<span class="saving">Saving...</span>{/if}
        </h3>
        <MetadataEditor value={metadataValue} onchange={handleMetadataChange} />
      </section>

      <section class="section">
        <h3>Add Note</h3>
        <NoteEditor onsave={handleSaveNote} />
      </section>

      <section class="section">
        <h3>Notes ({notes.length})</h3>
        {#if notes.length === 0}
          <p class="empty-text">No notes yet.</p>
        {:else}
          <div class="notes-list">
            {#each notes as note (note.id)}
              <Card>
                {#if editingNoteId === note.id}
                  <div class="note-edit">
                    <textarea
                      class="note-edit__textarea"
                      rows="3"
                      value={editContent}
                      oninput={(e) => (editContent = (e.target as HTMLTextAreaElement).value)}
                    ></textarea>
                    <div class="note-edit__actions">
                      <Button variant="ghost" size="sm" onclick={handleCancelEdit}>Cancel</Button>
                      <Button
                        variant="primary"
                        size="sm"
                        disabled={!editContent.trim() || editContent === note.content}
                        onclick={() => handleSaveEdit(note.id)}
                      >
                        Save
                      </Button>
                    </div>
                  </div>
                {:else}
                  <p class="note-content">{note.content}</p>
                  <p class="note-date">{new Date(note.createdAt).toLocaleString()}</p>
                  <div class="note-actions">
                    <Button variant="ghost" size="sm" onclick={() => handleEditNote(note)}>
                      Edit
                    </Button>
                    <Button variant="ghost" size="sm" onclick={() => handleDeleteNote(note.id)}>
                      Delete
                    </Button>
                  </div>
                {/if}
              </Card>
            {/each}
          </div>
        {/if}
      </section>

      {#if assets.some((a) => a.type !== 'audio')}
        <section class="section">
          <h3>Text Extraction</h3>
          {#if assets.filter((a) => a.type !== 'audio').length === 0}
            <p class="empty-text">No assets to extract text from.</p>
          {:else}
            <div class="ocr-list">
              {#each assets.filter((a) => a.type !== 'audio') as asset (asset.id)}
                {@const ocr = getOcrState(asset.id)}
                {@const filename = asset.path.split(/[/\\]/).pop() ?? 'Asset'}
                {@const busy = ocr.status === 'pending' || ocr.status === 'running'}
                <div class="ocr-item">
                  <div class="ocr-item-header">
                    <span class="ocr-filename">{filename}</span>
                    <button
                      class="ocr-btn"
                      disabled={busy}
                      onclick={() => handleExtractText(asset)}
                      title={busy ? 'Extraction in progress…' : 'Extract text from this asset'}
                    >
                      {busy ? 'Extracting…' : 'Extract Text'}
                    </button>
                  </div>

                  {#if ocr.status === 'running'}
                    <progress class="ocr-progress" value={ocr.progress} max="100">
                      {ocr.progress}%
                    </progress>
                    <p class="ocr-status-text">Running… {ocr.progress}%</p>
                  {:else if ocr.status === 'pending'}
                    <p class="ocr-status-text">Starting extraction…</p>
                  {:else if ocr.status === 'error'}
                    <p class="ocr-error">Extraction failed: {ocr.error}</p>
                  {:else if ocr.status === 'done'}
                    {@const editedText = ocrEditedText.get(asset.id) ?? ocr.textContent ?? ''}
                    {@const displayLength = editedText.length}
                    <details class="ocr-result">
                      <summary>
                        Extracted text
                        <span class="ocr-meta">
                          via {ocr.method ?? 'unknown'} · {displayLength} chars
                        </span>
                      </summary>
                      <textarea
                        class="ocr-result-body ocr-textarea"
                        rows="8"
                        oninput={(e) => {
                          const val = e.currentTarget.value
                          ocrEditedText.set(asset.id, val)
                          ocrStore.setTextContent(asset.id, val)
                          schedulePersist(asset.id, val)
                          ocrTick++
                        }}>{editedText}</textarea
                      >
                    </details>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </section>

        {#if assets.some((a) => a.type === 'audio')}
          <section class="section">
            <h3>Audio Transcription</h3>
            <div class="ocr-list">
              {#each assets.filter((a) => a.type === 'audio') as asset (asset.id)}
                {@const ts = getTranscriptionState(asset.id)}
                {@const filename = asset.path.split(/[/\\]/).pop() ?? 'Audio'}
                {@const busy = ts.status === 'pending' || ts.status === 'running'}
                <div class="ocr-item">
                  <div class="ocr-item-header">
                    <span class="ocr-filename">&#x1f50a; {filename}</span>
                    <button
                      class="ocr-btn"
                      disabled={busy}
                      onclick={() => handleTranscribeAudio(asset)}
                      title={busy ? 'Transcription in progress…' : 'Transcribe this audio file'}
                    >
                      {busy ? 'Transcribing…' : 'Transcribe'}
                    </button>
                  </div>

                  {#if ts.status === 'running'}
                    <progress class="ocr-progress" value={ts.progress} max="100">
                      {ts.progress}%
                    </progress>
                    <p class="ocr-status-text">Transcribing… {ts.progress}%</p>
                  {:else if ts.status === 'pending'}
                    <p class="ocr-status-text">Starting transcription…</p>
                  {:else if ts.status === 'error'}
                    <p class="ocr-error">Transcription failed: {ts.error}</p>
                  {:else if ts.status === 'done'}
                    {@const editedText = transEditedText.get(asset.id) ?? ts.text ?? ''}
                    {@const displayLength = editedText.length}
                    <details class="ocr-result">
                      <summary>
                        Transcription
                        <span class="ocr-meta">
                          {#if ts.language}{ts.language} &middot;
                          {/if}{displayLength} chars
                          {#if ts.durationMs}
                            &middot; {Math.round(ts.durationMs / 1000)}s{/if}
                        </span>
                      </summary>
                      <textarea
                        class="ocr-result-body ocr-textarea"
                        rows="8"
                        oninput={(e) => {
                          const val = e.currentTarget.value
                          transEditedText.set(asset.id, val)
                          transcriptionStore.setTextContent(asset.id, val)
                          scheduleTranscriptionPersist(asset.id, val)
                          transcriptionTick++
                        }}>{editedText}</textarea
                      >
                    </details>
                  {/if}
                </div>
              {/each}
            </div>
          </section>
        {/if}

        {#if assets.length > 0}
          <section class="section">
            <button
              class="analysis-toggle"
              onclick={() => {
                analysisOpen = !analysisOpen
                if (analysisOpen) {
                  loadEntities()
                  loadSimilarItems()
                  loadTriples()
                }
              }}
            >
              Analysis {analysisOpen ? '▲' : '▼'}
            </button>

            {#if analysisOpen}
              {@const nlp = getNlpState()}
              <div class="analysis-panel">
                <div class="nlp-actions">
                  <button
                    class="nlp-btn"
                    disabled={nlp.fts === 'pending' || nlp.fts === 'running'}
                    onclick={handleIndexFts}
                  >
                    Full-Text Index
                    <span class="nlp-badge nlp-badge--{nlp.fts}">{nlp.fts}</span>
                  </button>

                  <button
                    class="nlp-btn"
                    disabled={nlp.embed === 'pending' || nlp.embed === 'running'}
                    onclick={handleEmbedItem}
                  >
                    Generate Embeddings
                    <span class="nlp-badge nlp-badge--{nlp.embed}">{nlp.embed}</span>
                  </button>

                  <button
                    class="nlp-btn"
                    disabled={nlp.ner === 'pending' || nlp.ner === 'running'}
                    onclick={handleExtractEntities}
                  >
                    Extract Entities
                    <span class="nlp-badge nlp-badge--{nlp.ner}">{nlp.ner}</span>
                  </button>

                  <button
                    class="nlp-btn"
                    disabled={nlp.triples === 'pending' || nlp.triples === 'running'}
                    onclick={handleExtractTriples}
                  >
                    Extract Triples
                    <span class="nlp-badge nlp-badge--{nlp.triples}">{nlp.triples}</span>
                  </button>
                </div>

                <div class="entities-section">
                  <h4>Entities</h4>
                  <EntityViewer {entities} />
                </div>

                <div class="triples-section">
                  <h4>Semantic Triples (S|P|O)</h4>
                  {#if triples.length === 0}
                    <p class="empty-text">No triples extracted yet for this item.</p>
                  {:else}
                    <ul class="triples-list">
                      {#each triples as triple, i (`${triple.subject}-${triple.predicate}-${triple.object}-${i}`)}
                        <li class="triple-item">
                          <span class="triple-cell">{triple.subject}</span>
                          <span class="triple-cell">{triple.predicate}</span>
                          <span class="triple-cell">{triple.object}</span>
                        </li>
                      {/each}
                    </ul>
                  {/if}
                </div>

                {#if similarItemIds.length > 0}
                  <div class="similar-section">
                    <h4>Similar Items</h4>
                    <ul class="similar-list">
                      {#each similarItemIds.slice(0, 5) as id (id)}
                        <li class="similar-item">{id}</li>
                      {/each}
                    </ul>
                  </div>
                {:else}
                  <div class="similar-section">
                    <h4>Similar Items</h4>
                    <p class="empty-text">
                      No embeddings yet. Generate embeddings to find similar items.
                    </p>
                  </div>
                {/if}
              </div>
            {/if}
          </section>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .item-view {
    display: grid;
    grid-template-columns: 1fr 380px;
    gap: var(--space-4);
    height: 100%;
  }
  .left-panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    overflow-y: auto;
  }
  .right-panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    overflow-y: auto;
    padding: var(--space-3);
    border-left: 1px solid var(--color-border);
  }
  .item-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }
  .section h3 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-secondary);
    margin-bottom: var(--space-2);
  }
  .section {
    display: flex;
    flex-direction: column;
  }
  .asset-list {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .asset-thumb {
    padding: var(--space-1) var(--space-2);
    font-size: var(--font-size-xs);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    cursor: pointer;
  }
  .asset-thumb.active {
    border-color: var(--color-primary);
    background: var(--color-primary-subtle);
  }
  .empty-viewer {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 300px;
    color: var(--color-text-secondary);
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-md);
  }
  .notes-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .note-content {
    white-space: pre-wrap;
  }
  .note-date {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin-top: var(--space-1);
  }
  .note-actions {
    display: flex;
    gap: var(--space-1);
    margin-top: var(--space-2);
  }
  .note-edit {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .note-edit__textarea {
    width: 100%;
    min-height: 72px;
    padding: var(--space-2);
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
    color: var(--color-text-primary);
    background-color: var(--color-surface);
    border: 1px solid var(--color-accent);
    border-radius: var(--radius-md);
    outline: none;
    resize: vertical;
    box-sizing: border-box;
    box-shadow: 0 0 0 2px rgba(108, 142, 245, 0.2);
  }
  .note-edit__textarea:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(108, 142, 245, 0.3);
  }
  .note-edit__actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }
  .empty-text {
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .saving {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-weight: normal;
  }
  .status {
    color: var(--color-text-secondary);
    text-align: center;
  }
  .error {
    color: var(--color-danger);
  }

  /* ── OCR UI ── */
  .ocr-badge {
    display: inline-block;
    margin-left: var(--space-1);
    padding: 1px 5px;
    font-size: 10px;
    border-radius: var(--radius-sm);
    vertical-align: middle;
    text-transform: uppercase;
    font-weight: var(--font-weight-medium);
    background: var(--color-surface);
    color: var(--color-text-secondary);
    border: 1px solid var(--color-border);
  }
  .ocr-badge--running {
    background: var(--color-warning-subtle, #fef9c3);
    color: var(--color-warning, #ca8a04);
  }
  .ocr-badge--pending {
    background: var(--color-info-subtle, #eff6ff);
    color: var(--color-info, #3b82f6);
  }
  .ocr-badge--done {
    background: var(--color-success-subtle, #f0fdf4);
    color: var(--color-success, #16a34a);
  }
  .ocr-badge--error {
    background: var(--color-danger-subtle, #fef2f2);
    color: var(--color-danger, #dc2626);
  }

  .ocr-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .ocr-item {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .ocr-item-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }
  .ocr-filename {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .ocr-btn {
    padding: var(--space-1) var(--space-2);
    font-size: var(--font-size-xs);
    border: 1px solid var(--color-primary);
    border-radius: var(--radius-sm);
    background: var(--color-primary-subtle);
    color: var(--color-primary);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .ocr-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    border-color: var(--color-border);
    background: var(--color-surface);
    color: var(--color-text-muted);
  }
  .ocr-progress {
    width: 100%;
    height: 6px;
    border-radius: var(--radius-sm);
    appearance: none;
  }
  .ocr-status-text {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }
  .ocr-error {
    font-size: var(--font-size-xs);
    color: var(--color-danger);
  }
  .ocr-result {
    font-size: var(--font-size-sm);
  }
  .ocr-result summary {
    cursor: pointer;
    color: var(--color-text-secondary);
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .ocr-meta {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }
  .ocr-result-body {
    margin-top: var(--space-2);
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .ocr-textarea {
    width: 100%;
    min-height: 8rem;
    padding: var(--space-2);
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, monospace);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    color: var(--color-text-secondary);
    background: var(--color-surface-alt, rgba(0, 0, 0, 0.03));
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    resize: vertical;
    white-space: pre-wrap;
    word-break: break-word;
    outline: none;
    transition: border-color 0.15s ease;
  }
  .ocr-textarea:focus {
    border-color: var(--color-primary, #4a90d9);
    box-shadow: 0 0 0 2px rgba(74, 144, 217, 0.15);
  }
  .ocr-textarea:hover:not(:focus) {
    border-color: var(--color-border-hover, rgba(0, 0, 0, 0.15));
  }

  /* ── Analysis Panel ── */
  .analysis-toggle {
    width: 100%;
    text-align: left;
    padding: var(--space-2) var(--space-3);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-secondary);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  .analysis-toggle:hover {
    border-color: var(--color-text-muted);
  }

  .analysis-panel {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-top: none;
    border-radius: 0 0 var(--radius-md) var(--radius-md);
  }

  .nlp-actions {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .nlp-btn {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-2) var(--space-3);
    font-size: var(--font-size-sm);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    cursor: pointer;
    color: var(--color-text-primary);
    font-family: var(--font-sans);
  }

  .nlp-btn:hover:not(:disabled) {
    border-color: var(--color-accent);
    background: var(--color-surface-raised);
  }

  .nlp-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .nlp-badge {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--radius-full);
    text-transform: uppercase;
    font-weight: var(--font-weight-medium);
    background: var(--color-surface-raised);
    color: var(--color-text-muted);
    border: 1px solid var(--color-border);
  }

  .nlp-badge--running {
    background: var(--color-warning-subtle, #fef9c3);
    color: var(--color-warning, #ca8a04);
    border-color: transparent;
  }

  .nlp-badge--pending {
    background: var(--color-info-subtle, #eff6ff);
    color: var(--color-info, #3b82f6);
    border-color: transparent;
  }

  .nlp-badge--done {
    background: var(--color-success-subtle, #f0fdf4);
    color: var(--color-success, #16a34a);
    border-color: transparent;
  }

  .nlp-badge--error {
    background: var(--color-danger-subtle, #fef2f2);
    color: var(--color-danger, #dc2626);
    border-color: transparent;
  }

  .entities-section,
  .triples-section,
  .similar-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .entities-section h4,
  .similar-section h4 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-secondary);
  }

  .similar-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .similar-item {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
  }

  .triples-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .triple-item {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
  }

  .triple-cell {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
