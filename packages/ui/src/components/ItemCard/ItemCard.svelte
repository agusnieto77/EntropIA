<script lang="ts">
  import type { ItemCardProps } from './ItemCard.types'

  let {
    id: _id,
    title,
    assetCount,
    thumbnailPath,
    metadataPreview,
    onclick,
  }: ItemCardProps = $props()

  const assetLabel = $derived(assetCount === 1 ? 'asset' : 'assets')
</script>

<button class="item-card" type="button" {onclick}>
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

<style>
  .item-card {
    display: flex;
    flex-direction: column;
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition:
      border-color 0.15s ease,
      box-shadow 0.15s ease;
    overflow: hidden;
    text-align: left;
    width: 100%;
    font-family: var(--font-sans);
    color: var(--color-text-primary);
    padding: 0;
  }

  .item-card:hover {
    border-color: var(--color-text-muted);
    box-shadow: var(--shadow-md);
  }

  .item-card:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
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
</style>
