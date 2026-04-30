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

use crate::python_discovery::apply_windows_no_window;
use super::text_provider;

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

/// Compute embedding for item's text (extractions + transcriptions) and store it.
///
/// Computes and persists embeddings for an item.
///
/// Returns `Err(...)` with a precise reason when embedding cannot be generated
/// or persisted, so the UI can surface actionable errors.
pub fn compute_and_store(
    engine: Option<&EmbeddingEngine>,
    conn: &Connection,
    item_id: &str,
) -> Result<(), String> {
    // Fetch concatenated text from both extractions and transcriptions
    let text = text_provider::get_item_text(conn, item_id)?;
    if text.trim().is_empty() {
        return Err(format!(
            "No source text available for item '{item_id}' (run OCR/transcription first)"
        ));
    }

    // Need an engine to compute embeddings
    let engine = match engine {
        Some(e) => e,
        None => {
            return Err(embedding_degradation_log(
                item_id,
                "No embedding engine configured (Python with fastembed not found)",
            ));
        }
    };

    // Attempt to compute embedding via Python subprocess
    let vector = match engine.embed_text(&text) {
        Ok(v) => v,
        Err(e) => {
            return Err(embedding_degradation_log(item_id, &e));
        }
    };

    // Attempt to upsert into vec_items (requires table to exist)
    let blob = floats_to_blob(&vector);
    upsert_vec_item(conn, item_id, &blob)
}

/// Compute embedding for a single asset's text and store it.
///
/// Uses only the extraction/transcription text for the given `asset_id`,
/// not the entire item. The embedding is stored under `asset_id` in
/// `vec_assets`, preserving the item-level vector in `vec_items`.
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

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Serialize `Vec<f32>` to little-endian bytes for sqlite-vec BLOB storage.
fn floats_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn embedding_degradation_log(item_id: &str, reason: &str) -> String {
    format!("[nlp/embeddings] Skipping embedding for {item_id}: {reason}")
}

fn upsert_vec_item(conn: &Connection, item_id: &str, blob: &[u8]) -> Result<(), String> {
    let result = conn.execute(
        "INSERT OR REPLACE INTO vec_items(item_id, embedding) VALUES (?1, ?2)",
        params![item_id, blob],
    );

    match result {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "[nlp/embeddings] Failed to persist embedding for {item_id}: {e}"
        )),
    }
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
    fn upsert_vec_item_degrades_gracefully_when_vec_items_table_missing() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        let result = upsert_vec_item(&conn, "item-1", &[1, 2, 3, 4]);
        assert!(
            result.is_err(),
            "missing vec_items table should return a degradation error"
        );

        let error = result
            .err()
            .expect("missing vec_items table should yield error details");
        assert!(
            error.contains("Failed to persist embedding"),
            "error should preserve persistence failure context"
        );
    }

    #[test]
    fn upsert_vec_item_writes_when_vec_items_table_exists() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute(
            "CREATE TABLE vec_items(item_id TEXT PRIMARY KEY, embedding BLOB NOT NULL)",
            [],
        )
        .expect("vec_items table should be created");

        let result = upsert_vec_item(&conn, "item-1", &[1, 2, 3, 4]);
        assert!(result.is_ok(), "upsert should pass when table exists");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vec_items WHERE item_id = 'item-1'",
                [],
                |row| row.get(0),
            )
            .expect("row count query should succeed");
        assert_eq!(count, 1);
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
    fn compute_and_store_degrades_gracefully_when_no_engine() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(
            "
            CREATE TABLE items (
              id TEXT PRIMARY KEY,
              collection_id TEXT,
              title TEXT NOT NULL,
              metadata TEXT
            );
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
            ",
        )
        .expect("schema should be created");

        conn.execute(
            "INSERT INTO items(id, collection_id, title, metadata) VALUES (?1, ?2, ?3, ?4)",
            params!["item-1", "col-1", "Title", "{}"],
        )
        .expect("item should be inserted");

        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["asset-1", "item-1", "asset.txt", "txt", 1_i64],
        )
        .expect("asset should be inserted");
        conn.execute(
            "INSERT INTO extractions(id, asset_id, text_content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["ext-1", "asset-1", "texto para embedding", 2_i64],
        )
        .expect("extraction should be inserted");

        let result = compute_and_store(None, &conn, "item-1");
        assert!(
            result.is_err(),
            "no-engine embeddings path should return degradation error"
        );

        let error = result
            .err()
            .expect("no-engine path should include degradation reason");
        assert!(
            error.contains("Skipping embedding for item-1"),
            "degradation error should include item id"
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
    fn rolling_hash64_is_stable_for_same_input() {
        let a = rolling_hash64(b"hola");
        let b = rolling_hash64(b"hola");
        let c = rolling_hash64(b"adios");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
