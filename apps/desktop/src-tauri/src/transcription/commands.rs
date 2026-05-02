/// Tauri IPC commands for transcription operations.
use super::{TranscriptionJob, TranscriptionQueue};
use crate::db::state::AppDbState;
use crate::nlp::NlpQueue;
use tauri::{AppHandle, State};

/// Submit a transcription job to the background worker queue.
///
/// Returns immediately with `Ok("queued")`. The worker will process the job
/// asynchronously and emit `transcription:progress`, `transcription:complete`,
/// or `transcription:error` events.
///
/// # Arguments
/// * `asset_id`   — unique ID of the asset in the database
/// * `asset_path` — absolute filesystem path to the audio file
/// * `transcription_queue` — managed state injected by Tauri
#[tauri::command]
pub async fn transcribe_audio(
    asset_id: String,
    asset_path: String,
    transcription_queue: State<'_, TranscriptionQueue>,
) -> Result<String, String> {
    let job = TranscriptionJob {
        asset_id,
        asset_path,
    };

    transcription_queue.submit(job)?;
    Ok("queued".to_string())
}

/// Update the text_content of the latest transcription for an asset.
///
/// This allows users to manually correct transcription output.
/// Downstream NLP refresh is debounced in the frontend after a period of
/// user inactivity, so this command only persists the edited text.
#[tauri::command]
pub async fn update_transcription_text_cmd(
    asset_id: String,
    text_content: String,
    db: State<'_, AppDbState>,
    _nlp_queue: State<'_, NlpQueue>,
) -> Result<(), String> {
    let conn = db
        .ui_conn
        .lock()
        .map_err(|e| format!("DB lock poisoned: {e}"))?;

    // Find the latest transcription for this asset
    let mut stmt = conn
        .prepare(
            "SELECT id FROM transcriptions WHERE asset_id = ?1 ORDER BY created_at DESC LIMIT 1",
        )
        .map_err(|e| format!("Failed to prepare query: {e}"))?;

    let transcription_id: Result<String, _> = stmt.query_row([&asset_id], |row| row.get(0));

    drop(stmt); // release borrow before execute

    match transcription_id {
        Ok(id) => {
            conn.execute(
                "UPDATE transcriptions SET text_content = ?1 WHERE id = ?2",
                rusqlite::params![text_content, id],
            )
            .map_err(|e| format!("Failed to update transcription text: {e}"))?;
        }
        Err(_) => {} // no transcription exists — no-op
    }

    Ok(())
}

#[tauri::command]
pub async fn transcribe_dictation(audio_path: String, app_handle: AppHandle) -> Result<String, String> {
    let audio_path_for_worker = audio_path.clone();
    let transcription_result = tauri::async_runtime::spawn_blocking(move || {
        super::transcribe_audio_file(&app_handle, None, &audio_path_for_worker)
    })
    .await
    .map_err(|e| format!("Dictation task failed: {e}"))?;

    let cleanup_result = super::cleanup_temp_audio_file(&audio_path);

    match (transcription_result, cleanup_result) {
        (Ok(result), Ok(())) => Ok(result.text.trim().to_string()),
        (Ok(result), Err(cleanup_error)) => {
            eprintln!("[transcription] Dictation cleanup warning: {cleanup_error}");
            Ok(result.text.trim().to_string())
        }
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup_error)) => Err(format!(
            "{error}\nTemporary file cleanup failed: {cleanup_error}"
        )),
    }
}
