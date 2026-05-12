use nolost::watch_manager::{self, EventSink, WatchEvent};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tauri::{AppHandle, Emitter};

pub fn list_watched() -> Vec<String> {
    watch_manager::list_watched()
}

pub fn start_watch(app: &AppHandle, path: &Path) -> anyhow::Result<()> {
    let sink = sink_for(app);
    watch_manager::start_watch(path, sink)
}

pub fn stop_watch(app: &AppHandle, path: &Path) -> anyhow::Result<()> {
    let sink = sink_for(app);
    watch_manager::stop_watch(path, sink)
}

pub fn resume_all(app: &AppHandle) {
    let sink = sink_for(app);
    watch_manager::resume_all(sink);
}

fn sink_for(app: &AppHandle) -> EventSink {
    let handle = app.clone();
    Arc::new(move |event: WatchEvent| {
        let (channel, payload) = encode(&event);
        let _ = handle.emit(channel, payload);
    })
}

fn encode(event: &WatchEvent) -> (&'static str, Value) {
    match event {
        WatchEvent::Starting(p) => (
            "watcher-event",
            json!({ "kind": "starting", "folder": p.to_string_lossy() }),
        ),
        WatchEvent::ScanStart { folder, total } => (
            "watcher-event",
            json!({
                "kind": "scan-start",
                "folder": folder.to_string_lossy(),
                "total": total,
            }),
        ),
        WatchEvent::Ingest { folder, source, index, total, result, path } => {
            let (status, chunks, error) = match result {
                Ok(r) => (r.status.clone(), r.chunks.map(|n| n as i64), r.error.clone()),
                Err(e) => ("error".to_string(), None, Some(e.clone())),
            };
            (
                "watcher-ingest",
                json!({
                    "folder": folder.to_string_lossy(),
                    "source": source,
                    "index": index,
                    "total": total,
                    "file": path.to_string_lossy(),
                    "status": status,
                    "chunks": chunks,
                    "error": error,
                }),
            )
        }
        WatchEvent::Removed { folder, path } => (
            "watcher-removed",
            json!({
                "folder": folder.to_string_lossy(),
                "file": path.to_string_lossy(),
            }),
        ),
        WatchEvent::ScanComplete(p) => (
            "watcher-event",
            json!({ "kind": "scan-complete", "folder": p.to_string_lossy() }),
        ),
        WatchEvent::Stopped(p) => (
            "watcher-event",
            json!({ "kind": "stopped", "folder": p.to_string_lossy() }),
        ),
        WatchEvent::Error { folder, message } => (
            "watcher-error",
            json!({ "folder": folder.to_string_lossy(), "error": message }),
        ),
        WatchEvent::StatusChanged { watched, added, removed } => (
            "watcher-status",
            json!({ "watched": watched, "added": added, "removed": removed }),
        ),
    }
}
