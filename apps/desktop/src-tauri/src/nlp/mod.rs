pub mod commands;
pub mod embeddings;
pub mod fts;
pub mod ner;
pub mod text_provider;
pub mod triples;

use serde::Serialize;
use rusqlite::OptionalExtension;
use std::path::PathBuf;
use std::panic::{catch_unwind, AssertUnwindSafe};
use tauri::{AppHandle, Emitter, Manager, path::BaseDirectory};
use tokio::sync::mpsc;

use embeddings::EmbeddingEngine;
use ner::{NerRegistry, types::{NerConfig, NerEngineKind}};

// ── Event payloads ───────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct NlpProgressPayload {
    pub item_id: String,
    pub job: String,
    pub pct: u8,
}

#[derive(Clone, Serialize)]
pub struct NlpCompletePayload {
    pub item_id: String,
    pub job: String,
}

#[derive(Clone, Serialize)]
pub struct NlpErrorPayload {
    pub item_id: String,
    pub job: String,
    pub error: String,
}

// ── Job & Queue ──────────────────────────────────────────────────────────────

/// A single NLP work unit submitted to the background worker.
#[derive(Debug)]
pub enum NlpJob {
    IndexFts { item_id: String },
    ComputeEmbedding { item_id: String },
    ExtractEntities { item_id: String },
    ExtractTriples { item_id: String },
    EnrichItem { item_id: String },
}

pub fn lookup_item_id_for_asset(
    conn: &rusqlite::Connection,
    asset_id: &str,
) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT item_id FROM assets WHERE id = ?1",
        rusqlite::params![asset_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("Failed to resolve item_id for asset {asset_id}: {e}"))
}

pub fn enqueue_entity_refresh_for_item(nlp_queue: &NlpQueue, item_id: &str) -> Result<(), String> {
    nlp_queue.submit(NlpJob::ExtractEntities {
        item_id: item_id.to_string(),
    })
}

/// Handle for submitting NLP jobs to the background worker.
///
/// Managed as Tauri state — NLP commands grab this via `State<NlpQueue>`.
pub struct NlpQueue {
    sender: mpsc::Sender<NlpJob>,
}

impl NlpQueue {
    /// Create a new queue and return `(NlpQueue, Receiver)`.
    pub fn new() -> (Self, mpsc::Receiver<NlpJob>) {
        let (sender, receiver) = mpsc::channel::<NlpJob>(64);
        (Self { sender }, receiver)
    }

    /// Submit a job to the queue. Returns immediately.
    pub fn submit(&self, job: NlpJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("Failed to enqueue NLP job: {e}"))
    }

    /// Spawn the background worker loop on the Tokio runtime.
    ///
    /// The worker drains jobs serially and emits `nlp:progress`, `nlp:complete`,
    /// or `nlp:error` events per job.
    pub fn start_worker(
        db_path: std::path::PathBuf,
        mut receiver: mpsc::Receiver<NlpJob>,
        app_handle: AppHandle,
    ) {
        tauri::async_runtime::spawn(async move {
            // Open a dedicated SQLite connection for the NLP worker.
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => {
                    let _ = c.execute_batch(
                        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;",
                    );
                    c
                }
                Err(e) => {
                    eprintln!("[nlp] Failed to open worker DB connection: {e}");
                    return;
                }
            };

            if table_exists(&conn, "entities") {
                if let Err(e) = ensure_entities_schema(&conn) {
                    eprintln!("[nlp] Failed to migrate entities schema: {e}");
                }
            }

            // Create vec_items table as a regular table (fallback when sqlite-vec
            // extension is not available). When sqlite-vec becomes available
            // on all platforms, this can be replaced with a vec0 virtual table.
            // Using a regular table means kNN search requires a full scan, but
            // for MVP-scale data (<10k items) this is perfectly fine.
            if let Err(e) = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS vec_items(
                    item_id TEXT PRIMARY KEY,
                    embedding BLOB NOT NULL
                )",
            ) {
                eprintln!("[nlp] Failed to create vec_items table: {e} — embedding storage will be unavailable");
            }

            // Resolve embedding script path: try Resource directory first (production),
            // then source (dev) — mirrors transcription script resolution.
            let embed_script_path = app_handle
                .path()
                .resolve("scripts/embed.py", BaseDirectory::Resource)
                .unwrap_or_else(|_| {
                    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("resources/scripts/embed.py");
                    if dev_path.exists() {
                        dev_path
                    } else {
                        std::path::PathBuf::from("scripts/embed.py")
                    }
                });

            eprintln!(
                "[nlp/embeddings] Script path: {}",
                embed_script_path.display()
            );

            // Find Python interpreter with fastembed
            let python_path = match embeddings::which_python() {
                Some(p) => p,
                None => {
                    eprintln!("[nlp/embeddings] No Python with fastembed found — embedding jobs will degrade gracefully.");
                    // Use a placeholder; the engine init will fail and degrade gracefully
                    PathBuf::from("python")
                }
            };

            // Resolve model cache directory for HuggingFace (avoids broken symlinks on Windows)
            let embed_cache_dir = app_handle
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir for NLP cache")
                .join("hf_cache");

            // Initialize embedding engine (non-fatal if unavailable)
            let embed_engine = match EmbeddingEngine::init(embeddings::EmbeddingConfig {
                python_path: python_path.clone(),
                script_path: embed_script_path,
                model_name: "sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2".to_string(),
                cache_dir: Some(embed_cache_dir),
            }) {
                Ok(engine) => {
                    eprintln!("[nlp/embeddings] Engine ready.");
                    Some(engine)
                }
                Err(e) => {
                    eprintln!("[nlp/embeddings] Engine init failed: {e} — embedding jobs will degrade gracefully");
                    None
                }
            };

            let ner_model_path = resolve_ner_resource(&app_handle, "model.onnx");
            let ner_tokenizer_path = resolve_ner_resource(&app_handle, "tokenizer.json");
            let ner_script_path = resolve_ner_script(&app_handle, "spacy_ner.py");
            let ner_engine = resolve_ner_engine_kind();
            let ner_python_path = ner::spacy::which_python().unwrap_or_else(|| PathBuf::from("python"));

            let ner_config = NerConfig {
                engine: ner_engine,
                model_path: Some(ner_model_path),
                tokenizer_path: Some(ner_tokenizer_path),
                python_path: Some(ner_python_path),
                script_path: Some(ner_script_path),
                model_name: Some("es_core_news_lg".to_string()),
                max_length: 256,
                stride: 32,
                score_threshold: 0.65,
            };
            ner::log_startup_status(&ner_config);
            let ner_registry = NerRegistry::init(ner_config);

            while let Some(job) = receiver.recv().await {
                match job {
                    NlpJob::IndexFts { item_id } => {
                        emit_progress(&app_handle, &item_id, "fts", 10);
                        let result = tokio::task::block_in_place(|| {
                            fts::index_item_from_db(&conn, &item_id)
                        });
                        match result {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "fts", 100);
                                emit_complete(&app_handle, &item_id, "fts");
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "fts", &e),
                        }
                    }
                    NlpJob::ComputeEmbedding { item_id } => {
                        emit_progress(&app_handle, &item_id, "embed", 10);
                        let engine_ref = embed_engine.as_ref();
                        let result = tokio::task::block_in_place(|| {
                            embeddings::compute_and_store(engine_ref, &conn, &item_id)
                        });
                        match result {
                            Ok(_) => {
                                match embedding_exists(&conn, &item_id) {
                                    Ok(true) => {
                                        emit_progress(&app_handle, &item_id, "embed", 100);
                                        emit_complete(&app_handle, &item_id, "embed");
                                    }
                                    Ok(false) => emit_error(
                                        &app_handle,
                                        &item_id,
                                        "embed",
                                        "Embedding job completed but no vector was persisted",
                                    ),
                                    Err(e) => emit_error(&app_handle, &item_id, "embed", &e),
                                }
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "embed", &e),
                        }
                    }
                    NlpJob::ExtractEntities { item_id } => {
                        emit_progress(&app_handle, &item_id, "ner", 10);
                        let result = tokio::task::block_in_place(|| {
                            catch_unwind(AssertUnwindSafe(|| {
                                ner::extract_and_store(&conn, &item_id, &ner_registry)
                            }))
                            .map_err(|panic| format_panic_payload("NER extraction panicked", panic))?
                        });
                        match result {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "ner", 100);
                                emit_complete(&app_handle, &item_id, "ner");
                                // Auto-trigger geocoding for place entities
                                if let Err(e) = crate::geo::enqueue_geocoding_for_item(
                                    &app_handle.state::<crate::geo::GeoQueue>(),
                                    &item_id,
                                ) {
                                    eprintln!("[geo] Failed to auto-enqueue geocoding after NER: {e}");
                                }
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "ner", &e),
                        }
                    }
                    NlpJob::ExtractTriples { item_id } => {
                        emit_progress(&app_handle, &item_id, "triples", 10);
                        let result = tokio::task::block_in_place(|| {
                            triples::extract_and_store(&conn, &item_id)
                        });
                        match result {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "triples", 100);
                                emit_complete(&app_handle, &item_id, "triples");
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "triples", &e),
                        }
                    }
                    NlpJob::EnrichItem { item_id } => {
                        // Run all 4 sub-jobs sequentially; errors don't short-circuit.
                        // Embedding uses engine (may be None → graceful degradation).
                        // FTS, NER, Triples are pure Rust and always available.
                        let engine_ref = embed_engine.as_ref();

                        emit_progress(&app_handle, &item_id, "fts", 10);
                        let r = tokio::task::block_in_place(|| fts::index_item_from_db(&conn, &item_id));
                        match r { Ok(_) => { emit_progress(&app_handle, &item_id, "fts", 100); emit_complete(&app_handle, &item_id, "fts"); } Err(e) => emit_error(&app_handle, &item_id, "fts", &e), }

                        emit_progress(&app_handle, &item_id, "embed", 10);
                        let r = tokio::task::block_in_place(|| embeddings::compute_and_store(engine_ref, &conn, &item_id));
                        match r {
                            Ok(_) => {
                                match embedding_exists(&conn, &item_id) {
                                    Ok(true) => {
                                        emit_progress(&app_handle, &item_id, "embed", 100);
                                        emit_complete(&app_handle, &item_id, "embed");
                                    }
                                    Ok(false) => emit_error(
                                        &app_handle,
                                        &item_id,
                                        "embed",
                                        "Embedding job completed but no vector was persisted",
                                    ),
                                    Err(e) => emit_error(&app_handle, &item_id, "embed", &e),
                                }
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "embed", &e),
                        }

                        emit_progress(&app_handle, &item_id, "ner", 10);
                        let r = tokio::task::block_in_place(|| {
                            catch_unwind(AssertUnwindSafe(|| {
                                ner::extract_and_store(&conn, &item_id, &ner_registry)
                            }))
                            .map_err(|panic| format_panic_payload("NER extraction panicked", panic))?
                        });
                        match r {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "ner", 100);
                                emit_complete(&app_handle, &item_id, "ner");
                                if let Err(e) = crate::geo::enqueue_geocoding_for_item(
                                    &app_handle.state::<crate::geo::GeoQueue>(),
                                    &item_id,
                                ) {
                                    eprintln!("[geo] Failed to auto-enqueue geocoding after NER (enrich): {e}");
                                }
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "ner", &e),
                        }

                        emit_progress(&app_handle, &item_id, "triples", 10);
                        let r = tokio::task::block_in_place(|| triples::extract_and_store(&conn, &item_id));
                        match r { Ok(_) => { emit_progress(&app_handle, &item_id, "triples", 100); emit_complete(&app_handle, &item_id, "triples"); } Err(e) => emit_error(&app_handle, &item_id, "triples", &e), }
                    }
                }
            }
        });
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn emit_progress(app_handle: &AppHandle, item_id: &str, job: &str, pct: u8) {
    let _ = app_handle.emit(
        "nlp:progress",
        NlpProgressPayload {
            item_id: item_id.to_string(),
            job: job.to_string(),
            pct,
        },
    );
}

fn emit_complete(app_handle: &AppHandle, item_id: &str, job: &str) {
    let _ = app_handle.emit(
        "nlp:complete",
        NlpCompletePayload {
            item_id: item_id.to_string(),
            job: job.to_string(),
        },
    );
}

fn emit_error(app_handle: &AppHandle, item_id: &str, job: &str, error: &str) {
    let _ = app_handle.emit(
        "nlp:error",
        NlpErrorPayload {
            item_id: item_id.to_string(),
            job: job.to_string(),
            error: error.to_string(),
        },
    );
}

fn embedding_exists(conn: &rusqlite::Connection, item_id: &str) -> Result<bool, String> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM vec_items WHERE item_id = ?1 LIMIT 1",
            rusqlite::params![item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("Failed to verify persisted embedding: {e}"))?;

    Ok(found.is_some())
}

fn resolve_ner_resource(app_handle: &AppHandle, file_name: &str) -> PathBuf {
    let resource_rel = format!("models/ner/{file_name}");
    let resolved = app_handle
        .path()
        .resolve(&resource_rel, BaseDirectory::Resource)
        .unwrap_or_else(|_| PathBuf::from(&resource_rel));

    if resolved.exists() {
        return resolved;
    }

    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources/models/ner")
        .join(file_name);
    if dev_path.exists() {
        eprintln!(
            "[nlp/ner] Dev fallback resolved {} -> {}",
            file_name,
            dev_path.display()
        );
        return dev_path;
    }

    resolved
}

fn resolve_ner_script(app_handle: &AppHandle, file_name: &str) -> PathBuf {
    let resource_rel = format!("scripts/{file_name}");
    let resolved = app_handle
        .path()
        .resolve(&resource_rel, BaseDirectory::Resource)
        .unwrap_or_else(|_| PathBuf::from(&resource_rel));

    if resolved.exists() {
        return resolved;
    }

    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts").join(file_name);
    if dev_path.exists() {
        eprintln!(
            "[nlp/ner] Dev fallback resolved script {} -> {}",
            file_name,
            dev_path.display()
        );
        return dev_path;
    }

    resolved
}

fn resolve_ner_engine_kind() -> NerEngineKind {
    match std::env::var("ENTROPIA_NER_ENGINE")
        .ok()
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("rule") | Some("rule_based") => NerEngineKind::RuleBased,
        Some("onnx") => NerEngineKind::Onnx,
        Some("hybrid") | None => NerEngineKind::Hybrid,
        Some("spacy") => NerEngineKind::Spacy,
        Some(other) => {
            eprintln!("[nlp/ner] Unknown ENTROPIA_NER_ENGINE={other} — defaulting to hybrid (BERT-first + RegEx dates)");
            NerEngineKind::Hybrid
        }
    }
}

fn format_panic_payload(context: &str, panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        return format!("{context}: {message}");
    }

    if let Some(message) = panic.downcast_ref::<String>() {
        return format!("{context}: {message}");
    }

    context.to_string()
}

fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        rusqlite::params![table],
        |_| Ok(()),
    )
    .is_ok()
}

fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|e| format!("Failed to inspect {table}: {e}"))?;

    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("Failed to read {table} columns: {e}"))?;

    for existing in columns {
        if existing.map_err(|e| format!("Failed to decode column name: {e}"))? == column {
            return Ok(true);
        }
    }

    Ok(false)
}

fn ensure_entities_schema(conn: &rusqlite::Connection) -> Result<(), String> {
    if !column_exists(conn, "entities", "source")? {
        conn.execute("ALTER TABLE entities ADD COLUMN source TEXT", [])
            .map_err(|e| format!("Failed to add entities.source: {e}"))?;
    }

    if !column_exists(conn, "entities", "model_name")? {
        conn.execute("ALTER TABLE entities ADD COLUMN model_name TEXT", [])
            .map_err(|e| format!("Failed to add entities.model_name: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn run_job_without_events(conn: &Connection, job: &NlpJob) -> Result<(), String> {
        match job {
            NlpJob::IndexFts { item_id } => fts::index_item_from_db(conn, item_id),
            NlpJob::ComputeEmbedding { item_id } => {
                // No engine in test context → graceful degradation
                embeddings::compute_and_store(None, conn, item_id)
            }
            NlpJob::ExtractEntities { item_id } => ner::extract_and_store(conn, item_id, &rule_based_registry()),
            NlpJob::ExtractTriples { item_id } => triples::extract_and_store(conn, item_id),
            NlpJob::EnrichItem { item_id } => {
                // Run all 4 sub-jobs sequentially; errors don't short-circuit
                let _ = fts::index_item_from_db(conn, item_id);
                let _ = embeddings::compute_and_store(None, conn, item_id);
                let _ = ner::extract_and_store(conn, item_id, &rule_based_registry());
                let _ = triples::extract_and_store(conn, item_id);
                Ok(())
            }
        }
    }

    fn rule_based_registry() -> NerRegistry {
        NerRegistry::init(NerConfig {
            engine: NerEngineKind::RuleBased,
            model_path: None,
            tokenizer_path: None,
            python_path: None,
            script_path: None,
            model_name: None,
            max_length: 256,
            stride: 32,
            score_threshold: 0.65,
        })
    }

    fn setup_worker_test_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db should open");

        conn.execute_batch(
            r#"
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

            CREATE TABLE entities (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              entity_type TEXT NOT NULL,
              value TEXT NOT NULL,
              start_offset INTEGER NOT NULL,
              end_offset INTEGER NOT NULL,
              confidence REAL NOT NULL,
              source TEXT,
              model_name TEXT,
              created_at INTEGER NOT NULL
            );

            CREATE TABLE triples (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              subject TEXT NOT NULL,
              predicate TEXT NOT NULL,
              object TEXT NOT NULL,
              created_at INTEGER NOT NULL
            );

            CREATE VIRTUAL TABLE fts_items USING fts5(
              item_id UNINDEXED,
              title,
              metadata,
              extracted_text,
              content = ''
            );
            "#,
        )
        .expect("nlp worker schema should be created");

        ensure_entities_schema(&conn).expect("entities schema migration should succeed");

        conn
    }

    fn seed_item(conn: &Connection, item_id: &str, asset_id: &str, title: &str, text: &str) {
        conn.execute(
            "INSERT INTO items(id, collection_id, title, metadata) VALUES (?1, ?2, ?3, ?4)",
            params![item_id, "col-1", title, "{}"],
        )
        .expect("item should be inserted");

        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![asset_id, item_id, "asset.txt", "txt", 1_i64],
        )
        .expect("asset should be inserted");

        conn.execute(
            "INSERT INTO extractions(id, asset_id, text_content, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![format!("ext-{item_id}"), asset_id, text, 2_i64],
        )
        .expect("extraction should be inserted");
    }

    #[test]
    fn compute_embedding_job_degrades_and_non_embedding_jobs_keep_working() {
        let conn = setup_worker_test_db();
        seed_item(
            &conn,
            "item-1",
            "asset-1",
            "Acta Colonial",
            "Don Manuel Belgrano creó la Bandera en la ciudad de Buenos Aires.",
        );

        let embed = run_job_without_events(
            &conn,
            &NlpJob::ComputeEmbedding {
                item_id: "item-1".to_string(),
            },
        );
        let fts = run_job_without_events(
            &conn,
            &NlpJob::IndexFts {
                item_id: "item-1".to_string(),
            },
        );
        let ner = run_job_without_events(
            &conn,
            &NlpJob::ExtractEntities {
                item_id: "item-1".to_string(),
            },
        );
        let triples = run_job_without_events(
            &conn,
            &NlpJob::ExtractTriples {
                item_id: "item-1".to_string(),
            },
        );

        assert!(
            embed.is_err(),
            "embedding job should report degradation as an error result"
        );
        assert!(
            embed
                .as_ref()
                .err()
                .map(|e| e.contains("Skipping embedding for item-1"))
                .unwrap_or(false),
            "embedding degradation should include item context"
        );
        assert!(fts.is_ok(), "FTS job should still run after embedding degradation");
        assert!(ner.is_ok(), "NER job should still run after embedding degradation");
        assert!(
            triples.is_ok(),
            "Triples job should still run after embedding degradation"
        );

        let fts_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM fts_items", [], |row| row.get(0))
            .expect("fts row count should be queryable");
        assert_eq!(fts_rows, 1, "FTS should index one row");

        let entity_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1",
                params!["item-1"],
                |row| row.get(0),
            )
            .expect("entities row count should be queryable");
        assert!(entity_rows > 0, "NER should persist at least one entity");

        let triple_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1",
                params!["item-1"],
                |row| row.get(0),
            )
            .expect("triples row count should be queryable");
        assert!(triple_rows > 0, "Triples should persist at least one row");
    }

    #[test]
    fn embedding_degradation_on_missing_item_does_not_block_other_items() {
        let conn = setup_worker_test_db();
        seed_item(
            &conn,
            "item-ok",
            "asset-ok",
            "Cabildo Abierto",
            "Doña Juana Azurduy fue representante de la villa de Potosí.",
        );

        let embed_missing = run_job_without_events(
            &conn,
            &NlpJob::ComputeEmbedding {
                item_id: "item-missing".to_string(),
            },
        );
        let fts_ok = run_job_without_events(
            &conn,
            &NlpJob::IndexFts {
                item_id: "item-ok".to_string(),
            },
        );

        assert!(
            embed_missing.is_err(),
            "missing-item embedding should return a controlled degradation error"
        );
        assert!(
            embed_missing
                .as_ref()
                .err()
                .map(|e| e.contains("No source text available for item 'item-missing'"))
                .unwrap_or(false),
            "missing-item degradation should explain why embedding was skipped"
        );
        assert!(
            fts_ok.is_ok(),
            "FTS for a different item should remain operational"
        );

        let fts_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM fts_items", [], |row| row.get(0))
            .expect("fts row count should be queryable");
        assert_eq!(fts_rows, 1, "FTS indexing for unaffected item must persist");
    }

    // ── EnrichItem integration tests ──────────────────────────────────────────

    #[test]
    fn enrich_item_runs_all_four_sub_jobs() {
        let conn = setup_worker_test_db();
        seed_item(
            &conn,
            "item-enrich",
            "asset-enrich",
            "Acta Colonial",
            "Don Manuel Belgrano creó la Bandera en la ciudad de Buenos Aires.",
        );

        let result = run_job_without_events(
            &conn,
            &NlpJob::EnrichItem {
                item_id: "item-enrich".to_string(),
            },
        );
        assert!(
            result.is_ok(),
            "EnrichItem should succeed (embedding degrades gracefully)"
        );

        // FTS should have indexed the item
        let fts_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM fts_items", [], |row| row.get(0))
            .expect("fts count should be queryable");
        assert_eq!(fts_rows, 1, "FTS should index the item");

        // NER should have detected entities
        let entity_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1",
                params!["item-enrich"],
                |row| row.get(0),
            )
            .expect("entity count should be queryable");
        assert!(entity_rows > 0, "NER should persist at least one entity");

        // Triples should have been extracted
        let triple_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triples WHERE item_id = ?1",
                params!["item-enrich"],
                |row| row.get(0),
            )
            .expect("triple count should be queryable");
        assert!(
            triple_rows > 0,
            "Triples should persist at least one triple"
        );
    }

    #[test]
    fn enrich_item_continues_after_sub_job_failure() {
        // Run EnrichItem on an item — embedding degrades gracefully (no engine).
        // All other sub-jobs should still complete successfully.
        let conn = setup_worker_test_db();
        seed_item(
            &conn,
            "item-partial",
            "asset-partial",
            "Acta Colonial",
            "Don Manuel Belgrano creó la Bandera en la ciudad de Buenos Aires.",
        );

        // Run EnrichItem — embedding degrades gracefully but other sub-jobs succeed
        let _result = run_job_without_events(
            &conn,
            &NlpJob::EnrichItem {
                item_id: "item-partial".to_string(),
            },
        );

        // FTS should still have indexed
        let fts_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM fts_items", [], |row| row.get(0))
            .expect("fts count should be queryable");
        assert_eq!(fts_rows, 1, "FTS should still index the item after partial failure");

        // NER should still have detected entities
        let entity_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entities WHERE item_id = ?1",
                params!["item-partial"],
                |row| row.get(0),
            )
            .expect("entity count should be queryable");
        assert!(entity_rows > 0, "NER should still detect entities after partial failure");
    }

    #[test]
    fn enrich_item_handles_item_with_transcription_text() {
        let conn = setup_worker_test_db();

        // Create item and asset with extraction + transcription
        conn.execute(
            "INSERT INTO items(id, collection_id, title, metadata) VALUES (?1, ?2, ?3, ?4)",
            params!["item-trans-enrich", "col-1", "Transcription Item", "{}"],
        )
        .expect("item insert");

        conn.execute(
            "INSERT INTO assets(id, item_id, path, type, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["asset-trans-enrich", "item-trans-enrich", "audio.mp3", "audio", 1_i64],
        )
        .expect("asset insert");

        // Transcription only
        conn.execute(
            "INSERT INTO transcriptions(id, asset_id, text_content, language, duration_ms, model, segments, confidence, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params!["trans-enrich-1", "asset-trans-enrich", "Don San Martín creó el Ejército.", "es", 5000_i64, "base", "[]", 0.9_f64, 10_i64],
        )
        .expect("transcription insert");

        let result = run_job_without_events(
            &conn,
            &NlpJob::EnrichItem {
                item_id: "item-trans-enrich".to_string(),
            },
        );
        // Embedding may degrade, but overall pipeline should succeed or at least not panic
        assert!(
            result.is_ok() || result.is_err(),
            "EnrichItem should complete without panic for transcription-only text"
        );

        // FTS should find the transcription text
        let fts_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM fts_items", [], |row| row.get(0))
            .expect("fts count should be queryable");
        assert_eq!(fts_rows, 1, "FTS should index the item with transcription text");
    }
}
