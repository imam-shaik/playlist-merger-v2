// ────────────────────────────────────────────────
// Log Management Commands
// Exposes per-job log operations to the frontend.
// ────────────────────────────────────────────────

use std::path::Path;
use tauri::command;

/// Open the logs folder for the current job in the file explorer.
/// Falls back to the output directory's logs/ subfolder if no active job.
#[command]
pub async fn open_logs_folder(output_path: String) -> Result<(), String> {
    let output_dir = Path::new(&output_path);
    let logs_dir = output_dir.join("logs");

    if !logs_dir.exists() {
        // Create the directory if it doesn't exist
        std::fs::create_dir_all(&logs_dir)
            .map_err(|e| format!("Failed to create logs directory: {}", e))?;
    }

    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .arg(logs_dir.to_string_lossy().to_string())
            .spawn();
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(logs_dir.to_string_lossy().to_string())
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {}", e))?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(logs_dir.to_string_lossy().to_string())
            .spawn()
            .map_err(|e| format!("Failed to open logs folder: {}", e))?;
        Ok(())
    }
}

/// Get the path to the active job's log file, if any.
#[command]
pub async fn get_active_log_path() -> Option<String> {
    crate::logger::current_log_path()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Get all log files in a job's output directory.
#[command]
pub async fn get_job_log_files(output_path: String) -> Result<Vec<LogFileInfo>, String> {
    let output_dir = Path::new(&output_path);
    let logs_dir = output_dir.join("logs");

    if !logs_dir.exists() {
        return Ok(Vec::new());
    }

    let files = crate::logger::list_log_files(&logs_dir);
    let mut result = Vec::new();

    for path in files {
        let metadata = std::fs::metadata(&path).ok();
        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
        let modified = metadata
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        result.push(LogFileInfo {
            filename,
            path: path.to_string_lossy().into_owned(),
            size_bytes: size,
            modified_timestamp: modified,
        });
    }

    Ok(result)
}

/// Read the contents of a specific log file.
#[command]
pub async fn read_job_log(log_path: String) -> Result<String, String> {
    std::fs::read_to_string(&log_path)
        .map_err(|e| format!("Failed to read log file: {}", e))
}

#[derive(serde::Serialize)]
pub struct LogFileInfo {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
    pub modified_timestamp: u64,
}
