/**
 * Settings frontend client for EntropIA desktop app.
 * Wraps Tauri commands for the app_settings key-value store.
 */

import { invoke } from '@tauri-apps/api/core'

export interface SettingEntry {
  key: string
  value: string
}

export interface ModelInfo {
  id: string
  name: string
  context_length: number
}

// ---------------------------------------------------------------------------
// Settings CRUD
// ---------------------------------------------------------------------------

export function settingsGet(key: string): Promise<string | null> {
  return invoke<string | null>('settings_get', { key })
}

export function settingsSet(key: string, value: string): Promise<void> {
  return invoke<void>('settings_set', { key, value })
}

export function settingsGetAll(): Promise<SettingEntry[]> {
  return invoke<SettingEntry[]>('settings_get_all')
}

export function settingsDelete(key: string): Promise<void> {
  return invoke<void>('settings_delete', { key })
}

// ---------------------------------------------------------------------------
// OpenRouter-specific
// ---------------------------------------------------------------------------

export function testOpenrouterConnection(apiKey: string): Promise<ModelInfo[]> {
  return invoke<ModelInfo[]>('test_openrouter_connection', { apiKey })
}

// ---------------------------------------------------------------------------
// Well-known setting keys
// ---------------------------------------------------------------------------

export const SETTINGS_KEYS = {
  OPENROUTER_API_KEY: 'openrouter_api_key',
  OPENROUTER_MODEL: 'openrouter_model',
  LLM_MODE: 'llm_mode',
  LANGUAGE: 'language',
  DEPS_VENV_PYTHON_PATH: 'deps_venv_python_path',
} as const

export type LlmMode = 'local' | 'openrouter' | 'auto'

export const DEFAULT_OPENROUTER_MODEL = 'google/gemma-3-4b-it'
export const DEFAULT_LLM_MODE: LlmMode = 'local'
