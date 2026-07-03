#!/usr/bin/env python3
"""Patch FFmpeg/mkvmerge command execution to capture child process output in job logs."""

import re

def patch_merge_rs():
    with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # ── run_ffmpeg_cmd: Add command logging after CREATE_NO_WINDOW ──
    old_ffmpeg_cmd = '''    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd
        .args(args)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg: {}", e))?;

    // Take the stderr handle BEFORE wait() so we can read from it'''

    new_ffmpeg_cmd = '''    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Log the full command line for job log capture
    log::info!("[FFMPEG_CMD] {} {}", ffmpeg_path.display(), args.join(" "));
    crate::logger::write_raw(&format!("[FFMPEG_CMD] {} {}\n", ffmpeg_path.display(), args.join(" ")));

    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd
        .args(args)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg: {}", e))?;

    // Take the stderr handle BEFORE wait() so we can read from it'''

    if old_ffmpeg_cmd in content:
        # Only replace the first occurrence (run_ffmpeg_cmd)
        content = content.replace(old_ffmpeg_cmd, new_ffmpeg_cmd, 1)
        changes += 1
        print("PATCH run_ffmpeg_cmd: Added command logging")
    else:
        print("SKIP run_ffmpeg_cmd: Pattern not found (already patched?)")

    # ── run_ffmpeg_cmd: Add exit code logging on success ──
    old_success = '''            if status.success() {
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
                Err(err.into_owned())'''

    new_success = '''            if status.success() {
                log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                crate::logger::write_raw("[FFMPEG_EXIT] Exit code: 0 (success)\n");
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
                log::error!("[FFMPEG_EXIT] Exit code: {:?}\nStderr: {}", status.code(), err);
                crate::logger::write_raw(&format!("[FFMPEG_EXIT] Exit code: {:?}\nStderr:\n{}\n", status.code(), err));
                Err(err.into_owned())'''

    if old_success in content:
        content = content.replace(old_success, new_success, 1)
        changes += 1
        print("PATCH run_ffmpeg_cmd: Added exit code logging")
    else:
        print("SKIP run_ffmpeg_cmd success: Pattern not found")

    # ── run_ffmpeg_cmd: Add error logging for process error and timeout ──
    old_proc_err = '        Ok(Err(e)) => Err(format!("FFmpeg process error: {}", e)),'
    new_proc_err = '''        Ok(Err(e)) => {
            crate::logger::write_raw(&format!("[FFMPEG_EXIT] Process error: {}\n", e));
            Err(format!("FFmpeg process error: {}", e))
        }'''

    if old_proc_err in content:
        content = content.replace(old_proc_err, new_proc_err, 1)
        changes += 1
        print("PATCH run_ffmpeg_cmd: Added process error logging")
    else:
        print("SKIP process error: Pattern not found")

    old_timeout = '            Err("FFmpeg process timed out after 900 seconds".to_string())\n        }\n    }\n}'
    new_timeout = '''            crate::logger::write_raw("[FFMPEG_EXIT] Timed out after 900 seconds\n");
            Err("FFmpeg process timed out after 900 seconds".to_string())'''
    # More specific pattern
    old_timeout2 = '''            let _ = child.kill().await;
            let _ = child.wait().await;
            Err("FFmpeg process timed out after 900 seconds".to_string())
        }
    }
}'''

    new_timeout2 = '''            let _ = child.kill().await;
            let _ = child.wait().await;
            crate::logger::write_raw("[FFMPEG_EXIT] Timed out after 900 seconds\n");
            Err("FFmpeg process timed out after 900 seconds".to_string())
        }
    }
}'''

    if old_timeout2 in content:
        content = content.replace(old_timeout2, new_timeout2, 1)
        changes += 1
        print("PATCH run_ffmpeg_cmd: Added timeout logging")
    else:
        print("SKIP timeout: Pattern not found")

    # ── run_ffmpeg_cmd_with_cancel: Add command logging ──
    # Find the second occurrence of CREATE_NO_WINDOW (in run_ffmpeg_cmd_with_cancel)
    # We need to be more precise here
    cancel_func_start = content.find('async fn run_ffmpeg_cmd_with_cancel(')
    if cancel_func_start > 0:
        # Find the CREATE_NO_WINDOW in this function
        cwn_pos = content.find('const CREATE_NO_WINDOW: u32 = 0x08000000;', cancel_func_start)
        if cwn_pos > 0:
            # Find the end of this const line
            eol = content.index('\n', cwn_pos) + 1
            # Check if logging already added
            check_region = content[cwn_pos:cwn_pos+200]
            if 'Log the full command' not in check_region:
                # Insert command logging after the const line
                insert_text = '''
    // Log the full command line for job log capture
    log::info!("[FFMPEG_CMD] {} {}", ffmpeg_path.display(), args.join(" "));
    crate::logger::write_raw(&format!("[FFMPEG_CMD] {} {}\n", ffmpeg_path.display(), args.join(" ")));

'''
                content = content[:eol] + insert_text + content[eol:]
                changes += 1
                print("PATCH run_ffmpeg_cmd_with_cancel: Added command logging")
            else:
                print("SKIP run_ffmpeg_cmd_with_cancel command: Already patched")

    # ── run_ffmpeg_cmd_with_cancel: Add exit code logging on success ──
    cancel_fn = content.find('async fn run_ffmpeg_cmd_with_cancel(')
    if cancel_fn > 0:
        # Find the success path in this function (after cancel_fn)
        success_search = '                if status.success() {\n                    return Ok(());'
        success_pos = content.find(success_search, cancel_fn)
        if success_pos > 0:
            old_cancel_success = '                if status.success() {\n                    return Ok(());'
            new_cancel_success = '''                if status.success() {
                    log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                    crate::logger::write_raw("[FFMPEG_EXIT] Exit code: 0 (success)\n");
                    return Ok(());'''
            content = content[:success_pos] + new_cancel_success + content[success_pos + len(old_cancel_success):]
            changes += 1
            print("PATCH run_ffmpeg_cmd_with_cancel: Added exit success logging")
        else:
            print("SKIP cancel success: Pattern not found")

        # Find the error path in this function
        err_search = '                    return Err(String::from_utf8_lossy(&err_bytes).into_owned());'
        err_pos = content.find(err_search, cancel_fn)
        if err_pos > 0:
            # Find the line before to get the full block
            block_start = content.rfind('                } else {', cancel_fn, err_pos)
            old_cancel_err = '''                } else {
                    let mut err_bytes = Vec::new();
                    if let Some(mut stderr_reader) = stderr_handle {
                        let _ = stderr_reader.read_to_end(&mut err_bytes).await;
                    }
                    return Err(String::from_utf8_lossy(&err_bytes).into_owned());'''
            new_cancel_err = '''                } else {
                    let mut err_bytes = Vec::new();
                    if let Some(mut stderr_reader) = stderr_handle {
                        let _ = stderr_reader.read_to_end(&mut err_bytes).await;
                    }
                    let err = String::from_utf8_lossy(&err_bytes);
                    log::error!("[FFMPEG_EXIT] Exit code: {:?}\nStderr: {}", status.code(), err);
                    crate::logger::write_raw(&format!("[FFMPEG_EXIT] Exit code: {:?}\nStderr:\n{}\n", status.code(), err));
                    return Err(err.into_owned());'''
            if old_cancel_err in content[cancel_fn:]:
                # Replace only after cancel_fn
                pre = content[:cancel_fn]
                post = content[cancel_fn:]
                post = post.replace(old_cancel_err, new_cancel_err, 1)
                content = pre + post
                changes += 1
                print("PATCH run_ffmpeg_cmd_with_cancel: Added exit error logging")
            else:
                print("SKIP cancel error: Pattern not found in cancel function")

        # Add cancel/timeout/process error logging
        # Cancel path
        cancel_search = '            return Err("Merge cancelled by user".to_string());'
        cancel_pos = content.rfind(cancel_search, cancel_fn)
        if cancel_pos > 0:
            old_cancel = '            return Err("Merge cancelled by user".to_string());'
            new_cancel = '            crate::logger::write_raw("[FFMPEG_EXIT] Cancelled by user\n");\n            return Err("Merge cancelled by user".to_string());'
            # Only replace the one in run_ffmpeg_cmd_with_cancel
            pre = content[:cancel_pos]
            post = content[cancel_pos:]
            post = post.replace(old_cancel, new_cancel, 1)
            content = pre + post
            changes += 1
            print("PATCH run_ffmpeg_cmd_with_cancel: Added cancel logging")

        # Process error path
        proc_err_search = '                return Err(format!("FFmpeg process error: {}", e));'
        proc_err_pos = content.rfind(proc_err_search, cancel_fn)
        if proc_err_pos > 0:
            old_proc_err2 = '                return Err(format!("FFmpeg process error: {}", e));'
            new_proc_err2 = '                crate::logger::write_raw(&format!("[FFMPEG_EXIT] Process error: {}\n", e));\n                return Err(format!("FFmpeg process error: {}", e));'
            pre = content[:proc_err_pos]
            post = content[proc_err_pos:]
            post = post.replace(old_proc_err2, new_proc_err2, 1)
            content = pre + post
            changes += 1
            print("PATCH run_ffmpeg_cmd_with_cancel: Added process error logging")

        # Timeout path
        timeout_search = '            return Err(format!("FFmpeg process timed out after {} seconds", timeout_secs));'
        timeout_pos = content.rfind(timeout_search, cancel_fn)
        if timeout_pos > 0:
            old_timeout_c = '            return Err(format!("FFmpeg process timed out after {} seconds", timeout_secs));'
            new_timeout_c = '            crate::logger::write_raw(&format!("[FFMPEG_EXIT] Timed out after {} seconds\n", timeout_secs));\n            return Err(format!("FFmpeg process timed out after {} seconds", timeout_secs));'
            pre = content[:timeout_pos]
            post = content[timeout_pos:]
            post = post.replace(old_timeout_c, new_timeout_c, 1)
            content = pre + post
            changes += 1
            print("PATCH run_ffmpeg_cmd_with_cancel: Added timeout logging")

    with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"\nmerge.rs: {changes} patches applied")


def patch_mkvmerge_rs():
    with open('src-tauri/src/ffmpeg/mkvmerge.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # ── Add command logging after mkvmerge command construction ──
    old_mkv_cmd = '''    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn mkvmerge: {}", e))?;

    let stdout = child.stdout.take().unwrap();'''

    new_mkv_cmd = '''    // Log the full command line for job log capture
    log::info!("[MKVMERGE_CMD] {} --gui-mode -o {} {}", mkvmerge_path, output_path, input_files.join(" "));
    crate::logger::write_raw(&format!("[MKVMERGE_CMD] {} --gui-mode -o {} {}\n", mkvmerge_path, output_path, input_files.join(" ")));

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn mkvmerge: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();'''

    if old_mkv_cmd in content:
        content = content.replace(old_mkv_cmd, new_mkv_cmd, 1)
        changes += 1
        print("PATCH mkvmerge: Added command logging + stderr capture")
    else:
        print("SKIP mkvmerge command: Pattern not found")

    # ── Log mkvmerge stdout lines to job log ──
    old_stdout_parse = '''        let line = line.unwrap_or_default();

        // Parse progress from --gui-mode: "#GUI#progress 42%"
        if line.starts_with("#GUI#progress ") {'''

    new_stdout_parse = '''        let line = line.unwrap_or_default();

        // Log all mkvmerge stdout to job log
        crate::logger::write_raw(&format!("[MKVMERGE_STDOUT] {}\n", line));

        // Parse progress from --gui-mode: "#GUI#progress 42%"
        if line.starts_with("#GUI#progress ") {'''

    if old_stdout_parse in content:
        content = content.replace(old_stdout_parse, new_stdout_parse, 1)
        changes += 1
        print("PATCH mkvmerge: Added stdout line logging")
    else:
        print("SKIP mkvmerge stdout: Pattern not found")

    # ── Log mkvmerge exit code and stderr ──
    old_mkv_status = '''    let status = child.wait().map_err(|e| anyhow!("mkvmerge wait failed: {}", e))?;

    // mkvmerge exit codes: 0 = success, 1 = warnings (OK), 2 = fatal error
    if status.code() == Some(2) {
        cleanup_partial_output(Path::new(output_path));
        return Err(anyhow!("mkvmerge exited with fatal error (Code 2)"));
    } else if status.code() != Some(0) && status.code() != Some(1) {
        cleanup_partial_output(Path::new(output_path));
        return Err(anyhow!("mkvmerge exited with unexpected code {:?}", status.code()));
    }'''

    new_mkv_status = '''    // Log stderr from mkvmerge
    {
        use std::io::Read;
        let mut stderr_bytes = Vec::new();
        let mut stderr_reader = std::io::BufReader::new(stderr);
        let _ = stderr_reader.read_to_end(&mut stderr_bytes);
        let stderr_str = String::from_utf8_lossy(&stderr_bytes);
        if !stderr_str.trim().is_empty() {
            log::warn!("[MKVMERGE_STDERR] {}", stderr_str.trim());
            crate::logger::write_raw(&format!("[MKVMERGE_STDERR]\n{}\n", stderr_str));
        }
    }

    let status = child.wait().map_err(|e| anyhow!("mkvmerge wait failed: {}", e))?;
    let exit_code = status.code().unwrap_or(-1);
    log::info!("[MKVMERGE_EXIT] Exit code: {}", exit_code);
    crate::logger::write_raw(&format!("[MKVMERGE_EXIT] Exit code: {}\n", exit_code));

    // mkvmerge exit codes: 0 = success, 1 = warnings (OK), 2 = fatal error
    if status.code() == Some(2) {
        cleanup_partial_output(Path::new(output_path));
        return Err(anyhow!("mkvmerge exited with fatal error (Code 2)"));
    } else if status.code() != Some(0) && status.code() != Some(1) {
        cleanup_partial_output(Path::new(output_path));
        return Err(anyhow!("mkvmerge exited with unexpected code {:?}", status.code()));
    }'''

    if old_mkv_status in content:
        content = content.replace(old_mkv_status, new_mkv_status, 1)
        changes += 1
        print("PATCH mkvmerge: Added exit code + stderr logging")
    else:
        print("SKIP mkvmerge status: Pattern not found")

    with open('src-tauri/src/ffmpeg/mkvmerge.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"\nmkvmerge.rs: {changes} patches applied")


if __name__ == '__main__':
    patch_merge_rs()
    print()
    patch_mkvmerge_rs()
    print("\nDone!")
