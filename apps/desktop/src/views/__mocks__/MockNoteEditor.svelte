<script lang="ts">
  import type { NoteEditorProps } from '@entropia/ui'

  let {
    content = '',
    onsave,
    oncancel,
    saveLabel = 'Save note',
    cancelLabel = 'Cancel',
  }: NoteEditorProps = $props()
</script>

<div data-testid="mock-note-editor">
  <button type="button" aria-label="Bold">Bold</button>
  <div role="textbox">{content}</div>
  {#if oncancel}
    <button type="button" data-testid="note-cancel" onclick={() => oncancel()}>{cancelLabel}</button
    >
  {/if}
  <button
    type="button"
    data-testid="note-save"
    onclick={() => onsave?.(content || '<p>mock note</p>')}
  >
    {saveLabel}
  </button>
</div>
