import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import DependenciasTab from './DependenciasTab.svelte'

const depsMocks = vi.hoisted(() => ({
  checkAllDeps: vi.fn(),
  installAllDeps: vi.fn(),
  installOneDep: vi.fn(),
  getUvStatus: vi.fn(),
  resetDeps: vi.fn(),
  onDepsProgress: vi.fn(),
  onDepsComplete: vi.fn(),
  onDepsError: vi.fn(),
  getRuntimeStatus: vi.fn(),
  repairRuntime: vi.fn(),
  onRuntimeStatus: vi.fn(),
  onRuntimeProgress: vi.fn(),
  runtimeCanBootstrapAutomatically: vi.fn(),
}))

vi.mock('$lib/deps', () => ({
  checkAllDeps: depsMocks.checkAllDeps,
  installAllDeps: depsMocks.installAllDeps,
  installOneDep: depsMocks.installOneDep,
  getUvStatus: depsMocks.getUvStatus,
  resetDeps: depsMocks.resetDeps,
  onDepsProgress: depsMocks.onDepsProgress,
  onDepsComplete: depsMocks.onDepsComplete,
  onDepsError: depsMocks.onDepsError,
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

vi.mock('$lib/runtime', () => ({
  getRuntimeStatus: depsMocks.getRuntimeStatus,
  repairRuntime: depsMocks.repairRuntime,
  onRuntimeStatus: depsMocks.onRuntimeStatus,
  onRuntimeProgress: depsMocks.onRuntimeProgress,
  runtimeNeedsAttention: (status: { state?: string } | null | undefined) =>
    status != null && ['repairing', 'damaged', 'fixture', 'incompatible', 'blocked_source_unavailable', 'blocked_offline', 'checking', 'hydrating', 'verifying', 'downloading'].includes(status.state ?? ''),
  shouldShowRuntimeRepairAction: (status: { state?: string; repairAvailable?: boolean } | null | undefined) =>
    status?.repairAvailable === true && !['repairing', 'fixture', 'incompatible', 'blocked_source_unavailable', 'blocked_offline'].includes(status?.state ?? ''),
  runtimeCanBootstrapAutomatically: depsMocks.runtimeCanBootstrapAutomatically,
}))

describe('DependenciasTab', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    depsMocks.checkAllDeps.mockResolvedValue([
      { id: 'Python', status: { type: 'installed', version: '3.11.9' }, version: '3.11.9' },
    ])
    depsMocks.getUvStatus.mockResolvedValue({
      uv_ready: true,
      uv_path: '/runtime/uv',
      uv_version: '0.6.0',
      uv_source: 'managed-runtime',
      uv_compatible_for_dev: true,
      venv_exists: true,
      venv_path: '/runtime/venv',
      uv_warning: null,
      release_runtime_ready: true,
      release_runtime_state: 'healthy',
      dev_fallback_available: false,
      dev_fallback_reason: null,
    })
    depsMocks.onDepsProgress.mockResolvedValue(vi.fn())
    depsMocks.onDepsComplete.mockResolvedValue(vi.fn())
    depsMocks.onDepsError.mockResolvedValue(vi.fn())
    depsMocks.onRuntimeStatus.mockResolvedValue(vi.fn())
    depsMocks.onRuntimeProgress.mockResolvedValue(vi.fn())
    depsMocks.runtimeCanBootstrapAutomatically.mockReturnValue(false)
    depsMocks.getRuntimeStatus.mockResolvedValue({
      state: 'healthy',
      packVersion: '2026.05.0',
      repairNeeded: false,
      repairAvailable: false,
      summary: 'Runtime listo',
      blockedCapabilities: [],
      details: [],
      guidance: [],
      bootstrapEligible: false,
      bootstrapRequired: false,
      activeOperation: null,
    })
    depsMocks.repairRuntime.mockResolvedValue({
      state: 'healthy',
      packVersion: '2026.05.0',
      repairNeeded: false,
      repairAvailable: false,
      summary: 'Runtime listo',
      blockedCapabilities: [],
      details: [],
      guidance: [],
      bootstrapEligible: false,
      bootstrapRequired: false,
      activeOperation: null,
    })
  })

  it('shows runtime status details and repair CTA for damaged runtime', async () => {
    depsMocks.getRuntimeStatus.mockResolvedValueOnce({
      state: 'damaged',
      packVersion: '2026.05.0',
      repairNeeded: true,
      repairAvailable: true,
      summary: 'Runtime dañado',
      blockedCapabilities: ['ocr', 'nlp'],
      details: ['Checksum inválido'],
      guidance: ['Ejecutá la reparación del runtime desde Ajustes > Dependencias.'],
      bootstrapEligible: true,
      bootstrapRequired: true,
      activeOperation: null,
    })

    render(DependenciasTab)

    expect(await screen.findByText('Runtime dañado')).toBeInTheDocument()
    expect(screen.getByText(/ocr, nlp/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Reparar runtime' })).toBeInTheDocument()
  })

  it('invokes runtime repair from the dedicated runtime panel', async () => {
    depsMocks.getRuntimeStatus.mockResolvedValueOnce({
      state: 'damaged',
      packVersion: '2026.05.0',
      repairNeeded: true,
      repairAvailable: true,
      summary: 'Runtime dañado',
      blockedCapabilities: ['transcription'],
      details: ['Falta transcribe.py'],
      guidance: ['Podés intentar reparar el runtime.'],
      bootstrapEligible: true,
      bootstrapRequired: true,
      activeOperation: null,
    })

    render(DependenciasTab)

    const repairButton = await screen.findByRole('button', { name: 'Reparar runtime' })
    await fireEvent.click(repairButton)

    await waitFor(() => {
      expect(depsMocks.repairRuntime).toHaveBeenCalledTimes(1)
    })
  })

  it('shows fixture runtime messaging without exposing repair CTA', async () => {
    depsMocks.getRuntimeStatus.mockResolvedValueOnce({
      state: 'fixture',
      packVersion: '2026.05.0',
      repairNeeded: false,
      repairAvailable: false,
      summary: 'Runtime de desarrollo detectado para linux-x86_64: faltan payloads externos de release',
      blockedCapabilities: ['ocr', 'transcription', 'nlp'],
      details: ['La app 0.0.10 arrancó correctamente, pero este runtime-pack todavía está en modo fixture/dev (app_version declarada: 0.0.10).'],
      guidance: ['Próximo paso manual inevitable: inyectar los artefactos externos requeridos al runtime-pack de release para esta plataforma.'],
      bootstrapEligible: false,
      bootstrapRequired: true,
      activeOperation: null,
    })

    render(DependenciasTab)

    expect(await screen.findByText(/Runtime de desarrollo detectado/i)).toBeInTheDocument()
    expect(screen.getByText(/Próximo paso manual inevitable/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Reparar runtime' })).not.toBeInTheDocument()
  })

  it('shows a non-crash uv warning when a different system uv version is detected', async () => {
    depsMocks.getUvStatus.mockResolvedValueOnce({
      uv_ready: false,
      uv_path: '/usr/bin/uv',
      uv_version: '0.10.3',
      uv_source: 'system-dev-fallback',
      uv_compatible_for_dev: true,
      venv_exists: false,
      venv_path: null,
      uv_warning:
        'Se detectó uv 0.10.3 en /usr/bin/uv, pero EntropIA espera uv 0.6.14 para instalaciones administradas. En desarrollo esto explica el warning, no una caída de la app.',
      release_runtime_ready: false,
      release_runtime_state: 'fixture',
      dev_fallback_available: true,
      dev_fallback_reason:
        'Linux debug: si falta el runtime de release, EntropIA puede crear un venv local usando Python/uv del sistema. Esto NO valida ni reemplaza el contrato de runtime-pack de release.',
    })

    render(DependenciasTab)

    expect(await screen.findByText(/Detectado: uv 0\.10\.3 en \/usr\/bin\/uv/i)).toBeInTheDocument()
    expect(screen.getByText(/no una caída de la app/i)).toBeInTheDocument()
    expect(screen.getByText(/Fallback dev disponible/i)).toBeInTheDocument()
    expect(screen.getByText(/NO valida ni reemplaza el contrato/i)).toBeInTheDocument()
  })

  it('keeps install disabled when fixture runtime has no usable dev fallback', async () => {
    depsMocks.checkAllDeps.mockResolvedValueOnce([
      { id: 'Python', status: { type: 'missing' }, version: null },
    ])
    depsMocks.getRuntimeStatus.mockResolvedValueOnce({
      state: 'fixture',
      packVersion: '2026.05.0',
      repairNeeded: false,
      repairAvailable: false,
      summary: 'Runtime fixture',
      blockedCapabilities: ['ocr', 'transcription', 'nlp'],
      details: [],
      guidance: [],
      bootstrapEligible: false,
      bootstrapRequired: true,
      activeOperation: null,
    })
    depsMocks.getUvStatus.mockResolvedValueOnce({
      uv_ready: false,
      uv_path: null,
      uv_version: null,
      uv_source: null,
      uv_compatible_for_dev: false,
      venv_exists: false,
      venv_path: null,
      uv_warning: null,
      release_runtime_ready: false,
      release_runtime_state: 'fixture',
      dev_fallback_available: false,
      dev_fallback_reason: 'Fallback de desarrollo no disponible: falta Python 3.11+ y también falta un uv del sistema utilizable.',
    })

    render(DependenciasTab)

    const button = await screen.findByRole('button', { name: 'Instalar todo' })
    expect(button).toBeDisabled()
    expect(
      screen.getByText(/Necesitás runtime release hidratado o, en Linux dev, tener Python \+ uv del sistema disponibles/i),
    ).toBeInTheDocument()
  })

  it('shows blocked bootstrap reason and active operation progress honestly', async () => {
    depsMocks.getRuntimeStatus.mockResolvedValueOnce({
      state: 'blocked_source_unavailable',
      packVersion: '2026.05.0',
      repairNeeded: false,
      repairAvailable: false,
      summary: 'No hay una fuente confiable disponible',
      blockedCapabilities: ['ocr', 'transcription'],
      details: ['manifest not published'],
      guidance: ['Reintentá cuando exista una fuente confiable'],
      bootstrapEligible: false,
      bootstrapRequired: true,
      activeOperation: {
        kind: 'bootstrap',
        stage: 'blocked',
        summary: 'Bootstrap bloqueado por falta de source',
        progressPercent: 45,
        downloadedBytes: 45,
        totalBytes: 100,
        retryable: true,
      },
    })

    render(DependenciasTab)

    expect(await screen.findByText(/No hay una fuente confiable disponible/i)).toBeInTheDocument()
    expect(screen.getByText(/manifest not published/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Reparar runtime' })).not.toBeInTheDocument()
  })
})
