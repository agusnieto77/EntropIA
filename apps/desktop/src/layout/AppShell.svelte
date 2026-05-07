<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { locale, t } from '$lib/i18n'
  import { navigation } from '$lib/navigation'
  import {
    getCachedDepsStatuses,
    onDepsComplete,
    CRITICAL_DEPS,
    type DepCheckResult,
  } from '$lib/deps'
  import DocumentExplorer from './DocumentExplorer.svelte'
  import TopBar from './TopBar.svelte'
  import EntropicConstellation from './EntropicConstellation.svelte'
  import type { Snippet } from 'svelte'

  const HLAB_URL = 'https://hlab.com.ar/'
  const GITHUB_REPO_URL = 'https://github.com/agusnieto77/EntropIA'

  let { children }: { children: Snippet } = $props()
  const currentLocale = locale
  const activeLocale = $derived($currentLocale)
  const showExplorer = $derived(
    $navigation.current.name === 'collection' || $navigation.current.name === 'item',
  )

  // ── Ribbon sidebar state ──
  let sidebarOpen = $state(true)
  let searchExpanded = $state(false)
  let searchFilter = $state('')
  let searchInputEl: HTMLInputElement | undefined = $state()

  function toggleSidebar() {
    sidebarOpen = !sidebarOpen
  }

  function expandSearch() {
    searchExpanded = true
    // Focus input on next tick
    setTimeout(() => searchInputEl?.focus(), 0)
  }

  function collapseSearch() {
    if (!searchFilter) {
      searchExpanded = false
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
      e.preventDefault()
      sidebarOpen = !sidebarOpen
    }
  }

  // ── Deps banner ──
  let depsResults = $state<DepCheckResult[]>([])
  const hasCriticalMissing = $derived(
    depsResults.some(
      (d) =>
        CRITICAL_DEPS.includes(d.id) &&
        (d.status.type === 'missing' || d.status.type === 'failed'),
    ),
  )

  let unlistenDepsComplete: (() => void) | undefined

  onMount(async () => {
    document.addEventListener('keydown', handleKeydown)

    unlistenDepsComplete = await onDepsComplete((event) => {
      depsResults = event.results ?? []
    })

    void getCachedDepsStatuses()
      .then((results) => {
        depsResults = results
      })
      .catch((e) => {
        console.error('[AppShell] cached deps fetch failed', e)
      })
  })

  onDestroy(() => {
    document.removeEventListener('keydown', handleKeydown)
    unlistenDepsComplete?.()
  })

  function goToDepSettings() {
    navigation.openRootSection({ name: 'settings' })
  }

  async function openHlabWebsite(event: MouseEvent) {
    event.preventDefault()
    try {
      await invoke('open_external_url', { url: HLAB_URL })
    } catch (error) {
      console.error('[Footer] No se pudo abrir el sitio de HLab', error)
    }
  }

  async function openGithubRepo(event: MouseEvent) {
    event.preventDefault()
    try {
      await invoke('open_external_url', { url: GITHUB_REPO_URL })
    } catch (error) {
      console.error('[Footer] No se pudo abrir el repositorio de GitHub', error)
    }
  }
</script>

<!-- Fondo constelación entrópica -->
<EntropicConstellation />

<div class="shell">
  <TopBar />

  <div class="workspace">
    <!-- Sidebar: always visible, collapses to icon strip -->
    <aside class="sidebar" class:sidebar--collapsed={!sidebarOpen} aria-label="Panel lateral">
      <!-- Sidebar toolbar -->
      <div class="sidebar__toolbar">
        <button
          class="sidebar__tool"
          onclick={toggleSidebar}
          title={sidebarOpen ? 'Colapsar panel (Ctrl+B)' : 'Expandir panel (Ctrl+B)'}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
        </button>

        {#if sidebarOpen}
          {#if searchExpanded}
            <input
              bind:this={searchInputEl}
              class="sidebar__search-input"
              type="text"
              placeholder="Filtrar colecciones..."
              bind:value={searchFilter}
              onblur={collapseSearch}
              onkeydown={(e) => { if (e.key === 'Escape') { searchFilter = ''; searchExpanded = false } }}
            />
          {:else}
            <div class="sidebar__toolbar-spacer"></div>
            <button
              class="sidebar__tool"
              onclick={expandSearch}
              title="Filtrar colecciones"
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
              </svg>
            </button>
          {/if}
        {/if}
      </div>

      <!-- Sidebar body (hidden when collapsed) -->
      {#if sidebarOpen}
        <div class="sidebar__body">
          {#if showExplorer}
            <DocumentExplorer />
          {:else}
            <div class="sidebar__placeholder">
              <p>Abrí una colección para ver el explorador</p>
            </div>
          {/if}
        </div>
      {/if}
    </aside>

    <main class="content">
      {#if hasCriticalMissing}
        <div class="deps-banner" role="alert">
          <span>⚠ Algunas funciones de IA no están disponibles.</span>
          <button class="deps-banner__btn" onclick={goToDepSettings}
            >Configurar dependencias →</button
          >
        </div>
      {/if}
      {@render children()}
    </main>
  </div>

  <!-- Status bar -->
  {#key activeLocale}
    <footer class="statusbar" data-locale={activeLocale}>
      <div class="statusbar__left">
        <span>EntropIA β</span>
        <span class="statusbar__sep">·</span>
        <span>{t('appshell.caption')}</span>
      </div>
      <div class="statusbar__center">
        <a
          class="statusbar__link"
          href={GITHUB_REPO_URL}
          onclick={openGithubRepo}
          aria-label={t('appshell.githubAria')}
          title={t('appshell.githubTitle')}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
            <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49C3.78 14.2 3.31 12.73 3.31 12.73c-.36-.92-.88-1.16-.88-1.16-.72-.49.05-.48.05-.48.79.06 1.21.82 1.21.82.71 1.21 1.87.86 2.33.66.07-.51.28-.86.5-1.06-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.58.82-2.14-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82A7.64 7.64 0 0 1 8 4.77c.68 0 1.36.09 2 .27 1.53-1.03 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.14 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.06-.01 1.91-.01 2.17 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"/>
          </svg>
        </a>
      </div>
      <div class="statusbar__right">
        <span>{t('appshell.developedBy')}
          <a class="statusbar__link" href={HLAB_URL} onclick={openHlabWebsite}><b>HLab</b></a>
        </span>
      </div>
    </footer>
  {/key}
</div>

<style>
  .shell {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    height: 100%;
    background: transparent;
  }

  /* ── Workspace: ribbon + sidebar + content ── */
  .workspace {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: var(--color-bg);
  }

  /* ── Sidebar (Zotero-style, always visible) ── */
  .sidebar {
    display: flex;
    flex-direction: column;
    width: 240px;
    flex-shrink: 0;
    border-right: 1px solid var(--color-border-subtle);
    background: var(--color-surface);
    overflow: hidden;
    transition: width var(--transition-base);
  }

  .sidebar--collapsed {
    width: 36px;
  }

  .sidebar__toolbar {
    display: flex;
    align-items: center;
    gap: 1px;
    padding: 3px 4px;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-surface-sunken);
    flex-shrink: 0;
  }

  .sidebar--collapsed .sidebar__toolbar {
    flex-direction: column;
    padding: 4px 3px;
  }

  .sidebar__toolbar-spacer {
    flex: 1;
  }

  .sidebar__tool {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: color var(--transition-base), background-color var(--transition-base);
  }

  .sidebar__tool:hover {
    color: var(--color-text-primary);
    background: var(--color-accent-soft);
  }

  .sidebar__tool--active {
    color: var(--color-accent);
  }

  .sidebar__search-input {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    background: var(--color-surface-raised);
    color: var(--color-text-primary);
    font-size: var(--font-size-xs);
    outline: none;
    transition: border-color var(--transition-base);
  }

  .sidebar__search-input:focus {
    border-color: var(--color-accent);
  }

  .sidebar__search-input::placeholder {
    color: var(--color-text-muted);
  }

  .sidebar__body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .sidebar__placeholder {
    padding: var(--space-6) var(--space-4);
    text-align: center;
  }

  .sidebar__placeholder p {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  /* ── Main content ── */
  .content {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: var(--space-5);
    background: var(--color-bg);
  }

  /* ── Deps banner ── */
  .deps-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    margin-bottom: var(--space-4);
    padding: var(--space-2) var(--space-4);
    background: var(--color-warning-soft);
    border: 1px solid color-mix(in srgb, var(--color-warning) 36%, transparent);
    border-radius: 4px;
    font-size: var(--font-size-sm);
    color: var(--color-warning);
  }

  .deps-banner__btn {
    flex-shrink: 0;
    padding: 2px var(--space-3);
    border: 1px solid color-mix(in srgb, var(--color-warning) 42%, transparent);
    border-radius: 2px;
    background: transparent;
    color: var(--color-warning);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background-color var(--transition-base);
  }

  .deps-banner__btn:hover {
    background: color-mix(in srgb, var(--color-warning) 14%, transparent);
  }

  /* ── Status bar (compact, replaces footer) ── */
  .statusbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 26px;
    padding: 0 var(--space-3);
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-surface-sunken);
    font-family: var(--font-mono);
    font-size: 0.6rem;
    color: var(--color-text-muted);
    flex-shrink: 0;
    letter-spacing: 0.02em;
  }

  .statusbar__left,
  .statusbar__center,
  .statusbar__right {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .statusbar__right {
    justify-content: flex-end;
  }

  .statusbar__sep {
    opacity: 0.4;
  }

  .statusbar__link {
    display: inline-flex;
    align-items: center;
    color: var(--color-text-muted);
    text-decoration: none;
    transition: color var(--transition-base);
  }

  .statusbar__link:hover {
    color: var(--color-accent);
  }

  .statusbar__link b {
    font-weight: 600;
  }
</style>
