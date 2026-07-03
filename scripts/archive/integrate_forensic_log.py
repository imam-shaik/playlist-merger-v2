#!/usr/bin/env python3
"""Integrate forensic_log into merge.rs, split.rs, and section.rs.

Adds start_forensic_log at entry points and end_forensic_log at exit points.
This script is idempotent — safe to run multiple times.
"""
import re
import os

BASE = os.path.join(os.path.dirname(__file__), '..', 'src-tauri', 'src', 'commands')


def patch_merge_rs():
    path = os.path.join(BASE, 'merge.rs')
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # ── 1. Entry point: after start_job_log block ──────────────────────────
    old_entry = '''        crate::logger::start_job_log(&output_dir, &job_name, &request.job_id)
    };

    log::info!("[Merge] ═══════════════════════════════════════════════════════════════");
    log::info!("[Merge] Job ID: {}", request.job_id);
    log::info!("[Merge] Mode: {:?}", request.mode);
    log::info!("[Merge] Input Files: {}", request.input_files.len());'''

    new_entry = '''        crate::logger::start_job_log(&output_dir, &job_name, &request.job_id)
    };

    // ── Forensic Log Start ──────────────────────────────────────────────
    // Purely observational — writes a structured log to Desktop/PlaylistMerger Logs/
    // Does NOT modify any merge pipeline logic, progress, checkpoints, or modes.
    {
        let playlist_name = request.input_names.first().map(|s| s.as_str());
        let mode_str = format!("{:?}", request.mode);
        let _ = crate::forensic_log::start_forensic_log(
            std::path::Path::new(&request.output_path)
                .parent()
                .map(|p| p.to_string_lossy())
                .unwrap_or(std::borrow::Cow::Borrowed(".")),
            "merge",
            &request.job_id,
            playlist_name,
            &mode_str,
            &request.output_path,
            None, // ffmpeg path (resolved later)
            None, // mkvmerge path
            request.input_files.len(),
        );
    }

    log::info!("[Merge] ═══════════════════════════════════════════════════════════════");
    log::info!("[Merge] Job ID: {}", request.job_id);
    log::info!("[Merge] Mode: {:?}", request.mode);
    log::info!("[Merge] Input Files: {}", request.input_files.len());'''

    if old_entry in content and new_entry not in content:
        content = content.replace(old_entry, new_entry, 1)
        changes += 1
        print("PATCH merge.rs: Entry point forensic_log start")
    else:
        print("SKIP merge.rs entry: Already patched or pattern not found")

    # ── 2. SRT merge cancel ────────────────────────────────────────────────
    old_srt_cancel = '''            if cancel_flag_clone.load(Ordering::Relaxed) {
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle_inner.emit("merge-error", &serde_json::json!({'''

    new_srt_cancel = '''            if cancel_flag_clone.load(Ordering::Relaxed) {
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle_inner.emit("merge-error", &serde_json::json!({'''

    if old_srt_cancel in content and "crate::forensic_log::end_forensic_log" not in content.split(old_srt_cancel)[0][-200:]:
        content = content.replace(old_srt_cancel, new_srt_cancel, 1)
        changes += 1
        print("PATCH merge.rs: SRT merge cancel")
    else:
        print("SKIP merge.rs SRT cancel: Already patched or pattern not found")

    # ── 3. SRT merge success ──────────────────────────────────────────────
    old_srt_success = '''                log::info!("[EVENT_EMIT] event=merge-complete jobId={} (srt-only) emitted", job_id_inner);

                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully (SRT-only)"));'''

    new_srt_success = '''                log::info!("[EVENT_EMIT] event=merge-complete jobId={} (srt-only) emitted", job_id_inner);

                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully (SRT-only)"));'''

    if old_srt_success in content and "crate::forensic_log::end_forensic_log" not in content.split(old_srt_success)[0][-200:]:
        content = content.replace(old_srt_success, new_srt_success, 1)
        changes += 1
        print("PATCH merge.rs: SRT merge success")
    else:
        print("SKIP merge.rs SRT success: Already patched or pattern not found")

    # ── 4. SRT merge error ────────────────────────────────────────────────
    old_srt_error = '''                };
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle_inner.emit("merge-error", &serde_json::json!({'''

    new_srt_error = '''                };
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(&err_str), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle_inner.emit("merge-error", &serde_json::json!({'''

    if old_srt_error in content and "crate::forensic_log::end_forensic_log" not in content.split(old_srt_error)[0][-200:]:
        content = content.replace(old_srt_error, new_srt_error, 1)
        changes += 1
        print("PATCH merge.rs: SRT merge error")
    else:
        print("SKIP merge.rs SRT error: Already patched or pattern not found")

    # ── 5. Main merge success ──────────────────────────────────────────────
    old_main_success = '''                }));

crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));
                log::info!("[EVENT_EMIT] event=merge-complete jobId={} emitted", job_id);'''

    new_main_success = '''                }));

crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));
                log::info!("[EVENT_EMIT] event=merge-complete jobId={} emitted", job_id);'''

    if old_main_success in content and "crate::forensic_log::end_forensic_log" not in content.split(old_main_success)[0][-200:]:
        content = content.replace(old_main_success, new_main_success, 1)
        changes += 1
        print("PATCH merge.rs: Main merge success")
    else:
        print("SKIP merge.rs main success: Already patched or pattern not found")

    # ── 6. Main merge error ────────────────────────────────────────────────
    old_main_error = '''                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=concat_failed", job_id);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle.emit("merge-error", &serde_json::json!({'''

    new_main_error = '''                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=concat_failed", job_id);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(&err_msg), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle.emit("merge-error", &serde_json::json!({'''

    if old_main_error in content and "crate::forensic_log::end_forensic_log" not in content.split(old_main_error)[0][-200:]:
        content = content.replace(old_main_error, new_main_error, 1)
        changes += 1
        print("PATCH merge.rs: Main merge error")
    else:
        print("SKIP merge.rs main error: Already patched or pattern not found")

    # ── 7. Watchdog panic ──────────────────────────────────────────────────
    old_watchdog = '''            Err(je) => {
                // Thread panicked — JoinError
                log::error!("[MERGE_TRACE] WATCHDOG: merge thread PANICKED jobId={} panic={}", watchdog_job_id, je);
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=watchdog_panic", watchdog_job_id);
                let _ = watchdog_app.emit("merge-error", &serde_json::json!({
                    "jobId": watchdog_job_id,
                    "error": format!("Merge process panicked: {}", je),
                    "cancelled": false,
                    "phase": "concat"
                }));
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=watchdog_panic emitted", watchdog_job_id);'''

    new_watchdog = '''            Err(je) => {
                // Thread panicked — JoinError
                log::error!("[MERGE_TRACE] WATCHDOG: merge thread PANICKED jobId={} panic={}", watchdog_job_id, je);
                crate::forensic_log::append_panic_block(&format!("{}", je));
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Panic, Some(&format!("{}", je)), None);
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=watchdog_panic", watchdog_job_id);
                let _ = watchdog_app.emit("merge-error", &serde_json::json!({
                    "jobId": watchdog_job_id,
                    "error": format!("Merge process panicked: {}", je),
                    "cancelled": false,
                    "phase": "concat"
                }));
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=watchdog_panic emitted", watchdog_job_id);'''

    if old_watchdog in content and "crate::forensic_log::end_forensic_log" not in content.split(old_watchdog)[0][-200:]:
        content = content.replace(old_watchdog, new_watchdog, 1)
        changes += 1
        print("PATCH merge.rs: Watchdog panic handler")
    else:
        print("SKIP merge.rs watchdog: Already patched or pattern not found")

    # ── 8. Cancel points (normalization loops) ──────────────────────────────
    # Pattern: crate::logger::stop_job_log(Some("[JOB_CANCELLED] ...
    # Add end_forensic_log(Cancelled) before each JOB_CANCELLED stop_job_log
    cancel_pattern = re.compile(
        r"(crate::logger::stop_job_log\(Some\(\"\[JOB_CANCELLED\])"
    )
    cancel_count = 0
    def add_forensic_before_cancel(match):
        nonlocal cancel_count
        before = content[max(0, match.start()-300):match.start()]
        if "crate::forensic_log::end_forensic_log" in before:
            return match.group(0)  # Already patched
        cancel_count += 1
        return 'crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);\n' + match.group(0)

    content = cancel_pattern.sub(add_forensic_before_cancel, content)
    if cancel_count > 0:
        changes += 1
        print(f"PATCH merge.rs: {cancel_count} cancel point(s)")

    # ── 9. Fatal cancel points (JOB_FAILED with cancelled context) ─────────
    # Pattern: stop_job_log(Some("[JOB_FAILED] Merge failed")) near Cancelled
    fail_cancel_pattern = re.compile(
        r'(crate::logger::stop_job_log\(Some\("\[JOB_FAILED\] Merge failed"\)\);)'
    )
    fail_count = 0
    matches = list(fail_cancel_pattern.finditer(content))
    for m in matches:
        before = content[max(0, m.start()-400):m.start()]
        after = content[m.end():m.end()+400]
        # Only patch if near a cancelled keyword and no forensic_log nearby
        if ("Cancelled" in before or "cancelled" in after) and "crate::forensic_log::end_forensic_log" not in before:
            # Check if already has our forensic_log line within 5 chars before
            snippet_before = content[max(0, m.start()-50):m.start()]
            if "end_forensic_log" not in snippet_before:
                fail_count += 1
                replacement = 'crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);\n' + m.group(0)
                content = content[:m.start()] + replacement + content[m.end():]
                break  # Only one replacement needed (they're similar)

    if fail_count > 0:
        print(f"PATCH merge.rs: {fail_count} fail+cancel point(s)")

    if changes > 0:
        with open(path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"\n✅ merge.rs: {changes} change(s) applied")
    else:
        print(f"\n⏭️  merge.rs: No changes needed")


def patch_split_rs():
    path = os.path.join(BASE, 'split.rs')
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # ── Entry point ──────────────────────────────────────────────────────
    old_entry = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
    let _log_guard = crate::logger::JobLogGuard::new_empty();

    let settings = crate::services::settings::load_settings_internal();'''

    new_entry = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // ── Forensic Log Start ──────────────────────────────────────────────
    // Purely observational — writes a structured log to Desktop/PlaylistMerger Logs/
    {
        let _ = crate::forensic_log::start_forensic_log(
            &request.plan.output_dir,
            "split",
            &job_id,
            None, // playlist name not available in split context
            "split",
            &request.plan.output_dir,
            None,
            None,
            request.plan.segments.len(),
        );
    }

    // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
    let _log_guard = crate::logger::JobLogGuard::new_empty();

    let settings = crate::services::settings::load_settings_internal();'''

    if old_entry in content and "crate::forensic_log::start_forensic_log" not in content:
        content = content.replace(old_entry, new_entry, 1)
        changes += 1
        print("PATCH split.rs: Entry point")
    else:
        print("SKIP split.rs entry: Already patched or pattern not found")

    # ── Exit point (success) ─────────────────────────────────────────────
    old_exit = '''        let mut ms = state.merge_state.lock().await;
        crate::logger::stop_job_log(Some("[JOB_COMPLETE] Split completed"));
        ms.active_jobs.remove(&job_id);'''

    new_exit = '''        let mut ms = state.merge_state.lock().await;
        crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.plan.output_dir));
        crate::logger::stop_job_log(Some("[JOB_COMPLETE] Split completed"));
        ms.active_jobs.remove(&job_id);'''

    if old_exit in content and "crate::forensic_log::end_forensic_log" not in content.split(old_exit)[0][-200:]:
        content = content.replace(old_exit, new_exit, 1)
        changes += 1
        print("PATCH split.rs: Exit point")
    else:
        print("SKIP split.rs exit: Already patched or pattern not found")

    if changes > 0:
        with open(path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"\n✅ split.rs: {changes} change(s) applied")
    else:
        print(f"\n⏭️  split.rs: No changes needed")


def patch_section_rs():
    path = os.path.join(BASE, 'section.rs')
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    changes = 0

    # ── Entry point ──────────────────────────────────────────────────────
    old_entry = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    let cancel_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));'''

    new_entry = '''        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // ── Forensic Log Start ──────────────────────────────────────────────
    // Purely observational — writes a structured log to Desktop/PlaylistMerger Logs/
    {
        let _ = crate::forensic_log::start_forensic_log(
            &request.config.output_base_dir,
            "section_merge",
            &job_id,
            None, // playlist name not available in section context
            "section",
            &request.config.output_base_dir,
            None,
            None,
            request.folders.len(),
        );
    }

    let cancel_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));'''

    if old_entry in content and "crate::forensic_log::start_forensic_log" not in content:
        content = content.replace(old_entry, new_entry, 1)
        changes += 1
        print("PATCH section.rs: Entry point")
    else:
        print("SKIP section.rs entry: Already patched or pattern not found")

    # ── Exit point (success) ─────────────────────────────────────────────
    old_exit = '''        let mut ms = state_merge_state.lock().await;
            crate::logger::stop_job_log(Some("[JOB_COMPLETE] Section merge completed"));
            ms.active_jobs.remove(&job_id_clone);'''

    new_exit = '''        let mut ms = state_merge_state.lock().await;
            crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.config.output_base_dir));
            crate::logger::stop_job_log(Some("[JOB_COMPLETE] Section merge completed"));
            ms.active_jobs.remove(&job_id_clone);'''

    if old_exit in content and "crate::forensic_log::end_forensic_log" not in content.split(old_exit)[0][-200:]:
        content = content.replace(old_exit, new_exit, 1)
        changes += 1
        print("PATCH section.rs: Exit point")
    else:
        print("SKIP section.rs exit: Already patched or pattern not found")

    if changes > 0:
        with open(path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"\n✅ section.rs: {changes} change(s) applied")
    else:
        print(f"\n⏭️  section.rs: No changes needed")


if __name__ == '__main__':
    print("═" * 60)
    print("  Forensic Log Integration")
    print("═" * 60)
    patch_merge_rs()
    print()
    patch_split_rs()
    print()
    patch_section_rs()
    print()
    print("═" * 60)
    print("  Done.")
    print("═" * 60)
