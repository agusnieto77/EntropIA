<script lang="ts">
  import { getStore } from '$lib/db'
  import { locale, t } from '$lib/i18n'
  import { invoke } from '@tauri-apps/api/core'
  import type { Item, Entry, EntryResult } from '@entropia/store'

  let { itemId }: { itemId: string } = $props()

  const currentLocale = locale
  const translate = (key: string, params?: Record<string, string | number>) =>
    t(key as never, params)

  let item = $state<Item | null>(null)
  let entries = $state<Entry[]>([])
  let loading = $state(true)
  let entryCount = $state(0)

  // Column mapping + attribute keys
  let attributeKeys = $state<string[]>([])

  // Analysis results: entryId → { sentiment: {...}, emotion: {...}, ... }
  let resultsByEntry = $state<Record<string, Record<string, EntryResult>>>({})
  let resultJobTypes = $state<string[]>([])

  // Selection
  let selectedIds = $state<Set<string>>(new Set())
  let allSelected = $derived(entries.length > 0 && selectedIds.size === entries.length)
  let someSelected = $derived(selectedIds.size > 0 && selectedIds.size < entries.length)
  let selectionCount = $derived(selectedIds.size)

  // Analysis state
  let analyzing = $state(false)
  let analysisProgress = $state({ current: 0, total: 0, task: '' })

  // Pagination
  let page = $state(0)
  const PAGE_SIZE = 50

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

  function toggleSelect(entryId: string) {
    const next = new Set(selectedIds)
    if (next.has(entryId)) {
      next.delete(entryId)
    } else {
      next.add(entryId)
    }
    selectedIds = next
  }

  function toggleSelectAll() {
    if (allSelected) {
      selectedIds = new Set()
    } else {
      selectedIds = new Set(entries.map((e) => e.id))
    }
  }

  function getTargetEntries(): Entry[] {
    if (selectedIds.size > 0) {
      return entries.filter((e) => selectedIds.has(e.id))
    }
    return entries
  }

  function getAttr(entry: Entry, key: string): string {
    if (!entry.attributes) return ''
    try {
      return JSON.parse(entry.attributes)[key] ?? ''
    } catch {
      return ''
    }
  }

  function getResultLabel(entry: Entry, jobType: string): string {
    const result = resultsByEntry[entry.id]?.[jobType]
    if (!result) return ''
    try {
      const parsed = JSON.parse(result.result)
      if (parsed.output) return parsed.output
      if (parsed.error) return 'error'
      return JSON.stringify(parsed).slice(0, 40)
    } catch {
      return ''
    }
  }

  function getResultClass(entry: Entry, jobType: string): string {
    const label = getResultLabel(entry, jobType).toLowerCase()
    if (label === 'pos' || label === 'positive') return 'result--positive'
    if (label === 'neg' || label === 'negative') return 'result--negative'
    if (label === 'neu' || label === 'neutral') return 'result--neutral'
    return ''
  }

  function getResultTooltip(entry: Entry, jobType: string): string {
    const result = resultsByEntry[entry.id]?.[jobType]
    if (!result) return ''
    try {
      const parsed = JSON.parse(result.result)
      if (parsed.probas) {
        return Object.entries(parsed.probas)
          .map(([k, v]) => `${k}: ${(v as number * 100).toFixed(1)}%`)
          .join('\n')
      }
      return ''
    } catch {
      return ''
    }
  }

  interface SentimentTaskResult {
    output?: string
    probas?: Record<string, number>
    error?: string
  }

  interface SentimentEntryResult {
    id: string
    [task: string]: string | SentimentTaskResult | undefined
  }

  async function runAnalysis(taskName: string) {
    const targets = getTargetEntries()
    if (targets.length === 0) return

    analyzing = true
    analysisProgress = { current: 0, total: targets.length, task: taskName }

    const store = getStore()

    // Process in batches to avoid overwhelming Python / show progress
    const BATCH_SIZE = 50
    for (let batchStart = 0; batchStart < targets.length; batchStart += BATCH_SIZE) {
      const batch = targets.slice(batchStart, batchStart + BATCH_SIZE)
      const inputEntries = batch.map((e) => ({ id: e.id, text: e.content }))

      try {
        const results: SentimentEntryResult[] = await invoke('analyze_entries_sentiment', {
          request: {
            entries: inputEntries,
            tasks: [taskName],
          },
        })

        // Store results
        for (const entryResult of results) {
          const taskData = entryResult[taskName] as SentimentTaskResult | undefined
          if (taskData) {
            await store.entryResults.upsert({
              entryId: entryResult.id,
              jobType: taskName,
              result: JSON.stringify(taskData),
            })
          }
        }
      } catch (e) {
        console.error(`[CorpusView] ${taskName} analysis failed:`, e)
        analyzing = false
        await loadResults()
        return
      }

      analysisProgress = {
        current: Math.min(batchStart + BATCH_SIZE, targets.length),
        total: targets.length,
        task: taskName,
      }
    }

    await loadResults()
    analyzing = false
  }

  async function loadResults() {
    const store = getStore()
    const byEntry: Record<string, Record<string, EntryResult>> = {}
    const jobTypes = new Set<string>()

    for (const entry of entries) {
      const results = await store.entryResults.findByEntry(entry.id)
      if (results.length > 0) {
        byEntry[entry.id] = {}
        for (const r of results) {
          byEntry[entry.id]![r.jobType] = r
          jobTypes.add(r.jobType)
        }
      }
    }

    resultsByEntry = byEntry
    resultJobTypes = [...jobTypes].sort()
  }

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

    await loadResults()
    loading = false
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
        {#if selectionCount > 0}
          <span class="corpus-header__selection">
            {$currentLocale && translate('corpus.selected', { count: selectionCount })}
          </span>
        {/if}
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

    <!-- Action toolbar -->
    <div class="corpus-toolbar">
      <div class="corpus-toolbar__actions">
        <button
          class="corpus-action"
          onclick={() => runAnalysis('sentiment')}
          disabled={analyzing}
          title={$currentLocale && translate('corpus.sentimentTitle')}
        >
          {$currentLocale && translate('corpus.sentiment')}
        </button>
        <button
          class="corpus-action"
          onclick={() => runAnalysis('emotion')}
          disabled={analyzing}
          title={$currentLocale && translate('corpus.emotionTitle')}
        >
          {$currentLocale && translate('corpus.emotion')}
        </button>
        <button
          class="corpus-action"
          onclick={() => runAnalysis('hate_speech')}
          disabled={analyzing}
          title={$currentLocale && translate('corpus.hateSpeechTitle')}
        >
          {$currentLocale && translate('corpus.hateSpeech')}
        </button>
        <span class="corpus-toolbar__divider"></span>
        <button
          class="corpus-action"
          onclick={() => runAnalysis('ner')}
          disabled={analyzing}
          title={$currentLocale && translate('corpus.nerTitle')}
        >
          NER
        </button>
      </div>
      <div class="corpus-toolbar__hint">
        {#if analyzing}
          <span class="corpus-toolbar__progress">
            {analysisProgress.task}: {analysisProgress.current}/{analysisProgress.total}
          </span>
        {:else if selectionCount > 0}
          {$currentLocale && translate('corpus.actionOnSelected', { count: selectionCount })}
        {:else}
          {$currentLocale && translate('corpus.actionOnAll')}
        {/if}
      </div>
    </div>

    <div class="corpus-table-wrap">
      <table class="corpus-table">
        <thead>
          <tr>
            <th class="col-check">
              <input
                type="checkbox"
                checked={allSelected}
                indeterminate={someSelected}
                onchange={toggleSelectAll}
                title={$currentLocale && translate('corpus.selectAll')}
              />
            </th>
            <th class="col-index">#</th>
            <th class="col-content">{$currentLocale && translate('corpus.contentColumn')}</th>
            {#each attributeKeys as key}
              <th class="col-attr">{key}</th>
            {/each}
            {#each resultJobTypes as jobType}
              <th class="col-result">{jobType}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each displayEntries as entry (entry.id)}
            <tr class:selected={selectedIds.has(entry.id)}>
              <td class="col-check">
                <input
                  type="checkbox"
                  checked={selectedIds.has(entry.id)}
                  onchange={() => toggleSelect(entry.id)}
                />
              </td>
              <td class="col-index">{entry.sortIndex + 1}</td>
              <td class="col-content">{entry.content}</td>
              {#each attributeKeys as key}
                <td class="col-attr" title={getAttr(entry, key)}>{getAttr(entry, key)}</td>
              {/each}
              {#each resultJobTypes as jobType}
                <td
                  class="col-result {getResultClass(entry, jobType)}"
                  title={getResultTooltip(entry, jobType)}
                >
                  {getResultLabel(entry, jobType)}
                </td>
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
    padding: var(--space-3) var(--space-4);
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

  .corpus-header__selection {
    font-size: var(--font-size-xs);
    color: var(--color-accent);
    font-weight: var(--font-weight-medium);
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

  /* Toolbar */
  .corpus-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--color-hairline);
    background: var(--color-surface-sunken);
    flex-shrink: 0;
  }

  .corpus-toolbar__actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .corpus-toolbar__divider {
    width: 1px;
    height: 18px;
    background: var(--color-border-subtle);
    margin: 0 var(--space-1);
  }

  .corpus-action {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1) var(--space-3);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-secondary);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-hairline);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition:
      color var(--transition-base),
      background-color var(--transition-base),
      border-color var(--transition-base);
  }

  .corpus-action:hover:not(:disabled) {
    color: var(--color-text-primary);
    background: var(--color-surface-elevated);
    border-color: var(--color-border-strong);
  }

  .corpus-action:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .corpus-toolbar__hint {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .corpus-toolbar__progress {
    color: var(--color-accent);
    font-weight: var(--font-weight-medium);
  }

  /* Table */
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

  .corpus-table tr.selected td {
    background: color-mix(in srgb, var(--color-accent) 8%, transparent);
  }

  .col-check {
    width: 32px;
    text-align: center;
    vertical-align: middle;
  }

  .col-check input[type='checkbox'] {
    cursor: pointer;
    accent-color: var(--color-accent);
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

  .col-result {
    min-width: 70px;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    text-align: center;
    white-space: nowrap;
  }

  .result--positive {
    color: var(--color-success);
  }

  .result--negative {
    color: var(--color-danger);
  }

  .result--neutral {
    color: var(--color-text-muted);
  }

  /* Pagination */
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
