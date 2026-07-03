use crate::types::AppSettings;
use std::path::PathBuf;

fn get_settings_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("PlaylistMerger")
        .join("settings.json")
}

pub fn load_settings_internal() -> AppSettings {
    let path = get_settings_path();
    if !path.exists() {
        log::info!("[Settings] No settings file found at {:?}, using defaults", path);
        return AppSettings::default();
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log::error!("[Settings] CRITICAL: Failed to read settings file {:?}: {}", path, e);
            return AppSettings::default();
        }
    };

    match serde_json::from_str::<AppSettings>(&content) {
        Ok(mut settings) => {
            log::info!("[Settings] Loaded settings successfully from {:?}", path);

            // MIGRATION: Force enable packet timestamp certification
            // This is critical for detecting the "unknown timestamp" ghost bug
            // that causes merges to fail after 10+ hours with large playlists.
            if !settings.check_packet_timestamps {
                log::warn!("[Settings] check_packet_timestamps was disabled in settings file.");
                log::warn!("[Settings] Forcing ENABLE for ghost bug detection. Set to true in settings UI if you want to disable it.");
                settings.check_packet_timestamps = true;
            }

            settings
        }
        Err(e) => {
            log::error!("[Settings] CRITICAL: Settings file {:?} is corrupted (JSON parse failed): {}. Backing up corrupted file.", path, e);
            let backup_path = path.with_extension("json.corrupted");
            if let Err(be) = std::fs::rename(&path, &backup_path) {
                log::error!("[Settings] Failed to backup corrupted settings to {:?}: {}", backup_path, be);
            } else {
                log::info!("[Settings] Backed up corrupted settings to {:?}", backup_path);
            }
            AppSettings::default()
        }
    }
}

/// Investigate and log why sync_all might fail.
/// Returns (tmp_exists, file_openable, sync_all_result) for debugging.
fn investigate_sync_failure(tmp: &std::path::Path, path: &std::path::Path) -> String {
    let mut details = Vec::new();

    // Check if temp file exists
    if tmp.exists() {
        match std::fs::metadata(tmp) {
            Ok(meta) => {
                details.push(format!("tmp file exists: {} bytes", meta.len()));
            }
            Err(e) => {
                details.push(format!("tmp file exists but metadata failed: {}", e));
            }
        }
    } else {
        details.push("tmp file does NOT exist".to_string());
    }

    // Check settings.json
    if path.exists() {
        match std::fs::metadata(path) {
            Ok(meta) => {
                details.push(format!("settings.json exists: {} bytes", meta.len()));
            }
            Err(e) => {
                details.push(format!("settings.json exists but metadata failed: {}", e));
            }
        }
    } else {
        details.push("settings.json does NOT exist".to_string());
    }

    // Check directory permissions
    if let Some(parent) = path.parent() {
        match std::fs::metadata(parent) {
            Ok(meta) => {
                details.push(format!("parent dir accessible: {:?}", meta.permissions()));
            }
            Err(e) => {
                details.push(format!("parent dir metadata failed: {}", e));
            }
        }
    }

    details.join(" | ")
}

pub fn save_settings_internal(settings: &AppSettings) -> Result<(), String> {
    let path = get_settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create settings dir: {}", e))?;
    }

    // ── Validation ───────────────────────────────────────────────────
    // maxThumbnailCacheMb: reject values outside 50–5000 range
    if settings.max_thumbnail_cache_mb < 50 || settings.max_thumbnail_cache_mb > 5000 {
        return Err(format!(
            "Invalid thumbnail cache size: {} MB. Must be between 50 and 5000.",
            settings.max_thumbnail_cache_mb
        ));
    }

    // Write to a temp file first, then rename — atomic on most filesystems
    let tmp = path.with_extension("tmp");
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Serialization error: {}", e))?;

    // Step 1: Write temp file
    if let Err(e) = std::fs::write(&tmp, &json) {
        return Err(format!("Failed to write settings temp file: {} (path: {:?})", e, tmp));
    }

    // Step 2: Open for sync (this is where "Access denied" happens)
    match std::fs::File::open(&tmp) {
        Ok(file) => {
            // Step 3: Sync
            if let Err(e) = file.sync_all() {
                // sync_all failed — this is non-fatal but we should log it
                // The rename below will still work; sync_all is just a best-effort flush
                log::warn!("[Settings] sync_all failed: {} (path: {:?})", e, tmp);
                log::warn!("[Settings] Investigation: {}", investigate_sync_failure(&tmp, &path));
                // Continue anyway - sync_all is best effort
            }
        }
        Err(e) => {
            // Couldn't open file for sync — this is unusual
            log::error!("[Settings] Failed to open settings file for sync: {} (path: {:?})", e, tmp);
            log::error!("[Settings] Investigation: {}", investigate_sync_failure(&tmp, &path));

            // Try to proceed anyway - the rename might still work
            // If it fails, we'll catch it in the rename step
        }
    }

    // Step 4: Rename temp to final
    if let Err(e) = std::fs::rename(&tmp, &path) {
        // On Windows, rename fails if target exists and is open by another process
        // Clean up the temp file and report the error
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "Failed to commit settings (rename failed): {} (from {:?} to {:?}). \
             Another process may be holding settings.json open.",
            e, tmp, path
        ));
    }

    log::info!("[Settings] Saved successfully to {:?}", path);
    Ok(())
}
