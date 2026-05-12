#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use local_ai_memory::{
    embeddings::{default_model, resolve_config, EmbeddingConfig},
    extractors::parser_status,
    indexer::{add_path, search_memory, status, SearchResult},
    settings::{list_settings, set_settings, SettingRow},
};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf};

#[derive(Serialize)]
struct EmbeddingsView {
    active: EmbeddingActive,
    settings: Vec<SettingRow>,
}

#[derive(Serialize)]
struct EmbeddingActive {
    provider: String,
    model: String,
    dimensions: Option<usize>,
    base_url: String,
    api_key_set: bool,
}

impl From<EmbeddingConfig> for EmbeddingActive {
    fn from(config: EmbeddingConfig) -> Self {
        Self {
            provider: config.provider,
            model: config.model,
            dimensions: config.dimensions,
            base_url: config.base_url,
            api_key_set: config.api_key.is_some(),
        }
    }
}

#[tauri::command]
fn app_status() -> Result<Value, String> {
    status(None).map_err(stringify)
}

#[tauri::command]
fn app_search(query: String, budget: Option<String>, limit: Option<usize>) -> Result<Vec<SearchResult>, String> {
    let budget = budget.unwrap_or_else(|| "normal".to_string());
    search_memory(&query, &budget, limit, &HashMap::new(), None).map_err(stringify)
}

#[tauri::command]
fn app_add_paths(paths: Vec<String>) -> Result<Vec<Value>, String> {
    let mut all = Vec::new();
    for raw in paths {
        let path = PathBuf::from(raw);
        let results = add_path(&path, false, &HashMap::new(), None).map_err(stringify)?;
        for result in results {
            all.push(serde_json::to_value(result).map_err(stringify)?);
        }
    }
    Ok(all)
}

#[tauri::command]
fn app_parsers() -> Value {
    parser_status()
}

#[tauri::command]
fn app_embeddings() -> Result<EmbeddingsView, String> {
    let active = resolve_config(None, &HashMap::new(), true).map_err(stringify)?;
    let settings = list_settings("embedding.", None).map_err(stringify)?;
    Ok(EmbeddingsView { active: active.into(), settings })
}

#[tauri::command]
fn app_set_embedding(
    provider: String,
    model: Option<String>,
    base_url: Option<String>,
    dimensions: Option<usize>,
) -> Result<EmbeddingsView, String> {
    let resolved_model = model.unwrap_or_else(|| default_model(&provider).to_string());
    let mut values = vec![
        ("embedding.provider", provider.clone()),
        ("embedding.default_model", resolved_model),
        (
            "embedding.cloud_enabled",
            if provider == "local" { "false".to_string() } else { "true".to_string() },
        ),
    ];
    if let Some(value) = base_url {
        values.push(("embedding.base_url", value));
    }
    if let Some(value) = dimensions {
        values.push(("embedding.dimensions", value.to_string()));
    }
    set_settings(&values, None).map_err(stringify)?;
    app_embeddings()
}

#[tauri::command]
fn app_init_store() -> Result<String, String> {
    let path = local_ai_memory::indexer::init_store(None).map_err(stringify)?;
    Ok(path.to_string_lossy().to_string())
}

fn stringify<E: std::fmt::Display>(error: E) -> String {
    format!("{error}")
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            app_status,
            app_search,
            app_add_paths,
            app_parsers,
            app_embeddings,
            app_set_embedding,
            app_init_store
        ])
        .run(tauri::generate_context!())
        .expect("error while running Local AI Memory");
}
