// @entropia/store — barrel export

// Client
export { createDbClient, createDrizzleClient } from './client'

// Schema tables
export {
  collections,
  items,
  assets,
  notes,
  jobs,
  extractions,
  entities,
  triples,
  annotations,
  transcriptions,
} from './schema'

// Migration runner
export { runMigrations } from './runner'

// Repositories
export { CollectionRepo } from './repos/collection.repo'
export { ItemRepo } from './repos/item.repo'
export { AssetRepo } from './repos/asset.repo'
export { NoteRepo } from './repos/note.repo'
export { AnnotationRepo } from './repos/annotation.repo'
export { JobRepo } from './repos/job.repo'
export { ExtractionRepo } from './repos/extraction.repo'
export { EntityRepo } from './repos/entity.repo'
export { FtsRepo, sanitizeFts5Query } from './repos/fts.repo'
export { EmbeddingRepo } from './repos/embedding.repo'
export { TripleRepo } from './repos/triple.repo'
export { TranscriptionRepo } from './repos/transcription.repo'
export type { Transcription, TranscriptionSegment } from './repos/transcription.repo'

// Store API
export { initStore } from './repos/store'
export type { StoreApi } from './repos/store'

// Types
export type { DbClient, DrizzleClient } from './types'
export type { Collection, NewCollection } from './repos/collection.repo'
export type { Item, NewItem } from './repos/item.repo'
export type { Asset, NewAsset } from './repos/asset.repo'
export type { Note, NewNote } from './repos/note.repo'
export type {
  Annotation,
  AnnotationKind,
  AnnotationInput,
  NewAnnotation,
} from './repos/annotation.repo'
export type { Job, NewJob } from './repos/job.repo'
export type { Extraction, NewExtraction } from './repos/extraction.repo'
export type { Entity, NewEntity, EntityType } from './repos/entity.repo'
export type { Triple, NewTriple } from './repos/triple.repo'
export type { FtsResult } from './repos/fts.repo'
