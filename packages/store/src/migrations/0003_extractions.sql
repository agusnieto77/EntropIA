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
