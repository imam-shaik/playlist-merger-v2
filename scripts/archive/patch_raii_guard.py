#!/usr/bin/env python3
"""Deploy JobLogGuard RAII as safety net in merge.rs, split.rs, and section.rs.

Strategy: Add `let _log_guard = crate::logger::JobLogGuard::new_empty();` at the
start of each execution path. Keep all existing manual stop_job_log calls.

When manual stop_job_log runs first (sets writer=None), the guard's Drop becomes
a no-op. If a new return path is added that skips manual stop, the guard catches it.
"""

import re

def patch_merge_rs():
    with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # ── Patch 1: SRT-only spawn_blocking (line ~2000) ──
    # Add guard right after "tokio::task::spawn_blocking(move || {"
    # The SRT-only path starts with this pattern
    old_srt = '''        tokio::task::spawn_blocking(move || {
            let app_handle_inner = app_handle.clone();
            let job_id_inner = job_id.clone();

            // Check cancellation before starting SRT merge'''

    new_srt = '''        tokio::task::spawn_blocking(move || {
            // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
            let _log_guard = crate::logger::JobLogGuard::new_empty();
            let app_handle_inner = app_handle.clone();
            let job_id_inner = job_id.clone();

            // Check cancellation before starting SRT merge'''

    if old_srt in content:
        content = content.replace(old_srt, new_srt, 1)
        changes += 1
        print("PATCH merge.rs: Added JobLogGuard to SRT-only spawn_blocking")
    else:
        print("SKIP SRT-only: Pattern not found")

    # ── Patch 2: Main merge spawn_blocking (line ~5033) ──
    # Find the main spawn_blocking that wraps the entire merge
    # Pattern: "let merge_result = tokio::task::spawn_blocking(move || {"
    # followed by the merge logic
    old_main = '''        let merge_result = tokio::task::spawn_blocking(move || {
            let _guard = cleanup_guard; // Ownership moved here; dropped at end of closure
            let app_handle_inner = app_handle.clone();
            let job_id_inner = job_id.clone();
            let job_id_for_cancel = job_id.clone();
            let cancel_for_retry = cancel_flag.clone();'''

    new_main = '''        let merge_result = tokio::task::spawn_blocking(move || {
            // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
            let _log_guard = crate::logger::JobLogGuard::new_empty();
            let _guard = cleanup_guard; // Ownership moved here; dropped at end of closure
            let app_handle_inner = app_handle.clone();
            let job_id_inner = job_id.clone();
            let job_id_for_cancel = job_id.clone();
            let cancel_for_retry = cancel_flag.clone();'''

    if old_main in content:
        content = content.replace(old_main, new_main, 1)
        changes += 1
        print("PATCH merge.rs: Added JobLogGuard to main merge spawn_blocking")
    else:
        print("SKIP main merge: Pattern not found")

    with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"\nmerge.rs: {changes} guard patches applied")


def patch_split_rs():
    with open('src-tauri/src/commands/split.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    # Add guard right after start_job_log in execute_split_plan
    # The pattern is: start_job_log(...) followed by setup code
    # Find the line after start_job_log that has the first statement
    old_split = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    log::info!("[SplitCmd] execute_split_plan: job_id={}, segments={}", job_id, request.plan.segments.len());'''

    new_split = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
    let _log_guard = crate::logger::JobLogGuard::new_empty();

    log::info!("[SplitCmd] execute_split_plan: job_id={}, segments={}", job_id, request.plan.segments.len());'''

    if old_split in content:
        content = content.replace(old_split, new_split, 1)
        print("PATCH split.rs: Added JobLogGuard after start_job_log")
    else:
        print("SKIP split.rs: Pattern not found")

    with open('src-tauri/src/commands/split.rs', 'w', encoding='utf-8') as f:
        f.write(content)


def patch_section_rs():
    with open('src-tauri/src/commands/section.rs', 'r', encoding='utf-8') as f:
        content = f.read()

    # Add guard right after start_job_log in start_section_merge
    old_section = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    log::info!("[Section] Starting section merge with {} folders", request.folders.len());'''

    new_section = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
    let _log_guard = crate::logger::JobLogGuard::new_empty();

    log::info!("[Section] Starting section merge with {} folders", request.folders.len());'''

    if old_section in content:
        content = content.replace(old_section, new_section, 1)
        print("PATCH section.rs: Added JobLogGuard after start_job_log")
    else:
        print("SKIP section.rs: Pattern not found")

    with open('src-tauri/src/commands/section.rs', 'w', encoding='utf-8') as f:
        f.write(content)


if __name__ == '__main__':
    patch_merge_rs()
    print()
    patch_split_rs()
    print()
    patch_section_rs()
    print("\nAll RAII guard patches applied!")
