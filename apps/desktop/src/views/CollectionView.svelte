<script lang="ts">
  import { getStore } from '$lib/db'
  import { navigation } from '$lib/navigation'
  import { pickAndImportFiles } from '$lib/file-import'
  import { ItemCard, SearchBar, Button } from '@entropia/ui'
  import { onMount } from 'svelte'
  import type { Item } from '@entropia/store'

  let { collectionId }: { collectionId: string } = $props()

  let items = $state<Item[]>([])
  let searchQuery = $state('')
  let loading = $state(true)
  let error = $state<string | null>(null)
  let importing = $state(false)

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

  async function handleImport() {
    try {
      importing = true
      error = null
      const store = getStore()
      // Create a new item first, then import files as assets
      const item = await store.items.create({
        title: 'Untitled Document',
        collectionId,
        metadata: null,
      })

      const imported = await pickAndImportFiles(collectionId, item.id)

      if (imported.length === 0) {
        // User cancelled — delete the item
        await store.items.delete(item.id)
      } else {
        // Update title from first file name
        await store.items.update(item.id, {
          title: imported[0]!.originalName.replace(/\.[^.]+$/, ''),
        })

        // Create asset records
        for (const file of imported) {
          await store.assets.create({
            itemId: item.id,
            path: file.destPath,
            type: file.type,
            size: file.size,
          })
        }

        await loadItems()

        // Navigate to the new item
        navigation.navigate({
          name: 'item',
          collectionId,
          collectionName:
            navigation.current.name === 'collection'
              ? (navigation.current as { collectionName: string }).collectionName
              : '',
          itemId: item.id,
          itemTitle: imported[0]!.originalName.replace(/\.[^.]+$/, ''),
        })
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to import files'
    } finally {
      importing = false
    }
  }

  onMount(() => {
    loadItems()
  })
</script>

<div class="collection-view">
  <div class="toolbar">
    <SearchBar placeholder="Search items..." onsearch={handleSearch} onclear={handleClearSearch} />
    <Button variant="primary" onclick={handleImport} disabled={importing}>
      {importing ? 'Importing...' : '+ Import Document'}
    </Button>
  </div>

  {#if error}
    <p class="error">{error}</p>
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
</style>
