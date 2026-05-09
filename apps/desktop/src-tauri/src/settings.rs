use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::state::AppDbState;
use crate::runtime::bootstrap::BootstrapRemoteSource;

pub const RUNTIME_BOOTSTRAP_MANIFEST_URL_KEY: &str = "runtime_bootstrap_manifest_url";
pub const RUNTIME_BOOTSTRAP_PUBLIC_KEY_ID_KEY: &str = "runtime_bootstrap_public_key_id";
pub const RUNTIME_BOOTSTRAP_PUBLIC_KEY_KEY_PREFIX: &str = "runtime_bootstrap_public_key.";

async fn invalidate_dependency_probe_cache_if_needed(
    key: &str,
    deps: Option<&State<'_, crate::deps::DepsState>>,
) {
    if crate::deps::should_invalidate_cache_for_setting(key) {
        if let Some(deps_state) = deps {
            crate::deps::invalidate_probe_cache(deps_state.inner()).await;
        }
        crate::python_discovery::invalidate_probe_cache();
    }
}

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
    deps: State<'_, crate::deps::DepsState>,
) -> Result<(), String> {
    let should_invalidate = crate::deps::should_invalidate_cache_for_setting(&key);
    {
        let conn = db
            .ui_conn
            .lock()
            .map_err(|e| format!("DB lock error: {e}"))?;
        conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
            params![key.as_str(), value.as_str()],
        )
        .map_err(|e| format!("Failed to save setting: {e}"))?;
    }
    if should_invalidate {
        invalidate_dependency_probe_cache_if_needed(&key, Some(&deps)).await;
    }
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
pub async fn settings_delete(
    key: String,
    db: State<'_, AppDbState>,
    deps: State<'_, crate::deps::DepsState>,
) -> Result<(), String> {
    let should_invalidate = crate::deps::should_invalidate_cache_for_setting(&key);
    {
        let conn = db
            .ui_conn
            .lock()
            .map_err(|e| format!("DB lock error: {e}"))?;
        conn.execute(
            "DELETE FROM app_settings WHERE key = ?1",
            params![key.as_str()],
        )
        .map_err(|e| format!("Failed to delete setting: {e}"))?;
    }
    if should_invalidate {
        invalidate_dependency_probe_cache_if_needed(&key, Some(&deps)).await;
    }
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

pub fn get_runtime_bootstrap_remote_source(
    conn: &rusqlite::Connection,
) -> Result<Option<BootstrapRemoteSource>, String> {
    let manifest_url = get_setting(conn, RUNTIME_BOOTSTRAP_MANIFEST_URL_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let public_key_id = get_setting(conn, RUNTIME_BOOTSTRAP_PUBLIC_KEY_ID_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match (manifest_url, public_key_id) {
        (None, None) => Ok(None),
        (Some(_), None) => Err(
            "Remote bootstrap source is partially configured: missing public key id".to_string(),
        ),
        (None, Some(_)) => {
            Err("Remote bootstrap source is partially configured: missing manifest URL".to_string())
        }
        (Some(manifest_url), Some(public_key_id)) => {
            if !manifest_url.starts_with("https://") {
                return Err(
                    "Remote bootstrap manifest URL must use HTTPS to be considered trusted"
                        .to_string(),
                );
            }

            Ok(Some(BootstrapRemoteSource {
                manifest_url,
                public_key_id,
            }))
        }
    }
}

pub fn get_runtime_bootstrap_public_key(
    conn: &rusqlite::Connection,
    public_key_id: &str,
) -> Result<String, String> {
    let key = format!("{RUNTIME_BOOTSTRAP_PUBLIC_KEY_KEY_PREFIX}{public_key_id}");
    get_setting(conn, &key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Bootstrap public key '{public_key_id}' is not configured"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory_settings_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            "CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .expect("create app_settings");
        conn
    }

    #[test]
    fn returns_none_when_runtime_bootstrap_source_is_not_configured() {
        let conn = in_memory_settings_db();

        let source = get_runtime_bootstrap_remote_source(&conn).expect("lookup should succeed");

        assert_eq!(source, None);
    }

    #[test]
    fn loads_runtime_bootstrap_source_from_settings_when_complete_and_https() {
        let conn = in_memory_settings_db();
        set_setting(
            &conn,
            RUNTIME_BOOTSTRAP_MANIFEST_URL_KEY,
            "https://example.com/runtime/bootstrap.json",
        )
        .expect("save manifest url");
        set_setting(&conn, RUNTIME_BOOTSTRAP_PUBLIC_KEY_ID_KEY, "entropia-root")
            .expect("save public key id");

        let source = get_runtime_bootstrap_remote_source(&conn).expect("lookup should succeed");

        assert_eq!(
            source,
            Some(BootstrapRemoteSource {
                manifest_url: "https://example.com/runtime/bootstrap.json".to_string(),
                public_key_id: "entropia-root".to_string(),
            })
        );
    }

    #[test]
    fn rejects_partially_configured_runtime_bootstrap_source() {
        let conn = in_memory_settings_db();
        set_setting(
            &conn,
            RUNTIME_BOOTSTRAP_MANIFEST_URL_KEY,
            "https://example.com/runtime/bootstrap.json",
        )
        .expect("save manifest url");

        let error =
            get_runtime_bootstrap_remote_source(&conn).expect_err("partial config must fail");

        assert!(error.contains("missing public key id"));
    }

    #[test]
    fn rejects_non_https_runtime_bootstrap_source() {
        let conn = in_memory_settings_db();
        set_setting(
            &conn,
            RUNTIME_BOOTSTRAP_MANIFEST_URL_KEY,
            "http://example.com/runtime/bootstrap.json",
        )
        .expect("save manifest url");
        set_setting(&conn, RUNTIME_BOOTSTRAP_PUBLIC_KEY_ID_KEY, "entropia-root")
            .expect("save public key id");

        let error =
            get_runtime_bootstrap_remote_source(&conn).expect_err("non-https config must fail");

        assert!(error.contains("HTTPS"));
    }

    #[test]
    fn loads_runtime_bootstrap_public_key_by_key_id() {
        let conn = in_memory_settings_db();
        set_setting(
            &conn,
            "runtime_bootstrap_public_key.entropia-root",
            "base64-public-key",
        )
        .expect("save key");

        let public_key = get_runtime_bootstrap_public_key(&conn, "entropia-root")
            .expect("public key should load");

        assert_eq!(public_key, "base64-public-key");
    }

    #[test]
    fn rejects_missing_runtime_bootstrap_public_key() {
        let conn = in_memory_settings_db();

        let error = get_runtime_bootstrap_public_key(&conn, "entropia-root")
            .expect_err("missing public key should fail");

        assert!(error.contains("entropia-root"));
    }
}
