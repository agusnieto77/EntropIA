<script lang="ts">
  import { onMount } from 'svelte'
  import { initDb } from '$lib/db'
  import { navigation } from '$lib/navigation'
  import { setupKeyboardShortcuts } from '$lib/keyboard'
  import AppShell from './layout/AppShell.svelte'
  import CollectionsView from './views/CollectionsView.svelte'
  import CollectionView from './views/CollectionView.svelte'
  import ItemView from './views/ItemView.svelte'
  import SettingsView from './views/SettingsView.svelte'

  let ready = $state(false)
  let error = $state<string | null>(null)

  onMount(() => {
    console.log('[App] onMount starting')
    const cleanupKeyboard = setupKeyboardShortcuts()

    initDb()
      .then(() => {
        console.log('[App] initDb complete')
        ready = true
      })
      .catch((e) => {
        console.log('[App] initDb ERROR:', e)
        error = e instanceof Error ? e.message : 'Failed to initialize database'
      })

    return cleanupKeyboard
  })
</script>

{#if !ready && !error}
  <div class="loading"><p>Initializing...</p></div>
{:else if error}
  <div class="error"><p>{error}</p></div>
{:else}
  <AppShell>
    {#if $navigation.current.name === 'collections'}
      <CollectionsView />
    {:else if $navigation.current.name === 'collection'}
      <CollectionView collectionId={$navigation.current.id} />
    {:else if $navigation.current.name === 'item'}
      <ItemView
        itemId={$navigation.current.itemId}
        collectionId={$navigation.current.collectionId}
      />
    {:else if $navigation.current.name === 'settings'}
      <SettingsView />
    {/if}
  </AppShell>
{/if}

<style>
  .loading,
  .error {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
  }
  .error p {
    color: var(--color-danger);
  }
</style>
