<script lang="ts">
  import type { NoteEditorProps } from '@entropia/ui'

  let {
    content = '',
    onsave,
    oncancel,
    clearOnSave = true,
    saveLabel = 'Save note',
    cancelLabel = 'Cancel',
  }: NoteEditorProps = $props()

  const isEditing = $derived(typeof oncancel === 'function' || !clearOnSave)
  const isSaveDisabled = $derived(isEditing && !content.trim())
</script>

<div data-testid="mock-note-editor">
  <button type="button" aria-label="Bold">Bold</button>
  <div role="group" aria-label="Text style"></div>
  <div role="group" aria-label="Structure"></div>
  <div role="group" aria-label="Insert"></div>
  <button type="button" aria-label="Add link">Link</button>
  <button type="button" aria-label="Remove link">Unlink</button>
  <div role="textbox">{content}</div>
  <p>Tip: seleccioná texto para aplicar formato o links.</p>
  {#if oncancel}
    <button type="button" data-testid="note-cancel" onclick={() => oncancel()}>{cancelLabel}</button
    >
  {/if}
  <button
    type="button"
    data-testid="note-save"
    disabled={isSaveDisabled}
    onclick={() => onsave?.(content || '<p>mock note</p>')}
  >
    {saveLabel}
  </button>
</div>
