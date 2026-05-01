import { fireEvent, render, screen } from '@testing-library/svelte'
import { describe, expect, it, vi } from 'vitest'
import TopicEditor from '../TopicEditor.svelte'

describe('TopicEditor', () => {
  it('renders remove-topic controls with accessible names and no textual multiplication sign', () => {
    render(TopicEditor, {
      props: {
        topics: ['ARCHIVE'],
      },
    })

    const removeButton = screen.getByRole('button', { name: 'Remove topic ARCHIVE' })
    expect(removeButton).toBeInTheDocument()
    expect(removeButton).not.toHaveTextContent('×')
  })

  it('removes a topic when its icon button is clicked', async () => {
    const onchange = vi.fn()
    render(TopicEditor, {
      props: {
        topics: ['ARCHIVE', 'LETTER'],
        onchange,
      },
    })

    await fireEvent.click(screen.getByRole('button', { name: 'Remove topic ARCHIVE' }))

    expect(onchange).toHaveBeenCalledWith(['LETTER'])
  })
})
