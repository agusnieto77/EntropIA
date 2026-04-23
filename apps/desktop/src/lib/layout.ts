/**
 * Layout detection frontend client for EntropIA desktop app.
 * Plain TypeScript (not .svelte.ts) for full testability in Vitest.
 *
 * Communicates with the Rust backend via Tauri invoke + event listeners.
 * Mirrors the OcrStore and TranscriptionStore architecture for consistency.
 */

import { invoke } from '@tauri-apps/api/core'

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

export type LayoutStatus = 'idle' | 'pending' | 'running' | 'done' | 'error'

export interface LayoutProgress {
  assetId: string
  pct: number
  stage: string
}

export interface LayoutRegion {
  category: string
  bbox: { x: number; y: number; width: number; height: number }
  confidence: number
  reading_order: number
}

export interface LayoutResult {
  assetId: string
  regionsCount: number
  model: string
  regions: LayoutRegion[]
}

export interface AssetLayoutState {
  status: LayoutStatus
  progress: number
  error?: string
  regionsCount?: number
  model?: string
  regions?: LayoutRegion[]
}

// ─────────────────────────────────────────────────────────────────────────────
// Payload shapes emitted by the Rust backend
// ─────────────────────────────────────────────────────────────────────────────

interface ProgressPayload {
  asset_id: string
  pct: number
  stage: string
}

interface CompletePayload {
  asset_id: string
  regions_count: number
  model: string
  regions_json: string
}

interface ErrorPayload {
  asset_id: string
  error: string
}

// ─────────────────────────────────────────────────────────────────────────────
// LayoutStore
// ─────────────────────────────────────────────────────────────────────────────

const IDLE_STATE: AssetLayoutState = { status: 'idle', progress: 0 }

export interface LayoutStoreOptions {
  /** Called when a layout detection job completes successfully with the assetId. */
  onComplete?: (assetId: string) => void
}

export class LayoutStore {
  private states = new Map<string, AssetLayoutState>()
  private cleanupFns: Array<() => void> = []
  private onComplete?: (assetId: string) => void

  constructor(options?: LayoutStoreOptions) {
    this.onComplete = options?.onComplete
  }

  /** Returns the current layout state for an asset, or idle if unknown. */
  getState(assetId: string): AssetLayoutState {
    return this.states.get(assetId) ?? { ...IDLE_STATE }
  }

  /**
   * Registers Tauri event listeners for layout:progress, layout:complete,
   * layout:error. The `listen` function is injected for testability.
   */
  async startListening(
    listen: (event: string, callback: (e: { payload: unknown }) => void) => Promise<() => void>
  ): Promise<void> {
    const unlistenProgress = await listen('layout:progress', (e) => {
      const p = e.payload as ProgressPayload
      this._updateState(p.asset_id, { status: 'running', progress: p.pct })
    })

    const unlistenComplete = await listen('layout:complete', (e) => {
      const p = e.payload as CompletePayload
      let regions: LayoutRegion[] | undefined
      try {
        regions = JSON.parse(p.regions_json) as LayoutRegion[]
      } catch {
        regions = undefined
      }
      this._updateState(p.asset_id, {
        status: 'done',
        progress: 100,
        regionsCount: p.regions_count,
        model: p.model,
        regions,
      })
      this.onComplete?.(p.asset_id)
    })

    const unlistenError = await listen('layout:error', (e) => {
      const p = e.payload as ErrorPayload
      this._updateState(p.asset_id, { status: 'error', error: p.error })
    })

    this.cleanupFns = [unlistenProgress, unlistenComplete, unlistenError]
  }

  /** Calls all cleanup functions returned by listen(), removing event listeners. */
  stopListening(): void {
    for (const fn of this.cleanupFns) {
      fn()
    }
    this.cleanupFns = []
  }

  /** Merges partial state into the map for the given assetId. */
  _updateState(assetId: string, partial: Partial<AssetLayoutState>): void {
    const current = this.states.get(assetId) ?? { ...IDLE_STATE }
    this.states.set(assetId, { ...current, ...partial })
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// extractLayout — triggers a backend layout detection job
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Calls the Rust `extract_layout` command to kick off a layout detection job.
 */
export async function extractLayout(assetId: string, assetPath: string): Promise<void> {
  await invoke('extract_layout', { assetId, assetPath })
}
