/// Embedding computation via fastembed Python subprocess.
///
/// Spawns `embed.py` as a child process to compute text embeddings using
/// `sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2` (384 dims,
/// 50+ languages including Spanish).
///
/// This replaces the Rust fastembed crate which fails on Windows due to
/// ORT/MSVC linker issues (LNK2001/LNK2019 with `__std_*` symbols).
/// The subprocess approach provides complete crash isolation — if Python
/// crashes, we catch it as `Result::Err` instead of a hard abort.
///
/// Architecture mirrors the transcription engine (transcription/engine.rs):
/// - Each embedding call spawns a fresh Python process
/// - Model is loaded per-call (cached by fastembed after first download)
/// - Output wrapped in sentinel markers for reliable JSON extraction
use rusqlite::{params, Connection};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use super::text_provider;
use crate::python_discovery::apply_windows_no_window;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEmbeddingCandidate {
    pub asset_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEmbeddingCoverageSummary {
    pub total_assets: i64,
    pub assets_with_text: i64,
    pub assets_with_embedding: i64,
    pub assets_missing_embedding: i64,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Embedding engine configuration — resolved once at NLP worker startup.
#[derive(Clone)]
pub struct EmbeddingConfig {
    /// Path to the Python interpreter with `fastembed` installed.
    pub python_path: PathBuf,
    /// Path to the `embed.py` script.
    pub script_path: PathBuf,
    /// Model name for fastembed (default: multilingual).
    pub model_name: String,
    /// Directory to cache HuggingFace models (avoids broken symlinks on Windows).
    pub cache_dir: Option<PathBuf>,
}

/// Embedding engine — spawns Python as a child process.
pub struct EmbeddingEngine {
    config: EmbeddingConfig,
    cache: Mutex<HashMap<u64, Vec<f32>>>,
}

/// JSON output from the Python `embed.py` script.
#[derive(Debug, Deserialize)]
struct EmbedOutput {
    vector: Vec<f32>,
}

impl EmbeddingEngine {
    /// Initialize the engine by verifying the script path.
    ///
    /// NOTE: Python interpreter was already validated by `which_python_for_module()`
    /// which ran `import fastembed; print('ok')` successfully. No redundant
    /// verification needed — the discovery module already proved it works.
    pub fn init(config: EmbeddingConfig) -> Result<Self, String> {
        // Verify the script exists
        if !config.script_path.exists() {
            return Err(format!(
                "Embedding script not found: {}",
                config.script_path.display()
            ));
        }

        eprintln!(
            "[nlp/embeddings] Engine configured: python={}, script={}, model={}",
            config.python_path.display(),
            config.script_path.display(),
            config.model_name,
        );

        Ok(Self {
            config,
            cache: Mutex::new(HashMap::new()),
        })
    }

    /// Compute embedding for a single text string via Python subprocess.
    ///
    /// Returns a 384-dimensional float vector. Errors are non-fatal —
    /// callers should treat them as degradation.
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
        let key = rolling_hash64(text.as_bytes());
        if let Ok(cache) = self.cache.lock() {
            if let Some(hit) = cache.get(&key) {
                return Ok(hit.clone());
            }
        }

        let mut cmd = Command::new(&self.config.python_path);
        apply_windows_no_window(&mut cmd);
        cmd.arg(&self.config.script_path)
            .arg("--text")
            .arg(text)
            .arg("--model")
            .arg(&self.config.model_name);

        if let Some(ref cache_dir) = self.config.cache_dir {
            cmd.arg("--cache-dir").arg(cache_dir);
        }

        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd.output().map_err(|e| {
            format!(
                "Failed to spawn Python process (python={}): {e}",
                self.config.python_path.display()
            )
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !output.status.success() {
            let exit_code = output.status.code().unwrap_or(-1);
            return Err(format!(
                "Embedding script failed (exit code {exit_code}).\n\
                 Python: {}\n\
                 Script: {}\n\
                 Stderr: {}",
                self.config.python_path.display(),
                self.config.script_path.display(),
                stderr.trim(),
            ));
        }

        // Extract JSON between sentinel markers
        let json_str = extract_sentinel_json(&stdout);

        let embed_output: EmbedOutput = serde_json::from_str(json_str).map_err(|e| {
            format!(
                "Failed to parse embedding JSON: {e}\n\
                 Extracted ({} chars): {}\n\
                 Stderr: {}",
                json_str.len(),
                if json_str.len() > 200 {
                    &json_str[..200]
                } else {
                    json_str
                },
                stderr.trim(),
            )
        })?;

        if let Ok(mut cache) = self.cache.lock() {
            // Tiny bounded cache to avoid re-spawning Python for repeated text.
            if cache.len() >= 128 {
                if let Some(first_key) = cache.keys().next().copied() {
                    cache.remove(&first_key);
                }
            }
            cache.insert(key, embed_output.vector.clone());
        }

        Ok(embed_output.vector)
    }
}

/// Compute embedding for a single asset's text and store it.
///
/// Uses only the extraction/transcription text for the given `asset_id`,
/// not the entire item. The embedding is stored under `asset_id` in
/// `vec_assets`.
pub fn compute_and_store_for_asset(
    engine: Option<&EmbeddingEngine>,
    conn: &Connection,
    item_id: &str,
    asset_id: &str,
) -> Result<(), String> {
    let text = text_provider::get_asset_text(conn, asset_id)?;
    if text.trim().is_empty() {
        return Err(format!(
            "No source text available for asset '{asset_id}' (run OCR/transcription first)"
        ));
    }

    let engine = match engine {
        Some(e) => e,
        None => {
            return Err(embedding_degradation_log(
                item_id,
                "No embedding engine configured (Python with fastembed not found)",
            ));
        }
    };

    let vector = match engine.embed_text(&text) {
        Ok(v) => v,
        Err(e) => {
            return Err(embedding_degradation_log(item_id, &e));
        }
    };

    let blob = floats_to_blob(&vector);
    upsert_vec_asset(conn, item_id, asset_id, &blob)
}

pub fn summarize_asset_embedding_coverage(
    conn: &Connection,
) -> Result<AssetEmbeddingCoverageSummary, String> {
    conn.query_row(
        r#"
        WITH asset_text AS (
            SELECT
                a.id AS asset_id,
                EXISTS(
                    SELECT 1
                    FROM extractions e
                    WHERE e.asset_id = a.id
                      AND LENGTH(TRIM(COALESCE(e.text_content, ''))) > 0
                )
                OR EXISTS(
                    SELECT 1
                    FROM transcriptions t
                    WHERE t.asset_id = a.id
                      AND LENGTH(TRIM(COALESCE(t.text_content, ''))) > 0
                ) AS has_text,
                EXISTS(
                    SELECT 1
                    FROM vec_assets v
                    WHERE v.asset_id = a.id
                ) AS has_embedding
            FROM assets a
        )
        SELECT
            COUNT(*) AS total_assets,
            SUM(CASE WHEN has_text THEN 1 ELSE 0 END) AS assets_with_text,
            SUM(CASE WHEN has_embedding THEN 1 ELSE 0 END) AS assets_with_embedding,
            SUM(CASE WHEN has_text AND NOT has_embedding THEN 1 ELSE 0 END) AS assets_missing_embedding
        FROM asset_text
        "#,
        [],
        |row| {
            Ok(AssetEmbeddingCoverageSummary {
                total_assets: row.get(0)?,
                assets_with_text: row.get(1)?,
                assets_with_embedding: row.get(2)?,
                assets_missing_embedding: row.get(3)?,
            })
        },
    )
    .map_err(|e| format!("Failed to summarize asset embedding coverage: {e}"))
}

pub fn list_asset_embedding_candidates(
    conn: &Connection,
    force: bool,
    limit: Option<usize>,
) -> Result<Vec<AssetEmbeddingCandidate>, String> {
    let mut sql = String::from(
        r#"
        SELECT a.id, a.item_id
        FROM assets a
        WHERE (
            EXISTS(
                SELECT 1
                FROM extractions e
                WHERE e.asset_id = a.id
                  AND LENGTH(TRIM(COALESCE(e.text_content, ''))) > 0
            )
            OR EXISTS(
                SELECT 1
                FROM transcriptions t
                WHERE t.asset_id = a.id
                  AND LENGTH(TRIM(COALESCE(t.text_content, ''))) > 0
            )
        )
        AND (?1 = 1 OR NOT EXISTS(
            SELECT 1
            FROM vec_assets v
            WHERE v.asset_id = a.id
        ))
        ORDER BY a.created_at ASC, a.id ASC
        "#,
    );

    if let Some(limit) = limit {
        sql.push_str(&format!(" LIMIT {}", limit));
    }

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Failed to prepare asset embedding backfill query: {e}"))?;

    let rows = stmt
        .query_map(params![if force { 1_i64 } else { 0_i64 }], |row| {
            Ok(AssetEmbeddingCandidate {
                asset_id: row.get(0)?,
                item_id: row.get(1)?,
            })
        })
        .map_err(|e| format!("Failed to query asset embedding backfill candidates: {e}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read asset embedding backfill candidates: {e}"))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Serialize `Vec<f32>` to little-endian bytes for sqlite-vec BLOB storage.
fn floats_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn embedding_degradation_log(item_id: &str, reason: &str) -> String {
    format!("[nlp/embeddings] Skipping embedding for {item_id}: {reason}")
}

fn upsert_vec_asset(
    conn: &Connection,
    item_id: &str,
    asset_id: &str,
    blob: &[u8],
) -> Result<(), String> {
    let result = conn.execute(
        "INSERT OR REPLACE INTO vec_assets(asset_id, item_id, embedding) VALUES (?1, ?2, ?3)",
        params![asset_id, item_id, blob],
    );

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "[nlp/embeddings] Failed to persist asset embedding for {asset_id}: {e}"
        )),
    }
}

fn rolling_hash64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Extract JSON content between `===EMBED_JSON_BEGIN===` and
/// `===EMBED_JSON_END===` sentinels. Falls back to the full output
/// if sentinels are not found (backwards compatibility).
fn extract_sentinel_json(output: &str) -> &str {
    const BEGIN: &str = "===EMBED_JSON_BEGIN===";
    const END: &str = "===EMBED_JSON_END===";

    if let Some(start_idx) = output.find(BEGIN) {
        let content_start = start_idx + BEGIN.len();
        if let Some(end_idx) = output[content_start..].find(END) {
            let json_content = &output[content_start..content_start + end_idx];
            return json_content.trim();
        }
    }

    // Fallback: return the full trimmed output (backwards compat)
    output.trim()
}

/// Find the Python interpreter on the system that has `fastembed` available.
///
/// Uses the shared Python candidate cache to avoid redundant filesystem scans
/// and log noise. Probes each candidate for the `fastembed` module.
pub fn which_python(settings_db_path: Option<&std::path::Path>) -> Option<PathBuf> {
    crate::python_discovery::which_python_for_module(
        "nlp/embeddings",
        "fastembed",
        "fastembed",
        "import fastembed; print('ok')",
        settings_db_path,
    )
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_to_blob_produces_correct_byte_count() {
        let v = vec![1.0_f32, 2.0_f32, 3.0_f32];
        let blob = floats_to_blob(&v);
        assert_eq!(blob.len(), 3 * 4, "Each f32 should produce 4 bytes");
    }

    #[test]
    fn floats_to_blob_round_trips_correctly() {
        let original = vec![1.5_f32, -0.5_f32, 100.0_f32];
        let blob = floats_to_blob(&original);
        let recovered: Vec<f32> = blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(recovered, original);
    }

    #[test]
    fn empty_vec_produces_empty_blob() {
        let blob = floats_to_blob(&[]);
        assert!(blob.is_empty());
    }

    #[test]
    fn embedding_degradation_log_includes_item_id_and_reason() {
        let message = embedding_degradation_log("item-42", "No embedding engine configured");
        assert!(
            message.contains("item-42"),
            "log message must include item id for operational diagnosis"
        );
        assert!(
            message.contains("No embedding engine configured"),
            "log message must include degradation reason"
        );
    }

    #[test]
    fn embedding_degradation_log_keeps_expected_prefix_for_grepability() {
        let message = embedding_degradation_log("item-99", "fastembed embedding failed");
        assert!(
            message.starts_with("[nlp/embeddings] Skipping embedding for "),
            "log message prefix should remain stable for observability tooling"
        );
    }

    #[test]
    fn extract_sentinel_json_finds_embedded_json() {
        let output = "some noise\n===EMBED_JSON_BEGIN===\n{\"vector\":[1.0],\"dim\":1,\"model\":\"test\"}\n===EMBED_JSON_END===\nmore noise";
        let json = extract_sentinel_json(output);
        assert!(
            json.contains("\"vector\""),
            "should extract JSON between sentinels"
        );
    }

    #[test]
    fn extract_sentinel_json_falls_back_to_full_output() {
        let output = "{\"vector\":[1.0],\"dim\":1}";
        let json = extract_sentinel_json(output);
        assert_eq!(
            json,
            output.trim(),
            "should return full output when no sentinels"
        );
    }

    #[test]
    fn upsert_vec_asset_writes_when_vec_assets_table_exists() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute(
            "CREATE TABLE vec_assets(asset_id TEXT PRIMARY KEY, item_id TEXT NOT NULL, embedding BLOB NOT NULL)",
            [],
        )
        .expect("vec_assets table should be created");

        upsert_vec_asset(&conn, "item-1", "asset-1", &[9, 8, 7, 6]).expect("upsert should succeed");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vec_assets WHERE asset_id = 'asset-1' AND item_id = 'item-1'",
                [],
                |row| row.get(0),
            )
            .expect("count query should succeed");
        assert_eq!(count, 1);
    }

    #[test]
    fn list_asset_embedding_candidates_returns_only_assets_with_text_and_missing_embeddings() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(
            r#"
            CREATE TABLE assets (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              path TEXT NOT NULL,
              type TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE extractions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE transcriptions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT NOT NULL,
              language TEXT,
              duration_ms INTEGER,
              model TEXT NOT NULL,
              segments TEXT,
              confidence REAL,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE vec_assets (
              asset_id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              embedding BLOB NOT NULL
            );
            "#,
        )
        .expect("schema should be created");

        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["asset-a", "item-1", "a.txt", "txt", 1_i64],
        )
        .expect("asset a should insert");
        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["asset-b", "item-1", "b.txt", "txt", 2_i64],
        )
        .expect("asset b should insert");
        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["asset-c", "item-2", "c.txt", "txt", 3_i64],
        )
        .expect("asset c should insert");

        conn.execute(
            "INSERT INTO extractions(id, asset_id, text_content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["ext-a", "asset-a", "texto OCR", 10_i64],
        )
        .expect("extraction should insert");
        conn.execute(
            "INSERT INTO transcriptions(id, asset_id, text_content, language, duration_ms, model, segments, confidence, created_at) VALUES (?1, ?2, ?3, 'es', 1000, 'base', '[]', 0.9, ?4)",
            params!["tr-b", "asset-b", "audio transcripto", 20_i64],
        )
        .expect("transcription should insert");
        conn.execute(
            "INSERT INTO extractions(id, asset_id, text_content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["ext-c", "asset-c", "   ", 30_i64],
        )
        .expect("blank extraction should insert");
        conn.execute(
            "INSERT INTO vec_assets(asset_id, item_id, embedding) VALUES (?1, ?2, ?3)",
            params!["asset-b", "item-1", vec![1_u8, 2, 3, 4]],
        )
        .expect("existing vec asset should insert");

        let candidates = list_asset_embedding_candidates(&conn, false, None)
            .expect("candidate query should succeed");

        assert_eq!(
            candidates,
            vec![AssetEmbeddingCandidate {
                asset_id: "asset-a".to_string(),
                item_id: "item-1".to_string(),
            }]
        );
    }

    #[test]
    fn list_asset_embedding_candidates_force_mode_includes_existing_embeddings() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(
            r#"
            CREATE TABLE assets (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              path TEXT NOT NULL,
              type TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE extractions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE transcriptions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT NOT NULL,
              language TEXT,
              duration_ms INTEGER,
              model TEXT NOT NULL,
              segments TEXT,
              confidence REAL,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE vec_assets (
              asset_id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              embedding BLOB NOT NULL
            );
            "#,
        )
        .expect("schema should be created");

        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES ('asset-z', 'item-z', 'z.txt', 'txt', 1)",
            [],
        )
        .expect("asset should insert");
        conn.execute(
            "INSERT INTO extractions(id, asset_id, text_content, created_at) VALUES ('ext-z', 'asset-z', 'texto', 2)",
            [],
        )
        .expect("extraction should insert");
        conn.execute(
            "INSERT INTO vec_assets(asset_id, item_id, embedding) VALUES ('asset-z', 'item-z', ?1)",
            params![vec![9_u8, 9, 9, 9]],
        )
        .expect("vec asset should insert");

        let candidates = list_asset_embedding_candidates(&conn, true, Some(10))
            .expect("force query should succeed");

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].asset_id, "asset-z");
    }

    #[test]
    fn summarize_asset_embedding_coverage_counts_text_and_missing_rows() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(
            r#"
            CREATE TABLE assets (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              path TEXT NOT NULL,
              type TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE extractions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE transcriptions (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL,
              text_content TEXT NOT NULL,
              language TEXT,
              duration_ms INTEGER,
              model TEXT NOT NULL,
              segments TEXT,
              confidence REAL,
              created_at INTEGER NOT NULL
            );
            CREATE TABLE vec_assets (
              asset_id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              embedding BLOB NOT NULL
            );
            "#,
        )
        .expect("schema should be created");

        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES ('asset-1', 'item-1', '1.txt', 'txt', 1)",
            [],
        )
        .expect("asset 1 should insert");
        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES ('asset-2', 'item-2', '2.txt', 'audio', 2)",
            [],
        )
        .expect("asset 2 should insert");
        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES ('asset-3', 'item-3', '3.txt', 'txt', 3)",
            [],
        )
        .expect("asset 3 should insert");

        conn.execute(
            "INSERT INTO extractions(id, asset_id, text_content, created_at) VALUES ('ext-1', 'asset-1', 'texto uno', 10)",
            [],
        )
        .expect("extraction should insert");
        conn.execute(
            "INSERT INTO transcriptions(id, asset_id, text_content, language, duration_ms, model, segments, confidence, created_at) VALUES ('tr-2', 'asset-2', 'audio dos', 'es', 1000, 'base', '[]', 0.9, 20)",
            [],
        )
        .expect("transcription should insert");
        conn.execute(
            "INSERT INTO vec_assets(asset_id, item_id, embedding) VALUES ('asset-1', 'item-1', ?1)",
            params![vec![1_u8, 2, 3, 4]],
        )
        .expect("vec asset should insert");

        let summary =
            summarize_asset_embedding_coverage(&conn).expect("coverage summary should succeed");

        assert_eq!(summary.total_assets, 3);
        assert_eq!(summary.assets_with_text, 2);
        assert_eq!(summary.assets_with_embedding, 1);
        assert_eq!(summary.assets_missing_embedding, 1);
    }

    #[test]
    fn rolling_hash64_is_stable_for_same_input() {
        let a = rolling_hash64(b"hola");
        let b = rolling_hash64(b"hola");
        let c = rolling_hash64(b"adios");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
