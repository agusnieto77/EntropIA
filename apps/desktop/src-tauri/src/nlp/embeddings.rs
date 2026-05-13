/// Embedding computation via OpenRouter embeddings API.
///
/// Lightweight EntropIA embeddings are API-first: the default provider is
/// OpenRouter `baai/bge-m3`, which returns 1024-dimensional vectors. This path
/// intentionally does NOT fall back to Python or fastembed;
/// if OpenRouter is not configured, callers receive an explicit degraded state.
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use super::text_provider;

pub const OPENROUTER_EMBEDDING_MODEL_SETTING_KEY: &str = "openrouter_embedding_model";
pub const DEFAULT_OPENROUTER_EMBEDDING_MODEL: &str = "baai/bge-m3";
pub const OPENROUTER_EMBEDDING_DIMENSIONS: usize = 1024;
const OPENROUTER_EMBEDDINGS_URL: &str = "https://openrouter.ai/api/v1/embeddings";

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

/// Embedding engine configuration — resolved from app settings.
#[derive(Clone)]
pub struct EmbeddingConfig {
    /// OpenRouter API key. Never log this value.
    pub api_key: String,
    /// Embedding model name. Defaults to `baai/bge-m3`.
    pub model_name: String,
}

/// Embedding engine — calls OpenRouter's embeddings endpoint.
pub struct EmbeddingEngine {
    config: EmbeddingConfig,
    endpoint_url: String,
    cache: Mutex<HashMap<u64, Vec<f32>>>,
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl EmbeddingEngine {
    /// Initialize the engine without contacting OpenRouter.
    pub fn init(config: EmbeddingConfig) -> Result<Self, String> {
        Self::init_with_endpoint_url(config, OPENROUTER_EMBEDDINGS_URL.to_string())
    }

    fn init_with_endpoint_url(
        config: EmbeddingConfig,
        endpoint_url: String,
    ) -> Result<Self, String> {
        if config.api_key.trim().is_empty() {
            return Err("OpenRouter API key no configurada para embeddings".to_string());
        }
        if config.model_name.trim().is_empty() {
            return Err("OpenRouter embedding model no configurado".to_string());
        }

        eprintln!(
            "[nlp/embeddings] OpenRouter embedding engine configured: model={}, dimensions={}",
            config.model_name, OPENROUTER_EMBEDDING_DIMENSIONS,
        );

        Ok(Self {
            config,
            endpoint_url,
            cache: Mutex::new(HashMap::new()),
        })
    }

    #[cfg(test)]
    fn init_with_endpoint(config: EmbeddingConfig, endpoint_url: String) -> Result<Self, String> {
        Self::init_with_endpoint_url(config, endpoint_url)
    }

    /// Compute embedding for a single text string via OpenRouter.
    ///
    /// Returns a 1024-dimensional float vector. Errors are non-fatal —
    /// callers should treat them as degradation.
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>, String> {
        let key = rolling_hash64(text.as_bytes());
        if let Ok(cache) = self.cache.lock() {
            if let Some(hit) = cache.get(&key) {
                return Ok(hit.clone());
            }
        }

        let request = EmbeddingRequest {
            model: self.config.model_name.as_str(),
            input: text,
        };

        let client = reqwest::blocking::Client::builder()
            .user_agent("EntropIA-Desktop/0.1 (historical-research-app)")
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to build OpenRouter embedding client: {e}"))?;

        let response = client
            .post(&self.endpoint_url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("HTTP-Referer", "https://hlab.com.ar/")
            .header("X-Title", "EntropIA")
            .json(&request)
            .send()
            .map_err(|e| format!("OpenRouter embedding request failed: {e}"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(format!("OpenRouter embedding API error ({status}): {body}"));
        }

        let parsed: EmbeddingResponse = response
            .json()
            .map_err(|e| format!("Failed to parse OpenRouter embedding response: {e}"))?;

        let vector = parsed
            .data
            .into_iter()
            .next()
            .map(|entry| entry.embedding)
            .ok_or_else(|| "OpenRouter embedding response returned no vectors".to_string())?;

        if vector.len() != OPENROUTER_EMBEDDING_DIMENSIONS {
            return Err(format!(
                "OpenRouter embedding model '{}' returned {} dimensions; expected {} for {}",
                self.config.model_name,
                vector.len(),
                OPENROUTER_EMBEDDING_DIMENSIONS,
                DEFAULT_OPENROUTER_EMBEDDING_MODEL,
            ));
        }

        if let Ok(mut cache) = self.cache.lock() {
            // Tiny bounded cache to avoid repeated API calls for identical text.
            if cache.len() >= 128 {
                if let Some(first_key) = cache.keys().next().copied() {
                    cache.remove(&first_key);
                }
            }
            cache.insert(key, vector.clone());
        }

        Ok(vector)
    }
}

pub fn config_from_settings(conn: &Connection) -> Result<EmbeddingConfig, String> {
    let api_key = crate::settings::get_setting(conn, "openrouter_api_key")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "OpenRouter API key no configurada. Configurá OpenRouter para generar embeddings BGE-M3. No hay fallback a Python/fastembed."
                .to_string()
        })?;

    let model_name = crate::settings::get_setting(conn, OPENROUTER_EMBEDDING_MODEL_SETTING_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_OPENROUTER_EMBEDDING_MODEL.to_string());

    Ok(EmbeddingConfig {
        api_key,
        model_name,
    })
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
                "No OpenRouter embedding engine configured (set OpenRouter API key; Python/fastembed fallback is disabled)",
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

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

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
        let message = embedding_degradation_log("item-99", "OpenRouter embedding failed");
        assert!(
            message.starts_with("[nlp/embeddings] Skipping embedding for "),
            "log message prefix should remain stable for observability tooling"
        );
    }

    #[test]
    fn config_from_settings_defaults_to_bge_m3() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(
            "CREATE TABLE app_settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);\
             INSERT INTO app_settings(key, value) VALUES ('openrouter_api_key', 'sk-test');",
        )
        .expect("settings table should be created");

        let config = config_from_settings(&conn).expect("config should resolve");

        assert_eq!(config.model_name, DEFAULT_OPENROUTER_EMBEDDING_MODEL);
    }

    #[test]
    fn config_from_settings_allows_embedding_model_override() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch(
            "CREATE TABLE app_settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);\
             INSERT INTO app_settings(key, value) VALUES ('openrouter_api_key', 'sk-test');\
             INSERT INTO app_settings(key, value) VALUES ('openrouter_embedding_model', 'custom/model');",
        )
        .expect("settings table should be created");

        let config = config_from_settings(&conn).expect("config should resolve");

        assert_eq!(config.model_name, "custom/model");
    }

    #[test]
    fn config_from_settings_requires_openrouter_key() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite should open");
        conn.execute_batch("CREATE TABLE app_settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .expect("settings table should be created");

        let error = match config_from_settings(&conn) {
            Ok(_) => panic!("missing key should fail"),
            Err(error) => error,
        };

        assert!(error.contains("OpenRouter API key"));
        assert!(error.contains("No hay fallback"));
    }

    #[tokio::test]
    async fn init_can_drop_embedding_engine_inside_tokio_context() {
        let engine = EmbeddingEngine::init_with_endpoint(
            EmbeddingConfig {
                api_key: "sk-test".to_string(),
                model_name: DEFAULT_OPENROUTER_EMBEDDING_MODEL.to_string(),
            },
            "http://127.0.0.1:9".to_string(),
        )
        .expect("engine init should not create a blocking runtime");

        drop(engine);
    }

    #[test]
    fn embed_text_accepts_successful_openrouter_bge_m3_response_with_1024_dimensions() {
        let vector: Vec<f32> = (0..OPENROUTER_EMBEDDING_DIMENSIONS)
            .map(|index| index as f32 / 10.0)
            .collect();
        let expected_last = vector[OPENROUTER_EMBEDDING_DIMENSIONS - 1];
        let endpoint = local_openrouter_embedding_server(vector.clone());
        let engine = EmbeddingEngine::init_with_endpoint(
            EmbeddingConfig {
                api_key: "sk-test".to_string(),
                model_name: DEFAULT_OPENROUTER_EMBEDDING_MODEL.to_string(),
            },
            endpoint,
        )
        .expect("test embedding engine should initialize");

        let result = engine
            .embed_text("texto histórico para embedding")
            .expect("mocked OpenRouter response should embed successfully");

        assert_eq!(result.len(), OPENROUTER_EMBEDDING_DIMENSIONS);
        assert_eq!(result[0], 0.0);
        assert_eq!(result[OPENROUTER_EMBEDDING_DIMENSIONS - 1], expected_last);
    }

    fn local_openrouter_embedding_server(vector: Vec<f32>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock server should bind");
        let endpoint = format!(
            "http://{}",
            listener.local_addr().expect("local addr should exist")
        );

        thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("mock server should receive request");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = stream
                    .read(&mut buffer)
                    .expect("request should be readable");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let request_text = String::from_utf8_lossy(&request);
            let content_length = request_text
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .expect("OpenRouter request should include a JSON body length");
            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
                .expect("HTTP headers should terminate");
            while request.len() < header_end + content_length {
                let read = stream
                    .read(&mut buffer)
                    .expect("request body should be readable");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            let request_text = String::from_utf8_lossy(&request);
            assert!(request_text.starts_with("POST / HTTP/1.1"));
            assert!(request_text.contains("authorization: Bearer sk-test"));
            assert!(request_text.contains("\"model\":\"baai/bge-m3\""));
            assert!(request_text.contains("texto histórico para embedding"));

            let body = serde_json::json!({
                "data": [
                    { "embedding": vector }
                ]
            })
            .to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("mock response should write");
        });

        endpoint
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
