<script lang="ts">
  import { getStore } from '$lib/db'
  import { locale, t } from '$lib/i18n'
  import type { Item, Entry } from '@entropia/store'

  let { itemId }: { itemId: string } = $props()

  const currentLocale = locale
  const translate = (key: string, params?: Record<string, string | number>) =>
    t(key as never, params)

  let item = $state<Item | null>(null)
  let entries = $state<Entry[]>([])
  let loading = $state(true)
  let entryCount = $state(0)

  // Parsed column mapping from metadata
  let columnMapping = $state<Record<string, string> | null>(null)
  let attributeKeys = $state<string[]>([])

  // Pagination
  let page = $state(0)
  const PAGE_SIZE = 50
  let visibleEntries = $derived(entries.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE))
  let totalPages = $derived(Math.ceil(entries.length / PAGE_SIZE))

  // Search
  let searchQuery = $state('')
  let filteredEntries = $derived.by(() => {
    if (!searchQuery.trim()) return entries
    const q = searchQuery.toLowerCase()
    return entries.filter(
      (e) =>
        e.content.toLowerCase().includes(q) ||
        (e.attributes && e.attributes.toLowerCase().includes(q))
    )
  })
  let displayEntries = $derived.by(() => {
    const source = searchQuery.trim() ? filteredEntries : entries
    return source.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE)
  })
  let displayTotalPages = $derived(
    Math.ceil((searchQuery.trim() ? filteredEntries.length : entries.length) / PAGE_SIZE)
  )

  async function loadData() {
    loading = true
    const store = getStore()
    const [loadedItem, loadedEntries] = await Promise.all([
      store.items.findById(itemId),
      store.entries.findByItem(itemId),
    ])
    item = loadedItem
    entries = loadedEntries
    entryCount = loadedEntries.length

    // Parse column mapping from metadata
    if (loadedItem?.metadata) {
      try {
        const meta = JSON.parse(loadedItem.metadata)
        if (meta.__columnMapping) {
          columnMapping = JSON.parse(meta.__columnMapping)
        }
      } catch { /* ignore */ }
    }

    // Collect unique attribute keys
    const keys = new Set<string>()
    for (const entry of loadedEntries.slice(0, 100)) {
      if (entry.attributes) {
        try {
          for (const k of Object.keys(JSON.parse(entry.attributes))) {
            keys.add(k)
          }
        } catch { /* ignore */ }
      }
    }
    attributeKeys = [...keys].sort()

    loading = false
  }

  function getAttr(entry: Entry, key: string): string {
    if (!entry.attributes) return ''
    try {
      return JSON.parse(entry.attributes)[key] ?? ''
    } catch {
      return ''
    }
  }

  $effect(() => {
    void loadData()
  })
</script>

{#if loading}
  <div class="corpus-loading">
    <p>{$currentLocale && translate('corpus.loading')}</p>
  </div>
{:else if item}
  <div class="corpus-view">
    <header class="corpus-header">
      <div class="corpus-header__info">
        <span class="corpus-header__badge">{$currentLocale && translate('corpus.badge')}</span>
        <span class="corpus-header__count">
          {$currentLocale && translate('corpus.entryCount', { count: entryCount })}
        </span>
      </div>
      <div class="corpus-header__search">
        <input
          type="search"
          class="corpus-search"
          placeholder={$currentLocale && translate('corpus.searchPlaceholder')}
          bind:value={searchQuery}
        />
      </div>
    </header>

    <div class="corpus-table-wrap">
      <table class="corpus-table">
        <thead>
          <tr>
            <th class="col-index">#</th>
            <th class="col-content">{$currentLocale && translate('corpus.contentColumn')}</th>
            {#each attributeKeys as key}
              <th class="col-attr">{key}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each displayEntries as entry, i (entry.id)}
            <tr>
              <td class="col-index">{entry.sortIndex + 1}</td>
              <td class="col-content">{entry.content}</td>
              {#each attributeKeys as key}
                <td class="col-attr" title={getAttr(entry, key)}>{getAttr(entry, key)}</td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    {#if displayTotalPages > 1}
      <div class="corpus-pagination">
        <button
          class="corpus-pagination__btn"
          disabled={page <= 0}
          onclick={() => (page = Math.max(0, page - 1))}
        >&lt;</button>
        <span class="corpus-pagination__label">
          {page + 1} / {displayTotalPages}
        </span>
        <button
          class="corpus-pagination__btn"
          disabled={page >= displayTotalPages - 1}
          onclick={() => (page = Math.min(displayTotalPages - 1, page + 1))}
        >&gt;</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .corpus-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--color-text-secondary);
  }

  .corpus-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .corpus-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    border-bottom: 1px solid var(--color-border-subtle);
    flex-shrink: 0;
  }

  .corpus-header__info {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .corpus-header__badge {
    display: inline-flex;
    align-items: center;
    padding: var(--space-1) var(--space-3);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 10%, transparent);
    border-radius: var(--radius-full);
  }

  .corpus-header__count {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
  }

  .corpus-header__search {
    flex-shrink: 0;
  }

  .corpus-search {
    width: min(100%, 280px);
    min-height: var(--control-height-sm);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-hairline);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
  }

  .corpus-search:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: var(--focus-ring);
    background: var(--color-surface);
  }

  .corpus-table-wrap {
    flex: 1;
    overflow: auto;
  }

  .corpus-table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--font-size-sm);
  }

  .corpus-table thead {
    position: sticky;
    top: 0;
    z-index: 1;
  }

  .corpus-table th {
    padding: var(--space-2) var(--space-3);
    text-align: left;
    font-weight: var(--font-weight-medium);
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-border-subtle);
    white-space: nowrap;
  }

  .corpus-table td {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--color-hairline);
    color: var(--color-text-primary);
    vertical-align: top;
  }

  .corpus-table tr:hover td {
    background: color-mix(in srgb, var(--color-accent) 4%, transparent);
  }

  .col-index {
    width: 48px;
    color: var(--color-text-muted);
    text-align: center;
  }

  .col-content {
    min-width: 200px;
    max-width: 500px;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .col-attr {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .corpus-pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    padding: var(--space-3);
    border-top: 1px solid var(--color-border-subtle);
    flex-shrink: 0;
  }

  .corpus-pagination__btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--color-hairline);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }

  .corpus-pagination__btn:hover:not(:disabled) {
    color: var(--color-text-primary);
    background: var(--color-surface-elevated);
  }

  .corpus-pagination__btn:disabled {
    opacity: 0.38;
    cursor: default;
  }

  .corpus-pagination__label {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
  }
</style>
