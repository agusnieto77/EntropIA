<script lang="ts">
  import type { CardProps } from './Card.types'

  let {
    padding = 'md',
    hoverable = false,
    header,
    children,
    footer,
  }: CardProps = $props()
</script>

<div class="card card--padding-{padding}" class:card--hoverable={hoverable}>
  {#if header}
    <div class="card__header">
      {@render header()}
    </div>
  {/if}

  {#if children}
    <div class="card__body">
      {@render children()}
    </div>
  {/if}

  {#if footer}
    <div class="card__footer">
      {@render footer()}
    </div>
  {/if}
</div>

<style>
  .card {
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
    transition: box-shadow 0.15s ease, border-color 0.15s ease;
  }

  .card--hoverable:hover {
    box-shadow: var(--shadow-md);
    border-color: var(--color-text-muted);
  }

  /* ─── Padding ─── */
  .card--padding-sm .card__header,
  .card--padding-sm .card__body,
  .card--padding-sm .card__footer {
    padding: var(--space-3);
  }

  .card--padding-md .card__header,
  .card--padding-md .card__body,
  .card--padding-md .card__footer {
    padding: var(--space-4);
  }

  .card--padding-lg .card__header,
  .card--padding-lg .card__body,
  .card--padding-lg .card__footer {
    padding: var(--space-6);
  }

  /* ─── Sections ─── */
  .card__header {
    border-bottom: 1px solid var(--color-border);
    font-family: var(--font-sans);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-primary);
  }

  .card__body {
    font-family: var(--font-sans);
    color: var(--color-text-primary);
  }

  .card__footer {
    border-top: 1px solid var(--color-border);
    font-family: var(--font-sans);
    color: var(--color-text-secondary);
  }
</style>
