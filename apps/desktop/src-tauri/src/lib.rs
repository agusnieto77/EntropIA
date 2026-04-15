mod db;
mod nlp;
mod ocr;

use db::state::AppDbState;
use nlp::NlpQueue;
use ocr::OcrQueue;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_dir).expect("Failed to create app data dir");
            eprintln!("[setup] app_dir: {:?}", app_dir);

            let db_path = app_dir.join("entropia.sqlite");
            eprintln!("[setup] db_path: {:?}", db_path);

            // UI connection — used by Tauri IPC commands
            let ui_conn =
                rusqlite::Connection::open(&db_path).expect("Failed to open SQLite database (ui)");
            eprintln!("[setup] DB opened");
            ui_conn
                .execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
                .expect("Failed to configure SQLite pragmas (ui)");
            eprintln!("[setup] PRAGMA foreign_keys=ON");

            // OCR worker connection
            let worker_conn =
                rusqlite::Connection::open(&db_path).expect("Failed to open SQLite database (worker)");
            worker_conn
                .execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
                .expect("Failed to configure SQLite pragmas (worker)");

            app.manage(AppDbState::new(ui_conn, worker_conn));

            // OCR queue: create channel, manage the sender half, spawn worker with receiver
            let (ocr_queue, ocr_receiver) = OcrQueue::new();
            app.manage(ocr_queue);
            OcrQueue::start_worker(db_path.clone(), ocr_receiver, app.handle().clone());

            // NLP queue: create channel, manage the sender half, spawn worker with receiver
            // The NLP worker opens its own dedicated connection so sqlite-vec can be loaded
            // independently without affecting the OCR or UI connections.
            let (nlp_queue, nlp_receiver) = NlpQueue::new();
            app.manage(nlp_queue);
            NlpQueue::start_worker(db_path.clone(), nlp_receiver, app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            db::commands::db_execute,
            db::commands::db_execute_batch,
            db::commands::db_select,
            db::commands::db_select_rows,
            ocr::commands::extract_text,
            ocr::commands::update_extraction_text_cmd,
            nlp::commands::index_fts,
            nlp::commands::embed_item,
            nlp::commands::extract_entities,
            nlp::commands::extract_triples,
            nlp::commands::fts_search,
            nlp::commands::similar_items,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
