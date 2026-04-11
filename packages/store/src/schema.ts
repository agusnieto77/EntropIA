import { sqliteTable, text, integer, real } from 'drizzle-orm/sqlite-core'

// ---------------------------------------------------------------------------
// Collections — top-level grouping of items
// ---------------------------------------------------------------------------
export const collections = sqliteTable('collections', {
  id: text('id').primaryKey(),
  name: text('name').notNull(),
  description: text('description'),
  createdAt: integer('created_at').notNull(),
  updatedAt: integer('updated_at').notNull(),
})

// ---------------------------------------------------------------------------
// Items — documents / artifacts within a collection
// ---------------------------------------------------------------------------
export const items = sqliteTable('items', {
  id: text('id').primaryKey(),
  title: text('title').notNull(),
  collectionId: text('collection_id')
    .notNull()
    .references(() => collections.id),
  metadata: text('metadata'), // JSON blob
  createdAt: integer('created_at').notNull(),
  updatedAt: integer('updated_at').notNull(),
})

// ---------------------------------------------------------------------------
// Assets — files (images, PDFs) attached to an item
// ---------------------------------------------------------------------------
export const assets = sqliteTable('assets', {
  id: text('id').primaryKey(),
  itemId: text('item_id')
    .notNull()
    .references(() => items.id),
  path: text('path').notNull(),
  type: text('type').notNull(), // 'image' | 'pdf'
  size: integer('size'),
  createdAt: integer('created_at').notNull(),
})

// ---------------------------------------------------------------------------
// Notes — textual annotations on an item
// ---------------------------------------------------------------------------
export const notes = sqliteTable('notes', {
  id: text('id').primaryKey(),
  itemId: text('item_id')
    .notNull()
    .references(() => items.id),
  content: text('content').notNull(),
  createdAt: integer('created_at').notNull(),
  updatedAt: integer('updated_at').notNull(),
})

// ---------------------------------------------------------------------------
// Jobs — async processing tasks (OCR, NER, embeddings, triples)
// ---------------------------------------------------------------------------
export const jobs = sqliteTable('jobs', {
  id: text('id').primaryKey(),
  type: text('type').notNull(), // 'ocr' | 'embeddings' | 'ner'
  status: text('status').notNull().default('pending'), // 'pending' | 'running' | 'done' | 'error'
  assetId: text('asset_id')
    .notNull()
    .references(() => assets.id),
  result: text('result'), // JSON blob
  error: text('error'),
  createdAt: integer('created_at').notNull(),
  updatedAt: integer('updated_at').notNull(),
})

// ---------------------------------------------------------------------------
// Extractions — OCR / native text extraction results for an asset
// ---------------------------------------------------------------------------
export const extractions = sqliteTable('extractions', {
  id: text('id').primaryKey(),
  assetId: text('asset_id')
    .notNull()
    .references(() => assets.id),
  textContent: text('text_content').notNull(),
  method: text('method').notNull(), // 'native' | 'ocr'
  confidence: real('confidence'),
  createdAt: integer('created_at').notNull(),
})

// ---------------------------------------------------------------------------
// Entities — NER results linked to an item
// ---------------------------------------------------------------------------
export const entities = sqliteTable('entities', {
  id: text('id').primaryKey().notNull(),
  itemId: text('item_id')
    .notNull()
    .references(() => items.id),
  entityType: text('entity_type').notNull(), // 'person' | 'place' | 'date' | 'institution' | 'custom'
  value: text('value').notNull(),
  startOffset: integer('start_offset').notNull().default(0),
  endOffset: integer('end_offset').notNull().default(0),
  confidence: real('confidence').notNull().default(1.0),
  createdAt: integer('created_at').notNull(),
})

// ---------------------------------------------------------------------------
// Triples — semantic triples (S|P|O) linked to an item
// ---------------------------------------------------------------------------
export const triples = sqliteTable('triples', {
  id: text('id').primaryKey().notNull(),
  itemId: text('item_id')
    .notNull()
    .references(() => items.id),
  subject: text('subject').notNull(),
  predicate: text('predicate').notNull(),
  object: text('object').notNull(),
  createdAt: integer('created_at').notNull(),
})
