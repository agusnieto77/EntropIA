-- Migration: 0022_corpus_entries
-- Adds item type ('document' | 'corpus') and entries table for corpus items

ALTER TABLE items ADD COLUMN type TEXT DEFAULT 'document';

CREATE TABLE IF NOT EXISTS entries (
  id          TEXT    PRIMARY KEY,
  item_id     TEXT    NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  content     TEXT    NOT NULL,
  attributes  TEXT,
  sort_index  INTEGER NOT NULL DEFAULT 0,
  created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS entries_item_id_idx ON entries(item_id);
