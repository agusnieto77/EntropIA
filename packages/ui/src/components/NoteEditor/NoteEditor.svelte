<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { Editor } from '@tiptap/core'
  import StarterKit from '@tiptap/starter-kit'
  import Underline from '@tiptap/extension-underline'
  import Link from '@tiptap/extension-link'
  import Placeholder from '@tiptap/extension-placeholder'

  import type { NoteEditorProps } from './NoteEditor.types'
  import {
    isNoteHtmlEffectivelyEmpty,
    normalizeNoteContentForEditor,
    normalizeNoteContentForRender,
    sanitizeNoteHtml,
  } from './note-content'

  let {
    content = '',
    placeholder = '',
    onsave,
    oncancel,
    clearOnSave = true,
    saveLabel = 'Save',
    cancelLabel = 'Cancel',
  }: NoteEditorProps = $props()

  let editorElement: HTMLDivElement | undefined = $state(undefined)
  let editor = $state<Editor | null>(null)
  let editorRevision = $state(0)
  let currentHtml = $state('<p></p>')
  let originalHtml = $state('<p></p>')
  let lastExternalHtml = $state('')

  const isEmpty = $derived(isNoteHtmlEffectivelyEmpty(currentHtml))
  const isSaveDisabled = $derived(isEmpty)
  const showCancel = $derived(typeof oncancel === 'function')

  const toolbarButtons = [
    {
      label: 'Bold',
      isActive: () => editor?.isActive('bold') ?? false,
      action: () => editor?.chain().focus().toggleBold().run(),
    },
    {
      label: 'Italic',
      isActive: () => editor?.isActive('italic') ?? false,
      action: () => editor?.chain().focus().toggleItalic().run(),
    },
    {
      label: 'Underline',
      isActive: () => editor?.isActive('underline') ?? false,
      action: () => editor?.chain().focus().toggleUnderline().run(),
    },
    {
      label: 'H1',
      isActive: () => editor?.isActive('heading', { level: 1 }) ?? false,
      action: () => editor?.chain().focus().toggleHeading({ level: 1 }).run(),
    },
    {
      label: 'H2',
      isActive: () => editor?.isActive('heading', { level: 2 }) ?? false,
      action: () => editor?.chain().focus().toggleHeading({ level: 2 }).run(),
    },
    {
      label: 'H3',
      isActive: () => editor?.isActive('heading', { level: 3 }) ?? false,
      action: () => editor?.chain().focus().toggleHeading({ level: 3 }).run(),
    },
    {
      label: 'Bullet List',
      isActive: () => editor?.isActive('bulletList') ?? false,
      action: () => editor?.chain().focus().toggleBulletList().run(),
    },
    {
      label: 'Ordered List',
      isActive: () => editor?.isActive('orderedList') ?? false,
      action: () => editor?.chain().focus().toggleOrderedList().run(),
    },
    {
      label: 'Blockquote',
      isActive: () => editor?.isActive('blockquote') ?? false,
      action: () => editor?.chain().focus().toggleBlockquote().run(),
    },
    {
      label: 'Inline Code',
      isActive: () => editor?.isActive('code') ?? false,
      action: () => editor?.chain().focus().toggleCode().run(),
    },
  ]

  function bumpEditorRevision() {
    editorRevision += 1
  }

  function syncEditorState(nextHtml: string) {
    currentHtml = nextHtml || '<p></p>'
    bumpEditorRevision()
  }

  function buildEditor() {
    if (!editorElement) return

    const instance = new Editor({
      element: editorElement,
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3] },
        }),
        Underline,
        Link.configure({
          openOnClick: false,
          autolink: false,
          HTMLAttributes: {
            rel: 'noopener noreferrer nofollow',
            target: '_blank',
          },
        }),
        Placeholder.configure({ placeholder }),
      ],
      content: currentHtml,
      autofocus: false,
      editorProps: {
        attributes: {
          class: 'note-editor__content ProseMirror',
          role: 'textbox',
          'aria-multiline': 'true',
          'aria-placeholder': placeholder,
          'data-testid': 'note-editor-input',
        },
      },
      onCreate: ({ editor }: { editor: Editor }) => {
        syncEditorState(sanitizeNoteHtml(editor.getHTML()) || '<p></p>')
      },
      onUpdate: ({ editor }: { editor: Editor }) => {
        syncEditorState(sanitizeNoteHtml(editor.getHTML()) || '<p></p>')
      },
      onSelectionUpdate: bumpEditorRevision,
      onFocus: bumpEditorRevision,
      onBlur: bumpEditorRevision,
    })

    editor = instance
  }

  function updateLink() {
    if (!editor) return

    const previousHref = editor.getAttributes('link').href ?? ''
    const nextHref = window.prompt('Ingresá la URL del link', previousHref) ?? previousHref
    const normalizedHref = nextHref.trim()

    if (!normalizedHref) {
      editor.chain().focus().unsetLink().run()
    } else {
      editor.chain().focus().extendMarkRange('link').setLink({ href: normalizedHref }).run()
    }

    bumpEditorRevision()
  }

  function clearEditor() {
    editor?.commands.setContent('<p></p>', false)
    currentHtml = '<p></p>'
    originalHtml = '<p></p>'
    lastExternalHtml = normalizeNoteContentForEditor(content)
    bumpEditorRevision()
  }

  async function handleSave() {
    if (!editor || isSaveDisabled) return

    const html = normalizeNoteContentForRender(editor.getHTML())
    if (!html) return

    try {
      await onsave?.(html)

      if (clearOnSave) {
        clearEditor()
        editor.commands.focus('end')
        return
      }

      originalHtml = html
      lastExternalHtml = html
      currentHtml = html || '<p></p>'
      editor.commands.setContent(html, false)
      editor.commands.focus('end')
      bumpEditorRevision()
    } catch {
      // Save failed — keep content so the user can retry
    }
  }

  onMount(() => {
    const normalizedInitial = normalizeNoteContentForEditor(content)
    currentHtml = normalizedInitial
    originalHtml = normalizedInitial
    lastExternalHtml = normalizedInitial
    buildEditor()
  })

  onDestroy(() => {
    editor?.destroy()
    editor = null
  })

  $effect(() => {
    const normalizedExternal = normalizeNoteContentForEditor(content)

    if (normalizedExternal === lastExternalHtml) {
      return
    }

    lastExternalHtml = normalizedExternal
    originalHtml = normalizedExternal
    currentHtml = normalizedExternal

    if (!editor) {
      return
    }

    const currentEditorHtml = normalizeNoteContentForRender(editor.getHTML())
    const nextEditorHtml = normalizeNoteContentForRender(normalizedExternal)

    if (currentEditorHtml !== nextEditorHtml) {
      editor.commands.setContent(normalizedExternal, false)
      bumpEditorRevision()
    }
  })
</script>

<div class="note-editor">
  <div
    class="note-editor__toolbar"
    aria-label="Formatting toolbar"
    data-editor-revision={editorRevision}
  >
    {#each toolbarButtons as button (button.label)}
      <button
        type="button"
        class="note-editor__tool"
        class:note-editor__tool--active={button.isActive()}
        aria-pressed={button.isActive()}
        aria-label={button.label}
        title={button.label}
        onclick={button.action}
      >
        {button.label}
      </button>
    {/each}

    <button
      type="button"
      class="note-editor__tool"
      class:note-editor__tool--active={editor?.isActive('link') ?? false}
      aria-pressed={editor?.isActive('link') ?? false}
      aria-label="Link"
      title="Link"
      onclick={updateLink}
    >
      Link
    </button>
  </div>

  <div class="note-editor__surface">
    <div bind:this={editorElement}></div>
  </div>

  <div class="note-editor__actions">
    {#if showCancel}
      <button
        class="note-editor__btn note-editor__btn--ghost"
        type="button"
        data-testid="note-cancel"
        onclick={() => oncancel?.()}
      >
        {cancelLabel}
      </button>
    {/if}

    <button
      class="note-editor__btn note-editor__btn--save"
      type="button"
      data-testid="note-save"
      disabled={isSaveDisabled}
      onclick={handleSave}
    >
      {saveLabel}
    </button>
  </div>
</div>

<style>
  .note-editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .note-editor__toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .note-editor__tool,
  .note-editor__btn {
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-sans);
    font-size: var(--font-size-xs);
    font-weight: var(--font-weight-medium);
    border-radius: var(--radius-md);
    cursor: pointer;
    border: 1px solid var(--color-border);
    transition:
      background-color 0.15s ease,
      border-color 0.15s ease,
      color 0.15s ease,
      box-shadow 0.15s ease;
  }

  .note-editor__tool {
    background: color-mix(in srgb, var(--color-surface) 82%, black 18%);
    color: var(--color-text-secondary);
  }

  .note-editor__tool:hover,
  .note-editor__btn:hover:not(:disabled) {
    border-color: var(--color-border-strong);
    color: var(--color-text-primary);
  }

  .note-editor__tool--active {
    border-color: color-mix(in srgb, var(--color-accent) 55%, var(--color-border));
    background: color-mix(in srgb, var(--color-accent) 16%, var(--color-surface));
    color: var(--color-text-primary);
  }

  .note-editor__surface {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-surface) 88%, black 12%);
    overflow: hidden;
  }

  .note-editor__surface :global(.ProseMirror) {
    min-height: 88px;
    padding: var(--space-3);
    color: var(--color-text-primary);
    outline: none;
    font-family: var(--font-sans);
    font-size: var(--font-size-md);
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .note-editor__surface :global(.ProseMirror p.is-editor-empty:first-child::before) {
    content: attr(data-placeholder);
    color: var(--color-text-muted);
    float: left;
    height: 0;
    pointer-events: none;
  }

  .note-editor__surface :global(.ProseMirror p:first-child) {
    margin-top: 0;
  }

  .note-editor__surface :global(.ProseMirror p:last-child) {
    margin-bottom: 0;
  }

  .note-editor__surface :global(.ProseMirror a) {
    color: var(--color-accent-hover);
    text-decoration: underline;
  }

  .note-editor__surface :global(.ProseMirror blockquote) {
    margin: var(--space-3) 0;
    padding-left: var(--space-3);
    border-left: 3px solid color-mix(in srgb, var(--color-accent) 45%, var(--color-border));
    color: var(--color-text-secondary);
  }

  .note-editor__surface :global(.ProseMirror code) {
    background: color-mix(in srgb, var(--color-border) 65%, transparent);
    border-radius: var(--radius-sm);
    padding: 0.1rem 0.3rem;
    font-size: 0.95em;
  }

  .note-editor__surface :global(.ProseMirror pre) {
    background: color-mix(in srgb, var(--color-surface) 76%, black 24%);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    overflow-x: auto;
  }

  .note-editor__surface :global(.ProseMirror ul),
  .note-editor__surface :global(.ProseMirror ol) {
    padding-left: 1.25rem;
  }

  .note-editor__surface :global(.ProseMirror:focus) {
    box-shadow:
      inset 0 0 0 1px var(--color-accent),
      0 0 0 2px rgba(124, 149, 255, 0.18);
  }

  .note-editor__actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
  }

  .note-editor__btn:disabled {
    cursor: not-allowed;
    opacity: 0.5;
  }

  .note-editor__btn--ghost {
    background: transparent;
    color: var(--color-text-secondary);
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
