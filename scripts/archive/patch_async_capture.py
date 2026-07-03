#!/usr/bin/env python3
"""Fix FFmpeg capture to use tokio async I/O instead of std sync I/O.
tokio::process::ChildStderr/ChildStdout require AsyncRead, not std::io::Read."""

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

changes = 0

# ── Fix run_ffmpeg_cmd: Replace std::thread::spawn with tokio::spawn ──
old_ffmpeg_threads = '''    // Drain stderr and stdout in background threads to capture all output.
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
    // This is safer for large/high-resolution files on slower CPUs'''

new_ffmpeg_threads = '''    // Drain stderr and stdout in background tasks to capture all output.
    // Uses tokio::spawn (not std::thread) because these are tokio async handles.
    // This also prevents pipe buffer full deadlock on Windows (64KB buffer).
    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();
    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(mut h) = stderr_handle {
            let _ = h.read_to_end(&mut buf).await;
        }
        buf
    });
    let stdout_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(mut h) = stdout_handle {
            let _ = h.read_to_end(&mut buf).await;
        }
        buf
    });

    // 15 minute timeout (900s) for normalization/processing tasks
    // This is safer for large/high-resolution files on slower CPUs'''

# Find this in run_ffmpeg_cmd (before fn2)
fn2_pos = content.find('async fn run_ffmpeg_cmd_with_cancel(')
if fn2_pos > 0:
    pos = content.find(old_ffmpeg_threads)
    if pos >= 0 and pos < fn2_pos:
        content = content[:pos] + new_ffmpeg_threads + content[pos + len(old_ffmpeg_threads):]
        changes += 1
        print("PATCH run_ffmpeg_cmd: Fixed to use tokio::spawn")
    else:
        print("SKIP run_ffmpeg_cmd threads: Pattern not found before fn2")

# Also fix the result collection in run_ffmpeg_cmd
old_collect1 = '''            let stderr_bytes = stderr_thread.join().unwrap_or_default();
            let stderr_str = String::from_utf8_lossy(&stderr_bytes);
            let stdout_bytes = stdout_thread.join().unwrap_or_default();'''

new_collect1 = '''            let stderr_bytes = stderr_task.await.unwrap_or_default();
            let stderr_str = String::from_utf8_lossy(&stderr_bytes);
            let stdout_bytes = stdout_task.await.unwrap_or_default();'''

pos = content.find(old_collect1)
if pos >= 0 and (fn2_pos < 0 or pos < fn2_pos):
    content = content[:pos] + new_collect1 + content[pos + len(old_collect1):]
    changes += 1
    print("PATCH run_ffmpeg_cmd: Fixed task join to await")
else:
    print("SKIP run_ffmpeg_cmd collect: Pattern not found")

# ── Fix run_ffmpeg_cmd_with_cancel: Replace std::thread::spawn with tokio::spawn ──
old_cancel_threads = '''    // Drain stderr and stdout in background threads to capture all output.
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
    });'''

new_cancel_threads = '''    // Drain stderr and stdout in background tasks to capture all output.
    // Uses tokio::spawn (not std::thread) because these are tokio async handles.
    // This prevents pipe buffer full deadlock and captures FFmpeg progress info.
    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();
    
    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(mut h) = stderr_handle {
            let _ = h.read_to_end(&mut buf).await;
        }
        buf
    });
    let stdout_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(mut h) = stdout_handle {
            let _ = h.read_to_end(&mut buf).await;
        }
        buf
    });'''

pos = content.find(old_cancel_threads, fn2_pos) if fn2_pos > 0 else -1
if pos >= 0:
    content = content[:pos] + new_cancel_threads + content[pos + len(old_cancel_threads):]
    changes += 1
    print("PATCH cancel: Fixed to use tokio::spawn")
else:
    print("SKIP cancel threads: Pattern not found")

# Fix the collect in cancel function
old_cancel_collect = '''                    let stderr_bytes = stderr_thread.and_then(|t| t.join().ok()).unwrap_or_default();
                    let err = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_bytes = stdout_thread.and_then(|t| t.join().ok()).unwrap_or_default();'''

new_cancel_collect = '''                    let stderr_bytes = stderr_task.await.unwrap_or_default();
                    let err = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_bytes = stdout_task.await.unwrap_or_default();'''

pos = content.find(old_cancel_collect, fn2_pos) if fn2_pos > 0 else -1
if pos >= 0:
    content = content[:pos] + new_cancel_collect + content[pos + len(old_cancel_collect):]
    changes += 1
    print("PATCH cancel: Fixed error collect to use await")
else:
    print("SKIP cancel error collect: Pattern not found")

# Also fix the success collect in cancel function
old_cancel_success_collect = '''                    let stderr_bytes = stderr_thread.and_then(|t| t.join().ok()).unwrap_or_default();
                    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_bytes = stdout_thread.and_then(|t| t.join().ok()).unwrap_or_default();'''

new_cancel_success_collect = '''                    let stderr_bytes = stderr_task.await.unwrap_or_default();
                    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_bytes = stdout_task.await.unwrap_or_default();'''

pos = content.find(old_cancel_success_collect, fn2_pos) if fn2_pos > 0 else -1
if pos >= 0:
    content = content[:pos] + new_cancel_success_collect + content[pos + len(old_cancel_success_collect):]
    changes += 1
    print("PATCH cancel: Fixed success collect to use await")
else:
    print("SKIP cancel success collect: Pattern not found")

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print(f"\nmerge.rs: {changes} async fixes applied")
