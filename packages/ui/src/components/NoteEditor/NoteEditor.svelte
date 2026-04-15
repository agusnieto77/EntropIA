<script lang="ts">
  import type { NoteEditorProps } from './NoteEditor.types'

  let { content = '', placeholder = '', onsave }: NoteEditorProps = $props()

  let internalContent = $state('')
  let originalContent = $state('')
  let lastExternalContent = $state<string | undefined>(undefined)
  let textareaEl: HTMLTextAreaElement | undefined = $state(undefined)

  $effect(() => {
    if (lastExternalContent === undefined) {
      lastExternalContent = content
      originalContent = content
      internalContent = content
      return
    }

    if (content === lastExternalContent) {
      return
    }

    lastExternalContent = content
    originalContent = content
    internalContent = content
  })

  const isEmpty = $derived(internalContent.trim().length === 0)
  const isSaveDisabled = $derived(isEmpty)

  function handleInput(e: Event) {
    const target = e.target as HTMLTextAreaElement
    internalContent = target.value
  }

  async function handleSave() {
    try {
      await onsave?.(internalContent)
      internalContent = ''
      originalContent = ''
      lastExternalContent = ''
      textareaEl?.focus()
    } catch {
      // Save failed — keep content so the user can retry
    }
  }
</script>

<div class="note-editor">
  <textarea
    bind:this={textareaEl}
    class="note-editor__textarea"
    rows="3"
    {placeholder}
    value={internalContent}
    oninput={handleInput}
  ></textarea>

  <div class="note-editor__actions">
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
