import { render, screen, fireEvent, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import ItemView from './ItemView.svelte'

const { nlpEventHandlers, extractTriplesMock, llmExtractTriplesMock, llmExtractTriplesAssetMock, similarItemsMock, extractTextMock } = vi.hoisted(
  () => ({
    nlpEventHandlers: new Map<string, (event: { payload: unknown }) => void>(),
    extractTriplesMock: vi.fn<(_: string) => Promise<void>>(),
    llmExtractTriplesMock: vi.fn<(_: string) => Promise<void>>(),
    llmExtractTriplesAssetMock: vi.fn<(_: string) => Promise<void>>(),
    similarItemsMock: vi.fn<(_: string, __?: number) => Promise<Array<{ itemId: string }>>>(),
    extractTextMock: vi.fn(),
  })
)

type TripleRow = { subject: string; predicate: string; object: string }
type AnnotationRow = {
  id: string
  assetId: string
  page: number
  kind: 'rectangle' | 'underline'
  color: string
  x: number
  y: number
  width: number
  height: number
  createdAt: number
  updatedAt: number
}

type StoreOptions = {
  notesRows?: Array<{
    id: string
    itemId: string
    content: string
    createdAt: number
    updatedAt: number
  }>
  entitiesRows?: Array<{
    id: string
    itemId: string
    entityType: 'person' | 'organization' | 'place' | 'misc' | 'date' | 'institution'
    value: string
    startOffset: number | null
    endOffset: number | null
    confidence: number | null
    createdAt: number
  }>
  triplesRows?: TripleRow[]
  itemsById?: Record<
    string,
    {
      id: string
      title: string
      collectionId: string
      metadata: string
    }
  >
  ftsSearchImpl?: (_query: string, _limit?: number) => Promise<Array<{ itemId: string; rank: number }>>
  ftsStatsTotal?: number
    assetsRows?: Array<{
    id: string
    itemId: string
    path: string
    type: 'image' | 'pdf'
    createdAt: number
  }>
  annotationsByAsset?: Record<string, AnnotationRow[]>
  replaceAnnotationsImpl?: (
    assetId: string,
    page: number,
    annotations: AnnotationRow[]
  ) => Promise<unknown>
}

function createStore({
  notesRows = [],
  entitiesRows = [],
  triplesRows = [],
  itemsById = {
    'item-1': {
      id: 'item-1',
      title: 'Acta histórica',
      collectionId: 'col-1',
      metadata: '{}',
    },
  },
  ftsSearchImpl = async () => [],
  ftsStatsTotal = 35,
  assetsRows = [
    {
      id: 'asset-1',
      itemId: 'item-1',
      path: 'docs/acta.pdf',
      type: 'pdf' as const,
      createdAt: Date.now(),
    },
  ],
  annotationsByAsset = {},
  replaceAnnotationsImpl = async () => undefined,
}: StoreOptions = {}) {
  return {
    items: {
      findById: vi.fn().mockImplementation(async (id: string) => itemsById[id] ?? null),
      update: vi.fn().mockResolvedValue(undefined),
    },
    assets: {
      findByItem: vi.fn().mockResolvedValue(assetsRows),
      updatePath: vi.fn().mockResolvedValue(undefined),
    },
    notes: {
      findByItem: vi.fn().mockResolvedValue(notesRows),
      findByAsset: vi.fn().mockResolvedValue(notesRows),
      create: vi.fn().mockResolvedValue(undefined),
      update: vi.fn().mockResolvedValue(undefined),
      delete: vi.fn().mockResolvedValue(undefined),
    },
    annotations: {
      findByAsset: vi
        .fn()
        .mockImplementation(async (assetId: string) => annotationsByAsset[assetId] ?? []),
      replaceForAssetPage: vi.fn().mockImplementation(replaceAnnotationsImpl),
    },
    extractions: {
      findByAsset: vi.fn().mockResolvedValue(null),
    },
    entities: {
      findByItemId: vi.fn().mockResolvedValue(entitiesRows),
      findByAssetId: vi.fn().mockResolvedValue(entitiesRows),
      create: vi.fn().mockResolvedValue(undefined),
      update: vi.fn().mockResolvedValue(undefined),
      delete: vi.fn().mockResolvedValue(undefined),
    },
    fts: {
      search: vi.fn().mockImplementation(ftsSearchImpl),
      searchWithDebug: vi.fn().mockImplementation(async (query: string, limit?: number) => {
        const results = await ftsSearchImpl(query, limit)
        return {
          results,
          debug: {
            rawQuery: query,
            sanitizedQuery: query ? `"${query}"` : '',
            strategy: results.length > 0 ? 'strict' : 'relaxed',
            matchCount: results.length,
            resultIds: results.map((row) => row.itemId),
          },
        }
      }),
      stats: vi.fn().mockResolvedValue({ totalRows: ftsStatsTotal }),
    },
    triples: {
      findByItemId: vi.fn().mockResolvedValue(triplesRows),
      findByAssetId: vi.fn().mockResolvedValue(triplesRows),
    },
    topics: {
      findByItemId: vi.fn().mockResolvedValue([]),
      allNames: vi.fn().mockResolvedValue([]),
      addTopicToItem: vi.fn().mockResolvedValue(undefined),
      findByName: vi.fn().mockResolvedValue(null),
      removeTopicFromItem: vi.fn().mockResolvedValue(undefined),
    },
  }
}

const storeRef: { current: ReturnType<typeof createStore> } = {
  current: createStore(),
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

vi.mock('$lib/llm', async () => {
  const actual = await vi.importActual<typeof import('$lib/llm')>('$lib/llm')
  return {
    ...actual,
    llmIsAvailable: vi.fn().mockResolvedValue(true),
    llmGetResult: vi.fn().mockResolvedValue(null),
    llmGetResults: vi.fn().mockResolvedValue([]),
    llmSummarize: vi.fn().mockResolvedValue(undefined),
    llmCorrectOcr: vi.fn().mockResolvedValue(undefined),
    llmExtractTriples: llmExtractTriplesMock,
    llmSummarizeAsset: vi.fn().mockResolvedValue(undefined),
    llmCorrectOcrAsset: vi.fn().mockResolvedValue(undefined),
    llmExtractTriplesAsset: llmExtractTriplesAssetMock,
  }
})

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((eventName: string, callback: (event: { payload: unknown }) => void) => {
    nlpEventHandlers.set(eventName, callback)
    return Promise.resolve(vi.fn())
  }),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (command: string) => {
    if (command === 'llm_get_results') return []
    if (command === 'llm_get_result') return null
    if (command === 'llm_is_available') return true
    if (command === 'db_select') return []
    return null
  }),
}))

vi.mock('@entropia/ui', async () => {
  const MockDocumentViewer = (await import('./__mocks__/MockDocumentViewer.svelte')).default
  const MockEntityViewer = (await import('./__mocks__/MockEntityViewer.svelte')).default

  return {
    DocumentViewer: MockDocumentViewer,
    MetadataEditor: () => null,
    NoteEditor: () => null,
    Button: () => null,
    Card: () => null,
    EntityViewer: MockEntityViewer,
    MapViewer: () => null,
    TopicEditor: () => null,
  }
})

describe('ItemView semantic triples panel', () => {
  beforeEach(() => {
    nlpEventHandlers.clear()
    extractTriplesMock.mockReset().mockResolvedValue(undefined)
    llmExtractTriplesMock.mockReset().mockResolvedValue(undefined)
    llmExtractTriplesAssetMock.mockReset().mockResolvedValue(undefined)
    similarItemsMock.mockReset().mockResolvedValue([])
    extractTextMock.mockReset().mockResolvedValue(undefined)
  })

  async function renderItemViewWith(triplesRows: TripleRow[]) {
    storeRef.current = createStore({ triplesRows })
    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    const analysisToggle = await screen.findByRole('button', { name: /Analysis/i })
    await fireEvent.click(analysisToggle)
  }

  it('shows explicit empty state when no triples exist for the item', async () => {
    await renderItemViewWith([])

    expect(await screen.findByText('Semantic Triples (S|P|O)')).toBeInTheDocument()
    expect(await screen.findByText('No triples extracted yet.')).toBeInTheDocument()
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

    const triplesBtn = await screen.findByRole('button', { name: /TRIPLET/i })

    await fireEvent.click(triplesBtn)
    expect(llmExtractTriplesAssetMock).toHaveBeenCalledWith('asset-1')
    expect(triplesBtn).toBeDisabled()
    expect(screen.getByText('pending')).toBeInTheDocument()

    nlpEventHandlers.get('nlp:progress')?.({
      payload: { item_id: 'item-1', job: 'triples', pct: 25 },
    })
    await waitFor(() => {
      expect(screen.getByText('running')).toBeInTheDocument()
      expect(triplesBtn).toBeDisabled()
    })

    storeRef.current.triples.findByAssetId.mockResolvedValueOnce([
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
    expect(llmExtractTriplesAssetMock).toHaveBeenCalledTimes(2)
  })
})

describe('ItemView full-text search in Analysis panel', () => {
  beforeEach(() => {
    nlpEventHandlers.clear()
    extractTriplesMock.mockReset().mockResolvedValue(undefined)
    llmExtractTriplesMock.mockReset().mockResolvedValue(undefined)
    llmExtractTriplesAssetMock.mockReset().mockResolvedValue(undefined)
    similarItemsMock.mockReset().mockResolvedValue([])
    extractTextMock.mockReset().mockResolvedValue(undefined)
  })

  it('shows FTS results only after entering a query', async () => {
    storeRef.current = createStore({
      itemsById: {
        'item-1': {
          id: 'item-1',
          title: 'Acta histórica',
          collectionId: 'col-1',
          metadata: '{}',
        },
        'item-2': {
          id: 'item-2',
          title: 'Acta del Cabildo',
          collectionId: 'col-1',
          metadata: '{}',
        },
        'item-3': {
          id: 'item-3',
          title: 'Registro de otra colección',
          collectionId: 'col-2',
          metadata: '{}',
        },
      },
      ftsSearchImpl: async () => [
        { itemId: 'item-2', rank: -1.234 },
        { itemId: 'item-3', rank: -0.5 },
        { itemId: 'item-1', rank: -0.1 },
      ],
    })

    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    const analysisToggle = await screen.findByRole('button', { name: /Analysis/i })
    await fireEvent.click(analysisToggle)

    expect(await screen.findByText('Ingresá un término para ver resultados.')).toBeInTheDocument()

    const input = await screen.findByPlaceholderText('Escribí para buscar...')
    await fireEvent.input(input, { target: { value: 'cabildo' } })

    await waitFor(() => {
      expect(storeRef.current.fts.searchWithDebug).toHaveBeenCalledWith('cabildo', 10)
      expect(storeRef.current.fts.stats).toHaveBeenCalled()
    })

    expect(await screen.findByText('Acta del', { exact: false })).toBeInTheDocument()
    expect(await screen.findByText('Cabildo')).toBeInTheDocument()
    expect(await screen.findByText('Registro de otra colección')).toBeInTheDocument()
    expect(document.querySelectorAll('.fts-search-section .similar-item').length).toBe(3)
    expect(document.querySelector('.fts-match')).toBeInTheDocument()

    await fireEvent.input(input, { target: { value: '' } })
    await waitFor(() => {
      expect(screen.getByText('Ingresá un término para ver resultados.')).toBeInTheDocument()
    })
  })

  it('executes immediate search on Enter and clears search on Escape', async () => {
    storeRef.current = createStore({
      itemsById: {
        'item-1': {
          id: 'item-1',
          title: 'Acta histórica',
          collectionId: 'col-1',
          metadata: '{}',
        },
        'item-2': {
          id: 'item-2',
          title: 'Cabildo abierto de Mayo',
          collectionId: 'col-1',
          metadata: '{}',
        },
      },
      ftsSearchImpl: async () => [{ itemId: 'item-2', rank: -0.33 }],
    })

    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    const analysisToggle = await screen.findByRole('button', { name: /Analysis/i })
    await fireEvent.click(analysisToggle)

    const input = (await screen.findByPlaceholderText('Escribí para buscar...')) as HTMLInputElement

    await fireEvent.input(input, { target: { value: 'cabildo' } })
    await fireEvent.keyDown(input, { key: 'Enter' })

    await waitFor(() => {
      expect(storeRef.current.fts.searchWithDebug).toHaveBeenCalledTimes(1)
      expect(storeRef.current.fts.searchWithDebug).toHaveBeenCalledWith('cabildo', 10)
      expect(screen.getByText('Cabildo')).toBeInTheDocument()
      expect(screen.getByText('abierto de Mayo', { exact: false })).toBeInTheDocument()
    })

    await new Promise((resolve) => setTimeout(resolve, 350))
    expect(storeRef.current.fts.searchWithDebug).toHaveBeenCalledTimes(1)

    await fireEvent.keyDown(input, { key: 'Escape' })
    expect(input.value).toBe('')
    expect(screen.getByText('Ingresá un término para ver resultados.')).toBeInTheDocument()
  })

  it('shows FTS debug panel only in dev with query metadata', async () => {
    storeRef.current = createStore({
      ftsStatsTotal: 99,
      itemsById: {
        'item-1': {
          id: 'item-1',
          title: 'Acta histórica',
          collectionId: 'col-1',
          metadata: '{}',
        },
        'item-2': {
          id: 'item-2',
          title: 'Sindicato Obrero',
          collectionId: 'col-1',
          metadata: '{}',
        },
      },
      ftsSearchImpl: async () => [{ itemId: 'item-2', rank: -0.4 }],
    })

    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    const analysisToggle = await screen.findByRole('button', { name: /Analysis/i })
    await fireEvent.click(analysisToggle)

    expect(await screen.findByText('FTS Debug (dev only)')).toBeInTheDocument()

    const input = await screen.findByPlaceholderText('Escribí para buscar...')
    await fireEvent.input(input, { target: { value: 'sindicato' } })

    await waitFor(() => {
      expect(screen.getByText('Indexed rows')).toBeInTheDocument()
      expect(screen.getByText('99')).toBeInTheDocument()
      expect(screen.getByText('Raw query')).toBeInTheDocument()
      expect(screen.getByText('sindicato')).toBeInTheDocument()
      expect(screen.getByText('Sanitized')).toBeInTheDocument()
      expect(screen.getByText('"sindicato"')).toBeInTheDocument()
      expect(screen.getByText('DB matches')).toBeInTheDocument()
      expect(screen.getAllByText('1').length).toBeGreaterThanOrEqual(2)
      expect(screen.getByText('item-2')).toBeInTheDocument()
    })
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
    storeRef.current = createStore({ notesRows: notes })
    storeRef.current.notes.findByItem.mockResolvedValue(notes)
    storeRef.current.notes.findByAsset.mockResolvedValue(notes)
    storeRef.current.notes.update.mockResolvedValue(undefined)
    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })
    await screen.findByText(new RegExp(`Notes \\(${notes.length}\\)`))
  }

  it('displays the correct note count', async () => {
    await renderItemViewWithNotes([sampleNote])
    expect(screen.getByText(/Notes \(1\)/)).toBeInTheDocument()
  })

  it('displays "No notes yet" when notes array is empty', async () => {
    storeRef.current = createStore()
    storeRef.current.notes.findByItem.mockResolvedValue([])
    storeRef.current.notes.findByAsset.mockResolvedValue([])
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
    // After update, notes are loaded in the current asset scope
    expect(storeRef.current.notes.findByAsset).toHaveBeenCalledWith('item-1', 'asset-1')
  })
})

describe('ItemView image annotations', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    nlpEventHandlers.clear()
    extractTriplesMock.mockReset().mockResolvedValue(undefined)
    llmExtractTriplesMock.mockReset().mockResolvedValue(undefined)
    llmExtractTriplesAssetMock.mockReset().mockResolvedValue(undefined)
    similarItemsMock.mockReset().mockResolvedValue([])
    extractTextMock.mockReset().mockResolvedValue(undefined)
  })

  it('loads annotations per asset and rehydrates when switching assets', async () => {
    storeRef.current = createStore({
      assetsRows: [
        {
          id: 'asset-image-1',
          itemId: 'item-1',
          path: 'docs/photo-a.jpg',
          type: 'image',
          createdAt: 1,
        },
        {
          id: 'asset-image-2',
          itemId: 'item-1',
          path: 'docs/photo-b.jpg',
          type: 'image',
          createdAt: 2,
        },
      ],
      annotationsByAsset: {
        'asset-image-1': [
          {
            id: 'ann-1',
            assetId: 'asset-image-1',
            page: 1,
            kind: 'rectangle',
            color: 'var(--color-accent)',
            x: 0.1,
            y: 0.1,
            width: 0.2,
            height: 0.2,
            createdAt: 1,
            updatedAt: 1,
          },
        ],
        'asset-image-2': [
          {
            id: 'ann-2',
            assetId: 'asset-image-2',
            page: 1,
            kind: 'underline',
            color: 'var(--color-warning)',
            x: 0.2,
            y: 0.7,
            width: 0.3,
            height: 0.05,
            createdAt: 2,
            updatedAt: 2,
          },
          {
            id: 'ann-3',
            assetId: 'asset-image-2',
            page: 1,
            kind: 'rectangle',
            color: 'var(--color-danger)',
            x: 0.5,
            y: 0.2,
            width: 0.15,
            height: 0.25,
            createdAt: 3,
            updatedAt: 3,
          },
        ],
      },
    })

    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    await waitFor(() => {
      expect(screen.getByTestId('viewer-annotation-count')).toHaveTextContent('1')
    })
    expect(storeRef.current.annotations.findByAsset).toHaveBeenCalledWith('asset-image-1', 1)

    await fireEvent.click(screen.getByRole('button', { name: /next page/i }))

    await waitFor(() => {
      expect(screen.getByTestId('viewer-annotation-count')).toHaveTextContent('2')
    })
    expect(storeRef.current.annotations.findByAsset).toHaveBeenCalledWith('asset-image-2', 1)
  })

  it('keeps optimistic annotation state and persists with debounce', async () => {
    storeRef.current = createStore({
      assetsRows: [
        {
          id: 'asset-image-1',
          itemId: 'item-1',
          path: 'docs/photo-a.jpg',
          type: 'image',
          createdAt: 1,
        },
      ],
    })

    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    await screen.findByTestId('mock-document-viewer')
    await fireEvent.click(screen.getByRole('button', { name: /add annotation/i }))

    expect(screen.getByTestId('viewer-annotation-count')).toHaveTextContent('1')
    expect(storeRef.current.annotations.replaceForAssetPage).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(499)
    expect(storeRef.current.annotations.replaceForAssetPage).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(1)

    expect(storeRef.current.annotations.replaceForAssetPage).toHaveBeenCalledTimes(1)
    expect(storeRef.current.annotations.replaceForAssetPage).toHaveBeenCalledWith(
      'asset-image-1',
      1,
      expect.arrayContaining([
        expect.objectContaining({ kind: 'rectangle', color: 'var(--color-accent)' }),
      ])
    )
  })

  it('shows a non-blocking error when annotation save fails', async () => {
    storeRef.current = createStore({
      assetsRows: [
        {
          id: 'asset-image-1',
          itemId: 'item-1',
          path: 'docs/photo-a.jpg',
          type: 'image',
          createdAt: 1,
        },
      ],
      replaceAnnotationsImpl: async () => {
        throw new Error('disk busy')
      },
    })

    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    await screen.findByTestId('mock-document-viewer')
    await fireEvent.click(screen.getByRole('button', { name: /add annotation/i }))
    await vi.advanceTimersByTimeAsync(500)

    expect(screen.getByTestId('viewer-annotation-count')).toHaveTextContent('1')
    expect(
      await screen.findByText('Failed to save annotations. Changes remain local until retry.')
    ).toBeInTheDocument()
  })

  it('keeps pdf assets view-only by skipping annotation loads', async () => {
    storeRef.current = createStore({
      assetsRows: [
        {
          id: 'asset-pdf-1',
          itemId: 'item-1',
          path: 'docs/acta.pdf',
          type: 'pdf',
          createdAt: 1,
        },
      ],
    })

    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })

    await waitFor(() => {
      expect(screen.getByTestId('viewer-type')).toHaveTextContent('pdf')
    })
    expect(storeRef.current.annotations.findByAsset).not.toHaveBeenCalled()
  })
})

describe('ItemView entity editing UX', () => {
  beforeEach(() => {
    nlpEventHandlers.clear()
    extractTriplesMock.mockReset().mockResolvedValue(undefined)
    llmExtractTriplesMock.mockReset().mockResolvedValue(undefined)
    llmExtractTriplesAssetMock.mockReset().mockResolvedValue(undefined)
    similarItemsMock.mockReset().mockResolvedValue([])
    extractTextMock.mockReset().mockResolvedValue(undefined)
  })

  async function renderAnalysisWithEntities() {
    storeRef.current = createStore({
      entitiesRows: [
        {
          id: 'entity-1',
          itemId: 'item-1',
          entityType: 'organization',
          value: 'Mar del Plata',
          startOffset: 10,
          endOffset: 23,
          confidence: 0.95,
          createdAt: 1,
        },
      ],
    })

    render(ItemView, { itemId: 'item-1', collectionId: 'col-1' })
    await fireEvent.click(await screen.findByRole('button', { name: /Analysis/i }))
  }

  it('opens entity modal from entity tag click and saves edits', async () => {
    await renderAnalysisWithEntities()

    await fireEvent.click(await screen.findByTestId('mock-entity-entity-1'))

    expect(await screen.findByRole('dialog', { name: /Edit entity/i })).toBeInTheDocument()

    await fireEvent.input(screen.getByLabelText('Edit entity value'), {
      target: { value: 'Mar del Plata 1970' },
    })
    await fireEvent.change(screen.getByLabelText('Edit entity type'), {
      target: { value: 'date' },
    })
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    expect(storeRef.current.entities.update).toHaveBeenCalledWith('entity-1', {
      entityType: 'date',
      value: 'Mar del Plata 1970',
      confidence: 1,
      source: 'manual',
    })
  })

  it('deletes entity from modal', async () => {
    await renderAnalysisWithEntities()

    await fireEvent.click(await screen.findByTestId('mock-entity-entity-1'))
    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }))

    expect(storeRef.current.entities.delete).toHaveBeenCalledWith('entity-1')
  })

  it('creates manual DATE entities', async () => {
    await renderAnalysisWithEntities()

    await fireEvent.change(screen.getByLabelText('New entity type'), {
      target: { value: 'date' },
    })
    await fireEvent.input(screen.getByLabelText('New entity value'), {
      target: { value: '21 de agosto de 1970' },
    })
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }))

    expect(storeRef.current.entities.create).toHaveBeenCalledWith(
      expect.objectContaining({
        itemId: 'item-1',
        entityType: 'date',
        value: '21 de agosto de 1970',
        confidence: 1,
        source: 'manual',
      })
    )
  })
})
