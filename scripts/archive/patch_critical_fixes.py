#!/usr/bin/env python3
"""Fix critical issues identified by code review."""

def fix_redundant_logging():
    """Remove write_raw calls where log::info! already captures the same data."""
    with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # In run_ffmpeg_cmd: remove the write_raw that duplicates log::info!
    # Keep log::info! (goes through logger → both stderr + file)
    # Remove write_raw (redundant — same data written twice to file)

    # Pattern: log::info + write_raw on consecutive lines for same message
    old1 = '''    // Log the full command line for job log capture
    log::info!("[FFMPEG_CMD] {} {}", ffmpeg_path.display(), args.join(" "));
    crate::logger::write_raw(&format!("[FFMPEG_CMD] {} {}\\n", ffmpeg_path.display(), args.join(" ")));'''

    new1 = '''    // Log the full command line for job log capture
    log::info!("[FFMPEG_CMD] {} {}", ffmpeg_path.display(), args.join(" "));'''

    while old1 in content:
        content = content.replace(old1, new1, 1)
        changes += 1
    print(f"Fixed {changes}x redundant FFMPEG_CMD logging in merge.rs")

    # Remove write_raw for FFMPEG_EXIT (log::info/error already captures it)
    old2 = '''                log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                crate::logger::write_raw("[FFMPEG_EXIT] Exit code: 0 (success)\\n");'''
    new2 = '''                log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");'''

    while old2 in content:
        content = content.replace(old2, new2, 1)
        changes += 1
    print(f"Fixed redundant FFMPEG_EXIT logging in merge.rs")

    # Remove write_raw for error paths (log::error! already captures it)
    old3 = '''                log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);
                crate::logger::write_raw(&format!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr:\\n{}\\n", status.code(), err));'''
    new3 = '''                log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);'''

    while old3 in content:
        content = content.replace(old3, new3, 1)
        changes += 1
    print(f"Fixed redundant error logging in merge.rs")

    # Remove write_raw for process error
    old4 = '            crate::logger::write_raw(&format!("[FFMPEG_EXIT] Process error: {}\\n", e));\n            Err(format!("FFmpeg process error: {}", e))'
    new4 = '            Err(format!("FFmpeg process error: {}", e))'

    while old4 in content:
        content = content.replace(old4, new4, 1)
        changes += 1
    print(f"Fixed redundant process error logging in merge.rs")

    # Remove write_raw for timeout (900s version)
    old5 = '            crate::logger::write_raw("[FFMPEG_EXIT] Timed out after 900 seconds\\n");\n            Err("FFmpeg process timed out after 900 seconds".to_string())'
    new5 = '            Err("FFmpeg process timed out after 900 seconds".to_string())'

    while old5 in content:
        content = content.replace(old5, new5, 1)
        changes += 1

    # Remove write_raw for cancel
    old6 = '            crate::logger::write_raw("[FFMPEG_EXIT] Cancelled by user\\n");\n            return Err("Merge cancelled by user".to_string());'
    new6 = '            return Err("Merge cancelled by user".to_string());'

    while old6 in content:
        content = content.replace(old6, new6, 1)
        changes += 1

    # Remove write_raw for timeout (dynamic version)
    old7 = '            crate::logger::write_raw(&format!("[FFMPEG_EXIT] Timed out after {} seconds\\n", timeout_secs));\n            return Err(format!("FFmpeg process timed out after {} seconds", timeout_secs));'
    new7 = '            return Err(format!("FFmpeg process timed out after {} seconds", timeout_secs));'

    while old7 in content:
        content = content.replace(old7, new7, 1)
        changes += 1

    # Remove write_raw for process error in cancel fn
    old8 = '                crate::logger::write_raw(&format!("[FFMPEG_EXIT] Process error: {}\\n", e));\n                return Err(format!("FFmpeg process error: {}", e));'
    new8 = '                return Err(format!("FFmpeg process error: {}", e));'

    while old8 in content:
        content = content.replace(old8, new8, 1)
        changes += 1

    # Fix: in run_ffmpeg_cmd_with_cancel, the success path
    old9 = '''                if status.success() {
                    log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                    crate::logger::write_raw("[FFMPEG_EXIT] Exit code: 0 (success)\\n");
                    return Ok(());'''
    new9 = '''                if status.success() {
                    log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                    return Ok(());'''

    while old9 in content:
        content = content.replace(old9, new9, 1)
        changes += 1

    # Fix: error path in cancel fn
    old10 = '''                    let err = String::from_utf8_lossy(&err_bytes);
                    log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);
                    crate::logger::write_raw(&format!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr:\\n{}\\n", status.code(), err));
                    return Err(err.into_owned());'''
    new10 = '''                    let err = String::from_utf8_lossy(&err_bytes);
                    log::error!("[FFMPEG_EXIT] Exit code: {:?}\\nStderr: {}", status.code(), err);
                    return Err(err.into_owned());'''

    while old10 in content:
        content = content.replace(old10, new10, 1)
        changes += 1

    with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"\nmerge.rs: {changes} redundant logging fixes applied")


def fix_log_path_preservation():
    """In stop_job_log, preserve log_path so Export button works after job completes."""
    with open('src-tauri/src/logger.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    # In stop_job_log, remove: state.log_path = None;
    # Keep log_path and logs_dir so current_log_path() works after job ends
    old_stop = '''            state.writer = None;
            state.log_path = None;'''
    new_stop = '''            state.writer = None;
            // Preserve log_path and logs_dir so Export button works after job ends.
            // They are overwritten by the next start_job_log call.'''

    if old_stop in content:
        content = content.replace(old_stop, new_stop, 1)
        print("FIX logger.rs: Preserved log_path after stop_job_log")
    else:
        print("SKIP logger.rs: Pattern not found")

    with open('src-tauri/src/logger.rs', 'w', encoding='utf-8') as f:
        f.write(content)


def fix_mkvmerge_redundant_logging():
    """Remove write_raw calls in mkvmerge that duplicate log::info!/log::warn!."""
    with open('src-tauri/src/ffmpeg/mkvmerge.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # Remove redundant write_raw for command line (log::info! already captures it)
    old_cmd = '''    // Log the full command line for job log capture
    log::info!("[MKVMERGE_CMD] {} --gui-mode -o {} {}", mkvmerge_path, output_path, input_files.join(" "));
    crate::logger::write_raw(&format!("[MKVMERGE_CMD] {} --gui-mode -o {} {}\\r\\n", mkvmerge_path, output_path, input_files.join(" ")));'''

    new_cmd = '''    // Log the full command line for job log capture
    log::info!("[MKVMERGE_CMD] {} --gui-mode -o {} {}", mkvmerge_path, output_path, input_files.join(" "));'''

    if old_cmd in content:
        content = content.replace(old_cmd, new_cmd, 1)
        changes += 1
        print("FIX mkvmerge.rs: Removed redundant MKVMERGE_CMD write_raw")

    # Remove redundant write_raw for exit code (log::info! already captures it)
    old_exit = '''    log::info!("[MKVMERGE_EXIT] Exit code: {}", exit_code);
    crate::logger::write_raw(&format!("[MKVMERGE_EXIT] Exit code: {}\\r\\n", exit_code));'''

    new_exit = '''    log::info!("[MKVMERGE_EXIT] Exit code: {}", exit_code);'''

    if old_exit in content:
        content = content.replace(old_exit, new_exit, 1)
        changes += 1
        print("FIX mkvmerge.rs: Removed redundant MKVMERGE_EXIT write_raw")

    # Remove redundant write_raw for stderr (log::warn! already captures it)
    old_stderr = '''            log::warn!("[MKVMERGE_STDERR] {}", stderr_str.trim());
            crate::logger::write_raw(&format!("[MKVMERGE_STDERR]\\r\\n{}\\r\\n", stderr_str));'''

    new_stderr = '''            log::warn!("[MKVMERGE_STDERR] {}", stderr_str.trim());'''

    if old_stderr in content:
        content = content.replace(old_stderr, new_stderr, 1)
        changes += 1
        print("FIX mkvmerge.rs: Removed redundant MKVMERGE_STDERR write_raw")

    with open('src-tauri/src/ffmpeg/mkvmerge.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"\nmkvmerge.rs: {changes} fixes applied")


def fix_mkvmerge_stderr_deadlock():
    """Read stderr in a separate thread to prevent pipe buffer full deadlock."""
    with open('src-tauri/src/ffmpeg/mkvmerge.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    # The current code reads stderr AFTER the stdout loop, which can deadlock
    # if stderr buffer fills up while we're reading stdout.
    # Fix: spawn a thread to drain stderr concurrently with stdout reading.

    old_stderr_block = '''    // Log stderr from mkvmerge
    {
        use std::io::Read;
        let mut stderr_bytes = Vec::new();
        let mut stderr_reader = std::io::BufReader::new(stderr);
        let _ = stderr_reader.read_to_end(&mut stderr_bytes);
        let stderr_str = String::from_utf8_lossy(&stderr_bytes);
        if !stderr_str.trim().is_empty() {
            log::warn!("[MKVMERGE_STDERR] {}", stderr_str.trim());
        }
    }

    let status = child.wait().map_err(|e| anyhow!("mkvmerge wait failed: {}", e))?;'''

    new_stderr_block = '''    // Drain stderr in a background thread to prevent pipe buffer full deadlock.
    // If stderr fills up (64KB default on Windows) while we read stdout,
    // mkvmerge blocks on stderr writes → deadlock.
    let stderr_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut reader = std::io::BufReader::new(stderr);
        let _ = reader.read_to_end(&mut buf);
        buf
    });

    let status = child.wait().map_err(|e| anyhow!("mkvmerge wait failed: {}", e))?;
    let exit_code = status.code().unwrap_or(-1);

    // Collect stderr from background thread
    let stderr_bytes = stderr_handle.join().unwrap_or_default();
    let stderr_str = String::from_utf8_lossy(&stderr_bytes);
    if !stderr_str.trim().is_empty() {
        log::warn!("[MKVMERGE_STDERR] {}", stderr_str.trim());
    }'''

    if old_stderr_block in content:
        content = content.replace(old_stderr_block, new_stderr_block, 1)
        print("FIX mkvmerge.rs: Fixed stderr deadlock with background thread")
    else:
        print("SKIP mkvmerge stderr: Pattern not found")

    with open('src-tauri/src/ffmpeg/mkvmerge.rs', 'w', encoding='utf-8') as f:
        f.write(content)


if __name__ == '__main__':
    fix_redundant_logging()
    print()
    fix_log_path_preservation()
    print()
    fix_mkvmerge_redundant_logging()
    print()
    fix_mkvmerge_stderr_deadlock()
    print("\nAll critical fixes applied!")
