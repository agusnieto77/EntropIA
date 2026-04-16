import { createDbClient, createDrizzleClient } from '../client'
import { runMigrations } from '../runner'
import { CollectionRepo } from './collection.repo'
import { ItemRepo } from './item.repo'
import { AssetRepo } from './asset.repo'
import { NoteRepo } from './note.repo'
import { AnnotationRepo } from './annotation.repo'
import { JobRepo } from './job.repo'
import { ExtractionRepo } from './extraction.repo'
import { EntityRepo } from './entity.repo'
import { FtsRepo } from './fts.repo'
import { EmbeddingRepo } from './embedding.repo'
import { TripleRepo } from './triple.repo'
import { TranscriptionRepo } from './transcription.repo'

export interface StoreApi {
  collections: CollectionRepo
  items: ItemRepo
  assets: AssetRepo
  notes: NoteRepo
  annotations: AnnotationRepo
  jobs: JobRepo
  extractions: ExtractionRepo
  entities: EntityRepo
  fts: FtsRepo
  embeddings: EmbeddingRepo
  triples: TripleRepo
  transcriptions: TranscriptionRepo
}

export async function initStore(): Promise<StoreApi> {
  console.log('[store] initStore start')
  const client = createDbClient()
  console.log('[store] client created')
  await runMigrations(client)
  console.log('[store] migrations done')
  const db = createDrizzleClient(client)
  console.log('[store] drizzle client created')
  const embeddings = new EmbeddingRepo(client)
  await embeddings.initialize()
  console.log('[store] embeddings initialized, returning store')
  return {
    collections: new CollectionRepo(db, client),
    items: new ItemRepo(db, client),
    assets: new AssetRepo(db, client),
    notes: new NoteRepo(db),
    annotations: new AnnotationRepo(db),
    jobs: new JobRepo(db),
    extractions: new ExtractionRepo(db),
    entities: new EntityRepo(db),
    fts: new FtsRepo(client),
    embeddings,
    triples: new TripleRepo(db),
    transcriptions: new TranscriptionRepo(db),
  }
}
