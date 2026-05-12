use local_ai_memory::{
    extractors::supported_extension,
    indexer::{collect_files, ingest_file},
    settings::{get_settings, set_settings},
};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

const DEBOUNCE: Duration = Duration::from_millis(750);
const SETTINGS_KEY: &str = "desktop.watched_folders";

struct WatchHandle {
    stop: Arc<AtomicBool>,
}

type Registry = Arc<Mutex<HashMap<PathBuf, WatchHandle>>>;

fn registry() -> Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(|| Arc::new(Mutex::new(HashMap::new()))).clone()
}

pub fn list_watched() -> Vec<String> {
    let settings = match get_settings(&[SETTINGS_KEY], None) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let raw = settings.get(SETTINGS_KEY).cloned().unwrap_or_default();
    if raw.is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Vec<String>>(&raw).unwrap_or_default()
}

fn save_watched(paths: &[String]) -> anyhow::Result<()> {
    let serialized = serde_json::to_string(paths)?;
    set_settings(&[(SETTINGS_KEY, serialized)], None)?;
    Ok(())
}

pub fn start_watch(app: &AppHandle, path: &Path) -> anyhow::Result<()> {
    let canonical = path.canonicalize()?;
    if !canonical.is_dir() {
        return Err(anyhow::anyhow!("Not a directory: {}", canonical.display()));
    }
    let reg = registry();
    let mut reg = reg.lock().unwrap();
    if reg.contains_key(&canonical) {
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let handle = app.clone();
    let watch_path = canonical.clone();
    thread::spawn(move || {
        if let Err(err) = run_watcher(handle.clone(), watch_path.clone(), stop_clone) {
            let _ = handle.emit(
                "watcher-error",
                json!({ "folder": watch_path.to_string_lossy(), "error": err.to_string() }),
            );
        }
    });
    reg.insert(canonical.clone(), WatchHandle { stop });
    drop(reg);

    let mut all = list_watched();
    let path_str = canonical.to_string_lossy().to_string();
    if !all.contains(&path_str) {
        all.push(path_str.clone());
        save_watched(&all)?;
    }
    let _ = app.emit(
        "watcher-status",
        json!({ "watched": all, "added": path_str }),
    );
    Ok(())
}

pub fn stop_watch(app: &AppHandle, path: &Path) -> anyhow::Result<()> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let reg = registry();
    let mut guard = reg.lock().unwrap();
    if let Some(handle) = guard.remove(&canonical) {
        handle.stop.store(true, Ordering::SeqCst);
    }
    drop(guard);

    let path_str = canonical.to_string_lossy().to_string();
    let mut all = list_watched();
    all.retain(|p| p != &path_str);
    save_watched(&all)?;
    let _ = app.emit(
        "watcher-status",
        json!({ "watched": all, "removed": path_str }),
    );
    Ok(())
}

pub fn resume_all(app: &AppHandle) {
    for path in list_watched() {
        let p = PathBuf::from(&path);
        if let Err(err) = start_watch(app, &p) {
            let _ = app.emit(
                "watcher-error",
                json!({ "folder": path, "error": err.to_string() }),
            );
        }
    }
}

fn run_watcher(app: AppHandle, root: PathBuf, stop: Arc<AtomicBool>) -> anyhow::Result<()> {
    let _ = app.emit(
        "watcher-event",
        json!({ "folder": root.to_string_lossy(), "kind": "starting" }),
    );

    let initial = collect_files(&root);
    let initial_count = initial.len();
    let _ = app.emit(
        "watcher-event",
        json!({
            "folder": root.to_string_lossy(),
            "kind": "scan-start",
            "total": initial_count
        }),
    );
    for (i, file) in initial.into_iter().enumerate() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        emit_ingest(&app, &file, &root, i + 1, initial_count, "scan");
    }
    let _ = app.emit(
        "watcher-event",
        json!({ "folder": root.to_string_lossy(), "kind": "scan-complete" }),
    );

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    while !stop.load(Ordering::SeqCst) {
        let next_deadline = pending.values().min().copied();
        let timeout = match next_deadline {
            Some(deadline) => deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(50)),
            None => Duration::from_millis(500),
        };
        match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => collect_pending(&event, &mut pending),
            Ok(Err(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        flush_due(&app, &root, &mut pending);
    }

    let _ = app.emit(
        "watcher-event",
        json!({ "folder": root.to_string_lossy(), "kind": "stopped" }),
    );
    Ok(())
}

fn collect_pending(event: &Event, pending: &mut HashMap<PathBuf, Instant>) {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return;
    }
    let deadline = Instant::now() + DEBOUNCE;
    let mut seen = HashSet::new();
    for path in &event.paths {
        if !path.is_file() {
            continue;
        }
        if !supported_extension(path) {
            continue;
        }
        if seen.insert(path.clone()) {
            pending.insert(path.clone(), deadline);
        }
    }
}

fn flush_due(app: &AppHandle, root: &Path, pending: &mut HashMap<PathBuf, Instant>) {
    let now = Instant::now();
    let due: Vec<PathBuf> = pending
        .iter()
        .filter(|(_, deadline)| **deadline <= now)
        .map(|(p, _)| p.clone())
        .collect();
    for path in due {
        pending.remove(&path);
        emit_ingest(app, &path, root, 0, 0, "live");
    }
}

fn emit_ingest(
    app: &AppHandle,
    path: &Path,
    root: &Path,
    index: usize,
    total: usize,
    source: &str,
) {
    let outcome = ingest_file(path, false, &Default::default(), None);
    let payload: Value = match outcome {
        Ok(result) => json!({
            "folder": root.to_string_lossy(),
            "source": source,
            "index": index,
            "total": total,
            "file": result.file,
            "status": result.status,
            "chunks": result.chunks,
            "error": result.error,
        }),
        Err(error) => json!({
            "folder": root.to_string_lossy(),
            "source": source,
            "index": index,
            "total": total,
            "file": path.to_string_lossy(),
            "status": "error",
            "chunks": Value::Null,
            "error": error.to_string(),
        }),
    };
    let _ = app.emit("watcher-ingest", payload);
}
