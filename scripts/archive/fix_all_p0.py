#!/usr/bin/env python3
"""Fix mkvmerge.rs: Add try_wait polling with timeout to run_mkvmerge()."""
import os

BASE = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'src-tauri', 'src', 'ffmpeg')
path = os.path.join(BASE, 'mkvmerge.rs')

with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Add use for std::time::Duration and std::time::Instant after the existing use lines
old_uses = "use crate::ffmpeg::cleanup_partial_output;"
new_uses = "use crate::ffmpeg::cleanup_partial_output;\nuse std::time::{Duration, Instant};"
if old_uses in content:
    content = content.replace(old_uses, new_uses)
    print("Added Duration/Instant imports")
else:
    print("WARNING: Could not find cleanup_partial_output import")

# 2. Replace the run_mkvmerge function body after the spawn block
# Find the key sections we need to replace

# The stdout reader + stderr drainer + child.wait() block
# We need to replace from "let stdout = child.stdout.take()..." through to the end of the function
# down to before the test module

old_run_body = """    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = std::io::BufReader::new(stdout);

    // Drain stderr in a background thread to prevent pipe buffer full deadlock.
    // If stderr fills up (64KB default on Windows) while we read stdout,
    // mkvmerge blocks on stderr writes → deadlock.
    let stderr_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut reader = std::io::BufReader::new(stderr);
        let _ = reader.read_to_end(&mut buf);
        buf
    });

    for line in reader.lines() {
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(output_path));
            return Err(anyhow!("Operation Cancelled"));
        }

        let line = line.unwrap_or_default();

        // Log all mkvmerge stdout to job log
        crate::logger::write_raw(&format!("[MKVMERGE_STDOUT] {}\\n", line));

        // Parse progress from --gui-mode: "#GUI#progress 42%"
        if line.starts_with("#GUI#progress ") {
            if let Some(pct_str) = line.strip_prefix("#GUI#progress ").and_then(|s| s.strip_suffix('%')) {
                if let Ok(pct) = pct_str.trim().parse::<f32>() {
                    let pct_f64 = pct as f64;
                    let current_time = (pct_f64 / 100.0) * total_duration;
                    let remaining = total_duration - current_time;

                    on_progress(MergeProgress {
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

    let status = child.wait().map_err(|e| anyhow!("mkvmerge wait failed: {}", e))?;
    let exit_code = status.code().unwrap_or(-1);
    log::info!("[MKVMERGE_EXIT] Exit code: {}", exit_code);

    // Collect stderr from background thread
    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
    if !stderr_str.trim().is_empty() {
        log::warn!("[MKVMERGE_STDERR] {}", stderr_str.trim());
    }

    // mkvmerge exit codes: 0 = success, 1 = warnings (OK), 2 = fatal error
    if status.code() == Some(2) {
        cleanup_partial_output(Path::new(output_path));
        return Err(anyhow!("mkvmerge exited with fatal error (Code 2)"));
    } else if status.code() != Some(0) && status.code() != Some(1) {
        cleanup_partial_output(Path::new(output_path));
        return Err(anyhow!("mkvmerge exited with unexpected code {:?}", status.code()));
    }

    // Emit 100% completion so the frontend knows mkvmerge finished successfully.
    // Without this, the UI stays stuck at whatever percentage mkvmerge last reported
    // (typically 99%), causing "merge shown 100% but still waiting" perception.
    let output_size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
    on_progress(MergeProgress {
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

    Ok(())"""

new_run_body = """    let stdout = child.stdout.take().unwrap();
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
            crate::logger::write_raw(&format!("[MKVMERGE_STDOUT] {}\\n", line));
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
            cleanup_partial_output(Path::new(output_path));
            return Err(anyhow!("mkvmerge timed out after {} seconds", MKVMERGE_TIMEOUT_SECS));
        }
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(output_path));
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
        cleanup_partial_output(Path::new(output_path));
        return Err(anyhow!("mkvmerge exited with fatal error (Code 2)"));
    } else if status.code() != Some(0) && status.code() != Some(1) {
        cleanup_partial_output(Path::new(output_path));
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

    Ok(())"""

if old_run_body in content:
    content = content.replace(old_run_body, new_run_body)
    print("Applied try_wait polling with timeout to run_mkvmerge()")
else:
    print("WARNING: Could not find the run_mkvmerge body to replace")

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print(f"Updated {path}")
