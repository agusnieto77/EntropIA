<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { Editor } from '@tiptap/core'
  import StarterKit from '@tiptap/starter-kit'
  import Underline from '@tiptap/extension-underline'
  import Link from '@tiptap/extension-link'
  import Placeholder from '@tiptap/extension-placeholder'

  import type { NoteEditorProps } from './NoteEditor.types'
  import {
    hasNoteEditorMeaningfulChanges,
    isNoteHtmlEffectivelyEmpty,
    normalizeNoteContentForEditor,
    normalizeNoteContentForRender,
    sanitizeNoteHtml,
    shouldDisableNoteEditorSave,
  } from './note-content'

  let {
    content = '',
    placeholder = '',
    onsave,
    oncancel,
    ondictate,
    dictationMaxSeconds = 300,
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
  let isFocused = $state(false)
  let dictationState = $state<'idle' | 'recording' | 'transcribing' | 'error'>('idle')
  let dictationSeconds = $state(0)
  let dictationMessage = $state<string | null>(null)
  let dictationAutoStopped = $state(false)
  let mediaRecorder = $state<MediaRecorder | null>(null)
  let mediaStream = $state<MediaStream | null>(null)
  let dictationTimer = $state<ReturnType<typeof setInterval> | null>(null)
  let dictationChunks = $state<Blob[]>([])
  let dictationProcessing = $state<Promise<void> | null>(null)
  let dictationSelection = $state<{ from: number; to: number } | null>(null)

  const showCancel = $derived(typeof oncancel === 'function')
  const supportsDictation = $derived(typeof ondictate === 'function')
  const isEmpty = $derived(isNoteHtmlEffectivelyEmpty(currentHtml))
  const isEditing = $derived(showCancel || !clearOnSave)
  const hasChanges = $derived(
    hasNoteEditorMeaningfulChanges({
      originalContent: originalHtml,
      currentContent: currentHtml,
    })
  )
  const isSaveDisabled = $derived(
    shouldDisableNoteEditorSave({
      currentContent: currentHtml,
      originalContent: originalHtml,
      isEditing,
    })
  )

  const dictationButtonLabel = $derived.by(() => {
    if (dictationState === 'recording') return 'Detener dictado'
    if (dictationState === 'transcribing') return 'Procesando dictado...'
    return 'Iniciar dictado'
  })

  const dictationTimerLabel = $derived(formatDuration(dictationSeconds))

  type ToolbarButton = {
    label: string
    shortLabel: string
    isActive: () => boolean
    action: () => void
  }

  type ToolbarGroup = {
    label: string
    buttons: ToolbarButton[]
  }

  const toolbarGroups: ToolbarGroup[] = [
    {
      label: 'Text style',
      buttons: [
        {
          label: 'Bold',
          shortLabel: 'B',
          isActive: () => editor?.isActive('bold') ?? false,
          action: () => editor?.chain().focus().toggleBold().run(),
        },
        {
          label: 'Italic',
          shortLabel: 'I',
          isActive: () => editor?.isActive('italic') ?? false,
          action: () => editor?.chain().focus().toggleItalic().run(),
        },
        {
          label: 'Underline',
          shortLabel: 'U',
          isActive: () => editor?.isActive('underline') ?? false,
          action: () => editor?.chain().focus().toggleUnderline().run(),
        },
        {
          label: 'Inline code',
          shortLabel: '</>',
          isActive: () => editor?.isActive('code') ?? false,
          action: () => editor?.chain().focus().toggleCode().run(),
        },
      ],
    },
    {
      label: 'Structure',
      buttons: [
        {
          label: 'Heading 1',
          shortLabel: 'H1',
          isActive: () => editor?.isActive('heading', { level: 1 }) ?? false,
          action: () => editor?.chain().focus().toggleHeading({ level: 1 }).run(),
        },
        {
          label: 'Heading 2',
          shortLabel: 'H2',
          isActive: () => editor?.isActive('heading', { level: 2 }) ?? false,
          action: () => editor?.chain().focus().toggleHeading({ level: 2 }).run(),
        },
        {
          label: 'Heading 3',
          shortLabel: 'H3',
          isActive: () => editor?.isActive('heading', { level: 3 }) ?? false,
          action: () => editor?.chain().focus().toggleHeading({ level: 3 }).run(),
        },
        {
          label: 'Bullet list',
          shortLabel: '• List',
          isActive: () => editor?.isActive('bulletList') ?? false,
          action: () => editor?.chain().focus().toggleBulletList().run(),
        },
        {
          label: 'Ordered list',
          shortLabel: '1. List',
          isActive: () => editor?.isActive('orderedList') ?? false,
          action: () => editor?.chain().focus().toggleOrderedList().run(),
        },
        {
          label: 'Quote',
          shortLabel: 'Quote',
          isActive: () => editor?.isActive('blockquote') ?? false,
          action: () => editor?.chain().focus().toggleBlockquote().run(),
        },
      ],
    },
    {
      label: 'Insert',
      buttons: [
        {
          label: 'Add link',
          shortLabel: 'Link',
          isActive: () => editor?.isActive('link') ?? false,
          action: () => updateLink(),
        },
        {
          label: 'Remove link',
          shortLabel: 'Unlink',
          isActive: () => false,
          action: () => removeLink(),
        },
      ],
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
        dictationSelection = {
          from: editor.state.selection.from,
          to: editor.state.selection.to,
        }
        syncEditorState(sanitizeNoteHtml(editor.getHTML()) || '<p></p>')
      },
      onUpdate: ({ editor }: { editor: Editor }) => {
        syncEditorState(sanitizeNoteHtml(editor.getHTML()) || '<p></p>')
      },
      onSelectionUpdate: ({ editor }: { editor: Editor }) => {
        dictationSelection = {
          from: editor.state.selection.from,
          to: editor.state.selection.to,
        }
        bumpEditorRevision()
      },
      onFocus: () => {
        isFocused = true
        bumpEditorRevision()
      },
      onBlur: () => {
        isFocused = false
        bumpEditorRevision()
      },
    })

    editor = instance
  }

  function formatDuration(totalSeconds: number) {
    const minutes = Math.floor(totalSeconds / 60)
    const seconds = totalSeconds % 60
    return `${minutes}:${seconds.toString().padStart(2, '0')}`
  }

  function resetDictationTimer() {
    if (dictationTimer) {
      clearInterval(dictationTimer)
      dictationTimer = null
    }
    dictationSeconds = 0
  }

  function stopMediaStreamTracks() {
    mediaStream?.getTracks().forEach((track) => track.stop())
    mediaStream = null
  }

  function setDictationMessage(message: string | null, tone: 'idle' | 'error' = 'idle') {
    dictationMessage = message
    if (tone === 'error') {
      dictationState = 'error'
    }
  }

  function getDictationInsertionPlan(text: string) {
    if (!editor) {
      return { text: text.trim(), leadingSpace: false, trailingSpace: false }
    }

    const trimmed = text.trim()
    if (!trimmed) return { text: '', leadingSpace: false, trailingSpace: false }

    if (!dictationSelection) {
      const currentText = editor.getText()
      const prevChar = currentText.slice(-1)
      return {
        text: trimmed,
        leadingSpace: Boolean(prevChar) && !/\s/.test(prevChar) && !/^[\s,.;:!?)]/.test(trimmed),
        trailingSpace: false,
      }
    }

    const fallbackSelection = {
      from: editor.state.doc.content.size,
      to: editor.state.doc.content.size,
    }
    const { from, to } = dictationSelection ?? fallbackSelection
    const prevChar = editor.state.doc.textBetween(Math.max(0, from - 1), from, '', '')
    const nextChar = editor.state.doc.textBetween(
      to,
      Math.min(editor.state.doc.content.size, to + 1),
      '',
      ''
    )

    const needsLeadingSpace =
      from > 1 && prevChar && !/\s/.test(prevChar) && !/^[\s,.;:!?)]/.test(trimmed)
    const needsTrailingSpace = nextChar && !/\s/.test(nextChar) && !/[\s([{]$/.test(trimmed)

    return {
      text: trimmed,
      leadingSpace: Boolean(needsLeadingSpace),
      trailingSpace: Boolean(needsTrailingSpace),
    }
  }

  function insertDictationText(text: string) {
    if (!editor) return

    const insertion = getDictationInsertionPlan(text)
    if (!insertion.text) return

    const insertionText = `${insertion.leadingSpace ? ' ' : ''}${insertion.text}${insertion.trailingSpace ? ' ' : ''}`

    if (dictationSelection) {
      editor
        .chain()
        .focus()
        .insertContentAt(
          { from: dictationSelection.from, to: dictationSelection.to },
          insertionText
        )
        .run()
    } else {
      editor.chain().focus('end').insertContent(insertionText).run()
    }
    syncEditorState(sanitizeNoteHtml(editor.getHTML()) || '<p></p>')
  }

  async function finalizeDictation() {
    const recorder = mediaRecorder
    const wasAutoStopped = dictationAutoStopped
    const audioBlob = new Blob(dictationChunks, {
      type: recorder?.mimeType || 'audio/webm',
    })

    dictationChunks = []
    mediaRecorder = null
    stopMediaStreamTracks()
    resetDictationTimer()

    if (!ondictate || audioBlob.size === 0) {
      dictationState = 'idle'
      if (audioBlob.size === 0) {
        setDictationMessage('No se pudo capturar audio del micrófono.', 'error')
      }
      return
    }

    dictationState = 'transcribing'
    if (wasAutoStopped) {
      dictationMessage = `Se alcanzó el máximo de ${formatDuration(dictationMaxSeconds)}. Procesando audio...`
    } else {
      dictationMessage = 'Transcribiendo audio...'
    }

    try {
      const text = (await ondictate(audioBlob)).trim()
      if (text) {
        insertDictationText(text)
        dictationMessage = wasAutoStopped
          ? `Se alcanzó el máximo de ${formatDuration(dictationMaxSeconds)}. Texto insertado.`
          : 'Texto insertado desde el micrófono.'
        dictationState = 'idle'
      } else {
        setDictationMessage('No se detectó texto en el audio.', 'error')
      }
    } catch (error) {
      setDictationMessage(
        error instanceof Error ? error.message : 'No se pudo transcribir el audio.',
        'error'
      )
    } finally {
      dictationAutoStopped = false
      dictationProcessing = null
    }
  }

  async function stopDictation(options?: { autoStop?: boolean }) {
    const recorder = mediaRecorder
    if (!recorder || recorder.state !== 'recording') return

    dictationAutoStopped = options?.autoStop ?? false
    const processing = new Promise<void>((resolve) => {
      recorder.onstop = () => {
        void finalizeDictation().finally(resolve)
      }
    })
    dictationProcessing = processing
    recorder.stop()
    await processing
  }

  async function startDictation() {
    if (!ondictate) return

    if (
      typeof window === 'undefined' ||
      typeof navigator === 'undefined' ||
      !navigator.mediaDevices?.getUserMedia ||
      typeof MediaRecorder === 'undefined'
    ) {
      setDictationMessage('No hay micrófono disponible en este dispositivo.', 'error')
      return
    }

    try {
      dictationChunks = []
      dictationMessage = null
      dictationAutoStopped = false
      dictationSelection = editor?.isFocused
        ? {
            from: editor.state.selection.from,
            to: editor.state.selection.to,
          }
        : null
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true })
      mediaStream = stream

      const recorder = new MediaRecorder(stream)
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          dictationChunks = [...dictationChunks, event.data]
        }
      }
      mediaRecorder = recorder
      dictationState = 'recording'
      dictationSeconds = 0
      recorder.start()

      dictationTimer = setInterval(() => {
        dictationSeconds += 1
        if (dictationSeconds >= dictationMaxSeconds) {
          void stopDictation({ autoStop: true })
        }
      }, 1000)
    } catch (error) {
      stopMediaStreamTracks()
      resetDictationTimer()
      setDictationMessage(
        error instanceof Error ? error.message : 'No se pudo acceder al micrófono.',
        'error'
      )
    }
  }

  async function toggleDictation() {
    if (dictationState === 'transcribing') return
    if (dictationState === 'recording') {
      await stopDictation()
      return
    }
    await startDictation()
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

  function removeLink() {
    if (!editor) return

    editor.chain().focus().extendMarkRange('link').unsetLink().run()
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
    resetDictationTimer()
    if (mediaRecorder?.state === 'recording') {
      mediaRecorder.stop()
    }
    stopMediaStreamTracks()
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
    {#each toolbarGroups as group (group.label)}
      <div class="note-editor__tool-group" role="group" aria-label={group.label}>
        {#each group.buttons as button (button.label)}
          <button
            type="button"
            class="note-editor__tool"
            class:note-editor__tool--active={button.isActive()}
            aria-pressed={button.isActive()}
            aria-label={button.label}
            title={button.label}
            onclick={button.action}
          >
            {button.shortLabel}
          </button>
        {/each}
      </div>
    {/each}

    {#if supportsDictation}
      <div class="note-editor__tool-group" role="group" aria-label="Dictation">
        <button
          type="button"
          class="note-editor__tool note-editor__tool--dictation"
          class:note-editor__tool--active={dictationState === 'recording'}
          aria-label={dictationButtonLabel}
          title={dictationButtonLabel}
          disabled={dictationState === 'transcribing'}
          onmousedown={(event) => event.preventDefault()}
          onclick={toggleDictation}
        >
          🎙
        </button>
        <span class="note-editor__dictation-status" data-testid="note-editor-dictation-timer">
          {#if dictationState === 'recording'}
            {dictationTimerLabel} / {formatDuration(dictationMaxSeconds)}
          {:else if dictationState === 'transcribing'}
            Procesando...
          {:else}
            Dictado
          {/if}
        </span>
      </div>
    {/if}
  </div>

  <p class="note-editor__helper">Tip: seleccioná texto para aplicar formato o links.</p>

  {#if dictationMessage}
    <p
      class="note-editor__dictation-message"
      class:note-editor__dictation-message--error={dictationState === 'error'}
      data-testid="note-editor-dictation-message"
    >
      {dictationMessage}
    </p>
  {/if}

  <div class="note-editor__surface" class:note-editor__surface--focused={isFocused}>
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
      aria-disabled={isSaveDisabled}
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
    align-items: center;
  }

  .note-editor__tool-group {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 0;
    padding: 0.125rem;
    border: 1px solid color-mix(in srgb, var(--color-border) 88%, transparent);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface) 90%, black 10%);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.03);
  }

  .note-editor__tool,
  .note-editor__btn {
    padding: 0.45rem 0.65rem;
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

  .note-editor__tool--dictation {
    min-width: 2.75rem;
  }

  .note-editor__tool {
    min-width: 2.5rem;
    background: transparent;
    color: var(--color-text-secondary);
    border-color: transparent;
  }

  .note-editor__tool:hover,
  .note-editor__btn:hover:not(:disabled) {
    border-color: var(--color-border-strong);
    color: var(--color-text-primary);
    background: color-mix(in srgb, var(--color-surface) 72%, black 28%);
  }

  .note-editor__tool--active {
    border-color: color-mix(in srgb, var(--color-accent) 60%, var(--color-border));
    background: color-mix(in srgb, var(--color-accent) 22%, var(--color-surface));
    color: var(--color-text-primary);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-accent) 24%, transparent);
  }

  .note-editor__helper {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .note-editor__dictation-status {
    display: inline-flex;
    align-items: center;
    padding: 0 0.5rem;
    font-size: var(--font-size-xs);
    color: var(--color-text-muted);
  }

  .note-editor__dictation-message {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--color-text-secondary);
  }

  .note-editor__dictation-message--error {
    color: #ff8f8f;
  }

  .note-editor__surface {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-surface) 88%, black 12%);
    overflow: hidden;
    transition:
      border-color 0.15s ease,
      box-shadow 0.15s ease,
      background-color 0.15s ease;
  }

  .note-editor__surface--focused {
    border-color: color-mix(in srgb, var(--color-accent) 65%, var(--color-border));
    box-shadow: 0 0 0 2px rgba(124, 149, 255, 0.12);
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
    box-shadow: none;
  }

  .note-editor__surface :global(.ProseMirror ::selection) {
    background: color-mix(in srgb, var(--color-accent) 35%, transparent);
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
