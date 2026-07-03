use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::io::BufRead;
use anyhow::{Result, anyhow};
use crate::types::{MergeProgress, MergePhase};
use crate::ffmpeg::cleanup_partial_output;
use std::time::{Duration, Instant};

/// Locates the mkvmerge binary across common absolute locations or system PATH.
pub fn find_mkvmerge() -> Option<String> {
    let candidates = if cfg!(windows) {
        vec![
            "C:\\Program Files\\MKVToolNix\\mkvmerge.exe",
            "C:\\Program Files (x86)\\MKVToolNix\\mkvmerge.exe",
        ]
    } else {
        vec![
            "/usr/bin/mkvmerge",
            "/usr/local/bin/mkvmerge",
            "/opt/homebrew/bin/mkvmerge",
        ]
    };

    for path in &candidates {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }

    // Try PATH lookup
    if Command::new("mkvmerge")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Some("mkvmerge".to_string());
    }

    None
}

/// Executes zero-copy multiplexing via mkvmerge using stream copy rules.
/// Falls back to FFmpeg if mkvmerge is not available.
pub fn run_mkvmerge(
    mkvmerge_path: &str,
    input_files: &[String],
    output_path: &str,
    total_duration: f64,
    cancel_flag: Arc<AtomicBool>,
    on_progress: impl Fn(MergeProgress) + Send + 'static,
) -> Result<()> {
    let mut cmd = Command::new(mkvmerge_path);

    // --gui-mode forces machine-readable newline-delimited output (\n)
    // instead of terminal carriage returns (\r) that cause BufReader deadlocks
    cmd.arg("--gui-mode");

    // Do NOT strip \\?\ extended-length prefixes. mkvmerge supports Win32 long paths natively.
    cmd.arg("-o").arg(output_path);
    for file in input_files {
        cmd.arg(file);
    }

    // Log the full command line for job log capture
    log::info!("[MKVMERGE_CMD] {} --gui-mode -o {} {}", mkvmerge_path, output_path, input_files.join(" "));

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn mkvmerge: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let progress_callback = on_progress;

    // Drain stdout (progress parsing) and stderr in background threads.
    // This prevents pipe buffer full deadlock (64KB default on Windows).
    let progress_lines = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let progress_lines_clone = progress_lines.clone();
    let stdout_handle = std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            let line = line.unwrap_or_default();
            // Log all mkvmerge stdout to job log
            crate::logger::write_raw(&format!("[MKVMERGE_STDOUT] {}\n", line));
            if let Ok(mut lines) = progress_lines_clone.lock() {
                lines.push(line);
            }
        }
    });

    let stderr_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut reader = std::io::BufReader::new(stderr);
        let _ = reader.read_to_end(&mut buf);
        buf
    });

    // Poll try_wait with timeout and cancellation on the main thread.
    // This prevents infinite blocking if mkvmerge hangs.
    const MKVMERGE_TIMEOUT_SECS: u64 = 21600; // 6 hours for large merges
    let start = Instant::now();
    let status = loop {
        if start.elapsed().as_secs() > MKVMERGE_TIMEOUT_SECS {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(output_path), false);
            return Err(anyhow!("mkvmerge timed out after {} seconds", MKVMERGE_TIMEOUT_SECS));
        }
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(output_path), false);
            return Err(anyhow!("Operation Cancelled"));
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                // Process any collected progress lines from the background thread
                if let Ok(mut lines) = progress_lines.lock() {
                    for line in lines.drain(..) {
                        if line.starts_with("#GUI#progress ") {
                            if let Some(pct_str) = line.strip_prefix("#GUI#progress ").and_then(|s| s.strip_suffix('%')) {
                                if let Ok(pct) = pct_str.trim().parse::<f32>() {
                                    let pct_f64 = pct as f64;
                                    let current_time = (pct_f64 / 100.0) * total_duration;
                                    let remaining = total_duration - current_time;
                                    progress_callback(MergeProgress {
                                        percent: pct,
                                        current_time,
                                        total_duration,
                                        speed: None,
                                        fps: None,
                                        current_file: None,
                                        current_segment_index: None,
                                        remaining_duration: Some(remaining),
                                        bytes_written: None,
                                        eta_seconds: None,
                                        phase: MergePhase::Writing,
                                        overall_percent: Some(15.0 + (pct * 0.80)),
                                        stage_name: Some("Merging MKV (Zero-Copy)...".to_string()),
                                        stage_percent: Some(pct),
                                        current_file_index: None,
                                        total_files_in_stage: None,
                                        warning: None,
                                        is_large_playlist: false,
                                    });
                                }
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("mkvmerge wait error: {}", e));
            }
        }
    };

    let exit_code = status.code().unwrap_or(-1);
    log::info!("[MKVMERGE_EXIT] Exit code: {}", exit_code);

    // Collect stderr from background thread
    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
    if !stderr_str.trim().is_empty() {
        log::warn!("[MKVMERGE_STDERR] {}", stderr_str.trim());
    }

    // Collect stdout from background thread (for any final lines)
    let _ = stdout_handle.join();

    // mkvmerge exit codes: 0 = success, 1 = warnings (OK), 2 = fatal error
    if status.code() == Some(2) {
        cleanup_partial_output(Path::new(output_path), false);
        return Err(anyhow!("mkvmerge exited with fatal error (Code 2)"));
    } else if status.code() != Some(0) && status.code() != Some(1) {
        cleanup_partial_output(Path::new(output_path), false);
        return Err(anyhow!("mkvmerge exited with unexpected code {:?}", status.code()));
    }

    // Emit 100% completion so the frontend knows mkvmerge finished successfully.
    // Without this, the UI stays stuck at whatever percentage mkvmerge last reported
    // (typically 99%), causing "merge shown 100% but still waiting" perception.
    let output_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
    progress_callback(MergeProgress {
        percent: 100.0,
        current_time: total_duration,
        total_duration,
        speed: None,
        fps: None,
        current_file: None,
        current_segment_index: None,
        remaining_duration: Some(0.0),
        bytes_written: Some(output_size),
        eta_seconds: Some(0.0),
        phase: MergePhase::Writing,
        overall_percent: Some(100.0),
        stage_name: Some("MKV merge complete (zero-copy)".to_string()),
        stage_percent: Some(100.0),
        current_file_index: None,
        total_files_in_stage: None,
        warning: None,
        is_large_playlist: false,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_mkvmerge() {
        let _ = find_mkvmerge();
    }
}
