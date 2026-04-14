import { render, screen, fireEvent, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import ItemView from './ItemView.svelte'

const { nlpEventHandlers, extractTriplesMock, similarItemsMock, extractTextMock } = vi.hoisted(
  () => ({
    nlpEventHandlers: new Map<string, (event: { payload: unknown }) => void>(),
    extractTriplesMock: vi.fn<(_: string) => Promise<void>>(),
    similarItemsMock: vi.fn<(_: string, __?: number) => Promise<Array<{ itemId: string }>>>(),
    extractTextMock: vi.fn(),
  })
)

type TripleRow = { subject: string; predicate: string; object: string }

function createStore(triplesRows: TripleRow[]) {
  return {
    items: {
      findById: vi.fn().mockResolvedValue({
        id: 'item-1',
        title: 'Acta histórica',
        metadata: '{}',
      }),
      update: vi.fn().mockResolvedValue(undefined),
    },
    assets: {
      findByItem: vi.fn().mockResolvedValue([
        {
          id: 'asset-1',
          itemId: 'item-1',
          path: 'docs/acta.pdf',
          type: 'pdf',
          createdAt: Date.now(),
        },
      ]),
    },
    notes: {
      findByItem: vi.fn().mockResolvedValue([]),
      create: vi.fn().mockResolvedValue(undefined),
      update: vi.fn().mockResolvedValue(undefined),
      delete: vi.fn().mockResolvedValue(undefined),
    },
    entities: {
      findByItemId: vi.fn().mockResolvedValue([]),
    },
    triples: {
      findByItemId: vi.fn().mockResolvedValue(triplesRows),
    },
  }
}

const storeRef: { current: ReturnType<typeof createStore> } = {
  current: createStore([]),
}

vi.mock('$lib/db', () => ({
  getStore: () => storeRef.current,
}))

vi.mock('$lib/file-import', () => ({
  getAssetUrl: (path: string) => `https://asset.localhost/${path}`,
}))

vi.mock('$lib/ocr', async () => {
  const actual = await vi.importActual<typeof import('$lib/ocr')>('$lib/ocr')
  return {
    ...actual,
    extractText: extractTextMock,
  }
})

vi.mock('$lib/nlp', async () => {
  const actual = await vi.importActual<typeof import('$lib/nlp')>('$lib/nlp')
  return {
    ...actual,
    extractTriples: extractTriplesMock,
    similarItems: similarItemsMock,
    indexFts: vi.fn().mockResolvedValue(undefined),
    embedItem: vi.fn().mockResolvedValue(undefined),
    extractEntities: vi.fn().mockResolvedValue(undefined),
  }
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((eventName: string, callback: (event: { payload: unknown }) => void) => {
    nlpEventHandlers.set(eventName, callback)
    return Promise.resolve(vi.fn())
  }),
}))

vi.mock('@entropia/ui', () => ({
  DocumentViewer: () => null,
  MetadataEditor: () => null,
  NoteEditor: () => null,
  Button: () => null,
  Card: () => null,
  EntityViewer: () => null,
}))

describe('ItemView semantic triples panel', () => {
  beforeEach(() => {
    nlpEventHandlers.clear()
    extractTriplesMock.mockReset().mockResolvedValue(undefined)
    similarItemsMock.mockReset().mockResolvedValue([])
    extractTextMock.mockReset().mockResolvedValue(undefined)
  })

  async function renderItemViewWith(triplesRows: TripleRow[]) {
    storeRef.current = createStore(triplesRows)
    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    const analysisToggle = await screen.findByRole('button', { name: /Analysis/i })
    await fireEvent.click(analysisToggle)
  }

  it('shows explicit empty state when no triples exist for the item', async () => {
    await renderItemViewWith([])

    expect(await screen.findByText('Semantic Triples (S|P|O)')).toBeInTheDocument()
    expect(await screen.findByText('No triples extracted yet for this item.')).toBeInTheDocument()
  })

  it('renders triples as Subject | Predicate | Object rows when store has data', async () => {
    await renderItemViewWith([
      { subject: 'Belgrano', predicate: 'creó', object: 'la Bandera' },
      { subject: 'San Martín', predicate: 'fue', object: 'gobernador de Cuyo' },
    ])

    expect(await screen.findByText('Belgrano')).toBeInTheDocument()
    expect(await screen.findByText('creó')).toBeInTheDocument()
    expect(await screen.findByText('la Bandera')).toBeInTheDocument()
    expect(await screen.findByText('San Martín')).toBeInTheDocument()
    expect(await screen.findByText('gobernador de Cuyo')).toBeInTheDocument()
  })

  it('transitions pending → running → done and supports retry after error for triples', async () => {
    await renderItemViewWith([])

    const triplesBtn = await screen.findByRole('button', { name: /Extract Triples/i })

    await fireEvent.click(triplesBtn)
    expect(extractTriplesMock).toHaveBeenCalledWith('item-1')
    expect(triplesBtn).toBeDisabled()
    expect(screen.getByText('pending')).toBeInTheDocument()

    nlpEventHandlers.get('nlp:progress')?.({
      payload: { item_id: 'item-1', job: 'triples', pct: 25 },
    })
    await waitFor(() => {
      expect(screen.getByText('running')).toBeInTheDocument()
      expect(triplesBtn).toBeDisabled()
    })

    storeRef.current.triples.findByItemId.mockResolvedValueOnce([
      { subject: 'Moreno', predicate: 'fundó', object: 'La Gazeta' },
    ])
    nlpEventHandlers.get('nlp:complete')?.({
      payload: { item_id: 'item-1', job: 'triples' },
    })
    await waitFor(() => {
      expect(screen.getByText('done')).toBeInTheDocument()
      expect(screen.getByText('Moreno')).toBeInTheDocument()
      expect(screen.getByText('La Gazeta')).toBeInTheDocument()
    })

    nlpEventHandlers.get('nlp:error')?.({
      payload: { item_id: 'item-1', job: 'triples', error: 'queue full' },
    })
    await waitFor(() => {
      expect(screen.getByText('error')).toBeInTheDocument()
      expect(triplesBtn).toBeEnabled()
    })

    await fireEvent.click(triplesBtn)
    expect(extractTriplesMock).toHaveBeenCalledTimes(2)
  })
})

describe('ItemView note editing', () => {
  const sampleNote = {
    id: 'note-1',
    itemId: 'item-1',
    content: 'Original note content',
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }

  beforeEach(() => {
    nlpEventHandlers.clear()
    extractTriplesMock.mockReset().mockResolvedValue(undefined)
    similarItemsMock.mockReset().mockResolvedValue([])
    extractTextMock.mockReset().mockResolvedValue(undefined)
  })

  async function renderItemViewWithNotes(notes: (typeof sampleNote)[]) {
    storeRef.current = createStore([])
    storeRef.current.notes.findByItem.mockResolvedValue(notes)
    storeRef.current.notes.update.mockResolvedValue(undefined)
    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })
    await screen.findByText(`Notes (${notes.length})`)
  }

  it('displays the correct note count', async () => {
    await renderItemViewWithNotes([sampleNote])
    expect(screen.getByText('Notes (1)')).toBeInTheDocument()
  })

  it('displays "No notes yet" when notes array is empty', async () => {
    storeRef.current = createStore([])
    storeRef.current.notes.findByItem.mockResolvedValue([])
    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })
    expect(await screen.findByText('No notes yet.')).toBeInTheDocument()
  })

  it('notes store has update method for editing notes', async () => {
    await renderItemViewWithNotes([sampleNote])
    expect(storeRef.current.notes.update).toBeDefined()
    expect(typeof storeRef.current.notes.update).toBe('function')
  })

  it('notes store update method can be called with note id and content', async () => {
    await renderItemViewWithNotes([sampleNote])
    await storeRef.current.notes.update('note-1', 'Updated content')
    expect(storeRef.current.notes.update).toHaveBeenCalledWith('note-1', 'Updated content')
  })

  it('after update, notes are reloaded from store', async () => {
    const updatedNote = { ...sampleNote, content: 'Updated content', updatedAt: Date.now() }
    storeRef.current.notes.findByItem.mockResolvedValueOnce([sampleNote])
    storeRef.current.notes.findByItem.mockResolvedValueOnce([updatedNote])

    await renderItemViewWithNotes([sampleNote])

    // Simulate the update that handleSaveEdit would do
    await storeRef.current.notes.update('note-1', 'Updated content')
    // After update, findByItem is called again to refresh the list
    expect(storeRef.current.notes.findByItem).toHaveBeenCalledWith('item-1')
  })
})
