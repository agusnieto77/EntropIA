<script lang="ts">
  import { getStore } from '$lib/db'
  import { navigation } from '$lib/navigation'
  import { CollectionCard, SearchBar, Button, Input, Card } from '@entropia/ui'
  import { onMount } from 'svelte'
  import type { Collection } from '@entropia/store'

  let collections = $state<Collection[]>([])
  let searchQuery = $state('')
  let showCreate = $state(false)
  let newName = $state('')
  let newDescription = $state('')
  let loading = $state(true)
  let error = $state<string | null>(null)
  let itemCounts = $state<Record<string, number>>({})

  let filtered = $derived(
    searchQuery
      ? collections.filter((c) => c.name.toLowerCase().includes(searchQuery.toLowerCase()))
      : collections
  )

  async function loadCollections() {
    try {
      loading = true
      error = null
      const store = getStore()
      collections = await store.collections.findAll()

      // Load item counts
      const counts: Record<string, number> = {}
      for (const c of collections) {
        counts[c.id] = await store.collections.countItems(c.id)
      }
      itemCounts = counts
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load collections'
    } finally {
      loading = false
    }
  }

  async function handleCreate() {
    if (!newName.trim()) return
    try {
      const store = getStore()
      await store.collections.create({
        name: newName.trim(),
        description: newDescription.trim() || null,
      })
      newName = ''
      newDescription = ''
      showCreate = false
      await loadCollections()
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to create collection'
    }
  }

  async function handleDelete(id: string) {
    try {
      const store = getStore()
      await store.collections.delete(id)
      await loadCollections()
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to delete collection'
    }
  }

  onMount(() => {
    loadCollections()
  })
</script>

<div class="collections-view">
  <div class="toolbar">
    <SearchBar
      placeholder="Search collections..."
      onsearch={(q) => (searchQuery = q)}
      onclear={() => (searchQuery = '')}
    />
    <Button variant="primary" onclick={() => (showCreate = !showCreate)}>
      {showCreate ? 'Cancel' : '+ New Collection'}
    </Button>
  </div>

  {#if showCreate}
    <Card>
      <form
        class="create-form"
        onsubmit={(e) => {
          e.preventDefault()
          handleCreate()
        }}
      >
        <Input type="text" placeholder="Collection name" bind:value={newName} />
        <Input type="text" placeholder="Description (optional)" bind:value={newDescription} />
        <Button variant="primary" type="submit" disabled={!newName.trim()}>Create</Button>
      </form>
    </Card>
  {/if}

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if loading}
    <p class="status">Loading...</p>
  {:else if filtered.length === 0}
    <div class="empty">
      <p>
        {searchQuery
          ? 'No collections match your search.'
          : 'No collections yet. Create one to get started!'}
      </p>
    </div>
  {:else}
    <div class="grid">
      {#each filtered as collection (collection.id)}
        <CollectionCard
          id={collection.id}
          name={collection.name}
          description={collection.description ?? undefined}
          itemCount={itemCounts[collection.id] ?? 0}
          updatedAt={new Date(collection.updatedAt).getTime()}
          onclick={() =>
            navigation.navigate({
              name: 'collection',
              id: collection.id,
              collectionName: collection.name,
            })}
        />
      {/each}
    </div>
  {/if}
</div>

<style>
  .collections-view {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .toolbar {
    display: flex;
    gap: var(--space-3);
    align-items: center;
  }
  .create-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
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
