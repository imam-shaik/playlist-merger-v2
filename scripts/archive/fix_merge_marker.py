#!/usr/bin/env python3
"""Fix merge marker calls in merge.rs: add remove_merge_marker() to all success paths."""
import os

BASE = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'src-tauri', 'src', 'commands')
path = os.path.join(BASE, 'merge.rs')

with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

changes = 0

# ============================================================
# FIX 1: FastMkv success path (line ~1779)
# Pattern: end_forensic_log(Success, None, Some(&request.output_path))
# Add remove_merge_marker before it
# ============================================================
old1 = """        match &fastmkv_result {
            Ok(_) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.output_path)),
            Err(e) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(e), Some(&request.output_path)),
        }
        return fastmkv_result;"""

new1 = """        match &fastmkv_result {
            Ok(_) => {
                crate::commands::merge::remove_merge_marker(&request.output_path);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.output_path));
            }
            Err(e) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(e), Some(&request.output_path)),
        }
        return fastmkv_result;"""

if old1 in content:
    content = content.replace(old1, new1, 1)
    changes += 1
    print("FIX 1: Added remove_merge_marker to FastMkv success path")
else:
    print("FIX 1: Could not find FastMkv success path")

# ============================================================
# FIX 2: SrtMergeOnly success path (line ~2097)
# Pattern: end_forensic_log(Success, None, Some(&output_path))
# ============================================================
old2 = """                log::info!("[EVENT_EMIT] event=merge-complete jobId={} (srt-only) emitted", job_id_inner);

                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully (SRT-only)"));"""

new2 = """                log::info!("[EVENT_EMIT] event=merge-complete jobId={} (srt-only) emitted", job_id_inner);

                crate::commands::merge::remove_merge_marker(&output_path);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully (SRT-only)"));"""

if old2 in content:
    content = content.replace(old2, new2, 1)
    changes += 1
    print("FIX 2: Added remove_merge_marker to SrtMergeOnly success path")
else:
    print("FIX 2: Could not find SrtMergeOnly success path")

# ============================================================
# FIX 3: Lossless/Custom success path (line ~5753)
# Pattern: end_forensic_log(Success, None, Some(&output_path))
# ============================================================
old3 = """                }));
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));"""

new3 = """                }));
                crate::commands::merge::remove_merge_marker(&output_path);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));"""

if old3 in content:
    content = content.replace(old3, new3, 1)
    changes += 1
    print("FIX 3: Added remove_merge_marker to Lossless/Custom success path")
else:
    print("FIX 3: Could not find Lossless/Custom success path")

# ============================================================
# FIX 4: Cancel path in SrtMergeOnly (line ~2038-2039)
# Also needs remove_merge_marker to prevent orphan markers
# ============================================================
old4 = """                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle_inner.emit("merge-error", &serde_json::json!({
                    "jobId": job_id_inner,
                    "error": "Merge cancelled by user",
                    "cancelled": true
                }));
                return Ok::<String, String>(request.job_id.clone());"""

new4 = """                crate::commands::merge::remove_merge_marker(&output_path);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle_inner.emit("merge-error", &serde_json::json!({
                    "jobId": job_id_inner,
                    "error": "Merge cancelled by user",
                    "cancelled": true
                }));
                return Ok::<String, String>(request.job_id.clone());"""

if old4 in content:
    content = content.replace(old4, new4, 1)
    changes += 1
    print("FIX 4: Added remove_merge_marker to SrtMergeOnly cancel path")
else:
    print("FIX 4: Could not find SrtMergeOnly cancel path")

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print(f"Updated {path} - {changes} changes applied")
