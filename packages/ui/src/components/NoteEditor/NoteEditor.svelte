<script lang="ts">
  import type { NoteEditorProps } from './NoteEditor.types'

  let { content = '', placeholder = '', onsave, oncancel }: NoteEditorProps = $props()

  let internalContent = $state(content)
  const originalContent = content

  const isEmpty = $derived(internalContent.trim().length === 0)
  const isUnchanged = $derived(internalContent === originalContent)
  const isSaveDisabled = $derived(isEmpty || isUnchanged)

  function handleInput(e: Event) {
    const target = e.target as HTMLTextAreaElement
    internalContent = target.value
  }

  function handleSave() {
    onsave?.(internalContent)
  }

  function handleCancel() {
    oncancel?.()
  }
</script>

<div class="note-editor">
  <textarea
    class="note-editor__textarea"
    rows="3"
    {placeholder}
    value={internalContent}
    oninput={handleInput}
  ></textarea>

  <div class="note-editor__actions">
    <button
      class="note-editor__btn note-editor__btn--cancel"
      type="button"
      data-testid="note-cancel"
      onclick={handleCancel}
    >
      Cancel
    </button>
    <button
      class="note-editor__btn note-editor__btn--save"
      type="button"
      data-testid="note-save"
      disabled={isSaveDisabled}
      onclick={handleSave}
    >
      Save
    </button>
  </div>
</div>

<style>
  .note-editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .note-editor__textarea {
    width: 100%;
    min-height: 72px;
    padding: var(--space-3);
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
    color: var(--color-text-primary);
    background-color: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    outline: none;
    resize: vertical;
    transition: border-color 0.15s ease;
    box-sizing: border-box;
  }

  .note-editor__textarea::placeholder {
    color: var(--color-text-muted);
  }

  .note-editor__textarea:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px rgba(108, 142, 245, 0.2);
  }

  .note-editor__actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .note-editor__btn {
    padding: var(--space-2) var(--space-4);
    font-family: var(--font-sans);
    font-size: var(--font-size-sm);
    font-weight: var(--font-weight-medium);
    border-radius: var(--radius-md);
    cursor: pointer;
    border: 1px solid transparent;
    transition:
      background-color 0.15s ease,
      border-color 0.15s ease,
      color 0.15s ease;
  }

  .note-editor__btn:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .note-editor__btn--cancel {
    background-color: transparent;
    color: var(--color-text-secondary);
    border-color: var(--color-border);
  }

  .note-editor__btn--cancel:hover:not(:disabled) {
    background-color: var(--color-surface-raised);
    color: var(--color-text-primary);
  }

  .note-editor__btn--save {
    background-color: var(--color-accent);
    color: #ffffff;
    border-color: var(--color-accent);
  }

  .note-editor__btn--save:hover:not(:disabled) {
    background-color: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
  }
</style>
