mod commands;
pub mod recovery;
mod ffmpeg;
mod naming;
pub mod report;
mod services;
mod split;
mod types;
pub mod watcher;
pub mod logger;
pub mod forensic_log;
pub mod diagnostics;

pub mod certification_api;
pub mod certification;

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::Mutex;
use commands::merge::MergeState;
use watcher::WatcherState;

pub struct AppState {
    pub merge_state: Arc<Mutex<MergeState>>,
    pub watcher_state: Arc<Mutex<WatcherState>>,
    /// Monotonically increasing epoch used to cancel in-flight commands.
    /// Commands capture the epoch at entry and compare before each heavy operation.
    /// `cancel_pending_operations()` bumps the epoch, causing all captured epochs to
    /// mismatch and commands to bail early.
    pub cancel_epoch: Arc<AtomicU64>,
}

/// Run startup cleanup: clean up orphaned temp files and crash markers from
/// previous sessions. This runs synchronously before the Tauri app starts.
pub fn run_startup_cleanup() {
    log::info!("[Startup] Running cleanup routines...");

    // 1. Clean up old temp concat files (>24h old)
    if let Ok(tmp_dir) = crate::ffmpeg::get_temp_dir() {
        let now = std::time::SystemTime::now();
        let max_age = std::time::Duration::from_secs(24 * 3600);
        // Clean files directly in temp_dir
        if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if fname.starts_with("concat_") || fname.starts_with("concat_sub_") || fname.starts_with("concat_part_") || fname.starts_with("norm_") || fname.starts_with("conv_sub_") || fname.starts_with("conv_bin_sub_") || fname.starts_with("ext_sub_") || fname.starts_with("burn_sub_") || fname.starts_with("extracted_sub_") || fname == "dummy.srt" || fname == "dummy.vtt" {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if now.duration_since(modified).map(|age| age >= max_age).unwrap_or(false) {
                                let _ = std::fs::remove_file(&path);
                                log::info!("[Startup] Removed old temp file: {:?}", path);
                            }
                        }
                    }
                }
            }
        }
        // Clean old files in cards/ subdirectory
        let cards_dir = tmp_dir.join("cards");
        if let Ok(entries) = std::fs::read_dir(&cards_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if now.duration_since(modified).map(|age| age >= max_age).unwrap_or(false) {
                                let _ = std::fs::remove_file(&path);
                                log::info!("[Startup] Removed old card temp file: {:?}", path);
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Search for orphan .merging marker files in the temp dir.
    // Only clean up markers older than 24h to avoid deleting a valid output file
    // whose marker removal failed on the previous session.
    if let Ok(tmp_dir) = crate::ffmpeg::get_temp_dir() {
        if let Ok(entries) = std::fs::read_dir(&tmp_dir) {
            let now = std::time::SystemTime::now();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "merging").unwrap_or(false) {
                    let age_ok = std::fs::metadata(&path).ok()
                        .and_then(|m| m.modified().ok())
                        .map(|t| now.duration_since(t).map(|d| d.as_secs() > 86400).unwrap_or(true))
                        .unwrap_or(true);
                    if !age_ok {
                        log::info!("[Startup] Skipping recent .merging marker (younger than 24h): {:?}", path);
                        continue;
                    }
                    let output_path = path.to_string_lossy();
                    let output_path = output_path.strip_suffix(".merging").unwrap_or(&output_path);
                    log::info!("[Startup] Cleaning up orphan .merging marker (age > 24h): {}", output_path);
                    crate::commands::merge::cleanup_orphan_merge(output_path);
                }
            }
        }
    }

    // 3. Clean up old .tmp files in thumbnail cache (>24h)
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
    if cache_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            let now = std::time::SystemTime::now();
            let max_age = std::time::Duration::from_secs(24 * 3600);
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "tmp").unwrap_or(false) {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if now.duration_since(modified).map(|age| age >= max_age).unwrap_or(false) {
                                let _ = std::fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    log::info!("[Startup] Cleanup complete.");
}

pub fn run() {
    // Initialize the custom per-job log capture logger.
    // This replaces env_logger and captures ALL log output to per-job files.
    logger::init();

    // ── Panic Hook ──────────────────────────────────────────────────
    // Capture panics to the forensic log so crashes are recorded even
    // if no merge is in progress. Without this hook, panics would show
    // the default panic message with no forensic capture or cleanup trigger.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        let location = panic_info.location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>").to_string();

        let panic_msg = format!("{} at {} (thread: {})", message, location, thread_name);
        log::error!("[PANIC] {}", panic_msg);
        crate::forensic_log::append_panic_block(&panic_msg);

        // P0 FIX: Finalize the forensic log footer after appending panic info.
        // Without this, panics leave the forensic log with only the header + panic block
        // but no MERGE END footer — the exact "header-only" bug reported in production.
        crate::forensic_log::end_forensic_log(
            crate::forensic_log::ForensicStatus::Panic,
            Some(&panic_msg),
            None, // output_path unknown at panic time
        );

        // Also call the default hook so stderr gets the usual panic output
        default_hook(panic_info);
    }));

    run_startup_cleanup();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            merge_state: Arc::new(Mutex::new(MergeState::default())),
            watcher_state: Arc::new(Mutex::new(WatcherState::default())),
            cancel_epoch: Arc::new(AtomicU64::new(0)),
        })
        .invoke_handler(tauri::generate_handler![
            commands::fs::scan_directory,
            commands::fs::scan_subfolders,
            commands::fs::get_disk_space,
            commands::fs::reveal_in_explorer,
            commands::fs::open_with_default,
            commands::fs::sanitize_filename,
            commands::fs::ensure_directory,
            commands::media::probe_video,
            commands::media::batch_probe,
            commands::media::generate_thumbnail,
            commands::media::capture_frame,
            commands::media::clear_thumbnail_cache,
            commands::merge::start_merge,
            commands::merge::cancel_merge,
            commands::merge::get_merge_status,
            commands::merge::check_recovery_checkpoints,
            commands::merge::delete_recovery_checkpoint,
            commands::merge::validate_audio_files,
            commands::merge::check_merge_compatibility,
            commands::playlist::save_playlist,
            commands::playlist::load_playlist,
            commands::playlist::list_saved_playlists,
            commands::playlist::delete_playlist,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::get_ffmpeg_path,
            commands::fs::watch_directories,
            commands::fs::stop_watching_dirs,
            commands::fs::cancel_pending_operations,
            // Split commands
            commands::split::generate_split_plan,
            commands::split::generate_chapter_split_plan,
            commands::split::execute_split_plan,
            commands::split::cancel_split,
            // Naming commands
            commands::naming::validate_naming_template,
            commands::naming::resolve_naming,
            commands::naming::preview_naming_batch,
            commands::naming::write_text_file,
            // Health commands
            commands::health::check_file_health,
            // Section merge commands
            commands::section::compute_section_preview_cmd,
            commands::section::get_playlist_hash,
            commands::section::start_section_merge,
            commands::section::cancel_section_merge,
            commands::section::get_section_merge_status,
            commands::section::section_merge_disk_space_required,
            // Log management commands
            commands::log::open_logs_folder,
            commands::log::get_active_log_path,
            commands::log::get_job_log_files,
            commands::log::read_job_log,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            log::error!("[FATAL] Failed to run Tauri application: {}", e);
            std::process::exit(1);
        });
}
