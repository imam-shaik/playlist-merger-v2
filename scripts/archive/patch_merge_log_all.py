#!/usr/bin/env python3
"""
Comprehensive patch for merge.rs:
- Add stop_job_log at ALL merge-complete and merge-error emit sites
- Since stop_job_log is idempotent, calling it multiple times is safe
- Only the first call will write the footer; subsequent calls are no-ops
"""
import re

FILE = "src-tauri/src/commands/merge.rs"

with open(FILE, "r", encoding="utf-8") as f:
    content = f.read()

patches = 0

# ── Find all emit("merge-error" sites and add stop_job_log after ─────────
# Pattern: emit("merge-error", &serde_json::json!({ ... }));
# We add stop_job_log BEFORE the emit so the log captures the error details too.

# Strategy: Find "merge-error" emit lines and add stop_job_log after the closing }));
# Use regex to find the full emit statement ending with }));

# Find all positions of "merge-error" emits
error_pattern = re.compile(
    r'(let _ = app_handle(?:_inner)?\.emit\("merge-error", &serde_json::json!\(\{[^}]*"jobId"[^}]*\}\)\);)',
    re.DOTALL
)

def add_stop_before_error(match):
    """Add stop_job_log BEFORE the error emit."""
    global patches
    full = match.group(0)
    if "crate::logger::stop_job_log" in content[max(0,match.start()-200):match.start()]:
        return full  # Already patched nearby
    patches += 1
    return 'crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));\n' + full

content = error_pattern.sub(add_stop_before_error, content)
print(f"Added stop_job_log before {patches} merge-error emit(s)")

# ── Find all emit("merge-complete" sites and add stop_job_log after ──────
complete_pattern = re.compile(
    r'(let _ = app_handle(?:_inner)?\.emit\("merge-complete", &serde_json::json!\(\{[^}]*"jobId"[^}]*\}\)\);)',
    re.DOTALL
)

complete_patches = 0
def add_stop_after_complete(match):
    """Add stop_job_log AFTER the complete emit."""
    global complete_patches
    full = match.group(0)
    after = content[match.end():match.end()+200]
    if "crate::logger::stop_job_log" in after:
        return full  # Already patched nearby
    complete_patches += 1
    return full + '\ncrate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));'

content = complete_pattern.sub(add_stop_after_complete, content)
print(f"Added stop_job_log after {complete_patches} merge-complete emit(s)")

# Also handle the watchdog/cancel paths that emit merge-error with different patterns
# Pattern: "cancelled":true
cancel_pattern = re.compile(
    r'(let _ = (?:app_handle(?:_inner)?|ah)\.emit\("merge-error", &serde_json::json!\(\{\s*"jobId":\s*[a-z_]+,\s*"error":\s*"Merge cancelled by user",\s*"cancelled":\s*true\s*\}\)\);)',
    re.DOTALL
)

cancel_patches = 0
def add_stop_before_cancel(match):
    global cancel_patches
    full = match.group(0)
    before = content[max(0,match.start()-200):match.start()]
    if "crate::logger::stop_job_log" in before:
        return full
    cancel_patches += 1
    return 'crate::logger::stop_job_log(Some("[JOB_CANCELLED] Merge cancelled by user"));\n' + full

content = cancel_pattern.sub(add_stop_before_cancel, content)
print(f"Added stop_job_log before {cancel_patches} merge-cancel emit(s)")

# ── Summary ──────────────────────────────────────────────────────────────
total = patches + complete_patches + cancel_patches
print(f"\nTotal patches: {total}")

with open(FILE, "w", encoding="utf-8") as f:
    f.write(content)

print(f"File written: {len(content)} chars")
