use std::cmp::Reverse;
use std::path::Path;
use tauri::command;
use serde::{Deserialize, Serialize};
use crate::types::{SUPPORTED_EXTENSIONS, DiskSpaceInfo};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub extension: String,
    pub modified: Option<i64>,
    pub created: Option<i64>,
    pub parent_folder: Option<String>,
    pub relative_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    pub recursive: bool,
    pub max_depth: Option<usize>,
    pub sort_by: Option<String>,
}

/// Result of scanning first-level subfolders for folder selection UI.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubfolderInfo {
    pub name: String,
    pub path: String,
    pub video_count: usize,
    pub has_subfolders: bool,
}

/// Recursively scan a directory for supported video files.
/// Runs on the blocking thread pool. Respects the global cancel epoch.
#[command]
pub async fn scan_directory(
    path: String,
    options: ScanOptions,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ScannedFile>, String> {
    let epoch = state.cancel_epoch.load(std::sync::atomic::Ordering::SeqCst);

    if state.cancel_epoch.load(std::sync::atomic::Ordering::SeqCst) != epoch {
        return Err("Operation cancelled".to_string());
    }

    tokio::task::spawn_blocking(move || {
        scan_directory_blocking(&path, &options)
    })
    .await
    .map_err(|e| format!("Scan task panicked: {}", e))?
}

/// Scan first-level subfolders of a directory and return info about each.
/// Used for the folder-selection modal when a root folder contains subfolders.
#[command]
pub async fn scan_subfolders(
    path: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<SubfolderInfo>, String> {
    let epoch = state.cancel_epoch.load(std::sync::atomic::Ordering::SeqCst);

    if state.cancel_epoch.load(std::sync::atomic::Ordering::SeqCst) != epoch {
        return Err("Operation cancelled".to_string());
    }

    tokio::task::spawn_blocking(move || {
        scan_subfolders_blocking(&path)
    })
    .await
    .map_err(|e| format!("Scan task panicked: {}", e))?
}

fn scan_subfolders_blocking(path: &str) -> Result<Vec<SubfolderInfo>, String> {
    let dir = Path::new(path);
    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", path));
    }
    if !dir.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    let mut subfolders: Vec<SubfolderInfo> = Vec::new();

    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    for entry in entries.filter_map(|e| e.ok()) {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        if name.starts_with('.') {
            continue;
        }

        let mut video_count = 0;
        let mut has_subfolders = false;

        let sub_entries = match std::fs::read_dir(&entry_path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for sub_entry in sub_entries.filter_map(|e| e.ok()) {
            let sub_path = sub_entry.path();
            if sub_path.is_dir() {
                has_subfolders = true;
            } else if sub_path.is_file() {
                let ext = sub_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
                    video_count += 1;
                }
            }
        }

        subfolders.push(SubfolderInfo {
            name,
            path: entry_path.to_string_lossy().into_owned(),
            video_count,
            has_subfolders,
        });
    }

    subfolders.sort_by(|a, b| natural_cmp(&a.name, &b.name));

    Ok(subfolders)
}

fn scan_directory_blocking(path: &str, options: &ScanOptions) -> Result<Vec<ScannedFile>, String> {
    let dir = Path::new(path);
    if !dir.exists() {
        return Err(format!("Directory does not exist: {}", path));
    }
    if !dir.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    let max_depth = options.max_depth.unwrap_or(if options.recursive { 20 } else { 1 });

    let mut files = Vec::new();
    let walker = walkdir::WalkDir::new(dir)
        .max_depth(max_depth)
        .follow_links(false)
        .sort_by_file_name();

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }

        let file_path = entry.path();
        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        let metadata = match file_path.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let epoch_secs = |t: std::io::Result<std::time::SystemTime>| -> Option<i64> {
            t.ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
        };

        let rel_path = file_path.strip_prefix(dir).ok()
            .map(|p| p.to_string_lossy().into_owned());
        
        let parent_folder = file_path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());

        files.push(ScannedFile {
            path: file_path.to_string_lossy().into_owned(),
            name: file_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            size: metadata.len(),
            extension: ext,
            modified: epoch_secs(metadata.modified()),
            created: epoch_secs(metadata.created()),
            parent_folder,
            relative_path: rel_path,
        });
    }

    // Sort server-side so frontend receives pre-sorted data
    match options.sort_by.as_deref() {
        Some("name") => files.sort_by(|a, b| {
            let dir_a = a.relative_path.as_ref()
                .and_then(|p| Path::new(p).parent())
                .map(|p| p.to_string_lossy().to_lowercase().replace('\\', "/"))
                .unwrap_or_default();
            let dir_b = b.relative_path.as_ref()
                .and_then(|p| Path::new(p).parent())
                .map(|p| p.to_string_lossy().to_lowercase().replace('\\', "/"))
                .unwrap_or_default();

            let cmp = natural_cmp(&dir_a, &dir_b);
            if cmp == std::cmp::Ordering::Equal {
                natural_cmp(&a.name, &b.name)
            } else {
                cmp
            }
        }),
        Some("modified") => files.sort_by_key(|a| Reverse(a.modified)),
        Some("created") => files.sort_by_key(|a| Reverse(a.created)),
        Some("size") => files.sort_by_key(|a| Reverse(a.size)),
        _ => {} // keep walkdir's alphabetical sort
    }

    Ok(files)
}

/// Get real disk space for a given path.
/// Falls back gracefully on platforms where the API is unavailable.
#[command]
pub async fn get_disk_space(path: String) -> Result<DiskSpaceInfo, String> {
    tokio::task::spawn_blocking(move || {
        get_disk_space_blocking(&path)
    })
    .await
    .map_err(|e| format!("Disk space task panicked: {}", e))?
}

fn get_disk_space_blocking(path: &str) -> Result<DiskSpaceInfo, String> {
    // Find the nearest existing ancestor directory
    let check_path = {
        let mut p = Path::new(path);
        loop {
            if p.exists() {
                break p.to_path_buf();
            }
            match p.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => p = parent,
                _ => break std::env::temp_dir(),
            }
        }
    };

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;

        let wide: Vec<u16> = check_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes_caller: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut free_bytes_total: u64 = 0;

        // SAFETY: wide is null-terminated, pointers are valid stack locations
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_bytes_caller,
                &mut total_bytes,
                &mut free_bytes_total,
            )
        };

        if ok != 0 {
            return Ok(DiskSpaceInfo {
                available_bytes: free_bytes_caller,
                total_bytes,
                path: check_path.to_string_lossy().into_owned(),
            });
        }
        // Fall through on failure
    }

    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        let path_cstr = std::ffi::CString::new(check_path.to_string_lossy().as_bytes())
            .map_err(|e| e.to_string())?;
        let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
        let ret = unsafe { libc::statvfs(path_cstr.as_ptr(), stat.as_mut_ptr()) };
        if ret == 0 {
            let stat = unsafe { stat.assume_init() };
            let block = stat.f_frsize as u64;
            return Ok(DiskSpaceInfo {
                available_bytes: stat.f_bavail as u64 * block,
                total_bytes: stat.f_blocks as u64 * block,
                path: check_path.to_string_lossy().into_owned(),
            });
        }
    }

    // Fallback: cannot determine — return 0 so UI can show "unknown"
    Ok(DiskSpaceInfo {
        available_bytes: 0,
        total_bytes: 0,
        path: check_path.to_string_lossy().into_owned(),
    })
}

#[cfg(target_os = "windows")]
extern "system" {
    fn GetDiskFreeSpaceExW(
        lpDirectoryName: *const u16,
        lpFreeBytesAvailableToCaller: *mut u64,
        lpTotalNumberOfBytes: *mut u64,
        lpTotalNumberOfFreeBytes: *mut u64,
    ) -> i32;
}

/// Sanitize a filename — remove/replace invalid characters for the current OS
#[command]
pub fn sanitize_filename(name: String) -> Result<String, String> {
    // Windows forbidden chars (also apply on other OS to be cross-platform safe)
    let forbidden = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    let sanitized: String = name
        .chars()
        .map(|c| {
            if forbidden.contains(&c) || (c as u32) < 32 {
                '_'
            } else {
                c
            }
        })
        .collect();

    // Windows reserved names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
    let reserved = [
        "CON", "PRN", "AUX", "NUL",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];

    let trimmed = sanitized.trim_end_matches(['.', ' ']).to_string();

    if trimmed.is_empty() {
        return Ok("output".to_string());
    }

    // Check reserved names (case-insensitive, without extension)
    let stem = trimmed.split('.').next().unwrap_or(&trimmed).to_uppercase();
    if reserved.contains(&stem.as_str()) {
        return Ok(format!("_{}", trimmed));
    }

    // Enforce max length (Windows MAX_PATH minus overhead)
    Ok(trimmed.chars().take(200).collect())
}

/// Create directory and all parents if missing
#[command]
pub async fn ensure_directory(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&path)
            .map_err(|e| format!("Failed to create directory '{}': {}", path, e))
    })
    .await
    .map_err(|e| format!("Task panicked: {}", e))?
}

/// Reveal a file or folder in the OS file explorer
#[command]
pub async fn reveal_in_explorer(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            // Use /select, to highlight the file
            std::process::Command::new("explorer.exe")
                .args(["/select,", &path])
                .spawn()
                .map_err(|e| format!("explorer.exe failed: {}", e))?;
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .args(["-R", &path])
                .spawn()
                .map_err(|e| format!("open -R failed: {}", e))?;
        }
        #[cfg(target_os = "linux")]
        {
            // Try common file managers
            let managers = ["nautilus", "dolphin", "thunar", "xdg-open"];
            for mgr in &managers {
                if std::process::Command::new(mgr).arg(&path).spawn().is_ok() {
                    break;
                }
            }
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Task panicked: {}", e))?
}

/// Open file with the default OS application
#[command]
pub async fn open_with_default(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        open::that(&path).map_err(|e| format!("Failed to open: {}", e))
    })
    .await
    .map_err(|e| format!("Task panicked: {}", e))?
}

fn natural_cmp(s1: &str, s2: &str) -> std::cmp::Ordering {
    let mut chars1 = s1.chars().peekable();
    let mut chars2 = s2.chars().peekable();

    loop {
        match (chars1.peek(), chars2.peek()) {
            (None, None) => break,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(&c1), Some(&c2)) => {
                if c1.is_ascii_digit() && c2.is_ascii_digit() {
                    let mut n1: u64 = 0;
                    while let Some(&c) = chars1.peek() {
                        if c.is_ascii_digit() {
                            n1 = n1.saturating_mul(10).saturating_add((c as u8 - b'0') as u64);
                            chars1.next();
                        } else {
                            break;
                        }
                    }
                    let mut n2: u64 = 0;
                    while let Some(&c) = chars2.peek() {
                        if c.is_ascii_digit() {
                            n2 = n2.saturating_mul(10).saturating_add((c as u8 - b'0') as u64);
                            chars2.next();
                        } else {
                            break;
                        }
                    }
                    if n1 != n2 {
                        return n1.cmp(&n2);
                    }
                } else {
                    let l1 = c1.to_lowercase().next().unwrap_or(c1);
                    let l2 = c2.to_lowercase().next().unwrap_or(c2);
                    let cmp = l1.cmp(&l2);
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                    chars1.next();
                    chars2.next();
                }
            }
        }
    }

    s1.cmp(s2)
}

/// Start watching one or more directories for file changes.
/// Emits `fs-change` events to the frontend when video/subtitle files are
/// created, modified, renamed, or deleted.
#[command]
pub async fn watch_directories(
    dirs: Vec<String>,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    let mut ws = state.watcher_state.lock().await;
    let dirs: Vec<std::path::PathBuf> = dirs.iter().map(std::path::PathBuf::from).collect();
    crate::watcher::start_watching(&mut ws, app_handle, dirs)
}

/// Stop the filesystem watcher.
#[command]
pub async fn stop_watching_dirs(
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    let mut ws = state.watcher_state.lock().await;
    crate::watcher::stop_watching(&mut ws);
    Ok(())
}

/// Cancel all pending long-running operations (probes, thumbnails, scans).
/// Increments the cancel epoch, causing in-flight commands to detect the change
/// and bail early. Prevents "Couldn't find callback id" errors when the frontend
/// navigates screens or the webview reloads during async work.
#[command]
pub async fn cancel_pending_operations(
    state: tauri::State<'_, crate::AppState>,
) -> Result<(), String> {
    state
        .cancel_epoch
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    log::info!("[CancelOps] Cancelled all pending operations (epoch={})", 
        state.cancel_epoch.load(std::sync::atomic::Ordering::SeqCst));
    Ok(())
}
