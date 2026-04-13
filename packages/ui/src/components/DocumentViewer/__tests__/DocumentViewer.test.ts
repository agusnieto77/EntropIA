import { render, screen } from '@testing-library/svelte'
import { describe, it, expect, vi } from 'vitest'
import DocumentViewer from '../DocumentViewer.svelte'

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
  describe('image mode', () => {
    it('renders an img element with the asset URL', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/image.jpg',
          type: 'image',
          assetUrl: 'asset://localhost/path/to/image.jpg',
        },
      })
      const img = screen.getByRole('img')
      expect(img).toBeInTheDocument()
      expect(img).toHaveAttribute('src', 'asset://localhost/path/to/image.jpg')
    })

    it('does not render PDF controls in image mode', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/image.jpg',
          type: 'image',
          assetUrl: 'asset://localhost/path/to/image.jpg',
        },
      })
      expect(screen.queryByTestId('pdf-controls')).not.toBeInTheDocument()
    })
  })

  describe('pdf mode', () => {
    it('renders a canvas element for PDF', () => {
      render(DocumentViewer, {
        props: {
          path: '/path/to/doc.pdf',
          type: 'pdf',
          assetUrl: 'asset://localhost/path/to/doc.pdf',
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
        },
      })
      expect(screen.getByTestId('pdf-prev')).toBeDisabled()
    })
  })
})
