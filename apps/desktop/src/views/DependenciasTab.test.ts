import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import DependenciasTab from './DependenciasTab.svelte'

const {
  checkAllDepsMock,
  getUvStatusMock,
  resetDepsMock,
  installAllDepsMock,
  installOneDepMock,
  onDepsProgressMock,
  onDepsCompleteMock,
  onDepsErrorMock,
} = vi.hoisted(() => ({
  checkAllDepsMock: vi.fn(),
  getUvStatusMock: vi.fn(),
  resetDepsMock: vi.fn(),
  installAllDepsMock: vi.fn(),
  installOneDepMock: vi.fn(),
  onDepsProgressMock: vi.fn(),
  onDepsCompleteMock: vi.fn(),
  onDepsErrorMock: vi.fn(),
}))

vi.mock('$lib/deps', () => ({
  checkAllDeps: checkAllDepsMock,
  installAllDeps: installAllDepsMock,
  installOneDep: installOneDepMock,
  getUvStatus: getUvStatusMock,
  resetDeps: resetDepsMock,
  onDepsProgress: onDepsProgressMock,
  onDepsComplete: onDepsCompleteMock,
  onDepsError: onDepsErrorMock,
  DEP_DISPLAY_NAMES: {
    Python: 'Python 3.11',
    Fastembed: 'Fastembed (embeddings)',
    PaddleOcr: 'PaddleOCR (OCR principal)',
    FasterWhisper: 'Faster Whisper (transcripción)',
    Spacy: 'spaCy (NER)',
    SpacyModelEs: 'Modelo spaCy español',
  },
  DEP_DESCRIPTIONS: {
    Python: 'Intérprete Python requerido para todas las funciones de IA',
    Fastembed: 'Motor de embeddings para búsqueda semántica',
    PaddleOcr: 'Motor principal de reconocimiento óptico de caracteres',
    FasterWhisper: 'Transcripción de audio a texto',
    Spacy: 'Reconocimiento de entidades nombradas',
    SpacyModelEs: 'Modelo de lenguaje español para spaCy',
  },
  CRITICAL_DEPS: ['Python', 'Fastembed', 'PaddleOcr'],
}))

describe('DependenciasTab', () => {
  beforeEach(() => {
    checkAllDepsMock.mockReset().mockResolvedValue([
      { id: 'Python', status: { type: 'installed', version: '3.11.9' }, version: '3.11.9' },
      { id: 'Fastembed', status: { type: 'installed', version: '0.2.0' }, version: '0.2.0' },
    ])
    getUvStatusMock.mockReset().mockResolvedValue({
      uv_ready: true,
      uv_path: 'C:/tools/uv.exe',
      uv_version: '0.5.0',
      venv_exists: true,
      venv_path: 'C:/EntropIA/.venv',
    })
    resetDepsMock.mockReset().mockResolvedValue(undefined)
    installAllDepsMock.mockReset().mockResolvedValue(undefined)
    installOneDepMock.mockReset().mockResolvedValue({
      id: 'Python',
      status: { type: 'installed', version: '3.11.9' },
      version: '3.11.9',
    })
    onDepsProgressMock.mockReset().mockResolvedValue(() => {})
    onDepsCompleteMock.mockReset().mockResolvedValue(() => {})
    onDepsErrorMock.mockReset().mockResolvedValue(() => {})
  })

  it('requires typing the confirmation phrase before enabling reset', async () => {
    render(DependenciasTab)

    await fireEvent.click(await screen.findByRole('button', { name: 'Resetear entorno' }))

    expect(screen.getByRole('dialog', { name: 'Resetear entorno' })).toBeInTheDocument()
    const confirmButton = screen.getAllByRole('button', { name: 'Resetear entorno' })[1]
    const confirmationInput = screen.getByLabelText('Escribí la frase exacta')

    expect(screen.getByText('Acción destructiva')).toBeInTheDocument()
    expect(screen.getByText('Esta acción no se puede deshacer desde la app.')).toBeInTheDocument()
    expect(screen.getByText('RESETEAR ENTORNO')).toBeInTheDocument()

    expect(confirmButton).toBeDefined()
    if (!confirmButton) throw new Error('Reset confirmation button was not found')

    expect(confirmButton).toBeDisabled()

    await fireEvent.input(confirmationInput, { target: { value: 'resetear entorno' } })
    expect(confirmButton).toBeDisabled()

    await fireEvent.input(confirmationInput, { target: { value: 'RESETEAR ENTORNO' } })
    expect(confirmButton).toBeEnabled()

    await fireEvent.click(confirmButton)

    await waitFor(() => {
      expect(resetDepsMock).toHaveBeenCalledTimes(1)
      expect(checkAllDepsMock).toHaveBeenCalledTimes(2)
      expect(getUvStatusMock).toHaveBeenCalledTimes(2)
    })

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('lets the user cancel the destructive confirmation dialog', async () => {
    render(DependenciasTab)

    await fireEvent.click(await screen.findByRole('button', { name: 'Resetear entorno' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Cancelar' }))

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(resetDepsMock).not.toHaveBeenCalled()
  })
})
