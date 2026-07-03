// ────────────────────────────────────────────────
// Per-Job Log Capture System
// Captures ALL log::info!, log::warn!, log::error! output
// to a dedicated per-job log file while also writing to stderr.
// ────────────────────────────────────────────────

use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;


/// Global singleton logger instance.
static LOGGER: OnceLock<JobLogger> = OnceLock::new();

/// The shared logger state — protected by a Mutex for thread-safe access.
struct LoggerState {
    /// The active job log writer (None when no job is running).
    writer: Option<BufWriter<File>>,
    /// Path to the current log file (for export/copy operations).
    log_path: Option<PathBuf>,
    /// Directory where logs are stored.
    logs_dir: Option<PathBuf>,
    /// ID of the currently active job (prevents concurrent job log corruption).
    active_job_id: Option<String>,
}

struct JobLogger {
    state: Mutex<LoggerState>,
}

/// Initialize the global logger. Call once at startup (replaces env_logger).
/// Returns Ok(()) on success, or an error if already initialized.
pub fn init() {
    let logger = JobLogger {
        state: Mutex::new(LoggerState {
            writer: None,
            log_path: None,
            logs_dir: None,
            active_job_id: None,
        }),
    };
    // Use log::set_logger directly — OnceLock::set is infallible if called once.
    // If this panics, the logger was already initialized (double-init).
    let _ = LOGGER.set(logger);

    // CRITICAL: Also set as the global logger so log::info!() etc dispatch to us
    // Box::leak needed because set_logger takes Box<dyn Log + 'static>
    let logger_ref = LOGGER.get().expect("Logger must be set before calling init()");
    let boxed: Box<dyn Log + 'static> = Box::new(logger_ref);
    let _ = log::set_logger(Box::leak(boxed));

    log::set_max_level(LevelFilter::Info);
}

/// Start logging for a job. Creates the log file and begins capturing output.
///
/// `output_dir` — directory where the output file lives (logs/ subfolder created inside)
/// `job_name` — sanitized output filename stem (e.g., "merged_output")
/// `job_id` — unique job identifier
///
/// Returns the path to the created log file.
pub fn start_job_log(output_dir: &str, job_name: &str, job_id: &str) -> Result<PathBuf, String> {
    let output_path = Path::new(output_dir);
    let logs_dir = output_path.join("logs");
    fs::create_dir_all(&logs_dir)
        .map_err(|e| format!("Failed to create logs directory: {}", e))?;

    // Generate timestamp for filename
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let timestamp_str = format_timestamp_file(timestamp);

    // Sanitize job_name for filesystem safety
    let safe_name = sanitize_log_filename(job_name);

    // Build log filename: <job_name>_<job_id>_<timestamp>.log
    let log_filename = format!("{}_{}_{}.log", safe_name, job_id, timestamp_str);
    let log_path = logs_dir.join(&log_filename);

    // Open file for writing (create or truncate)
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to create log file {}: {}", log_path.display(), e))?;

    let mut writer = BufWriter::new(file);

    // Write header
    let header = format!(
        "═══════════════════════════════════════════════════════════════════════\n\
         Job Log: {}\n\
         Job ID:  {}\n\
         Started: {}\n\
         ═══════════════════════════════════════════════════════════════════════\n\n",
        job_name,
        job_id,
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
    );
    let _ = writer.write_all(header.as_bytes());
    let _ = writer.flush();

    // Set up the global logger state
    // Reject if another job is already active (prevents log corruption)
    if let Some(logger) = LOGGER.get() {
        if let Ok(mut state) = logger.state.lock() {
            if state.writer.is_some() {
                log::warn!("[JobLog] Another job ({}) is already logging — rejecting start for '{}'", 
                    state.active_job_id.as_deref().unwrap_or("?"), job_id);
                // Still return the path so caller knows where logs would go
                return Ok(log_path);
            }
            state.writer = Some(writer);
            state.log_path = Some(log_path.clone());
            state.logs_dir = Some(logs_dir.clone());
            state.active_job_id = Some(job_id.to_string());
        }
    }

    log::info!("[JobLog] Logging started for job '{}' ({})", job_name, job_id);
    log::info!("[JobLog] Log file: {}", log_path.display());

    Ok(log_path)
}

/// Stop logging for the current job. Flushes and closes the log file.
/// Called on job completion, failure, or cancellation.
pub fn stop_job_log(final_message: Option<&str>) {
    if let Some(logger) = LOGGER.get() {
        if let Ok(mut state) = logger.state.lock() {
            if let Some(ref mut writer) = state.writer {
                if let Some(msg) = final_message {
                    let _ = writeln!(writer, "\n{}", msg);
                }
                let footer = format!(
                    "\n═══════════════════════════════════════════════════════════════════════\n\
                     Finished: {}\n\
                     ═══════════════════════════════════════════════════════════════════════\n",
                    chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                );
                let _ = writer.write_all(footer.as_bytes());
                let _ = writer.flush();
            }
            state.writer = None;
            state.active_job_id = None;
            // Preserve log_path and logs_dir so Export button works after job ends.
            // They are overwritten by the next start_job_log call.
        }
    }
}

/// Get the path to the current active job log file.
pub fn current_log_path() -> Option<PathBuf> {
    LOGGER.get().and_then(|logger| {
        logger.state.lock().ok().and_then(|state| state.log_path.clone())
    })
}

/// Get the logs directory for the current job.
pub fn current_logs_dir() -> Option<PathBuf> {
    LOGGER.get().and_then(|logger| {
        logger.state.lock().ok().and_then(|state| state.logs_dir.clone())
    })
}

/// Write a raw line directly to the active job log file (no timestamp prefix).
/// Used for capturing child process (ffmpeg/mkvmerge) stdout/stderr.
///
/// NOTE: BufWriter batches writes internally. No per-line flush — the buffer
/// flushes automatically when full or on Drop.
pub fn write_raw(line: &str) {
    if let Some(logger) = LOGGER.get() {
        if let Ok(mut state) = logger.state.lock() {
            if let Some(ref mut writer) = state.writer {
                let _ = writer.write_all(line.as_bytes());
            }
        }
    }
}

/// Get all log files in a directory, sorted by modification time (newest first).
pub fn list_log_files(logs_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(logs_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().map(|e| e == "log").unwrap_or(false))
        .collect();

    files.sort_by(|a, b| {
        let ta = a.metadata().and_then(|m| m.modified()).ok();
        let tb = b.metadata().and_then(|m| m.modified()).ok();
        tb.cmp(&ta) // newest first
    });

    files
}


// ─── RAII Guard for automatic job log finalization ─────────────────

/// RAII guard that automatically calls `stop_job_log` when dropped.
/// Ensures the job log file is always finalized, even on error/panic paths.
///
/// # Usage
/// ```ignore
/// let _log_guard = crate::logger::JobLogGuard::new("[JOB_COMPLETE] Merge completed");
/// // ... do work ...
/// // When `_log_guard` is dropped (scope exit), the log is finalized.
/// ```
pub struct JobLogGuard {
    message: Option<String>,
}

impl JobLogGuard {
    /// Create a new guard that will stop logging with the given final message on drop.
    pub fn new(message: &str) -> Self {
        Self {
            message: Some(message.to_string()),
        }
    }

    /// Create a guard with no final message (just finalizes the log).
    pub fn new_empty() -> Self {
        Self { message: None }
    }
}

impl Drop for JobLogGuard {
    fn drop(&mut self) {
        stop_job_log(self.message.as_deref());
    }
}

/// Format a Unix timestamp into a filesystem-safe string: YYYYMMDD_HHMMSS
fn format_timestamp_file(secs: u64) -> String {
    let dt: chrono::DateTime<chrono::Local> = chrono::DateTime::from(
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
    );
    dt.format("%Y%m%d_%H%M%S").to_string()
}

/// Sanitize a filename for log files (remove/replace unsafe characters).
fn sanitize_log_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            ' ' => '_',
            _ => c,
        })
        .take(80) // Limit length
        .collect()
}

// ─── log::Log trait implementation ────────────────────────────────

impl Log for JobLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level = record.level();
        let target = record.target();
        let message = record.args();

        // Format timestamp
        let now = chrono::Local::now();
        let timestamp = now.format("%Y-%m-%d %H:%M:%S%.3f").to_string();

        // Format level string
        let level_str = match level {
            Level::Error => "ERROR",
            Level::Warn => "WARN ",
            Level::Info => "INFO ",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };

        // Build the log line
        let log_line = format!("[{}] [{}] [{}] {}\n", timestamp, level_str, target, message);

        // Always write to stderr (console output)
        eprint!("{}", log_line);

        // Also write to the active job log file if one is open.
        // NOTE: No per-line flush() — BufWriter batches internally and flushes
        // when its 8KB buffer is full or on Drop. Per-line flush defeats buffering
        // and adds a syscall per log call. Crash safety is provided by flush-on-drop
        // semantics (stop_job_log, Drop, end_forensic_log).
        if let Some(logger) = LOGGER.get() {
            if let Ok(mut state) = logger.state.lock() {
                if let Some(ref mut writer) = state.writer {
                    let _ = writer.write_all(log_line.as_bytes());
                }
            }
        }
    }

    fn flush(&self) {
        // Flush stderr
        let _ = io::stderr().flush();

        // Flush the job log file
        if let Some(logger) = LOGGER.get() {
            if let Ok(mut state) = logger.state.lock() {
                if let Some(ref mut writer) = state.writer {
                    let _ = writer.flush();
                }
            }
        }
    }
}
