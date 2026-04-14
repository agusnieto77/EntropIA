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
  let editingId = $state<string | null>(null)
  let editName = $state('')
  let editDescription = $state('')
  let deletingId = $state<string | null>(null)
  let deletingName = $state('')

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
      const collection = await store.collections.create({
        name: newName.trim(),
        description: newDescription.trim() || null,
      })
      console.log('[Collections] created collection:', collection.id, collection.name)
      newName = ''
      newDescription = ''
      showCreate = false
      await loadCollections()
    } catch (e) {
      console.log('[Collections] ERROR creating collection:', e)
      error = e instanceof Error ? e.message : 'Failed to create collection'
    }
  }

  function handleEdit(collection: Collection) {
    editingId = collection.id
    editName = collection.name
    editDescription = collection.description ?? ''
  }

  function handleCancelEdit() {
    editingId = null
    editName = ''
    editDescription = ''
  }

  async function handleSaveEdit() {
    if (!editingId || !editName.trim()) return
    try {
      const store = getStore()
      await store.collections.update(editingId, {
        name: editName.trim(),
        description: editDescription.trim() || null,
      })
      editingId = null
      editName = ''
      editDescription = ''
      await loadCollections()
    } catch (e) {
      error = e instanceof Error ? e.message : 'Error al actualizar la colección'
    }
  }

  function handleDeleteRequest(id: string, name: string) {
    deletingId = id
    deletingName = name
  }

  function handleCancelDelete() {
    deletingId = null
    deletingName = ''
  }

  async function handleConfirmDelete() {
    if (!deletingId) return
    console.log('[Collections] deleting collection:', deletingId, deletingName)
    try {
      const store = getStore()
      await store.collections.delete(deletingId)
      console.log('[Collections] deleted successfully')
      deletingId = null
      deletingName = ''
      await loadCollections()
    } catch (e) {
      console.error('[Collections] ERROR deleting collection:', e)
      error = e instanceof Error ? e.message : String(e)
      deletingId = null
      deletingName = ''
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
        {#if editingId === collection.id}
          <Card>
            <form
              class="edit-form"
              onsubmit={(e) => {
                e.preventDefault()
                handleSaveEdit()
              }}
            >
              <Input type="text" placeholder="Nombre" bind:value={editName} />
              <Input type="text" placeholder="Descripción (opcional)" bind:value={editDescription} />
              <div class="edit-form__actions">
                <Button variant="primary" type="submit" disabled={!editName.trim()}>Guardar</Button>
                <Button variant="ghost" onclick={handleCancelEdit}>Cancelar</Button>
              </div>
            </form>
          </Card>
        {:else}
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
            onedit={() => handleEdit(collection)}
            ondelete={() => handleDeleteRequest(collection.id, collection.name)}
          />
        {/if}
      {/each}
    </div>
  {/if}

  {#if deletingId}
    <div class="confirm-overlay">
      <Card>
        <div class="confirm-dialog">
          <p class="confirm-dialog__message">¿Estás seguro que querés eliminar la colección <strong>'{deletingName}'</strong>? Se eliminarán todos sus items y datos asociados.</p>
          <div class="confirm-dialog__actions">
            <Button variant="primary" onclick={handleConfirmDelete}>Eliminar</Button>
            <Button variant="ghost" onclick={handleCancelDelete}>Cancelar</Button>
          </div>
        </div>
      </Card>
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
  .edit-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-3);
  }
  .edit-form__actions {
    display: flex;
    gap: var(--space-2);
  }
  .confirm-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: rgba(0, 0, 0, 0.5);
    z-index: 100;
  }
  .confirm-dialog {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-4);
  }
  .confirm-dialog__message {
    margin: 0;
    font-size: var(--font-size-base, 1rem);
    color: var(--color-text-primary);
  }
  .confirm-dialog__actions {
    display: flex;
    gap: var(--space-2);
  }
  .error {
    color: var(--color-danger);
  }
</style>
