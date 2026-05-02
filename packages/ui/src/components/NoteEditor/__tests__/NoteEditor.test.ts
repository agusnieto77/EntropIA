import { render, screen, fireEvent, waitFor } from '@testing-library/svelte'
import { describe, it, expect, vi } from 'vitest'
import NoteEditor from '../NoteEditor.svelte'

describe('NoteEditor', () => {
  it('renders a rich text textbox with toolbar', () => {
    render(NoteEditor, { props: {} })
    expect(screen.getByRole('textbox')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Bold' })).toBeInTheDocument()
  })

  it('renders with initial content', () => {
    render(NoteEditor, { props: { content: 'Hello world' } })
    expect(screen.getByRole('textbox')).toHaveTextContent('Hello world')
  })

  it('renders placeholder when provided', () => {
    render(NoteEditor, { props: { placeholder: 'Write a note...' } })
    expect(screen.getByRole('textbox')).toHaveAttribute('aria-placeholder', 'Write a note...')
  })

  it('save button is disabled when content is empty', () => {
    render(NoteEditor, { props: {} })
    const saveBtn = screen.getByTestId('note-save')
    expect(saveBtn).toBeDisabled()
  })

  it('save button is enabled when the editor starts with content', async () => {
    render(NoteEditor, { props: { content: 'New content' } })
    const saveBtn = screen.getByTestId('note-save')
    expect(saveBtn).not.toBeDisabled()
  })

  it('calls onsave with sanitized html when save is clicked', async () => {
    const onsave = vi.fn()
    render(NoteEditor, { props: { onsave, content: 'My note' } })

    const saveBtn = screen.getByTestId('note-save')
    await fireEvent.click(saveBtn)
    expect(onsave).toHaveBeenCalledOnce()
    expect(onsave).toHaveBeenCalledWith('<p>My note</p>')
  })

  it('clears the editor after successful save by default', async () => {
    const onsave = vi.fn().mockResolvedValue(undefined)
    render(NoteEditor, { props: { onsave, content: 'A note to save' } })

    const saveBtn = screen.getByTestId('note-save')
    await fireEvent.click(saveBtn)

    await waitFor(() => {
      expect(screen.getByRole('textbox')).not.toHaveTextContent('A note to save')
      expect(saveBtn).toBeDisabled()
    })
  })

  it('keeps the current content after saving when clearOnSave is false', async () => {
    const onsave = vi.fn().mockResolvedValue(undefined)
    render(NoteEditor, { props: { onsave, content: 'A note to save', clearOnSave: false } })

    const saveBtn = screen.getByTestId('note-save')
    await fireEvent.click(saveBtn)

    expect(screen.getByRole('textbox')).toHaveTextContent('A note to save')
  })

  it('does not clear editor when onsave rejects', async () => {
    const onsave = vi.fn().mockRejectedValue(new Error('Save failed'))
    render(NoteEditor, { props: { onsave, content: 'A note that fails' } })

    const saveBtn = screen.getByTestId('note-save')
    await fireEvent.click(saveBtn)

    expect(screen.getByRole('textbox')).toHaveTextContent('A note that fails')
  })

  it('does not render a cancel button', () => {
    render(NoteEditor, { props: {} })
    expect(screen.queryByTestId('note-cancel')).not.toBeInTheDocument()
  })
})
