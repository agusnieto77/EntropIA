<script lang="ts">
  import type { ItemCardProps } from './ItemCard.types'

  let {
    id: _id,
    title,
    assetCount,
    thumbnailPath,
    metadataPreview,
    onclick,
    onDelete,
  }: ItemCardProps = $props()

  const assetLabel = $derived(assetCount === 1 ? 'asset' : 'assets')
  const showDelete = $derived(!!onDelete)
</script>

<div class="item-card">
  <button class="item-card__main" type="button" {onclick}>
    <div class="item-card__thumbnail">
      {#if thumbnailPath}
        <img src={thumbnailPath} alt={title} class="item-card__img" />
      {:else}
        <div class="item-card__placeholder" data-testid="item-placeholder">
          <span class="item-card__placeholder-icon" aria-hidden="true">&#128196;</span>
        </div>
      {/if}
    </div>

    <div class="item-card__content">
      <span class="item-card__title">{title}</span>
      <span class="item-card__chip">{assetCount} {assetLabel}</span>
      {#if metadataPreview}
        <span class="item-card__metadata">{metadataPreview}</span>
      {/if}
    </div>
  </button>

  {#if showDelete}
    <button
      class="item-card__delete"
      type="button"
      aria-label={`Delete ${title}`}
      onclick={(e) => {
        e.stopPropagation()
        onDelete?.(e)
      }}
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M3 6h18" />
        <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
        <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
        <line x1="10" y1="11" x2="10" y2="17" />
        <line x1="14" y1="11" x2="14" y2="17" />
      </svg>
    </button>
  {/if}
</div>

<style>
  .item-card {
    display: flex;
    flex-direction: column;
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    transition:
      border-color 0.15s ease,
      box-shadow 0.15s ease;
    overflow: hidden;
    width: 100%;
    font-family: var(--font-sans);
    color: var(--color-text-primary);
    position: relative;
  }

  .item-card:hover,
  .item-card:focus-within {
    border-color: var(--color-text-muted);
    box-shadow: var(--shadow-md);
  }

  .item-card:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .item-card__main {
    display: flex;
    flex-direction: column;
    width: 100%;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    padding: 0;
    font-family: inherit;
    color: inherit;
  }

  .item-card__main:focus-visible {
    outline: none;
  }

  .item-card__thumbnail {
    width: 100%;
    height: 120px;
    overflow: hidden;
    background-color: var(--color-surface-raised);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .item-card__img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .item-card__placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
  }

  .item-card__placeholder-icon {
    font-size: 32px;
    opacity: 0.4;
  }

  .item-card__content {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-3);
  }

  .item-card__title {
    font-size: var(--font-size-md);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-primary);
  }

  .item-card__chip {
    display: inline-block;
    width: fit-content;
    padding: 2px var(--space-2);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-accent);
    background-color: var(--color-surface-raised);
    border-radius: var(--radius-full);
  }

  .item-card__metadata {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Delete button overlay */
  .item-card__delete {
    position: absolute;
    top: var(--space-2);
    right: var(--space-2);
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    color: var(--color-text-muted);
    cursor: pointer;
    opacity: 0;
    transition:
      opacity 0.15s ease,
      color 0.15s ease,
      background-color 0.15s ease,
      border-color 0.15s ease;
    padding: 0;
    z-index: 1;
  }

  .item-card:hover .item-card__delete,
  .item-card:focus-within .item-card__delete,
  .item-card__delete:focus-visible {
    opacity: 1;
  }

  .item-card__delete:hover {
    color: var(--color-danger);
    background-color: var(--color-surface-raised);
    border-color: var(--color-danger);
  }

  .item-card__delete:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }

  .item-card__delete svg {
    pointer-events: none;
  }
</style>
