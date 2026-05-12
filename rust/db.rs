use crate::paths::{DataPaths, data_paths};
use anyhow::Result;
use rusqlite::Connection;
use std::{fs, path::PathBuf, time::Duration};

pub fn ensure_store(base: Option<PathBuf>) -> Result<(Connection, DataPaths)> {
    let paths = data_paths(base);
    fs::create_dir_all(&paths.base)?;
    fs::create_dir_all(&paths.canonical)?;
    fs::create_dir_all(&paths.originals)?;
    fs::create_dir_all(&paths.logs)?;
    let conn = Connection::open(&paths.db)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    migrate(&conn)?;
    Ok((conn, paths))
}

pub fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 5000;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS settings (
          key TEXT PRIMARY KEY,
          value TEXT NOT NULL,
          updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS collections (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          path TEXT,
          kind TEXT NOT NULL DEFAULT 'manual',
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS documents (
          id TEXT PRIMARY KEY,
          path TEXT NOT NULL UNIQUE,
          hash TEXT NOT NULL,
          type TEXT NOT NULL,
          title TEXT NOT NULL,
          status TEXT NOT NULL,
          error TEXT,
          canonical_md_path TEXT,
          canonical_json_path TEXT,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS document_versions (
          id TEXT PRIMARY KEY,
          document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
          hash TEXT NOT NULL,
          parser TEXT NOT NULL,
          extraction_model TEXT,
          error TEXT,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS chunks (
          id TEXT PRIMARY KEY,
          document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
          ordinal INTEGER NOT NULL,
          text TEXT NOT NULL,
          heading TEXT,
          page INTEGER,
          slide INTEGER,
          token_count INTEGER NOT NULL,
          hash TEXT NOT NULL,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
          UNIQUE(document_id, hash)
        );

        CREATE TABLE IF NOT EXISTS embeddings (
          chunk_id TEXT PRIMARY KEY REFERENCES chunks(id) ON DELETE CASCADE,
          model TEXT NOT NULL,
          dimensions INTEGER NOT NULL,
          vector_json TEXT NOT NULL,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS jobs (
          id TEXT PRIMARY KEY,
          type TEXT NOT NULL,
          subject TEXT NOT NULL,
          status TEXT NOT NULL,
          progress INTEGER NOT NULL DEFAULT 0,
          error TEXT,
          created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
          updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
          chunk_id UNINDEXED,
          document_id UNINDEXED,
          title UNINDEXED,
          text
        );
        "#,
    )?;

    let defaults = [
        ("embedding.provider", "local"),
        ("embedding.default_model", "local-hash-v1"),
        ("embedding.dimensions", ""),
        ("embedding.base_url", ""),
        ("embedding.cloud_enabled", "false"),
        ("privacy.telemetry_opt_in", "false"),
        ("search.default_budget", "normal"),
    ];
    for (key, value) in defaults {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
            (key, value),
        )?;
    }
    Ok(())
}
