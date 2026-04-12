pub mod commands;
pub mod embeddings;
pub mod fts;
pub mod ner;
pub mod triples;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

#[cfg(feature = "embeddings")]
fn load_sqlite_vec(conn: &rusqlite::Connection) -> Result<(), String> {
    #[cfg(windows)]
    {
        sqlite_vec_shim::load(conn)
    }

    #[cfg(not(windows))]
    {
        sqlite_vec_upstream::load(conn)
    }
}

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
pub enum NlpJob {
    IndexFts { item_id: String },
    ComputeEmbedding { item_id: String },
    ExtractEntities { item_id: String },
    ExtractTriples { item_id: String },
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
        tokio::spawn(async move {
            // Open a dedicated SQLite connection for the NLP worker.
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => {
                    let _ = c.execute_batch(
                        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;",
                    );
                    #[cfg(feature = "embeddings")]
                    {
                        // Load sqlite-vec extension for vec0 virtual table support.
                        if let Err(e) = load_sqlite_vec(&c) {
                            // Non-fatal: embeddings will be degraded, FTS5/NER continue.
                            eprintln!("[nlp] sqlite-vec load failed: {e} — embedding jobs will be skipped");
                        }
                    }
                    c
                }
                Err(e) => {
                    eprintln!("[nlp] Failed to open worker DB connection: {e}");
                    return;
                }
            };

            while let Some(job) = receiver.recv().await {
                match job {
                    NlpJob::IndexFts { item_id } => {
                        emit_progress(&app_handle, &item_id, "fts", 10);
                        let result = tokio::task::block_in_place(|| {
                            // Fetch item data from DB and index into FTS5.
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
                        let result = tokio::task::block_in_place(|| embeddings::compute_and_store(&conn, &item_id));
                        match result {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "embed", 100);
                                emit_complete(&app_handle, &item_id, "embed");
                            }
                            Err(e) => emit_error(&app_handle, &item_id, "embed", &e),
                        }
                    }
                    NlpJob::ExtractEntities { item_id } => {
                        emit_progress(&app_handle, &item_id, "ner", 10);
                        let result = tokio::task::block_in_place(|| {
                            ner::extract_and_store(&conn, &item_id)
                        });
                        match result {
                            Ok(_) => {
                                emit_progress(&app_handle, &item_id, "ner", 100);
                                emit_complete(&app_handle, &item_id, "ner");
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn run_job_without_events(conn: &Connection, job: &NlpJob) -> Result<(), String> {
        match job {
            NlpJob::IndexFts { item_id } => fts::index_item_from_db(conn, item_id),
            NlpJob::ComputeEmbedding { item_id } => embeddings::compute_and_store(conn, item_id),
            NlpJob::ExtractEntities { item_id } => ner::extract_and_store(conn, item_id),
            NlpJob::ExtractTriples { item_id } => triples::extract_and_store(conn, item_id),
        }
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

            CREATE TABLE entities (
              id TEXT PRIMARY KEY,
              item_id TEXT NOT NULL,
              entity_type TEXT NOT NULL,
              value TEXT NOT NULL,
              start_offset INTEGER NOT NULL,
              end_offset INTEGER NOT NULL,
              confidence REAL NOT NULL,
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

    #[cfg(not(feature = "embeddings"))]
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

        assert!(embed.is_ok(), "embedding job should degrade non-fatally");
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

    #[cfg(not(feature = "embeddings"))]
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
            embed_missing.is_ok(),
            "missing-item embedding should degrade without hard failure"
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
}
