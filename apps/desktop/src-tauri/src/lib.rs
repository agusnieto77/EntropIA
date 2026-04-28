mod db;
mod geo;
mod image_edit;
mod llm;
mod nlp;
mod ocr;
mod python_discovery;
mod transcription;

use db::state::AppDbState;
use geo::GeoQueue;
use llm::LlmQueue;
use nlp::NlpQueue;
use ocr::OcrQueue;
use rusqlite::OptionalExtension;
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use std::process::Command;
use tauri::Manager;
use transcription::TranscriptionQueue;

const LEGACY_APP_IDENTIFIER: &str = "com.entropia.app";
const SQLITE_BASENAME: &str = "entropia.sqlite";

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only HTTP(S) URLs are allowed".to_string());
    }

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", &url]);
        cmd
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.arg(&url);
        cmd
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(&url);
        cmd
    };

    command
        .spawn()
        .map_err(|error| format!("Failed to open URL: {error}"))?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Suppress Windows error dialogs and CRT debug assertions that block the
    // process when native libraries crash.
    // This must run before any other initialization.
    #[cfg(target_os = "windows")]
    unsafe {
        const SEM_FAILCRITICALERRORS: u32 = 0x0001;
        const SEM_NOGPFAULTERRORBOX: u32 = 0x0002;
        const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;
        extern "system" { fn SetErrorMode(uMode: u32) -> u32; }
        SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX);

        // Suppress CRT debug assertions in debug builds.
        // Routes assertion output to stderr instead of a blocking dialog.
        #[cfg(debug_assertions)]
        {
            extern "C" {
                fn _CrtSetReportMode(reportType: i32, reportMode: i32) -> i32;
            }
            const _CRT_ASSERT: i32 = 2;
            const _CRTDBG_MODE_FILE: i32 = 4;
            const _CRTDBG_FILE_STDERR: i32 = 2;
            _CrtSetReportMode(_CRT_ASSERT, _CRTDBG_MODE_FILE);
            _CrtSetReportMode(_CRT_ASSERT, _CRTDBG_FILE_STDERR);
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            migrate_legacy_app_dir(&app_dir).expect("Failed to migrate legacy app data dir");
            std::fs::create_dir_all(&app_dir).expect("Failed to create app data dir");
            let db_path = app_dir.join("entropia.sqlite");

migrate_legacy_asset_paths(&db_path, &app_dir)
                .expect("Failed to migrate legacy asset paths in database");

            // UI connection — used by Tauri IPC commands
            let ui_conn =
                rusqlite::Connection::open(&db_path).expect("Failed to open SQLite database (ui)");
            ui_conn
                .execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
                .expect("Failed to configure SQLite pragmas (ui)");

            // Normalize legacy duplicates and enforce one-row-per-asset semantics
            // for extractions/transcriptions so Rust workers can use real UPSERT.
            ui_conn
                .execute_batch(
                    "DELETE FROM extractions
                     WHERE rowid NOT IN (
                       SELECT MAX(rowid) FROM extractions GROUP BY asset_id
                     );
                     DELETE FROM transcriptions
                     WHERE rowid NOT IN (
                       SELECT MAX(rowid) FROM transcriptions GROUP BY asset_id
                     );
                     CREATE UNIQUE INDEX IF NOT EXISTS idx_extractions_asset_id_unique
                     ON extractions(asset_id);
                     CREATE UNIQUE INDEX IF NOT EXISTS idx_transcriptions_asset_id_unique
                     ON transcriptions(asset_id);",
                )
                .expect("Failed to enforce unique asset_id indexes for extraction/transcription");

            // Migrate extractions.method CHECK constraint: remove the legacy
            // `CHECK(method IN ('native', 'ocr'))` which blocked PaddleOCR methods
            // like 'paddle', 'tesseract', 'pdf_paddle', 'pdf_tesseract'.
            migrate_extractions_method_check(&ui_conn)
                .expect("Failed to migrate extractions method CHECK constraint");

// Create layouts table for PaddleVL region persistence
            ui_conn
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS layouts (
                        id TEXT PRIMARY KEY,
                        asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                        regions TEXT NOT NULL,
                        model TEXT NOT NULL,
                        image_width INTEGER NOT NULL,
                        image_height INTEGER NOT NULL,
                        created_at INTEGER NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_layouts_asset_id ON layouts(asset_id);",
                )
                .map_err(|e| format!("Failed to create layouts table: {e}"))
                .expect("Failed to create layouts table");
            let modern_schema_bootstrapped =
                migration_applied(&ui_conn, "0017_vec_assets").unwrap_or(false);

            if !modern_schema_bootstrapped {
                // Legacy fallback for databases that haven't run JS migrations yet.
                let has_sort_index: bool = ui_conn
                    .prepare("SELECT sort_index FROM assets LIMIT 0")
                    .and_then(|mut stmt| {
                        let _ = stmt.query_map([], |_| Ok(()));
                        Ok(true)
                    })
                    .unwrap_or(false);

                if !has_sort_index {
                    ui_conn
                        .execute_batch(
                            "ALTER TABLE assets ADD COLUMN sort_index INTEGER NOT NULL DEFAULT 0;
                             CREATE INDEX IF NOT EXISTS idx_assets_item_sort ON assets(item_id, sort_index);",
                        )
                        .map_err(|e| format!("Failed to add sort_index column to assets: {e}"))
                        .expect("Failed to add sort_index column");
                    eprintln!("[setup] Added sort_index column to assets table");
                }

                ui_conn
                    .execute_batch(
                        "CREATE TABLE IF NOT EXISTS llm_results (
                            id TEXT PRIMARY KEY,
                            target_id TEXT NOT NULL,
                            job_type TEXT NOT NULL,
                            result TEXT NOT NULL,
                            created_at INTEGER NOT NULL
                        );
                        CREATE INDEX IF NOT EXISTS idx_llm_results_target ON llm_results(target_id);",
                    )
                    .map_err(|e| format!("Failed to create llm_results table: {e}"))
                    .expect("Failed to create llm_results table");

                let has_notes_asset_id: bool = ui_conn
                    .prepare("SELECT asset_id FROM notes LIMIT 0")
                    .and_then(|mut stmt| {
                        let _ = stmt.query_map([], |_| Ok(()));
                        Ok(true)
                    })
                    .unwrap_or(false);

                if !has_notes_asset_id {
                    ui_conn
                        .execute_batch(
                            "ALTER TABLE notes ADD COLUMN asset_id TEXT;
                             ALTER TABLE entities ADD COLUMN asset_id TEXT;
                             ALTER TABLE triples ADD COLUMN asset_id TEXT;
                             CREATE INDEX IF NOT EXISTS idx_notes_asset_id ON notes(asset_id);
                             CREATE INDEX IF NOT EXISTS idx_entities_asset_id ON entities(asset_id);
                             CREATE INDEX IF NOT EXISTS idx_triples_asset_id ON triples(asset_id);",
                        )
                        .map_err(|e| format!("Failed to add asset_id columns: {e}"))
                        .expect("Failed to add asset_id columns");
                    eprintln!("[setup] Added asset_id columns to notes, entities, triples");
                }
            }

            // OCR worker connection
            let worker_conn = rusqlite::Connection::open(&db_path)
                .expect("Failed to open SQLite database (worker)");
            worker_conn
                .execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
                .expect("Failed to configure SQLite pragmas (worker)");

            app.manage(AppDbState::new(ui_conn, worker_conn));

            // OCR queue: create channel, manage the sender half, spawn worker with receiver
            let (ocr_queue, ocr_receiver) = OcrQueue::new();
            app.manage(ocr_queue);

            // PaddleVL and layout engine creation deferred to OCR worker (lazy init).
            // This removes Python probing and ONNX model loading from the critical
            // startup path, which previously blocked app window display by 3-15s.
            OcrQueue::start_worker(db_path.clone(), ocr_receiver, app.handle().clone());

            // LLM queue: local Gemma model via llama.cpp for NER, summarization,
            // OCR correction, Q&A, etc. Degrades gracefully if model not present.
            let (llm_queue, llm_receiver) = LlmQueue::new();
            let llm_available = llm_queue.available_flag();
            let nlp_llm_queue = llm_queue.clone();
            app.manage(llm_queue);
            LlmQueue::start_worker(
                db_path.clone(),
                llm_receiver,
                app.handle().clone(),
                llm_available,
            );

            // NLP queue: create channel, manage the sender half, spawn worker with receiver
            // The NLP worker opens its own dedicated connection and initializes the
            // embedding engine (Python subprocess) independently from OCR/UI connections.
            let (nlp_queue, nlp_receiver) = NlpQueue::new();
            // Clone the dedup handle before moving nlp_queue into managed state
            let ner_pending = nlp_queue.ner_pending_handle();
            app.manage(nlp_queue);
            NlpQueue::start_worker(
                db_path.clone(),
                nlp_receiver,
                app.handle().clone(),
                ner_pending,
                nlp_llm_queue,
            );

            // Transcription queue: faster-whisper subprocess for audio transcription.
            // Each job spawns a Python process, no persistent state needed.
            let (transcription_queue, transcription_receiver) = TranscriptionQueue::new();
            app.manage(transcription_queue);
            TranscriptionQueue::start_worker(
                db_path.clone(),
                transcription_receiver,
                app.handle().clone(),
            );

            // Geo queue: Nominatim geocoding for place entities.
            let (geo_queue, geo_receiver) = GeoQueue::new();
            app.manage(geo_queue);
            GeoQueue::start_worker(
                db_path.clone(),
                geo_receiver,
                app.handle().clone(),
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db::commands::db_execute,
            db::commands::db_execute_batch,
            db::commands::db_select,
            db::commands::db_select_rows,
            ocr::commands::extract_text,
            ocr::commands::update_extraction_text_cmd,
            ocr::commands::generate_pdf_thumbnail,
            ocr::commands::delete_pdf_thumbnail,
            ocr::commands::is_scanned_pdf,
            ocr::commands::render_pdf_pages,
            nlp::commands::index_fts,
            nlp::commands::embed_item,
            nlp::commands::embed_asset,
            nlp::commands::extract_entities,
            nlp::commands::extract_entities_for_asset,
            nlp::commands::extract_triples,
            nlp::commands::extract_triples_for_asset,
            nlp::commands::enrich_item,
            nlp::commands::fts_search,
            nlp::commands::similar_items,
            transcription::commands::transcribe_audio,
            transcription::commands::update_transcription_text_cmd,
            llm::commands::llm_correct_ocr,
            llm::commands::llm_extract_entities,
            llm::commands::llm_extract_triples,
            llm::commands::llm_summarize,
            llm::commands::llm_classify,
            llm::commands::llm_ask,
            llm::commands::llm_correct_ocr_asset,
            llm::commands::llm_extract_entities_asset,
            llm::commands::llm_extract_triples_asset,
            llm::commands::llm_summarize_asset,
llm::commands::llm_get_results,
        llm::commands::llm_get_result,
        llm::commands::llm_is_available,
        geo::commands::geocode_entity,
            geo::commands::geocode_item_entities,
            image_edit::crop_image,
            image_edit::rotate_image,
            image_edit::erase_region,
            open_external_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn migrate_legacy_app_dir(app_dir: &Path) -> Result<(), String> {
    let Some(parent_dir) = app_dir.parent() else {
        return Ok(());
    };

    let legacy_dir = parent_dir.join(LEGACY_APP_IDENTIFIER);
    if !legacy_dir.exists() || legacy_dir == app_dir {
        return Ok(());
    }

    if !app_dir.exists() {
        fs::rename(&legacy_dir, app_dir).map_err(|error| {
            format!(
                "Failed to rename legacy app dir from {} to {}: {error}",
                legacy_dir.display(),
                app_dir.display()
            )
        })?;
        eprintln!(
            "[setup] migrated legacy app dir: {} -> {}",
            legacy_dir.display(),
            app_dir.display()
        );
        return Ok(());
    }

    prefer_richer_legacy_database(&legacy_dir, app_dir)?;
    copy_missing_recursive(&legacy_dir, app_dir)?;
    eprintln!(
        "[setup] merged legacy app dir into current app dir: {} -> {}",
        legacy_dir.display(),
        app_dir.display()
    );
    Ok(())
}

fn prefer_richer_legacy_database(legacy_dir: &Path, app_dir: &Path) -> Result<(), String> {
    let legacy_db = legacy_dir.join(SQLITE_BASENAME);
    let current_db = app_dir.join(SQLITE_BASENAME);

    if !legacy_db.exists() {
        return Ok(());
    }

    if !current_db.exists() {
        copy_sqlite_bundle(&legacy_db, &current_db)?;
        eprintln!(
            "[setup] copied legacy sqlite bundle into new app dir: {} -> {}",
            legacy_db.display(),
            current_db.display()
        );
        return Ok(());
    }

    let legacy_score = sqlite_richness_score(&legacy_db).unwrap_or(0);
    let current_score = sqlite_richness_score(&current_db).unwrap_or(0);

    if legacy_score <= current_score {
        eprintln!(
            "[setup] keeping current sqlite bundle (current_score={}, legacy_score={})",
            current_score, legacy_score
        );
        return Ok(());
    }

    backup_sqlite_bundle(&current_db)?;
    remove_sqlite_bundle(&current_db)?;
    copy_sqlite_bundle(&legacy_db, &current_db)?;
    eprintln!(
        "[setup] restored richer legacy sqlite bundle (legacy_score={} > current_score={})",
        legacy_score, current_score
    );
    Ok(())
}

fn sqlite_richness_score(db_path: &Path) -> Option<u64> {
    let conn = Connection::open(db_path).ok()?;
    let mut score = 0_u64;
    for table in [
        "collections",
        "items",
        "assets",
        "notes",
        "extractions",
        "transcriptions",
        "entities",
        "triples",
        "annotations",
    ] {
        score += table_row_count(&conn, table).unwrap_or(0);
    }
    Some(score)
}

fn table_row_count(conn: &Connection, table: &str) -> Option<u64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .ok()
        .map(|count| count.max(0) as u64)
}

fn copy_sqlite_bundle(from_db: &Path, to_db: &Path) -> Result<(), String> {
    let Some(parent) = to_db.parent() else {
        return Err(format!("Target database path has no parent: {}", to_db.display()));
    };
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create directory {}: {error}", parent.display()))?;

    for (source, target) in sqlite_bundle_paths(from_db, to_db) {
        if !source.exists() {
            continue;
        }
        fs::copy(&source, &target).map_err(|error| {
            format!(
                "Failed to copy sqlite bundle file from {} to {}: {error}",
                source.display(),
                target.display()
            )
        })?;
    }
    Ok(())
}

fn remove_sqlite_bundle(db_path: &Path) -> Result<(), String> {
    for path in sqlite_bundle_members(db_path) {
        if !path.exists() {
            continue;
        }
        fs::remove_file(&path)
            .map_err(|error| format!("Failed to remove {}: {error}", path.display()))?;
    }
    Ok(())
}

fn backup_sqlite_bundle(db_path: &Path) -> Result<(), String> {
    for path in sqlite_bundle_members(db_path) {
        if !path.exists() {
            continue;
        }
        let backup = backup_path(&path);
        if backup.exists() {
            continue;
        }
        fs::copy(&path, &backup).map_err(|error| {
            format!(
                "Failed to backup sqlite bundle file from {} to {}: {error}",
                path.display(),
                backup.display()
            )
        })?;
    }
    Ok(())
}

fn backup_path(path: &Path) -> std::path::PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("entropia.sqlite");
    path.with_file_name(format!("{file_name}.before-legacy-restore.bak"))
}

fn sqlite_bundle_paths(from_db: &Path, to_db: &Path) -> Vec<(std::path::PathBuf, std::path::PathBuf)> {
    let from = sqlite_bundle_members(from_db);
    let to = sqlite_bundle_members(to_db);
    from.into_iter().zip(to).collect()
}

fn sqlite_bundle_members(db_path: &Path) -> Vec<std::path::PathBuf> {
    vec![
        db_path.to_path_buf(),
        db_path.with_file_name(format!("{}-wal", db_path.file_name().and_then(|name| name.to_str()).unwrap_or(SQLITE_BASENAME))),
        db_path.with_file_name(format!("{}-shm", db_path.file_name().and_then(|name| name.to_str()).unwrap_or(SQLITE_BASENAME))),
    ]
}

fn copy_missing_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to)
        .map_err(|error| format!("Failed to create directory {}: {error}", to.display()))?;

    for entry in fs::read_dir(from)
        .map_err(|error| format!("Failed to read directory {}: {error}", from.display()))?
    {
        let entry = entry.map_err(|error| {
            format!("Failed to read directory entry in {}: {error}", from.display())
        })?;
        let source_path = entry.path();
        let target_path = to.join(entry.file_name());

        if source_path.is_dir() {
            copy_missing_recursive(&source_path, &target_path)?;
            continue;
        }

        if target_path.exists() {
            continue;
        }

        fs::copy(&source_path, &target_path).map_err(|error| {
            format!(
                "Failed to copy file from {} to {}: {error}",
                source_path.display(),
                target_path.display()
            )
        })?;
    }

    Ok(())
}

fn migrate_legacy_asset_paths(db_path: &Path, app_dir: &Path) -> Result<(), String> {
    let Some(parent_dir) = app_dir.parent() else {
        return Ok(());
    };

    let legacy_dir = parent_dir.join(LEGACY_APP_IDENTIFIER);
    if legacy_dir == app_dir {
        return Ok(());
    }

    let legacy_prefix = legacy_dir.to_string_lossy().to_string();
    let current_prefix = app_dir.to_string_lossy().to_string();

    let conn = Connection::open(db_path)
        .map_err(|error| format!("Failed to open database for asset-path migration: {error}"))?;

    conn.execute(
        "UPDATE assets SET path = REPLACE(path, ?1, ?2) WHERE path LIKE ?3",
        rusqlite::params![legacy_prefix, current_prefix, format!("{}%", legacy_dir.to_string_lossy())],
    )
    .map_err(|error| format!("Failed to migrate asset paths from legacy app dir: {error}"))?;

    Ok(())
}

fn migration_applied(conn: &Connection, name: &str) -> Result<bool, String> {
    let has_migrations_table: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='_migrations' LIMIT 1",
            [],
            |_row| Ok(true),
        )
        .unwrap_or(false);

    if !has_migrations_table {
        return Ok(false);
    }

    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM _migrations WHERE name = ?1 LIMIT 1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("Failed to check migration '{name}': {e}"))?;

    Ok(found.is_some())
}

/// Migrate the `extractions` table to remove the legacy CHECK constraint
/// on the `method` column that only allowed 'native' and 'ocr'.
/// PaddleOCR uses methods like 'paddle', 'tesseract', 'pdf_paddle', 'pdf_tesseract'.
/// SQLite doesn't support ALTER TABLE DROP CONSTRAINT, so we recreate the table.
fn migrate_extractions_method_check(conn: &Connection) -> Result<(), String> {
    // Check if the CHECK constraint exists by attempting an insert with a new method value.
    // If it succeeds, no migration needed.
    let has_check: bool = conn
        .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='extractions'")
        .and_then(|mut stmt| {
            stmt.query_row([], |row| {
                let sql: String = row.get(0)?;
                Ok(sql.contains("CHECK(method IN"))
            })
        })
        .unwrap_or(false);

    if !has_check {
        eprintln!("[setup] extractions.method: no legacy CHECK constraint found — skipping migration");
        return Ok(());
    }

    eprintln!("[setup] Migrating extractions table to remove legacy method CHECK constraint...");

    conn.execute_batch(
        "BEGIN TRANSACTION;
         CREATE TABLE extractions_new (
           id TEXT PRIMARY KEY,
           asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
           text_content TEXT NOT NULL,
           method TEXT NOT NULL,
           confidence REAL,
           created_at INTEGER NOT NULL
         );
         INSERT INTO extractions_new SELECT * FROM extractions;
         DROP TABLE extractions;
         ALTER TABLE extractions_new RENAME TO extractions;
         CREATE INDEX IF NOT EXISTS idx_extractions_asset_id ON extractions(asset_id);
         COMMIT;"
    )
    .map_err(|e| format!("Failed to migrate extractions table: {e}"))?;

    eprintln!("[setup] extractions.method CHECK constraint removed successfully");
    Ok(())
}
