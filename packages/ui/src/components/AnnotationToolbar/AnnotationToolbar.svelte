<script lang="ts">
  import type { AnnotationTool } from '../DocumentViewer/DocumentViewer.types'

  export interface AnnotationColorOption {
    value: string
    label: string
  }

  interface AnnotationToolbarProps {
    tool: AnnotationTool
    color: string
    hasSelection: boolean
    colors: AnnotationColorOption[]
    onToolChange?: (tool: AnnotationTool) => void
    onColorChange?: (color: string) => void
    onDeleteSelected?: () => void
  }

  let {
    tool,
    color,
    hasSelection,
    colors,
    onToolChange = () => {},
    onColorChange = () => {},
    onDeleteSelected = () => {},
  }: AnnotationToolbarProps = $props()

  const toolOptions: Array<{ value: AnnotationTool; label: string; short: string }> = [
    { value: 'select', label: 'Select annotation tool', short: '↖' },
    { value: 'rectangle', label: 'Rectangle annotation tool', short: '▭' },
    { value: 'underline', label: 'Underline annotation tool', short: '▁' },
  ]
</script>

<div
  class="annotation-toolbar"
  data-testid="annotation-toolbar"
  role="toolbar"
  aria-label="Image annotations"
>
  <div class="annotation-toolbar__group">
    {#each toolOptions as option (option.value)}
      <button
        type="button"
        class="annotation-toolbar__button"
        class:annotation-toolbar__button--active={tool === option.value}
        aria-label={option.label}
        aria-pressed={tool === option.value}
        onclick={() => onToolChange(option.value)}
      >
        {option.short}
      </button>
    {/each}
  </div>

  <div class="annotation-toolbar__group">
    {#each colors as option (option.value)}
      <button
        type="button"
        class="annotation-toolbar__swatch"
        class:annotation-toolbar__swatch--active={color === option.value}
        aria-label={`${option.label} annotation color`}
        aria-pressed={color === option.value}
        title={option.label}
        onclick={() => onColorChange(option.value)}
      >
        <span class="annotation-toolbar__swatch-fill" style={`background:${option.value}`}></span>
      </button>
    {/each}
  </div>

  <button
    type="button"
    class="annotation-toolbar__button annotation-toolbar__button--danger"
    aria-label="Delete selected annotation"
    disabled={!hasSelection}
    onclick={onDeleteSelected}
  >
    ✕
  </button>
</div>

<style>
  .annotation-toolbar {
    position: absolute;
    top: var(--space-3);
    right: var(--space-3);
    z-index: 3;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface-raised) 92%, transparent);
    box-shadow: var(--shadow-md);
    backdrop-filter: blur(10px);
  }

  .annotation-toolbar__group {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .annotation-toolbar__button,
  .annotation-toolbar__swatch {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text-primary);
    cursor: pointer;
    transition:
      background-color 0.15s ease,
      border-color 0.15s ease,
      transform 0.15s ease;
  }

  .annotation-toolbar__button:hover:not(:disabled),
  .annotation-toolbar__swatch:hover:not(:disabled) {
    background: var(--color-surface-raised);
    border-color: var(--color-text-secondary);
  }

  .annotation-toolbar__button:disabled,
  .annotation-toolbar__swatch:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .annotation-toolbar__button--active,
  .annotation-toolbar__swatch--active {
    border-color: var(--color-accent);
    box-shadow: inset 0 0 0 1px var(--color-accent);
  }

  .annotation-toolbar__button--danger:disabled {
    border-color: var(--color-border);
  }

  .annotation-toolbar__button--danger:not(:disabled) {
    color: var(--color-danger);
  }

  .annotation-toolbar__swatch-fill {
    width: 14px;
    height: 14px;
    border-radius: var(--radius-full);
    border: 1px solid color-mix(in srgb, var(--color-text-primary) 35%, transparent);
  }

  @media (max-width: 720px) {
    .annotation-toolbar {
      left: var(--space-3);
      right: var(--space-3);
      justify-content: space-between;
      flex-wrap: wrap;
    }
  }
</style>
