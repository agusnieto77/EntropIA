<script lang="ts">
  let {
    title,
    description,
    wikiUrl = undefined,
    wikiLabel = 'Más información',
  }: {
    title: string
    description: string
    wikiUrl?: string
    wikiLabel?: string
  } = $props()

  let open = $state(false)
  let triggerEl: HTMLButtonElement | undefined = $state()
  let popoverEl: HTMLDivElement | undefined = $state()

  function toggle() {
    open = !open
  }

  function close() {
    open = false
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close()
  }

  function handleClickOutside(e: MouseEvent) {
    if (
      open &&
      triggerEl &&
      popoverEl &&
      !triggerEl.contains(e.target as Node) &&
      !popoverEl.contains(e.target as Node)
    ) {
      close()
    }
  }

  $effect(() => {
    if (open) {
      document.addEventListener('click', handleClickOutside, true)
      document.addEventListener('keydown', handleKeydown)
      return () => {
        document.removeEventListener('click', handleClickOutside, true)
        document.removeEventListener('keydown', handleKeydown)
      }
    }
  })
</script>

<span class="help-popover">
  <button
    bind:this={triggerEl}
    class="help-popover__trigger"
    type="button"
    aria-label="Ayuda: {title}"
    aria-expanded={open}
    onclick={toggle}
  >
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="10" />
      <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
      <line x1="12" y1="17" x2="12.01" y2="17" />
    </svg>
  </button>

  {#if open}
    <div bind:this={popoverEl} class="help-popover__panel" role="tooltip">
      <div class="help-popover__title">{title}</div>
      <p class="help-popover__desc">{description}</p>
      {#if wikiUrl}
        <a
          class="help-popover__link"
          href={wikiUrl}
          target="_blank"
          rel="noopener noreferrer"
        >
          {wikiLabel} →
        </a>
      {/if}
    </div>
  {/if}
</span>

<style>
  .help-popover {
    position: relative;
    display: inline-flex;
    align-items: center;
  }

  .help-popover__trigger {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: color var(--transition-base), background-color var(--transition-base);
  }

  .help-popover__trigger:hover {
    color: var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 10%, transparent);
  }

  .help-popover__trigger[aria-expanded='true'] {
    color: var(--color-accent);
  }

  .help-popover__panel {
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    z-index: 300;
    width: max-content;
    max-width: 280px;
    padding: var(--space-3) var(--space-4);
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.04), transparent),
      var(--color-surface-elevated);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.2);
  }

  .help-popover__title {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
    margin-bottom: var(--space-1);
  }

  .help-popover__desc {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin: 0;
  }

  .help-popover__link {
    display: inline-block;
    margin-top: var(--space-2);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-accent);
    text-decoration: none;
    transition: color var(--transition-base);
  }

  .help-popover__link:hover {
    text-decoration: underline;
  }
</style>
