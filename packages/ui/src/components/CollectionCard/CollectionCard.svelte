<script lang="ts">
  import type { CollectionCardProps } from './CollectionCard.types'

  let { id: _id, name, description, itemCount, updatedAt, onclick }: CollectionCardProps = $props()

  function formatRelativeDate(timestamp: number): string {
    const now = Date.now()
    const diff = now - timestamp
    const seconds = Math.floor(diff / 1000)
    const minutes = Math.floor(seconds / 60)
    const hours = Math.floor(minutes / 60)
    const days = Math.floor(hours / 24)

    if (days > 0) return `hace ${days} ${days === 1 ? 'dia' : 'dias'}`
    if (hours > 0) return `hace ${hours} ${hours === 1 ? 'hora' : 'horas'}`
    if (minutes > 0) return `hace ${minutes} ${minutes === 1 ? 'minuto' : 'minutos'}`
    return 'hace un momento'
  }

  const itemLabel = $derived(itemCount === 1 ? 'item' : 'items')
  const relativeDate = $derived(formatRelativeDate(updatedAt))
</script>

<button class="collection-card" type="button" {onclick}>
  <div class="collection-card__header">
    <h3 class="collection-card__name">{name}</h3>
    <span class="collection-card__badge">{itemCount} {itemLabel}</span>
  </div>

  {#if description}
    <p class="collection-card__description" data-testid="collection-description">
      {description}
    </p>
  {/if}

  <span class="collection-card__date" data-testid="collection-date">
    {relativeDate}
  </span>
</button>

<style>
  .collection-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-4);
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    transition:
      border-color 0.15s ease,
      box-shadow 0.15s ease;
    text-align: left;
    width: 100%;
    font-family: var(--font-sans);
    color: var(--color-text-primary);
  }

  .collection-card:hover {
    border-color: var(--color-text-muted);
    box-shadow: var(--shadow-md);
  }

  .collection-card:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  .collection-card__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .collection-card__name {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-bold);
    color: var(--color-text-primary);
  }

  .collection-card__badge {
    flex-shrink: 0;
    padding: var(--space-1) var(--space-2);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-accent);
    background-color: var(--color-surface-raised);
    border-radius: var(--radius-full);
  }

  .collection-card__description {
    margin: 0;
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .collection-card__date {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }
</style>
