<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import TopBar from './TopBar.svelte'
  import type { Snippet } from 'svelte'

  const HLAB_URL = 'https://hlab.com.ar/'
  const GITHUB_REPO_URL = 'https://github.com/agusnieto77/EntropIA'

  let { children }: { children: Snippet } = $props()

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

<div class="shell">
  <TopBar />
  <main class="content">
    {@render children()}
  </main>
  <footer class="footer">
    <div class="footer__brand">
      <span class="footer__version">EntropIA β</span>
      <span class="footer__caption">Archivo, OCR y análisis asistido.</span>
    </div>

    <a
      class="footer__github"
      href={GITHUB_REPO_URL}
      onclick={openGithubRepo}
      aria-label="GitHub"
      title="Repositorio en GitHub"
    >
      <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
        <path
          d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49C3.78 14.2 3.31 12.73 3.31 12.73c-.36-.92-.88-1.16-.88-1.16-.72-.49.05-.48.05-.48.79.06 1.21.82 1.21.82.71 1.21 1.87.86 2.33.66.07-.51.28-.86.5-1.06-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.58.82-2.14-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82A7.64 7.64 0 0 1 8 4.77c.68 0 1.36.09 2 .27 1.53-1.03 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.14 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.06-.01 1.91-.01 2.17 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
        />
      </svg>
    </a>

    <p class="footer__credits">
      Desarrollado por
      <a href={HLAB_URL} onclick={openHlabWebsite} aria-label="HLab"><b>HLab</b></a>
    </p>
  </footer>
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

  .footer {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: var(--space-3);
    border-top: 1px solid var(--color-border-subtle);
    padding: var(--space-3) var(--space-4);
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.02), transparent), var(--color-surface);
  }

  .footer__brand {
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
  }

  .footer__version {
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .footer__caption {
    color: var(--color-text-muted);
  }

  .footer__credits {
    margin: 0;
    text-align: right;
  }

  .footer a {
    color: var(--color-accent);
    text-decoration: none;
  }

  .footer__github {
    justify-self: center;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    color: var(--color-text-secondary);
    transition:
      color var(--transition-base),
      background-color var(--transition-base),
      border-color var(--transition-base),
      box-shadow var(--transition-base);
  }

  .footer__github svg {
    width: 18px;
    height: 18px;
    fill: currentColor;
  }

  .footer__github:hover {
    color: var(--color-accent);
    background: var(--color-surface-elevated);
    border-color: var(--color-border-strong);
  }

  .footer__github:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .footer a:hover {
    text-decoration: underline;
  }

  @media (max-width: 720px) {
    .footer {
      grid-template-columns: 1fr;
      gap: var(--space-2);
      justify-items: center;
      text-align: center;
    }

    .footer__brand,
    .footer__credits {
      text-align: center;
    }
  }
</style>
