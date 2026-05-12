#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod watch_service;

use nolost::{
    embeddings::{default_model, resolve_config, EmbeddingConfig},
    extractors::parser_status,
    indexer::{
        collect_files, delete_collection, delete_document, ingest_file, list_collections,
        list_documents, reset_store, search_with_collection, status, SearchResult,
    },
    settings::{list_settings, set_settings, SettingRow},
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    thread,
};
use tauri::{AppHandle, Emitter};

static INGEST_CANCEL: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn ingest_flag() -> Arc<AtomicBool> {
    INGEST_CANCEL
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

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
fn app_search(
    query: String,
    budget: Option<String>,
    limit: Option<usize>,
    collection: Option<String>,
) -> Result<Vec<SearchResult>, String> {
    let budget = budget.unwrap_or_else(|| "normal".to_string());
    search_with_collection(
        &query,
        &budget,
        limit,
        collection.as_deref(),
        &HashMap::new(),
        None,
    )
    .map_err(stringify)
}

#[tauri::command]
fn app_collections() -> Result<Vec<Value>, String> {
    list_collections(None).map_err(stringify)
}

#[tauri::command]
fn app_delete_collection(name: String) -> Result<Value, String> {
    delete_collection(&name, None).map_err(stringify)
}

#[tauri::command]
fn app_add_paths(app: AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let mut files = Vec::new();
    for raw in &paths {
        let resolved = PathBuf::from(raw);
        files.extend(collect_files(&resolved));
    }
    let total = files.len();
    if total == 0 {
        return Err("No supported files in the drop".to_string());
    }
    let handle = app.clone();
    let cancel = ingest_flag();
    cancel.store(false, Ordering::SeqCst);
    thread::spawn(move || {
        let _ = handle.emit("ingest-start", serde_json::json!({ "total": total }));
        let mut completed = 0usize;
        for (index, file) in files.into_iter().enumerate() {
            if cancel.load(Ordering::SeqCst) {
                let _ = handle.emit(
                    "ingest-complete",
                    serde_json::json!({ "total": total, "completed": completed, "cancelled": true }),
                );
                return;
            }
            let outcome = ingest_file(&file, false, &HashMap::new(), None);
            let payload = match outcome {
                Ok(result) => serde_json::json!({
                    "index": index + 1,
                    "total": total,
                    "file": result.file,
                    "status": result.status,
                    "chunks": result.chunks,
                    "error": result.error,
                }),
                Err(error) => serde_json::json!({
                    "index": index + 1,
                    "total": total,
                    "file": file.to_string_lossy(),
                    "status": "error",
                    "chunks": serde_json::Value::Null,
                    "error": error.to_string(),
                }),
            };
            let _ = handle.emit("ingest-progress", payload);
            completed += 1;
        }
        let _ = handle.emit(
            "ingest-complete",
            serde_json::json!({ "total": total, "completed": completed, "cancelled": false }),
        );
    });
    Ok(total)
}

#[tauri::command]
fn app_cancel_ingest() {
    ingest_flag().store(true, Ordering::SeqCst);
}

#[tauri::command]
fn app_reset_library() -> Result<Value, String> {
    reset_store(None).map_err(stringify)
}

#[tauri::command]
fn app_list_documents() -> Result<Vec<Value>, String> {
    list_documents(None).map_err(stringify)
}

#[tauri::command]
fn app_delete_document(id: String) -> Result<Value, String> {
    delete_document(&id, None).map_err(stringify)
}

#[tauri::command]
fn app_watch_folder(app: AppHandle, path: String) -> Result<Vec<String>, String> {
    watch_service::start_watch(&app, PathBuf::from(&path).as_path()).map_err(stringify)?;
    Ok(watch_service::list_watched())
}

#[tauri::command]
fn app_unwatch_folder(app: AppHandle, path: String) -> Result<Vec<String>, String> {
    watch_service::stop_watch(&app, PathBuf::from(&path).as_path()).map_err(stringify)?;
    Ok(watch_service::list_watched())
}

#[tauri::command]
fn app_watched_folders() -> Vec<String> {
    watch_service::list_watched()
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
    let path = nolost::indexer::init_store(None).map_err(stringify)?;
    Ok(path.to_string_lossy().to_string())
}

fn stringify<E: std::fmt::Display>(error: E) -> String {
    format!("{error}")
}

#[tauri::command]
fn app_pick_folder(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    app.dialog().file().pick_folder(move |folder| {
        let path = folder.and_then(|p| match p {
            tauri_plugin_dialog::FilePath::Path(p) => Some(p.to_string_lossy().to_string()),
            tauri_plugin_dialog::FilePath::Url(_) => None,
        });
        let _ = tx.send(path);
    });
    rx.recv().ok().flatten()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            use tauri::{Emitter, Listener, Manager};
            let handle = app.handle().clone();
            if let Some(window) = handle.get_webview_window("main") {
                let emit_handle = handle.clone();
                window.listen("tauri://drag-drop", move |event| {
                    let payload = event.payload();
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
                        if let Some(paths) = value.get("paths").cloned() {
                            let _ = emit_handle.emit("files-dropped", paths);
                        }
                    }
                });
            }
            let resume_handle = handle.clone();
            std::thread::spawn(move || watch_service::resume_all(&resume_handle));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_status,
            app_search,
            app_add_paths,
            app_cancel_ingest,
            app_reset_library,
            app_list_documents,
            app_delete_document,
            app_collections,
            app_delete_collection,
            app_watch_folder,
            app_unwatch_folder,
            app_watched_folders,
            app_pick_folder,
            app_parsers,
            app_embeddings,
            app_set_embedding,
            app_init_store
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nolost");
}
