<script lang="ts">
  import type { InputProps } from './Input.types'

  let {
    value = $bindable(''),
    type = 'text',
    placeholder = '',
    disabled = false,
    label,
    error,
    hint,
  }: InputProps = $props()

  const inputId = $props.id()
  const hasError = $derived(!!error)

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement
    value = target.value
  }
</script>

<div class="input-field" class:input-field--disabled={disabled} class:input-field--error={hasError}>
  {#if label}
    <label class="input-field__label" for={inputId}>
      {label}
    </label>
  {/if}

  <input
    id={inputId}
    class="input-field__input"
    type={type}
    placeholder={placeholder}
    disabled={disabled}
    value={value}
    oninput={handleInput}
    aria-invalid={hasError}
    aria-describedby={error ? `${inputId}-error` : hint ? `${inputId}-hint` : undefined}
  />

  {#if error}
    <span class="input-field__error" id="{inputId}-error" role="alert">
      {error}
    </span>
  {:else if hint}
    <span class="input-field__hint" id="{inputId}-hint">
      {hint}
    </span>
  {/if}
</div>

<style>
  .input-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    width: 100%;
  }

  .input-field__label {
    font-family: var(--font-sans);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    color: var(--color-text-secondary);
    user-select: none;
  }

  .input-field__input {
    width: 100%;
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
    color: var(--color-text-primary);
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    outline: none;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
    box-sizing: border-box;
  }

  .input-field__input::placeholder {
    color: var(--color-text-muted);
  }

  .input-field__input:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(108, 142, 245, 0.2);
  }

  .input-field__input:disabled {
    cursor: not-allowed;
    opacity: 0.5;
    background-color: var(--color-bg);
  }

  .input-field--error .input-field__input {
    border-color: var(--color-danger);
  }

  .input-field--error .input-field__input:focus {
    box-shadow: 0 0 0 2px rgba(224, 92, 106, 0.2);
  }

  .input-field__error {
    font-family: var(--font-sans);
    font-size: var(--font-size-xs);
    color: var(--color-danger);
  }

  .input-field__hint {
    font-family: var(--font-sans);
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .input-field--disabled .input-field__label {
    opacity: 0.5;
  }
</style>
