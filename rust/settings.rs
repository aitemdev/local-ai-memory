use crate::db::ensure_store;
use anyhow::Result;
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Serialize)]
pub struct SettingRow {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

pub fn get_settings(keys: &[&str], base: Option<PathBuf>) -> Result<HashMap<String, String>> {
    let (conn, _) = ensure_store(base)?;
    let mut map = HashMap::new();
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    for key in keys {
        let value: Option<String> = stmt.query_row([key], |row| row.get(0)).ok();
        if let Some(value) = value {
            map.insert((*key).to_string(), value);
        }
    }
    Ok(map)
}

pub fn set_settings(values: &[(&str, String)], base: Option<PathBuf>) -> Result<()> {
    let (conn, _) = ensure_store(base)?;
    for (key, value) in values {
        conn.execute(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP
            "#,
            (*key, value),
        )?;
    }
    Ok(())
}

pub fn list_settings(prefix: &str, base: Option<PathBuf>) -> Result<Vec<SettingRow>> {
    let (conn, _) = ensure_store(base)?;
    let mut stmt = conn.prepare(
        "SELECT key, value, updated_at FROM settings WHERE key LIKE ?1 ORDER BY key",
    )?;
    let rows = stmt
        .query_map([format!("{prefix}%")], |row| {
            Ok(SettingRow {
                key: row.get(0)?,
                value: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
