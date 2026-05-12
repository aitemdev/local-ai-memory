use crate::{
    embeddings::{default_model, resolve_config},
    extractors::parser_status,
    indexer::{
        add_path, delete_document, get_chunk, get_document, list_collections, list_documents,
        reset_store, search_memory, status,
    },
    settings::{list_settings, set_settings},
    watch_manager::{self, EventSink, WatchEvent},
};
use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tiny_http::{Header, Method, Request, Response, Server};

#[derive(Deserialize)]
struct AddBody {
    paths: Vec<String>,
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize)]
struct WatchBody {
    path: String,
}

#[derive(Deserialize)]
struct DocumentBody {
    id: String,
}

#[derive(Deserialize)]
struct EmbeddingSetBody {
    provider: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    dimensions: Option<usize>,
}

pub fn serve(port: u16) -> Result<()> {
    let public_dir = std::env::current_dir()?.join("public");
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!("nolost HTTP on http://localhost:{port}");
    for request in server.incoming_requests() {
        if let Err(error) = handle(request, &public_dir) {
            eprintln!("request error: {error:?}");
        }
    }
    Ok(())
}

pub fn serve_daemon(port: u16) -> Result<()> {
    if let Some(existing) = crate::daemon::read_info() {
        return Err(anyhow::anyhow!(
            "daemon already running (pid {} on port {})",
            existing.pid,
            existing.port
        ));
    }
    let info = crate::daemon::DaemonInfo {
        pid: std::process::id(),
        port,
    };
    crate::daemon::write_info(&info)?;
    let _guard = PidGuard;
    let sink: EventSink = Arc::new(|event| log_event(&event));
    watch_manager::resume_all(sink);
    serve(port)
}

struct PidGuard;

impl Drop for PidGuard {
    fn drop(&mut self) {
        crate::daemon::clear_info();
    }
}

fn log_event(event: &WatchEvent) {
    let value = match event {
        WatchEvent::Starting(p) => json!({ "kind": "starting", "folder": p.to_string_lossy() }),
        WatchEvent::ScanStart { folder, total } => json!({
            "kind": "scan-start", "folder": folder.to_string_lossy(), "total": total
        }),
        WatchEvent::Ingest { folder, source, index, total, result, path } => {
            let (status, chunks, error) = match result {
                Ok(r) => (r.status.clone(), r.chunks.map(|n| n as i64), r.error.clone()),
                Err(e) => ("error".to_string(), None, Some(e.clone())),
            };
            json!({
                "kind": "ingest",
                "folder": folder.to_string_lossy(),
                "file": path.to_string_lossy(),
                "source": source,
                "index": index,
                "total": total,
                "status": status,
                "chunks": chunks,
                "error": error,
            })
        }
        WatchEvent::Removed { folder, path } => json!({
            "kind": "removed",
            "folder": folder.to_string_lossy(),
            "file": path.to_string_lossy(),
        }),
        WatchEvent::ScanComplete(p) => json!({ "kind": "scan-complete", "folder": p.to_string_lossy() }),
        WatchEvent::Stopped(p) => json!({ "kind": "stopped", "folder": p.to_string_lossy() }),
        WatchEvent::Error { folder, message } => json!({
            "kind": "error", "folder": folder.to_string_lossy(), "error": message
        }),
        WatchEvent::StatusChanged { watched, added, removed } => json!({
            "kind": "status", "watched": watched, "added": added, "removed": removed
        }),
    };
    eprintln!("{value}");
}

fn handle(mut request: Request, public_dir: &Path) -> Result<()> {
    let url = request.url().to_string();
    let (path, query) = split_url(&url);
    let method = request.method().clone();
    match (&method, path.as_str()) {
        (Method::Get, "/api/status") => json_response(request, &status(None)?),
        (Method::Get, "/api/search") => {
            let params = parse_query(&query);
            let q = params.get("q").cloned().unwrap_or_default();
            let budget = params.get("budget").cloned().unwrap_or_else(|| "normal".to_string());
            let limit = params.get("limit").and_then(|v| v.parse().ok());
            let results = search_memory(&q, &budget, limit, &HashMap::new(), None)?;
            json_response(request, &results)
        }
        (Method::Post, "/api/add") => {
            let body = read_body(&mut request)?;
            let parsed: AddBody = serde_json::from_str(&body)?;
            let mut all = Vec::new();
            for raw in parsed.paths {
                let results = add_path(Path::new(&raw), parsed.force, &HashMap::new(), None)?;
                for r in results {
                    all.push(serde_json::to_value(r)?);
                }
            }
            json_response(request, &all)
        }
        (Method::Post, "/api/reset") => {
            let result = reset_store(None)?;
            json_response(request, &result)
        }
        (Method::Get, "/api/documents") => json_response(request, &list_documents(None)?),
        (Method::Get, "/api/collections") => json_response(request, &list_collections(None)?),
        (Method::Get, p) if p.starts_with("/api/document/") && p != "/api/document/delete" => {
            let id = p.trim_start_matches("/api/document/");
            json_response(request, &get_document(id, None)?)
        }
        (Method::Get, p) if p.starts_with("/api/chunk/") => {
            let id = p.trim_start_matches("/api/chunk/");
            json_response(request, &get_chunk(id, None)?)
        }
        (Method::Post, "/api/document/delete") => {
            let body = read_body(&mut request)?;
            let parsed: DocumentBody = serde_json::from_str(&body)?;
            json_response(request, &delete_document(&parsed.id, None)?)
        }
        (Method::Get, "/api/parsers") => json_response(request, &parser_status()),
        (Method::Get, "/api/embeddings") => {
            let active = resolve_config(None, &HashMap::new(), true)?;
            let settings = list_settings("embedding.", None)?;
            json_response(request, &json!({
                "active": {
                    "provider": active.provider,
                    "model": active.model,
                    "dimensions": active.dimensions,
                    "base_url": active.base_url,
                    "api_key_set": active.api_key.is_some(),
                },
                "settings": settings,
            }))
        }
        (Method::Post, "/api/embeddings/set") => {
            let body = read_body(&mut request)?;
            let parsed: EmbeddingSetBody = serde_json::from_str(&body)?;
            let model = parsed.model.unwrap_or_else(|| default_model(&parsed.provider).to_string());
            let mut values = vec![
                ("embedding.provider", parsed.provider.clone()),
                ("embedding.default_model", model),
                (
                    "embedding.cloud_enabled",
                    if parsed.provider == "local" { "false".to_string() } else { "true".to_string() },
                ),
            ];
            if let Some(url) = parsed.base_url {
                values.push(("embedding.base_url", url));
            }
            if let Some(dims) = parsed.dimensions {
                values.push(("embedding.dimensions", dims.to_string()));
            }
            set_settings(&values, None)?;
            json_response(request, &json!({ "ok": true }))
        }
        (Method::Get, "/api/watched") => {
            json_response(request, &watch_manager::list_watched())
        }
        (Method::Post, "/api/watch") => {
            let body = read_body(&mut request)?;
            let parsed: WatchBody = serde_json::from_str(&body)?;
            let sink: EventSink = Arc::new(|event| log_event(&event));
            watch_manager::start_watch(Path::new(&parsed.path), sink)?;
            json_response(request, &watch_manager::list_watched())
        }
        (Method::Post, "/api/unwatch") => {
            let body = read_body(&mut request)?;
            let parsed: WatchBody = serde_json::from_str(&body)?;
            let sink: EventSink = Arc::new(|event| log_event(&event));
            watch_manager::stop_watch(Path::new(&parsed.path), sink)?;
            json_response(request, &watch_manager::list_watched())
        }
        (Method::Get, _) => serve_static(request, public_dir, &path),
        _ => Ok(request.respond(Response::from_string("Not found").with_status_code(404))?),
    }
}

fn read_body(request: &mut Request) -> Result<String> {
    let mut body = String::new();
    request.as_reader().read_to_string(&mut body)?;
    Ok(body)
}

fn json_response<T: serde::Serialize>(request: Request, value: &T) -> Result<()> {
    let body = serde_json::to_string_pretty(value)?;
    let response = Response::from_string(body)
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
    request.respond(response)?;
    Ok(())
}

fn serve_static(request: Request, public_dir: &Path, path: &str) -> Result<()> {
    let rel = if path == "/" { "index.html" } else { path.trim_start_matches('/') };
    let candidate = public_dir.join(rel);
    let canonical = canonicalize_safely(&candidate, public_dir);
    let Some(file_path) = canonical else {
        return Ok(request.respond(Response::from_string("Not found").with_status_code(404))?);
    };
    if !file_path.is_file() {
        return Ok(request.respond(Response::from_string("Not found").with_status_code(404))?);
    }
    let body = fs::read(&file_path)?;
    let ct = content_type(&file_path);
    let response = Response::from_data(body)
        .with_header(Header::from_bytes(&b"Content-Type"[..], ct.as_bytes()).unwrap());
    request.respond(response)?;
    Ok(())
}

fn canonicalize_safely(candidate: &Path, public_dir: &Path) -> Option<PathBuf> {
    let resolved = candidate.canonicalize().ok()?;
    let root = public_dir.canonicalize().ok()?;
    resolved.starts_with(&root).then_some(resolved)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "text/html; charset=utf-8",
    }
}

fn split_url(url: &str) -> (String, String) {
    match url.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (url.to_string(), String::new()),
    }
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (k.to_string(), url_decode(v)))
        .collect()
}

fn url_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                let byte = u8::from_str_radix(hex, 16).unwrap_or(b'?');
                out.push(byte);
                i += 3;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
