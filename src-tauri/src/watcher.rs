use std::path::{Path, PathBuf};
use notify::{RecursiveMode, RecommendedWatcher};
use notify_debouncer_mini::{DebounceEventResult, DebouncedEventKind, new_debouncer};
use tauri::{AppHandle, Emitter};
use std::time::Duration;

/// File change event kinds we care about
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FsChangeKind {
    Created,
    Modified,
    Renamed,
    Deleted,
}

/// File change event payload emitted to frontend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsChangeEvent {
    pub paths: Vec<String>,
    pub kind: FsChangeKind,
}

#[derive(Debug)]
#[derive(Default)]
pub struct WatcherState {
    /// The inner debounced watcher (Option so we can drop it on stop)
    watcher: Option<notify_debouncer_mini::Debouncer<RecommendedWatcher>>,
    /// Directories currently being watched
    watched_dirs: Vec<PathBuf>,
}


/// Watched file extensions for video and subtitle files
const WATCHED_VIDEO_EXTS: &[&str] = &["mp4", "mkv", "mov", "avi", "webm", "m4v", "ts", "mts", "flv", "wmv", "3gp"];
const WATCHED_SUB_EXTS: &[&str] = &["srt", "vtt", "ass", "ssa", "sub"];

fn is_watched_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_lowercase();
            WATCHED_VIDEO_EXTS.contains(&e.as_str()) || WATCHED_SUB_EXTS.contains(&e.as_str())
        })
        .unwrap_or(false)
}

fn map_debounced_kind(_kind: DebouncedEventKind) -> FsChangeKind {
    // notify-debouncer-mini 0.4 only distinguishes between Any and AnyContinuous.
    // We treat both as Modified since we can't distinguish create/delete/rename.
    FsChangeKind::Modified
}

/// Start watching a list of directories for file changes.
/// Emits Tauri events (`fs-change`) to the frontend.
pub fn start_watching(
    state: &mut WatcherState,
    app_handle: AppHandle,
    dirs: Vec<PathBuf>,
) -> Result<(), String> {
    // Stop any existing watcher first
    if let Some(old) = state.watcher.take() {
        drop(old);
    }
    state.watched_dirs.clear();

    let handle = app_handle.clone();

    let mut debouncer = new_debouncer(
        Duration::from_secs(1), // debounce window
        move |res: DebounceEventResult| {
            match res {
                Ok(events) => {
                    for event in events {
                        let kind = map_debounced_kind(event.kind);
                        let path = &event.path;
                        if !is_watched_file(path) {
                            continue;
                        }
                        let paths = vec![path.to_string_lossy().into_owned()];
                        log::info!(
                            "[FsWatcher] Detected {} file(s): {:?} (kind={:?})",
                            paths.len(),
                            paths,
                            kind
                        );
                        let _ = handle.emit(
                            "fs-change",
                            FsChangeEvent {
                                paths,
                                kind,
                            },
                        );
                    }
                }
                Err(err) => {
                    log::warn!("[FsWatcher] Debounce error: {}", err);
                }
            }
        },
    )
    .map_err(|e| format!("Failed to create debounced watcher: {}", e))?;

    // Watch each directory
    let mut watched = Vec::new();
    for dir in &dirs {
        if !dir.exists() {
            log::warn!("[FsWatcher] Directory does not exist, skipping: {:?}", dir);
            continue;
        }
        // Canonicalize to avoid duplicate watches on symlinked dirs
        let canon = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if watched.contains(&canon) {
            continue;
        }
        debouncer
            .watcher()
            .watch(&canon, RecursiveMode::Recursive)
            .map_err(|e| format!("Failed to watch directory {:?}: {}", canon, e))?;
        log::info!("[FsWatcher] Watching directory: {:?}", &canon);
        watched.push(canon);
    }

    state.watcher = Some(debouncer);
    state.watched_dirs = watched;

    Ok(())
}

/// Stop the watcher cleanly
pub fn stop_watching(state: &mut WatcherState) {
    if let Some(w) = state.watcher.take() {
        drop(w);
    }
    state.watched_dirs.clear();
    log::info!("[FsWatcher] Stopped");
}
