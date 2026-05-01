<script lang="ts">
  import type { ButtonProps } from './Button.types'

  let {
    variant = 'primary',
    size = 'md',
    iconOnly = false,
    disabled = false,
    loading = false,
    type = 'button',
    children,
    ...rest
  }: ButtonProps = $props()

  let isDisabled = $derived(disabled || loading)
</script>

<button
  class="btn btn--{variant} btn--{size}"
  class:btn--icon-only={iconOnly}
  class:btn--loading={loading}
  {type}
  disabled={isDisabled}
  aria-busy={loading}
  {...rest}
>
  {#if loading}
    <span class="btn__spinner" aria-hidden="true"></span>
  {/if}
  <span class="btn__label" class:btn__label--hidden={loading}>
    {#if children}
      {@render children()}
    {/if}
  </span>
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: var(--control-height-md);
    padding: 0 var(--space-4);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font-family: var(--font-sans);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    line-height: var(--line-height-tight);
    cursor: pointer;
    transition:
      background-color var(--transition-base),
      border-color var(--transition-base),
      color var(--transition-base),
      box-shadow var(--transition-base),
      transform var(--transition-base);
    position: relative;
    white-space: nowrap;
    user-select: none;
    box-shadow: none;
  }

  .btn:disabled {
    cursor: not-allowed;
    opacity: 0.48;
    transform: none;
  }

  .btn:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .btn:hover:not(:disabled) {
    transform: translateY(-1px);
  }

  /* ─── Variants ─── */
  .btn--primary {
    background-color: var(--color-accent);
    color: #ffffff;
    border-color: var(--color-accent);
    box-shadow: 0 8px 18px rgba(124, 149, 255, 0.2);
  }
  .btn--primary:hover:not(:disabled) {
    background-color: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
  }

  .btn--secondary {
    background-color: var(--color-surface-raised);
    color: var(--color-text-primary);
    border-color: var(--color-border-subtle);
  }
  .btn--secondary:hover:not(:disabled) {
    background-color: var(--color-surface-elevated);
    border-color: var(--color-border-strong);
  }

  .btn--ghost {
    background-color: transparent;
    color: var(--color-text-secondary);
    border-color: transparent;
  }
  .btn--ghost:hover:not(:disabled) {
    background-color: var(--color-surface-raised);
    color: var(--color-text-primary);
  }

  .btn--danger {
    background-color: var(--color-danger);
    color: #ffffff;
    border-color: var(--color-danger);
    box-shadow: 0 8px 18px rgba(225, 109, 123, 0.18);
  }
  .btn--danger:hover:not(:disabled) {
    background-color: var(--color-danger-hover);
    border-color: var(--color-danger-hover);
  }

  /* ─── Sizes ─── */
  .btn--sm {
    min-height: var(--control-height-sm);
    padding: 0 var(--space-3);
    font-size: var(--font-size-xs);
  }
  .btn--md {
    min-height: var(--control-height-md);
    padding: 0 var(--space-4);
    font-size: var(--font-size-sm);
  }
  .btn--lg {
    min-height: var(--control-height-lg);
    padding: 0 var(--space-6);
    font-size: var(--font-size-lg);
  }

  /* ─── Spinner ─── */
  .btn__spinner {
    width: 14px;
    height: 14px;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: var(--radius-full);
    animation: spin 0.6s linear infinite;
    position: absolute;
  }

  .btn__label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }

  .btn__label--hidden {
    visibility: hidden;
  }

  .btn--loading {
    cursor: wait;
  }

  .btn--icon-only {
    gap: 0;
    aspect-ratio: 1;
    padding: 0;
    flex-shrink: 0;
  }

  .btn--icon-only.btn--sm {
    width: var(--control-height-sm);
  }

  .btn--icon-only.btn--md {
    width: var(--control-height-md);
  }

  .btn--icon-only.btn--lg {
    width: var(--control-height-lg);
  }

  .btn--icon-only :global(svg) {
    flex-shrink: 0;
    pointer-events: none;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
