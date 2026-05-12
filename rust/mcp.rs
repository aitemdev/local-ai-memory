use crate::{
    client,
    indexer::{get_chunk, get_document, list_collections, search_memory},
};
use anyhow::Result;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
};

pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = handle_line(&line);
        if let Some(response) = response {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn handle_line(line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => {
            return Some(error_response(Value::Null, -32700, &error.to_string()));
        }
    };
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let is_notification = message.get("id").is_none();
    match handle_request(&message) {
        Ok(result) => {
            if is_notification {
                None
            } else {
                Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
            }
        }
        Err(error) => Some(error_response(id, -32000, &error.to_string())),
    }
}

fn handle_request(message: &Value) -> Result<Value> {
    let method = message.get("method").and_then(|m| m.as_str()).unwrap_or("");
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "nolost", "version": env!("CARGO_PKG_VERSION") }
        })),
        "tools/list" => Ok(json!({ "tools": tool_descriptors() })),
        "tools/call" => handle_tool_call(message.get("params").cloned().unwrap_or(Value::Null)),
        _ => Ok(Value::Object(Default::default())),
    }
}

fn handle_tool_call(params: Value) -> Result<Value> {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let via_daemon = client::endpoint().is_some();
    let payload = match name {
        "search_memory" => {
            let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
            let budget = args.get("budget").and_then(|b| b.as_str()).unwrap_or("low");
            let limit = args.get("limit").and_then(|l| l.as_u64()).map(|n| n as usize);
            if via_daemon {
                let mut path = format!(
                    "/api/search?q={}&budget={}",
                    client::url_encode(query),
                    client::url_encode(budget)
                );
                if let Some(n) = limit {
                    path.push_str(&format!("&limit={n}"));
                }
                client::get(&path)?
            } else {
                serde_json::to_value(search_memory(query, budget, limit, &HashMap::new(), None)?)?
            }
        }
        "get_document" => {
            let id = args.get("document_id").and_then(|i| i.as_str()).unwrap_or("");
            if via_daemon {
                client::get(&format!("/api/document/{}", client::url_encode(id)))?
            } else {
                get_document(id, None)?
            }
        }
        "get_chunk" => {
            let id = args.get("chunk_id").and_then(|i| i.as_str()).unwrap_or("");
            if via_daemon {
                client::get(&format!("/api/chunk/{}", client::url_encode(id)))?
            } else {
                get_chunk(id, None)?
            }
        }
        "list_collections" => {
            if via_daemon {
                client::get("/api/collections")?
            } else {
                json!(list_collections(None)?)
            }
        }
        other => return Err(anyhow::anyhow!("Unknown tool: {other}")),
    };
    Ok(json!({
        "content": [
            { "type": "text", "text": serde_json::to_string_pretty(&payload)? }
        ]
    }))
}

fn tool_descriptors() -> Value {
    json!([
        {
            "name": "search_memory",
            "description": "Search local indexed memory and return grounded chunks with citations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "budget": { "type": "string", "enum": ["low", "normal", "wide"] },
                    "limit": { "type": "number" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "get_document",
            "description": "Return metadata for a local document.",
            "inputSchema": {
                "type": "object",
                "properties": { "document_id": { "type": "string" } },
                "required": ["document_id"]
            }
        },
        {
            "name": "get_chunk",
            "description": "Return exact chunk text and source metadata.",
            "inputSchema": {
                "type": "object",
                "properties": { "chunk_id": { "type": "string" } },
                "required": ["chunk_id"]
            }
        },
        {
            "name": "list_collections",
            "description": "List local memory collections.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
