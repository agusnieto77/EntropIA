<script lang="ts">
  import { ActionIcon } from '../Button'
  import type { SearchBarProps } from './SearchBar.types'

  let {
    value = '',
    placeholder = '',
    debounceMs = 300,
    onsearch,
    onclear,
  }: SearchBarProps = $props()

  let internalValue = $state('')
  let lastExternalValue = $state<string | undefined>(undefined)
  let debounceTimer: ReturnType<typeof setTimeout> | null = null

  function clearDebounceTimer() {
    if (!debounceTimer) {
      return
    }
    clearTimeout(debounceTimer)
    debounceTimer = null
  }

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement
    internalValue = target.value

    clearDebounceTimer()

    debounceTimer = setTimeout(() => {
      onsearch?.(internalValue)
    }, debounceMs)
  }

  function handleClear() {
    internalValue = ''
    clearDebounceTimer()
    onclear?.()
  }

  $effect(() => {
    if (lastExternalValue === undefined) {
      lastExternalValue = value
      internalValue = value
      return
    }

    if (value === lastExternalValue) {
      return
    }
    lastExternalValue = value
    internalValue = value
    clearDebounceTimer()
  })

  let showClear = $derived(internalValue.length > 0)
</script>

<div class="search-bar">
  <span class="search-bar__icon" data-testid="search-icon" aria-hidden="true">&#128269;</span>
  <input
    class="search-bar__input"
    type="search"
    {placeholder}
    aria-label={placeholder || 'Search'}
    value={internalValue}
    oninput={handleInput}
  />
  {#if showClear}
    <button
      class="search-bar__clear"
      type="button"
      data-testid="search-clear"
      onclick={handleClear}
      aria-label="Clear search"
    >
      <ActionIcon name="close" size={14} />
    </button>
  {/if}
</div>

<style>
  .search-bar {
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

  .search-bar:focus-within {
    border-color: var(--color-accent);
    box-shadow: var(--focus-ring);
    background-color: var(--color-surface);
  }

  .search-bar__icon {
    flex-shrink: 0;
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    opacity: 0.9;
  }

  .search-bar__input {
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

  .search-bar__input::placeholder {
    color: var(--color-text-muted);
  }

  /* Remove default search input clear button */
  .search-bar__input::-webkit-search-cancel-button {
    display: none;
  }

  .search-bar__clear {
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

  .search-bar__clear :global(svg) {
    pointer-events: none;
  }

  .search-bar__clear:hover {
    background-color: var(--color-border-subtle);
    color: var(--color-text-primary);
  }

  .search-bar__clear:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }
</style>
