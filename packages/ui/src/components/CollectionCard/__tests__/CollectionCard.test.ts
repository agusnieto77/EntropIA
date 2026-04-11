import { render, screen, fireEvent } from '@testing-library/svelte'
import { describe, it, expect, vi } from 'vitest'
import CollectionCard from '../CollectionCard.svelte'

describe('CollectionCard', () => {
  const baseProps = {
    id: 'col-1',
    name: 'My Collection',
    itemCount: 5,
    updatedAt: Date.now() - 2 * 24 * 60 * 60 * 1000, // 2 days ago
  }

  it('renders the collection name as bold text', () => {
    render(CollectionCard, { props: baseProps })
    const nameEl = screen.getByText('My Collection')
    expect(nameEl).toBeInTheDocument()
    expect(nameEl.tagName).toBe('H3')
  })

  it('renders the item count badge', () => {
    render(CollectionCard, { props: baseProps })
    expect(screen.getByText('5 items')).toBeInTheDocument()
  })

  it('renders description when provided', () => {
    render(CollectionCard, {
      props: { ...baseProps, description: 'A test description for the collection' },
    })
    expect(screen.getByText('A test description for the collection')).toBeInTheDocument()
  })

  it('does not render description element when not provided', () => {
    render(CollectionCard, { props: baseProps })
    const descEl = screen.queryByTestId('collection-description')
    expect(descEl).not.toBeInTheDocument()
  })

  it('renders a relative date string', () => {
    render(CollectionCard, { props: baseProps })
    // Should show something like "hace 2 dias" (relative time)
    const dateEl = screen.getByTestId('collection-date')
    expect(dateEl).toBeInTheDocument()
    expect(dateEl.textContent).toBeTruthy()
  })

  it('calls onclick when clicked', async () => {
    const onclick = vi.fn()
    render(CollectionCard, { props: { ...baseProps, onclick } })
    const card = screen.getByRole('button')
    await fireEvent.click(card)
    expect(onclick).toHaveBeenCalledOnce()
  })

  it('renders singular "item" for count of 1', () => {
    render(CollectionCard, { props: { ...baseProps, itemCount: 1 } })
    expect(screen.getByText('1 item')).toBeInTheDocument()
  })
})
