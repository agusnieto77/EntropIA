import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import TopBar from './TopBar.svelte'

const { navigationStore, setNavigationState, navigateMock, backMock, storeRef } = vi.hoisted(() => {
  let current: any = {
    history: [{ name: 'collections' as const }],
    current: { name: 'collections' as const },
    canGoBack: false,
    breadcrumb: ['Collections'],
  }
  const subscribers = new Set<(value: any) => void>()

  return {
    navigationStore: {
      subscribe(run: (value: any) => void) {
        subscribers.add(run)
        run(current)
        return () => subscribers.delete(run)
      },
    },
    setNavigationState(value: typeof current) {
      current = value
      subscribers.forEach((run) => run(current))
    },
    navigateMock: vi.fn(),
    backMock: vi.fn(),
    storeRef: {
      current: {
        items: { searchGlobal: vi.fn() },
        collections: { findById: vi.fn() },
      },
    },
  }
})

vi.mock('$lib/navigation', () => ({
  navigation: {
    subscribe: navigationStore.subscribe,
    navigate: navigateMock,
    back: backMock,
  },
}))

vi.mock('$lib/db', () => ({
  getStore: () => storeRef.current,
}))

describe('TopBar', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    navigateMock.mockReset()
    backMock.mockReset()
    storeRef.current.items.searchGlobal.mockReset()
    storeRef.current.collections.findById.mockReset()
    setNavigationState({
      history: [
        { name: 'collections' },
        { name: 'collection', id: 'col-1', collectionName: 'Archivo' },
      ],
      current: { name: 'collection', id: 'col-1', collectionName: 'Archivo' },
      canGoBack: true,
      breadcrumb: ['Collections', 'Archivo'],
    })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('renders accessible controls for navigation and global search', () => {
    render(TopBar)

    expect(screen.getByRole('button', { name: 'Abrir configuración' })).toBeInTheDocument()
    expect(screen.getByRole('searchbox', { name: 'Buscar archivos' })).toBeInTheDocument()
    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toBeInTheDocument()
  })

  it('shows results and navigates to the selected item', async () => {
    storeRef.current.items.searchGlobal.mockResolvedValueOnce([
      { id: 'item-1', title: 'Acta fundacional', collectionId: 'col-1' },
    ])
    storeRef.current.collections.findById.mockResolvedValueOnce({
      id: 'col-1',
      name: 'Archivo',
    })

    render(TopBar)

    const input = screen.getByRole('searchbox', { name: 'Buscar archivos' })
    await fireEvent.input(input, { target: { value: 'acta' } })
    vi.advanceTimersByTime(300)

    await waitFor(() => {
      expect(screen.getByRole('button', { name: /Acta fundacional/i })).toBeInTheDocument()
    })

    await fireEvent.click(screen.getByRole('button', { name: /Acta fundacional/i }))

    expect(navigateMock).toHaveBeenNthCalledWith(1, {
      name: 'collection',
      id: 'col-1',
      collectionName: 'Archivo',
    })
    expect(navigateMock).toHaveBeenNthCalledWith(2, {
      name: 'item',
      collectionId: 'col-1',
      collectionName: 'Archivo',
      itemId: 'item-1',
      itemTitle: 'Acta fundacional',
    })
  })
})
