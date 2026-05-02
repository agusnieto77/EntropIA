import { describe, expect, it } from 'vitest'

import {
  convertLegacyNoteTextToHtml,
  isLegacyPlainTextNoteContent,
  normalizeNoteContentForEditor,
  normalizeNoteContentForRender,
  sanitizeNoteHtml,
} from '../note-content'

describe('note-content helpers', () => {
  it('detects legacy plain text and ignores html note content', () => {
    expect(isLegacyPlainTextNoteContent('Primera linea\n\nSegunda linea')).toBe(true)
    expect(isLegacyPlainTextNoteContent('<p><strong>Nota</strong></p>')).toBe(false)
  })

  it('converts legacy text to safe paragraph html', () => {
    expect(convertLegacyNoteTextToHtml('Hola <mundo>\nlinea 2\n\nBloque 2')).toBe(
      '<p>Hola &lt;mundo&gt;<br>linea 2</p><p>Bloque 2</p>'
    )
  })

  it('sanitizes unsafe html while preserving supported formatting', () => {
    expect(
      sanitizeNoteHtml(
        '<h2>Titulo</h2><p><a href="javascript:alert(1)">click</a> <strong>ok</strong></p><script>alert(1)</script>'
      )
    ).toBe('<h2>Titulo</h2><p>click <strong>ok</strong></p>')
  })

  it('normalizes both legacy and rich text content for rendering and editing', () => {
    expect(normalizeNoteContentForRender('Linea 1\nLinea 2')).toBe('<p>Linea 1<br>Linea 2</p>')
    expect(normalizeNoteContentForEditor('<p>Hola <em>mundo</em></p><iframe></iframe>')).toBe(
      '<p>Hola <em>mundo</em></p>'
    )
  })
})
