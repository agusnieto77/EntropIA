<script lang="ts">
  import TopBar from './TopBar.svelte'
  import type { Snippet } from 'svelte'

  let { children }: { children: Snippet } = $props()
</script>

<div class="shell">
  <TopBar />
  <main class="content">
    {@render children()}
  </main>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-4);
  }
</style>
