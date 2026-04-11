<script lang="ts">
  import type { SearchBarProps } from './SearchBar.types'

  let {
    value = '',
    placeholder = '',
    debounceMs = 300,
    onsearch,
    onclear,
  }: SearchBarProps = $props()

  let internalValue = $state(value)
  let debounceTimer: ReturnType<typeof setTimeout> | null = null

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement
    internalValue = target.value

    if (debounceTimer) {
      clearTimeout(debounceTimer)
    }

    debounceTimer = setTimeout(() => {
      onsearch?.(internalValue)
    }, debounceMs)
  }

  function handleClear() {
    internalValue = ''
    if (debounceTimer) {
      clearTimeout(debounceTimer)
      debounceTimer = null
    }
    onclear?.()
  }

  const showClear = $derived(internalValue.length > 0)
</script>

<div class="search-bar">
  <span class="search-bar__icon" data-testid="search-icon" aria-hidden="true">&#128269;</span>
  <input
    class="search-bar__input"
    type="search"
    {placeholder}
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
      &times;
    </button>
  {/if}
</div>

<style>
  .search-bar {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    transition:
      border-color 0.15s ease,
      box-shadow 0.15s ease;
  }

  .search-bar:focus-within {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(108, 142, 245, 0.2);
  }

  .search-bar__icon {
    flex-shrink: 0;
    font-size: var(--font-size-md);
    opacity: 0.5;
  }

  .search-bar__input {
    flex: 1;
    border: none;
    outline: none;
    background: transparent;
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
    color: var(--color-text-primary);
    min-width: 0;
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
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    border-radius: var(--radius-full);
    background-color: var(--color-surface-raised);
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: var(--font-size-md);
    line-height: 1;
    transition:
      background-color 0.15s ease,
      color 0.15s ease;
  }

  .search-bar__clear:hover {
    background-color: var(--color-border);
    color: var(--color-text-primary);
  }
</style>
