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

  it('save button is disabled when content is unchanged', () => {
    render(NoteEditor, { props: { content: 'Original text' } })
    const saveBtn = screen.getByTestId('note-save')
    expect(saveBtn).toBeDisabled()
  })

  it('save button is enabled when content is changed', async () => {
    render(NoteEditor, { props: { content: 'Original' } })
    const textarea = screen.getByRole('textbox')
    await fireEvent.input(textarea, { target: { value: 'Modified' } })
    const saveBtn = screen.getByTestId('note-save')
    expect(saveBtn).not.toBeDisabled()
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

  it('calls oncancel when cancel is clicked', async () => {
    const oncancel = vi.fn()
    render(NoteEditor, { props: { oncancel } })
    const cancelBtn = screen.getByTestId('note-cancel')
    await fireEvent.click(cancelBtn)
    expect(oncancel).toHaveBeenCalledOnce()
  })
})
