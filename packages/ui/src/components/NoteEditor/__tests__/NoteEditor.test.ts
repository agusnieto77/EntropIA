import { render, screen, fireEvent } from '@testing-library/svelte'
import { describe, it, expect, vi } from 'vitest'
import NoteEditor from '../NoteEditor.svelte'

describe('NoteEditor', () => {
  it('renders a textarea', () => {
    render(NoteEditor, { props: {} })
    expect(screen.getByRole('textbox')).toBeInTheDocument()
  })

  it('renders with initial content', () => {
    render(NoteEditor, { props: { content: 'Hello world' } })
    expect(screen.getByRole('textbox')).toHaveValue('Hello world')
  })

  it('renders placeholder when provided', () => {
    render(NoteEditor, { props: { placeholder: 'Write a note...' } })
    expect(screen.getByPlaceholderText('Write a note...')).toBeInTheDocument()
  })

  it('save button is disabled when content is empty', () => {
    render(NoteEditor, { props: {} })
    const saveBtn = screen.getByTestId('note-save')
    expect(saveBtn).toBeDisabled()
  })

  it('save button is enabled when content goes from empty to non-empty', async () => {
    render(NoteEditor, { props: {} })
    const textarea = screen.getByRole('textbox')
    await fireEvent.input(textarea, { target: { value: 'New content' } })
    const saveBtn = screen.getByTestId('note-save')
    expect(saveBtn).not.toBeDisabled()
  })

  it('calls onsave with current content when save is clicked', async () => {
    const onsave = vi.fn()
    render(NoteEditor, { props: { onsave } })
    const textarea = screen.getByRole('textbox')
    await fireEvent.input(textarea, { target: { value: 'My note' } })

    const saveBtn = screen.getByTestId('note-save')
    await fireEvent.click(saveBtn)
    expect(onsave).toHaveBeenCalledOnce()
    expect(onsave).toHaveBeenCalledWith('My note')
  })

  it('clears textarea after successful save', async () => {
    const onsave = vi.fn().mockResolvedValue(undefined)
    render(NoteEditor, { props: { onsave } })
    const textarea = screen.getByRole('textbox')
    await fireEvent.input(textarea, { target: { value: 'A note to save' } })

    const saveBtn = screen.getByTestId('note-save')
    await fireEvent.click(saveBtn)

    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('')
    expect(saveBtn).toBeDisabled()
  })

  it('does not clear textarea when onsave rejects', async () => {
    const onsave = vi.fn().mockRejectedValue(new Error('Save failed'))
    render(NoteEditor, { props: { onsave } })
    const textarea = screen.getByRole('textbox')
    await fireEvent.input(textarea, { target: { value: 'A note that fails' } })

    const saveBtn = screen.getByTestId('note-save')
    await fireEvent.click(saveBtn)

    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('A note that fails')
  })

  it('does not render a cancel button', () => {
    render(NoteEditor, { props: {} })
    expect(screen.queryByTestId('note-cancel')).not.toBeInTheDocument()
  })
})
