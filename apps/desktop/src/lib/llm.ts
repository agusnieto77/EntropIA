/**
 * LLM frontend client for EntropIA desktop app.
 * Communicates with the Rust LLM backend (Gemma 4 via llama.cpp).
 * Mirrors the NlpStore architecture.
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

export type LlmJobType =
  | 'correct_ocr'
  | 'extract_entities'
  | 'extract_triples'
  | 'summarize'
  | 'classify'
  | 'ask'

export type LlmStatus = 'idle' | 'pending' | 'running' | 'done' | 'error'

export interface ItemLlmState {
  status: LlmStatus
  activeJob: LlmJobType | null
  result: string | null
  error: string | null
}

export interface LlmResultEntry {
  target_id: string
  job_type: string
  result: string
  created_at: number
}

// ─────────────────────────────────────────────────────────────────────────────
// Payload shapes emitted by the Rust backend
// ─────────────────────────────────────────────────────────────────────────────

interface LlmProgressPayload {
  id: string
  job: string
  pct: number
}

interface LlmCompletePayload {
  id: string
  job: string
  result: string
}

interface LlmErrorPayload {
  id: string
  job: string
  error: string
}

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

export class LlmStore {
  private state: Map<string, ItemLlmState> = new Map()
  private listeners: Array<() => void> = []
  private unlisteners: UnlistenFn[] = []
  private onComplete?: (id: string, job: string, result: string) => void

  constructor(opts?: { onComplete?: (id: string, job: string, result: string) => void }) {
    this.onComplete = opts?.onComplete
  }

  private defaultState(): ItemLlmState {
    return { status: 'idle', activeJob: null, result: null, error: null }
  }

  getState(id: string): ItemLlmState {
    return this.state.get(id) ?? this.defaultState()
  }

  private update(id: string, patch: Partial<ItemLlmState>) {
    const current = this.getState(id)
    this.state.set(id, { ...current, ...patch })
    this.listeners.forEach((fn) => fn())
  }

  /**
   * Hydrate the store from persisted results for a given target.
   * Call this on mount to restore state after a page reload.
   */
  async loadPersistedResults(targetId: string): Promise<void> {
    try {
      const results: LlmResultEntry[] = await invoke('llm_get_results', { targetId })
      for (const entry of results) {
        this.update(entry.target_id, {
          status: 'done',
          activeJob: null,
          result: entry.result,
          error: null,
        })
      }
    } catch (e) {
      // Silently degrade — persisted results are optional
      console.warn('[LlmStore] Failed to load persisted results:', e)
    }
  }

  onChange(fn: () => void) {
    this.listeners.push(fn)
  }

  async startListening() {
    this.unlisteners.push(
      await listen<LlmProgressPayload>('llm:progress', (event) => {
        const { id, job, pct } = event.payload
        this.update(id, {
          status: pct < 100 ? 'running' : 'done',
          activeJob: job as LlmJobType,
        })
      }),
      await listen<LlmCompletePayload>('llm:complete', (event) => {
        const { id, job, result } = event.payload
        this.update(id, {
          status: 'done',
          activeJob: null,
          result,
          error: null,
        })
        this.onComplete?.(id, job, result)
      }),
      await listen<LlmErrorPayload>('llm:error', (event) => {
        const { id, job, error } = event.payload
        this.update(id, {
          status: 'error',
          activeJob: null,
          error,
        })
      }),
    )
  }

  stopListening() {
    this.unlisteners.forEach((fn) => fn())
    this.unlisteners = []
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Invoke helpers
// ─────────────────────────────────────────────────────────────────────────────

export function llmCorrectOcr(itemId: string): Promise<string> {
  return invoke<string>('llm_correct_ocr', { itemId })
}

export function llmExtractEntities(itemId: string): Promise<string> {
  return invoke<string>('llm_extract_entities', { itemId })
}

export function llmExtractTriples(itemId: string): Promise<string> {
  return invoke<string>('llm_extract_triples', { itemId })
}

export function llmSummarize(itemId: string): Promise<string> {
  return invoke<string>('llm_summarize', { itemId })
}

export function llmClassify(itemId: string, categories: string[]): Promise<string> {
  return invoke<string>('llm_classify', { itemId, categories })
}

export function llmAsk(collectionId: string, question: string): Promise<string> {
  return invoke<string>('llm_ask', { collectionId, question })
}

/** Retrieve all latest LLM results for a target (item or collection). */
export function llmGetResults(targetId: string): Promise<LlmResultEntry[]> {
  return invoke<LlmResultEntry[]>('llm_get_results', { targetId })
}

/** Retrieve the latest single LLM result for a target + job type. */
export function llmGetResult(targetId: string, jobType: string): Promise<LlmResultEntry | null> {
  return invoke<LlmResultEntry | null>('llm_get_result', { targetId, jobType })
}