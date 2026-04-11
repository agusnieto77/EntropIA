<script lang="ts">
  import type { ButtonProps } from './Button.types'

  let {
    variant = 'primary',
    size = 'md',
    disabled = false,
    loading = false,
    type = 'button',
    children,
    ...rest
  }: ButtonProps = $props()

  const isDisabled = $derived(disabled || loading)
</script>

<button
  class="btn btn--{variant} btn--{size}"
  class:btn--loading={loading}
  type={type}
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
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    font-family: var(--font-sans);
    font-weight: var(--font-weight-medium);
    cursor: pointer;
    transition: background-color 0.15s ease, border-color 0.15s ease, color 0.15s ease;
    position: relative;
    white-space: nowrap;
    user-select: none;
    line-height: 1;
  }

  .btn:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .btn:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  /* ─── Variants ─── */
  .btn--primary {
    background-color: var(--color-accent);
    color: #ffffff;
    border-color: var(--color-accent);
  }
  .btn--primary:hover:not(:disabled) {
    background-color: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
  }

  .btn--secondary {
    background-color: transparent;
    color: var(--color-text-primary);
    border-color: var(--color-border);
  }
  .btn--secondary:hover:not(:disabled) {
    background-color: var(--color-surface-raised);
    border-color: var(--color-text-muted);
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
  }
  .btn--danger:hover:not(:disabled) {
    background-color: #c94e5a;
    border-color: #c94e5a;
  }

  /* ─── Sizes ─── */
  .btn--sm {
    padding: var(--space-1) var(--space-3);
    font-size: var(--font-size-sm);
    height: 28px;
  }
  .btn--md {
    padding: var(--space-2) var(--space-4);
    font-size: var(--font-size-md);
    height: 36px;
  }
  .btn--lg {
    padding: var(--space-3) var(--space-6);
    font-size: var(--font-size-lg);
    height: 44px;
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

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
