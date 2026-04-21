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
CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);

-- vec_items virtual table will be created at runtime by Rust when sqlite-vec extension is loaded.
-- This comment marks the intent; the actual CREATE is done in the NLP worker startup
-- (apps/desktop/src-tauri/src/nlp/mod.rs — NlpQueue::start_worker calls sqlite_vec::load then
--  the embedding module creates the table via CREATE VIRTUAL TABLE IF NOT EXISTS vec_items
--  USING vec0(item_id TEXT PRIMARY KEY, embedding FLOAT[384])).
-- Reason: sqlite-vec must be loaded before vec0 tables can be created. SQLite migrations run
-- before extension loading, so CREATE VIRTUAL TABLE USING vec0 cannot be in a migration file.
