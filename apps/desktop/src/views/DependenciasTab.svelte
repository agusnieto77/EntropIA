<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte'
  import { Button } from '@entropia/ui'
  import {
    checkAllDeps,
    installAllDeps,
    installOneDep,
    getUvStatus,
    resetDeps,
    onDepsProgress,
    onDepsComplete,
    onDepsError,
    DEP_DISPLAY_NAMES,
    DEP_DESCRIPTIONS,
    CRITICAL_DEPS,
    type DepCheckResult,
    type DependencyId,
    type DependencyStatus,
    type UvStatusResult,
  } from '$lib/deps'

  // ---------------------------------------------------------------------------
  // State
  // ---------------------------------------------------------------------------

  let deps = $state<DepCheckResult[]>([])
  let uvStatus = $state<UvStatusResult | null>(null)
  let installing = $state(false)
  let errorBanner = $state<string | null>(null)
  let expandedErrors = $state<Set<DependencyId>>(new Set())
  let resetting = $state(false)
  let showResetConfirm = $state(false)
  let resetConfirmationValue = $state('')
  let resetConfirmationInput = $state<HTMLInputElement | null>(null)

  const RESET_CONFIRMATION_PHRASE = 'RESETEAR ENTORNO'

  // ---------------------------------------------------------------------------
  // Derived
  // ---------------------------------------------------------------------------

  let hasMissingOrFailed = $derived(
    deps.some((d) => d.status.type === 'missing' || d.status.type === 'failed'),
  )

  let allInstalled = $derived(deps.length > 0 && deps.every((d) => d.status.type === 'installed'))

  let overallProgress = $derived(() => {
    if (!installing || deps.length === 0) return 0
    const done = deps.filter(
      (d) => d.status.type === 'installed' || d.status.type === 'failed',
    ).length
    return Math.round((done / deps.length) * 100)
  })

  // ---------------------------------------------------------------------------
  // Event listeners
  // ---------------------------------------------------------------------------

  let unlisteners: Array<() => void> = []

  onMount(async () => {
    try {
      const [checkResults, uv] = await Promise.all([checkAllDeps(), getUvStatus()])
      deps = checkResults
      uvStatus = uv
    } catch (e) {
      errorBanner = `Error al verificar dependencias: ${String(e)}`
    }

    unlisteners.push(
      await onDepsProgress((event) => {
        deps = deps.map((d) => (d.id === event.id ? { ...d, status: event.status } : d))
      }),
      await onDepsComplete((event) => {
        deps = event.results
        installing = false
      }),
      await onDepsError((event) => {
        errorBanner = event.error
        installing = false
      }),
    )
  })

  onDestroy(() => {
    unlisteners.forEach((fn) => fn())
  })

  // ---------------------------------------------------------------------------
  // Handlers
  // ---------------------------------------------------------------------------

  async function handleInstallAll() {
    installing = true
    errorBanner = null
    try {
      await installAllDeps()
    } catch (e) {
      errorBanner = String(e)
      installing = false
    }
  }

  async function handleInstallOne(id: DependencyId) {
    deps = deps.map((d) =>
      d.id === id ? { ...d, status: { type: 'installing', percent: 0 } } : d,
    )
    try {
      const result = await installOneDep(id)
      deps = deps.map((d) => (d.id === id ? result : d))
    } catch (e) {
      deps = deps.map((d) =>
        d.id === id ? { ...d, status: { type: 'failed', message: String(e) } } : d,
      )
    }
  }

  async function handleResetDialogOpen() {
    resetConfirmationValue = ''
    showResetConfirm = true
    await tick()
    resetConfirmationInput?.focus()
  }

  function handleResetDialogCancel() {
    if (resetting) return
    showResetConfirm = false
    resetConfirmationValue = ''
  }

  async function handleReset() {
    if (resetConfirmationValue.trim() !== RESET_CONFIRMATION_PHRASE) return

    resetting = true
    errorBanner = null
    try {
      await resetDeps()
      const [checkResults, uv] = await Promise.all([checkAllDeps(), getUvStatus()])
      deps = checkResults
      uvStatus = uv
      showResetConfirm = false
      resetConfirmationValue = ''
    } catch (e) {
      errorBanner = String(e)
    } finally {
      resetting = false
    }
  }

  function toggleError(id: DependencyId) {
    const next = new Set(expandedErrors)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    expandedErrors = next
  }

  // ---------------------------------------------------------------------------
  // Display helpers
  // ---------------------------------------------------------------------------

  function statusIcon(status: DependencyStatus): string {
    switch (status.type) {
      case 'installed':
        return '✓'
      case 'missing':
        return '✗'
      case 'installing':
      case 'checking':
        return '⏳'
      case 'failed':
        return '⚠'
      default:
        return '?'
    }
  }

  function statusColor(status: DependencyStatus): string {
    switch (status.type) {
      case 'installed':
        return 'var(--color-success)'
      case 'missing':
        return 'var(--color-danger)'
      case 'failed':
        return 'var(--color-warning)'
      default:
        return 'var(--color-text-muted)'
    }
  }

  function isCritical(id: DependencyId): boolean {
    return CRITICAL_DEPS.includes(id)
  }

  function getInstalledVersion(dep: DepCheckResult): string | null {
    if (dep.status.type === 'installed') return dep.status.version ?? dep.version
    return dep.version
  }

  function getInstallingPercent(status: DependencyStatus): number {
    if (status.type === 'installing') return status.percent
    return 0
  }

  function getFailedMessage(status: DependencyStatus): string {
    if (status.type === 'failed') return status.message
    return ''
  }

  function supportsInstallOne(id: DependencyId): boolean {
    return id !== 'Python'
  }

  let isResetConfirmationValid = $derived(
    resetConfirmationValue.trim() === RESET_CONFIRMATION_PHRASE,
  )
</script>

<div class="deps-tab">
  <!-- Error banner -->
  {#if errorBanner}
    <div class="deps-banner deps-banner--error">
      <span class="deps-banner__message">{errorBanner}</span>
      <button
        class="deps-banner__dismiss"
        type="button"
        onclick={() => (errorBanner = null)}
        aria-label="Cerrar error"
      >
        ✕
      </button>
    </div>
  {/if}

  <!-- UV status row -->
  <div class="deps-uv-status">
    {#if uvStatus}
      {#if uvStatus.uv_ready}
        <span class="deps-uv-status__text">
          uv {uvStatus.uv_version ?? ''} · {uvStatus.uv_path ?? ''}
          {#if uvStatus.venv_exists}
            · entorno virtual en {uvStatus.venv_path ?? ''}
          {:else}
            · sin entorno virtual
          {/if}
        </span>
      {:else}
        <span class="deps-uv-status__text deps-uv-status__text--warn">
          uv no instalado — las dependencias no pueden gestionarse automáticamente
        </span>
      {/if}
    {:else}
      <span class="deps-uv-status__text">Verificando uv...</span>
    {/if}
  </div>

  <!-- Install all button -->
  {#if hasMissingOrFailed && !installing}
    <div class="deps-actions">
      <Button variant="primary" onclick={handleInstallAll} disabled={installing}>
        Instalar todo
      </Button>
    </div>
  {/if}

  <!-- Progress bar -->
  {#if installing}
    <div class="deps-progress">
      <div class="deps-progress__bar">
        <div
          class="deps-progress__fill"
          style="width: {overallProgress()}%"
        ></div>
      </div>
      <span class="deps-progress__label">{overallProgress()}% instalado</span>
    </div>
  {/if}

  <!-- All installed banner -->
  {#if allInstalled && !installing}
    <div class="deps-banner deps-banner--success">
      <span class="deps-banner__message">
        Todas las dependencias están instaladas y listas para usar.
      </span>
    </div>
  {/if}

  <!-- Dependency list -->
  <div class="deps-list">
    {#each deps as dep (dep.id)}
      <div class="deps-row" class:deps-row--failed={dep.status.type === 'failed'}>
        <!-- Status icon -->
        <span class="deps-row__icon" style="color: {statusColor(dep.status)}">
          {statusIcon(dep.status)}
        </span>

        <!-- Name + description -->
        <div class="deps-row__info">
          <div class="deps-row__name-line">
            <strong class="deps-row__name">{DEP_DISPLAY_NAMES[dep.id]}</strong>
            {#if isCritical(dep.id)}
              <span class="deps-badge deps-badge--required">Requerido</span>
            {/if}
            {#if dep.status.type === 'installed'}
              {@const version = getInstalledVersion(dep)}
              {#if version}
                <span class="deps-badge deps-badge--version">{version}</span>
              {/if}
            {/if}
          </div>
          <p class="deps-row__desc">{DEP_DESCRIPTIONS[dep.id]}</p>

          <!-- Installing progress per-item -->
          {#if dep.status.type === 'installing'}
            <div class="deps-row__progress">
              <div class="deps-progress__bar deps-progress__bar--sm">
                <div
                  class="deps-progress__fill"
                  style="width: {getInstallingPercent(dep.status)}%"
                ></div>
              </div>
              <span class="deps-row__progress-pct">{getInstallingPercent(dep.status)}%</span>
            </div>
          {/if}

          <!-- Error detail (expandable) -->
          {#if dep.status.type === 'failed'}
            <button
              class="deps-row__error-toggle"
              type="button"
              onclick={() => toggleError(dep.id)}
            >
              {expandedErrors.has(dep.id) ? 'Ocultar detalle' : 'Ver detalle del error'}
            </button>
            {#if expandedErrors.has(dep.id)}
              <pre class="deps-row__error-detail">{getFailedMessage(dep.status)}</pre>
            {/if}
          {/if}
        </div>

        <!-- Action button -->
        <div class="deps-row__action">
          {#if supportsInstallOne(dep.id) && dep.status.type === 'missing'}
            <Button
              variant="secondary"
              size="sm"
              onclick={() => handleInstallOne(dep.id)}
              disabled={installing}
            >
              Instalar
            </Button>
          {:else if supportsInstallOne(dep.id) && dep.status.type === 'failed'}
            <Button
              variant="secondary"
              size="sm"
              onclick={() => handleInstallOne(dep.id)}
              disabled={installing}
            >
              Reintentar
            </Button>
          {/if}
        </div>
      </div>
    {/each}

    {#if deps.length === 0 && !errorBanner}
      <p class="deps-empty">Verificando dependencias...</p>
    {/if}
  </div>

  <!-- Disk space estimate -->
  <p class="deps-disk-estimate">
    Espacio estimado en disco: ~2.5 GB (incluye modelos de IA y entorno virtual Python)
  </p>

  <!-- Reset button -->
  <section class="deps-danger-zone" aria-labelledby="deps-danger-zone-title">
    <div class="deps-danger-zone__content">
      <p class="deps-danger-zone__eyebrow">Danger zone</p>
      <h3 id="deps-danger-zone-title" class="deps-danger-zone__title">Resetear entorno</h3>
      <p class="deps-danger-zone__hint">
        Elimina el entorno virtual local y las dependencias de IA instaladas en esta máquina.
        Después vas a tener que reinstalarlas para volver a usar estas funciones.
      </p>
    </div>

    <div class="deps-danger-zone__action">
      <Button variant="danger" onclick={handleResetDialogOpen} disabled={installing || resetting}>
        Resetear entorno
      </Button>
    </div>
  </section>

  {#if showResetConfirm}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div class="modal-overlay" onclick={handleResetDialogCancel} role="presentation">
      <div
        class="modal deps-reset-modal"
        tabindex="-1"
        role="dialog"
        aria-modal="true"
        aria-labelledby="deps-reset-modal-title"
        aria-describedby="deps-reset-modal-description"
        onclick={(event) => event.stopPropagation()}
        onkeydown={(event) => {
          if (event.key === 'Escape') handleResetDialogCancel()
        }}
      >
        <div class="deps-reset-modal__header">
          <span class="deps-reset-modal__eyebrow">Acción destructiva</span>
          <h3 id="deps-reset-modal-title" class="modal-title">Resetear entorno</h3>
        </div>
        <p id="deps-reset-modal-description" class="modal-message">
          Esto elimina el entorno virtual local y borra las dependencias de IA instaladas.
          Después vas a tener que reinstalarlas para volver a usar estas funciones.
        </p>

        <div class="deps-reset-modal__warning" role="note" aria-label="Impacto del reseteo">
          <p class="deps-reset-modal__warning-title">Esta acción no se puede deshacer desde la app.</p>
          <p class="deps-reset-modal__warning-copy">
            Para confirmar que entendés el impacto, escribí exactamente esta frase:
          </p>
          <code class="deps-reset-modal__phrase">{RESET_CONFIRMATION_PHRASE}</code>
        </div>

        <label class="deps-reset-modal__label" for="deps-reset-confirmation-input">
          Escribí la frase exacta
        </label>
        <input
          id="deps-reset-confirmation-input"
          bind:this={resetConfirmationInput}
          bind:value={resetConfirmationValue}
          class="deps-reset-modal__input"
          type="text"
          spellcheck="false"
          autocomplete="off"
          autocapitalize="off"
          placeholder={RESET_CONFIRMATION_PHRASE}
          disabled={resetting}
        />
        <p class="deps-reset-modal__hint">La confirmación distingue espacios y mayúsculas.</p>

        <div class="modal-actions">
          <Button variant="secondary" onclick={handleResetDialogCancel} disabled={resetting}>
            Cancelar
          </Button>
          <Button variant="danger" onclick={handleReset} disabled={!isResetConfirmationValid || resetting}>
            {resetting ? 'Reseteando...' : 'Resetear entorno'}
          </Button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .deps-tab {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .deps-tab {
    --deps-panel-bg: color-mix(in srgb, var(--color-surface-raised) 58%, transparent);
    --deps-panel-border: color-mix(in srgb, var(--color-hairline) 72%, transparent);
  }

  /* Banner */
  .deps-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    font-size: var(--font-size-sm);
  }

  .deps-banner--error {
    background: var(--color-danger-soft);
    border: 1px solid color-mix(in srgb, var(--color-danger) 28%, transparent);
    color: var(--color-danger);
  }

  .deps-banner--success {
    background: var(--color-success-soft);
    border: 1px solid color-mix(in srgb, var(--color-success) 28%, transparent);
    color: var(--color-success);
  }

  .deps-banner__message {
    flex: 1;
  }

  .deps-banner__dismiss {
    background: none;
    border: none;
    cursor: pointer;
    font-size: var(--font-size-sm);
    color: inherit;
    padding: 0 var(--space-1);
    opacity: 0.7;
  }

  .deps-banner__dismiss:hover {
    opacity: 1;
  }

  /* UV status */
  .deps-uv-status {
    padding: var(--space-2) 0;
  }

  .deps-uv-status__text {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    font-family: var(--font-mono, monospace);
  }

  .deps-uv-status__text--warn {
    color: var(--color-warning);
  }

  /* Actions */
  .deps-actions {
    display: flex;
    gap: var(--space-3);
  }

  /* Progress bar */
  .deps-progress {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .deps-progress__bar {
    flex: 1;
    height: 6px;
    background: var(--color-border-subtle);
    border-radius: var(--radius-full);
    overflow: hidden;
  }

  .deps-progress__bar--sm {
    flex: none;
    width: 120px;
    height: 4px;
  }

  .deps-progress__fill {
    height: 100%;
    background: var(--color-accent);
    border-radius: var(--radius-full);
    transition: width 0.3s ease;
  }

  .deps-progress__label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    white-space: nowrap;
  }

  /* Dep list */
  .deps-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .deps-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--deps-panel-border);
    border-radius: var(--radius-md);
    background: var(--deps-panel-bg);
  }

  .deps-row--failed {
    border-color: color-mix(in srgb, var(--color-warning) 34%, transparent);
    background: color-mix(in srgb, var(--color-warning) 10%, var(--deps-panel-bg));
  }

  .deps-row__icon {
    font-size: 16px;
    line-height: 1.5;
    flex: 0 0 auto;
    margin-top: 2px;
  }

  .deps-row__info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .deps-row__name-line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .deps-row__name {
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
  }

  .deps-row__desc {
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    margin: 0;
  }

  .deps-row__progress {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-1);
  }

  .deps-row__progress-pct {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .deps-row__error-toggle {
    background: none;
    border: none;
    cursor: pointer;
    font-size: var(--font-size-xs);
    color: var(--color-warning);
    padding: 0;
    text-decoration: underline;
    text-align: left;
  }

  .deps-row__error-detail {
    font-size: 11px;
    font-family: var(--font-mono, monospace);
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-hairline);
    border-radius: var(--radius-sm);
    padding: var(--space-2) var(--space-3);
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 160px;
    overflow-y: auto;
    margin: 0;
    color: var(--color-text-primary);
  }

  .deps-row__action {
    flex: 0 0 auto;
  }

  /* Badges */
  .deps-badge {
    display: inline-block;
    padding: 2px 7px;
    border-radius: var(--radius-full);
    font-size: 10px;
    font-weight: var(--font-weight-medium);
    vertical-align: middle;
  }

  .deps-badge--required {
    background: var(--color-accent-faint);
    color: var(--color-accent-hover);
  }

  .deps-badge--version {
    background: var(--color-success-soft);
    color: var(--color-success);
    font-family: var(--font-mono, monospace);
  }

  /* Empty state */
  .deps-empty {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted);
    text-align: center;
    padding: var(--space-6) 0;
  }

  /* Disk estimate */
  .deps-disk-estimate {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
    margin: 0;
    padding: var(--space-2) 0;
    border-top: 1px solid var(--color-hairline);
  }

  /* Danger zone */
  .deps-danger-zone {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-4);
    border: 1px solid color-mix(in srgb, var(--color-danger) 24%, transparent);
    border-left: 4px solid color-mix(in srgb, var(--color-danger) 72%, transparent);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-danger) 10%, var(--deps-panel-bg));
  }

  .deps-danger-zone__content {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    flex: 1;
    min-width: 0;
  }

  .deps-danger-zone__action {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .deps-danger-zone__eyebrow {
    margin: 0;
    font-size: 11px;
    font-weight: var(--font-weight-semibold);
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--color-danger);
  }

  .deps-danger-zone__title {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .deps-danger-zone__hint {
    margin: 0;
    font-size: var(--font-size-xs);
    line-height: var(--line-height-relaxed);
    color: var(--color-text-secondary);
  }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background-color: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: var(--space-4);
  }

  .modal {
    width: min(100%, 520px);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-5);
    border-radius: var(--radius-xl);
    border: 1px solid var(--color-border-strong);
    background:
      linear-gradient(180deg, color-mix(in srgb, var(--color-danger-soft) 46%, transparent), transparent 36%),
      var(--color-surface-raised);
    box-shadow: var(--shadow-overlay);
  }

  .modal-title {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .modal-message {
    margin: 0;
    font-size: var(--font-size-sm);
    line-height: var(--line-height-relaxed);
    color: var(--color-text-secondary);
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .deps-reset-modal__label {
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-secondary);
  }

  .deps-reset-modal__header {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .deps-reset-modal__eyebrow {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: var(--radius-full);
    background: color-mix(in srgb, var(--color-danger) 12%, transparent);
    color: var(--color-danger);
    font-size: 11px;
    font-weight: var(--font-weight-semibold);
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  .deps-reset-modal__warning {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid color-mix(in srgb, var(--color-danger) 22%, transparent);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-danger-soft) 68%, transparent);
  }

  .deps-reset-modal__warning-title {
    margin: 0;
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-semibold);
    color: var(--color-text-primary);
  }

  .deps-reset-modal__warning-copy {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
    line-height: var(--line-height-relaxed);
  }

  .deps-reset-modal__phrase {
    display: block;
    width: 100%;
    box-sizing: border-box;
    padding: var(--space-3);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-hairline);
    background: var(--color-surface-raised);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    font-family: var(--font-mono, monospace);
    font-weight: var(--font-weight-semibold);
    text-align: center;
  }

  .deps-reset-modal__input {
    width: 100%;
    min-height: var(--control-height-md);
    padding: 0 var(--space-3);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-hairline);
    background-color: color-mix(in srgb, var(--color-surface-sunken) 88%, transparent);
    color: var(--color-text-primary);
    font-size: var(--font-size-sm);
    box-sizing: border-box;
  }

  .deps-reset-modal__input:focus,
  .deps-reset-modal__input:focus-visible {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: var(--focus-ring);
    background-color: var(--color-surface);
  }

  .deps-reset-modal__input:disabled {
    cursor: not-allowed;
    opacity: 0.56;
  }

  .deps-reset-modal__hint {
    margin: calc(var(--space-2) * -1) 0 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }

  @media (max-width: 640px) {
    .deps-danger-zone {
      flex-direction: column;
    }

    .deps-danger-zone__action {
      width: 100%;
    }

    .deps-danger-zone__action :global(.btn) {
      width: 100%;
    }

    .modal-actions {
      flex-direction: column-reverse;
    }

    .modal-actions :global(.btn) {
      width: 100%;
    }
  }
</style>
