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

  it('disables save again when edited content returns to baseline', async () => {
    render(NoteEditor, { props: { content: 'Original note' } })
    const textarea = screen.getByRole('textbox')
    const saveBtn = screen.getByTestId('note-save')

    await fireEvent.input(textarea, { target: { value: 'Original note v2' } })
    expect(saveBtn).not.toBeDisabled()

    await fireEvent.input(textarea, { target: { value: 'Original note' } })
    expect(saveBtn).toBeDisabled()
  })

  it('updates baseline when parent provides new content', async () => {
    const view = render(NoteEditor, { props: { content: 'first baseline' } })
    const textarea = screen.getByRole('textbox')
    const saveBtn = screen.getByTestId('note-save')

    await fireEvent.input(textarea, { target: { value: 'locally edited' } })
    expect(saveBtn).not.toBeDisabled()

    await view.rerender({ content: 'second baseline' })

    expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('second baseline')
    expect(screen.getByTestId('note-save')).toBeDisabled()
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
