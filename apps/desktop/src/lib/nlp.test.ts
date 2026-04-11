import { describe, it, expect, beforeEach, vi } from 'vitest'
import { NlpStore, indexFts, embedItem, extractEntities, ftsSearch, similarItems } from './nlp'

// Mocks are set up in test-setup.ts:
//   @tauri-apps/api/core  → invoke vi.fn()
//   @tauri-apps/api/event → listen vi.fn() returning Promise<vi.fn()>

const { invoke } = await import('@tauri-apps/api/core')
const { listen } = await import('@tauri-apps/api/event')

describe('NlpStore', () => {
  let store: NlpStore

  beforeEach(() => {
    store = new NlpStore()
    vi.clearAllMocks()
  })

  // ─────────────────────────────────────────────────────────────────────────
  // getState
  // ─────────────────────────────────────────────────────────────────────────

  it('getState returns idle for all job types for unknown itemId', () => {
    const state = store.getState('unknown-item')
    expect(state.fts).toBe('idle')
    expect(state.embed).toBe('idle')
    expect(state.ner).toBe('idle')
  })

  // ─────────────────────────────────────────────────────────────────────────
  // invoke wrappers
  // ─────────────────────────────────────────────────────────────────────────

  it('indexFts calls invoke with correct command and itemId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('queued')
    await indexFts('item-1')
    expect(invoke).toHaveBeenCalledWith('index_fts', { itemId: 'item-1' })
  })

  it('embedItem calls invoke with correct command and itemId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('queued')
    await embedItem('item-2')
    expect(invoke).toHaveBeenCalledWith('embed_item', { itemId: 'item-2' })
  })

  it('extractEntities calls invoke with correct command and itemId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('queued')
    await extractEntities('item-3')
    expect(invoke).toHaveBeenCalledWith('extract_entities', { itemId: 'item-3' })
  })

  it('ftsSearch calls invoke with query and optional collectionId', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([])
    await ftsSearch('historia colonial', 'col-1')
    expect(invoke).toHaveBeenCalledWith('fts_search', {
      query: 'historia colonial',
      collectionId: 'col-1',
    })
  })

  it('similarItems calls invoke with itemId and default limit', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([])
    await similarItems('item-4')
    expect(invoke).toHaveBeenCalledWith('similar_items', { itemId: 'item-4', limit: 5 })
  })

  // ─────────────────────────────────────────────────────────────────────────
  // startListening — nlp:progress
  // ─────────────────────────────────────────────────────────────────────────

  it('nlp:progress for fts job sets fts status to running', async () => {
    let progressCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'nlp:progress') {
        progressCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    progressCallback!({ payload: { item_id: 'item-fts', job: 'fts', pct: 50 } })

    const state = store.getState('item-fts')
    expect(state.fts).toBe('running')
    expect(state.embed).toBe('idle')
    expect(state.ner).toBe('idle')
  })

  it('nlp:progress for embed job sets embed status to running', async () => {
    let progressCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'nlp:progress') {
        progressCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    progressCallback!({ payload: { item_id: 'item-emb', job: 'embed', pct: 10 } })

    const state = store.getState('item-emb')
    expect(state.embed).toBe('running')
    expect(state.fts).toBe('idle')
    expect(state.ner).toBe('idle')
  })

  it('nlp:progress for ner job sets ner status to running', async () => {
    let progressCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'nlp:progress') {
        progressCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    progressCallback!({ payload: { item_id: 'item-ner', job: 'ner', pct: 10 } })

    const state = store.getState('item-ner')
    expect(state.ner).toBe('running')
    expect(state.fts).toBe('idle')
    expect(state.embed).toBe('idle')
  })

  // ─────────────────────────────────────────────────────────────────────────
  // startListening — nlp:complete
  // ─────────────────────────────────────────────────────────────────────────

  it('nlp:complete for fts job transitions fts to done', async () => {
    let completeCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'nlp:complete') {
        completeCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    completeCallback!({ payload: { item_id: 'item-1', job: 'fts' } })

    const state = store.getState('item-1')
    expect(state.fts).toBe('done')
    expect(state.embed).toBe('idle')
    expect(state.ner).toBe('idle')
  })

  it('nlp:complete for embed job transitions embed to done', async () => {
    let completeCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'nlp:complete') {
        completeCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    completeCallback!({ payload: { item_id: 'item-2', job: 'embed' } })

    expect(store.getState('item-2').embed).toBe('done')
  })

  it('nlp:complete for ner job transitions ner to done', async () => {
    let completeCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'nlp:complete') {
        completeCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    completeCallback!({ payload: { item_id: 'item-3', job: 'ner' } })

    expect(store.getState('item-3').ner).toBe('done')
  })

  // ─────────────────────────────────────────────────────────────────────────
  // startListening — nlp:error
  // ─────────────────────────────────────────────────────────────────────────

  it('nlp:error for fts job transitions fts to error with message', async () => {
    let errorCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'nlp:error') {
        errorCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    errorCallback!({ payload: { item_id: 'item-err', job: 'fts', error: 'FTS index failed' } })

    const state = store.getState('item-err')
    expect(state.fts).toBe('error')
    expect(state.errors?.fts).toBe('FTS index failed')
  })

  it('nlp:error for embed job transitions embed to error', async () => {
    let errorCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'nlp:error') {
        errorCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    errorCallback!({ payload: { item_id: 'item-err2', job: 'embed', error: 'fastembed failed' } })

    const state = store.getState('item-err2')
    expect(state.embed).toBe('error')
    expect(state.errors?.embed).toBe('fastembed failed')
  })

  // ─────────────────────────────────────────────────────────────────────────
  // stopListening
  // ─────────────────────────────────────────────────────────────────────────

  it('stopListening calls all cleanup functions', async () => {
    const cleanup1 = vi.fn()
    const cleanup2 = vi.fn()
    const cleanup3 = vi.fn()

    let callCount = 0
    vi.mocked(listen).mockImplementation(() => {
      callCount++
      if (callCount === 1) return Promise.resolve(cleanup1)
      if (callCount === 2) return Promise.resolve(cleanup2)
      return Promise.resolve(cleanup3)
    })

    await store.startListening(listen)
    store.stopListening()

    expect(cleanup1).toHaveBeenCalledOnce()
    expect(cleanup2).toHaveBeenCalledOnce()
    expect(cleanup3).toHaveBeenCalledOnce()
  })

  it('stopListening is safe to call without startListening', () => {
    expect(() => store.stopListening()).not.toThrow()
  })
})
