import { open } from '@tauri-apps/plugin-dialog'
import { readFile } from '@tauri-apps/plugin-fs'
import { getStore } from './db'

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface CsvColumn {
  name: string
  sampleValues: string[]
}

export interface CsvParseResult {
  columns: CsvColumn[]
  rows: Record<string, string>[]
  totalRows: number
  filePath: string
  fileName: string
}

export interface CsvColumnMapping {
  /** Column that contains the main text content (required) */
  textColumn: string
  /** Column to use as entry title/author (optional — stored in attributes) */
  titleColumn: string | null
  /** Column with a date/timestamp (optional — stored in attributes) */
  dateColumn: string | null
  /** Column with original ID from the source dataset (optional — stored in attributes) */
  idColumn: string | null
}

export interface CsvImportOptions {
  collectionName: string
  collectionDescription: string | null
  /** Name for the corpus item (e.g. "Tweets de @usuario", "Comentarios Facebook") */
  itemName: string
  mapping: CsvColumnMapping
  /** Name of the source dataset (e.g. "tweets", "fb_comments") — stored in metadata */
  sourceLabel: string | null
}

export interface CsvImportProgress {
  current: number
  total: number
  phase: 'creating' | 'done' | 'error'
  error?: string
}

// ---------------------------------------------------------------------------
// CSV Parsing
// ---------------------------------------------------------------------------

/**
 * Open a file picker for CSV files and return the selected path.
 */
export async function pickCsvFile(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    filters: [{ name: 'CSV', extensions: ['csv', 'tsv', 'txt'] }],
  })
  if (!selected) return null
  return Array.isArray(selected) ? selected[0] ?? null : selected
}

/**
 * Parse a CSV file and return column info + all rows.
 * Auto-detects separator (comma, semicolon, tab).
 */
export async function parseCsvFile(filePath: string): Promise<CsvParseResult> {
  const bytes = await readFile(filePath)
  const text = new TextDecoder('utf-8').decode(bytes)

  // Use the first line (up to first newline outside quotes) to detect separator
  const firstLineEnd = text.indexOf('\n')
  const headerLine = (firstLineEnd >= 0 ? text.slice(0, firstLineEnd) : text).replace(/\r$/, '')

  if (!headerLine.trim()) {
    throw new Error('CSV file is empty')
  }

  const separator = detectSeparator(headerLine)

  // Parse entire file respecting multiline quoted fields
  const allRows = parseCsvText(text, separator)

  if (allRows.length < 2) {
    throw new Error('CSV file must have at least a header row and one data row')
  }

  const headers = allRows[0]!
  const rows: Record<string, string>[] = []
  for (let i = 1; i < allRows.length; i++) {
    const values = allRows[i]!
    const row: Record<string, string> = {}
    for (let j = 0; j < headers.length; j++) {
      row[headers[j]!] = values[j] ?? ''
    }
    rows.push(row)
  }

  const columns: CsvColumn[] = headers.map((name) => ({
    name,
    sampleValues: rows.slice(0, 5).map((r) => r[name] ?? ''),
  }))

  const fileName = filePath.split(/[/\\]/).pop() ?? 'unknown.csv'

  return { columns, rows, totalRows: rows.length, filePath, fileName }
}

function detectSeparator(headerLine: string): string {
  const candidates = [
    { sep: '\t', count: (headerLine.match(/\t/g) ?? []).length },
    { sep: ';', count: (headerLine.match(/;/g) ?? []).length },
    { sep: ',', count: (headerLine.match(/,/g) ?? []).length },
  ]
  candidates.sort((a, b) => b.count - a.count)
  return candidates[0]!.sep
}

/**
 * Parse an entire CSV text handling multiline quoted fields (RFC 4180).
 * Returns an array of rows, each row being an array of field strings.
 */
function parseCsvText(text: string, separator: string): string[][] {
  const rows: string[][] = []
  let field = ''
  let inQuotes = false
  let currentRow: string[] = []

  for (let i = 0; i < text.length; i++) {
    const ch = text[i]!

    if (inQuotes) {
      if (ch === '"') {
        if (i + 1 < text.length && text[i + 1] === '"') {
          field += '"'
          i++ // skip escaped quote
        } else {
          inQuotes = false
        }
      } else {
        field += ch
      }
    } else {
      if (ch === '"') {
        inQuotes = true
      } else if (ch === separator) {
        currentRow.push(field.trim())
        field = ''
      } else if (ch === '\r') {
        // skip \r, handle \n next
      } else if (ch === '\n') {
        currentRow.push(field.trim())
        if (currentRow.some((f) => f.length > 0)) {
          rows.push(currentRow)
        }
        currentRow = []
        field = ''
      } else {
        field += ch
      }
    }
  }

  // Last row (no trailing newline)
  if (field.length > 0 || currentRow.length > 0) {
    currentRow.push(field.trim())
    if (currentRow.some((f) => f.length > 0)) {
      rows.push(currentRow)
    }
  }

  return rows
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/**
 * Import a parsed CSV into EntropIA as a single corpus item with N entries.
 * Calls `onProgress` after each batch so the UI can show a progress bar.
 */
export async function importCsv(
  parsed: CsvParseResult,
  options: CsvImportOptions,
  onProgress: (progress: CsvImportProgress) => void
): Promise<{ collectionId: string; itemId: string; entryCount: number; skippedDuplicates: number }> {
  const store = getStore()

  const { mapping } = options

  // 1. Create or reuse collection
  const collection = await store.collections.create({
    name: options.collectionName,
    description: options.collectionDescription ?? null,
  })

  // 2. Create corpus item
  const metadata: Record<string, string> = {
    __importedFrom: parsed.fileName,
    __columnMapping: JSON.stringify(mapping),
  }
  if (options.sourceLabel) {
    metadata.__source = options.sourceLabel
  }

  const item = await store.items.create({
    title: options.itemName,
    type: 'corpus',
    collectionId: collection.id,
    metadata: JSON.stringify(metadata),
  })

  // 3. Create entries in batches
  let imported = 0
  let skipped = 0
  const seenTexts = new Set<string>()
  const BATCH_SIZE = 100

  onProgress({ current: 0, total: parsed.totalRows, phase: 'creating' })

  let batch: { itemId: string; content: string; attributes: string | null; sortIndex: number }[] = []

  for (let i = 0; i < parsed.rows.length; i++) {
    const row = parsed.rows[i]!
    const text = row[mapping.textColumn] ?? ''
    if (!text.trim()) {
      skipped++
      continue
    }

    // Dedup
    const textKey = text.trim()
    if (seenTexts.has(textKey)) {
      skipped++
      continue
    }
    seenTexts.add(textKey)

    // Build attributes — all columns except the text column
    const attrs: Record<string, string> = {}
    for (const [key, value] of Object.entries(row)) {
      if (key !== mapping.textColumn && value) {
        attrs[key] = value
      }
    }

    batch.push({
      itemId: item.id,
      content: text,
      attributes: Object.keys(attrs).length > 0 ? JSON.stringify(attrs) : null,
      sortIndex: imported,
    })

    imported++

    if (batch.length >= BATCH_SIZE) {
      try {
        await store.entries.createBatch(batch)
      } catch (e) {
        onProgress({
          current: imported,
          total: parsed.totalRows,
          phase: 'error',
          error: `Batch at row ${i + 1}: ${e instanceof Error ? e.message : String(e)}`,
        })
        throw e
      }
      batch = []
      onProgress({ current: imported, total: parsed.totalRows, phase: 'creating' })
    }
  }

  // Flush remaining batch
  if (batch.length > 0) {
    await store.entries.createBatch(batch)
  }

  onProgress({ current: imported, total: parsed.totalRows, phase: 'done' })
  return { collectionId: collection.id, itemId: item.id, entryCount: imported, skippedDuplicates: skipped }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Suggest a column mapping based on column names.
 * Returns a best-guess mapping that the user can adjust.
 */
export function suggestMapping(columns: CsvColumn[]): CsvColumnMapping {
  const names = columns.map((c) => c.name.toLowerCase())

  const textCandidates = ['texto', 'text', 'content', 'post', 'body', 'message', 'comentario', 'comment', 'texto_limpio']
  const titleCandidates = ['title', 'titulo', 'nombre', 'name', 'handle', 'nombre_usuario', 'autor', 'author', 'user', 'usuario']
  const dateCandidates = ['fecha', 'date', 'created_at', 'timestamp', 'datetime', 'utime']
  const idCandidates = ['id', 'tid', 'tweet_id', 'comment_id', 'dom_pos']

  const findFirst = (candidates: string[]): string | null => {
    for (const c of candidates) {
      const idx = names.indexOf(c)
      if (idx >= 0) return columns[idx]!.name
    }
    return null
  }

  const textColumn = findFirst(textCandidates) ?? columns[0]!.name

  return {
    textColumn,
    titleColumn: findFirst(titleCandidates),
    dateColumn: findFirst(dateCandidates),
    idColumn: findFirst(idCandidates),
  }
}
