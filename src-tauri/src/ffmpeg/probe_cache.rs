use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::sync::Semaphore;
use crate::types::MediaInfo;
use super::probe::probe_file;

/// Maximum number of entries in the probe cache before FIFO eviction kicks in.
/// Protects against OOM on pathological inputs (100k+ unique file playlists).
/// 50_000 entries � ~2 KB avg per entry � 100 MB worst-case resident memory.
const PROBE_CACHE_MAX_ENTRIES: usize = 50_000;

/// Thread-safe cache for ffprobe results.
/// Probe once in parallel, then reuse results everywhere.
/// Uses bounded FIFO eviction: when the cache exceeds PROBE_CACHE_MAX_ENTRIES,
/// the oldest-inserted entry is dropped. This prevents unbounded memory growth
/// on extremely large playlists (>100k files).
pub struct ProbeCache {
    results: RwLock<HashMap<PathBuf, Result<MediaInfo, String>>>,
    /// Insertion-order tracker for FIFO eviction.
    order: RwLock<VecDeque<PathBuf>>,
}

impl Default for ProbeCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ProbeCache {
    pub fn new() -> Self {
        Self {
            results: RwLock::new(HashMap::new()),
            order: RwLock::new(VecDeque::new()),
        }
    }

    /// Get a cached probe result for a file path.
    pub fn get(&self, path: &Path) -> Option<Result<MediaInfo, String>> {
        match self.results.read() {
            Ok(guard) => guard.get(path).cloned(),
            Err(poisoned) => {
                log::error!("[ProbeCache] Read lock poisoned in get() -- recovering");
                let guard = poisoned.into_inner();
                guard.get(path).cloned()
            }
        }
    }

    /// Store a probe result in the cache.
    /// If the cache exceeds PROBE_CACHE_MAX_ENTRIES, the oldest entry is evicted.
    pub fn insert(&self, path: PathBuf, result: Result<MediaInfo, String>) {
        // Track insertion order for eviction
        if let Ok(mut order) = self.order.write() {
            // If the path already exists, remove its old position
            if let Some(pos) = order.iter().position(|p| *p == path) {
                order.remove(pos);
            }
            order.push_back(path.clone());
        }

        match self.results.write() {
            Ok(mut map) => {
                map.insert(path.clone(), result);

                // Evict oldest entries if over capacity
                if map.len() > PROBE_CACHE_MAX_ENTRIES {
                    if let Ok(mut order) = self.order.write() {
                        while map.len() > PROBE_CACHE_MAX_ENTRIES {
                            if let Some(oldest) = order.pop_front() {
                                map.remove(&oldest);
                                log::debug!("[ProbeCache] Evicted oldest entry: {:?}", oldest);
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            Err(poisoned) => {
                log::error!("[ProbeCache] Write lock poisoned in insert() -- recovering");
                let mut map = poisoned.into_inner();
                map.insert(path, result);
            }
        }
    }

/// Number of cached entries.
    pub fn len(&self) -> usize {
        match self.results.read() {
            Ok(guard) => guard.len(),
            Err(poisoned) => {
                log::error!("[ProbeCache] Read lock poisoned in len() -- recovering");
                poisoned.into_inner().len()
            }
        }
    }

    /// Returns true if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Max concurrent ffprobe processes during parallel probe.
/// Matches the existing batch_probe limit for consistency.
const MAX_PARALLEL_PROBES: usize = 6;

/// Probe all files in parallel and populate a shared cache.
///
/// Uses a tokio Semaphore to limit concurrent ffprobe processes.
/// Returns a ProbeCache that can be shared across the merge pipeline.
///
/// This consolidates multiple sequential probe passes into a single parallel probe.
/// The cached results are reused by compatibility checking, profile scanning,
/// and post-normalization verification -- avoiding redundant ffprobe calls.
pub async fn probe_all_parallel(
    files: &[&Path],
    ffprobe_path: &Path,
) -> ProbeCache {
    let cache = ProbeCache::new();
    if files.is_empty() {
        return cache;
    }

    let start_time = std::time::Instant::now();
    log::info!("[FORENSIC:PROBE] ENTER probe_all_parallel ({} files)", files.len());

    let semaphore = Arc::new(Semaphore::new(MAX_PARALLEL_PROBES));
    let ffprobe = Arc::new(ffprobe_path.to_path_buf());

    let handles: Vec<_> = files.iter().map(|file| {
        let sem = semaphore.clone();
        let ffprobe = ffprobe.clone();
        let file_path = file.to_path_buf();
        let file_path_clone = file_path.clone();
        tokio::spawn(async move {
            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    log::error!("[ProbeCache] Semaphore acquire failed for {}: {} -- aborting probe", file_path.display(), e);
                    return (file_path, Err(format!("Semaphore closed: {}", e)));
                }
            };
            let result = tokio::task::spawn_blocking(move || {
                probe_file(&ffprobe, &file_path_clone)
                    .map_err(|e| e.to_string())
            }).await
            .map_err(|e| format!("Probe task panicked: {}", e))
            .unwrap_or_else(Err);
            (file_path, result)
        })
    }).collect();

    for handle in handles {
        match handle.await {
            Ok((path, result)) => {
                cache.insert(path, result);
            }
            Err(e) => {
                log::error!("[ProbeCache] Probe task join error (possible panic): {} -- file identity lost due to panic", e);
            }
        }
    }

    log::info!("[FORENSIC:PROBE] EXIT probe_all_parallel | ELAPSED: {:?} | Probed {} files",
        start_time.elapsed(), cache.len());
    cache
}
