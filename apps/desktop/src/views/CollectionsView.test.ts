import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import CollectionsView from './CollectionsView.svelte'

const { storeRef, navigationRef } = vi.hoisted(() => ({
  storeRef: {
    current: {
      collections: {
        findAll: vi.fn(),
        countItems: vi.fn(),
        create: vi.fn(),
        delete: vi.fn(),
      },
    },
  },
  navigationRef: {
    navigate: vi.fn(),
  },
}))

type CollectionRow = {
  id: string
  name: string
  description: string | null
  createdAt: number
  updatedAt: number
}

function createStore(collections: CollectionRow[], count = 0) {
  return {
    collections: {
      findAll: vi.fn().mockResolvedValue(collections),
      countItems: vi.fn().mockResolvedValue(count),
      create: vi.fn(),
      delete: vi.fn(),
    },
  }
}

vi.mock('$lib/db', () => ({
  getStore: () => storeRef.current,
}))

vi.mock('$lib/navigation', () => ({
  navigation: navigationRef,
}))

describe('CollectionsView consumer compatibility', () => {
  beforeEach(() => {
    navigationRef.navigate.mockReset()
    storeRef.current = createStore(
      [
        {
          id: 'col-1',
          name: 'Historia',
          description: 'Colección histórica',
          createdAt: Date.now(),
          updatedAt: 1711000000000,
        },
      ],
      7
    )
  })

  it('passes CollectionCard props and preserves onclick navigation contract', async () => {
    render(CollectionsView)

    expect(await screen.findByText('Historia')).toBeInTheDocument()
    expect(await screen.findByText('7 items')).toBeInTheDocument()
    expect(await screen.findByText('Colección histórica')).toBeInTheDocument()

    const card = (await screen.findByRole('button', { name: /Historia/i })) as HTMLButtonElement

    await fireEvent.click(card)

    await waitFor(() => {
      expect(navigationRef.navigate).toHaveBeenCalledWith({
        name: 'collection',
        id: 'col-1',
        collectionName: 'Historia',
      })
    })
  })
})
