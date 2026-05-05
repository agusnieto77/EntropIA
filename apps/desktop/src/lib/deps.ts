/**
 * Dependency manager frontend client for EntropIA desktop app.
 * Wraps Tauri commands for the Python dependency manager (uv-based).
 */

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type DependencyId =
  | 'Python'
  | 'Fastembed'
  | 'PaddleOcr'
  | 'FasterWhisper'
  | 'Spacy'
  | 'SpacyModelEs'

export type DependencyStatus =
  | { type: 'unknown' }
  | { type: 'checking' }
  | { type: 'installed'; version?: string }
  | { type: 'missing' }
  | { type: 'installing'; percent: number }
  | { type: 'failed'; message: string }

export interface DepCheckResult {
  id: DependencyId
  status: DependencyStatus
  version: string | null
}

export interface UvStatusResult {
  uv_ready: boolean
  uv_path: string | null
  uv_version: string | null
  venv_exists: boolean
  venv_path: string | null
}

export interface DepsProgressEvent {
  id: DependencyId
  status: DependencyStatus
  message: string
}

export interface DepsCompleteEvent {
  results: DepCheckResult[]
  all_critical_installed: boolean
}

export interface DepsErrorEvent {
  stage: string
  error: string
  recoverable: boolean
}

// ---------------------------------------------------------------------------
// Invoke wrappers
// ---------------------------------------------------------------------------

export function checkAllDeps(): Promise<DepCheckResult[]> {
  return invoke<DepCheckResult[]>('deps_check_all')
}

export function installAllDeps(): Promise<void> {
  return invoke<void>('deps_install_all')
}

export function installOneDep(id: DependencyId): Promise<DepCheckResult> {
  return invoke<DepCheckResult>('deps_install_one', { id })
}

export function getUvStatus(): Promise<UvStatusResult> {
  return invoke<UvStatusResult>('deps_get_uv_status')
}

export function resetDeps(): Promise<void> {
  return invoke<void>('deps_reset')
}

// ---------------------------------------------------------------------------
// Display metadata (Spanish)
// ---------------------------------------------------------------------------

export const DEP_DISPLAY_NAMES: Record<DependencyId, string> = {
  Python: 'Python 3.11',
  Fastembed: 'Fastembed (embeddings)',
  PaddleOcr: 'PaddleOCR (OCR principal)',
  FasterWhisper: 'Faster Whisper (transcripción)',
  Spacy: 'spaCy (NER)',
  SpacyModelEs: 'Modelo spaCy español',
}

export const CRITICAL_DEPS: DependencyId[] = ['Python', 'Fastembed', 'PaddleOcr']

export const DEP_DESCRIPTIONS: Record<DependencyId, string> = {
  Python: 'Intérprete Python requerido para todas las funciones de IA',
  Fastembed: 'Motor de embeddings para búsqueda semántica',
  PaddleOcr: 'Motor principal de reconocimiento óptico de caracteres',
  FasterWhisper: 'Transcripción de audio a texto',
  Spacy: 'Reconocimiento de entidades nombradas',
  SpacyModelEs: 'Modelo de lenguaje español para spaCy',
}

// ---------------------------------------------------------------------------
// Event listener helpers
// ---------------------------------------------------------------------------

export function onDepsProgress(callback: (event: DepsProgressEvent) => void): Promise<UnlistenFn> {
  return listen<DepsProgressEvent>('deps://progress', (e) => callback(e.payload))
}

export function onDepsComplete(callback: (event: DepsCompleteEvent) => void): Promise<UnlistenFn> {
  return listen<DepsCompleteEvent>('deps://complete', (e) => callback(e.payload))
}

export function onDepsError(callback: (event: DepsErrorEvent) => void): Promise<UnlistenFn> {
  return listen<DepsErrorEvent>('deps://error', (e) => callback(e.payload))
}
