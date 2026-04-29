import { describe, it, expect, vi, beforeEach } from 'vitest'

// Mock @tauri-apps/api/core
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}))

import { invoke } from '@tauri-apps/api/core'
import {
  settingsGet,
  settingsSet,
  settingsGetAll,
  settingsDelete,
  testOpenrouterConnection,
  SETTINGS_KEYS,
  DEFAULT_OPENROUTER_MODEL,
  DEFAULT_LLM_MODE,
} from './settings'

const mockInvoke = vi.mocked(invoke)

beforeEach(() => {
  vi.clearAllMocks()
})

describe('settings', () => {
  describe('settingsGet', () => {
    it('calls invoke with correct command and key', async () => {
      mockInvoke.mockResolvedValueOnce('test-value')
      const result = await settingsGet('my_key')
      expect(mockInvoke).toHaveBeenCalledWith('settings_get', { key: 'my_key' })
      expect(result).toBe('test-value')
    })

    it('returns null when setting does not exist', async () => {
      mockInvoke.mockResolvedValueOnce(null)
      const result = await settingsGet('nonexistent')
      expect(result).toBeNull()
    })
  })

  describe('settingsSet', () => {
    it('calls invoke with correct command, key and value', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)
      await settingsSet('my_key', 'my_value')
      expect(mockInvoke).toHaveBeenCalledWith('settings_set', {
        key: 'my_key',
        value: 'my_value',
      })
    })
  })

  describe('settingsGetAll', () => {
    it('returns array of settings', async () => {
      const mockSettings = [
        { key: 'a', value: '1' },
        { key: 'b', value: '2' },
      ]
      mockInvoke.mockResolvedValueOnce(mockSettings)
      const result = await settingsGetAll()
      expect(mockInvoke).toHaveBeenCalledWith('settings_get_all')
      expect(result).toEqual(mockSettings)
    })
  })

  describe('settingsDelete', () => {
    it('calls invoke with correct command and key', async () => {
      mockInvoke.mockResolvedValueOnce(undefined)
      await settingsDelete('my_key')
      expect(mockInvoke).toHaveBeenCalledWith('settings_delete', { key: 'my_key' })
    })
  })

  describe('testOpenrouterConnection', () => {
    it('calls invoke with api key', async () => {
      const mockModels = [
        { id: 'google/gemma-3-4b-it', name: 'Gemma 3 4B', context_length: 8192 },
      ]
      mockInvoke.mockResolvedValueOnce(mockModels)
      const result = await testOpenrouterConnection('sk-or-test')
      expect(mockInvoke).toHaveBeenCalledWith('test_openrouter_connection', {
        apiKey: 'sk-or-test',
      })
      expect(result).toEqual(mockModels)
    })
  })

  describe('constants', () => {
    it('exports well-known setting keys', () => {
      expect(SETTINGS_KEYS.OPENROUTER_API_KEY).toBe('openrouter_api_key')
      expect(SETTINGS_KEYS.OPENROUTER_MODEL).toBe('openrouter_model')
      expect(SETTINGS_KEYS.LLM_MODE).toBe('llm_mode')
    })

    it('has correct defaults', () => {
      expect(DEFAULT_OPENROUTER_MODEL).toBe('google/gemma-3-4b-it')
      expect(DEFAULT_LLM_MODE).toBe('local')
    })
  })
})
