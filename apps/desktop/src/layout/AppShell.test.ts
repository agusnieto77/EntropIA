import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import AppShellHost from './__fixtures__/AppShellHost.svelte'
import { locale } from '$lib/i18n'

type EventListenerCallback = (event: { payload: unknown }) => void

const { invokeMock, listenMock, navigationStore, storeRef } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn<(eventName: string, callback: EventListenerCallback) => Promise<() => void>>(
    () => Promise.resolve(vi.fn()),
  ),
  navigationStore: {
    subscribe(run: (value: unknown) => void) {
      run({
        history: [{ name: 'collections' }],
        current: { name: 'collections' },
        canGoBack: false,
        breadcrumb: ['Collections'],
      })
      return () => {}
    },
  },
  storeRef: {
    current: {
      collections: {
        findAll: vi.fn().mockResolvedValue([]),
        countItems: vi.fn().mockResolvedValue(0),
        findById: vi.fn().mockResolvedValue(null),
      },
      assets: { findByItem: vi.fn().mockResolvedValue([]) },
      items: {
        searchGlobal: vi.fn().mockResolvedValue([]),
        findByCollection: vi.fn().mockResolvedValue([]),
      },
    },
  },
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}))

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock,
}))

vi.mock('$lib/navigation', () => ({
  navigation: {
    subscribe: navigationStore.subscribe,
    navigate: vi.fn(),
    back: vi.fn(),
  },
}))

vi.mock('$lib/db', () => ({
  getStore: () => storeRef.current,
}))

describe('AppShell', () => {
  beforeEach(() => {
    locale.set('es')
    invokeMock.mockReset().mockImplementation((command: string) => {
      if (command === 'deps_get_cached_statuses') {
        return Promise.resolve([])
      }

      return Promise.resolve(undefined)
    })
    listenMock.mockClear().mockImplementation(() => Promise.resolve(vi.fn()))
    storeRef.current.items.searchGlobal.mockClear()
    storeRef.current.items.findByCollection.mockClear()
    storeRef.current.collections.findAll.mockClear()
    storeRef.current.collections.countItems.mockClear()
    storeRef.current.assets.findByItem.mockClear()
    storeRef.current.collections.findById.mockClear()
  })

  it('renders the app frame, visible footer actions, and projected content', () => {
    render(AppShellHost)

    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toBeInTheDocument()
    expect(screen.getByTestId('app-shell-child')).toHaveTextContent('Contenido de prueba')
    expect(screen.getByText('EntropIA β')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'GitHub' })).toBeInTheDocument()
    expect(screen.getByText('Desarrollado por')).toBeInTheDocument()
  })

  it('opens external links through the desktop bridge', async () => {
    render(AppShellHost)

    await fireEvent.click(screen.getByRole('link', { name: 'GitHub' }))
    expect(invokeMock).toHaveBeenCalledWith('open_external_url', {
      url: 'https://github.com/agusnieto77/EntropIA',
    })

    await fireEvent.click(screen.getByRole('link', { name: 'HLab' }))
    expect(invokeMock).toHaveBeenCalledWith('open_external_url', {
      url: 'https://hlab.com.ar/',
    })
  })

  it('reacts to locale changes in footer copy', async () => {
    render(AppShellHost)

    locale.set('en')

    expect(await screen.findByText('Archive, OCR, and assisted analysis.')).toBeInTheDocument()
    expect(screen.getByText('Developed by')).toBeInTheDocument()
  })

  it('boots without awaiting a fresh dependency probe and updates from completion events', async () => {
    let depsCompleteHandler: ((event: { payload: { results: Array<{ id: string; status: { type: string } }> } }) => void) | undefined

    listenMock.mockImplementation((eventName: string, callback: EventListenerCallback) => {
      if (eventName === 'deps://complete') {
        depsCompleteHandler = callback as typeof depsCompleteHandler
      }

      return Promise.resolve(vi.fn())
    })

    render(AppShellHost)

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('deps_get_cached_statuses')
    })
    expect(invokeMock).not.toHaveBeenCalledWith('deps_check_all')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    depsCompleteHandler?.({
      payload: {
        results: [
          { id: 'Python', status: { type: 'missing' } },
          { id: 'Fastembed', status: { type: 'installed' } },
          { id: 'PaddleOcr', status: { type: 'installed' } },
        ],
      },
    })

    expect(await screen.findByRole('alert')).toHaveTextContent(
      '⚠ Algunas funciones de IA no están disponibles.',
    )
  })
})
