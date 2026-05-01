<script lang="ts">
  import { navigation } from '$lib/navigation'
  import { getStore } from '$lib/db'
  import { ActionIcon, Button } from '@entropia/ui'
  import type { Collection, Item } from '@entropia/store'

  interface SearchResult {
    item: Item
    collection: Collection
  }

  let searchQuery = $state('')
  let searchResults = $state<SearchResult[]>([])
  let showResults = $state(false)
  let searching = $state(false)
  let debounceTimer: ReturnType<typeof setTimeout> | null = null
  let searchInputEl: HTMLInputElement | undefined = $state()

  async function performSearch(query: string) {
    if (!query.trim()) {
      searchResults = []
      showResults = false
      return
    }

    searching = true
    try {
      const store = getStore()
      const matchedItems = await store.items.searchGlobal(query, 20)
      const results: SearchResult[] = []

      // Cache collections to avoid repeated lookups
      const collectionCache = new Map<string, Collection>()
      for (const item of matchedItems) {
        let collection = collectionCache.get(item.collectionId)
        if (!collection) {
          const found = await store.collections.findById(item.collectionId)
          if (!found) continue
          collection = found
          collectionCache.set(item.collectionId, collection)
        }
        results.push({ item, collection })
      }

      searchResults = results
      showResults = true
    } catch (e) {
      console.error('[Search] error:', e)
      searchResults = []
    } finally {
      searching = false
    }
  }

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement
    searchQuery = target.value

    if (debounceTimer) clearTimeout(debounceTimer)

    if (!searchQuery.trim()) {
      searchResults = []
      showResults = false
      return
    }

    debounceTimer = setTimeout(() => {
      performSearch(searchQuery)
    }, 300)
  }

  function handleClear() {
    searchQuery = ''
    searchResults = []
    showResults = false
    if (debounceTimer) clearTimeout(debounceTimer)
  }

  function handleResultClick(result: SearchResult) {
    navigation.navigate({
      name: 'collection',
      id: result.collection.id,
      collectionName: result.collection.name,
    })
    navigation.navigate({
      name: 'item',
      collectionId: result.collection.id,
      collectionName: result.collection.name,
      itemId: result.item.id,
      itemTitle: result.item.title,
    })
    handleClear()
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      handleClear()
      searchInputEl?.blur()
    }
  }

  function handleBlur() {
    setTimeout(() => {
      showResults = false
    }, 200)
  }

  function handleFocus() {
    if (searchResults.length > 0) {
      showResults = true
    }
  }
</script>

<header class="topbar">
  {#if $navigation.canGoBack}
    <Button variant="ghost" size="sm" onclick={() => navigation.back()}>← Back</Button>
  {/if}
  <nav class="breadcrumb" aria-label="Breadcrumb">
    {#each $navigation.breadcrumb as crumb, i}
      {#if i > 0}<span class="sep">/</span>{/if}
      <span class="crumb" class:last={i === $navigation.breadcrumb.length - 1}>{crumb}</span>
    {/each}
  </nav>

  <button
    class="settings-btn"
    type="button"
    onclick={() => navigation.navigate({ name: 'settings' })}
    title="Configuracion"
    aria-label="Abrir configuración"
  >
    <svg width="16" height="16" viewBox="0 0 20 20" fill="currentColor">
      <path
        fill-rule="evenodd"
        d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z"
        clip-rule="evenodd"
      />
    </svg>
  </button>

  <div class="global-search">
    <div class="global-search__input-wrapper">
      <span class="global-search__icon" aria-hidden="true">&#128269;</span>
      <input
        bind:this={searchInputEl}
        class="global-search__input"
        type="search"
        placeholder="Buscar por nombre de archivo..."
        aria-label="Buscar archivos"
        value={searchQuery}
        oninput={handleInput}
        onkeydown={handleKeydown}
        onblur={handleBlur}
        onfocus={handleFocus}
      />
      {#if searchQuery}
        <button
          class="global-search__clear"
          type="button"
          onclick={handleClear}
          aria-label="Limpiar búsqueda"
        >
          <ActionIcon name="close" size={14} />
        </button>
      {/if}
    </div>

    {#if showResults}
      <div class="global-search__dropdown">
        {#if searching}
          <div class="global-search__status">Buscando...</div>
        {:else if searchResults.length === 0}
          <div class="global-search__status">Sin resultados para "{searchQuery}"</div>
        {:else}
          {#each searchResults as result (result.item.id)}
            <button
              class="global-search__result"
              type="button"
              onclick={() => handleResultClick(result)}
            >
              <span class="global-search__result-title">{result.item.title}</span>
              <span class="global-search__result-collection">{result.collection.name}</span>
            </button>
          {/each}
        {/if}
      </div>
    {/if}
  </div>
</header>

<style>
  .topbar {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto minmax(260px, 360px);
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border-subtle);
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.02), transparent), var(--color-surface);
    box-shadow: var(--shadow-sm);
  }
  .breadcrumb {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }
  .crumb {
    color: var(--color-text-secondary);
    font-size: var(--font-size-xs);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .crumb.last {
    color: var(--color-text-primary);
    font-weight: var(--font-weight-medium);
  }
  .sep {
    color: var(--color-text-muted);
  }

  .settings-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height-sm);
    height: var(--control-height-sm);
    padding: 0;
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition:
      color var(--transition-base),
      background-color var(--transition-base),
      border-color var(--transition-base),
      box-shadow var(--transition-base);
  }
  .settings-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-surface-elevated);
    border-color: var(--color-border-strong);
  }
  .settings-btn:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .global-search {
    position: relative;
    width: 100%;
    min-width: 0;
  }

  .global-search__input-wrapper {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-height-md);
    padding: 0 var(--space-3);
    background-color: var(--color-surface-sunken);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    transition:
      border-color var(--transition-base),
      box-shadow var(--transition-base),
      background-color var(--transition-base);
  }

  .global-search__input-wrapper:focus-within {
    border-color: var(--color-accent);
    box-shadow: var(--focus-ring);
    background-color: var(--color-surface);
  }

  .global-search__icon {
    flex-shrink: 0;
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    opacity: 0.9;
  }

  .global-search__input {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    font-family: var(--font-sans);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    min-width: 0;
    line-height: var(--line-height-base);
  }

  .global-search__input::placeholder {
    color: var(--color-text-muted);
  }

  .global-search__input::-webkit-search-cancel-button {
    display: none;
  }

  .global-search__clear {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: var(--radius-full);
    background-color: var(--color-surface-raised);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition:
      background-color var(--transition-base),
      color var(--transition-base),
      box-shadow var(--transition-base);
  }

  .global-search__clear :global(svg) {
    pointer-events: none;
  }

  .global-search__clear:hover {
    background-color: var(--color-border-subtle);
    color: var(--color-text-primary);
  }

  .global-search__clear:focus-visible,
  .global-search__result:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .global-search__dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    margin-top: var(--space-1);
    background-color: var(--color-surface-elevated);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg, 0 8px 24px rgba(0, 0, 0, 0.15));
    max-height: 320px;
    overflow-y: auto;
    z-index: 200;
  }

  .global-search__status {
    padding: var(--space-3);
    text-align: center;
    color: var(--color-text-secondary);
    font-size: var(--font-size-xs);
  }

  .global-search__result {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    width: 100%;
    padding: var(--space-3);
    border: none;
    background: none;
    cursor: pointer;
    text-align: left;
    font-family: var(--font-sans);
    transition:
      background-color var(--transition-base),
      box-shadow var(--transition-base);
  }

  .global-search__result:hover {
    background-color: var(--color-surface-raised);
  }

  .global-search__result + .global-search__result {
    border-top: 1px solid var(--color-border-subtle);
  }

  .global-search__result-title {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-primary);
  }

  .global-search__result-collection {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }

  @media (max-width: 900px) {
    .topbar {
      grid-template-columns: auto minmax(0, 1fr);
    }

    .settings-btn {
      justify-self: end;
    }

    .global-search {
      grid-column: 1 / -1;
    }
  }
</style>
