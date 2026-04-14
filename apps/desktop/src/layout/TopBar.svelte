<script lang="ts">
  import { navigation } from '$lib/navigation'
  import { Button } from '@entropia/ui'
</script>

<header class="topbar">
  {#if $navigation.canGoBack}
    <Button variant="ghost" size="sm" onclick={() => navigation.back()}>← Back</Button>
  {/if}
  <nav class="breadcrumb">
    {#each $navigation.breadcrumb as crumb, i}
      {#if i > 0}<span class="sep">/</span>{/if}
      <span class="crumb" class:last={i === $navigation.breadcrumb.length - 1}>{crumb}</span>
    {/each}
  </nav>
</header>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }
  .breadcrumb {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .crumb {
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .crumb.last {
    color: var(--color-text-primary);
    font-weight: var(--font-weight-medium);
  }
  .sep {
    color: var(--color-text-muted);
  }
</style>
