#!/usr/bin/env python3
"""Deploy JobLogGuard RAII safety net - fix remaining patches."""

# ── Fix merge.rs: main merge spawn_blocking ──
with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# The main spawn_blocking starts differently than expected - it's mkvmerge call
# Find the actual pattern
old_main = '''        let merge_result = tokio::task::spawn_blocking(move || {
            crate::ffmpeg::mkvmerge::run_mkvmerge('''

new_main = '''        let merge_result = tokio::task::spawn_blocking(move || {
            // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
            let _log_guard = crate::logger::JobLogGuard::new_empty();
            crate::ffmpeg::mkvmerge::run_mkvmerge('''

if old_main in content:
    content = content.replace(old_main, new_main, 1)
    print("PATCH merge.rs: Added JobLogGuard to main merge spawn_blocking")
else:
    print("SKIP main merge: Pattern not found")

# Also check for the other main spawn_blocking (FFmpeg concat path)
old_concat = '''        let merge_result = tokio::task::spawn_blocking(move || {
            let _guard = cleanup_guard;'''

new_concat = '''        let merge_result = tokio::task::spawn_blocking(move || {
            // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
            let _log_guard = crate::logger::JobLogGuard::new_empty();
            let _guard = cleanup_guard;'''

if old_concat in content:
    content = content.replace(old_concat, new_concat, 1)
    print("PATCH merge.rs: Added JobLogGuard to concat merge spawn_blocking")
else:
    print("SKIP concat merge: Pattern not found (may already have guard or different pattern)")

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

# ── Fix split.rs ──
with open('src-tauri/src/commands/split.rs', 'r', encoding='utf-8') as f:
    content = f.read()

old_split = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    log::info!("[SplitCmd] execute_split_plan'''

new_split = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
    let _log_guard = crate::logger::JobLogGuard::new_empty();

    log::info!("[SplitCmd] execute_split_plan'''

if old_split in content:
    content = content.replace(old_split, new_split, 1)
    print("PATCH split.rs: Added JobLogGuard after start_job_log")
else:
    print("SKIP split.rs: Pattern not found")

with open('src-tauri/src/commands/split.rs', 'w', encoding='utf-8') as f:
    f.write(content)

# ── Fix section.rs ──
with open('src-tauri/src/commands/section.rs', 'r', encoding='utf-8') as f:
    content = f.read()

old_section = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    log::info!("[Section] Starting section merge'''

new_section = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
    let _log_guard = crate::logger::JobLogGuard::new_empty();

    log::info!("[Section] Starting section merge'''

if old_section in content:
    content = content.replace(old_section, new_section, 1)
    print("PATCH section.rs: Added JobLogGuard after start_job_log")
else:
    print("SKIP section.rs: Pattern not found")

with open('src-tauri/src/commands/section.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("\nDone!")
