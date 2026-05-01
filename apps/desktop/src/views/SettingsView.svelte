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
  import { Button, Card, Input } from '@entropia/ui'

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
  let saveFeedback = $state<{ tone: 'success' | 'error'; text: string } | null>(null)

  let currentModeLabel = $derived(
    llmMode === 'local' ? 'Local' : llmMode === 'openrouter' ? 'OpenRouter' : 'Automático'
  )

  let currentModeDescription = $derived(
    llmMode === 'local'
      ? 'Procesa con el modelo local disponible en este equipo.'
      : llmMode === 'openrouter'
        ? 'Usa exclusivamente tu cuenta remota de OpenRouter.'
        : 'Prioriza el modelo local y cae a OpenRouter cuando haga falta.'
  )

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
      testResult = { success: false, message: 'Ingresá una API key antes de probar la conexión.' }
      return
    }
    testing = true
    testResult = null
    try {
      const models = await testOpenrouterConnection(apiKey.trim())
      availableModels = models
      testResult = {
        success: true,
        message: `Conexión lista · ${models.length} modelos disponibles.`,
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
    saveFeedback = null
    try {
      await Promise.all([
        settingsSet(SETTINGS_KEYS.OPENROUTER_API_KEY, apiKey.trim()),
        settingsSet(SETTINGS_KEYS.OPENROUTER_MODEL, model),
        settingsSet(SETTINGS_KEYS.LLM_MODE, llmMode),
      ])
      maskedApiKey = maskKey(apiKey)
      saveFeedback = {
        tone: 'success',
        text: 'Configuración guardada. Ya podés usar esta preferencia en toda la app.',
      }
      setTimeout(() => {
        saveFeedback = null
      }, 3000)
    } catch (e) {
      saveFeedback = {
        tone: 'error',
        text: `Error: ${e instanceof Error ? e.message : String(e)}`,
      }
    } finally {
      saving = false
    }
  }

  function handleModelSelect(modelId: string) {
    model = modelId
  }
</script>

<div class="settings-view page-shell">
  <section class="page-header settings-view__header">
    <div class="page-header__content">
      <span class="page-header__eyebrow">Preferencias</span>
      <h1>Configuración</h1>
      <p>Ajustá cómo EntropIA resuelve tareas locales y remotas de inteligencia artificial.</p>
      <span class="page-header__meta">Modo actual: {currentModeLabel}</span>
    </div>

    <div class="page-toolbar settings-view__toolbar">
      <Button variant="primary" onclick={handleSave} disabled={saving}>
        {saving ? 'Guardando...' : 'Guardar cambios'}
      </Button>
    </div>
  </section>

  {#if saveFeedback}
    <p
      class="surface-message"
      class:surface-message--error={saveFeedback.tone === 'error'}
      class:surface-message--success={saveFeedback.tone === 'success'}
    >
      {saveFeedback.text}
    </p>
  {/if}

  <Card>
    <section class="settings-card-section">
      <div class="settings-card-section__copy">
        <h2>Modo LLM</h2>
        <p>{currentModeDescription}</p>
      </div>

      <div class="settings__mode-options">
        <label class="settings__radio" class:active={llmMode === 'local'}>
          <input type="radio" name="llm_mode" value="local" bind:group={llmMode} />
          <div class="settings__radio-content">
            <strong>Local</strong>
            <span class="settings__radio-desc">
              Gemma local vía llama.cpp. Sin conexión a internet.
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
              API remota. Requiere API key y conexión a internet.
            </span>
          </div>
        </label>

        <label class="settings__radio" class:active={llmMode === 'auto'}>
          <input type="radio" name="llm_mode" value="auto" bind:group={llmMode} />
          <div class="settings__radio-content">
            <strong>Automático</strong>
            <span class="settings__radio-desc">
              Prioriza el motor local y usa OpenRouter sólo como respaldo.
            </span>
          </div>
        </label>
      </div>
    </section>
  </Card>

  <Card>
    <section class="settings-card-section">
      <div class="settings-card-section__copy">
        <h2>OpenRouter</h2>
        <p>Configurá la cuenta remota, validá el acceso y elegí el modelo por defecto.</p>
      </div>

      <div class="settings__field settings__field--stacked">
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
            title={showApiKey ? 'Ocultar API key' : 'Mostrar API key'}
            aria-label={showApiKey ? 'Ocultar API key' : 'Mostrar API key'}
          >
            {showApiKey ? '🙈' : '👁'}
          </button>
          <Button
            variant="secondary"
            size="sm"
            onclick={handleTestConnection}
            disabled={testing || !apiKey.trim()}
          >
            {testing ? 'Probando...' : 'Probar conexión'}
          </Button>
        </div>

        {#if maskedApiKey}
          <p class="settings__hint">Clave cargada: {maskedApiKey}</p>
        {/if}

        {#if testResult}
          <p
            class="surface-message settings__feedback"
            class:surface-message--success={testResult.success}
            class:surface-message--error={!testResult.success}
          >
            {testResult.message}
          </p>
        {/if}
      </div>

      <div class="settings__field settings__field--stacked">
        <Input label="Modelo" type="text" bind:value={model} placeholder="google/gemma-3-4b-it" />

        {#if availableModels.length > 0}
          <div class="settings__model-list">
            <p class="settings__model-list-title">Modelos sugeridos desde OpenRouter</p>
            {#each availableModels
              .filter((m) => m.id.includes('gemma') || m.id.includes('llama') || m.id.includes('mistral') || m.id.includes('qwen') || m.id.includes('claude') || m.id.includes('gpt'))
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
  </Card>
</div>

<style>
  .settings-view {
    min-height: 100%;
  }

  .settings-view__toolbar {
    justify-content: flex-end;
    flex: 1;
  }

  .settings-card-section {
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .settings-card-section__copy {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .settings-card-section__copy p,
  .settings__hint {
    font-size: var(--font-size-sm);
    color: var(--color-text-secondary);
  }

  .settings__mode-options {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .settings__radio {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    background: var(--color-surface);
    transition:
      border-color 0.15s ease,
      background-color 0.15s ease,
      box-shadow 0.15s ease,
      transform 0.15s ease;
  }

  .settings__radio:hover {
    background: var(--color-surface-raised);
    transform: translateY(-1px);
  }

  .settings__radio.active {
    border-color: var(--color-accent);
    background: color-mix(in srgb, var(--color-accent) 8%, var(--color-surface));
    box-shadow: var(--shadow-sm);
  }

  .settings__radio input[type='radio'] {
    margin-top: 3px;
    accent-color: var(--color-accent);
  }

  .settings__radio-content {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .settings__radio-content strong {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .settings__radio-desc {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    line-height: 1.5;
  }

  .settings__badge {
    display: inline-block;
    margin-left: var(--space-2);
    padding: 2px 8px;
    border-radius: var(--radius-full);
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
    margin-bottom: var(--space-1);
  }

  .settings__field--stacked {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .settings__label {
    display: block;
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-secondary);
    margin-bottom: var(--space-1);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .settings__input-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
  }

  .settings__input {
    flex: 1;
    min-height: var(--control-height-md);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
    color: var(--color-text-primary);
    font-family: var(--font-mono, monospace);
    font-size: var(--font-size-sm);
  }

  .settings__input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: var(--focus-ring);
    background: var(--color-surface);
  }

  .settings__icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height-md);
    height: var(--control-height-md);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: 14px;
  }

  .settings__icon-btn:hover {
    background: var(--color-surface-elevated);
  }

  .settings__feedback {
    margin: 0;
  }

  .settings__model-list {
    max-height: 240px;
    overflow-y: auto;
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    background: var(--color-surface);
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
    background: transparent;
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
    background: color-mix(in srgb, var(--color-accent) 10%, var(--color-surface));
    font-weight: var(--font-weight-medium);
  }

  .settings__model-option + .settings__model-option {
    border-top: 1px solid var(--color-border-subtle);
  }

  .settings__model-id {
    color: var(--color-text-primary);
  }

  .settings__model-ctx {
    color: var(--color-text-muted);
    font-size: var(--font-size-xs);
  }

  @media (max-width: 720px) {
    .settings-view__toolbar,
    .settings__input-row {
      width: 100%;
    }

    .settings-view__toolbar :global(.btn),
    .settings__input-row :global(.btn) {
      width: 100%;
    }

    .settings__icon-btn {
      flex: 0 0 auto;
    }
  }
</style>
