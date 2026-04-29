use rusqlite::types::Value;
use serde::Serialize;
use tauri::State;

use crate::db::state::AppDbState;

#[derive(Serialize)]
pub struct ExecuteResult {
    pub rows_affected: u64,
}

/// Execute multiple SQL statements atomically within a transaction.
/// Used for cascade deletes and other multi-statement operations.
#[tauri::command]
pub fn db_execute_batch(db: State<'_, AppDbState>, sql: String) -> Result<(), String> {
    validate_sql_batch(&sql)?;
    let conn = db.ui_conn.lock().map_err(|e| e.to_string())?;
    conn.execute_batch(&sql).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn db_execute(
    db: State<'_, AppDbState>,
    sql: String,
    params: Vec<serde_json::Value>,
) -> Result<ExecuteResult, String> {
    validate_sql_execute(&sql)?;
    let conn = db.ui_conn.lock().map_err(|e| e.to_string())?;
    let params_ref: Vec<Box<dyn rusqlite::ToSql>> = params.iter().map(json_to_sql_param).collect();
    let params_as_refs: Vec<&dyn rusqlite::ToSql> = params_ref.iter().map(|b| b.as_ref()).collect();
    let rows_affected = conn
        .execute(&sql, params_as_refs.as_slice())
        .map_err(|e| e.to_string())?;
    Ok(ExecuteResult {
        rows_affected: rows_affected as u64,
    })
}

#[tauri::command]
pub fn db_select(
    db: State<'_, AppDbState>,
    sql: String,
    params: Vec<serde_json::Value>,
) -> Result<Vec<serde_json::Value>, String> {
    validate_sql_row_query(&sql)?;
    let conn = db.ui_conn.lock().map_err(|e| e.to_string())?;
    let params_ref: Vec<Box<dyn rusqlite::ToSql>> = params.iter().map(json_to_sql_param).collect();
    let params_as_refs: Vec<&dyn rusqlite::ToSql> = params_ref.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let col_count = stmt.column_count();
    let col_names: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("").to_string())
        .collect();

    let rows = stmt
        .query_map(params_as_refs.as_slice(), |row| {
            let mut map = serde_json::Map::new();
            for (i, name) in col_names.iter().enumerate() {
                let val: Value = row.get(i)?;
                map.insert(name.clone(), rusqlite_value_to_json(val));
            }
            Ok(serde_json::Value::Object(map))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

/// Returns rows as arrays in column order — required by Drizzle sqlite-proxy
/// to guarantee correct column mapping (Object.values() order is not guaranteed).
#[tauri::command]
pub fn db_select_rows(
    db: State<'_, AppDbState>,
    sql: String,
    params: Vec<serde_json::Value>,
) -> Result<Vec<Vec<serde_json::Value>>, String> {
    validate_sql_row_query(&sql)?;
    let conn = db.ui_conn.lock().map_err(|e| e.to_string())?;
    let params_ref: Vec<Box<dyn rusqlite::ToSql>> = params.iter().map(json_to_sql_param).collect();
    let params_as_refs: Vec<&dyn rusqlite::ToSql> = params_ref.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let col_count = stmt.column_count();

    let rows = stmt
        .query_map(params_as_refs.as_slice(), |row| {
            let mut values = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let val: Value = row.get(i)?;
                values.push(rusqlite_value_to_json(val));
            }
            Ok(values)
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(rows)
}

fn json_to_sql_param(val: &serde_json::Value) -> Box<dyn rusqlite::ToSql> {
    match val {
        serde_json::Value::Null => Box::new(rusqlite::types::Null),
        serde_json::Value::Bool(b) => Box::new(*b as i64),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(rusqlite::types::Null)
            }
        }
        serde_json::Value::String(s) => Box::new(s.clone()),
        other => Box::new(other.to_string()),
    }
}

fn rusqlite_value_to_json(val: Value) -> serde_json::Value {
    match val {
        Value::Null => serde_json::Value::Null,
        Value::Integer(i) => serde_json::Value::Number(i.into()),
        Value::Real(f) => serde_json::json!(f),
        Value::Text(s) => serde_json::Value::String(s),
        Value::Blob(b) => serde_json::Value::String(base64_encode(&b)),
    }
}

fn base64_encode(data: &[u8]) -> String {
    // Simple base64 without external crate
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 {
            chunk[1] as usize
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            chunk[2] as usize
        } else {
            0
        };
        result.push(CHARS[b0 >> 2] as char);
        result.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((b1 & 15) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[b2 & 63] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn normalize_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn validate_sql_row_query(sql: &str) -> Result<(), String> {
    let normalized = normalize_sql(sql);

    if normalized.contains(';') {
        return Err("db_select/db_select_rows accept only a single SQL statement".to_string());
    }

    for forbidden in ["pragma ", "attach ", "detach ", "vacuum "] {
        if normalized.starts_with(forbidden) || normalized.contains(&format!(" {forbidden}")) {
            return Err("Restricted SQL statement for db_select/db_select_rows".to_string());
        }
    }

    if normalized.starts_with("select ") || normalized.starts_with("with ") {
        return Ok(());
    }

    let is_dml = normalized.starts_with("insert ")
        || normalized.starts_with("update ")
        || normalized.starts_with("delete ");

    if is_dml && normalized.contains(" returning ") {
        return Ok(());
    }

    Err(
        "Only row-returning queries (SELECT/WITH or DML with RETURNING) are allowed in db_select/db_select_rows"
            .to_string(),
    )
}

fn validate_sql_execute(sql: &str) -> Result<(), String> {
    let normalized = normalize_sql(sql);
    if normalized.contains(';') {
        return Err("db_execute accepts only a single SQL statement".to_string());
    }
    if normalized.starts_with("pragma ")
        || normalized.starts_with("attach ")
        || normalized.starts_with("detach ")
        || normalized.starts_with("vacuum ")
    {
        return Err("Restricted SQL statement for db_execute".to_string());
    }
    Ok(())
}

fn validate_sql_batch(sql: &str) -> Result<(), String> {
    let normalized = normalize_sql(sql);
    for forbidden in ["attach ", "detach ", "vacuum ", "pragma "] {
        if normalized.contains(forbidden) {
            return Err("Restricted SQL statement in db_execute_batch".to_string());
        }
    }
    Ok(())
}
