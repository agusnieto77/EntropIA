<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
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

  async function handleReset() {
    if (
      !confirm(
        '¿Estás seguro? Esto eliminará el entorno virtual y todas las dependencias instaladas.',
      )
    )
      return
    errorBanner = null
    try {
      await resetDeps()
      const [checkResults, uv] = await Promise.all([checkAllDeps(), getUvStatus()])
      deps = checkResults
      uvStatus = uv
    } catch (e) {
      errorBanner = String(e)
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
        return 'var(--color-success, #22c55e)'
      case 'missing':
        return 'var(--color-error, #ef4444)'
      case 'failed':
        return 'var(--color-warning, #f59e0b)'
      default:
        return 'var(--color-text-muted, #6b7280)'
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
          {#if dep.status.type === 'missing'}
            <Button
              variant="secondary"
              size="sm"
              onclick={() => handleInstallOne(dep.id)}
              disabled={installing}
            >
              Instalar
            </Button>
          {:else if dep.status.type === 'failed'}
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
  <div class="deps-danger-zone">
    <Button variant="danger" onclick={handleReset} disabled={installing}>
      Resetear entorno
    </Button>
    <p class="deps-danger-zone__hint">
      Elimina el entorno virtual y todas las dependencias instaladas. Requiere reinstalación.
    </p>
  </div>
</div>

<style>
  .deps-tab {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
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
    background: rgba(239, 68, 68, 0.1);
    border: 1px solid rgba(239, 68, 68, 0.3);
    color: #b91c1c;
  }

  .deps-banner--success {
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(34, 197, 94, 0.3);
    color: #15803d;
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
    color: var(--color-text-muted, #6b7280);
    font-family: var(--font-mono, monospace);
  }

  .deps-uv-status__text--warn {
    color: var(--color-warning, #f59e0b);
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
    background: var(--color-border-subtle, #e5e7eb);
    border-radius: var(--radius-full, 9999px);
    overflow: hidden;
  }

  .deps-progress__bar--sm {
    flex: none;
    width: 120px;
    height: 4px;
  }

  .deps-progress__fill {
    height: 100%;
    background: var(--color-accent, #6366f1);
    border-radius: var(--radius-full, 9999px);
    transition: width 0.3s ease;
  }

  .deps-progress__label {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted, #6b7280);
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
    border: 1px solid var(--color-border-subtle, #e5e7eb);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .deps-row--failed {
    border-color: rgba(245, 158, 11, 0.4);
    background: rgba(245, 158, 11, 0.04);
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
    color: var(--color-text-secondary, #6b7280);
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
    color: var(--color-text-muted, #6b7280);
  }

  .deps-row__error-toggle {
    background: none;
    border: none;
    cursor: pointer;
    font-size: var(--font-size-xs);
    color: var(--color-warning, #f59e0b);
    padding: 0;
    text-decoration: underline;
    text-align: left;
  }

  .deps-row__error-detail {
    font-size: 11px;
    font-family: var(--font-mono, monospace);
    background: var(--color-surface-sunken, #f3f4f6);
    border: 1px solid var(--color-border, #d1d5db);
    border-radius: var(--radius-sm, 4px);
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
    border-radius: var(--radius-full, 9999px);
    font-size: 10px;
    font-weight: var(--font-weight-medium, 500);
    vertical-align: middle;
  }

  .deps-badge--required {
    background: rgba(99, 102, 241, 0.12);
    color: #4f46e5;
  }

  .deps-badge--version {
    background: rgba(34, 197, 94, 0.12);
    color: #15803d;
    font-family: var(--font-mono, monospace);
  }

  /* Empty state */
  .deps-empty {
    font-size: var(--font-size-sm);
    color: var(--color-text-muted, #6b7280);
    text-align: center;
    padding: var(--space-6) 0;
  }

  /* Disk estimate */
  .deps-disk-estimate {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted, #6b7280);
    margin: 0;
    padding: var(--space-2) 0;
    border-top: 1px solid var(--color-border-subtle, #e5e7eb);
  }

  /* Danger zone */
  .deps-danger-zone {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border: 1px solid rgba(239, 68, 68, 0.2);
    border-radius: var(--radius-md);
    background: rgba(239, 68, 68, 0.04);
  }

  .deps-danger-zone__hint {
    font-size: var(--font-size-xs);
    color: var(--color-text-muted, #6b7280);
    margin: 0;
    flex: 1;
  }
</style>
