// ────────────────────────────────────────────────
// Merge Forensic Log Export
//
// PURE OBSERVABILITY FEATURE — does not modify any
// merge pipeline logic, progress calculations,
// checkpoints, normalization, concat, or any mode.
//
// Writes a structured log file for every merge job
// to Desktop/PlaylistMerger Logs/YYYY-MM/ regardless
// of outcome (success, failure, cancel, panic).
//
// Exported files are NEVER deleted automatically.
// ────────────────────────────────────────────────

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Global state for the forensic log writer.
static FORENSIC_LOG: once_cell::sync::Lazy<Mutex<Option<ForensicLogState>>> =
    once_cell::sync::Lazy::new(|| Mutex::new(None));

struct ForensicLogState {
    writer: Option<BufWriter<File>>,
    #[allow(dead_code)]
    log_path: PathBuf,
    start_time: std::time::Instant,
    job_id: String,
}

/// Represents the final status of a merge job.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ForensicStatus {
    Success,
    Failed,
    Cancelled,
    Panic,
}

impl ForensicStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::Panic => "PANIC",
        }
    }
}

/// Write a raw line to the active forensic log file (no timestamp prefix).
/// Used for appending panic info and FFmpeg command capture.
///
/// NOTE: BufWriter handles batching. Explicit flush is only at critical points
/// (header, footer, panic block) — not on every write.
pub fn write_raw(line: &str) {
    if let Ok(mut state) = FORENSIC_LOG.lock() {
        if let Some(ref mut s) = *state {
            if let Some(ref mut writer) = s.writer {
                let _ = writer.write_all(line.as_bytes());
            }
        }
    }
}

/// Check if the forensic log is currently active and initialized.
/// Returns (is_active, log_path) for debugging.
pub fn is_forensic_log_active() -> (bool, Option<PathBuf>) {
    match FORENSIC_LOG.lock() {
        Ok(state) => {
            if let Some(ref s) = *state {
                (true, Some(s.log_path.clone()))
            } else {
                (false, None)
            }
        }
        Err(_) => (false, None),
    }
}

/// Start a forensic log for a merge job.
///
/// Creates:
///   Desktop/PlaylistMerger Logs/<YYYY-MM>/
///     <playlist_name>*<yyyy-mm-dd_HH-mm-ss>*<job_id>.log
///
/// Returns the path to the created log file.
/// If the desktop cannot be determined, falls back to the output directory.
///
/// IMPORTANT: Failures are logged to the standard logger AND returned as errors.
/// The caller MUST check the return value and must NOT use `let _ = ...`.
#[allow(clippy::too_many_arguments)]
pub fn start_forensic_log(
    output_dir: &str,
    _job_name: &str,
    job_id: &str,
    playlist_name: Option<&str>,
    mode: &str,
    output_path: &str,
    ffmpeg_path: Option<&str>,
    mkvmerge_path: Option<&str>,
    input_count: usize,
) -> Result<PathBuf, String> {
    let safe_name = playlist_name
        .filter(|n| !n.is_empty())
        .unwrap_or("merge")
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' ' => '_',
            _ => c,
        })
        .take(120)
        .collect::<String>();

    // Timestamp for filename (human-readable)
    let now = chrono::Local::now();
    let ts_file = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let ts_header = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let month_folder = now.format("%Y-%m").to_string();

    // Determine log directory: Desktop/PlaylistMerger Logs/YYYY-MM/
    let desktop_path = dirs::desktop_dir();
    log::info!("Desktop path resolved: {:?}", desktop_path.as_ref().map(|p| p.display().to_string()));

    let log_dir = desktop_path
        .map(|d| d.join("PlaylistMerger Logs").join(&month_folder))
        .unwrap_or_else(|| {
            // Fallback: output_dir/forensic_logs/YYYY-MM/
            log::warn!("Desktop not available, falling back to output_dir forensic logs");
            Path::new(output_dir)
                .join("forensic_logs")
                .join(&month_folder)
        });

    log::info!("Log directory: {}", log_dir.display());

    // Create directory with full error logging
    if let Err(e) = fs::create_dir_all(&log_dir) {
        let err_msg = format!("Failed to create forensic log directory {}: {}", log_dir.display(), e);
        log::error!("[CRITICAL] {}", err_msg);
        return Err(err_msg);
    }
    log::info!("Log directory created/opened successfully");

    // Log filename: <playlist_name>_<yyyy-mm-dd_HH-mm-ss>_<job_id>.log
    let log_filename = format!("{}_{}_{}.log", safe_name, ts_file, job_id);
    let log_path = log_dir.join(&log_filename);
    log::info!("Log file path: {}", log_path.display());

    // Open file for writing (create or truncate)
    let file = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
    {
        Ok(f) => {
            log::info!("Log file opened successfully");
            f
        }
        Err(e) => {
            let err_msg = format!("Failed to create forensic log file {}: {}", log_path.display(), e);
            log::error!("[CRITICAL] {}", err_msg);
            return Err(err_msg);
        }
    };

    let mut writer = BufWriter::new(file);

    // ── SYSTEM METADATA ──────────────────────────────────────────────
    // App version
    let app_version = env!("CARGO_PKG_VERSION");

    // Git commit hash (set by build.rs)
    let git_commit = option_env!("GIT_COMMIT_HASH").unwrap_or("N/A");

    // Build timestamp (set by build.rs)
    let build_ts = option_env!("BUILD_TIMESTAMP")
        .and_then(|s| s.parse::<i64>().ok())
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "N/A".to_string());

    // OS info
    let os_name = std::env::consts::OS;
    let os_arch = std::env::consts::ARCH;

    // Rust version (compile-time)
    let rust_version = option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("N/A");

    // ── HEADER ───────────────────────────────────────────────────────
    let mut write_err = || {
        writeln!(writer, "═══════════════════════════════════════════════════════════════════════").ok();
        writeln!(writer, " MERGE FORENSIC LOG").ok();
        writeln!(writer, "═══════════════════════════════════════════════════════════════════════").ok();
        writeln!(writer).ok();
        writeln!(writer, " Application Version:  {}", app_version).ok();
        writeln!(writer, " Git Commit:           {}", git_commit).ok();
        writeln!(writer, " Build Timestamp:      {}", build_ts).ok();
        writeln!(writer, " OS:                   {} ({})", os_name, os_arch).ok();
        writeln!(writer, " Rust Version:         {}", rust_version).ok();
        writeln!(writer).ok();
        writeln!(writer, "═══════════════════════════════════════════════════════════════════════").ok();
        writeln!(writer, " === MERGE START ===").ok();
        writeln!(writer, "═══════════════════════════════════════════════════════════════════════").ok();
        writeln!(writer).ok();
        writeln!(writer, " Job ID:      {}", job_id).ok();
        writeln!(writer, " Timestamp:   {}", ts_header).ok();
        writeln!(writer, " Mode:        {}", mode).ok();
        writeln!(writer, " Output Path: {}", output_path).ok();
        if let Some(ff) = ffmpeg_path {
            writeln!(writer, " FFmpeg Path: {}", ff).ok();
        }
        if let Some(mk) = mkvmerge_path {
            writeln!(writer, " MKVMerge:    {}", mk).ok();
        }
        writeln!(writer, " Input Count: {}", input_count).ok();
        writeln!(writer).ok();
        writeln!(writer, "---").ok();
        writeln!(writer).ok();
        writer.flush()
    };

    if let Err(e) = write_err() {
        let err_msg = format!("Failed to write forensic log header: {}", e);
        log::error!("[CRITICAL] {}", err_msg);
        return Err(err_msg);
    }

    // Store state in global
    let state = ForensicLogState {
        writer: Some(writer),
        log_path: log_path.clone(),
        start_time: std::time::Instant::now(),
        job_id: job_id.to_string(),
    };

    match FORENSIC_LOG.lock() {
        Ok(mut f) => {
            *f = Some(state);
            log::info!("Forensic log state stored in global: {}", log_path.display());
        }
        Err(poisoned) => {
            // Lock was poisoned - state is lost but file was created
            let err_msg = format!("FORENSIC LOG CRITICAL: Mutex poisoned, state not stored. File exists at {} but writes will fail. Mutex error: {:?}", log_path.display(), poisoned);
            log::error!("{}", err_msg);
            return Err(err_msg);
        }
    }

    log::info!("[ForensicLog] Active=true path=\"{}\"", log_path.display());
    log::info!("[INIT_COMPLETE] Forensic log started successfully at {}", log_path.display());

    Ok(log_path)
}

/// Write a line to the forensic log (for capturing merge logs in between start/end).
/// This is called by the integration points to copy log lines.
///
/// NOTE: Does NOT call eprintln!() — the global logger (JobLogger) already writes
/// every log::info!() call to stderr. Calling eprintln!() here would produce
/// duplicate stderr output for every line. BufWriter handles batching; explicit
/// flush is only done at critical points (header, footer, panic).
pub fn write_line(line: &str) {
    if let Ok(mut state) = FORENSIC_LOG.lock() {
        if let Some(ref mut s) = *state {
            if let Some(ref mut writer) = s.writer {
                let _ = writeln!(writer, "{}", line);
            }
        }
    }
}

/// End the forensic log with the given status and details.
/// Writes the footer and closes the file.
///
/// This automatically appends the current job log (all detailed logs)
/// before writing the end block, so all details are preserved.
pub fn end_forensic_log(
    status: ForensicStatus,
    error: Option<&str>,
    output_path: Option<&str>,
) {
    // First, append the full job log (all detailed logs)
    append_current_job_log();

    let (elapsed_secs, job_id) = match FORENSIC_LOG.lock() {
        Ok(mut state) => {
            if let Some(ref mut s) = *state {
                let elapsed = s.start_time.elapsed().as_secs_f64();
                let jid = s.job_id.clone();
                if let Some(ref mut writer) = s.writer {
                    let end_ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
                    let _ = writeln!(writer);
                    let _ = writeln!(writer, "---");
                    let _ = writeln!(writer);
                    let _ = writeln!(writer, "═══════════════════════════════════════════════════════════════════════");
                    let _ = writeln!(writer, " === MERGE END ===");
                    let _ = writeln!(writer, "═══════════════════════════════════════════════════════════════════════");
                    let _ = writeln!(writer);
                    let _ = writeln!(writer, " Final Status: {}", status.as_str());
                    let _ = writeln!(writer, " End Time:     {}", end_ts);
                    let _ = writeln!(writer, " Duration:     {:.1} seconds", elapsed);
                    if let Some(out) = output_path {
                        let _ = writeln!(writer, " Output:       {}", out);
                    }
                    if let Some(err) = error {
                        let _ = writeln!(writer, " Error:        {}", err);
                    }
                    let _ = writeln!(writer);
                    let _ = writeln!(writer, "═══════════════════════════════════════════════════════════════════════");
                    let _ = writer.flush();
                }
                // Close the writer with sync_all for forensic log durability.
                // into_inner() flushes BufWriter and returns the inner File,
                // then sync_all() ensures all data reaches physical disk.
                // Without this, a power loss after flush() but before OS writeback
                // would lose the MERGE END footer, leaving the log header-only.
                if let Some(writer) = s.writer.take() {
                    if let Ok(file) = writer.into_inner() {
                        let _ = file.sync_all();
                    }
                }
                (elapsed, jid)
            } else {
                log::error!("[end_forensic_log] No active forensic log state — cannot end (never started?)");
                return;
            }
        }
        Err(poisoned) => {
            log::error!("[end_forensic_log] FORENSIC LOG CRITICAL: Mutex poisoned: {:?}", poisoned);
            return;
        }
    };

    log::info!(
        "[END_COMPLETE] Forensic log ended: status={} duration={:.1}s jobId={}",
        status.as_str(),
        elapsed_secs,
        job_id
    );
}

/// Append a panic/crash block to an existing forensic log.
/// Call this when the watchdog detects a panic.
/// The forensic log stays open so the end block can still be written.
pub fn append_panic_block(panic_info: &str) {
    if let Ok(mut state) = FORENSIC_LOG.lock() {
        if let Some(ref mut s) = *state {
            if let Some(ref mut writer) = s.writer {
                let _ = writeln!(writer);
                let _ = writeln!(writer, "=== PANIC DETECTED ===");
                let _ = writeln!(writer, " Panic: {}", panic_info);
                let _ = writeln!(writer, " Timestamp: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
                let _ = writeln!(writer);
                let _ = writer.flush();
            }
        }
    }
}

/// Append the entire contents of a job log file to the forensic log.
/// This captures all detailed log output (FFmpeg commands, file processing,
/// timestamps, etc.) into the forensic log at the end of merge.
///
/// Call this before end_forensic_log() to include the full job log.
pub fn append_job_log(job_log_path: &Path) {
    log::info!("[ForensicLog] Reading job log from: {}", job_log_path.display());

    if let Ok(mut state) = FORENSIC_LOG.lock() {
        if let Some(ref mut s) = *state {
            if let Some(ref mut writer) = s.writer {
                let _ = writeln!(writer);
                let _ = writeln!(writer, "═══════════════════════════════════════════════════════════════════════");
                let _ = writeln!(writer, " === FULL JOB LOG (ALL DETAILS) ===");
                let _ = writeln!(writer, "═══════════════════════════════════════════════════════════════════════");
                let _ = writeln!(writer);

                match std::fs::read_to_string(job_log_path) {
                    Ok(content) => {
                        log::info!("[ForensicLog] Job log size: {} bytes", content.len());
                        let _ = writeln!(writer, "Job log: {}", job_log_path.display());
                        let _ = writeln!(writer, "Size: {} bytes", content.len());
                        let _ = writeln!(writer);
                        let _ = writeln!(writer, "--- CONTENT START ---");
                        let _ = writer.write_all(content.as_bytes());
                        let _ = writeln!(writer);
                        let _ = writeln!(writer, "--- CONTENT END ---");
                        log::info!("[ForensicLog] Job log content appended successfully");
                    }
                    Err(e) => {
                        log::error!("[ForensicLog] Failed to read job log: {}", e);
                        let _ = writeln!(writer, "Failed to read job log: {}", e);
                    }
                }

                let _ = writeln!(writer);
                let _ = writer.flush();
            } else {
                log::error!("[ForensicLog] No active writer in forensic log state");
            }
        } else {
            log::error!("[ForensicLog] No active forensic log state");
        }
    } else {
        log::error!("[ForensicLog] Failed to lock FORENSIC_LOG");
    }
}

/// Append the current job log to the forensic log.
/// This reads from the active logger and appends all content.
/// Call this before end_forensic_log() to include the full job log.
pub fn append_current_job_log() {
    match crate::logger::current_log_path() {
        Some(job_path) => {
            log::info!("[ForensicLog] Job log path found: {}", job_path.display());
            append_job_log(&job_path);
        }
        None => {
            log::warn!("[ForensicLog] No job log path available — forensic log will be incomplete");
            if let Ok(mut state) = FORENSIC_LOG.lock() {
                if let Some(ref mut s) = *state {
                    if let Some(ref mut writer) = s.writer {
                        let _ = writeln!(writer, "(No job log available — logger not active or already stopped)");
                    }
                }
            }
        }
    }
}

/// Get the path to the current forensic log file for external access.
pub fn get_current_log_path() -> Option<PathBuf> {
    match FORENSIC_LOG.lock() {
        Ok(state) => state.as_ref().map(|s| s.log_path.clone()),
        Err(_) => None,
    }
}

/// RAII guard that automatically calls `end_forensic_log` when dropped.
/// Ensures the forensic log file is always finalized, even on error/panic paths.
///
/// Variants:
/// - `new(status, output)`: finalizes with the given status on drop.
/// - `with_error(status, error, output)`: finalizes with status + error message on drop.
/// - `new_empty()`: no-op on drop — caller manually calls `end_forensic_log`.
/// - `new_auto_fail(output)`: finalizes with `Failed` on drop if not explicitly ended.
pub struct ForensicLogGuard {
    status: Option<ForensicStatus>,
    error: Option<String>,
    output_path: Option<String>,
    auto_fail_on_drop: bool,
}

impl ForensicLogGuard {
    pub fn new(status: ForensicStatus, output_path: Option<String>) -> Self {
        Self {
            status: Some(status),
            error: None,
            output_path,
            auto_fail_on_drop: false,
        }
    }

    pub fn with_error(status: ForensicStatus, error: String, output_path: Option<String>) -> Self {
        Self {
            status: Some(status),
            error: Some(error),
            output_path,
            auto_fail_on_drop: false,
        }
    }

    pub fn new_empty() -> Self {
        Self {
            status: None,
            error: None,
            output_path: None,
            auto_fail_on_drop: false,
        }
    }

    /// Create a guard that finalizes with `Failed` on drop if not explicitly ended.
    /// Useful for functions with multiple early returns — the guard ensures the log
    /// is always finalized, even on paths that don't call `end_forensic_log`.
    pub fn new_auto_fail(output_path: Option<String>) -> Self {
        Self {
            status: None,
            error: None,
            output_path,
            auto_fail_on_drop: true,
        }
    }
}

impl Drop for ForensicLogGuard {
    fn drop(&mut self) {
        if let Some(status) = self.status {
            end_forensic_log(status, self.error.as_deref(), self.output_path.as_deref());
        } else if self.auto_fail_on_drop {
            end_forensic_log(ForensicStatus::Failed, Some("Merge failed (early exit)"), self.output_path.as_deref());
        }
    }
}
