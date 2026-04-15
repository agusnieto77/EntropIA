import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import DocumentViewer from '../DocumentViewer.svelte'

type ResizeObserverCallback = globalThis.ResizeObserverCallback

class MockResizeObserver {
  static instances: MockResizeObserver[] = []

  callback: ResizeObserverCallback

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback
    MockResizeObserver.instances.push(this)
  }

  observe = vi.fn()
  disconnect = vi.fn()

  trigger(target: Element) {
    this.callback(
      [
        {
          target,
          contentRect: target.getBoundingClientRect(),
        } as ResizeObserverEntry,
      ],
      this as unknown as ResizeObserver
    )
  }
}

vi.stubGlobal('ResizeObserver', MockResizeObserver)

// Mock pdfjs-dist for test environment
vi.mock('pdfjs-dist', () => {
  const mockPage = {
    getViewport: vi.fn(() => ({ width: 800, height: 600, scale: 1 })),
    render: vi.fn(() => ({ promise: Promise.resolve() })),
  }
  const mockDocument = {
    numPages: 3,
    getPage: vi.fn(() => Promise.resolve(mockPage)),
  }
  return {
    getDocument: vi.fn(() => ({ promise: Promise.resolve(mockDocument) })),
    GlobalWorkerOptions: { workerSrc: '' },
  }
})

describe('DocumentViewer', () => {
  beforeEach(() => {
    MockResizeObserver.instances = []
  })

  function setImageSize(img: HTMLImageElement, width: number, height: number) {
    Object.defineProperty(img, 'clientWidth', { configurable: true, value: width })
    Object.defineProperty(img, 'clientHeight', { configurable: true, value: height })
    img.getBoundingClientRect = vi.fn(() => ({
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: width,
      bottom: height,
      width,
      height,
      toJSON: () => ({}),
    }))
  }

  describe('image mode', () => {
    it('renders an img element with the asset URL', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/image.jpg',
          type: 'image',
          assetUrl: 'asset://localhost/path/to/image.jpg',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })
      const img = screen.getByRole('img')
      expect(img).toBeInTheDocument()
      expect(img).toHaveAttribute('src', 'asset://localhost/path/to/image.jpg')
    })

    it('renders annotation toolbar in image mode and hides pdf controls', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/image.jpg',
          type: 'image',
          assetUrl: 'asset://localhost/path/to/image.jpg',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })

      expect(screen.getByTestId('annotation-toolbar')).toBeInTheDocument()
      expect(screen.queryByTestId('pdf-controls')).not.toBeInTheDocument()
    })

    it('creates a rectangle annotation from normalized drag geometry', async () => {
      const onAnnotationsChange = vi.fn()

      render(DocumentViewer, {
        props: {
          path: '/path/to/image.jpg',
          type: 'image',
          assetUrl: 'asset://localhost/path/to/image.jpg',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'rectangle',
          annotationColor: 'var(--color-accent)',
          onAnnotationsChange,
        },
      })

      const img = screen.getByRole('img') as HTMLImageElement
      setImageSize(img, 200, 100)
      await fireEvent.load(img)
      MockResizeObserver.instances[0]?.trigger(img)

      const overlay = await screen.findByTestId('annotation-overlay')
      overlay.getBoundingClientRect = vi.fn(() => ({
        x: 0,
        y: 0,
        top: 0,
        left: 0,
        right: 200,
        bottom: 100,
        width: 200,
        height: 100,
        toJSON: () => ({}),
      }))

      await fireEvent.pointerDown(overlay, { clientX: 20, clientY: 10, button: 0 })
      await fireEvent.pointerMove(overlay, { clientX: 120, clientY: 60, button: 0 })
      await fireEvent.pointerUp(overlay, { clientX: 120, clientY: 60, button: 0 })

      expect(onAnnotationsChange).toHaveBeenCalledTimes(1)
      expect(onAnnotationsChange).toHaveBeenCalledWith(
        expect.arrayContaining([
          expect.objectContaining({
            kind: 'rectangle',
            color: 'var(--color-accent)',
            x: 0.1,
            y: 0.1,
            width: 0.5,
            height: 0.5,
            page: 1,
          }),
        ])
      )
    })

    it('selects, recolors, deletes, and deselects annotations', async () => {
      const onAnnotationsChange = vi.fn()
      const onSelectedAnnotationIdChange = vi.fn()
      const onAnnotationColorChange = vi.fn()

      const annotation = {
        id: 'ann-1',
        assetId: 'asset-1',
        page: 1,
        kind: 'rectangle' as const,
        color: 'var(--color-accent)',
        x: 0.1,
        y: 0.2,
        width: 0.4,
        height: 0.3,
        createdAt: 10,
        updatedAt: 10,
      }

      const view = render(DocumentViewer, {
        props: {
          path: '/path/to/image.jpg',
          type: 'image',
          assetUrl: 'asset://localhost/path/to/image.jpg',
          annotations: [annotation],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
          onAnnotationsChange,
          onSelectedAnnotationIdChange,
          onAnnotationColorChange,
        },
      })

      const img = screen.getByRole('img') as HTMLImageElement
      setImageSize(img, 200, 100)
      await fireEvent.load(img)
      MockResizeObserver.instances[0]?.trigger(img)

      const shape = await screen.findByTestId('annotation-shape-ann-1')
      await fireEvent.click(shape)
      expect(onSelectedAnnotationIdChange).toHaveBeenCalledWith('ann-1')

      await view.rerender({
        path: '/path/to/image.jpg',
        type: 'image',
        assetUrl: 'asset://localhost/path/to/image.jpg',
        annotations: [annotation],
        selectedAnnotationId: 'ann-1',
        annotationTool: 'select',
        annotationColor: 'var(--color-accent)',
        onAnnotationsChange,
        onSelectedAnnotationIdChange,
        onAnnotationColorChange,
      })

      await fireEvent.click(screen.getByRole('button', { name: /warning annotation color/i }))
      expect(onAnnotationColorChange).toHaveBeenCalledWith('var(--color-warning)')
      expect(onAnnotationsChange).toHaveBeenCalledWith([
        expect.objectContaining({ id: 'ann-1', color: 'var(--color-warning)' }),
      ])

      await fireEvent.click(screen.getByRole('button', { name: /delete selected annotation/i }))
      expect(onAnnotationsChange).toHaveBeenCalledWith([])
      expect(onSelectedAnnotationIdChange).toHaveBeenCalledWith(null)

      const overlay = screen.getByTestId('annotation-overlay')
      overlay.getBoundingClientRect = vi.fn(() => ({
        x: 0,
        y: 0,
        top: 0,
        left: 0,
        right: 200,
        bottom: 100,
        width: 200,
        height: 100,
        toJSON: () => ({}),
      }))

      await fireEvent.pointerDown(overlay, { clientX: 199, clientY: 99, button: 0 })
      expect(onSelectedAnnotationIdChange).toHaveBeenLastCalledWith(null)
    })

    it('keeps overlay alignment after image resize', async () => {
      const annotation = {
        id: 'ann-1',
        assetId: 'asset-1',
        page: 1,
        kind: 'rectangle' as const,
        color: 'var(--color-accent)',
        x: 0.25,
        y: 0.1,
        width: 0.5,
        height: 0.4,
        createdAt: 10,
        updatedAt: 10,
      }

      render(DocumentViewer, {
        props: {
          path: '/path/to/image.jpg',
          type: 'image',
          assetUrl: 'asset://localhost/path/to/image.jpg',
          annotations: [annotation],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })

      const img = screen.getByRole('img') as HTMLImageElement
      setImageSize(img, 200, 100)
      await fireEvent.load(img)

      const observer = MockResizeObserver.instances[0]
      observer?.trigger(img)

      const shape = await screen.findByTestId('annotation-shape-ann-1')
      expect(shape).toHaveAttribute('x', '50')
      expect(shape).toHaveAttribute('y', '10')
      expect(shape).toHaveAttribute('width', '100')
      expect(shape).toHaveAttribute('height', '40')

      setImageSize(img, 400, 200)
      observer?.trigger(img)

      await waitFor(() => {
        expect(shape).toHaveAttribute('x', '100')
        expect(shape).toHaveAttribute('y', '20')
        expect(shape).toHaveAttribute('width', '200')
        expect(shape).toHaveAttribute('height', '80')
      })
    })
  })

  describe('pdf mode', () => {
    it('renders a canvas element for PDF', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/doc.pdf',
          type: 'pdf',
          assetUrl: 'asset://localhost/path/to/doc.pdf',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })
      const canvas = screen.getByTestId('pdf-canvas')
      expect(canvas).toBeInTheDocument()
    })

    it('renders PDF navigation controls', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/doc.pdf',
          type: 'pdf',
          assetUrl: 'asset://localhost/path/to/doc.pdf',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })
      expect(screen.getByTestId('pdf-controls')).toBeInTheDocument()
      expect(screen.getByTestId('pdf-prev')).toBeInTheDocument()
      expect(screen.getByTestId('pdf-next')).toBeInTheDocument()
      expect(screen.getByTestId('pdf-zoom-in')).toBeInTheDocument()
      expect(screen.getByTestId('pdf-zoom-out')).toBeInTheDocument()
    })

    it('shows loading state initially', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/doc.pdf',
          type: 'pdf',
          assetUrl: 'asset://localhost/path/to/doc.pdf',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })
      expect(screen.getByTestId('pdf-loading')).toBeInTheDocument()
    })

    it('prev button is disabled on first page', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/doc.pdf',
          type: 'pdf',
          assetUrl: 'asset://localhost/path/to/doc.pdf',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })
      expect(screen.getByTestId('pdf-prev')).toBeDisabled()
    })

    it('keeps annotation controls inactive for PDFs', () => {
      const onAnnotationsChange = vi.fn()

      render(DocumentViewer, {
        props: {
          path: '/path/to/doc.pdf',
          type: 'pdf',
          assetUrl: 'asset://localhost/path/to/doc.pdf',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'rectangle',
          annotationColor: 'var(--color-accent)',
          onAnnotationsChange,
        },
      })

      expect(screen.queryByTestId('annotation-toolbar')).not.toBeInTheDocument()
      expect(screen.queryByTestId('annotation-overlay')).not.toBeInTheDocument()
      expect(onAnnotationsChange).not.toHaveBeenCalled()
    })

    it('shows PDF loading state when transitioning from image to pdf', async () => {
      const view = render(DocumentViewer, {
        props: {
          path: '/path/to/image.jpg',
          type: 'image',
          assetUrl: 'asset://localhost/path/to/image.jpg',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })

      expect(screen.queryByTestId('pdf-loading')).not.toBeInTheDocument()

      await view.rerender({
        path: '/path/to/doc.pdf',
        type: 'pdf',
        assetUrl: 'asset://localhost/path/to/doc.pdf',
        annotations: [],
        selectedAnnotationId: null,
        annotationTool: 'select',
        annotationColor: 'var(--color-accent)',
      })

      expect(screen.getByTestId('pdf-loading')).toBeInTheDocument()
    })

    it('hides PDF-only UI when transitioning from pdf to image', async () => {
      const view = render(DocumentViewer, {
        props: {
          path: '/path/to/doc.pdf',
          type: 'pdf',
          assetUrl: 'asset://localhost/path/to/doc.pdf',
          annotations: [],
          selectedAnnotationId: null,
          annotationTool: 'select',
          annotationColor: 'var(--color-accent)',
        },
      })

      expect(screen.getByTestId('pdf-controls')).toBeInTheDocument()

      await view.rerender({
        path: '/path/to/image.jpg',
        type: 'image',
        assetUrl: 'asset://localhost/path/to/image.jpg',
        annotations: [],
        selectedAnnotationId: null,
        annotationTool: 'select',
        annotationColor: 'var(--color-accent)',
      })

      expect(screen.getByRole('img')).toHaveAttribute('src', 'asset://localhost/path/to/image.jpg')
      expect(screen.queryByTestId('pdf-controls')).not.toBeInTheDocument()
      expect(screen.queryByTestId('pdf-loading')).not.toBeInTheDocument()
    })
  })
})
