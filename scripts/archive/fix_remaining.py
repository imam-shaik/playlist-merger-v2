#!/usr/bin/env python3
"""Fix remaining issues: lossless/custom success path in merge.rs, panic hook in lib.rs."""
import os

PROJECT = os.path.dirname(os.path.dirname(__file__))

# ============================================================
# FIX: Lossless/Custom success path in merge.rs
# ============================================================
merge_path = os.path.join(PROJECT, 'src-tauri', 'src', 'commands', 'merge.rs')
with open(merge_path, 'r', encoding='utf-8') as f:
    content = f.read()

changes = 0

old = """crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));
                log::info!(\"[EVENT_EMIT] event=merge-complete jobId={} emitted\", job_id);"""

new = """crate::commands::merge::remove_merge_marker(&output_path);
crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));
                log::info!(\"[EVENT_EMIT] event=merge-complete jobId={} emitted\", job_id);"""

if old in content:
    content = content.replace(old, new, 1)
    changes += 1
    print("FIX merge.rs: Added remove_merge_marker to Lossless/Custom success path")
else:
    print("FIX merge.rs: Could not find Lossless/Custom success path - trying alternate pattern")
    # Try with different whitespace
    alt = 'crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));\n                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));'
    if alt in content:
        print("  Found alternate pattern!")
        idx = content.index(alt)
        print(f"  At position {idx}")
        print(f"  Context: ...{content[idx-50:idx+200]}...")
    else:
        print("  Alternate pattern also not found")

with open(merge_path, 'w', encoding='utf-8') as f:
    f.write(content)

print(f"{changes} changes to merge.rs")

# ============================================================
# FIX: Add panic::set_hook to lib.rs
# ============================================================
lib_path = os.path.join(PROJECT, 'src-tauri', 'src', 'lib.rs')
with open(lib_path, 'r', encoding='utf-8') as f:
    lib_content = f.read()

old_panic = """pub fn run() {
    // Initialize the custom per-job log capture logger.
    // This replaces env_logger and captures ALL log output to per-job files.
    logger::init();

    run_startup_cleanup();"""

new_panic = """pub fn run() {
    // Initialize the custom per-job log capture logger.
    // This replaces env_logger and captures ALL log output to per-job files.
    logger::init();

    // ── Panic Hook ──────────────────────────────────────────────────
    // Capture panics to the forensic log so crashes are recorded even
    // if no merge is in progress. Without this hook, panics would show
    // the default panic message with no forensic capture or cleanup trigger.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        let location = panic_info.location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>").to_string();

        let panic_msg = format!("{} at {} (thread: {})", message, location, thread_name);
        log::error!("[PANIC] {}", panic_msg);
        crate::forensic_log::append_panic_block(&panic_msg);

        // Also call the default hook so stderr gets the usual panic output
        default_hook(panic_info);
    }));

    run_startup_cleanup();"""

if old_panic in lib_content:
    lib_content = lib_content.replace(old_panic, new_panic, 1)
    changes += 1
    print("FIX lib.rs: Added panic::set_hook")
else:
    print("FIX lib.rs: Could not find the run() function start")

with open(lib_path, 'w', encoding='utf-8') as f:
    f.write(lib_content)

print(f"{changes} changes to lib.rs")
print(f"\nTotal changes: {changes}")
