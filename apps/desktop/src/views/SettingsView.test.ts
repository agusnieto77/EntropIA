import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import SettingsView from './SettingsView.svelte'
import { locale } from '$lib/i18n'

const { settingsGetMock, settingsSetMock, testOpenrouterConnectionMock, llmIsAvailableMock } =
  vi.hoisted(() => ({
    settingsGetMock: vi.fn(),
    settingsSetMock: vi.fn(),
    testOpenrouterConnectionMock: vi.fn(),
    llmIsAvailableMock: vi.fn(),
  }))

vi.mock('$lib/settings', async () => {
  const actual = await vi.importActual<typeof import('$lib/settings')>('$lib/settings')
  return {
    ...actual,
    settingsGet: settingsGetMock,
    settingsSet: settingsSetMock,
    testOpenrouterConnection: testOpenrouterConnectionMock,
  }
})

vi.mock('$lib/llm', () => ({
  llmIsAvailable: llmIsAvailableMock,
}))

describe('SettingsView', () => {
  beforeEach(() => {
    locale.set('es')
    settingsGetMock.mockReset()
    settingsSetMock.mockReset().mockResolvedValue(undefined)
    testOpenrouterConnectionMock.mockReset()
    llmIsAvailableMock.mockReset().mockResolvedValue(true)

    settingsGetMock.mockImplementation(async (key: string) => {
      if (key === 'openrouter_api_key') return 'sk-or-v1-test-key'
      if (key === 'openrouter_model') return 'anthropic/claude-3.7-sonnet'
      if (key === 'llm_mode') return 'openrouter'
      if (key === 'language') return 'es'
      return null
    })
  })

  it('renders the unified header hierarchy with the active mode summary', async () => {
    render(SettingsView)

    expect(await screen.findByText('Preferencias')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Configuración' })).toBeInTheDocument()
    expect(
      screen.getByText(
        'Ajustá cómo EntropIA resuelve tareas locales y remotas de inteligencia artificial.'
      )
    ).toBeInTheDocument()

    await waitFor(() => {
      expect(screen.getByText('Modo actual: OpenRouter')).toBeInTheDocument()
    })
  })

  it('shows refined success feedback for connection checks and saves', async () => {
    testOpenrouterConnectionMock.mockResolvedValue([
      { id: 'google/gemma-3-4b-it', name: 'Gemma 3 4B', context_length: 8192 },
      { id: 'anthropic/claude-3.7-sonnet', name: 'Claude 3.7 Sonnet', context_length: 200000 },
    ])

    render(SettingsView)

    await screen.findByRole('button', { name: 'Probar conexión' })

    await fireEvent.click(screen.getByRole('button', { name: 'Probar conexión' }))

    expect(await screen.findByText('Conexión lista · 2 modelos disponibles.')).toBeInTheDocument()
    expect(screen.getByText('Modelos sugeridos desde OpenRouter')).toBeInTheDocument()

    await fireEvent.click(screen.getByRole('button', { name: 'Guardar cambios' }))

    expect(
      await screen.findByText(
        'Configuración guardada. Ya podés usar esta preferencia en toda la app.'
      )
    ).toBeInTheDocument()
  })

  it('saves language preference and updates the interface reactively', async () => {
    render(SettingsView)

    const languageSelect = await screen.findByLabelText('Idioma')
    await fireEvent.change(languageSelect, { target: { value: 'en' } })
    expect((languageSelect as HTMLSelectElement).value).toBe('en')

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Settings' })).toBeInTheDocument()
    })

    await fireEvent.click(screen.getByRole('button', { name: 'Save changes' }))

    await waitFor(() => {
      expect(settingsSetMock).toHaveBeenNthCalledWith(4, 'language', 'en')
      expect(screen.getByRole('heading', { name: 'Settings' })).toBeInTheDocument()
    })
  })
})
