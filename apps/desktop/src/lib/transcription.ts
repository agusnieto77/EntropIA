/**
 * Transcription frontend client for EntropIA desktop app.
 * Plain TypeScript (not .svelte.ts) for full testability in Vitest.
 *
 * Communicates with the Rust backend via Tauri invoke + event listeners.
 * Mirrors the OcrStore architecture for consistency.
 */

import { invoke } from '@tauri-apps/api/core'

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

export type TranscriptionStatus = 'idle' | 'pending' | 'running' | 'done' | 'error'

export interface TranscriptionProgress {
  assetId: string
  pct: number
  stage: string
}

export interface TranscriptionResult {
  assetId: string
  text: string
  language: string
  durationMs: number
  segmentsCount: number
}

export interface AssetTranscriptionState {
  status: TranscriptionStatus
  progress: number
  error?: string
  text?: string
  language?: string
  durationMs?: number
  segmentsCount?: number
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
  text: string
  language: string
  duration_ms: number
  segments_count: number
}

interface ErrorPayload {
  asset_id: string
  error: string
}

// ─────────────────────────────────────────────────────────────────────────────
// TranscriptionStore
// ─────────────────────────────────────────────────────────────────────────────

const IDLE_STATE: AssetTranscriptionState = { status: 'idle', progress: 0 }

export interface TranscriptionStoreOptions {
  /** Called when a transcription job completes successfully with the assetId. */
  onComplete?: (assetId: string) => void
}

export class TranscriptionStore {
  private states = new Map<string, AssetTranscriptionState>()
  private cleanupFns: Array<() => void> = []
  private onComplete?: (assetId: string) => void

  constructor(options?: TranscriptionStoreOptions) {
    this.onComplete = options?.onComplete
  }

  /** Returns the current transcription state for an asset, or idle if unknown. */
  getState(assetId: string): AssetTranscriptionState {
    return this.states.get(assetId) ?? { ...IDLE_STATE }
  }

  /**
   * Registers Tauri event listeners for transcription:progress, transcription:complete,
   * transcription:error. The `listen` function is injected for testability.
   */
  async startListening(
    listen: (event: string, callback: (e: { payload: unknown }) => void) => Promise<() => void>
  ): Promise<void> {
    const unlistenProgress = await listen('transcription:progress', (e) => {
      const p = e.payload as ProgressPayload
      this._updateState(p.asset_id, { status: 'running', progress: p.pct })
    })

    const unlistenComplete = await listen('transcription:complete', (e) => {
      const p = e.payload as CompletePayload
      this._updateState(p.asset_id, {
        status: 'done',
        progress: 100,
        text: p.text,
        language: p.language,
        durationMs: p.duration_ms,
        segmentsCount: p.segments_count,
      })
      // Notify caller (e.g., to trigger FTS indexing after transcription completes)
      this.onComplete?.(p.asset_id)
    })

    const unlistenError = await listen('transcription:error', (e) => {
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
  _updateState(assetId: string, partial: Partial<AssetTranscriptionState>): void {
    const current = this.states.get(assetId) ?? { ...IDLE_STATE }
    this.states.set(assetId, { ...current, ...partial })
  }

  /** Updates the transcription text content (user edit). */
  setTextContent(assetId: string, text: string): void {
    const current = this.states.get(assetId)
    if (!current) return
    this.states.set(assetId, { ...current, text })
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// transcribeAudio — triggers a backend transcription job
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Calls the Rust `transcribe_audio` command to kick off a transcription job.
 * Sets the asset state to 'pending' before the invocation resolves.
 */
export async function transcribeAudio(assetId: string, assetPath: string): Promise<void> {
  await invoke('transcribe_audio', { assetId, assetPath })
}
