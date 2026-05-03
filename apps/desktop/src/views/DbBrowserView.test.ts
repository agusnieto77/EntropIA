import { fireEvent, render, screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { locale } from '$lib/i18n'
import DbBrowserView from './DbBrowserView.svelte'

const {
  listTablesMock,
  describeTableMock,
  queryRowsMock,
  clipboardWriteTextMock,
  jsonCellValue,
  expandedJsonValue,
} = vi.hoisted(() => {
  const jsonCellValue = '{"title":"Acta","meta":{"page":2}}'

  return {
    listTablesMock: vi.fn(),
    describeTableMock: vi.fn(),
    queryRowsMock: vi.fn(),
    clipboardWriteTextMock: vi.fn<(_: string) => Promise<void>>(),
    jsonCellValue,
    expandedJsonValue: JSON.stringify(JSON.parse(jsonCellValue), null, 2),
  }
})

vi.mock('$lib/db-browser', () => ({
  listDbBrowserTables: listTablesMock,
  describeDbBrowserTable: describeTableMock,
  queryDbBrowserRows: queryRowsMock,
}))

vi.mock('@entropia/ui', async () => {
  const MockButton = (await import('./__mocks__/MockButton.svelte')).default
  return { Button: MockButton }
})

function flushPromises() {
  return new Promise((resolve) => setTimeout(resolve, 0))
}

describe('DbBrowserView', () => {
  beforeEach(() => {
    locale.set('es')

    listTablesMock.mockReset().mockResolvedValue([{ name: 'documents' }])
    describeTableMock.mockReset().mockResolvedValue([
      {
        name: 'body',
        dataType: 'TEXT',
        nullable: true,
        isPrimaryKey: false,
      },
    ])
    queryRowsMock.mockReset().mockResolvedValue({
      table: 'documents',
      page: 1,
      pageSize: 25,
      total: 1,
      rows: [{ id: 'row-1', body: jsonCellValue }],
    })

    Object.defineProperty(globalThis.navigator, 'clipboard', {
      configurable: true,
      value: { writeText: clipboardWriteTextMock },
    })
    clipboardWriteTextMock.mockReset().mockResolvedValue(undefined)
  })

  async function renderDbBrowserView() {
    render(DbBrowserView)

    await flushPromises()
    await flushPromises()

    await waitFor(() => {
      expect(listTablesMock).toHaveBeenCalledTimes(1)
      expect(describeTableMock).toHaveBeenCalledWith('documents')
      expect(queryRowsMock).toHaveBeenCalledTimes(1)
    })
  }

  it('shows a visual tooltip on focus while preserving the native title', async () => {
    await renderDbBrowserView()

    const copyButton = screen.getByRole('button', { name: 'Copiar valor de body' })

    expect(copyButton).toHaveAttribute('title', 'Copiar valor de body')
    await fireEvent.focus(copyButton)

    const tooltipText = copyButton.parentElement?.getAttribute('data-tooltip')
    expect(tooltipText).toBe('Copiar valor de body')

    await fireEvent.blur(copyButton)

    expect(copyButton.parentElement).toHaveAttribute('data-tooltip', 'Copiar valor de body')
  })

  it('shows the compact action tooltip on hover for the expand action', async () => {
    await renderDbBrowserView()

    const expandButton = screen.getByRole('button', { name: 'Expandir valor de body' })

    await fireEvent.mouseEnter(expandButton)

    expect(expandButton.parentElement).toHaveAttribute('data-tooltip', 'Expandir valor de body')

    await fireEvent.mouseLeave(expandButton)

    expect(expandButton.parentElement).toHaveAttribute('data-tooltip', 'Expandir valor de body')
  })

  it('copies the expanded modal content with Ctrl+C and reuses copy feedback', async () => {
    await renderDbBrowserView()

    await fireEvent.click(screen.getByRole('button', { name: 'Expandir valor de body' }))

    const dialog = await screen.findByRole('dialog', { name: 'Valor completo de body' })

    await fireEvent.keyDown(dialog, { key: 'c', ctrlKey: true })

    expect(clipboardWriteTextMock).toHaveBeenCalledWith(expandedJsonValue)
    expect(await screen.findByRole('status')).toHaveTextContent('Valor copiado.')
  })
})
