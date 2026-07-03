use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{command, State};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use crate::AppState;
use crate::ffmpeg::probe::probe_file;
use crate::types::MediaInfo;

/// Max concurrent ffprobe processes — protects against spawning thousands at once
const MAX_PROBE_CONCURRENCY: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailSize {
    pub width: u32,
    pub height: u32,
}

/// Probe a single video file with ffprobe.
/// Runs on the blocking thread pool — safe for async callers.
/// Times out after 60 seconds to prevent hanging on corrupted files.
#[command]
pub async fn probe_video(
    path: String,
    _state: State<'_, AppState>,
) -> Result<MediaInfo, String> {
    let settings = crate::services::settings::load_settings_internal();
    let ffprobe_path = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref())
        .map_err(|e| e.to_string())?;

    // Run synchronous ffprobe on the blocking thread pool with a 60s timeout
    let path_display = path.clone();
    tokio::time::timeout(
        std::time::Duration::from_secs(60),
        tokio::task::spawn_blocking(move || {
            probe_file(&ffprobe_path, Path::new(&path))
                .map_err(|e| e.to_string())
        }),
    )
    .await
    .map_err(|_| format!("Probe timed out after 60s for: {}", path_display))?
    .map_err(|e| format!("Task panicked: {}", e))?
}

/// Probe multiple files in parallel, with a concurrency limit.
/// Returns one result per path — errors are returned as Err strings (not panics).
/// Respects the global cancel epoch — returns partial results if cancelled.
#[command]
pub async fn batch_probe(
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<Result<MediaInfo, String>>, String> {
    let epoch = state.cancel_epoch.load(Ordering::SeqCst);
    let cancel_epoch = state.cancel_epoch.clone();

    if paths.is_empty() {
        return Ok(vec![]);
    }

    if cancel_epoch.load(Ordering::SeqCst) != epoch {
        return Ok(vec![]);
    }

    let settings = crate::services::settings::load_settings_internal();
    let ffprobe_path = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref())
        .map_err(|e| e.to_string())?;
    let ffprobe_arc = Arc::new(ffprobe_path);

    // Semaphore limits how many concurrent probe processes run
    let semaphore = Arc::new(Semaphore::new(MAX_PROBE_CONCURRENCY));

    let handles: Vec<_> = paths
        .into_iter()
        .map(|path| {
            let ffprobe = ffprobe_arc.clone();
            let sem = semaphore.clone();
            tokio::spawn(async move {
                // Acquire semaphore permit before spawning process
                let _permit = sem.acquire().await
                    .map_err(|_| "Semaphore closed".to_string())?;

                tokio::task::spawn_blocking(move || {
                    probe_file(&ffprobe, Path::new(&path))
                        .map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| format!("Task panicked: {}", e))?
            })
        })
        .collect();

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        // Check cancellation between each pending probe
        if cancel_epoch.load(Ordering::SeqCst) != epoch {
            log::info!("[batch_probe] Cancelled — returning {} partial results", results.len());
            return Ok(results);
        }
        match handle.await {
            Ok(r) => results.push(r),
            Err(e) => results.push(Err(format!("Task join error: {}", e))),
        }
    }

    Ok(results)
}

/// Generate a thumbnail for a video file at a given timestamp.
/// Thumbnails are cached by a hash of (path, timestamp, size).
/// Runs on blocking thread pool. Respects the global cancel epoch.
#[command]
pub async fn generate_thumbnail(
    video_path: String,
    timestamp_seconds: Option<f64>,
    size: Option<ThumbnailSize>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let epoch = state.cancel_epoch.load(Ordering::SeqCst);

    if state.cancel_epoch.load(Ordering::SeqCst) != epoch {
        return Err("Operation cancelled".to_string());
    }

    let settings = crate::services::settings::load_settings_internal();
    let ffmpeg_path = crate::ffmpeg::find_ffmpeg(settings.ffmpeg_path.as_deref())
        .map_err(|e| e.to_string())?;

    let size = size.unwrap_or(ThumbnailSize { width: 160, height: 90 });
    let ts = timestamp_seconds.unwrap_or(2.0).max(0.0);

    // Cache directory
    let cache_dir = settings
        .thumbnail_cache_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::cache_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("PlaylistMerger")
                .join("thumbnails")
        });

    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Cannot create thumbnail cache dir: {}", e))?;

        log::info!("[Thumbnail] Cache dir: {:?}", cache_dir);

        // Deterministic cache key: hash of path + timestamp + size
        let cache_key = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            video_path.hash(&mut h);
            ((ts * 1000.0) as u64).hash(&mut h);
            size.width.hash(&mut h);
            size.height.hash(&mut h);
            h.finish()
        };

        let thumb_tmp = cache_dir.join(format!("{:016x}.tmp", cache_key));
        let thumb_path = cache_dir.join(format!("{:016x}.jpg", cache_key));

        // Check cache BEFORE generating — if another request already wrote it, return
        if thumb_path.exists() && thumb_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            return Ok(thumb_path.to_string_lossy().into_owned());
        }

        // Scale+pad filter: maintain aspect ratio, pad black to exact size
        let vf = format!(
            "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:black",
            w = size.width,
            h = size.height
        );

        let generate = |seek: f64| -> bool {
            #[cfg(windows)]
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            let mut cmd = std::process::Command::new(&ffmpeg_path);
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.args([
                    "-hide_banner",
                    "-loglevel", "error",
                    "-ss", &seek.to_string(),
                    "-i", &video_path,
                    "-vframes", "1",
                    "-vf", &vf,
                    "-q:v", "4",
                    "-f", "image2",
                    "-y",
                ])
                .arg(&thumb_tmp)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };

        // Try at the requested timestamp, fall back to 0.1s for short clips
        if !generate(ts) {
            generate(0.1);
        }

        // Verify the temp file exists, then atomically rename
        if thumb_tmp.exists() && thumb_tmp.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            // Atomic rename on the same filesystem — prevents partial-read race
            if let Err(e) = std::fs::rename(&thumb_tmp, &thumb_path) {
                log::error!("[Thumbnail] Rename failed: {} -> {:?}: {}", thumb_tmp.display(), thumb_path, e);
                return Err(format!("Failed to save thumbnail: {}", e));
            }
            log::info!("[Thumbnail] Generated: {:?}", thumb_path);
            Ok(thumb_path.to_string_lossy().into_owned())
        } else {
            let _ = std::fs::remove_file(&thumb_tmp);
            Err(format!("Thumbnail generation produced no output for '{}'", video_path))
        }
    })
    .await
    .map_err(|e| format!("Thumbnail task panicked: {}", e))?
}

/// Capture a full-resolution frame from a video at a given timestamp.
/// Unlike thumbnails (small, cached), screenshots are full-quality captures
/// saved to a persistent screenshots directory for the side panel.
/// Returns the path to the saved screenshot image.
#[command]
pub async fn capture_frame(
    video_path: String,
    timestamp_seconds: Option<f64>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let epoch = state.cancel_epoch.load(Ordering::SeqCst);
    if state.cancel_epoch.load(Ordering::SeqCst) != epoch {
        return Err("Operation cancelled".to_string());
    }

    let settings = crate::services::settings::load_settings_internal();
    let ffmpeg_path = crate::ffmpeg::find_ffmpeg(settings.ffmpeg_path.as_deref())
        .map_err(|e| e.to_string())?;

    let ts = timestamp_seconds.unwrap_or(2.0).max(0.0);

    // Screenshots directory — persistent, not auto-cleaned
    let screenshots_dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("PlaylistMerger")
        .join("screenshots");

    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&screenshots_dir)
            .map_err(|e| format!("Cannot create screenshots dir: {}", e))?;

        // Unique filename based on timestamp + random suffix
        let timestamp_str = format!("{:.3}", ts).replace('.', "_");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let filename = format!("frame_{}_{}.png", timestamp_str, nanos);
        let out_path = screenshots_dir.join(&filename);
        let tmp_path = screenshots_dir.join(format!("{}.tmp", filename));

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd
            .arg(&tmp_path)
            .status()
            .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

        if !status.success() {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("ffmpeg exited with status {} for '{}'", status, video_path));
        }

        // Verify and atomic rename
        if tmp_path.exists() && tmp_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            std::fs::rename(&tmp_path, &out_path)
                .map_err(|e| format!("Failed to save screenshot: {}", e))?;
            log::info!("[Screenshot] Captured: {:?}", out_path);
            Ok(out_path.to_string_lossy().into_owned())
        } else {
            let _ = std::fs::remove_file(&tmp_path);
            Err(format!("Frame capture produced no output for '{}'", video_path))
        }
    })
    .await
    .map_err(|e| format!("Screenshot task panicked: {}", e))?
}

#[command]
pub async fn clear_thumbnail_cache(
    _state: State<'_, AppState>,
) -> Result<u64, String> {
    let settings = crate::services::settings::load_settings_internal();
    let cache_dir = settings
        .thumbnail_cache_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::cache_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("PlaylistMerger")
                .join("thumbnails")
        });

    tokio::task::spawn_blocking(move || {
        if !cache_dir.exists() {
            return Ok(0);
        }

        let mut deleted_files: u64 = 0;
        let mut freed_bytes: u64 = 0;
        let read_dir = std::fs::read_dir(&cache_dir)
            .map_err(|e| format!("Cannot read cache directory: {}", e))?;

        log::info!("[ThumbnailCache] Clearing cache directory: {:?}", cache_dir);

        // Use a 60-second age threshold to avoid racing with concurrent thumbnail generation.
        // A thumbnail being written right now will be skipped and kept — it will be cleaned
        // up on the next cache clear instead.
        let now = std::time::SystemTime::now();
        let age_threshold = std::time::Duration::from_secs(60);

        for entry in read_dir.flatten() {
            let path = entry.path();
            // Skip .tmp files — they're being written right now
            if path.extension().map(|e| e == "tmp").unwrap_or(false) {
                continue;
            }
            if path.is_file() {
                // Only delete files older than the threshold
                let should_delete = match path.metadata() {
                    Ok(meta) => match meta.modified() {
                        Ok(modified) => now.duration_since(modified).map(|age| age >= age_threshold).unwrap_or(true),
                        Err(_) => true,
                    },
                    Err(_) => false,
                };
                if should_delete {
                    if let Ok(meta) = path.metadata() {
                        freed_bytes += meta.len();
                    }
                    if std::fs::remove_file(&path).is_ok() {
                        deleted_files += 1;
                    }
                }
            }
        }

        log::info!("[ThumbnailCache] Deleted {} files, freed {} bytes", deleted_files, freed_bytes);

        if let Ok(remaining) = std::fs::read_dir(&cache_dir) {
            if remaining.count() == 0 {
                let _ = std::fs::remove_dir(&cache_dir);
            }
        }

        Ok(deleted_files)
    })
    .await
    .map_err(|e| format!("Clear cache task panicked: {}", e))?
}
