<script lang="ts">
  import { getStore } from '$lib/db'
  import { navigation } from '$lib/navigation'
  import { pickAndImportFiles, importFilesFromPaths, type ImportedFile } from '$lib/file-import'
  import { exportCollectionById } from '$lib/export'
  import { ItemCard, SearchBar, Button } from '@entropia/ui'
  import { onMount, onDestroy } from 'svelte'
  import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview'
  import type { Item } from '@entropia/store'

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

  async function finalizeImportedItem(itemId: string, imported: ImportedFile[]) {
    const store = getStore()

    if (imported.length === 0) {
      return
    }

    await store.items.update(itemId, {
      title: imported[0]!.originalName.replace(/\.[^.]+$/, ''),
    })

    for (const file of imported) {
      await store.assets.create({
        itemId,
        path: file.destPath,
        type: file.type,
        size: file.size,
      })
    }

    await loadItems()

    navigation.navigate({
      name: 'item',
      collectionId,
      collectionName:
        navigation.current.name === 'collection'
          ? (navigation.current as { collectionName: string }).collectionName
          : '',
      itemId,
      itemTitle: imported[0]!.originalName.replace(/\.[^.]+$/, ''),
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

    let itemId: string
    try {
      console.log('[import] creating item, collectionId:', collectionId)
      const item = await store.items.create({
        title: 'Untitled Document',
        collectionId,
        metadata: null,
      })
      itemId = item.id
    } catch (e) {
      console.log('[import] ERROR creating item, collectionId:', collectionId, e)
      error = formatImportStageError('Failed to import files', 'creating item', e)
      importing = false
      return
    }

    let imported: ImportedFile[]
    try {
      imported = await pickAndImportFiles(collectionId, itemId)
    } catch (e) {
      error = formatImportStageError(
        'Failed to import files',
        'selecting/copying/importing files',
        e
      )
      importing = false
      return
    }

    if (imported.length === 0) {
      try {
        await store.items.delete(itemId)
      } catch (e) {
        error = formatImportStageError('Failed to import files', 'cleaning up empty import item', e)
      } finally {
        importing = false
      }
      return
    }

    try {
      await finalizeImportedItem(itemId, imported)
    } catch (e) {
      error = formatImportStageError('Failed to import files', 'finalizing imported item', e)
    } finally {
      importing = false
    }
  }

  async function handleImportFromDroppedPaths(paths: string[]) {
    importing = true
    error = null
    importNotice = null
    const store = getStore()

    let itemId: string
    try {
      const item = await store.items.create({
        title: 'Untitled Document',
        collectionId,
        metadata: null,
      })
      itemId = item.id
    } catch (e) {
      error = formatImportStageError('Failed to import dropped files', 'creating item', e)
      importing = false
      dragActive = false
      return
    }

    let result: Awaited<ReturnType<typeof importFilesFromPaths>>
    try {
      result = await importFilesFromPaths(paths, collectionId, itemId)
    } catch (e) {
      error = formatImportStageError(
        'Failed to import dropped files',
        'selecting/copying/importing files',
        e
      )
      importing = false
      dragActive = false
      return
    }

    if (result.imported.length === 0) {
      try {
        await store.items.delete(itemId)
        if (result.rejected.length > 0) {
          error = `Unsupported format: ${result.rejected.join(', ')}`
        }
      } catch (e) {
        error = formatImportStageError(
          'Failed to import dropped files',
          'cleaning up empty import item',
          e
        )
      } finally {
        importing = false
        dragActive = false
      }
      return
    }

    try {
      await finalizeImportedItem(itemId, result.imported)

      const noticeParts: string[] = []
      if (result.rejected.length > 0) {
        noticeParts.push(`unsupported skipped: ${result.rejected.join(', ')}`)
      }
      if (result.skippedDuplicatePaths > 0) {
        noticeParts.push(`duplicate paths skipped: ${result.skippedDuplicatePaths}`)
      }
      importNotice = noticeParts.length > 0 ? `Import completed (${noticeParts.join(' · ')})` : null
    } catch (e) {
      error = formatImportStageError(
        'Failed to import dropped files',
        'finalizing imported item',
        e
      )
    } finally {
      importing = false
      dragActive = false
    }
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
        <ItemCard
          id={item.id}
          title={item.title}
          assetCount={0}
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
        />
      {/each}
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
</style>
