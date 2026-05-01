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

  let visibleCountLabel = $derived(
    `${filtered.length} ${filtered.length === 1 ? 'colección visible' : 'colecciones visibles'}`
  )

  async function loadCollections() {
    try {
      loading = true
      error = null
      const store = getStore()
      // Load ALL collections (including newly created ones with 0 items)
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

<div class="collections-view page-shell">
  <section class="page-header">
    <div class="page-header__content">
      <span class="page-header__eyebrow">Biblioteca</span>
      <h1>Colecciones</h1>
      <p>Gestioná tus espacios de trabajo y organizá el archivo por tema.</p>
      <span class="page-header__meta">{visibleCountLabel}</span>
    </div>

    <div class="page-toolbar collections-toolbar">
      <SearchBar
        placeholder="Buscar colecciones..."
        onsearch={(q) => (searchQuery = q)}
        onclear={() => (searchQuery = '')}
      />
      <Button variant="primary" onclick={() => (showCreate = !showCreate)}>
        {showCreate ? 'Cancelar' : '+ Nueva colección'}
      </Button>
    </div>
  </section>

  {#if showCreate}
    <Card>
      <form
        class="create-form"
        onsubmit={(e) => {
          e.preventDefault()
          handleCreate()
        }}
      >
        <div class="section-copy">
          <h2>Nueva colección</h2>
          <p>Creá un espacio para agrupar documentos, notas y análisis relacionados.</p>
        </div>
        <Input type="text" placeholder="Nombre de la colección" bind:value={newName} />
        <Input type="text" placeholder="Descripción (opcional)" bind:value={newDescription} />
        <div class="create-form__actions">
          <Button variant="primary" type="submit" disabled={!newName.trim()}>Crear colección</Button
          >
          <Button variant="ghost" onclick={() => (showCreate = false)}>Cancelar</Button>
        </div>
      </form>
    </Card>
  {/if}

  {#if error}
    <p class="surface-message surface-message--error">{error}</p>
  {/if}

  {#if loading}
    <p class="surface-message surface-message--center">Cargando colecciones...</p>
  {:else if filtered.length === 0}
    <div class="surface-message surface-message--center empty">
      <p>
        {searchQuery
          ? 'No encontramos colecciones para esa búsqueda. Probá con otro nombre o limpiá el filtro.'
          : 'Todavía no hay colecciones. Creá una para empezar a ordenar el material.'}
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
              <Input
                type="text"
                placeholder="Descripción (opcional)"
                bind:value={editDescription}
              />
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
          <h3 class="confirm-dialog__title">Eliminar colección</h3>
          <p class="confirm-dialog__message">
            ¿Estás seguro que querés eliminar la colección <strong>'{deletingName}'</strong>? Se
            eliminarán todos sus items y datos asociados.
          </p>
          <div class="confirm-dialog__actions">
            <Button variant="danger" onclick={handleConfirmDelete}>Eliminar</Button>
            <Button variant="ghost" onclick={handleCancelDelete}>Cancelar</Button>
          </div>
        </div>
      </Card>
    </div>
  {/if}
</div>

<style>
  .collections-view {
    min-height: 100%;
  }

  .collections-toolbar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex: 1;
  }

  .collections-toolbar :global(.search-bar) {
    min-width: min(100%, 320px);
    flex: 1 1 260px;
  }

  .create-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-4);
  }

  .create-form__actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .section-copy {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .section-copy h2 {
    font-size: var(--font-size-lg);
  }

  .section-copy p {
    max-width: 56ch;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--space-4);
  }

  .empty {
    min-height: 220px;
  }

  .edit-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
  }

  .edit-form__actions {
    display: flex;
    flex-wrap: wrap;
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
    padding: var(--space-5);
    min-width: min(100vw - 32px, 440px);
  }

  .confirm-dialog__title {
    margin: 0;
  }

  .confirm-dialog__message {
    margin: 0;
    font-size: var(--font-size-base, 1rem);
    color: var(--color-text-primary);
  }

  .confirm-dialog__actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    justify-content: flex-end;
  }

  @media (max-width: 720px) {
    .collections-toolbar {
      width: 100%;
      justify-content: stretch;
    }

    .collections-toolbar :global(.search-bar),
    .collections-toolbar :global(.btn) {
      width: 100%;
    }

    .create-form__actions :global(.btn),
    .edit-form__actions :global(.btn),
    .confirm-dialog__actions :global(.btn) {
      width: 100%;
    }
  }
</style>
