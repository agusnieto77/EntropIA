import { describe, it, expect, beforeEach, vi } from 'vitest'
import { LayoutStore, extractLayout } from './layout'

// Mocks are set up in test-setup.ts:
//   @tauri-apps/api/core  → invoke vi.fn()
//   @tauri-apps/api/event → listen vi.fn() returning Promise<vi.fn()>

const { invoke } = await import('@tauri-apps/api/core')
const { listen } = await import('@tauri-apps/api/event')

describe('LayoutStore', () => {
  let store: LayoutStore

  beforeEach(() => {
    store = new LayoutStore()
    vi.clearAllMocks()
  })

  // ─────────────────────────────────────────────────────────────────────────
  // getState
  // ─────────────────────────────────────────────────────────────────────────

  it('getState returns idle for unknown assetId', () => {
    const state = store.getState('unknown-asset')
    expect(state.status).toBe('idle')
    expect(state.progress).toBe(0)
  })

  it('getState returns idle with no error or regionsCount for unknown assetId', () => {
    const state = store.getState('another-asset')
    expect(state.status).toBe('idle')
    expect(state.progress).toBe(0)
    expect(state.error).toBeUndefined()
    expect(state.regionsCount).toBeUndefined()
  })

  // ─────────────────────────────────────────────────────────────────────────
  // extractLayout
  // ─────────────────────────────────────────────────────────────────────────

  it('extractLayout calls invoke with correct arguments', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined)

    await extractLayout('asset-1', '/path/to/file.pdf')

    expect(invoke).toHaveBeenCalledWith('extract_layout', {
      assetId: 'asset-1',
      assetPath: '/path/to/file.pdf',
    })
  })

  it('extractLayout calls invoke with correct arguments for images', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined)

    await extractLayout('asset-img', '/images/photo.jpg')

    expect(invoke).toHaveBeenCalledWith('extract_layout', {
      assetId: 'asset-img',
      assetPath: '/images/photo.jpg',
    })
  })

  // ─────────────────────────────────────────────────────────────────────────
  // startListening — layout:progress
  // ─────────────────────────────────────────────────────────────────────────

  it('startListening on layout:progress event updates state to running with pct', async () => {
    let progressCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'layout:progress') {
        progressCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    expect(progressCallback).not.toBeNull()

    progressCallback!({ payload: { asset_id: 'asset-1', pct: 45, stage: 'detecting' } })

    const state = store.getState('asset-1')
    expect(state.status).toBe('running')
    expect(state.progress).toBe(45)
  })

  it('startListening on layout:progress updates to correct pct value', async () => {
    let progressCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'layout:progress') {
        progressCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    progressCallback!({ payload: { asset_id: 'asset-2', pct: 100, stage: 'done' } })

    const state = store.getState('asset-2')
    expect(state.progress).toBe(100)
    expect(state.status).toBe('running')
  })

  // ─────────────────────────────────────────────────────────────────────────
  // startListening — layout:complete
  // ─────────────────────────────────────────────────────────────────────────

  it('startListening on layout:complete event updates status to done with regionsCount and model', async () => {
    let completeCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'layout:complete') {
        completeCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    completeCallback!({
      payload: {
        asset_id: 'asset-3',
        regions_count: 12,
        model: 'doclayout_yolo',
        regions_json: '[{"category":"title","bbox":{"x":10,"y":10,"width":200,"height":30},"confidence":0.95,"reading_order":0}]',
      },
    })

    const state = store.getState('asset-3')
    expect(state.status).toBe('done')
    expect(state.regionsCount).toBe(12)
    expect(state.model).toBe('doclayout_yolo')
    expect(state.progress).toBe(100)
  })

  // ─────────────────────────────────────────────────────────────────────────
  // startListening — layout:error
  // ─────────────────────────────────────────────────────────────────────────

  it('startListening on layout:error event updates status to error with error message', async () => {
    let errorCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'layout:error') {
        errorCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    errorCallback!({
      payload: { asset_id: 'asset-err', error: 'Layout model not found' },
    })

    const state = store.getState('asset-err')
    expect(state.status).toBe('error')
    expect(state.error).toBe('Layout model not found')
  })

  it('startListening on layout:error with different error message', async () => {
    let errorCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'layout:error') {
        errorCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await store.startListening(listen)

    errorCallback!({
      payload: { asset_id: 'asset-err2', error: 'Python not found' },
    })

    const state = store.getState('asset-err2')
    expect(state.status).toBe('error')
    expect(state.error).toBe('Python not found')
  })

  // ─────────────────────────────────────────────────────────────────────────
  // stopListening
  // ─────────────────────────────────────────────────────────────────────────

  it('stopListening calls cleanup functions returned by listen', async () => {
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

  // ─────────────────────────────────────────────────────────────────────────
  // onComplete callback
  // ─────────────────────────────────────────────────────────────────────────

  it('calls onComplete callback with assetId when layout:complete fires', async () => {
    const onComplete = vi.fn()
    const storeWithCallback = new LayoutStore({ onComplete })

    let completeCallback: ((event: { payload: unknown }) => void) | null = null

    vi.mocked(listen).mockImplementation((eventName, callback) => {
      if (eventName === 'layout:complete') {
        completeCallback = callback as (event: { payload: unknown }) => void
      }
      return Promise.resolve(vi.fn())
    })

    await storeWithCallback.startListening(listen)

    completeCallback!({
      payload: {
        asset_id: 'asset-layout-done',
        regions_count: 8,
        model: 'doclayout_yolo',
        regions_json: '[]',
      },
    })

    expect(onComplete).toHaveBeenCalledWith('asset-layout-done')
    const state = storeWithCallback.getState('asset-layout-done')
    expect(state.status).toBe('done')
  })
})
