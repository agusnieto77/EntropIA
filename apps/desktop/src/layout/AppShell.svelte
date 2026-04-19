<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import TopBar from './TopBar.svelte'
  import type { Snippet } from 'svelte'

  const HLAB_URL = 'https://hlab.com.ar/'

  let { children }: { children: Snippet } = $props()

  async function openHlabWebsite(event: MouseEvent) {
    event.preventDefault()

    try {
      await invoke('open_external_url', { url: HLAB_URL })
    } catch (error) {
      console.error('[Footer] No se pudo abrir el sitio de HLab', error)
    }
  }
</script>

<div class="shell">
  <TopBar />
  <main class="content">
    {@render children()}
  </main>
  <footer class="footer">
    <p>
      Desarrollado por
      <a href={HLAB_URL} onclick={openHlabWebsite}>HLab (Laboratorio de Humanidades Digitales)</a>
      <a href="https://hlab.com.ar/" target="_blank" rel="noopener noreferrer"
        >HLab (Laboratorio de Humanidades Digitales)</a
      >
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
    border-top: 1px solid var(--color-border);
    padding: var(--space-2) var(--space-4);
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    background: var(--color-surface);
  }

  .footer p {
    margin: 0;
  }

  .footer a {
    color: var(--color-accent);
    text-decoration: none;
  }

  .footer a:hover {
    text-decoration: underline;
  }
</style>
