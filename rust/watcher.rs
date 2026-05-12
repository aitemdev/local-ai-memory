use crate::{
    extractors::supported_extension,
    indexer::ingest_file,
};
use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

const DEBOUNCE: Duration = Duration::from_millis(750);

pub fn watch(root: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(anyhow::anyhow!("Watch target must be a directory: {}", root.display()));
    }
    eprintln!("Watching {} (Ctrl-C to stop)", root.display());

    initial_scan(&root)?;

    let (tx, rx) = mpsc::channel();
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |event| {
        let _ = tx.send(event);
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    loop {
        let next_deadline = pending.values().min().copied();
        let timeout = match next_deadline {
            Some(deadline) => deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(50)),
            None => Duration::from_secs(60),
        };
        match rx.recv_timeout(timeout) {
            Ok(Ok(event)) => collect_pending(&event, &mut pending),
            Ok(Err(error)) => eprintln!("watch error: {error:?}"),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        flush_due(&mut pending);
    }
    Ok(())
}

fn initial_scan(root: &Path) -> Result<()> {
    let mut count = 0usize;
    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if entry.file_type().is_file() && supported_extension(path) {
            reindex(path);
            count += 1;
        }
    }
    eprintln!("Initial scan complete ({count} files).");
    Ok(())
}

fn collect_pending(event: &Event, pending: &mut HashMap<PathBuf, Instant>) {
    if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) {
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

fn flush_due(pending: &mut HashMap<PathBuf, Instant>) {
    let now = Instant::now();
    let due: Vec<PathBuf> = pending
        .iter()
        .filter(|(_, deadline)| **deadline <= now)
        .map(|(path, _)| path.clone())
        .collect();
    for path in due {
        pending.remove(&path);
        reindex(&path);
    }
}

fn reindex(path: &Path) {
    match ingest_file(path, false, &HashMap::new(), None) {
        Ok(result) => match serde_json::to_string(&result) {
            Ok(json) => println!("{json}"),
            Err(error) => eprintln!("watch serialize error: {error}"),
        },
        Err(error) => eprintln!("watch ingest error for {}: {error}", path.display()),
    }
}
