import { fireEvent, render, screen } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import AppShellHost from './__fixtures__/AppShellHost.svelte'

const { invokeMock, navigationStore, storeRef } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
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
      items: { searchGlobal: vi.fn().mockResolvedValue([]) },
      collections: { findById: vi.fn().mockResolvedValue(null) },
    },
  },
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
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
    invokeMock.mockReset().mockResolvedValue(undefined)
    storeRef.current.items.searchGlobal.mockClear()
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
})
