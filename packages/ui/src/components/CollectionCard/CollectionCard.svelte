<script lang="ts">
  import type { CollectionCardProps } from './CollectionCard.types'

  let { id: _id, name, description, itemCount, updatedAt, onclick, onedit, ondelete }: CollectionCardProps = $props()

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

<div class="collection-card" role="button" tabindex="0" onclick={onclick} onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onclick?.(); } }}>
  <div class="collection-card__header">
    <h3 class="collection-card__name">{name}</h3>
    <span class="collection-card__badge">{itemCount} {itemLabel}</span>
    {#if onedit || ondelete}
      <div class="collection-card__actions">
        {#if onedit}
          <button
            class="collection-card__action-btn"
            type="button"
            title="Editar"
            data-testid="edit-button"
            onclick={(e: MouseEvent) => { e.stopPropagation(); onedit(); }}
          >
            <svg style="width: 16px; height: 16px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M17 3a2.83 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z"/>
              <path d="m15 5 4 4"/>
            </svg>
          </button>
        {/if}
        {#if ondelete}
          <button
            class="collection-card__action-btn collection-card__action-btn--danger"
            type="button"
            title="Eliminar"
            data-testid="delete-button"
            onclick={(e: MouseEvent) => { e.stopPropagation(); ondelete(); }}
          >
            <svg style="width: 16px; height: 16px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M3 6h18"/>
              <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/>
              <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>
              <line x1="10" y1="11" x2="10" y2="17"/>
              <line x1="14" y1="11" x2="14" y2="17"/>
            </svg>
          </button>
        {/if}
      </div>
    {/if}
  </div>

  {#if description}
    <p class="collection-card__description" data-testid="collection-description">
      {description}
    </p>
  {/if}

  <span class="collection-card__date" data-testid="collection-date">
    {relativeDate}
  </span>
</div>

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
    line-clamp: 2;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .collection-card__actions {
    display: flex;
    gap: var(--space-1);
    flex-shrink: 0;
  }

  .collection-card__action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-1);
    border: none;
    background: none;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: color 0.15s ease, background-color 0.15s ease;
  }

  .collection-card__action-btn:hover {
    color: var(--color-text-primary);
    background-color: var(--color-surface-raised);
  }

  .collection-card__action-btn--danger:hover {
    color: var(--color-danger, #ef4444);
  }

  .collection-card__date {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }
</style>
