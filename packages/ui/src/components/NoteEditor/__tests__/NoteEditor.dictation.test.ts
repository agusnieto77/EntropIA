import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import NoteEditor from '../NoteEditor.svelte'

class FakeMediaRecorder {
  static instances: FakeMediaRecorder[] = []

  public state: 'inactive' | 'recording' = 'inactive'
  public mimeType = 'audio/webm'
  public ondataavailable: ((event: { data: Blob }) => void) | null = null
  public onstop: (() => void) | null = null
  public stream: MediaStream

  constructor(stream: MediaStream) {
    this.stream = stream
    FakeMediaRecorder.instances.push(this)
  }

  start() {
    this.state = 'recording'
  }

  stop() {
    this.state = 'inactive'
    this.ondataavailable?.({ data: new Blob(['audio'], { type: this.mimeType }) })
    this.onstop?.()
  }
}

describe('NoteEditor dictation', () => {
  const getUserMediaMock = vi.fn<() => Promise<MediaStream>>()
  const stopTrackMock = vi.fn()

  beforeEach(() => {
    FakeMediaRecorder.instances = []
    stopTrackMock.mockReset()
    getUserMediaMock.mockReset()

    Object.defineProperty(globalThis, 'MediaRecorder', {
      configurable: true,
      value: FakeMediaRecorder,
    })

    Object.defineProperty(globalThis.navigator, 'mediaDevices', {
      configurable: true,
      value: {
        getUserMedia: getUserMediaMock,
      },
    })

    getUserMediaMock.mockResolvedValue({
      getTracks: () => [{ stop: stopTrackMock }],
    } as unknown as MediaStream)
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('renders the microphone button only when dictation is enabled', () => {
    const { rerender } = render(NoteEditor, { props: {} })
    expect(screen.queryByRole('button', { name: 'Iniciar dictado' })).not.toBeInTheDocument()

    rerender({ ondictate: vi.fn() })
    expect(screen.getByRole('button', { name: 'Iniciar dictado' })).toBeInTheDocument()
  })

  it('shows a non intrusive message when microphone APIs are unavailable', async () => {
    Object.defineProperty(globalThis, 'MediaRecorder', {
      configurable: true,
      value: undefined,
    })

    render(NoteEditor, { props: { ondictate: vi.fn() } })

    await fireEvent.click(screen.getByRole('button', { name: 'Iniciar dictado' }))

    expect(screen.getByTestId('note-editor-dictation-message')).toHaveTextContent(
      'No hay micrófono disponible en este dispositivo.'
    )
  })

  it('records, transcribes, and appends the text when no cursor selection is active', async () => {
    const ondictate = vi.fn().mockResolvedValue('texto dictado')
    render(NoteEditor, { props: { ondictate, content: '<p>Hola </p>' } })

    await fireEvent.click(screen.getByRole('button', { name: 'Iniciar dictado' }))

    expect(screen.getByRole('button', { name: 'Detener dictado' })).toBeInTheDocument()

    await fireEvent.click(screen.getByRole('button', { name: 'Detener dictado' }))

    await waitFor(() => {
      expect(ondictate).toHaveBeenCalledOnce()
      expect(screen.getByRole('textbox')).toHaveTextContent('Hola texto dictado')
    })

    expect(stopTrackMock).toHaveBeenCalledOnce()
  })

  it('adds spacing when dictation is inserted after a word without trailing whitespace', async () => {
    const ondictate = vi.fn().mockResolvedValue('texto dictado')
    render(NoteEditor, { props: { ondictate, content: '<p>Hola</p>' } })

    await fireEvent.click(screen.getByRole('button', { name: 'Iniciar dictado' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Detener dictado' }))

    await waitFor(() => {
      expect(screen.getByRole('textbox')).toHaveTextContent('Hola texto dictado')
    })
  })

  it('auto stops at the configured limit and shows a brief message', async () => {
    vi.useFakeTimers()
    const ondictate = vi.fn().mockResolvedValue('texto automático')
    render(NoteEditor, { props: { ondictate, dictationMaxSeconds: 2 } })

    await fireEvent.click(screen.getByRole('button', { name: 'Iniciar dictado' }))

    await vi.advanceTimersByTimeAsync(2100)

    await waitFor(() => {
      expect(ondictate).toHaveBeenCalledOnce()
      expect(screen.getByTestId('note-editor-dictation-message')).toHaveTextContent(
        'Se alcanzó el máximo de 0:02. Texto insertado.'
      )
    })
  })
})
