use crate::indexer::{add_path, search_memory, status};
use anyhow::Result;
use serde::Deserialize;
use std::{collections::HashMap, fs, path::{Path, PathBuf}};
use tiny_http::{Header, Method, Request, Response, Server};

#[derive(Deserialize)]
struct AddBody {
    path: String,
    #[serde(default)]
    force: bool,
}

pub fn serve(port: u16) -> Result<()> {
    let public_dir = std::env::current_dir()?.join("public");
    let server = Server::http(("127.0.0.1", port)).map_err(|e| anyhow::anyhow!("{e}"))?;
    eprintln!("Local AI Memory UI: http://localhost:{port}");
    for request in server.incoming_requests() {
        if let Err(error) = handle(request, &public_dir) {
            eprintln!("request error: {error:?}");
        }
    }
    Ok(())
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
            let results = search_memory(&q, &budget, None, &HashMap::new(), None)?;
            json_response(request, &results)
        }
        (Method::Post, "/api/add") => {
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body)?;
            let parsed: AddBody = serde_json::from_str(&body)?;
            let results = add_path(Path::new(&parsed.path), parsed.force, &HashMap::new(), None)?;
            json_response(request, &results)
        }
        (Method::Get, _) => serve_static(request, public_dir, &path),
        _ => Ok(request.respond(Response::from_string("Not found").with_status_code(404))?),
    }
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
