#!/usr/bin/env python3
"""Fix run_ffmpeg_cmd and cancel function error paths in merge.rs.
Reads the actual file content and makes precise edits."""

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

changes = 0

# ── Fix run_ffmpeg_cmd: Replace old stderr handling with background threads ──
# Find the exact block in run_ffmpeg_cmd (before run_ffmpeg_cmd_with_cancel)
fn2_start = content.find('async fn run_ffmpeg_cmd_with_cancel(')

# In run_ffmpeg_cmd region, find the old pattern
old_block = '''    // Take the stderr handle BEFORE wait() so we can read from it
    // after the process exits. On Windows, calling wait() then
    // wait_with_output() can return empty/broken pipe data.
    let stderr_handle = child.stderr.take();

    // 15 minute timeout (900s) for normalization/processing tasks
    // This is safer for large/high-resolution files on slower CPUs
    match timeout(Duration::from_secs(900), child.wait()).await {
        Ok(Ok(status)) => {
            if status.success() {
                log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                crate::logger::write_raw("[FFMPEG_EXIT] Exit code: 0 (success)
");
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
                log::error!("[FFMPEG_EXIT] Exit code: {:?}
Stderr: {}", status.code(), err);
                crate::logger::write_raw(&format!("[FFMPEG_EXIT] Exit code: {:?}
Stderr:
{}
", status.code(), err));
                Err(err.into_owned())
            }
        }
        Ok(Err(e)) => {
            crate::logger::write_raw(&format!("[FFMPEG_EXIT] Process error: {}
", e));
            Err(format!("FFmpeg process error: {}", e))
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            crate::logger::write_raw("[FFMPEG_EXIT] Timed out after 900 seconds
");
            Err("FFmpeg process timed out after 900 seconds".to_string())
        }
    }'''

new_block = '''    // Drain stderr and stdout in background threads to capture all output.
    // This prevents pipe buffer full deadlock and captures FFmpeg progress info.
    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();
    let stderr_thread = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut r) = stderr_handle {
            let _ = r.read_to_end(&mut buf);
        }
        buf
    });
    let stdout_thread = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Some(mut r) = stdout_handle {
            let _ = r.read_to_end(&mut buf);
        }
        buf
    });

    // 15 minute timeout (900s) for normalization/processing tasks
    // This is safer for large/high-resolution files on slower CPUs
    match timeout(Duration::from_secs(900), child.wait()).await {
        Ok(Ok(status)) => {
            let stderr_bytes = stderr_thread.join().unwrap_or_default();
            let stderr_str = String::from_utf8_lossy(&stderr_bytes);
            let stdout_bytes = stdout_thread.join().unwrap_or_default();
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
                if !stdout_str.trim().is_empty() {
                    log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\\n"));
                }
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

if old_block in content:
    content = content.replace(old_block, new_block, 1)
    changes += 1
    print("PATCH run_ffmpeg_cmd: Added background stderr/stdout capture")
else:
    print("SKIP run_ffmpeg_cmd: Exact pattern not found, trying line-by-line fix")
    # Fallback: find by line numbers
    lines = content.split('\n')
    # Find the line "let stderr_handle = child.stderr.take();" before fn2_start
    for i, line in enumerate(lines):
        if 'let stderr_handle = child.stderr.take();' in line:
            pos = content.find(line)
            if pos < fn2_start or fn2_start < 0:
                # This is in run_ffmpeg_cmd - replace from here
                print(f"  Found stderr take at char position {pos} (line {i+1})")
                break

# ── Fix run_ffmpeg_cmd_with_cancel error path ──
# The cancel function already has background threads but the error path
# still references the old stderr_handle
fn2_start = content.find('async fn run_ffmpeg_cmd_with_cancel(')
if fn2_start >= 0:
    # Find the error path that still uses stderr_handle
    old_error = '''                } else {
                    let mut err_bytes = Vec::new();
                    if let Some(mut stderr_reader) = stderr_handle {
                        let _ = stderr_reader.read_to_end(&mut err_bytes).await;
                    }
                    let err = String::from_utf8_lossy(&err_bytes);
                    log::error!("[FFMPEG_EXIT] Exit code: {:?}
Stderr: {}", status.code(), err);
                    return Err(err.into_owned());'''

    new_error = '''                } else {
                    let stderr_bytes = stderr_thread.and_then(|t| t.join().ok()).unwrap_or_default();
                    let err = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_bytes = stdout_thread.and_then(|t| t.join().ok()).unwrap_or_default();
                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                    log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);
                    if !stdout_str.trim().is_empty() {
                        log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\\n"));
                    }
                    return Err(err.into_owned());'''

    pos = content.find(old_error, fn2_start)
    if pos >= 0:
        content = content[:pos] + new_error + content[pos + len(old_error):]
        changes += 1
        print("PATCH cancel: Fixed error path to use background threads")
    else:
        print("SKIP cancel error: Pattern not found")

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print(f"\nmerge.rs: {changes} fixes applied")
