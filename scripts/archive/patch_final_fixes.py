#!/usr/bin/env python3
"""Fix remaining critical issues:
1. FFmpeg stderr/stdout capture on success (background threads)
2. Logger concurrency guard (prevent concurrent job log corruption)
"""

def fix_logger_concurrency():
    """Add an active_job_id field to prevent concurrent job log corruption."""
    with open('src-tauri/src/logger.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # Add active_job_id to LoggerState
    old_state = '''struct LoggerState {
    /// The active job log writer (None when no job is running).
    writer: Option<BufWriter<File>>,
    /// Path to the current log file (for export/copy operations).
    log_path: Option<PathBuf>,
    /// Directory where logs are stored.
    logs_dir: Option<PathBuf>,
}'''

    new_state = '''struct LoggerState {
    /// The active job log writer (None when no job is running).
    writer: Option<BufWriter<File>>,
    /// Path to the current log file (for export/copy operations).
    log_path: Option<PathBuf>,
    /// Directory where logs are stored.
    logs_dir: Option<PathBuf>,
    /// ID of the currently active job (prevents concurrent job log corruption).
    active_job_id: Option<String>,
}'''

    if old_state in content:
        content = content.replace(old_state, new_state, 1)
        changes += 1
        print("Added active_job_id to LoggerState")

    # Update init to include active_job_id
    old_init = '''        state: Mutex::new(LoggerState {
            writer: None,
            log_path: None,
            logs_dir: None,
        }),'''

    new_init = '''        state: Mutex::new(LoggerState {
            writer: None,
            log_path: None,
            logs_dir: None,
            active_job_id: None,
        }),'''

    if old_init in content:
        content = content.replace(old_init, new_init, 1)
        changes += 1
        print("Updated init with active_job_id")

    # Update start_job_log to set active_job_id and reject if already active
    old_start = '''    // Set up the global logger state
    if let Some(logger) = LOGGER.get() {
        if let Ok(mut state) = logger.state.lock() {
            state.writer = Some(writer);
            state.log_path = Some(log_path.clone());
            state.logs_dir = Some(logs_dir.clone());
        }
    }'''

    new_start = '''    // Set up the global logger state
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
    }'''

    if old_start in content:
        content = content.replace(old_start, new_start, 1)
        changes += 1
        print("Updated start_job_log with concurrency guard")

    # Update stop_job_log to clear active_job_id
    old_stop_writer = '            state.writer = None;\n            // Preserve log_path and logs_dir so Export button works after job ends.'
    new_stop_writer = '            state.writer = None;\n            state.active_job_id = None;\n            // Preserve log_path and logs_dir so Export button works after job ends.'

    if old_stop_writer in content:
        content = content.replace(old_stop_writer, new_stop_writer, 1)
        changes += 1
        print("Updated stop_job_log to clear active_job_id")

    with open('src-tauri/src/logger.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"\nlogger.rs: {changes} concurrency fixes applied")


def fix_ffmpeg_stderr_capture():
    """Add background stderr/stdout capture to run_ffmpeg_cmd and run_ffmpeg_cmd_with_cancel."""
    with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # ── Fix run_ffmpeg_cmd: Add background stderr/stdout capture ──
    # Find the function and its stderr_handle pattern
    fn1_start = content.find('async fn run_ffmpeg_cmd(')
    fn2_start = content.find('async fn run_ffmpeg_cmd_with_cancel(')

    if fn1_start >= 0 and fn2_start > fn1_start:
        # In run_ffmpeg_cmd: replace the stderr take + timeout block
        fn1_region = content[fn1_start:fn2_start]
        
        # Replace: take stderr, wait, read on failure
        # With: spawn background threads for stderr+stdout, wait, join threads, log
        old_ffmpeg1 = '''    // Take the stderr handle BEFORE wait() so we can read from it
    // after the process exits. On Windows, calling wait() then
    // wait_with_output() can return empty/broken pipe data.
    let stderr_handle = child.stderr.take();

    // 15 minute timeout (900s) for normalization/processing tasks
    // This is safer for large/high-resolution files on slower CPUs
    match timeout(Duration::from_secs(900), child.wait()).await {
        Ok(Ok(status)) => {
            if status.success() {
                log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                Ok(())
            } else {
                // Read stderr directly from the pipe handle we took earlier
                // (on Windows, calling wait() then wait_with_output() can return
                // stale/empty pipe data, so we take ownership of the handle first)
                let mut err_bytes = Vec::new();
                if let Some(mut stderr_reader) = stderr_handle {
                    let _ = stderr_reader.read_to_end(&mut err_bytes).await;
                }
                let err = String::from_utf8_lossy(&err_bytes);
                log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);
                Err(err.into_owned())
            }
        }
        Ok(Err(e)) => {
            Err(format!("FFmpeg process error: {}", e))
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err("FFmpeg process timed out after 900 seconds".to_string())
        }
    }'''

        new_ffmpeg1 = '''    // Drain stderr and stdout in background threads to capture all output.
    // This also prevents pipe buffer full deadlock on Windows (64KB buffer).
    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();
    
    let stderr_thread = stderr_handle.map(|h| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let mut reader = std::io::BufReader::new(h);
            let _ = reader.read_to_end(&mut buf);
            buf
        })
    });
    let stdout_thread = stdout_handle.map(|h| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let mut reader = std::io::BufReader::new(h);
            let _ = reader.read_to_end(&mut buf);
            buf
        })
    });

    // 15 minute timeout (900s) for normalization/processing tasks
    // This is safer for large/high-resolution files on slower CPUs
    match timeout(Duration::from_secs(900), child.wait()).await {
        Ok(Ok(status)) => {
            // Collect stderr from background thread
            let stderr_bytes = stderr_thread.and_then(|t| t.join().ok()).unwrap_or_default();
            let stderr_str = String::from_utf8_lossy(&stderr_bytes);
            
            // Collect stdout from background thread
            let stdout_bytes = stdout_thread.and_then(|t| t.join().ok()).unwrap_or_default();
            let stdout_str = String::from_utf8_lossy(&stdout_bytes);
            
            if status.success() {
                log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                if !stderr_str.trim().is_empty() {
                    log::info!("[FFMPEG_STDERR] {}", stderr_str.lines().take(20).collect::<Vec<_>>().join("\\n"));
                }
                if !stdout_str.trim().is_empty() {
                    log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\\n"));
                }
                Ok(())
            } else {
                log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), stderr_str);
                Err(stderr_str.into_owned())
            }
        }
        Ok(Err(e)) => {
            Err(format!("FFmpeg process error: {}", e))
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err("FFmpeg process timed out after 900 seconds".to_string())
        }
    }'''

        if old_ffmpeg1 in content:
            content = content.replace(old_ffmpeg1, new_ffmpeg1, 1)
            changes += 1
            print("PATCH run_ffmpeg_cmd: Added background stderr/stdout capture")
        else:
            print("SKIP run_ffmpeg_cmd: Pattern not found")

    # ── Fix run_ffmpeg_cmd_with_cancel: Add background stderr/stdout capture ──
    # Find the stderr take in the cancel function
    fn2_start = content.find('async fn run_ffmpeg_cmd_with_cancel(')
    if fn2_start >= 0:
        # Find the stderr_handle = child.stderr.take() in this function
        stderr_take_pos = content.find('let stderr_handle = child.stderr.take();', fn2_start)
        if stderr_take_pos >= 0:
            # Replace: single stderr take
            # With: background threads for stderr + stdout
            old_cancel_stderr = '    let stderr_handle = child.stderr.take();\n    let start = Instant::now();'
            new_cancel_stderr = '''    // Drain stderr and stdout in background threads to capture all output.
    // This prevents pipe buffer full deadlock and captures FFmpeg progress info.
    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();
    
    let stderr_thread = stderr_handle.map(|h| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let mut reader = std::io::BufReader::new(h);
            let _ = reader.read_to_end(&mut buf);
            buf
        })
    });
    let stdout_thread = stdout_handle.map(|h| {
        std::thread::spawn(move || {
            use std::io::Read;
            let mut buf = Vec::new();
            let mut reader = std::io::BufReader::new(h);
            let _ = reader.read_to_end(&mut buf);
            buf
        })
    });
    let start = Instant::now();'''

            if old_cancel_stderr in content[fn2_start:]:
                # Only replace in the cancel function
                pre = content[:fn2_start]
                post = content[fn2_start:]
                post = post.replace(old_cancel_stderr, new_cancel_stderr, 1)
                content = pre + post
                changes += 1
                print("PATCH run_ffmpeg_cmd_with_cancel: Added background stderr/stdout capture")
            else:
                print("SKIP cancel stderr: Pattern not found")

            # Now fix the success/error paths in the cancel function to join threads
            # Success path: find "if status.success()" after fn2_start
            old_cancel_success = '''                if status.success() {
                    log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                    return Ok(());'''

            new_cancel_success = '''                if status.success() {
                    // Collect stderr/stdout from background threads
                    let stderr_bytes = stderr_thread.and_then(|t| t.join().ok()).unwrap_or_default();
                    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_bytes = stdout_thread.and_then(|t| t.join().ok()).unwrap_or_default();
                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                    log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                    if !stderr_str.trim().is_empty() {
                        log::info!("[FFMPEG_STDERR] {}", stderr_str.lines().take(20).collect::<Vec<_>>().join("\\n"));
                    }
                    if !stdout_str.trim().is_empty() {
                        log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\\n"));
                    }
                    return Ok(());'''

            # Find this pattern after fn2_start
            pos = content.find(old_cancel_success, fn2_start)
            if pos >= 0:
                content = content[:pos] + new_cancel_success + content[pos + len(old_cancel_success):]
                changes += 1
                print("PATCH cancel: Fixed success path to collect threads")
            else:
                print("SKIP cancel success: Pattern not found")

            # Error path: find the stderr read in cancel function error path
            old_cancel_error = '''                } else {
                    let mut err_bytes = Vec::new();
                    if let Some(mut stderr_reader) = stderr_handle {
                        let _ = stderr_reader.read_to_end(&mut err_bytes).await;
                    }
                    let err = String::from_utf8_lossy(&err_bytes);
                    log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);
                    return Err(err.into_owned());'''

            new_cancel_error = '''                } else {
                    // Collect stderr/stdout from background threads
                    let stderr_bytes = stderr_thread.and_then(|t| t.join().ok()).unwrap_or_default();
                    let err = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_bytes = stdout_thread.and_then(|t| t.join().ok()).unwrap_or_default();
                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                    log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);
                    if !stdout_str.trim().is_empty() {
                        log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\\n"));
                    }
                    return Err(err.into_owned());'''

            pos = content.find(old_cancel_error, fn2_start)
            if pos >= 0:
                content = content[:pos] + new_cancel_error + content[pos + len(old_cancel_error):]
                changes += 1
                print("PATCH cancel: Fixed error path to collect threads")
            else:
                print("SKIP cancel error: Pattern not found")

    with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"\nmerge.rs: {changes} FFmpeg capture fixes applied")


if __name__ == '__main__':
    fix_logger_concurrency()
    print()
    fix_ffmpeg_stderr_capture()
    print("\nAll final fixes applied!")
