#!/usr/bin/env python3
"""Patch merge.rs to add per-job log capture hooks. Uses LF line endings."""
import sys

FILE = "src-tauri/src/commands/merge.rs"

with open(FILE, "r", encoding="utf-8") as f:
    content = f.read()

original_len = len(content)
patches_applied = 0

# ── PATCH 1: Start job log at the beginning of start_merge ──────────────
old1 = (
    '    log::info!("[FORENSIC:ENTRY] jobId: {} | thread_id: {:?} | timestamp: {}", \n'
    '        request.job_id, std::thread::current().id(), chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));\n'
    '    \n'
    '    log::info!("[Merge] ═══════════════════════════════════════════════════════════════");'
)

new1 = (
    '    log::info!("[FORENSIC:ENTRY] jobId: {} | thread_id: {:?} | timestamp: {}", \n'
    '        request.job_id, std::thread::current().id(), chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));\n'
    '\n'
    '    // ── Per-Job Log Capture ──────────────────────────────────────────────\n'
    '    // Start logging to a dedicated file for this job.\n'
    '    // All subsequent log::info!, log::warn!, log::error! calls will be captured.\n'
    '    let _job_log_path = {\n'
    '        let output_dir = std::path::Path::new(&request.output_path)\n'
    '            .parent()\n'
    '            .map(|p| p.to_string_lossy().into_owned())\n'
    '            .unwrap_or_else(|| ".".to_string());\n'
    '        let job_name = std::path::Path::new(&request.output_path)\n'
    '            .file_stem()\n'
    '            .map(|n| n.to_string_lossy().into_owned())\n'
    '            .unwrap_or_else(|| "merged_output".to_string());\n'
    '        crate::logger::start_job_log(&output_dir, &job_name, &request.job_id)\n'
    '    };\n'
    '\n'
    '    log::info!("[Merge] ═══════════════════════════════════════════════════════════════");'
)

if old1 in content:
    content = content.replace(old1, new1, 1)
    patches_applied += 1
    print("PATCH 1 APPLIED: Job log start hook in start_merge")
else:
    print("PATCH 1 FAILED: Could not find FORENSIC:ENTRY anchor")

# ── PATCH 2: Already applied by previous run, check if present ──────────
if "crate::logger::stop_job_log" in content:
    print("PATCH 2: Already present (skip)")
    patches_applied += 1
else:
    # Try to apply it
    old2 = '                log::info!("[EVENT_EMIT] event=merge-complete jobId={} (srt-only) emitted", job_id_inner);'
    new2 = (
        '                log::info!("[EVENT_EMIT] event=merge-complete jobId={} (srt-only) emitted", job_id_inner);\n'
        '                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully (SRT-only)"));'
    )
    if old2 in content:
        content = content.replace(old2, new2, 1)
        patches_applied += 1
        print("PATCH 2 APPLIED: stop_job_log on SRT-only success")
    else:
        print("PATCH 2 FAILED: Could not find SRT-only emit anchor")

# ── PATCH 3: Stop job log on main merge completion ─────────────────────
# Find the main merge-complete event emit (not srt-only)
# Look for the pattern after "Finalising output" log
old3_marker = '                    "warnings": Vec::<String>::new()\n                }));\n                log::info!("[EVENT_EMIT] event=merge-complete jobId={}", job_id_inner);'
new3_marker = (
    '                    "warnings": Vec::<String>::new()\n                }));\n'
    '                log::info!("[EVENT_EMIT] event=merge-complete jobId={}", job_id_inner);\n'
    '                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));'
)

if old3_marker in content and "crate::logger::stop_job_log" not in content.split("event=merge-complete jobId={}")[1].split("\n")[0:3]:
    # Only apply if not already there after the main merge-complete
    content = content.replace(old3_marker, new3_marker, 1)
    patches_applied += 1
    print("PATCH 3 APPLIED: stop_job_log on main merge complete")
else:
    print("PATCH 3 SKIPPED: Already present or anchor not found")

with open(FILE, "w", encoding="utf-8") as f:
    f.write(content)

print(f"\nDone: {patches_applied}/3 patches applied. File size: {original_len} -> {len(content)} chars")
sys.exit(0 if patches_applied >= 2 else 1)
