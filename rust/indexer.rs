use crate::{
    chunker::{Chunk, chunk_markdown},
    db::ensure_store,
    embeddings::{cosine_similarity, embed_text, embedding_key},
    extractors::{ExtractedDocument, extract_document, supported_extension},
    hash::{hash_file, hash_text},
    reranker::{apply_budget, rerank},
};
use anyhow::Result;
use rusqlite::{Connection, params};
use serde::Serialize;
use std::{collections::HashMap, fs, path::{Path, PathBuf}};
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct IngestResult {
    pub file: String,
    pub document_id: String,
    pub status: String,
    pub chunks: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub document_id: String,
    pub title: String,
    pub path: String,
    pub text: String,
    pub heading: Option<String>,
    pub page: Option<i64>,
    pub slide: Option<i64>,
    pub token_count: usize,
    pub fts_score: f32,
    pub vector_score: f32,
    pub score: f32,
    pub score_breakdown: serde_json::Value,
    pub citation: String,
}

pub fn init_store(base: Option<PathBuf>) -> Result<PathBuf> {
    let (_, paths) = ensure_store(base)?;
    Ok(paths.base)
}

pub fn add_path(input: &Path, force: bool, overrides: &HashMap<String, String>, base: Option<PathBuf>) -> Result<Vec<IngestResult>> {
    let files = if input.is_dir() {
        WalkDir::new(input)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file() && supported_extension(entry.path()))
            .map(|entry| entry.path().to_path_buf())
            .collect()
    } else {
        vec![input.to_path_buf()]
    };
    files.into_iter().map(|file| ingest_file(&file, force, overrides, base.clone())).collect()
}

pub fn ingest_file(file: &Path, force: bool, overrides: &HashMap<String, String>, base: Option<PathBuf>) -> Result<IngestResult> {
    let (conn, paths) = ensure_store(base.clone())?;
    let resolved = fs::canonicalize(file)?;
    let path_string = resolved.to_string_lossy().to_string();
    let document_id = hash_text(&path_string)[..24].to_string();
    let hash = hash_file(&resolved)?;
    let title = resolved.file_name().and_then(|n| n.to_str()).unwrap_or("document").to_string();
    let doc_type = resolved.extension().and_then(|e| e.to_str()).unwrap_or("unknown").to_string();

    if !force {
        let existing: Option<(String, String)> = conn
            .query_row("SELECT hash, status FROM documents WHERE path = ?1", [&path_string], |row| Ok((row.get(0)?, row.get(1)?)))
            .ok();
        if matches!(existing, Some((ref h, ref s)) if h == &hash && s == "ready") {
            return Ok(IngestResult { file: path_string, document_id, status: "unchanged".to_string(), chunks: None, error: None });
        }
    }

    match extract_document(&resolved).and_then(|extracted| index_extracted(&conn, &paths.canonical, &resolved, &document_id, &hash, &title, &doc_type, extracted, overrides, base)) {
        Ok(chunks) => Ok(IngestResult { file: path_string, document_id, status: "ready".to_string(), chunks: Some(chunks), error: None }),
        Err(error) => {
            conn.execute(
                r#"
                INSERT INTO documents (id, path, hash, type, title, status, error, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, 'error', ?6, CURRENT_TIMESTAMP)
                ON CONFLICT(path) DO UPDATE SET hash = excluded.hash, status = 'error', error = excluded.error, updated_at = CURRENT_TIMESTAMP
                "#,
                params![document_id, path_string, hash, doc_type, title, error.to_string()],
            )?;
            Ok(IngestResult { file: path_string, document_id, status: "error".to_string(), chunks: None, error: Some(error.to_string()) })
        }
    }
}

pub fn search_memory(query: &str, budget: &str, limit: Option<usize>, overrides: &HashMap<String, String>, base: Option<PathBuf>) -> Result<Vec<SearchResult>> {
    let (conn, _) = ensure_store(base.clone())?;
    let candidate_limit = limit.unwrap_or(10).max(40) * 4;
    let fts = fts_search(&conn, query, candidate_limit)?;
    let vector = vector_search(&conn, query, candidate_limit, overrides, base)?;
    let mut merged: HashMap<String, SearchResult> = HashMap::new();
    for row in fts {
        merged.insert(row.chunk_id.clone(), row);
    }
    for row in vector {
        merged.entry(row.chunk_id.clone()).and_modify(|existing| existing.vector_score = row.vector_score).or_insert(row);
    }
    Ok(apply_budget(rerank(query, merged.into_values().collect()), budget, limit))
}

pub fn status(base: Option<PathBuf>) -> Result<serde_json::Value> {
    let (conn, _) = ensure_store(base)?;
    Ok(serde_json::json!({
        "documents": count_by_status(&conn, "documents")?,
        "jobs": count_by_status(&conn, "jobs")?
    }))
}

pub fn get_document(document_id: &str, base: Option<PathBuf>) -> Result<serde_json::Value> {
    let (conn, _) = ensure_store(base)?;
    let row = conn.query_row(
        "SELECT id, path, hash, type, title, status, error, canonical_md_path, canonical_json_path, created_at, updated_at FROM documents WHERE id = ?1",
        [document_id],
        |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "path": row.get::<_, String>(1)?,
                "hash": row.get::<_, String>(2)?,
                "type": row.get::<_, String>(3)?,
                "title": row.get::<_, String>(4)?,
                "status": row.get::<_, String>(5)?,
                "error": row.get::<_, Option<String>>(6)?,
                "canonical_md_path": row.get::<_, Option<String>>(7)?,
                "canonical_json_path": row.get::<_, Option<String>>(8)?,
                "created_at": row.get::<_, String>(9)?,
                "updated_at": row.get::<_, String>(10)?,
            }))
        },
    ).ok();
    Ok(row.unwrap_or(serde_json::Value::Null))
}

pub fn get_chunk(chunk_id: &str, base: Option<PathBuf>) -> Result<serde_json::Value> {
    let (conn, _) = ensure_store(base)?;
    let row = conn.query_row(
        r#"
        SELECT c.id, c.document_id, c.ordinal, c.text, c.heading, c.page, c.slide, c.token_count, c.hash, d.title, d.path
        FROM chunks c JOIN documents d ON d.id = c.document_id
        WHERE c.id = ?1
        "#,
        [chunk_id],
        |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "document_id": row.get::<_, String>(1)?,
                "ordinal": row.get::<_, i64>(2)?,
                "text": row.get::<_, String>(3)?,
                "heading": row.get::<_, Option<String>>(4)?,
                "page": row.get::<_, Option<i64>>(5)?,
                "slide": row.get::<_, Option<i64>>(6)?,
                "token_count": row.get::<_, i64>(7)?,
                "hash": row.get::<_, String>(8)?,
                "title": row.get::<_, String>(9)?,
                "path": row.get::<_, String>(10)?,
            }))
        },
    ).ok();
    Ok(row.unwrap_or(serde_json::Value::Null))
}

pub fn list_collections(base: Option<PathBuf>) -> Result<Vec<serde_json::Value>> {
    let (conn, _) = ensure_store(base)?;
    let mut stmt = conn.prepare("SELECT id, name, path, kind, created_at FROM collections ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "name": row.get::<_, String>(1)?,
            "path": row.get::<_, Option<String>>(2)?,
            "kind": row.get::<_, String>(3)?,
            "created_at": row.get::<_, String>(4)?,
        }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn index_extracted(
    conn: &Connection,
    canonical_dir: &Path,
    resolved: &Path,
    document_id: &str,
    hash: &str,
    title: &str,
    doc_type: &str,
    extracted: ExtractedDocument,
    overrides: &HashMap<String, String>,
    base: Option<PathBuf>,
) -> Result<usize> {
    let canonical_md = canonical_dir.join(format!("{document_id}.md"));
    let canonical_json = canonical_dir.join(format!("{document_id}.json"));
    fs::write(&canonical_md, &extracted.markdown)?;
    fs::write(&canonical_json, serde_json::to_string_pretty(&extracted.structured)?)?;
    let path_string = resolved.to_string_lossy();

    conn.execute(
        r#"
        INSERT INTO documents (id, path, hash, type, title, status, error, canonical_md_path, canonical_json_path, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, 'indexing', NULL, ?6, ?7, CURRENT_TIMESTAMP)
        ON CONFLICT(path) DO UPDATE SET hash = excluded.hash, type = excluded.type, title = excluded.title, status = 'indexing',
        error = NULL, canonical_md_path = excluded.canonical_md_path, canonical_json_path = excluded.canonical_json_path, updated_at = CURRENT_TIMESTAMP
        "#,
        params![document_id, path_string.as_ref(), hash, doc_type, extracted.title, canonical_md.to_string_lossy(), canonical_json.to_string_lossy()],
    )?;
    conn.execute("DELETE FROM chunks_fts WHERE document_id = ?1", [document_id])?;
    conn.execute("DELETE FROM chunks WHERE document_id = ?1", [document_id])?;

    let chunks = chunks_from_extraction(&extracted);
    for chunk in &chunks {
        let chunk_id = &hash_text(&format!("{document_id}:{}", chunk.hash))[..32];
        conn.execute(
            "INSERT OR IGNORE INTO chunks (id, document_id, ordinal, text, heading, page, slide, token_count, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![chunk_id, document_id, chunk.ordinal as i64, chunk.text, chunk.heading, chunk.page, chunk.slide, chunk.token_count as i64, chunk.hash],
        )?;
        conn.execute(
            "INSERT INTO chunks_fts (chunk_id, document_id, title, text) VALUES (?1, ?2, ?3, ?4)",
            params![chunk_id, document_id, title, chunk.text],
        )?;
        let embedding = embed_text(&chunk.text, base.clone(), overrides)?;
        conn.execute(
            "INSERT OR REPLACE INTO embeddings (chunk_id, model, dimensions, vector_json) VALUES (?1, ?2, ?3, ?4)",
            params![chunk_id, embedding_key(&embedding), embedding.dimensions as i64, serde_json::to_string(&embedding.vector)?],
        )?;
    }
    conn.execute("UPDATE documents SET status = 'ready', error = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?1", [document_id])?;
    Ok(chunks.len())
}

fn chunks_from_extraction(extracted: &ExtractedDocument) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut ordinal = 0usize;
    for section in &extracted.structured.sections {
        let text = section.text.clone().unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        for mut chunk in chunk_markdown(&text) {
            if is_heading_only(&chunk.text) {
                continue;
            }
            chunk.ordinal = ordinal;
            chunk.heading = section.heading.clone().or(chunk.heading);
            chunk.page = section.page;
            chunk.slide = section.slide;
            ordinal += 1;
            chunks.push(chunk);
        }
    }
    if chunks.is_empty() {
        chunks = chunk_markdown(&extracted.markdown);
    }
    chunks
}

fn fts_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
    let fts_query = query
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" OR ");
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        r#"
        SELECT c.id, c.document_id, c.text, c.heading, c.page, c.slide, c.token_count, d.title, d.path, -bm25(chunks_fts) AS score
        FROM chunks_fts
        JOIN chunks c ON c.id = chunks_fts.chunk_id
        JOIN documents d ON d.id = c.document_id
        WHERE chunks_fts MATCH ?1
        ORDER BY bm25(chunks_fts)
        LIMIT ?2
        "#,
    )?;
    let rows = stmt.query_map(params![fts_query, limit as i64], row_to_search_result)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn vector_search(conn: &Connection, query: &str, limit: usize, overrides: &HashMap<String, String>, base: Option<PathBuf>) -> Result<Vec<SearchResult>> {
    let embedding = embed_text(query, base, overrides)?;
    let key = embedding_key(&embedding);
    let mut stmt = conn.prepare(
        r#"
        SELECT c.id, c.document_id, c.text, c.heading, c.page, c.slide, c.token_count, d.title, d.path, e.vector_json
        FROM embeddings e
        JOIN chunks c ON c.id = e.chunk_id
        JOIN documents d ON d.id = c.document_id
        WHERE e.model = ?1
        "#,
    )?;
    let mut rows = stmt
        .query_map([key], |row| {
            let vector_json: String = row.get(9)?;
            let vector: Vec<f32> = serde_json::from_str(&vector_json).unwrap_or_default();
            let mut result = row_to_search_result(row)?;
            result.vector_score = cosine_similarity(&embedding.vector, &vector);
            Ok(result)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.sort_by(|a, b| b.vector_score.partial_cmp(&a.vector_score).unwrap());
    rows.truncate(limit);
    Ok(rows)
}

fn row_to_search_result(row: &rusqlite::Row) -> rusqlite::Result<SearchResult> {
    let title: String = row.get(7)?;
    let heading: Option<String> = row.get(3)?;
    let page: Option<i64> = row.get(4)?;
    let slide: Option<i64> = row.get(5)?;
    let citation = format!("{} ({})", title, page.map(|p| format!("page {p}")).or_else(|| slide.map(|s| format!("slide {s}"))).or_else(|| heading.clone()).unwrap_or_else(|| "document".to_string()));
    Ok(SearchResult {
        chunk_id: row.get(0)?,
        document_id: row.get(1)?,
        text: row.get(2)?,
        heading,
        page,
        slide,
        token_count: row.get::<_, i64>(6)? as usize,
        title,
        path: row.get(8)?,
        fts_score: row.get::<_, f32>(9).unwrap_or(0.0),
        vector_score: 0.0,
        score: 0.0,
        score_breakdown: serde_json::json!({}),
        citation,
    })
}

fn count_by_status(conn: &Connection, table: &str) -> Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(&format!("SELECT status, COUNT(*) FROM {table} GROUP BY status"))?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({ "status": row.get::<_, String>(0)?, "count": row.get::<_, i64>(1)? }))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn is_heading_only(text: &str) -> bool {
    let lines: Vec<_> = text.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    lines.len() == 1 && lines[0].starts_with('#')
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn ingests_markdown_and_finds_it() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join(".memoria");
        let file = write_file(
            dir.path(),
            "strategy.md",
            "# Strategy\n\nEnterprise pricing uses annual contracts and renewal notices.",
        );
        init_store(Some(base.clone())).unwrap();
        let overrides = HashMap::new();
        let results = add_path(&file, false, &overrides, Some(base.clone())).unwrap();
        assert_eq!(results[0].status, "ready");

        let hits = search_memory("enterprise pricing", "low", None, &overrides, Some(base.clone())).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].text.to_lowercase().contains("enterprise pricing"));

        let state = status(Some(base)).unwrap();
        let docs = state.get("documents").unwrap().as_array().unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].get("status").unwrap(), "ready");
        assert_eq!(docs[0].get("count").unwrap(), 1);
    }

    #[test]
    fn reranks_exact_lexical_above_nearby_semantic() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join(".memoria");
        write_file(
            dir.path(),
            "pricing.md",
            "# Enterprise Pricing\n\nEnterprise pricing uses renewal notices for seat tiers.",
        );
        write_file(
            dir.path(),
            "sales.md",
            "# Sales Motion\n\nCustomer contracts include annual plans and account expansion.",
        );
        init_store(Some(base.clone())).unwrap();
        let overrides = HashMap::new();
        add_path(dir.path(), false, &overrides, Some(base.clone())).unwrap();

        let hits = search_memory("enterprise pricing renewal", "low", None, &overrides, Some(base)).unwrap();
        assert!(hits.len() >= 2);
        assert_eq!(hits[0].title, "pricing.md");
        assert!(hits[0].score >= hits[1].score);
    }
}
