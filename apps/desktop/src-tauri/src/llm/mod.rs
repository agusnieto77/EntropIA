pub mod commands;
pub mod engine;
pub mod prompt;

use std::path::PathBuf;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::nlp::text_provider;

use self::engine::{LlmConfig, LlmEngine};

// ---------------------------------------------------------------------------
// Job definition
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LlmJob {
    CorrectOcr { item_id: String },
    ExtractEntities { item_id: String },
    ConsolidateEntities {
        item_id: String,
        candidate_entities_json: String,
    },
    ExtractTriples { item_id: String },
    Summarize { item_id: String },
    Classify { item_id: String, categories: Vec<String> },
    Ask { collection_id: String, question: String },
    // Asset-level variants — operate on a single asset/page instead of the whole item.
    // These use get_asset_text() which only fetches text for the specified asset,
    // avoiding context-window overflow on multi-page documents.
    CorrectOcrAsset { asset_id: String },
    ExtractEntitiesAsset { asset_id: String },
    ConsolidateEntitiesAsset {
        asset_id: String,
        candidate_entities_json: String,
    },
    ExtractTriplesAsset { asset_id: String },
    SummarizeAsset { asset_id: String },
}

impl LlmJob {
    fn job_name(&self) -> &'static str {
        match self {
            LlmJob::CorrectOcr { .. } => "correct_ocr",
            LlmJob::ExtractEntities { .. } => "extract_entities",
            LlmJob::ConsolidateEntities { .. } => "consolidate_entities",
            LlmJob::ExtractTriples { .. } => "extract_triples",
            LlmJob::Summarize { .. } => "summarize",
            LlmJob::Classify { .. } => "classify",
            LlmJob::Ask { .. } => "ask",
            LlmJob::CorrectOcrAsset { .. } => "correct_ocr",
            LlmJob::ExtractEntitiesAsset { .. } => "extract_entities",
            LlmJob::ConsolidateEntitiesAsset { .. } => "consolidate_entities",
            LlmJob::ExtractTriplesAsset { .. } => "extract_triples",
            LlmJob::SummarizeAsset { .. } => "summarize",
        }
    }

    /// Returns the ID used as the event/persistence target.
    /// For asset-level jobs, this is the asset_id; for item-level, the item_id.
    fn target_id(&self) -> &str {
        match self {
            LlmJob::CorrectOcr { item_id }
            | LlmJob::ExtractEntities { item_id }
            | LlmJob::ConsolidateEntities { item_id, .. }
            | LlmJob::ExtractTriples { item_id }
            | LlmJob::Summarize { item_id }
            | LlmJob::Classify { item_id, .. } => item_id,
            LlmJob::Ask { collection_id, .. } => collection_id,
            LlmJob::CorrectOcrAsset { asset_id }
            | LlmJob::ExtractEntitiesAsset { asset_id }
            | LlmJob::ConsolidateEntitiesAsset { asset_id, .. }
            | LlmJob::ExtractTriplesAsset { asset_id }
            | LlmJob::SummarizeAsset { asset_id } => asset_id,
        }
    }

}

// ---------------------------------------------------------------------------
// Event payloads
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct LlmProgressPayload {
    pub id: String,
    pub job: String,
    pub pct: u8,
}

#[derive(Clone, Serialize)]
pub struct LlmCompletePayload {
    pub id: String,
    pub job: String,
    pub result: String,
}

#[derive(Clone, Serialize)]
pub struct LlmErrorPayload {
    pub id: String,
    pub job: String,
    pub error: String,
}

fn emit_progress(app_handle: &AppHandle, id: &str, job: &str, pct: u8) {
    let _ = app_handle.emit(
        "llm:progress",
        LlmProgressPayload {
            id: id.to_string(),
            job: job.to_string(),
            pct,
        },
    );
}

fn emit_complete(app_handle: &AppHandle, id: &str, job: &str, result: &str) {
    let _ = app_handle.emit(
        "llm:complete",
        LlmCompletePayload {
            id: id.to_string(),
            job: job.to_string(),
            result: result.to_string(),
        },
    );
}

fn emit_error(app_handle: &AppHandle, id: &str, job: &str, error: &str) {
    let _ = app_handle.emit(
        "llm:error",
        LlmErrorPayload {
            id: id.to_string(),
            job: job.to_string(),
            error: error.to_string(),
        },
    );
}

// ---------------------------------------------------------------------------
// Result retrieval (for UI hydration after page reload)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct LlmResultEntry {
    pub target_id: String,
    pub job_type: String,
    pub result: String,
    pub created_at: i64,
}

/// Fetch the latest LLM result for a given target (item or collection) and
/// optional job type. Returns `None` if no result is found.
pub fn get_latest_result(
    conn: &rusqlite::Connection,
    target_id: &str,
    job_type: Option<&str>,
) -> Result<Option<LlmResultEntry>, String> {
    let row = if let Some(jt) = job_type {
        conn.query_row(
            "SELECT target_id, job_type, result, created_at
             FROM llm_results
             WHERE target_id = ?1 AND job_type = ?2
             ORDER BY created_at DESC LIMIT 1",
            params![target_id, jt],
            |row| {
                Ok(LlmResultEntry {
                    target_id: row.get(0)?,
                    job_type: row.get(1)?,
                    result: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
    } else {
        conn.query_row(
            "SELECT target_id, job_type, result, created_at
             FROM llm_results
             WHERE target_id = ?1
             ORDER BY created_at DESC LIMIT 1",
            params![target_id],
            |row| {
                Ok(LlmResultEntry {
                    target_id: row.get(0)?,
                    job_type: row.get(1)?,
                    result: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
    };

    match row {
        Ok(entry) => Ok(Some(entry)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to query llm_results: {e}")),
    }
}

/// Fetch all latest LLM results for a given target (one per job_type).
pub fn get_all_results_for_target(
    conn: &rusqlite::Connection,
    target_id: &str,
) -> Result<Vec<LlmResultEntry>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT target_id, job_type, result, created_at
             FROM llm_results
             WHERE target_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| format!("Failed to prepare llm_results query: {e}"))?;

    let rows = stmt
        .query_map(params![target_id], |row| {
            Ok(LlmResultEntry {
                target_id: row.get(0)?,
                job_type: row.get(1)?,
                result: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| format!("Failed to query llm_results: {e}"))?;

    let mut results = Vec::new();
    let mut seen_job_types = std::collections::HashSet::new();
    for row in rows {
        if let Ok(entry) = row {
            // Keep only the latest result per job_type (DESC order means first is latest)
            if seen_job_types.insert(entry.job_type.clone()) {
                results.push(entry);
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Persist an LLM result to the database. Uses INSERT OR REPLACE so the
/// latest result per (target, job_type) pair is always kept.
fn persist_result(
    conn: &rusqlite::Connection,
    target_id: &str,
    job_type: &str,
    result: &str,
) -> Result<(), String> {
    let id = format!("llr-{target_id}-{job_type}");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    conn.execute(
        "INSERT OR REPLACE INTO llm_results (id, target_id, job_type, result, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, target_id, job_type, result, now],
    )
    .map_err(|e| format!("Failed to persist LLM result: {e}"))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Parse LLM triples JSON and store in the `triples` table
// ---------------------------------------------------------------------------

/// A single triple parsed from the LLM JSON response.
/// Fields use `#[serde(default)]` so incomplete triples (missing object, etc.)
/// deserialize with empty strings instead of failing the entire array.
/// Incomplete triples are filtered out after parsing.
#[derive(Clone, serde::Deserialize)]
struct LlmTriple {
    #[serde(default, alias = "sujeto")]
    subject: String,
    #[serde(default, alias = "predicado")]
    predicate: String,
    #[serde(default, alias = "objeto")]
    object: String,
}

static TRAILING_COMMA_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r",\s*([}\]])").expect("valid trailing comma regex"));
static MISSING_OBJECT_COMMA_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"}\s*\{").expect("valid missing object comma regex"));

impl LlmTriple {
    fn cleaned(mut self) -> Option<Self> {
        self.subject = self.subject.trim().to_string();
        self.predicate = self.predicate.trim().to_string();
        self.object = self.object.trim().to_string();

        if self.subject.is_empty() || self.predicate.is_empty() || self.object.is_empty() {
            return None;
        }

        Some(self)
    }
}

fn strip_markdown_fences(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let without_opening = trimmed
        .strip_prefix("```")
        .unwrap_or(trimmed)
        .trim_start_matches("json")
        .trim_start_matches("JSON")
        .trim_start_matches("javascript")
        .trim_start_matches("js")
        .trim();

    without_opening
        .strip_suffix("```")
        .unwrap_or(without_opening)
        .trim()
        .to_string()
}

fn normalize_jsonish(text: &str) -> String {
    let normalized_quotes = text
        .replace(['“', '”', '„', '‟'], "\"")
        .replace(['’', '‘', '‚', '‛'], "'");

    let without_trailing_commas = TRAILING_COMMA_RE
        .replace_all(normalized_quotes.trim(), "$1")
        .into_owned();

    MISSING_OBJECT_COMMA_RE
        .replace_all(&without_trailing_commas, "},{")
        .into_owned()
}

fn preview_for_log(text: &str, max_chars: usize) -> String {
    let sanitized = text.replace('\r', "\\r").replace('\n', "\\n");
    let mut chars = sanitized.chars();
    let preview: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

fn extract_json_objects(text: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut depth = 0usize;
    let mut start = None;
    let mut in_string = false;
    let mut escape = false;

    for (i, ch) in text.char_indices() {
        if in_string {
            if escape {
                escape = false;
                continue;
            }

            match ch {
                '\\' => escape = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                if depth == 0 {
                    continue;
                }
                depth -= 1;
                if depth == 0 {
                    if let Some(obj_start) = start.take() {
                        objects.push(text[obj_start..=i].to_string());
                    }
                }
            }
            _ => {}
        }
    }

    objects
}

fn parse_single_triple(raw: &str) -> Option<LlmTriple> {
    let normalized = normalize_jsonish(raw);
    serde_json::from_str::<LlmTriple>(&normalized)
        .ok()
        .and_then(LlmTriple::cleaned)
}

fn dedupe_triples(triples: Vec<LlmTriple>) -> Vec<LlmTriple> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for triple in triples {
        let key = (
            triple.subject.to_lowercase(),
            triple.predicate.to_lowercase(),
            triple.object.to_lowercase(),
        );
        if seen.insert(key) {
            deduped.push(triple);
        }
    }

    deduped
}

/// Parse the JSON array of triples returned by Gemma 4.
///
/// The LLM is prompted to return `[{"subject": ..., "predicate": ..., "object": ...}]`.
/// This function is tolerant: it strips markdown fences and trailing text,
/// and parses each triple individually so one bad entry doesn't spoil the rest.
fn parse_triples_json(raw: &str) -> Vec<LlmTriple> {
    let content = strip_markdown_fences(raw);
    let normalized_content = normalize_jsonish(&content);

    let json_candidate = if let Some(start) = normalized_content.find('[') {
        if let Some(end) = normalized_content[start..].rfind(']') {
            normalized_content[start..=start + end].to_string()
        } else {
            normalized_content.clone()
        }
    } else if let (Some(start), Some(end)) = (
        normalized_content.find('{'),
        normalized_content.rfind('}'),
    ) {
        format!("[{}]", &normalized_content[start..=end])
    } else {
        normalized_content.clone()
    };

    // Try parsing the whole array first (fast path).
    // With #[serde(default)] on LlmTriple, incomplete triples become empty-string fields
    // instead of causing a parse error.
    match serde_json::from_str::<Vec<LlmTriple>>(&json_candidate) {
        Ok(triples) => dedupe_triples(
            triples
                .into_iter()
                .filter_map(LlmTriple::cleaned)
                .collect(),
        ),
        Err(_) => {
            // Fast path failed — parse each object individually so one bad triple
            // doesn't spoil the rest. Gemma sometimes omits fields or produces
            // malformed entries in the middle of an otherwise valid array.
            let valid_triples = dedupe_triples(
                extract_json_objects(&normalized_content)
                    .into_iter()
                    .filter_map(|obj| parse_single_triple(&obj))
                    .collect(),
            );

            if valid_triples.is_empty() {
                eprintln!("[llm][triples] failed to parse any triples");
                eprintln!(
                    "[llm][triples] normalized_preview=\"{}\", candidate_preview=\"{}\"",
                    preview_for_log(&normalized_content, 220),
                    preview_for_log(&json_candidate, 220),
                );
            } else {
                eprintln!(
                    "[llm][triples] parse fallback ok: parsed={}, object_candidates={}, candidate_preview=\"{}\"",
                    valid_triples.len(),
                    normalized_content.matches('{').count(),
                    preview_for_log(&json_candidate, 220),
                );
            }

            valid_triples
        }
    }
}

fn fn_uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn fn_now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Store parsed LLM triples into the `triples` table for an item-level job.
/// Deletes existing triples for the item before inserting new ones.
fn store_triples_for_item(
    conn: &rusqlite::Connection,
    item_id: &str,
    raw_json: &str,
) -> Result<usize, String> {
    let triples = parse_triples_json(raw_json);

    // Delete old triples for this item (no asset_id filter => item-level)
    conn.execute("DELETE FROM triples WHERE item_id = ?1", params![item_id])
        .map_err(|e| format!("Failed to delete old triples for item: {e}"))?;

    let mut count = 0;
    for triple in &triples {
        conn.execute(
            "INSERT INTO triples (id, item_id, subject, predicate, object, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                fn_uuid_v4(),
                item_id,
                triple.subject,
                triple.predicate,
                triple.object,
                fn_now_millis(),
            ],
        )
        .map_err(|e| format!("Failed to insert triple: {e}"))?;
        count += 1;
    }
    Ok(count)
}

/// Store parsed LLM triples into the `triples` table for an asset-level job.
/// Deletes existing triples for the specific asset before inserting new ones.
fn store_triples_for_asset(
    conn: &rusqlite::Connection,
    item_id: &str,
    asset_id: &str,
    raw_json: &str,
) -> Result<usize, String> {
    let triples = parse_triples_json(raw_json);

    // Delete old triples for this specific asset only
    conn.execute(
        "DELETE FROM triples WHERE item_id = ?1 AND asset_id = ?2",
        params![item_id, asset_id],
    )
    .map_err(|e| format!("Failed to delete old triples for asset: {e}"))?;

    let mut count = 0;
    for triple in &triples {
        conn.execute(
            "INSERT INTO triples (id, item_id, asset_id, subject, predicate, object, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                fn_uuid_v4(),
                item_id,
                asset_id,
                triple.subject,
                triple.predicate,
                triple.object,
                fn_now_millis(),
            ],
        )
        .map_err(|e| format!("Failed to insert triple: {e}"))?;
        count += 1;
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// Queue
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct LlmQueue {
    sender: mpsc::Sender<LlmJob>,
    /// Shared flag set to `true` after the LLM engine initializes successfully.
    available: Arc<AtomicBool>,
}

impl LlmQueue {
    pub fn new() -> (Self, mpsc::Receiver<LlmJob>) {
        let (sender, receiver) = mpsc::channel::<LlmJob>(64);
        let available = Arc::new(AtomicBool::new(false));
        (
            Self {
                sender,
                available: available.clone(),
            },
            receiver,
        )
    }

    pub fn submit(&self, job: LlmJob) -> Result<(), String> {
        self.sender
            .try_send(job)
            .map_err(|e| format!("LLM queue full or closed: {e}"))
    }

    /// Returns `true` if the LLM engine has been loaded successfully.
    pub fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }

    /// Returns a clone of the availability flag for sharing with the worker.
    /// Used to signal engine readiness from the worker back to the main state.
    pub fn available_flag(&self) -> Arc<AtomicBool> {
        self.available.clone()
    }

    pub fn start_worker(
        db_path: PathBuf,
        mut receiver: mpsc::Receiver<LlmJob>,
        app_handle: AppHandle,
        available: Arc<AtomicBool>,
    ) {
        tauri::async_runtime::spawn(async move {
            const MODEL_FILENAME: &str = "gemma-4-E2B-it-Q4_K_M.gguf";

            // Search for model in multiple locations (first match wins)
            let app_models_dir = db_path
                .parent()
                .expect("db_path should have a parent")
                .join("models");
            std::fs::create_dir_all(&app_models_dir).ok();

            let search_paths = [
                // 1. App data dir: {app_data_dir}/models/
                app_models_dir.join(MODEL_FILENAME),
                // 2. Project root (dev convenience) — handles bartowski prefix too
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(MODEL_FILENAME),
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(format!("google_{MODEL_FILENAME}")),
            ];

            let model_path = search_paths
                .iter()
                .find(|p| p.exists())
                .cloned()
                .unwrap_or_else(|| app_models_dir.join(MODEL_FILENAME));
            eprintln!("[llm] OCRC configured as text-only (multimodal disabled)");

            let config = LlmConfig {
                model_path: model_path.clone(),
                n_ctx: 4096,
                n_threads: None,
                seed: 1234,
            };

            // Initialize engine (optional — degrades gracefully)
            let engine = match tokio::task::spawn_blocking(move || LlmEngine::init(config)).await {
                Ok(Ok(engine)) => {
                    eprintln!("[llm] Engine ready: {}", model_path.display());
                    available.store(true, Ordering::Relaxed);
                    Some(engine)
                }
                Ok(Err(e)) => {
                    eprintln!(
                        "[llm] Engine unavailable: {e} — LLM jobs will degrade gracefully. \
                         Place a GGUF model at: {}",
                        model_path.display()
                    );
                    None
                }
                Err(e) => {
                    eprintln!("[llm] Engine init panicked: {e}");
                    None
                }
            };

            // Open dedicated DB connection for the worker
            let conn = match rusqlite::Connection::open(&db_path) {
                Ok(c) => {
                    let _ =
                        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;");
                    c
                }
                Err(e) => {
                    eprintln!("[llm] Failed to open worker DB connection: {e}");
                    return;
                }
            };

            // Ensure llm_results table exists (idempotent)
            if let Err(e) = conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS llm_results (
                    id TEXT PRIMARY KEY,
                    target_id TEXT NOT NULL,
                    job_type TEXT NOT NULL,
                    result TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_llm_results_target ON llm_results(target_id);",
            ) {
                eprintln!("[llm] Warning: could not create llm_results table: {e}");
            }

            // Main worker loop
            while let Some(job) = receiver.recv().await {
                let job_name = job.job_name();
                let id = job.target_id().to_string();

                let engine = match &engine {
                    Some(e) => e,
                    None => {
                        emit_error(
                            &app_handle,
                            &id,
                            job_name,
                            "LLM engine not available. Place a GGUF model in the models/ directory.",
                        );
                        continue;
                    }
                };

                emit_progress(&app_handle, &id, job_name, 10);

                let result = tokio::task::block_in_place(|| {
                    process_job(engine, &conn, &job)
                });

                match result {
                    Ok(output) => {
                        // Persist result to database (non-fatal if it fails)
                        if let Err(e) = persist_result(&conn, &id, job_name, &output) {
                            eprintln!("[llm] Warning: failed to persist result for {id}/{job_name}: {e}");
                        }

                        // Parse triples from LLM response and store in `triples` table
                        // so the Semantic Triples section UI shows LLM-extracted triples.
                        match &job {
                            LlmJob::ExtractTriples { item_id } => {
                                match store_triples_for_item(&conn, item_id, &output) {
                                    Ok(count) => eprintln!("[llm] Stored {count} triples for item {item_id}"),
                                    Err(e) => eprintln!("[llm] Warning: failed to store triples for item {item_id}: {e}"),
                                }
                            }
                            LlmJob::ExtractTriplesAsset { asset_id } => {
                                // Resolve item_id from asset_id for the triples table
                                match crate::nlp::lookup_item_id_for_asset(&conn, asset_id) {
                                    Ok(Some(item_id)) => {
                                        match store_triples_for_asset(&conn, &item_id, asset_id, &output) {
                                            Ok(count) => eprintln!("[llm] Stored {count} triples for asset {asset_id}"),
                                            Err(e) => eprintln!("[llm] Warning: failed to store triples for asset {asset_id}: {e}"),
                                        }
                                    }
                                    Ok(None) => eprintln!("[llm] Warning: no item_id found for asset {asset_id}, skipping triples storage"),
                                    Err(e) => eprintln!("[llm] Warning: failed to lookup item_id for asset {asset_id}: {e}"),
                                }
                            }
                            _ => {} // Other job types don't produce triples
                        }

                        emit_progress(&app_handle, &id, job_name, 100);
                        emit_complete(&app_handle, &id, job_name, &output);
                    }
                    Err(e) => {
                        emit_error(&app_handle, &id, job_name, &e);
                    }
                }
            }

            eprintln!("[llm] Worker loop ended — channel closed.");
        });
    }
}

// ---------------------------------------------------------------------------
// Job processing
// ---------------------------------------------------------------------------

/// Max tokens for generation per job type.
fn max_tokens_for(job: &LlmJob) -> i32 {
    match job {
        LlmJob::CorrectOcr { .. } | LlmJob::CorrectOcrAsset { .. } => 2048,
        LlmJob::ExtractEntities { .. }
        | LlmJob::ExtractEntitiesAsset { .. }
        | LlmJob::ConsolidateEntities { .. }
        | LlmJob::ConsolidateEntitiesAsset { .. } => 1024,
        LlmJob::ExtractTriples { .. } | LlmJob::ExtractTriplesAsset { .. } => 1024,
        LlmJob::Summarize { .. } | LlmJob::SummarizeAsset { .. } => 512,
        LlmJob::Classify { .. } => 256,
        LlmJob::Ask { .. } => 512,
    }
}

/// Truncate text to the last sentence boundary (period, exclamation, question mark)
/// so it doesn't cut mid-sentence. Used for summaries that get truncated by token limits.
fn truncate_to_sentence_boundary(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // If the text already ends with a sentence-ending punctuation, it's fine.
    if trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?') || trimmed.ends_with('。') || trimmed.ends_with('！') {
        return trimmed.to_string();
    }

    // Find the last sentence-ending punctuation and truncate there.
    // Search backwards for . ! ? 。 ！
    let sentence_enders = ['.', '!', '?', '。', '！'];
    if let Some(pos) = trimmed.rfind(sentence_enders) {
        // Include the punctuation character
        trimmed[..=pos].to_string()
    } else {
        // No sentence boundary found at all — return as-is (better than nothing)
        trimmed.to_string()
}
}

// ---------------------------------------------------------------------------
// Text truncation for context safety
// ---------------------------------------------------------------------------

/// Conservative characters-per-token estimate for Latin-script text.
/// Gemma tokenizer averages ~3.5 chars/token for English/Spanish; using 3.0
/// provides a safety margin for multi-byte characters and template overhead.
const CHARS_PER_TOKEN_ESTIMATE: usize = 3;

/// Tokens reserved for prompt template instructions and formatting.
/// Each prompt wraps the text in instruction text (~50-150 tokens for Gemma
/// chat format markers + task instructions).
const TEMPLATE_OVERHEAD_TOKENS: i32 = 128;

/// Truncate text so that the resulting prompt + max_tokens fits within n_ctx.
/// Uses a conservative heuristic and cuts at sentence boundaries when possible.
fn truncate_text_for_context(n_ctx: u32, max_tokens: i32, text: &str) -> String {
    let budget_tokens = (n_ctx as i32) - max_tokens - TEMPLATE_OVERHEAD_TOKENS;
    if budget_tokens <= 0 {
        // Extremely small context — return first ~500 chars as a last resort
        return text.chars().take(500).collect();
    }
    let budget_chars = budget_tokens as usize * CHARS_PER_TOKEN_ESTIMATE;
    let text_chars = text.chars().count();
    if text_chars <= budget_chars {
        return text.to_string();
    }

    // Collect chars up to budget, then try to cut at the last sentence boundary
    let truncated: String = text.chars().take(budget_chars).collect();
    if let Some(pos) = truncated.rfind(|c: char| c == '.' || c == '\n' || c == '！' || c == '。') {
        // Keep up to and including the sentence boundary char
        truncated[..=pos].to_string()
    } else {
        truncated
    }
}

// ---------------------------------------------------------------------------
// Context gathering for Ask (FTS-based)
// ---------------------------------------------------------------------------

/// Maximum context size in characters for Ask queries (~2000 tokens budget).
const MAX_ASK_CONTEXT_CHARS: usize = 6000;

/// Maximum characters per individual document snippet (~400 tokens).
const MAX_SNIPPET_CHARS: usize = 1200;

fn process_job(engine: &LlmEngine, conn: &rusqlite::Connection, job: &LlmJob) -> Result<String, String> {
    let n_ctx = engine.n_ctx();

    match job {
        LlmJob::CorrectOcr { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for OCR correction".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::ocr_correction(&truncated);
            engine.generate_ocr_correction(&p, max_tokens_for(job))
        }

        LlmJob::ExtractEntities { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for entity extraction".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::extract_entities(&truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ConsolidateEntities {
            item_id,
            candidate_entities_json,
        } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for entity consolidation".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::consolidate_entities(&truncated, candidate_entities_json);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ExtractTriples { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for triple extraction".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::extract_triples(&truncated);
            engine.generate_triples(&p, max_tokens_for(job))
        }

        LlmJob::Summarize { item_id } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for summarization".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::summarize(&truncated);
            let result = engine.generate(&p, max_tokens_for(job))?;
            Ok(truncate_to_sentence_boundary(&result))
        }

        LlmJob::Classify { item_id, categories } => {
            let text = text_provider::get_item_text(conn, item_id)?;
            if text.is_empty() {
                return Err("No text available for classification".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::classify(&truncated, categories);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::Ask { collection_id, question } => {
            // Gather context from FTS search over the collection
            let context = gather_collection_context(conn, collection_id, question)?;
            if context.is_empty() {
                return Err("No relevant documents found for this question".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &context);
            let p = prompt::question_answer(question, &truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        // ── Asset-level variants (single page/asset, avoids context overflow) ──

        LlmJob::CorrectOcrAsset { asset_id } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for OCR correction on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::ocr_correction(&truncated);
            engine.generate_ocr_correction(&p, max_tokens_for(job))
        }

        LlmJob::ExtractEntitiesAsset { asset_id } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for entity extraction on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::extract_entities(&truncated);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ConsolidateEntitiesAsset {
            asset_id,
            candidate_entities_json,
        } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for entity consolidation on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::consolidate_entities(&truncated, candidate_entities_json);
            engine.generate(&p, max_tokens_for(job))
        }

        LlmJob::ExtractTriplesAsset { asset_id } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for triple extraction on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::extract_triples(&truncated);
            engine.generate_triples(&p, max_tokens_for(job))
        }

        LlmJob::SummarizeAsset { asset_id } => {
            let text = text_provider::get_asset_text(conn, asset_id)?;
            if text.is_empty() {
                return Err("No text available for summarization on this asset".to_string());
            }
            let truncated = truncate_text_for_context(n_ctx, max_tokens_for(job), &text);
            let p = prompt::summarize(&truncated);
            let result = engine.generate(&p, max_tokens_for(job))?;
            Ok(truncate_to_sentence_boundary(&result))
        }
    }
}

/// Gathers relevant text snippets from a collection using FTS search.
///
/// Uses the existing `sanitize_fts5_query` to safely handle natural-language
/// questions, and retrieves full text via `text_provider::get_item_text`
/// instead of a broken LEFT JOIN on extrations.
fn gather_collection_context(
    conn: &rusqlite::Connection,
    collection_id: &str,
    question: &str,
) -> Result<String, String> {
    // Sanitize the question for FTS5 — natural-language queries contain
    // operators and noise that break FTS MATCH.
    let fts_query = crate::nlp::fts::sanitize_fts5_query(question);
    if fts_query.is_empty() {
        return Ok(String::new());
    }

    // Find matching item IDs via FTS (top 5 by relevance)
    let item_ids: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT i.id
                 FROM fts_items f
                 JOIN items i ON i.rowid = f.rowid
                 WHERE fts_items MATCH ?1 AND i.collection_id = ?2
                 ORDER BY rank
                 LIMIT 5",
            )
            .map_err(|e| format!("FTS query prepare failed: {e}"))?;

        let rows = stmt
            .query_map(params![fts_query, collection_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| format!("FTS query failed: {e}"))?;

        rows.filter_map(|r| r.ok()).collect()
    };

    if item_ids.is_empty() {
        return Ok(String::new());
    }

    // For each matching item, retrieve full text via text_provider
    let mut context = String::new();
    for item_id in &item_ids {
        let title: String = conn
            .query_row(
                "SELECT title FROM items WHERE id = ?1",
                params![item_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "Unknown".to_string());

        let text = text_provider::get_item_text(conn, item_id).unwrap_or_default();
        if !text.is_empty() {
            // Truncate each snippet to stay within budget
            let display_text: String = if text.chars().count() > MAX_SNIPPET_CHARS {
                text.chars().take(MAX_SNIPPET_CHARS).collect()
            } else {
                text.clone()
            };

            let snippet = format!("--- {} ---\n{}\n\n", title, display_text);
            if context.len() + snippet.len() > MAX_ASK_CONTEXT_CHARS {
                // Budget exceeded — add what fits and stop
                let remaining = MAX_ASK_CONTEXT_CHARS.saturating_sub(context.len());
                if remaining > 0 {
                    context.push_str(&snippet[..remaining.min(snippet.len())]);
                }
                break;
            }
            context.push_str(&snippet);
        }
    }

    Ok(context)
}
