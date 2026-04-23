import type { DbClient } from './types'

// ---------------------------------------------------------------------------
// Migration registry — SQL inlined as strings for Tauri bundling.
// Cannot use dynamic fs reads at runtime in a Tauri webview.
// ---------------------------------------------------------------------------
const MIGRATIONS: Record<string, string> = {
  '0001_initial': `
-- Migration tracking table
CREATE TABLE IF NOT EXISTS _migrations (
  id    INTEGER PRIMARY KEY AUTOINCREMENT,
  name  TEXT    NOT NULL UNIQUE,
  applied_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS collections (
  id          TEXT    PRIMARY KEY,
  name        TEXT    NOT NULL,
  description TEXT,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS items (
  id            TEXT    PRIMARY KEY,
  title         TEXT    NOT NULL,
  collection_id TEXT    NOT NULL REFERENCES collections(id),
  metadata      TEXT,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS assets (
  id         TEXT    PRIMARY KEY,
  item_id    TEXT    NOT NULL REFERENCES items(id),
  path       TEXT    NOT NULL,
  type       TEXT    NOT NULL,
  size       INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
  id         TEXT    PRIMARY KEY,
  item_id    TEXT    NOT NULL REFERENCES items(id),
  content    TEXT    NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
  id         TEXT    PRIMARY KEY,
  type       TEXT    NOT NULL,
  status     TEXT    NOT NULL DEFAULT 'pending',
  asset_id   TEXT    NOT NULL REFERENCES assets(id),
  result     TEXT,
  error      TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
  `.trim(),

  '0002_metadata_search': `
-- Add search_text generated column for LIKE queries
ALTER TABLE items ADD COLUMN search_text TEXT GENERATED ALWAYS AS (
  COALESCE(title, '') || ' ' || COALESCE(json(metadata), '')
) STORED;

-- Performance indexes
CREATE INDEX IF NOT EXISTS idx_items_search ON items(search_text);
CREATE INDEX IF NOT EXISTS idx_items_collection ON items(collection_id);
CREATE INDEX IF NOT EXISTS idx_assets_item ON assets(item_id);
CREATE INDEX IF NOT EXISTS idx_notes_item ON notes(item_id);
  `.trim(),

  '0003_extractions': `
CREATE TABLE IF NOT EXISTS extractions (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  text_content TEXT NOT NULL,
  method TEXT NOT NULL CHECK(method IN ('native', 'ocr')),
  confidence REAL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_extractions_asset_id ON extractions(asset_id);

CREATE INDEX IF NOT EXISTS idx_jobs_asset_id ON jobs(asset_id);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
  `.trim(),

  '0004_fts5': `
CREATE VIRTUAL TABLE IF NOT EXISTS fts_items
USING fts5(
  item_id UNINDEXED,
  title,
  metadata,
  extracted_text,
  tokenize='unicode61 remove_diacritics 1',
  content=''
);

INSERT INTO fts_items(item_id, title, metadata, extracted_text)
SELECT i.id, i.title, COALESCE(i.metadata,''),
       COALESCE((SELECT GROUP_CONCAT(e.text_content,' ') FROM extractions e
                 JOIN assets a ON e.asset_id=a.id WHERE a.item_id=i.id), '')
FROM items i
  `.trim(),

  '0005_nlp_tables': `
CREATE TABLE IF NOT EXISTS entities (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  entity_type TEXT NOT NULL CHECK(entity_type IN ('person','place','date','institution','organization','misc','custom')),
  value TEXT NOT NULL,
  start_offset INTEGER NOT NULL DEFAULT 0,
  end_offset INTEGER NOT NULL DEFAULT 0,
  confidence REAL NOT NULL DEFAULT 1.0,
  created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_entities_item_id ON entities(item_id);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type)
  `.trim(),

  '0006_triples': `
CREATE TABLE IF NOT EXISTS triples (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  subject TEXT NOT NULL,
  predicate TEXT NOT NULL,
  object TEXT NOT NULL,
  created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS triples_item_id_idx ON triples(item_id)
  `.trim(),

  '0007_annotations': `
CREATE TABLE IF NOT EXISTS annotations (
  id TEXT PRIMARY KEY NOT NULL,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  page INTEGER NOT NULL DEFAULT 1,
  kind TEXT NOT NULL CHECK(kind IN ('rectangle', 'underline')),
  color TEXT NOT NULL,
  x REAL NOT NULL,
  y REAL NOT NULL,
  width REAL NOT NULL,
  height REAL NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS annotations_asset_id_idx ON annotations(asset_id);
CREATE INDEX IF NOT EXISTS annotations_asset_page_idx ON annotations(asset_id, page)
  `.trim(),

  '0008_transcriptions': `
CREATE TABLE IF NOT EXISTS transcriptions (
  id TEXT PRIMARY KEY,
  asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  text_content TEXT NOT NULL,
  language TEXT,
  duration_ms INTEGER,
  model TEXT NOT NULL,
  segments TEXT,
  confidence REAL,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_transcriptions_asset_id ON transcriptions(asset_id)
  `.trim(),

  '0009_entities_provenance': `
CREATE TEMP TABLE IF NOT EXISTS __entropia_migration_0009_noop (id INTEGER);
DROP TABLE IF EXISTS __entropia_migration_0009_noop
  `.trim(),

  '0010_entities_type_expansion': `
CREATE TABLE entities_v2 (
  id TEXT PRIMARY KEY NOT NULL,
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  entity_type TEXT NOT NULL CHECK(entity_type IN ('person','place','date','institution','organization','misc','custom')),
  value TEXT NOT NULL,
  start_offset INTEGER NOT NULL DEFAULT 0,
  end_offset INTEGER NOT NULL DEFAULT 0,
  confidence REAL NOT NULL DEFAULT 1.0,
  source TEXT,
  model_name TEXT,
  created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

INSERT INTO entities_v2 (
  id, item_id, entity_type, value, start_offset, end_offset,
  confidence, source, model_name, created_at
)
SELECT
  id, item_id, entity_type, value, start_offset, end_offset,
  confidence, source, model_name, created_at
FROM entities;

DROP TABLE entities;
ALTER TABLE entities_v2 RENAME TO entities;

CREATE INDEX IF NOT EXISTS idx_entities_item_id ON entities(item_id);
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type)
  `.trim(),

  '0011_entities_geocoding': `
ALTER TABLE entities ADD COLUMN latitude REAL;
ALTER TABLE entities ADD COLUMN longitude REAL;
ALTER TABLE entities ADD COLUMN geo_status TEXT NOT NULL DEFAULT 'pending';
CREATE INDEX IF NOT EXISTS idx_entities_geo_status ON entities(geo_status)
  `.trim(),
}

/**
 * Split a multi-statement SQL string into individual statements.
 * Strips comments and empty lines, splits on semicolons.
 */
function splitStatements(sql: string): string[] {
  return sql
    .split(';')
    .map((s) => s.trim())
    .filter((s) => s.length > 0)
}

/**
 * Runs all pending migrations in filename order.
 *
 * - Ensures the `_migrations` tracking table exists
 * - Fetches already-applied migration names
 * - Applies each pending migration inside a BEGIN/COMMIT transaction
 * - Inserts a record into `_migrations` on success
 * - On error: rolls back the transaction and rethrows with the migration name
 */
export async function runMigrations(client: DbClient): Promise<void> {
  console.log('[runner] runMigrations start')
  // Ensure tracking table exists (idempotent)
  await client.execute(`
    CREATE TABLE IF NOT EXISTS _migrations (
      id    INTEGER PRIMARY KEY AUTOINCREMENT,
      name  TEXT    NOT NULL UNIQUE,
      applied_at INTEGER NOT NULL
    )
  `)

  // Fetch already-applied migrations
  const applied = await client.select<{ name: string }>('SELECT name FROM _migrations ORDER BY id')
  const appliedSet = new Set(applied.map((row) => row.name))

  // Sort migration keys by filename order (lexicographic)
  const pending = Object.keys(MIGRATIONS)
    .sort()
    .filter((name) => !appliedSet.has(name))

  for (const name of pending) {
    const sql = MIGRATIONS[name]!
    const statements = splitStatements(sql)

    try {
      await client.execute('BEGIN')

      for (const stmt of statements) {
        await client.execute(stmt)
      }

      await client.execute('INSERT INTO _migrations (name, applied_at) VALUES (?, ?)', [
        name,
        Math.floor(Date.now() / 1000),
      ])

      await client.execute('COMMIT')
    } catch (error) {
      // Best-effort rollback — if BEGIN didn't succeed, ROLLBACK may also fail
      try {
        await client.execute('ROLLBACK')
      } catch {
        // Swallow rollback errors — the original error is more important
      }

      throw new Error(
        `Migration "${name}" failed: ${error instanceof Error ? error.message : String(error)}`
      )
    }
  }
}
