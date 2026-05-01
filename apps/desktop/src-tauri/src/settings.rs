use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::state::AppDbState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct SettingEntry {
    pub key: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn settings_get(
    key: String,
    db: State<'_, AppDbState>,
) -> Result<Option<String>, String> {
    let conn = db
        .ui_conn
        .lock()
        .map_err(|e| format!("DB lock error: {e}"))?;
    let result = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .ok();
    Ok(result)
}

#[tauri::command]
pub async fn settings_set(
    key: String,
    value: String,
    db: State<'_, AppDbState>,
) -> Result<(), String> {
    let conn = db
        .ui_conn
        .lock()
        .map_err(|e| format!("DB lock error: {e}"))?;
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )
    .map_err(|e| format!("Failed to save setting: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn settings_get_all(db: State<'_, AppDbState>) -> Result<Vec<SettingEntry>, String> {
    let conn = db
        .ui_conn
        .lock()
        .map_err(|e| format!("DB lock error: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT key, value FROM app_settings ORDER BY key")
        .map_err(|e| format!("Failed to prepare settings query: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SettingEntry {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })
        .map_err(|e| format!("Failed to query settings: {e}"))?;
    let mut entries = Vec::new();
    for row in rows {
        if let Ok(entry) = row {
            entries.push(entry);
        }
    }
    Ok(entries)
}

#[tauri::command]
pub async fn settings_delete(key: String, db: State<'_, AppDbState>) -> Result<(), String> {
    let conn = db
        .ui_conn
        .lock()
        .map_err(|e| format!("DB lock error: {e}"))?;
    conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])
        .map_err(|e| format!("Failed to delete setting: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers (for Rust-side reading, used by LLM worker)
// ---------------------------------------------------------------------------

/// Read a setting value directly from a rusqlite connection.
/// Used by the LLM worker to read API keys without going through Tauri state.
pub fn get_setting(conn: &rusqlite::Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// Persist a setting value directly from Rust-side worker code.
pub fn set_setting(
    conn: &rusqlite::Connection,
    key: &str,
    value: &str,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

/// Delete a setting directly from Rust-side worker code.
pub fn delete_setting(conn: &rusqlite::Connection, key: &str) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])?;
    Ok(())
}
