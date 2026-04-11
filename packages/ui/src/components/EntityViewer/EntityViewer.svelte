<script lang="ts">
  import type { Entity, EntityType } from './EntityViewer.types'
  import { ENTITY_TYPE_LABELS } from './EntityViewer.types'

  interface Props {
    entities: Entity[]
    onhighlight?: (detail: { startOffset: number; endOffset: number }) => void
  }

  let { entities, onhighlight }: Props = $props()

  // Group entities by type, preserving order: person, place, date, institution, custom
  const TYPE_ORDER: EntityType[] = ['person', 'place', 'date', 'institution', 'custom']

  const grouped = $derived(
    TYPE_ORDER.reduce<Map<EntityType, Entity[]>>((acc, type) => {
      const group = entities.filter((e) => e.entityType === type)
      if (group.length > 0) acc.set(type, group)
      return acc
    }, new Map())
  )

  function handlePillClick(entity: Entity) {
    if (entity.startOffset != null && entity.endOffset != null) {
      onhighlight?.({ startOffset: entity.startOffset, endOffset: entity.endOffset })
    }
  }
</script>

{#if entities.length === 0}
  <div class="entity-viewer__empty" data-testid="entity-viewer-empty">
    <span class="entity-viewer__empty-icon" aria-hidden="true">&#128270;</span>
    <p class="entity-viewer__empty-text">No entities extracted yet.</p>
  </div>
{:else}
  <div class="entity-viewer">
    {#each TYPE_ORDER as type}
      {#if grouped.has(type)}
        <div class="entity-viewer__group" data-testid="entity-group">
          <span class="entity-viewer__group-label entity-viewer__group-label--{type}">
            {ENTITY_TYPE_LABELS[type]}
          </span>
          <div class="entity-viewer__pills">
            {#each grouped.get(type) ?? [] as entity (entity.id)}
              <button
                type="button"
                class="entity-viewer__pill entity-viewer__pill--{type}"
                onclick={() => handlePillClick(entity)}
                title={entity.value}
              >
                {entity.value}
              </button>
            {/each}
          </div>
        </div>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .entity-viewer {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .entity-viewer__empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-4);
    color: var(--color-text-muted);
    text-align: center;
  }

  .entity-viewer__empty-icon {
    font-size: 24px;
    opacity: 0.4;
  }

  .entity-viewer__empty-text {
    font-size: var(--font-size-sm);
    margin: 0;
  }

  .entity-viewer__group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .entity-viewer__group-label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--color-text-muted);
  }

  .entity-viewer__pills {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .entity-viewer__pill {
    display: inline-flex;
    align-items: center;
    padding: 2px var(--space-2);
    border-radius: var(--radius-full);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    font-family: var(--font-sans);
    cursor: pointer;
    border: none;
    transition:
      opacity 0.15s ease,
      transform 0.1s ease;
  }

  .entity-viewer__pill:hover {
    opacity: 0.85;
    transform: scale(1.02);
  }

  .entity-viewer__pill:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  /* Color-coded pills per entity type */
  .entity-viewer__pill--person {
    background-color: #dbeafe;
    color: #1e40af;
  }

  .entity-viewer__pill--place {
    background-color: #dcfce7;
    color: #166534;
  }

  .entity-viewer__pill--date {
    background-color: #fef9c3;
    color: #854d0e;
  }

  .entity-viewer__pill--institution {
    background-color: #fce7f3;
    color: #9d174d;
  }

  .entity-viewer__pill--custom {
    background-color: var(--color-surface-raised);
    color: var(--color-text-secondary);
  }
</style>
