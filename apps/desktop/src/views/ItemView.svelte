<script lang="ts">
  import { getStore } from '$lib/db'
  import { getAssetUrl } from '$lib/file-import'
  import { OcrStore, extractText } from '$lib/ocr'
  import {
    NlpStore,
    indexFts,
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
  import type { Item, Asset, Note } from '@entropia/store'
  import type { Entity } from '@entropia/ui'

  let { itemId, collectionId }: { itemId: string; collectionId: string } = $props()

  let item = $state<Item | null>(null)
  let assets = $state<Asset[]>([])
  let notes = $state<Note[]>([])
  let loading = $state(true)
  let error = $state<string | null>(null)
  let selectedAssetIndex = $state(0)
  let savingMetadata = $state(false)

  // OCR state — plain TS class, updated via Tauri events
  const ocrStore = new OcrStore({
    onComplete: (assetId) => {
      // After OCR extraction completes, auto-trigger FTS indexing for this item
      void indexFts(itemId).catch(() => {
        // Non-fatal: FTS indexing failure doesn't block the UI
      })
      void assetId // suppress unused warning (assetId belongs to an asset of itemId)
    },
    fetchText: async (assetId: string) => {
      const store = getStore()
      const extraction = await store.extractions.findByAsset(assetId)
      return extraction?.textContent ?? ''
    },
  })
      void assetId // suppress unused warning (assetId belongs to an asset of itemId)
    },
    fetchText: async (assetId: string) => {
      const store = getStore()
      const extraction = await store.extractions.findByAsset(assetId)
      return extraction?.textContent ?? ''
    },
  })
  // Reactive tick counter: incremented on every OCR event to force Svelte re-evaluation
  let ocrTick = $state(0)

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

  let viewerType = $derived<'image' | 'pdf'>(selectedAsset?.type === 'pdf' ? 'pdf' : 'image')

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

  function getOcrState(assetId: string) {
    // Depend on ocrTick to trigger Svelte reactivity when events arrive
    void ocrTick
    return ocrStore.getState(assetId)
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
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load item'
    } finally {
      loading = false
    }
  }

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

    return () => {
      if (metadataSaveTimer) clearTimeout(metadataSaveTimer)
    }
  })

  onDestroy(() => {
    ocrStore.stopListening()
    nlpStore.stopListening()
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
        <DocumentViewer path={selectedAsset.path} assetUrl={viewerSrc} type={viewerType} />
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
                <p class="note-content">{note.content}</p>
                <p class="note-date">{new Date(note.createdAt).toLocaleString()}</p>
                <Button variant="ghost" size="sm" onclick={() => handleDeleteNote(note.id)}>
                  Delete
                </Button>
              </Card>
            {/each}
          </div>
        {/if}
      </section>

      <section class="section">
        <h3>Text Extraction</h3>
        {#if assets.length === 0}
          <p class="empty-text">No assets to extract text from.</p>
        {:else}
          <div class="ocr-list">
            {#each assets as asset (asset.id)}
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
                  <details class="ocr-result">
                    <summary>
                      Extracted text
                      <span class="ocr-meta">
                        via {ocr.method ?? 'unknown'} · {ocr.textLength ?? 0} chars
                      </span>
                    </summary>
                    <pre class="ocr-result-body">
                      {#if ocr.textContent}{ocr.textContent}{:else}Text extracted successfully ({ocr.textLength ??
                          0} characters via {ocr.method ?? 'unknown'}).{/if}
                    </pre>
                  </details>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </section>

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
