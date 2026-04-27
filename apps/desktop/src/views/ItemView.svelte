<script lang="ts">
  import { getStore } from '$lib/db'
  import { getAssetUrl } from '$lib/file-import'
  import { OcrStore, extractText, type OcrMode } from '$lib/ocr'
  import { TranscriptionStore, transcribeAudio } from '$lib/transcription'
  import {
    NlpStore,
    indexFts,
    embedItem,
    embedAsset,
    extractEntities,
    extractEntitiesForAsset,
    extractTriples,
    extractTriplesForAsset,
    similarItems as fetchSimilarItems,
  } from '$lib/nlp'
  import {
    LlmStore,
    llmSummarize,
    llmCorrectOcr,
    llmExtractEntities,
    llmExtractTriples,
    llmSummarizeAsset,
    llmCorrectOcrAsset,
    llmExtractEntitiesAsset,
    llmExtractTriplesAsset,
    llmIsAvailable,
    llmIsMultimodal,
    llmGetResult,
  } from '$lib/llm'
  import { GeoStore } from '$lib/geo'
  import {
    DocumentViewer,
    MetadataEditor,
    NoteEditor,
    Button,
    Card,
    EntityViewer,
    MapViewer,
    TopicEditor,
  } from '@entropia/ui'
  import type { MapMarker } from '@entropia/ui'
  import { onMount, onDestroy } from 'svelte'
  import { listen, emit } from '@tauri-apps/api/event'
  import { invoke } from '@tauri-apps/api/core'
  import { navigation } from '$lib/navigation'
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
    EditTool,
    ImageEditResult,
  } from '@entropia/ui'
  import { TranscriptionRepo } from '@entropia/store'

  const isDev = import.meta.env.DEV

  // ── Sidebar resize ──
  const MIN_SIDEBAR_PCT = 20
  const MAX_SIDEBAR_PCT = 50
  const DEFAULT_SIDEBAR_PCT = 33

  let sidebarWidth = $state((() => {
    try {
      const stored = localStorage.getItem('entropia-sidebar-width')
      if (stored !== null) {
        const parsed = Number(stored)
        if (!isNaN(parsed)) {
          return Math.max(MIN_SIDEBAR_PCT, Math.min(MAX_SIDEBAR_PCT, parsed))
        }
      }
    } catch {}
    return DEFAULT_SIDEBAR_PCT
  })())

  let isDragging = $state(false)
  let itemViewEl: HTMLElement | undefined = $state()
  let dragCleanup: (() => void) | null = null

  function onResizeHandlePointerDown(e: PointerEvent) {
    e.preventDefault()
    isDragging = true

    const startX = e.clientX
    const startWidthPct = sidebarWidth
    const containerEl = itemViewEl ?? document.querySelector('.item-view') ?? document.body
    const containerWidth = (containerEl as HTMLElement).clientWidth

    let rafId: number | null = null
    let lastClientX = startX

    function onPointerMove(e: PointerEvent) {
      lastClientX = e.clientX
      if (rafId !== null) return
      rafId = requestAnimationFrame(() => {
        const deltaX = lastClientX - startX
        const deltaPct = (deltaX / containerWidth) * 100
        sidebarWidth = Math.max(MIN_SIDEBAR_PCT, Math.min(MAX_SIDEBAR_PCT, startWidthPct - deltaPct))
        rafId = null
      })
    }

    function onPointerUp() {
      isDragging = false
      try {
        localStorage.setItem('entropia-sidebar-width', String(Math.round(sidebarWidth)))
      } catch {}
      window.removeEventListener('pointermove', onPointerMove)
      window.removeEventListener('pointerup', onPointerUp)
      document.body.classList.remove('no-select')
      dragCleanup = null
    }

    document.body.classList.add('no-select')
    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', onPointerUp)
    dragCleanup = onPointerUp
  }

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

  // Image edit state
  let editTool = $state<EditTool>('none')
  let imageVersion = $state(0)

  // Undo history: stack of { path, width, height, annotations } snapshots
  // Each entry represents the state BEFORE an edit operation. Popping restores
  // the asset to that state (the file is still on disk because edits create
  // versioned files rather than overwriting in-place).
  interface UndoEntry {
    path: string
    width: number
    height: number
    annotations: ViewerAnnotation[]
  }
  let undoStack = $state<UndoEntry[]>([])
  let canUndo = $derived(undoStack.length > 0)
  let lastSelectedAssetId = $state<string | null>(null)

  // OCR state — plain TS class, updated via Tauri events
  const ocrStore = new OcrStore({
    onComplete: (assetId) => {
      // After OCR extraction completes on a specific asset, auto-trigger
      // asset-level enrichment for that asset
      if (selectedAsset && selectedAsset.id === assetId) {
        void extractEntitiesForAsset(itemId, assetId).catch(() => {})
        void extractTriplesForAsset(itemId, assetId).catch(() => {})
      }
      // Auto-summarize: OCR completed → text is now available
      void autoSummarizeIfNeeded(assetId)
    },
  })
  // Reactive tick counter: incremented on every OCR event to force Svelte re-evaluation
  let ocrTick = $state(0)
  // Edited text per asset — tracks user corrections to OCR output
  let ocrEditedText = $state(new Map<string, string>())
  // Debounce timers per asset for persisting edits to DB
  let ocrPersistTimers = $state(new Map<string, ReturnType<typeof setTimeout>>())
  // Debounce timers per asset for downstream NLP reprocessing after user inactivity
  let assetReanalysisTimers = $state(new Map<string, ReturnType<typeof setTimeout>>())

  // Transcription state — mirrors OcrStore pattern for audio assets
  const transcriptionStore = new TranscriptionStore({
    onComplete: (assetId) => {
      // After transcription completes, auto-trigger enrichment for that asset
      if (selectedAsset && selectedAsset.id === assetId) {
        void extractEntitiesForAsset(itemId, assetId).catch(() => {})
        void extractTriplesForAsset(itemId, assetId).catch(() => {})
      }
      // Auto-summarize: transcription completed → text is now available
      void autoSummarizeIfNeeded(assetId)
    },
  })
  let transcriptionTick = $state(0)

  let transEditedText = $state(new Map<string, string>())
  let transPersistTimers = $state(new Map<string, ReturnType<typeof setTimeout>>())

  const PERSIST_IDLE_MS = 500
  const REANALYSIS_IDLE_MS = 1500

  function scheduleAssetReanalysis(assetId: string) {
    const existing = assetReanalysisTimers.get(assetId)
    if (existing) clearTimeout(existing)

    const timer = setTimeout(async () => {
      const jobs = [
        ['ner', () => extractEntitiesForAsset(itemId, assetId)],
        ['triples', () => extractTriplesForAsset(itemId, assetId)],
        ['fts', () => indexFts(itemId)],
        ['embed', () => embedAsset(itemId, assetId)],
      ] as const

      try {
        console.info('[ItemView] Re-running post-edit analysis', { itemId, assetId })

        const results = await Promise.allSettled(jobs.map(([, run]) => run()))
        results.forEach((result, index) => {
          const [jobName] = jobs[index]
          if (result.status === 'rejected') {
            console.error(`[ItemView] Post-edit ${jobName} failed`, result.reason)
          }
        })
      } finally {
        assetReanalysisTimers.delete(assetId)
      }
    }, REANALYSIS_IDLE_MS)

    assetReanalysisTimers.set(assetId, timer)
  }

  /** Save quickly, but only re-run expensive analysis after longer inactivity. */
  function schedulePersist(assetId: string, text: string) {
    // Cancel any pending timer for this asset
    const existing = ocrPersistTimers.get(assetId)
    if (existing) clearTimeout(existing)

    // Schedule new persist
    const timer = setTimeout(async () => {
      try {
        await invoke('update_extraction_text_cmd', { assetId, textContent: text })
        scheduleAssetReanalysis(assetId)
      } catch (e) {
        console.error('[ItemView] Failed to persist OCR correction:', e)
      }
      ocrPersistTimers.delete(assetId)
    }, PERSIST_IDLE_MS)

    ocrPersistTimers.set(assetId, timer)
  }

  /** Schedule a debounced persist of edited transcription text to the DB. */
  function scheduleTranscriptionPersist(assetId: string, text: string) {
    const existing = transPersistTimers.get(assetId)
    if (existing) clearTimeout(existing)

    const timer = setTimeout(async () => {
      try {
        await invoke('update_transcription_text_cmd', { assetId, textContent: text })
        scheduleAssetReanalysis(assetId)
      } catch (e) {
        console.error('[ItemView] Failed to persist transcription correction:', e)
      }
      transPersistTimers.delete(assetId)
    }, PERSIST_IDLE_MS)

    transPersistTimers.set(assetId, timer)
  }

  // NLP state — mirrors OcrStore pattern
  const nlpStore = new NlpStore()
  let nlpTick = $state(0)
  let entities = $state<Entity[]>([])
  type EditableEntityType = 'person' | 'organization' | 'place' | 'misc' | 'date'
  const EDITABLE_ENTITY_TYPES: EditableEntityType[] = [
    'person',
    'organization',
    'place',
    'misc',
    'date',
  ]
  let newEntityValue = $state('')
  let newEntityType = $state<EditableEntityType>('organization')
  let editingEntityId = $state<string | null>(null)
  let editingEntityValue = $state('')
  let editingEntityType = $state<EditableEntityType>('organization')
  let entityEditorOpen = $state(false)
  let entityActionError = $state<string | null>(null)
  let similarItems = $state<
    Array<{ itemId: string; title: string; collectionId: string; similarity: number }>
  >([])
  let ftsQuery = $state('')
  let ftsResults = $state<Array<{ itemId: string; title: string; rank: number; collectionId: string }>>(
    []
  )
  let ftsSearching = $state(false)
  let ftsSearchError = $state<string | null>(null)
  let ftsSearchTimer: ReturnType<typeof setTimeout> | null = null
  let ftsIndexedRows = $state<number | null>(null)
  let ftsDebug = $state<{
    rawQuery: string
    sanitizedQuery: string
    strategy: 'empty' | 'strict' | 'relaxed'
    matchCount: number
    hydratedCount: number
    resultIds: string[]
  } | null>(null)
  let triples = $state<Array<{ subject: string; predicate: string; object: string }>>([])
  let analysisOpen = $state(false)

  // LLM state (Gemma 4)
  const llmStore = new LlmStore({
    onComplete: (id, job, result) => {
      llmTick++
      // Track summary results in the dedicated map
      if (job === 'summarize') {
        summaryTexts.set(id, result)
        summaryTick++
      }
      // When OCRC completes, replace the OCR extracted text with the corrected text
      if (job === 'correct_ocr') {
        ocrCorrectedAssets.add(id)
        ocrTick++ // Force Svelte reactivity for the textarea
        const assetId = selectedAsset?.id === id ? id : null
        if (assetId) {
          ocrEditedText.set(assetId, result)
          ocrStore.setTextContent(assetId, result)
          schedulePersist(assetId, result)
        } else {
          // Item-level (legacy): update the first asset's text or whichever asset has OCR text
          const asset = assets.find((a: Asset) => ocrStore.getTextContent(a.id))
          if (asset) {
            ocrEditedText.set(asset.id, result)
            ocrStore.setTextContent(asset.id, result)
            schedulePersist(asset.id, result)
          }
        }
      }
    },
    onCorrectOcr: (id, _result) => {
      // Track that OCRC already ran for this asset (from persisted results or live)
      ocrCorrectedAssets.add(id)
    },
  })
  let llmTick = $state(0)

  // OCRC tracking: once OCRC is done for an asset, hide the button and show
  // only Embedding + Triple buttons in the LLM section.
  let ocrCorrectedAssets = $state(new Set<string>()) // asset IDs that have been OCRC'd

  // Auto-summary: tracks whether LLM is available and per-asset summary text
  let llmAvailable = $state(false)
  let llmMultimodal = $state(false)
  let summaryTexts = $state(new Map<string, string>()) // assetId → summary text
  let summaryTick = $state(0) // reactivity trigger for summary display
  let autoSummarizeTriggered = $state(new Set<string>()) // asset IDs we've already queued

  /**
   * Get the LLM state for the currently active context.
   * When a specific asset/page is selected (multipage), use the asset ID
   * so LLM state is scoped per-page. Otherwise fall back to item ID.
   */
  function getLlmState() {
    void llmTick
    const targetId = selectedAsset ? selectedAsset.id : itemId
    return llmStore.getState(targetId)
  }

  async function handleLlmSummarize() {
    try {
      if (selectedAsset) {
        await llmSummarizeAsset(selectedAsset.id)
      } else {
        await llmSummarize(itemId)
      }
    } catch (e) {
      console.error('[LLM] summarize failed:', e)
    }
  }

  async function handleLlmCorrectOcr() {
    try {
      if (selectedAsset) {
        await llmCorrectOcrAsset(selectedAsset.id)
      } else {
        await llmCorrectOcr(itemId)
      }
    } catch (e) {
      console.error('[LLM] correct OCR failed:', e)
    }
  }

  async function handleLlmExtractEntities() {
    try {
      if (selectedAsset) {
        await llmExtractEntitiesAsset(selectedAsset.id)
      } else {
        await llmExtractEntities(itemId)
      }
    } catch (e) {
      console.error('[LLM] extract entities failed:', e)
    }
  }

  async function handleLlmExtractTriples() {
    try {
      if (selectedAsset) {
        await llmExtractTriplesAsset(selectedAsset.id)
      } else {
        await llmExtractTriples(itemId)
      }
    } catch (e) {
      console.error('[LLM] extract triples failed:', e)
    }
  }

  /**
   * Auto-trigger summarization for a given asset.
   * Only fires if: (a) the LLM engine is available, (b) the asset hasn't
   * been queued already, and (c) no existing summary is stored in the DB.
   */
  async function autoSummarizeIfNeeded(assetId: string) {
    if (!llmAvailable) return
    if (autoSummarizeTriggered.has(assetId)) return

    // Check if a summary already exists for this asset
    try {
      const existing = await llmGetResult(assetId, 'summarize')
      if (existing) {
        // Already have a summary — just populate the display
        summaryTexts.set(assetId, existing.result)
        summaryTick++
        return
      }
    } catch {
      // DB query failed — continue to try auto-summarize anyway
    }

    autoSummarizeTriggered.add(assetId)
    try {
      await llmSummarizeAsset(assetId)
    } catch (e) {
      console.error('[LLM] auto-summarize failed:', e)
    }
  }

  // Geo state (OpenStreetMap)
  const geoStore = new GeoStore({
    onEntityComplete: () => {
      loadGeoMarkers()
    },
    onItemComplete: () => {
      loadGeoMarkers()
    },
  })
  let geoMarkers = $state<MapMarker[]>([])

  async function loadGeoMarkers() {
    try {
      const rows = await invoke<
        Array<{ id: string; value: string; latitude: number; longitude: number }>
      >('db_select', {
        sql: `SELECT id, value, latitude, longitude FROM entities
              WHERE item_id = ? AND entity_type = 'place' AND geo_status = 'resolved'
              AND latitude IS NOT NULL AND longitude IS NOT NULL`,
        params: [itemId],
      })
      geoMarkers = rows.map((r) => ({
        entityId: r.id,
        label: r.value,
        latitude: r.latitude,
        longitude: r.longitude,
      }))
    } catch (e) {
      console.error('[geo] Failed to load markers:', e)
    }
  }

  let metadataValue = $derived<Record<string, string>>(
    item?.metadata ? parseMetadataRecord(item.metadata) : {}
  )

  // Topic state
  let itemTopics = $state<string[]>([])
  let topicSuggestions = $state<string[]>([])

  async function loadTopics() {
    try {
      const topics = await getStore().topics.findByItemId(itemId)
      itemTopics = topics.map((t) => t.name)
    } catch (e) {
      console.error('[topics] Failed to load topics:', e)
    }
  }

  async function loadTopicSuggestions() {
    try {
      topicSuggestions = await getStore().topics.allNames()
    } catch (e) {
      console.error('[topics] Failed to load suggestions:', e)
    }
  }

  async function handleTopicsChange(newTopics: string[]) {
    try {
      const store = getStore()
      // Find topics to add (in new but not in current)
      const currentSet = new Set(itemTopics)
      const newSet = new Set(newTopics)
      // Add new topics
      for (const name of newTopics) {
        if (!currentSet.has(name)) {
          await store.topics.addTopicToItem(itemId, name)
        }
      }
      // Remove topics no longer present
      for (const name of itemTopics) {
        if (!newSet.has(name)) {
          const topic = await store.topics.findByName(name)
          if (topic) {
            await store.topics.removeTopicFromItem(itemId, topic.id)
          }
        }
      }
      itemTopics = newTopics.map((t) => t.toUpperCase())
      // Refresh suggestions to include any newly created topics
      void loadTopicSuggestions()
    } catch (e) {
      console.error('[topics] Failed to save topics:', e)
    }
  }

  let selectedAsset = $derived(assets[selectedAssetIndex] ?? null)

  let viewerSrc = $derived(
    selectedAsset
      ? getAssetUrl(selectedAsset.path) + (imageVersion > 0 ? `?_t=${imageVersion}` : '')
      : ''
  )

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

  // ── Image editing handlers ────────────────────────────────────────────

  /** Convert normalized (0-1) region to pixel coordinates based on image dimensions */
  function normalizedToPixels(
    region: { x: number; y: number; width: number; height: number },
    naturalW: number,
    naturalH: number
  ) {
    return {
      x: Math.round(region.x * naturalW),
      y: Math.round(region.y * naturalH),
      width: Math.round(region.width * naturalW),
      height: Math.round(region.height * naturalH),
    }
  }

  /** Adjust annotations after a rotation. Converted = new image dimensions. */
  function adjustAnnotationsAfterRotation(
    rotation: 'left' | 'right'
  ) {
    annotations = annotations.map((a) => {
      if (rotation === 'right') {
        // 90° CW: new_x = 1 - old_y - old_height, new_y = old_x
        const nx = 1 - a.y - a.height
        const ny = a.x
        return { ...a, x: nx, y: ny, width: a.height, height: a.width }
      } else {
        // 90° CCW: new_x = old_y, new_y = 1 - old_x - old_width
        const nx = a.y
        const ny = 1 - a.x - a.width
        return { ...a, x: nx, y: ny, width: a.height, height: a.width }
      }
    })
  }

  /** Adjust annotations after a crop. Region is the crop area in normalized coords. */
  function adjustAnnotationsAfterCrop(
    region: { x: number; y: number; width: number; height: number }
  ) {
    const { x: cx, y: cy, width: cw, height: ch } = region
    annotations = annotations
      .filter((a) => {
        // Keep annotations that overlap with the crop region
        const overlapsX = a.x < cx + cw && a.x + a.width > cx
        const overlapsY = a.y < cy + ch && a.y + a.height > cy
        return overlapsX && overlapsY
      })
      .map((a) => {
        // Clamp to crop region
        const clampedX = Math.max(a.x, cx)
        const clampedY = Math.max(a.y, cy)
        const clampedRight = Math.min(a.x + a.width, cx + cw)
        const clampedBottom = Math.min(a.y + a.height, cy + ch)
        const newWidth = clampedRight - clampedX
        const newHeight = clampedBottom - clampedY
        return {
          ...a,
          x: (clampedX - cx) / cw,
          y: (clampedY - cy) / ch,
          width: newWidth / cw,
          height: newHeight / ch,
        }
      })
  }

  async function handleEditSelect(
    region: { x: number; y: number; width: number; height: number }
  ) {
    if (!selectedAsset || selectedAsset.type !== 'image') return
    if (imageNaturalW === 0 || imageNaturalH === 0) return

    await flushPendingAnnotationSave()

    const asset = selectedAsset
    const pixelRegion = normalizedToPixels(region, imageNaturalW, imageNaturalH)

    // Push current state onto undo stack before performing the edit
    undoStack = [...undoStack, {
      path: asset.path,
      width: imageNaturalW,
      height: imageNaturalH,
      annotations: JSON.parse(JSON.stringify(annotations)),
    }]

    try {
      if (editTool === 'crop') {
        const result: ImageEditResult = await invoke('crop_image', {
          path: asset.path,
          x: pixelRegion.x,
          y: pixelRegion.y,
          width: pixelRegion.width,
          height: pixelRegion.height,
        })
        adjustAnnotationsAfterCrop(region)
        await handleImageEditResult(result, asset.id)
      } else if (editTool === 'erase') {
        const result: ImageEditResult = await invoke('erase_region', {
          path: asset.path,
          x: pixelRegion.x,
          y: pixelRegion.y,
          width: pixelRegion.width,
          height: pixelRegion.height,
          fill: 'white',
        })
        await handleImageEditResult(result, asset.id)
      }
    } catch (e) {
      // On failure, pop the undo entry since the edit didn't succeed
      undoStack = undoStack.slice(0, -1)
      console.error('[ItemView] Image edit failed:', e)
    } finally {
      // Reset edit tool after operation
      editTool = 'none'
    }
  }

  async function handleRotateLeft() {
    if (!selectedAsset || selectedAsset.type !== 'image') return
    await flushPendingAnnotationSave()
    const asset = selectedAsset

    // Push current state onto undo stack before rotating
    undoStack = [...undoStack, {
      path: asset.path,
      width: imageNaturalW,
      height: imageNaturalH,
      annotations: JSON.parse(JSON.stringify(annotations)),
    }]

    try {
      const result: ImageEditResult = await invoke('rotate_image', {
        path: asset.path,
        direction: 'left',
      })
      adjustAnnotationsAfterRotation('left')
      await handleImageEditResult(result, asset.id)
    } catch (e) {
      undoStack = undoStack.slice(0, -1)
      console.error('[ItemView] Rotate left failed:', e)
    }
  }

  async function handleRotateRight() {
    if (!selectedAsset || selectedAsset.type !== 'image') return
    await flushPendingAnnotationSave()
    const asset = selectedAsset

    // Push current state onto undo stack before rotating
    undoStack = [...undoStack, {
      path: asset.path,
      width: imageNaturalW,
      height: imageNaturalH,
      annotations: JSON.parse(JSON.stringify(annotations)),
    }]

    try {
      const result: ImageEditResult = await invoke('rotate_image', {
        path: asset.path,
        direction: 'right',
      })
      adjustAnnotationsAfterRotation('right')
      await handleImageEditResult(result, asset.id)
    } catch (e) {
      undoStack = undoStack.slice(0, -1)
      console.error('[ItemView] Rotate right failed:', e)
    }
  }

  /** Undo the last image edit: restore the asset path, dimensions,
   *  and annotations to the previous state. */
  async function handleUndo() {
    if (!selectedAsset || selectedAsset.type !== 'image') return
    if (undoStack.length === 0) return

    await flushPendingAnnotationSave()

    const entry = undoStack[undoStack.length - 1]!
    const assetId = selectedAsset.id

    // Restore state from undo entry
    const store = getStore()
    await store.assets.updatePath(assetId, entry.path)
    assets = assets.map((a) =>
      a.id === assetId ? { ...a, path: entry.path } : a
    )
    annotations = entry.annotations
    selectedAnnotationId = null
    // Force image refresh
    imageVersion++

    // Persist the restored annotations
    await persistAnnotations(assetId, annotations)

    // Pop the undo stack
    undoStack = undoStack.slice(0, -1)

    // Notify other views
    try {
      await emit('asset:image-updated', {
        itemId,
        assetId,
        path: entry.path,
      })
    } catch (e) {
      console.warn('[ItemView] Failed to emit asset:image-updated event on undo:', e)
    }
  }

  /** Post-edit: always update asset path in DB (even if format didn't change),
   *  refresh image, persist annotations, push undo entry, and notify other views. */
  async function handleImageEditResult(result: ImageEditResult, assetId: string) {
    // Always update the asset path in DB — versioned paths change on every edit,
    // and the DB must reflect the current file on disk.
    const store = getStore()
    await store.assets.updatePath(assetId, result.path)
    // Update the local assets array with the new path
    assets = assets.map((a) =>
      a.id === assetId ? { ...a, path: result.path } : a
    )

    // Force image refresh: bump version counter so the browser fetches the
    // new file (versioned paths already make the URL unique, but this helps
    // if something caches at the protocol level).
    imageVersion++

    // Persist adjusted annotations
    if (selectedAsset && selectedAsset.id === assetId) {
      await persistAnnotations(assetId, annotations)
    }

    // Notify CollectionView (and any other listeners) that the asset path
    // has changed, so they can invalidate their cached thumbnail URLs.
    try {
      await emit('asset:image-updated', {
        itemId,
        assetId,
        path: result.path,
      })
    } catch (e) {
      console.warn('[ItemView] Failed to emit asset:image-updated event:', e)
    }
  }

  // Track natural image dimensions for pixel coordinate conversion
  let imageNaturalW = $state(0)
  let imageNaturalH = $state(0)

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

  async function handleExtractText(asset: Asset, mode: OcrMode = 'light') {
    ocrStore._updateState(asset.id, { status: 'pending', progress: 0 })
    ocrTick++
    try {
      await extractText(asset.id, asset.path, asset.type, mode)
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
      if (selectedAsset) {
        await embedAsset(itemId, selectedAsset.id)
      } else {
        await embedItem(itemId)
      }
    } catch (e) {
      nlpStore._setJobStatus(itemId, 'embed', 'error', e instanceof Error ? e.message : 'Failed')
      nlpTick++
    }
  }

  async function handleExtractEntities() {
    nlpStore._setJobStatus(itemId, 'ner', 'pending')
    nlpTick++
    try {
      if (selectedAsset) {
        await extractEntitiesForAsset(itemId, selectedAsset.id)
      } else {
        await extractEntities(itemId)
      }
    } catch (e) {
      nlpStore._setJobStatus(itemId, 'ner', 'error', e instanceof Error ? e.message : 'Failed')
      nlpTick++
    }
  }

  async function handleExtractTriples() {
    nlpStore._setJobStatus(itemId, 'triples', 'pending')
    nlpTick++
    try {
      if (selectedAsset) {
        await extractTriplesForAsset(itemId, selectedAsset.id)
      } else {
        await extractTriples(itemId)
      }
    } catch (e) {
      nlpStore._setJobStatus(itemId, 'triples', 'error', e instanceof Error ? e.message : 'Failed')
      nlpTick++
    }
  }

  async function loadEntities() {
    try {
      const store = getStore()
      if (selectedAsset) {
        entities = ((await store.entities.findByAssetId(itemId, selectedAsset.id)) as Entity[]).filter(
          (entity) => entity.confidence == null || entity.confidence > 0.89
        )
      } else {
        entities = ((await store.entities.findByItemId(itemId)) as Entity[]).filter(
          (entity) => entity.confidence == null || entity.confidence > 0.89
        )
      }
    } catch {
      // Non-fatal: entities panel shows empty state
    }
  }

  function normalizeManualEntityValue(value: string) {
    return value.trim().replace(/^["'“”‘’«»\-–—\s]+|["'“”‘’«»\-–—\s]+$/g, '').trim()
  }

  function toEditableEntityType(entityType: Entity['entityType']): EditableEntityType {
    if (
      entityType === 'person' ||
      entityType === 'organization' ||
      entityType === 'place' ||
      entityType === 'misc' ||
      entityType === 'date'
    ) {
      return entityType
    }
    return 'organization'
  }

  async function handleCreateEntity() {
    const value = normalizeManualEntityValue(newEntityValue)
    if (!value) return
    try {
      await getStore().entities.create({
        itemId,
        assetId: selectedAsset?.id ?? null,
        entityType: newEntityType,
        value,
        startOffset: 0,
        endOffset: 0,
        confidence: 1.0,
        source: 'manual',
        modelName: null,
        createdAt: Date.now(),
      })
      newEntityValue = ''
      newEntityType = 'organization'
      entityActionError = null
      await loadEntities()
    } catch (e) {
      entityActionError = e instanceof Error ? e.message : 'Failed to add entity'
    }
  }

  function startEditingEntity(entity: Entity) {
    editingEntityId = entity.id
    editingEntityValue = entity.value
    editingEntityType = toEditableEntityType(entity.entityType)
    entityEditorOpen = true
    entityActionError = null
  }

  function cancelEditingEntity() {
    editingEntityId = null
    editingEntityValue = ''
    editingEntityType = 'organization'
    entityEditorOpen = false
  }

  async function handleSaveEntity(entityId: string) {
    const value = normalizeManualEntityValue(editingEntityValue)
    if (!value) return
    try {
      await getStore().entities.update(entityId, {
        entityType: editingEntityType,
        value,
        confidence: 1.0,
        source: 'manual',
      })
      cancelEditingEntity()
      entityActionError = null
      await loadEntities()
    } catch (e) {
      entityActionError = e instanceof Error ? e.message : 'Failed to save entity'
    }
  }

  async function handleDeleteEntity(entityId: string) {
    try {
      await getStore().entities.delete(entityId)
      if (editingEntityId === entityId) {
        cancelEditingEntity()
      }
      entityActionError = null
      await loadEntities()
    } catch (e) {
      entityActionError = e instanceof Error ? e.message : 'Failed to delete entity'
    }
  }

  async function loadSimilarItems() {
    try {
      const results = await fetchSimilarItems(itemId, 5)
      similarItems = results.map((r) => ({
        itemId: r.itemId,
        title: r.title,
        collectionId: r.collectionId,
        similarity: r.similarity,
      }))
    } catch {
      similarItems = []
    }
  }

  function navigateToSimilarItem(item: { itemId: string; title: string; collectionId: string }) {
    navigation.replace({
      name: 'item',
      itemId: item.itemId,
      collectionId: item.collectionId,
      collectionName: '',
      itemTitle: item.title || item.itemId,
    })
  }

  function clearFtsSearchTimer() {
    if (ftsSearchTimer) {
      clearTimeout(ftsSearchTimer)
      ftsSearchTimer = null
    }
  }

  function escapeRegExp(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  }

  function getFtsTerms(rawQuery: string): string[] {
    if (!rawQuery.trim()) return []

    const noOperators = rawQuery.replace(/\b(AND|OR|NOT|NEAR)\b/gi, ' ')
    const terms = noOperators
      .split(/\s+/)
      .map((token) => token.replace(/[()"\-*^:,./\\]/g, '').trim())
      .filter((token) => token.length > 0)

    return Array.from(new Set(terms.map((token) => token.toLocaleLowerCase())))
  }

  function splitHighlightedSegments(text: string, rawQuery: string) {
    const terms = getFtsTerms(rawQuery)
    if (terms.length === 0 || !text) return [{ text, isMatch: false }]

    const pattern = terms
      .slice()
      .sort((a, b) => b.length - a.length)
      .map((term) => escapeRegExp(term))
      .join('|')

    if (!pattern) return [{ text, isMatch: false }]

    const regex = new RegExp(pattern, 'gi')
    const segments: Array<{ text: string; isMatch: boolean }> = []
    let lastIndex = 0

    for (const match of text.matchAll(regex)) {
      const index = match.index ?? 0
      const value = match[0] ?? ''
      if (index > lastIndex) {
        segments.push({ text: text.slice(lastIndex, index), isMatch: false })
      }
      if (value) {
        segments.push({ text: value, isMatch: true })
      }
      lastIndex = index + value.length
    }

    if (lastIndex < text.length) {
      segments.push({ text: text.slice(lastIndex), isMatch: false })
    }

    return segments.length > 0 ? segments : [{ text, isMatch: false }]
  }

  async function runFtsSearch(rawQuery: string) {
    const query = rawQuery.trim()
    if (!query) {
      ftsResults = []
      ftsSearchError = null
      ftsSearching = false
      ftsDebug = null
      return
    }

    ftsSearching = true
    ftsSearchError = null

    try {
      const store = getStore()
      if (isDev) {
        const stats = await store.fts.stats()
        ftsIndexedRows = stats.totalRows
      }

      const response = await store.fts.searchWithDebug(query, 10)
      const rows = response.results

      const hydrated = await Promise.all(
        rows.map(async (row) => {
          const found = await store.items.findById(row.itemId)
          if (!found) return null

          return {
            itemId: found.id,
            title: found.title,
            rank: row.rank,
            collectionId: found.collectionId,
          }
        })
      )

      ftsResults = hydrated.filter(
        (row): row is { itemId: string; title: string; rank: number; collectionId: string } => !!row
      )

      if (isDev) {
        ftsDebug = {
          ...response.debug,
          hydratedCount: ftsResults.length,
        }
      }
    } catch {
      ftsResults = []
      ftsSearchError = 'No se pudo ejecutar la búsqueda full-text.'
      if (isDev) {
        ftsDebug = null
      }
    } finally {
      ftsSearching = false
    }
  }

  async function loadFtsStats() {
    if (!isDev) return

    try {
      const store = getStore()
      const stats = await store.fts.stats()
      ftsIndexedRows = stats.totalRows
    } catch {
      ftsIndexedRows = null
    }
  }

  function handleFtsInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value
    ftsQuery = value

    clearFtsSearchTimer()
    if (!value.trim()) {
      ftsResults = []
      ftsSearchError = null
      ftsSearching = false
      ftsDebug = null
      return
    }

    ftsSearchTimer = setTimeout(() => {
      void runFtsSearch(value)
    }, 250)
  }

  function handleFtsKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault()
      clearFtsSearchTimer()
      void runFtsSearch(ftsQuery)
      return
    }

    if (event.key === 'Escape') {
      event.preventDefault()
      clearFtsSearchTimer()
      ftsQuery = ''
      ftsResults = []
      ftsSearchError = null
      ftsSearching = false
      ftsDebug = null
    }
  }

  async function loadTriples() {
    try {
      const store = getStore()
      if (selectedAsset) {
        triples = await store.triples.findByAssetId(itemId, selectedAsset.id)
      } else {
        triples = await store.triples.findByItemId(itemId)
      }
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
      await store.notes.create({ itemId, assetId: selectedAsset?.id ?? null, content })
      notes = await loadNotesForAsset()
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to save note'
    }
  }

  async function handleDeleteNote(noteId: string) {
    try {
      error = null
      const store = getStore()
      await store.notes.delete(noteId)
      notes = await loadNotesForAsset()
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
      notes = await loadNotesForAsset()
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

  /** Load notes scoped to the current asset (plus item-level notes). */
  async function loadNotesForAsset(): Promise<Note[]> {
    if (!selectedAsset) {
      const store = getStore()
      return store.notes.findByItem(itemId)
    }
    const store = getStore()
    return store.notes.findByAsset(itemId, selectedAsset.id)
  }

  async function loadData() {
    try {
      loading = true
      error = null
      selectedAssetIndex = 0 // Reset page selection on item change
      const store = getStore()
      const [loadedItem, loadedAssets] = await Promise.all([
        store.items.findById(itemId),
        store.assets.findByItem(itemId),
      ])
      item = loadedItem
      assets = loadedAssets
      // Asset-scoped data (notes, entities, triples) will be loaded by the selectedAsset effect
      // Load item-scoped data (similar items, topics) - not asset-dependent
      void loadSimilarItems()
      void loadTopics()
      void loadTopicSuggestions()
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load item'
    } finally {
      loading = false
    }
  }

  $effect(() => {
    const asset = selectedAsset
    const currentAssetId = asset?.id ?? null
    const switchedAsset = currentAssetId !== lastSelectedAssetId

    lastSelectedAssetId = currentAssetId

    if (switchedAsset) {
      selectedAnnotationId = null
      annotationTool = 'select'
      editTool = 'none'
      // Reset undo stack only when switching to a DIFFERENT asset by id.
      // Editing the same asset creates a new versioned path, which should NOT
      // clear undo history.
      undoStack = []
    }

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

  // Reload asset-scoped data when the selected asset changes
  $effect(() => {
    const asset = selectedAsset
    if (!asset) return

    // Reload notes for this asset (plus item-level notes)
    void loadNotesForAsset().then((loadedNotes) => {
      notes = loadedNotes
    })

    // Load existing extraction text for this asset
    const store = getStore()
    void store.extractions.findByAsset(asset.id).then((extraction) => {
      if (extraction) {
        ocrStore._updateState(asset.id, {
          status: 'done',
          progress: 100,
          textLength: extraction.textContent.length,
          method: extraction.method,
          textContent: extraction.textContent,
        })
        ocrTick++
        // Auto-trigger summary if text exists and LLM is available
        void autoSummarizeIfNeeded(asset.id)
      }
    })

    // Load existing transcription for audio assets
    if (asset.type === 'audio') {
      void store.transcriptions.findByAsset(asset.id).then((transcription) => {
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
          // Auto-trigger summary for transcribed audio if LLM is available
          void autoSummarizeIfNeeded(asset.id)
        }
      })
    }
  })

  // Reload analysis data when the selected asset changes
  $effect(() => {
    const asset = selectedAsset
    if (!asset) return
    void loadEntities()
    void loadTriples()
    // Load persisted LLM results for this asset so previous
    // asset-level results (summarize, correct_ocr, etc.) are visible.
    llmStore.loadPersistedResults(asset.id)
  })

  $effect(() => {
    // Reload all data when navigating to a different item.
    // Reading itemId here ensures the effect re-runs when the prop changes.
    const _id = itemId
    // Reset auto-summarize tracking for the new item
    autoSummarizeTriggered = new Set()
    void loadData()
  })

  onMount(() => {
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
          // After NER completes, reload entities for the current context
          if (job === 'ner' && status === 'done' && id === itemId) {
            loadEntities()
          }
          if (job === 'embed' && status === 'done' && id === itemId) {
            loadSimilarItems()
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

    llmStore.startListening().then(() => {
      llmStore.onChange(() => {
        llmTick++
      })
      // Load persisted LLM results for the item (legacy item-level results).
      // Asset-level results are loaded in the selectedAsset effect below.
      llmStore.loadPersistedResults(itemId)
    })

    // Check if the LLM engine (Gemma 4) is available for auto-summarize
    llmIsAvailable().then((available) => {
      llmAvailable = available
    }).catch(() => {
      llmAvailable = false
    })

    // Check if the LLM engine supports multimodal (vision) for OCRC
    llmIsMultimodal().then((multimodal) => {
      llmMultimodal = multimodal
    }).catch(() => {
      llmMultimodal = false
    })

    geoStore.startListening()
    return () => {
      if (metadataSaveTimer) clearTimeout(metadataSaveTimer)
    }
  })

  onDestroy(() => {
    ocrStore.stopListening()
    nlpStore.stopListening()
    transcriptionStore.stopListening()
    llmStore.stopListening()
    geoStore.stopListening()
    // Clear any pending debounce timers to avoid stale persist after unmount
    for (const timer of ocrPersistTimers.values()) {
      clearTimeout(timer)
    }
    ocrPersistTimers.clear()
    for (const timer of transPersistTimers.values()) {
      clearTimeout(timer)
    }
    transPersistTimers.clear()
    for (const timer of assetReanalysisTimers.values()) {
      clearTimeout(timer)
    }
    assetReanalysisTimers.clear()
    clearAnnotationSaveTimer()
    clearFtsSearchTimer()
    if (dragCleanup) dragCleanup()
  })
</script>

{#if loading}
  <p class="status">Loading...</p>
{:else if error && !item}
  <p class="error">{error}</p>
{:else if item}
  <div class="item-view" bind:this={itemViewEl} style="grid-template-columns: 1fr 6px {sidebarWidth}%">
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
            {editTool}
            canUndo={canUndo}
            onAnnotationsChange={handleAnnotationsChange}
            onSelectedAnnotationIdChange={handleSelectedAnnotationIdChange}
            onAnnotationToolChange={handleAnnotationToolChange}
            onAnnotationColorChange={handleAnnotationColorChange}
            onEditSelect={handleEditSelect}
            onEditToolChange={(tool) => { editTool = tool; if (tool !== 'none') annotationTool = 'select' }}
            onRotateLeft={handleRotateLeft}
            onRotateRight={handleRotateRight}
            onUndo={handleUndo}
            onDimensionsChange={(dims) => { imageNaturalW = dims.width; imageNaturalH = dims.height }}
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
        <div class="asset-pagination">
          <button
            class="pagination-btn"
            disabled={selectedAssetIndex <= 0}
            onclick={() => (selectedAssetIndex = Math.max(0, selectedAssetIndex - 1))}
            aria-label="Previous page"
          >
            ‹
          </button>
          <span class="pagination-info">
            {selectedAssetIndex + 1} / {assets.length}
          </span>
          <button
            class="pagination-btn"
            disabled={selectedAssetIndex >= assets.length - 1}
            onclick={() => (selectedAssetIndex = Math.min(assets.length - 1, selectedAssetIndex + 1))}
            aria-label="Next page"
          >
            ›
          </button>
        </div>
      {/if}
    </div>

    <div
      class="resize-handle"
      role="separator"
      aria-orientation="vertical"
      onpointerdown={onResizeHandlePointerDown}
    ></div>

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
        <h3>Tópicos</h3>
        <TopicEditor topics={itemTopics} suggestions={topicSuggestions} onchange={handleTopicsChange} />
      </section>

      <section class="section">
<h3>Add Note{#if assets.length > 1} · Page {selectedAssetIndex + 1}{/if}</h3>
          <NoteEditor onsave={handleSaveNote} />
        </section>

        <section class="section">
          <h3>Notes ({notes.length}){#if assets.length > 1} · Page {selectedAssetIndex + 1}{/if}</h3>
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

      {#if selectedAsset && selectedAsset.type !== 'audio'}
        {@const ocr = getOcrState(selectedAsset.id)}
        {@const busy = ocr.status === 'pending' || ocr.status === 'running'}
        <section class="section">
          <h3>Text Extraction{#if assets.length > 1} · Page {selectedAssetIndex + 1}{/if}</h3>
          <div class="ocr-item">
            <div class="ocr-item-header">
              <span class="ocr-filename">
                {assets.length > 1 && assets.every(a => a.type === 'image')
                  ? `Page ${selectedAssetIndex + 1}`
                  : (selectedAsset.path.split(/[/\\]/).pop() ?? 'Asset')}
              </span>
              <div class="ocr-btn-group">
                <button
                  class="ocr-btn ocr-btn--light"
                  disabled={busy}
                  onclick={() => handleExtractText(selectedAsset, 'light')}
                  title={busy ? 'Extraction in progress…' : 'Fast OCR (PaddleOCR/Tesseract)'}
                >
                  OCRL
                </button>
                <button
                  class="ocr-btn ocr-btn--high"
                  disabled={busy}
                  onclick={() => handleExtractText(selectedAsset, 'high')}
                  title={busy ? 'Extraction in progress…' : 'High-accuracy OCR (PaddleVL)'}
                >
                  OCRH
                </button>
                {#if llmAvailable && !ocrCorrectedAssets.has(selectedAsset.id)}
                  <button
                    class="ocr-btn ocr-btn--correct"
                    disabled={getLlmState().status === 'running' || ocr.status !== 'done'}
                    onclick={handleLlmCorrectOcr}
                    title={!llmAvailable ? 'Gemma 4 not available' : ocr.status !== 'done' ? 'Extract text first' : llmMultimodal ? 'LLM OCR correction with image + text (Gemma 4)' : 'LLM OCR correction, text only (Gemma 4)'}
                  >
                    OCRC{#if llmMultimodal} 👁{/if}
                  </button>
                {/if}
              </div>
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
              {@const editedText = (() => { void ocrTick; return ocrEditedText.get(selectedAsset.id) ?? ocr.textContent ?? '' })()}
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
                    ocrEditedText.set(selectedAsset.id, val)
                    ocrStore.setTextContent(selectedAsset.id, val)
                    schedulePersist(selectedAsset.id, val)
                    ocrTick++
                  }}>{editedText}</textarea
                >
              </details>
            {/if}
          </div>
        </section>
      {/if}

      {#if selectedAsset && selectedAsset.type === 'audio'}
        {@const ts = getTranscriptionState(selectedAsset.id)}
        {@const busy = ts.status === 'pending' || ts.status === 'running'}
        <section class="section">
          <h3>Audio Transcription{#if assets.length > 1} · Page {selectedAssetIndex + 1}{/if}</h3>
          <div class="ocr-item">
            <div class="ocr-item-header">
              <span class="ocr-filename">&#x1f50a; {selectedAsset.path.split(/[/\\]/).pop() ?? 'Audio'}</span>
              <button
                class="ocr-btn"
                disabled={busy}
                onclick={() => handleTranscribeAudio(selectedAsset)}
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
              {@const editedText = transEditedText.get(selectedAsset.id) ?? ts.text ?? ''}
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
                    transEditedText.set(selectedAsset.id, val)
                    transcriptionStore.setTextContent(selectedAsset.id, val)
                    scheduleTranscriptionPersist(selectedAsset.id, val)
                    transcriptionTick++
                  }}>{editedText}</textarea
                >
              </details>
            {/if}
          </div>
        </section>
      {/if}

      {#if selectedAsset}
        {@const currentSummary = (() => { void summaryTick; return summaryTexts.get(selectedAsset.id) ?? null })()}
        {@const isSummarizing = getLlmState().status === 'running' && getLlmState().activeJob === 'summarize'}
        {#if currentSummary || isSummarizing}
          <section class="section">
            <h3>Resumen{#if assets.length > 1} · Page {selectedAssetIndex + 1}{/if}</h3>
            {#if isSummarizing}
              <p class="summary-status">Generando resumen…</p>
            {:else if currentSummary}
              <div class="summary-result">
                <pre class="summary-text">{currentSummary}</pre>
              </div>
            {/if}
          </section>
        {/if}
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
                loadFtsStats()
                loadGeoMarkers()
              }
            }}
          >
            Analysis {analysisOpen ? '▲' : '▼'}
          </button>

          {#if analysisOpen}
            {@const nlp = getNlpState()}
            <div class="analysis-panel">
              <div class="fts-search-section">
                <h4>Search by Similar Text (FTS)</h4>
                <input
                  class="fts-search-input"
                  type="search"
                  placeholder="Escribí para buscar..."
                  value={ftsQuery}
                  oninput={handleFtsInput}
                  onkeydown={handleFtsKeydown}
                />

                {#if ftsSearchError}
                  <p class="ocr-error">{ftsSearchError}</p>
                {:else if ftsSearching}
                  <p class="empty-text">Buscando textos similares...</p>
                {:else if ftsQuery.trim().length === 0}
                  <p class="empty-text">Ingresá un término para ver resultados.</p>
                {:else if ftsResults.length === 0}
                  <p class="empty-text">No hay resultados para esa búsqueda.</p>
                {:else}
                  <ul class="similar-list">
                    {#each ftsResults as result (result.itemId)}
                      <li class="similar-item">
                        <button class="similar-item-btn" onclick={() => navigateToSimilarItem(result)}>
                          <span class="similar-title">
                            {#each splitHighlightedSegments(result.title || result.itemId, ftsQuery) as segment, i (`${result.itemId}-seg-${i}-${segment.text}`)}
                              {#if segment.isMatch}
                                <mark class="fts-match">{segment.text}</mark>
                              {:else}
                                {segment.text}
                              {/if}
                            {/each}
                          </span>
                          <span class="similar-score">rank {result.rank.toFixed(3)}</span>
                        </button>
                      </li>
                    {/each}
                  </ul>
                {/if}

                {#if isDev}
                  <details class="fts-debug-panel">
                    <summary>FTS Debug (dev only)</summary>

                    <div class="fts-debug-grid">
                      <div class="fts-debug-row">
                        <span class="fts-debug-label">Indexed rows</span>
                        <code>{ftsIndexedRows ?? 'unknown'}</code>
                      </div>
                      <div class="fts-debug-row">
                        <span class="fts-debug-label">Raw query</span>
                        <code>{ftsDebug?.rawQuery ?? (ftsQuery.trim() || '—')}</code>
                      </div>
                      <div class="fts-debug-row">
                        <span class="fts-debug-label">Sanitized</span>
                        <code>{ftsDebug?.sanitizedQuery || '—'}</code>
                      </div>
                      <div class="fts-debug-row">
                        <span class="fts-debug-label">Strategy</span>
                        <code>{ftsDebug?.strategy ?? '—'}</code>
                      </div>
                      <div class="fts-debug-row">
                        <span class="fts-debug-label">DB matches</span>
                        <code>{ftsDebug?.matchCount ?? 0}</code>
                      </div>
                      <div class="fts-debug-row">
                        <span class="fts-debug-label">Hydrated items</span>
                        <code>{ftsDebug?.hydratedCount ?? 0}</code>
                      </div>
                      <div class="fts-debug-row fts-debug-row--stacked">
                        <span class="fts-debug-label">Result IDs</span>
                        <code>{ftsDebug?.resultIds.join(', ') || '—'}</code>
                      </div>
                    </div>
                  </details>
                {/if}
              </div>

              <div class="nlp-actions">
                <button
                  class="nlp-btn"
                  disabled={nlp.fts === 'pending' || nlp.fts === 'running'}
                  onclick={handleIndexFts}
                >
                  INDEX <span class="nlp-badge nlp-badge--{nlp.fts}">{nlp.fts}</span>
                </button>

                <button
                  class="nlp-btn"
                  disabled={nlp.embed === 'pending' || nlp.embed === 'running'}
                  onclick={handleEmbedItem}
                >
                  EMBED <span class="nlp-badge nlp-badge--{nlp.embed}">{nlp.embed}</span>
                </button>

                <button
                  class="nlp-btn"
                  disabled={nlp.ner === 'pending' || nlp.ner === 'running'}
                  onclick={handleExtractEntities}
                >
                  NER <span class="nlp-badge nlp-badge--{nlp.ner}">{nlp.ner}</span>
                </button>

                <button
                  class="nlp-btn"
                  disabled={nlp.triples === 'pending' || nlp.triples === 'running'}
                  onclick={handleExtractTriples}
                >
                  TRIPLET <span class="nlp-badge nlp-badge--{nlp.triples}">{nlp.triples}</span>
                </button>
              </div>

              {#if nlp.errors?.embed}
                <p class="ocr-error">Embedding error: {nlp.errors.embed}</p>
              {/if}

              <!-- LLM section (Gemma 4) -->
              <div class="llm-section">
                <h4>IA Generativa (Gemma 4){#if assets.length > 1} · Page {selectedAssetIndex + 1}{/if}</h4>

                <div class="nlp-actions">
                  {#if !ocrCorrectedAssets.has(selectedAsset?.id ?? '')}
                    <button
                      class="nlp-btn llm-btn"
                      disabled={getLlmState().status === 'running'}
                      onclick={handleLlmCorrectOcr}
                    >
                      Corregir OCR
                      {#if getLlmState().status === 'running' && getLlmState().activeJob === 'correct_ocr'}
                        <span class="nlp-badge nlp-badge--running">procesando...</span>
                      {/if}
                    </button>
                  {/if}

                  <button
                    class="nlp-btn llm-btn"
                    disabled={getLlmState().status === 'running'}
                    onclick={handleLlmExtractEntities}
                  >
                    Entidades (LLM)
                    {#if getLlmState().status === 'running' && getLlmState().activeJob === 'extract_entities'}
                      <span class="nlp-badge nlp-badge--running">procesando...</span>
                    {/if}
                  </button>

                  <button
                    class="nlp-btn llm-btn"
                    disabled={getLlmState().status === 'running'}
                    onclick={handleLlmExtractTriples}
                  >
                    Triples (LLM)
                    {#if getLlmState().status === 'running' && getLlmState().activeJob === 'extract_triples'}
                      <span class="nlp-badge nlp-badge--running">procesando...</span>
                    {/if}
                  </button>
                </div>

                {#if getLlmState().error}
                  <p class="ocr-error">{getLlmState().error}</p>
                {/if}

                {#if getLlmState().result && !ocrCorrectedAssets.has(selectedAsset?.id ?? itemId)}
                  <div class="llm-result">
                    <h5>Resultado LLM{#if assets.length > 1} · Page {selectedAssetIndex + 1}{/if}</h5>
                    <pre class="llm-result-text">{getLlmState().result}</pre>
                  </div>
                {/if}
              </div>

              <!-- Map section (OpenStreetMap) -->
              <div class="geo-section">
                <MapViewer markers={geoMarkers} height="280px" />
              </div>

              <div class="entities-section">
                <h4>Entities</h4>
                <EntityViewer {entities} onentityclick={startEditingEntity} />

                <div class="entity-editor">
                  <h5>Manual Entities</h5>
                  <p class="entity-editor__hint">Click an entity tag above to edit or delete it.</p>

                  <div class="entity-editor__create">
                    <select
                      value={newEntityType}
                      aria-label="New entity type"
                      onchange={(event) => {
                        newEntityType = event.currentTarget.value as EditableEntityType
                      }}
                    >
                      {#each EDITABLE_ENTITY_TYPES as type}
                        <option value={type}>{type.toUpperCase()}</option>
                      {/each}
                    </select>
                    <input
                      bind:value={newEntityValue}
                      type="text"
                      placeholder="Add entity manually"
                      aria-label="New entity value"
                      onkeydown={(event) => event.key === 'Enter' && void handleCreateEntity()}
                    />
                    <button type="button" class="nlp-btn" onclick={handleCreateEntity}>Add</button>
                  </div>

                  {#if entityActionError}
                    <p class="error">{entityActionError}</p>
                  {/if}

                </div>

                {#if entityEditorOpen && editingEntityId}
                  <div class="entity-modal" role="dialog" aria-modal="true" aria-label="Edit entity">
                    <button
                      type="button"
                      class="entity-modal__backdrop"
                      aria-label="Close entity editor"
                      onclick={cancelEditingEntity}
                    ></button>

                    <div class="entity-modal__panel">
                      <div class="entity-modal__header">
                        <h5>Edit entity</h5>
                        <button type="button" class="entity-modal__close" onclick={cancelEditingEntity}>×</button>
                      </div>

                      <div class="entity-modal__body">
                        <label class="entity-modal__field">
                          <span>Type</span>
                          <select
                            value={editingEntityType}
                            aria-label="Edit entity type"
                            onchange={(event) => {
                              editingEntityType = event.currentTarget.value as EditableEntityType
                            }}
                          >
                            {#each EDITABLE_ENTITY_TYPES as type}
                              <option value={type}>{type.toUpperCase()}</option>
                            {/each}
                          </select>
                        </label>

                        <label class="entity-modal__field">
                          <span>Value</span>
                          <input
                            bind:value={editingEntityValue}
                            type="text"
                            aria-label="Edit entity value"
                            onkeydown={(event) => event.key === 'Enter' && editingEntityId && void handleSaveEntity(editingEntityId)}
                          />
                        </label>
                      </div>

                      <div class="entity-modal__actions">
                        <button
                          type="button"
                          class="nlp-btn entity-modal__danger"
                          onclick={() => editingEntityId && handleDeleteEntity(editingEntityId)}
                        >
                          Delete
                        </button>
                        <div class="entity-modal__actions-right">
                          <button type="button" class="nlp-btn" onclick={cancelEditingEntity}>Cancel</button>
                          <button
                            type="button"
                            class="nlp-btn"
                            onclick={() => editingEntityId && handleSaveEntity(editingEntityId)}
                          >
                            Save
                          </button>
                        </div>
                      </div>
                    </div>
                  </div>
                {/if}
              </div>

              <div class="triples-section">
                <h4>Semantic Triples (S|P|O){#if assets.length > 1} · Page {selectedAssetIndex + 1}{/if}</h4>
                {#if triples.length === 0}
                  <p class="empty-text">No triples extracted yet{#if assets.length > 1} for this page{/if}.</p>
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

              {#if similarItems.length > 0}
                <div class="similar-section">
                  <h4>Similar Items{#if assets.length > 1} (by page {selectedAssetIndex + 1}){/if}</h4>
                  <ul class="similar-list">
                    {#each similarItems.slice(0, 5) as item (item.itemId)}
                      <li class="similar-item">
                        <button
                          class="similar-item-btn"
                          onclick={() => navigateToSimilarItem(item)}
                        >
                          <span class="similar-title">{item.title || item.itemId}</span>
                          <span class="similar-score">({(item.similarity * 100).toFixed(1)}%)</span>
                        </button>
                      </li>
                    {/each}
                  </ul>
                </div>
              {:else}
                <div class="similar-section">
                  <h4>Similar Items{#if assets.length > 1} (by page {selectedAssetIndex + 1}){/if}</h4>
                  <p class="empty-text">
                    No embeddings yet. Generate embeddings to find similar items.
                  </p>
                </div>
              {/if}
            </div>
          {/if}
        </section>
      {/if}
    </div>
  </div>
{/if}

<style>
  .item-view {
    display: grid;
    /* grid-template-columns set via inline style */
    gap: 0;
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
  }
  .resize-handle {
    width: 6px;
    position: relative;
    cursor: col-resize;
    z-index: 1;
  }
  .resize-handle::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    transform: translateX(-50%);
    width: 1px;
    background-color: var(--color-border);
    transition: background-color 0.15s ease, width 0.15s ease;
  }
  .resize-handle:hover::before {
    background-color: var(--color-text-muted, var(--color-border));
    width: 2px;
  }
  :global(body.no-select),
  :global(body.no-select *) {
    cursor: col-resize !important;
    user-select: none !important;
    -webkit-user-select: none !important;
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
  .asset-pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    padding: var(--space-2) 0;
  }
  .pagination-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-primary);
    font-size: var(--font-size-md);
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .pagination-btn:hover:not(:disabled) {
    border-color: var(--color-primary);
    background: var(--color-primary-subtle);
  }
  .pagination-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .pagination-info {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    min-width: 60px;
    text-align: center;
    font-variant-numeric: tabular-nums;
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

  /* ── Summary (auto-generated by Gemma 4) ── */
  .summary-result {
    margin-top: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }

  .summary-status {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    font-style: italic;
  }

  .summary-text {
    margin: 0;
    font-size: var(--font-size-sm);
    font-family: var(--font-sans);
    white-space: pre-wrap;
    word-wrap: break-word;
    max-height: 300px;
    overflow-y: auto;
    line-height: 1.6;
    color: var(--color-text-secondary);
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
  .ocr-btn-group {
    display: flex;
    gap: var(--space-1);
    flex-shrink: 0;
  }
  .ocr-btn--light {
    border-color: var(--color-success, #16a34a);
    background: var(--color-success-subtle, #f0fdf4);
    color: var(--color-success, #16a34a);
  }
  .ocr-btn--light:disabled {
    border-color: var(--color-border);
    background: var(--color-surface);
    color: var(--color-text-muted);
  }
  .ocr-btn--high {
    border-color: var(--color-info, #3b82f6);
    background: var(--color-info-subtle, #eff6ff);
    color: var(--color-info, #3b82f6);
  }
  .ocr-btn--high:disabled {
    border-color: var(--color-border);
    background: var(--color-surface);
    color: var(--color-text-muted);
  }
  .ocr-btn--correct {
    border-color: var(--color-accent, #6366f1);
    background: color-mix(in srgb, var(--color-accent, #6366f1) 10%, transparent);
    color: var(--color-accent, #6366f1);
  }
  .ocr-btn--correct:disabled {
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
    overflow: hidden;
  }

  .nlp-actions {
    display: flex;
    flex-direction: row;
    gap: var(--space-2);
  }

  .nlp-btn {
    display: inline-flex;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: var(--space-1);
    flex: 1 1 25%;
    min-width: 0;
    padding: var(--space-2) var(--space-1);
    font-size: var(--font-size-xs);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    cursor: pointer;
    color: var(--color-text-primary);
    font-family: var(--font-sans);
    text-align: center;
    white-space: nowrap;
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
  .fts-search-section,
  .triples-section,
  .similar-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .entities-section h4,
  .fts-search-section h4,
  .similar-section h4 {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-secondary);
  }

  .entity-editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-3);
    min-width: 0;
  }

  .entity-editor h5 {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .entity-editor__hint {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .entity-editor__create {
    display: grid;
    grid-template-columns: 35fr 50fr 15fr;
    gap: var(--space-2);
    align-items: center;
    padding-bottom: var(--space-2);
    min-width: 0;
  }

  .entity-editor__create select {
    min-width: 0;
    padding: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-primary);
    font-size: var(--font-size-xs);
  }

  .entity-editor__create input {
    min-width: 0;
    padding: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .entity-editor__create .nlp-btn {
    width: 100%;
    flex-direction: row;
    justify-content: center;
    font-size: var(--font-size-sm);
    padding: var(--space-2) var(--space-3);
  }

  .entity-modal {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .entity-modal__backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: rgb(0 0 0 / 0.45);
  }

  .entity-modal__panel {
    position: relative;
    width: min(520px, calc(100vw - 2rem));
    background: var(--color-surface-raised, var(--color-surface));
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    box-shadow: 0 20px 40px rgb(0 0 0 / 0.18);
  }

  .entity-modal__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .entity-modal__header h5 {
    margin: 0;
    font-size: var(--font-size-sm);
  }

  .entity-modal__close {
    border: none;
    background: transparent;
    color: var(--color-text-secondary);
    font-size: 24px;
    line-height: 1;
    cursor: pointer;
  }

  .entity-modal__body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .entity-modal__field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }

  .entity-modal__actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .entity-modal__actions-right {
    display: flex;
    gap: var(--space-2);
  }

  .entity-modal__danger {
    color: var(--color-danger, #dc2626);
  }

  .fts-search-input {
    width: 100%;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    padding: var(--space-2) var(--space-3);
    outline: none;
    font-family: var(--font-sans);
  }

  .fts-search-input:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 20%, transparent);
  }

  .fts-match {
    background: color-mix(in srgb, var(--color-warning, #f59e0b) 30%, transparent);
    color: var(--color-text-primary);
    border-radius: 2px;
    padding: 0 1px;
  }

  .fts-debug-panel {
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-sm);
    padding: var(--space-2);
    background: var(--color-surface-raised);
  }

  .fts-debug-panel summary {
    cursor: pointer;
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    font-weight: var(--font-weight-medium);
  }

  .fts-debug-grid {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .fts-debug-row {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--space-2);
    font-size: var(--font-size-xs);
  }

  .fts-debug-row--stacked {
    flex-direction: column;
  }

  .fts-debug-label {
    color: var(--color-text-secondary);
    min-width: 90px;
  }

  .fts-debug-row code {
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--color-text-primary);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 4px;
    padding: 2px 6px;
    flex: 1;
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
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    transition:
      background 0.15s ease,
      border-color 0.15s ease;
  }

  .similar-item:hover {
    background: var(--color-surface-elevated);
    border-color: var(--color-accent);
  }

  .similar-item-btn {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    background: none;
    border: none;
    color: inherit;
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    cursor: pointer;
    text-align: left;
  }

  .similar-item-btn:hover {
    background: transparent;
  }

  .similar-score {
    font-size: var(--font-size-xs);
    color: var(--color-text-tertiary, var(--color-text-secondary));
    opacity: 0.7;
    white-space: nowrap;
    margin-left: var(--space-2);
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
  /* Geo Section */
  .geo-section {
    margin-top: var(--space-4);
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-border);
  }

  /* LLM Section */
  .llm-section {
    margin-top: var(--space-4);
    padding-top: var(--space-4);
    border-top: 1px solid var(--color-border);
  }

  .llm-section h4 {
    margin: 0 0 var(--space-2) 0;
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .llm-btn {
    border-left: 3px solid var(--color-accent, #6366f1);
  }

  .llm-result {
    margin-top: var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }

  .llm-result h5 {
    margin: 0 0 var(--space-2) 0;
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
  }

  .llm-result-text {
    margin: 0;
    font-size: var(--font-size-sm);
    white-space: pre-wrap;
    word-wrap: break-word;
    max-height: 300px;
    overflow-y: auto;
    line-height: 1.5;
  }
</style>
