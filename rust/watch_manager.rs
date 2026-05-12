use crate::{
    extractors::supported_extension,
    indexer::{collect_files, ingest_file, IngestResult},
    settings::{get_settings, set_settings},
};
use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
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

const DEBOUNCE: Duration = Duration::from_millis(750);
const SETTINGS_KEY: &str = "watch.folders";

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Starting(PathBuf),
    ScanStart { folder: PathBuf, total: usize },
    Ingest {
        folder: PathBuf,
        source: &'static str,
        index: usize,
        total: usize,
        result: Result<IngestResult, String>,
        path: PathBuf,
    },
    ScanComplete(PathBuf),
    Stopped(PathBuf),
    Error { folder: PathBuf, message: String },
    StatusChanged { watched: Vec<String>, added: Option<String>, removed: Option<String> },
}

pub type EventSink = Arc<dyn Fn(WatchEvent) + Send + Sync + 'static>;

struct Handle {
    stop: Arc<AtomicBool>,
}

type Registry = Arc<Mutex<HashMap<PathBuf, Handle>>>;

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

fn save_watched(paths: &[String]) -> Result<()> {
    let serialized = serde_json::to_string(paths)?;
    set_settings(&[(SETTINGS_KEY, serialized)], None)?;
    Ok(())
}

pub fn start_watch(path: &Path, sink: EventSink) -> Result<()> {
    let canonical = path.canonicalize()?;
    if !canonical.is_dir() {
        return Err(anyhow::anyhow!("Not a directory: {}", canonical.display()));
    }
    let reg = registry();
    let mut guard = reg.lock().unwrap();
    if guard.contains_key(&canonical) {
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let sink_clone = sink.clone();
    let path_clone = canonical.clone();
    thread::spawn(move || {
        if let Err(err) = run_watcher(path_clone.clone(), stop_clone, sink_clone.clone()) {
            sink_clone(WatchEvent::Error {
                folder: path_clone,
                message: err.to_string(),
            });
        }
    });
    guard.insert(canonical.clone(), Handle { stop });
    drop(guard);

    let mut all = list_watched();
    let path_str = canonical.to_string_lossy().to_string();
    if !all.contains(&path_str) {
        all.push(path_str.clone());
        save_watched(&all)?;
    }
    sink(WatchEvent::StatusChanged { watched: all, added: Some(path_str), removed: None });
    Ok(())
}

pub fn stop_watch(path: &Path, sink: EventSink) -> Result<()> {
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
    sink(WatchEvent::StatusChanged { watched: all, added: None, removed: Some(path_str) });
    Ok(())
}

pub fn resume_all(sink: EventSink) {
    for path in list_watched() {
        let p = PathBuf::from(&path);
        if let Err(err) = start_watch(&p, sink.clone()) {
            sink(WatchEvent::Error { folder: p, message: err.to_string() });
        }
    }
}

fn run_watcher(root: PathBuf, stop: Arc<AtomicBool>, sink: EventSink) -> Result<()> {
    sink(WatchEvent::Starting(root.clone()));

    let initial = collect_files(&root);
    let total = initial.len();
    sink(WatchEvent::ScanStart { folder: root.clone(), total });
    for (i, file) in initial.into_iter().enumerate() {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        run_ingest(&root, &file, i + 1, total, "scan", &sink);
    }
    sink(WatchEvent::ScanComplete(root.clone()));

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
        flush_due(&root, &mut pending, &sink);
    }

    sink(WatchEvent::Stopped(root));
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

fn flush_due(root: &Path, pending: &mut HashMap<PathBuf, Instant>, sink: &EventSink) {
    let now = Instant::now();
    let due: Vec<PathBuf> = pending
        .iter()
        .filter(|(_, deadline)| **deadline <= now)
        .map(|(p, _)| p.clone())
        .collect();
    for path in due {
        pending.remove(&path);
        run_ingest(root, &path, 0, 0, "live", sink);
    }
}

fn run_ingest(
    root: &Path,
    path: &Path,
    index: usize,
    total: usize,
    source: &'static str,
    sink: &EventSink,
) {
    let outcome = ingest_file(path, false, &HashMap::new(), None).map_err(|e| e.to_string());
    sink(WatchEvent::Ingest {
        folder: root.to_path_buf(),
        source,
        index,
        total,
        result: outcome,
        path: path.to_path_buf(),
    });
}
