<script lang="ts">
  import type { MetadataEditorProps } from './MetadataEditor.types'

  let { value, onchange }: MetadataEditorProps = $props()

  interface MetadataRow {
    key: string
    value: string
  }

  let rows = $state<MetadataRow[]>(
    value ? Object.entries(value).map(([key, val]) => ({ key, value: val })) : []
  )

  function serializeRows(): Record<string, string> {
    const result: Record<string, string> = {}
    for (const row of rows) {
      if (row.key || row.value) {
        result[row.key] = row.value
      }
    }
    return result
  }

  function handleKeyInput(index: number, e: Event) {
    const target = e.target as HTMLInputElement
    rows[index]!.key = target.value
    onchange?.(serializeRows())
  }

  function handleValueInput(index: number, e: Event) {
    const target = e.target as HTMLInputElement
    rows[index]!.value = target.value
    onchange?.(serializeRows())
  }

  function addRow() {
    rows.push({ key: '', value: '' })
  }

  function deleteRow(index: number) {
    rows.splice(index, 1)
    onchange?.(serializeRows())
  }
</script>

<div class="metadata-editor">
  {#each rows as row, index}
    <div class="metadata-editor__row">
      <input
        class="metadata-editor__input metadata-editor__key"
        type="text"
        placeholder="Key"
        value={row.key}
        oninput={(e: Event) => handleKeyInput(index, e)}
      />
      <input
        class="metadata-editor__input metadata-editor__value"
        type="text"
        placeholder="Value"
        value={row.value}
        oninput={(e: Event) => handleValueInput(index, e)}
      />
      <button
        class="metadata-editor__delete"
        type="button"
        data-testid="metadata-delete"
        onclick={() => deleteRow(index)}
        aria-label="Remove field"
      >
        &times;
      </button>
    </div>
  {/each}

  <button class="metadata-editor__add" type="button" data-testid="metadata-add" onclick={addRow}>
    + Add field
  </button>
</div>

<style>
  .metadata-editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .metadata-editor__row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .metadata-editor__input {
    flex: 1;
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-sans);
    font-size: var(--font-size-sm);
    color: var(--color-text-primary);
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    outline: none;
    transition: border-color 0.15s ease;
  }

  .metadata-editor__input::placeholder {
    color: var(--color-text-muted);
  }

  .metadata-editor__input:focus {
    border-color: var(--color-accent);
  }

  .metadata-editor__key {
    flex: 0.4;
    font-weight: var(--font-weight-medium);
  }

  .metadata-editor__value {
    flex: 0.6;
  }

  .metadata-editor__delete {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background-color: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
    font-size: var(--font-size-lg);
    line-height: 1;
    transition:
      background-color 0.15s ease,
      color 0.15s ease,
      border-color 0.15s ease;
  }

  .metadata-editor__delete:hover {
    background-color: var(--color-danger);
    border-color: var(--color-danger);
    color: #ffffff;
  }

  .metadata-editor__add {
    align-self: flex-start;
    padding: var(--space-1) var(--space-3);
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-md);
    background-color: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
    font-family: var(--font-sans);
    font-size: var(--font-size-sm);
    transition:
      border-color 0.15s ease,
      color 0.15s ease;
  }

  .metadata-editor__add:hover {
    border-color: var(--color-accent);
    color: var(--color-accent);
  }
</style>
