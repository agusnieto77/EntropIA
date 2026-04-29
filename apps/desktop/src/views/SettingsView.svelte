<script lang="ts">
  import { onMount } from 'svelte'
  import {
    settingsGet,
    settingsSet,
    testOpenrouterConnection,
    SETTINGS_KEYS,
    DEFAULT_OPENROUTER_MODEL,
    DEFAULT_LLM_MODE,
    type LlmMode,
    type ModelInfo,
  } from '$lib/settings'
  import { llmIsAvailable } from '$lib/llm'
  import { Button } from '@entropia/ui'

  // State
  let apiKey = $state('')
  let maskedApiKey = $state('')
  let showApiKey = $state(false)
  let model = $state(DEFAULT_OPENROUTER_MODEL)
  let llmMode = $state<LlmMode>(DEFAULT_LLM_MODE)
  let localAvailable = $state(false)

  // Test connection state
  let testing = $state(false)
  let testResult = $state<{ success: boolean; message: string } | null>(null)
  let availableModels = $state<ModelInfo[]>([])

  // Save state
  let saving = $state(false)
  let saveMessage = $state<string | null>(null)

  onMount(async () => {
    const [storedKey, storedModel, storedMode, isAvail] = await Promise.all([
      settingsGet(SETTINGS_KEYS.OPENROUTER_API_KEY),
      settingsGet(SETTINGS_KEYS.OPENROUTER_MODEL),
      settingsGet(SETTINGS_KEYS.LLM_MODE),
      llmIsAvailable(),
    ])

    if (storedKey) {
      apiKey = storedKey
      maskedApiKey = maskKey(storedKey)
    }
    if (storedModel) model = storedModel
    if (storedMode) llmMode = storedMode as LlmMode
    localAvailable = isAvail
  })

  function maskKey(key: string): string {
    if (key.length <= 8) return '*'.repeat(key.length)
    return key.slice(0, 4) + '*'.repeat(key.length - 8) + key.slice(-4)
  }

  async function handleTestConnection() {
    if (!apiKey.trim()) {
      testResult = { success: false, message: 'Ingresá una API key primero' }
      return
    }
    testing = true
    testResult = null
    try {
      const models = await testOpenrouterConnection(apiKey.trim())
      availableModels = models
      testResult = {
        success: true,
        message: `Conexion exitosa — ${models.length} modelos disponibles`,
      }
    } catch (e) {
      testResult = {
        success: false,
        message: e instanceof Error ? e.message : String(e),
      }
    } finally {
      testing = false
    }
  }

  async function handleSave() {
    saving = true
    saveMessage = null
    try {
      await Promise.all([
        settingsSet(SETTINGS_KEYS.OPENROUTER_API_KEY, apiKey.trim()),
        settingsSet(SETTINGS_KEYS.OPENROUTER_MODEL, model),
        settingsSet(SETTINGS_KEYS.LLM_MODE, llmMode),
      ])
      maskedApiKey = maskKey(apiKey)
      saveMessage = 'Configuracion guardada'
      setTimeout(() => {
        saveMessage = null
      }, 3000)
    } catch (e) {
      saveMessage = `Error: ${e instanceof Error ? e.message : String(e)}`
    } finally {
      saving = false
    }
  }

  function handleModelSelect(modelId: string) {
    model = modelId
  }
</script>

<div class="settings">
  <h1 class="settings__title">Configuracion</h1>

  <!-- LLM Mode -->
  <section class="settings__section">
    <h2 class="settings__section-title">Modo LLM</h2>
    <p class="settings__description">
      Elegí cómo procesar las tareas de inteligencia artificial.
    </p>

    <div class="settings__mode-options">
      <label class="settings__radio" class:active={llmMode === 'local'}>
        <input type="radio" name="llm_mode" value="local" bind:group={llmMode} />
        <div class="settings__radio-content">
          <strong>Local</strong>
          <span class="settings__radio-desc">
            Gemma local via llama.cpp. Sin conexion a internet.
            {#if localAvailable}
              <span class="settings__badge settings__badge--ok">Disponible</span>
            {:else}
              <span class="settings__badge settings__badge--warn">Modelo no encontrado</span>
            {/if}
          </span>
        </div>
      </label>

      <label class="settings__radio" class:active={llmMode === 'openrouter'}>
        <input type="radio" name="llm_mode" value="openrouter" bind:group={llmMode} />
        <div class="settings__radio-content">
          <strong>OpenRouter</strong>
          <span class="settings__radio-desc">
            API remota. Requiere API key y conexion a internet.
          </span>
        </div>
      </label>

      <label class="settings__radio" class:active={llmMode === 'auto'}>
        <input type="radio" name="llm_mode" value="auto" bind:group={llmMode} />
        <div class="settings__radio-content">
          <strong>Auto</strong>
          <span class="settings__radio-desc">
            Intenta local primero, si no esta disponible usa OpenRouter.
          </span>
        </div>
      </label>
    </div>
  </section>

  <!-- OpenRouter Config -->
  <section class="settings__section">
    <h2 class="settings__section-title">OpenRouter</h2>
    <p class="settings__description">
      Configura tu cuenta de OpenRouter para usar modelos remotos.
    </p>

    <!-- API Key -->
    <div class="settings__field">
      <label class="settings__label" for="api-key">API Key</label>
      <div class="settings__input-row">
        {#if showApiKey}
          <input
            id="api-key"
            type="text"
            class="settings__input"
            bind:value={apiKey}
            placeholder="sk-or-v1-..."
          />
        {:else}
          <input
            id="api-key"
            type="password"
            class="settings__input"
            bind:value={apiKey}
            placeholder="sk-or-v1-..."
          />
        {/if}
        <button
          class="settings__icon-btn"
          type="button"
          onclick={() => (showApiKey = !showApiKey)}
          title={showApiKey ? 'Ocultar' : 'Mostrar'}
        >
          {showApiKey ? '🙈' : '👁'}
        </button>
        <Button
          variant="secondary"
          size="sm"
          onclick={handleTestConnection}
          disabled={testing || !apiKey.trim()}
        >
          {testing ? 'Probando...' : 'Probar conexion'}
        </Button>
      </div>
      {#if testResult}
        <p
          class="settings__test-result"
          class:success={testResult.success}
          class:error={!testResult.success}
        >
          {testResult.message}
        </p>
      {/if}
    </div>

    <!-- Model -->
    <div class="settings__field">
      <label class="settings__label" for="model">Modelo</label>
      <input
        id="model"
        type="text"
        class="settings__input"
        bind:value={model}
        placeholder="google/gemma-3-4b-it"
      />
      {#if availableModels.length > 0}
        <div class="settings__model-list">
          <p class="settings__model-list-title">Modelos populares disponibles:</p>
          {#each availableModels
            .filter((m) =>
              m.id.includes('gemma') ||
              m.id.includes('llama') ||
              m.id.includes('mistral') ||
              m.id.includes('qwen') ||
              m.id.includes('claude') ||
              m.id.includes('gpt')
            )
            .slice(0, 15) as m (m.id)}
            <button
              class="settings__model-option"
              type="button"
              class:selected={model === m.id}
              onclick={() => handleModelSelect(m.id)}
            >
              <span class="settings__model-id">{m.id}</span>
              <span class="settings__model-ctx">{Math.round(m.context_length / 1024)}k ctx</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </section>

  <!-- Save -->
  <div class="settings__actions">
    <Button variant="primary" onclick={handleSave} disabled={saving}>
      {saving ? 'Guardando...' : 'Guardar configuracion'}
    </Button>
    {#if saveMessage}
      <span class="settings__save-msg">{saveMessage}</span>
    {/if}
  </div>
</div>

<style>
  .settings {
    max-width: 680px;
    margin: 0 auto;
    padding: var(--space-4) 0;
  }
  .settings__title {
    font-size: var(--font-size-xl);
    font-weight: var(--font-weight-bold);
    margin-bottom: var(--space-6);
    color: var(--color-text-primary);
  }
  .settings__section {
    margin-bottom: var(--space-6);
    padding: var(--space-4);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
  }
  .settings__section-title {
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-semibold);
    margin-bottom: var(--space-1);
    color: var(--color-text-primary);
  }
  .settings__description {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
    margin-bottom: var(--space-4);
  }
  .settings__mode-options {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .settings__radio {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: border-color 0.15s ease, background-color 0.15s ease;
  }
  .settings__radio:hover {
    background: var(--color-surface-raised);
  }
  .settings__radio.active {
    border-color: var(--color-accent);
    background: rgba(108, 142, 245, 0.05);
  }
  .settings__radio input[type='radio'] {
    margin-top: 3px;
    accent-color: var(--color-accent);
  }
  .settings__radio-content {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .settings__radio-content strong {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }
  .settings__radio-desc {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }
  .settings__badge {
    display: inline-block;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: var(--font-weight-medium);
    vertical-align: middle;
  }
  .settings__badge--ok {
    background: rgba(34, 197, 94, 0.15);
    color: #16a34a;
  }
  .settings__badge--warn {
    background: rgba(234, 179, 8, 0.15);
    color: #a16207;
  }
  .settings__field {
    margin-bottom: var(--space-4);
  }
  .settings__label {
    display: block;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-primary);
    margin-bottom: var(--space-1);
  }
  .settings__input-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }
  .settings__input {
    flex: 1;
    padding: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    color: var(--color-text-primary);
    font-family: var(--font-mono, monospace);
    font-size: var(--font-size-sm);
  }
  .settings__input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(108, 142, 245, 0.2);
  }
  .settings__icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    cursor: pointer;
    font-size: 14px;
  }
  .settings__icon-btn:hover {
    background: var(--color-surface-raised);
  }
  .settings__test-result {
    margin-top: var(--space-2);
    font-size: var(--font-size-sm);
    padding: var(--space-2);
    border-radius: var(--radius-md);
  }
  .settings__test-result.success {
    background: rgba(34, 197, 94, 0.1);
    color: #16a34a;
  }
  .settings__test-result.error {
    background: rgba(239, 68, 68, 0.1);
    color: #dc2626;
  }
  .settings__model-list {
    margin-top: var(--space-2);
    max-height: 240px;
    overflow-y: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .settings__model-list-title {
    padding: var(--space-2) var(--space-3);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    border-bottom: 1px solid var(--color-border);
  }
  .settings__model-option {
    display: flex;
    justify-content: space-between;
    align-items: center;
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: none;
    background: none;
    cursor: pointer;
    font-family: var(--font-sans);
    font-size: var(--font-size-sm);
    text-align: left;
    transition: background-color 0.1s ease;
  }
  .settings__model-option:hover {
    background: var(--color-surface-raised);
  }
  .settings__model-option.selected {
    background: rgba(108, 142, 245, 0.1);
    font-weight: var(--font-weight-medium);
  }
  .settings__model-option + .settings__model-option {
    border-top: 1px solid var(--color-border);
  }
  .settings__model-id {
    color: var(--color-text-primary);
  }
  .settings__model-ctx {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }
  .settings__actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .settings__save-msg {
    font-size: var(--font-size-sm);
    color: #16a34a;
  }
</style>
