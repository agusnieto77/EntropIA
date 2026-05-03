<script lang="ts">
  import { navigation } from '$lib/navigation'
  import { getStore } from '$lib/db'
  import { locale, t } from '$lib/i18n'
  import { Button } from '@entropia/ui'
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
  const currentLocale = locale
  const translate = (key: string, params?: Record<string, string | number>) =>
    t(key as never, params)

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

  function handleInput() {
    if (debounceTimer) clearTimeout(debounceTimer)
  }

  function handleSearchValueChange(query: string, e: Event) {
    searchQuery = query
    handleInput()

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
  <div class="topbar__leading">
    {#if $navigation.canGoBack}
      <Button variant="ghost" size="sm" onclick={() => navigation.back()}
        >{$currentLocale && t('topbar.back')}</Button
      >
    {/if}
    <nav class="breadcrumb" aria-label={$currentLocale && t('topbar.breadcrumb')}>
      {#each $navigation.breadcrumb as crumb, i}
        {#if i > 0}<span class="sep">/</span>{/if}
        <span class="crumb" class:last={i === $navigation.breadcrumb.length - 1}>{crumb}</span>
      {/each}
    </nav>
  </div>

  <div class="global-search">
    <div class="global-search__input-wrap">
      <input
        class="global-search__input"
        type="search"
        bind:value={searchQuery}
        bind:this={searchInputEl}
        placeholder={$currentLocale && translate('topbar.searchPlaceholder')}
        aria-label={$currentLocale && translate('topbar.searchAria')}
        oninput={(event: Event) =>
          handleSearchValueChange((event.currentTarget as HTMLInputElement).value, event)}
        onkeydown={handleKeydown}
        onblur={handleBlur}
        onfocus={handleFocus}
      />

      {#if searchQuery}
        <button
          class="global-search__clear"
          type="button"
          aria-label={$currentLocale && translate('topbar.searchClear')}
          title={$currentLocale && translate('topbar.searchClear')}
          onclick={handleClear}
        >
          <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor">
            <path d="M5 5l10 10M15 5L5 15" stroke-width="1.8" stroke-linecap="round" />
          </svg>
        </button>
      {/if}
    </div>

    {#if showResults}
      <div class="global-search__dropdown">
        {#if searching}
          <div class="global-search__status">
            {$currentLocale && translate('topbar.searchSearching')}
          </div>
        {:else if searchResults.length === 0}
          <div class="global-search__status">
            {$currentLocale && translate('topbar.searchNoResults', { query: searchQuery })}
          </div>
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

  <div class="topbar__actions">
    <button
      class="topbar__icon-btn"
      type="button"
      onclick={() => navigation.openRootSection({ name: 'db-browser' })}
      title={$currentLocale && translate('topbar.dbBrowserTitle')}
      aria-label={$currentLocale && translate('topbar.dbBrowserAria')}
    >
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor">
        <ellipse cx="12" cy="5" rx="7" ry="3" stroke-width="1.8" />
        <path d="M5 5v6c0 1.66 3.13 3 7 3s7-1.34 7-3V5" stroke-width="1.8" />
        <path d="M5 11v6c0 1.66 3.13 3 7 3s7-1.34 7-3v-6" stroke-width="1.8" />
      </svg>
    </button>

    <button
      class="topbar__icon-btn"
      type="button"
      onclick={() => navigation.openRootSection({ name: 'settings' })}
      title={$currentLocale && t('topbar.settingsTitle')}
      aria-label={$currentLocale && t('topbar.settingsAria')}
    >
      <svg width="16" height="16" viewBox="0 0 20 20" fill="currentColor">
        <path
          fill-rule="evenodd"
          d="M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z"
          clip-rule="evenodd"
        />
      </svg>
    </button>
  </div>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border-subtle);
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.02), transparent), var(--color-surface);
    box-shadow: var(--shadow-sm);
    min-width: 0;
  }

  .topbar__leading {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex: 1 1 auto;
    min-width: 0;
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: 1 1 auto;
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

  .topbar__actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-left: auto;
    flex-shrink: 0;
  }

  .topbar__icon-btn {
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
  .topbar__icon-btn:hover {
    color: var(--color-text-primary);
    background: var(--color-surface-elevated);
    border-color: var(--color-border-strong);
  }
  .topbar__icon-btn:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .global-search {
    position: relative;
    width: 100%;
    max-width: 320px;
    flex: 1 1 260px;
    min-width: 0;
  }

  .global-search__input-wrap {
    position: relative;
  }

  .global-search__input {
    width: 100%;
    min-height: var(--control-height-md);
    padding: 0 calc(var(--space-4) + 18px) 0 var(--space-3);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .global-search__input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: var(--focus-ring);
    background: var(--color-surface);
  }

  .global-search__clear {
    position: absolute;
    top: 50%;
    right: var(--space-2);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
    transform: translateY(-50%);
  }

  .global-search__clear:hover {
    color: var(--color-text-primary);
    background: var(--color-surface-raised);
  }

  .global-search__clear:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

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
      flex-wrap: wrap;
    }

    .topbar__leading {
      flex: 1 1 0;
    }

    .global-search {
      order: 3;
      max-width: none;
      flex-basis: 100%;
    }
  }
</style>
