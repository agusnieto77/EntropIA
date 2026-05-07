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
  type RibbonTab = 'explorer' | 'search'
  let sidebarOpen = $state(true)
  let activeRibbonTab = $state<RibbonTab>('explorer')

  function toggleSidebar(tab?: RibbonTab) {
    if (tab && tab === activeRibbonTab && sidebarOpen) {
      sidebarOpen = false
    } else {
      if (tab) activeRibbonTab = tab
      sidebarOpen = true
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
    <!-- Ribbon: thin icon strip (Obsidian-style) -->
    <nav class="ribbon" aria-label="Navegación principal">
      <div class="ribbon__top">
        {#if showExplorer}
          <button
            class="ribbon__btn"
            class:ribbon__btn--active={activeRibbonTab === 'explorer' && sidebarOpen}
            onclick={() => toggleSidebar('explorer')}
            title="Explorador (Ctrl+B)"
            aria-label="Explorador"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
            </svg>
          </button>
        {/if}

        <button
          class="ribbon__btn"
          class:ribbon__btn--active={activeRibbonTab === 'search' && sidebarOpen}
          onclick={() => toggleSidebar('search')}
          title="Buscar"
          aria-label="Buscar"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="8"/>
            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
        </button>
      </div>

      <div class="ribbon__bottom">
        <button
          class="ribbon__btn"
          onclick={() => navigation.openRootSection({ name: 'settings' })}
          title="Configuración"
          aria-label="Configuración"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
        </button>
      </div>
    </nav>

    <!-- Sidebar: contextual content -->
    {#if sidebarOpen && showExplorer && activeRibbonTab === 'explorer'}
      <DocumentExplorer />
    {:else if sidebarOpen && activeRibbonTab === 'search'}
      <aside class="sidebar-placeholder">
        <p class="sidebar-placeholder__text">Usá la barra de búsqueda en el header</p>
      </aside>
    {/if}

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

  /* ── Ribbon (Obsidian-style icon strip) ── */
  .ribbon {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    width: 42px;
    flex-shrink: 0;
    background: var(--color-surface-sunken);
    border-right: 1px solid var(--color-border-subtle);
    padding: var(--space-2) 0;
  }

  .ribbon__top,
  .ribbon__bottom {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  .ribbon__btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 34px;
    height: 34px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: color var(--transition-base), background-color var(--transition-base);
  }

  .ribbon__btn:hover {
    color: var(--color-text-secondary);
    background: var(--color-accent-soft);
  }

  .ribbon__btn--active {
    color: var(--color-accent);
    background: var(--color-accent-soft);
  }

  /* ── Sidebar placeholder ── */
  .sidebar-placeholder {
    width: 220px;
    flex-shrink: 0;
    border-right: 1px solid var(--color-border-subtle);
    background: var(--color-surface);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-4);
  }

  .sidebar-placeholder__text {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    text-align: center;
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
