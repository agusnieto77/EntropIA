-- Migration: 0024_entry_results
-- NLP analysis results for corpus entries (sentiment, NER, etc.)

CREATE TABLE IF NOT EXISTS entry_results (
  id          TEXT    PRIMARY KEY,
  entry_id    TEXT    NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  job_type    TEXT    NOT NULL,
  result      TEXT    NOT NULL,
  created_at  INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS entry_results_entry_id_idx ON entry_results(entry_id);
CREATE INDEX IF NOT EXISTS entry_results_entry_job_idx ON entry_results(entry_id, job_type);
