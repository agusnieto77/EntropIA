import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import CollectionView from './CollectionView.svelte'

const { storeRef, navigationRef } = vi.hoisted(() => ({
  storeRef: {
    current: {
      items: {
        findByCollection: vi.fn(),
        searchByText: vi.fn(),
        create: vi.fn(),
        update: vi.fn(),
        delete: vi.fn(),
      },
      assets: {
        create: vi.fn(),
      },
    },
  },
  navigationRef: {
    current: { name: 'collection', collectionName: 'Colección' } as const,
    navigate: vi.fn(),
  },
}))

type ItemRow = {
  id: string
  title: string
  createdAt: number
  updatedAt: number
  collectionId: string
  metadata: string | null
}

type AssetRow = {
  id: string
  itemId: string
  path: string
  type: string
  size: number | null
  createdAt: number
}

function createStore(items: ItemRow[], assets: AssetRow[] = []) {
  return {
    items: {
      findByCollection: vi.fn().mockResolvedValue(items),
      searchByText: vi.fn().mockResolvedValue(items),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    },
    assets: {
      create: vi.fn(),
      findByItem: vi.fn().mockResolvedValue(assets),
      findById: vi.fn().mockResolvedValue(assets[0] ?? null),
      deleteWithCascade: vi.fn().mockResolvedValue(undefined),
    },
  }
}

vi.mock('$lib/db', () => ({
  getStore: () => storeRef.current,
}))

vi.mock('$lib/navigation', () => ({
  navigation: navigationRef,
}))

vi.mock('$lib/file-import', () => ({
  pickAndImportFiles: vi.fn().mockResolvedValue([]),
  importFilesFromPaths: vi
    .fn()
    .mockResolvedValue({ imported: [], rejected: [], skippedDuplicatePaths: 0 }),
  getAssetUrl: vi.fn().mockImplementation((p: string) => `asset://localhost${p}`),
  deleteAssetFile: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('$lib/export', () => ({
  exportCollectionById: vi.fn().mockResolvedValue(undefined),
}))

vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: vi.fn(() => ({
    onDragDropEvent: vi.fn().mockResolvedValue(vi.fn()),
  })),
}))

describe('CollectionView consumer compatibility', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    navigationRef.navigate.mockReset()
    storeRef.current = createStore([
      {
        id: 'item-1',
        title: 'Acta',
        createdAt: Date.now(),
        updatedAt: Date.now(),
        collectionId: 'col-1',
        metadata: null,
      },
    ])
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('uses SearchBar onsearch/onclear contract to call collection queries', async () => {
    render(CollectionView, { collectionId: 'col-1' })

    await waitFor(() => {
      expect(storeRef.current.items.findByCollection).toHaveBeenCalledWith('col-1')
    })

    const searchInput = screen.getByRole('searchbox')
    await fireEvent.input(searchInput, { target: { value: 'acta' } })
    vi.advanceTimersByTime(300)

    await waitFor(() => {
      expect(storeRef.current.items.searchByText).toHaveBeenCalledWith('col-1', 'acta')
    })

    await fireEvent.click(screen.getByRole('button', { name: /clear search/i }))

    await waitFor(() => {
      expect(storeRef.current.items.findByCollection).toHaveBeenCalledTimes(2)
    })
  })
})

describe('CollectionView asset deletion', () => {
  const sampleAsset: AssetRow = {
    id: 'asset-1',
    itemId: 'item-1',
    path: '/app-data/assets/col-1/item-1/uuid_acta.pdf',
    type: 'pdf',
    size: 1024,
    createdAt: Date.now(),
  }

  beforeEach(() => {
    vi.useFakeTimers()
    navigationRef.navigate.mockReset()
    storeRef.current = createStore(
      [
        {
          id: 'item-1',
          title: 'Acta',
          createdAt: Date.now(),
          updatedAt: Date.now(),
          collectionId: 'col-1',
          metadata: null,
        },
      ],
      [sampleAsset]
    )
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  async function renderAndWaitForItems() {
    render(CollectionView, { collectionId: 'col-1' })

    // Wait for the async load to complete
    await waitFor(() => {
      expect(storeRef.current.items.findByCollection).toHaveBeenCalled()
    })

    // Advance timers to let the promise resolution propagate to Svelte state
    await vi.advanceTimersByTimeAsync(0)
    await vi.advanceTimersByTimeAsync(0)
  }

  it('shows delete confirmation modal when delete button is clicked', async () => {
    await renderAndWaitForItems()

    // Find and click the delete button
    const deleteBtn = screen.getByRole('button', { name: 'Delete Acta' })
    await fireEvent.click(deleteBtn)

    // Modal should appear
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    expect(screen.getByText(/Are you sure you want to delete/)).toBeInTheDocument()
    expect(screen.getByText('uuid_acta.pdf')).toBeInTheDocument()
  })

  it('cancels deletion when Cancel is clicked', async () => {
    await renderAndWaitForItems()

    const deleteBtn = screen.getByRole('button', { name: 'Delete Acta' })
    await fireEvent.click(deleteBtn)

    expect(screen.getByRole('dialog')).toBeInTheDocument()

    const cancelBtn = screen.getByRole('button', { name: 'Cancel' })
    await fireEvent.click(cancelBtn)

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    })
  })

  it('executes deletion and reloads metadata when confirmed', async () => {
    const { deleteAssetFile } = await import('$lib/file-import')

    await renderAndWaitForItems()

    const deleteBtn = screen.getByRole('button', { name: 'Delete Acta' })
    await fireEvent.click(deleteBtn)

    const confirmBtn = screen.getByRole('button', { name: 'Delete' })
    await fireEvent.click(confirmBtn)

    await waitFor(() => {
      expect(deleteAssetFile).toHaveBeenCalledWith(sampleAsset.path)
      expect(storeRef.current.assets.deleteWithCascade).toHaveBeenCalledWith('asset-1')
    })

    // Modal should close after successful deletion
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    })
  })

  it('shows error when deletion fails', async () => {
    const { deleteAssetFile } = await import('$lib/file-import')
    vi.mocked(deleteAssetFile).mockRejectedValueOnce(new Error('Permission denied'))

    await renderAndWaitForItems()

    const deleteBtn = screen.getByRole('button', { name: 'Delete Acta' })
    await fireEvent.click(deleteBtn)

    const confirmBtn = screen.getByRole('button', { name: 'Delete' })
    await fireEvent.click(confirmBtn)

    await waitFor(() => {
      expect(screen.getByText('Permission denied')).toBeInTheDocument()
    })

    // Modal should still be open with error
    expect(screen.getByRole('dialog')).toBeInTheDocument()
  })
})
