<script lang="ts">
  import { getStore } from '$lib/db'
  import { navigation } from '$lib/navigation'
  import { pickFiles, classifyFiles, importSingleFile, type ImportedFile } from '$lib/file-import'
  import { getAssetUrl, deleteAssetFile } from '$lib/file-import'
  import { exportCollectionById } from '$lib/export'
  import { ItemCard, SearchBar, Button } from '@entropia/ui'
  import { onMount, onDestroy } from 'svelte'
  import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview'
  import type { Item, Asset } from '@entropia/store'

  let { collectionId }: { collectionId: string } = $props()

  let items = $state<Item[]>([])
  let searchQuery = $state('')
  let loading = $state(true)
  let error = $state<string | null>(null)
  let importing = $state(false)
  let exporting = $state(false)
  let importNotice = $state<string | null>(null)
  let dragActive = $state(false)
  let unlistenDragDrop: (() => void) | null = null

  // Cache itemId → { assetCount, thumbnailUrl, primaryAssetId, primaryAssetPath }
  let itemAssetMeta = $state<
    Map<
      string,
      {
        assetCount: number
        thumbnailUrl: string | null
        primaryAssetId: string | null
        primaryAssetPath: string | null
      }
    >
  >(new Map())

  // Delete confirmation state
  let showDeleteConfirm = $state(false)
  let pendingDeleteAssetId = $state<string | null>(null)
  let pendingDeleteItemId = $state<string | null>(null)
  let pendingDeleteFilename = $state<string | null>(null)
  let deleting = $state(false)
  let deleteError = $state<string | null>(null)

  function getItemAssetMeta(itemId: string): {
    assetCount: number
    thumbnailUrl: string | null
    primaryAssetId: string | null
    primaryAssetPath: string | null
  } {
    return (
      itemAssetMeta.get(itemId) ?? {
        assetCount: 0,
        thumbnailUrl: null,
        primaryAssetId: null,
        primaryAssetPath: null,
      }
    )
  }

  async function loadItemAssets(itemIds: string[]) {
    if (itemIds.length === 0) return
    const store = getStore()
    const newMeta = new Map(itemAssetMeta)
    for (const itemId of itemIds) {
      try {
        const assets: Asset[] = await store.assets.findByItem(itemId)
        const imageAsset = assets.find((a) => a.type === 'image')
        // Use first asset as thumbnail if no image asset found (PDFs get preview too)
        const thumbAsset = imageAsset ?? assets[0]
        newMeta.set(itemId, {
          assetCount: assets.length,
          thumbnailUrl: thumbAsset ? getAssetUrl(thumbAsset.path) : null,
          primaryAssetId: thumbAsset?.id ?? null,
          primaryAssetPath: thumbAsset?.path ?? null,
        })
      } catch (e) {
        console.error('[CollectionView] Failed to load assets for item', itemId, e)
        // Non-fatal: item card shows placeholder
      }
    }
    itemAssetMeta = newMeta
  }

  let filtered = $derived(
    searchQuery ? items : items // search is handled by repo call below
  )

  async function loadItems() {
    try {
      loading = true
      error = null
      const store = getStore()
      items = searchQuery
        ? await store.items.searchByText(collectionId, searchQuery)
        : await store.items.findByCollection(collectionId)
      // Load asset metadata (count + thumbnail) for each item
      await loadItemAssets(items.map((i) => i.id))
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load items'
    } finally {
      loading = false
    }
  }

  async function handleSearch(query: string) {
    searchQuery = query
    await loadItems()
  }

  async function handleClearSearch() {
    searchQuery = ''
    await loadItems()
  }

  async function finalizeImportedItem(itemId: string, imported: ImportedFile) {
    const store = getStore()

    await store.items.update(itemId, {
      title: imported.originalName.replace(/\.[^.]+$/, ''),
    })

    await store.assets.create({
      itemId,
      path: imported.destPath,
      type: imported.type,
      size: imported.size,
    })
  }

  function getErrorDetails(e: unknown): string {
    return e instanceof Error ? e.message : String(e)
  }

  function formatImportStageError(baseMessage: string, stage: string, e: unknown): string {
    return `${baseMessage} (${stage}): ${getErrorDetails(e)}`
  }

  async function handleImport() {
    importing = true
    error = null
    importNotice = null
    const store = getStore()

    // Step 1: Open file picker — get raw paths BEFORE creating any items
    let selectedPaths: string[]
    try {
      selectedPaths = await pickFiles()
    } catch (e) {
      error = formatImportStageError('Failed to import files', 'selecting files', e)
      importing = false
      return
    }

    if (selectedPaths.length === 0) {
      importing = false
      return
    }

    // Step 2: Classify files (no DB or FS side effects yet)
    const { classified, rejected } = classifyFiles(selectedPaths)

    if (classified.length === 0) {
      if (rejected.length > 0) {
        error = `Unsupported format: ${rejected.join(', ')}`
      }
      importing = false
      return
    }

    // Step 3: Create one item per file, copy file, create asset
    const createdItemIds: string[] = []
    let importError: string | null = null

    for (const file of classified) {
      let itemId: string
      try {
        const item = await store.items.create({
          title: file.name.replace(/\.[^.]+$/, ''),
          collectionId,
          metadata: null,
        })
        itemId = item.id
      } catch (e) {
        importError = formatImportStageError('Failed to import files', 'creating item', e)
        break
      }

      try {
        const imported = await importSingleFile(file.sourcePath, collectionId, itemId)
        await finalizeImportedItem(itemId, imported)
        createdItemIds.push(itemId)
      } catch (e) {
        // Clean up the item if file copy failed
        try {
          await store.items.delete(itemId)
        } catch {
          // ignore cleanup errors
        }
        importError = formatImportStageError('Failed to import files', `importing ${file.name}`, e)
        break
      }
    }

    await loadItems()

    // Navigate to the last created item
    if (createdItemIds.length > 0) {
      const lastItemId = createdItemIds[createdItemIds.length - 1]!
      const lastFile = classified[classified.length - 1]!
      navigation.navigate({
        name: 'item',
        collectionId,
        collectionName:
          navigation.current.name === 'collection'
            ? (navigation.current as { collectionName: string }).collectionName
            : '',
        itemId: lastItemId,
        itemTitle: lastFile.name.replace(/\.[^.]+$/, ''),
      })
    }

    // Build notice for partial imports
    const noticeParts: string[] = []
    if (rejected.length > 0) {
      noticeParts.push(`${rejected.length} unsupported skipped: ${rejected.join(', ')}`)
    }
    if (importError) {
      noticeParts.push(`error: ${importError}`)
    }
    importNotice =
      noticeParts.length > 0
        ? `${createdItemIds.length} imported (${noticeParts.join(' · ')})`
        : null

    if (importError && createdItemIds.length === 0) {
      error = importError
    }

    importing = false
  }

  async function handleImportFromDroppedPaths(paths: string[]) {
    importing = true
    error = null
    importNotice = null
    const store = getStore()

    // Step 1: Classify dropped files (no DB or FS side effects yet)
    const { classified, rejected } = classifyFiles(paths)

    if (classified.length === 0) {
      if (rejected.length > 0) {
        error = `Unsupported format: ${rejected.join(', ')}`
      }
      importing = false
      dragActive = false
      return
    }

    // Step 2: Create one item per file, copy file, create asset
    const createdItemIds: string[] = []
    let importError: string | null = null

    for (const file of classified) {
      let itemId: string
      try {
        const item = await store.items.create({
          title: file.name.replace(/\.[^.]+$/, ''),
          collectionId,
          metadata: null,
        })
        itemId = item.id
      } catch (e) {
        importError = formatImportStageError('Failed to import dropped files', 'creating item', e)
        break
      }

      try {
        const imported = await importSingleFile(file.sourcePath, collectionId, itemId)
        await finalizeImportedItem(itemId, imported)
        createdItemIds.push(itemId)
      } catch (e) {
        try {
          await store.items.delete(itemId)
        } catch {
          // ignore cleanup errors
        }
        importError = formatImportStageError(
          'Failed to import dropped files',
          `importing ${file.name}`,
          e
        )
        break
      }
    }

    await loadItems()

    // Navigate to the last created item
    if (createdItemIds.length > 0) {
      const lastItemId = createdItemIds[createdItemIds.length - 1]!
      const lastFile = classified[classified.length - 1]!
      navigation.navigate({
        name: 'item',
        collectionId,
        collectionName:
          navigation.current.name === 'collection'
            ? (navigation.current as { collectionName: string }).collectionName
            : '',
        itemId: lastItemId,
        itemTitle: lastFile.name.replace(/\.[^.]+$/, ''),
      })
    }

    // Build notice
    const noticeParts: string[] = []
    if (rejected.length > 0) {
      noticeParts.push(`${rejected.length} unsupported skipped: ${rejected.join(', ')}`)
    }
    if (importError) {
      noticeParts.push(`error: ${importError}`)
    }
    importNotice =
      noticeParts.length > 0
        ? `${createdItemIds.length} imported (${noticeParts.join(' · ')})`
        : null

    if (importError && createdItemIds.length === 0) {
      error = importError
    }

    importing = false
    dragActive = false
  }

  async function handleExportJson() {
    try {
      exporting = true
      error = null
      const store = getStore()
      await exportCollectionById(store, collectionId)
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to export collection'
    } finally {
      exporting = false
    }
  }

  // ---------------------------------------------------------------------------
  // Asset deletion flow
  // ---------------------------------------------------------------------------

  /**
   * Extract just the filename from a full native path.
   */
  function extractFilename(nativePath: string): string {
    return nativePath.split(/[/\\]/).pop() ?? 'unknown file'
  }

  /**
   * Open the delete confirmation dialog for the primary asset of an item.
   */
  function handleDeleteClick(itemId: string) {
    const meta = getItemAssetMeta(itemId)
    if (!meta.primaryAssetId || !meta.primaryAssetPath) {
      error = 'No asset found to delete for this item.'
      return
    }
    pendingDeleteAssetId = meta.primaryAssetId
    pendingDeleteItemId = itemId
    pendingDeleteFilename = extractFilename(meta.primaryAssetPath)
    showDeleteConfirm = true
    deleteError = null
  }

  /**
   * Cancel the delete confirmation dialog.
   */
  function handleDeleteCancel() {
    showDeleteConfirm = false
    pendingDeleteAssetId = null
    pendingDeleteItemId = null
    pendingDeleteFilename = null
    deleteError = null
  }

  /**
   * Execute the asset deletion: remove file from FS, then cascade delete from DB.
   * If the deleted asset is the item's last one, the entire item is removed
   * (with all associated metadata) and the card disappears from the grid.
   *
   * Resilient: DB errors do NOT block file deletion or UI update.
   * The file is always removed and the UI is always refreshed.
   */
  async function handleDeleteConfirm() {
    if (!pendingDeleteAssetId || !pendingDeleteItemId) return

    deleting = true
    deleteError = null

    const store = getStore()
    const meta = getItemAssetMeta(pendingDeleteItemId)
    const assetPath = meta.primaryAssetPath
    const isLastAsset = meta.assetCount <= 1

    // Step 1: Always delete the file from filesystem (ENOENT is OK)
    // Use the cached path — do NOT depend on a DB lookup
    if (assetPath) {
      try {
        await deleteAssetFile(assetPath)
      } catch (e) {
        // Log but continue — file deletion should not block UI update
        console.warn('[CollectionView] File deletion warning:', e)
      }
    }

    // Step 2: Try DB cleanup — non-blocking
    try {
      if (isLastAsset) {
        await store.items.deleteWithCascade(pendingDeleteItemId)
      } else {
        await store.assets.deleteWithCascade(pendingDeleteAssetId)
      }
    } catch (e) {
      // Log DB error but do NOT block UI update
      const message = e instanceof Error ? e.message : String(e)
      console.error('[CollectionView] DB cleanup failed (UI will still update):', message)
      // Show a subtle warning in the error field but still close the dialog
      deleteError = `File removed. DB cleanup failed: ${message}`
    }

    // Step 3: Always update UI — remove card or refresh meta
    if (isLastAsset) {
      items = items.filter((i) => i.id !== pendingDeleteItemId)
      const newMeta = new Map(itemAssetMeta)
      newMeta.delete(pendingDeleteItemId)
      itemAssetMeta = newMeta
    } else {
      await loadItemAssets([pendingDeleteItemId])
    }

    // Step 4: Close dialog (even if DB failed — file is gone, UI is updated)
    handleDeleteCancel()
    deleting = false
  }

  onMount(() => {
    loadItems()

    getCurrentWebview()
      .onDragDropEvent((event: { payload: DragDropEvent }) => {
        if (event.payload.type === 'enter') {
          dragActive = true
          return
        }

        if (event.payload.type === 'over') {
          dragActive = true
          return
        }

        if (event.payload.type === 'leave') {
          dragActive = false
          return
        }

        if (event.payload.type !== 'drop') {
          return
        }

        dragActive = false
        void handleImportFromDroppedPaths(event.payload.paths)
      })
      .then((unlisten: () => void) => {
        unlistenDragDrop = unlisten
      })
  })

  onDestroy(() => {
    unlistenDragDrop?.()
  })
</script>

<div class="collection-view" class:drag-active={dragActive}>
  <div class="toolbar">
    <SearchBar placeholder="Search items..." onsearch={handleSearch} onclear={handleClearSearch} />
    <Button variant="primary" onclick={handleImport} disabled={importing}>
      {importing ? 'Importing...' : '+ Import Document'}
    </Button>
    <Button variant="secondary" onclick={handleExportJson} disabled={exporting}>
      {exporting ? 'Exporting...' : 'Export JSON'}
    </Button>
  </div>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if importNotice}
    <p class="notice">{importNotice}</p>
  {/if}

  {#if dragActive}
    <div class="drop-hint">Drop files to import into this collection</div>
  {/if}

  {#if loading}
    <p class="status">Loading...</p>
  {:else if items.length === 0}
    <div class="empty">
      <p>
        {searchQuery
          ? 'No items match your search.'
          : 'No documents yet. Import one to get started!'}
      </p>
    </div>
  {:else}
    <div class="grid">
      {#each items as item (item.id)}
        {@const meta = getItemAssetMeta(item.id)}
        <ItemCard
          id={item.id}
          title={item.title}
          assetCount={meta.assetCount}
          thumbnailPath={meta.thumbnailUrl ?? undefined}
          onclick={() =>
            navigation.navigate({
              name: 'item',
              collectionId,
              collectionName:
                navigation.current.name === 'collection'
                  ? (navigation.current as { collectionName: string }).collectionName
                  : '',
              itemId: item.id,
              itemTitle: item.title,
            })}
          onDelete={() => handleDeleteClick(item.id)}
        />
      {/each}
    </div>
  {/if}

  <!-- Delete confirmation modal -->
  {#if showDeleteConfirm}
    <div class="modal-overlay" onclick={handleDeleteCancel} role="presentation">
      <div
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="delete-modal-title"
        onclick={(e) => e.stopPropagation()}
      >
        <h3 id="delete-modal-title" class="modal-title">Delete Asset</h3>
        <p class="modal-message">
          Are you sure you want to delete <strong>{pendingDeleteFilename}</strong>? This will also
          remove associated OCR text and processing jobs. This action cannot be undone.
        </p>

        {#if deleteError}
          <p class="modal-error">{deleteError}</p>
        {/if}

        <div class="modal-actions">
          <Button variant="secondary" onclick={handleDeleteCancel} disabled={deleting}>
            Cancel
          </Button>
          <Button variant="danger" onclick={handleDeleteConfirm} disabled={deleting}>
            {deleting ? 'Deleting...' : 'Delete'}
          </Button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .collection-view {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .toolbar {
    display: flex;
    gap: var(--space-3);
    align-items: center;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--space-4);
  }
  .empty {
    text-align: center;
    padding: var(--space-8);
    color: var(--color-text-secondary);
  }
  .status {
    color: var(--color-text-secondary);
    text-align: center;
  }
  .error {
    color: var(--color-danger);
  }
  .notice {
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .drop-hint {
    padding: var(--space-3);
    border: 1px dashed var(--color-primary);
    border-radius: var(--radius-md);
    color: var(--color-text-secondary);
    text-align: center;
    background: var(--color-primary-subtle);
  }
  .collection-view.drag-active {
    outline: 1px dashed var(--color-primary);
    outline-offset: 6px;
    border-radius: var(--radius-md);
  }

  /* Delete confirmation modal */
  .modal-overlay {
    position: fixed;
    inset: 0;
    background-color: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: var(--space-4);
  }

  .modal {
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    padding: var(--space-6);
    max-width: 420px;
    width: 100%;
    box-shadow: var(--shadow-lg);
  }

  .modal-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
    margin: 0 0 var(--space-3) 0;
  }

  .modal-message {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    margin: 0 0 var(--space-4) 0;
    line-height: 1.5;
  }

  .modal-message strong {
    color: var(--color-text-primary);
  }

  .modal-error {
    font-size: var(--font-size-sm);
    color: var(--color-danger);
    margin: 0 0 var(--space-4) 0;
    padding: var(--space-2) var(--space-3);
    background-color: rgba(224, 92, 106, 0.1);
    border-radius: var(--radius-sm);
  }

  .modal-actions {
    display: flex;
    gap: var(--space-3);
    justify-content: flex-end;
  }
</style>
