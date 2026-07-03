#!/usr/bin/env python3
"""Fix the scope issue in merge.rs.

The `smart_mkv_breakdown`, `normalize_count`, `remux_count`, `skip_count` 
variables were defined inside `if actual_mode == MergeMode::SmartMkv { }` 
block but referenced later in the progress event emission outside that block.

Fix: Declare them as Option before the if block.
"""

import os

MERGE_RS = os.path.join(os.path.dirname(__file__), '..', 'src', 'commands', 'merge.rs')

with open(MERGE_RS, 'r', encoding='utf-8') as f:
    content = f.read()

# ======================================================================
# Fix 1: Remove the parallel_workers dead code
# ======================================================================

# The script inserted a parallel_workers block after forensics init.
# Remove it since it's dead code.
old_parallel = """
    // ── PHASE 7: Parallel Normalization ──────────────────────────────────
    // When Smart MKV or Custom mode with many files, parallelize normalize+verify.
    // Each file's normalization is independent (unique temp path per file),
    // so we can run N concurrent FFmpeg processes using tokio::JoinSet.
    //
    // Concurrency is limited by available CPU cores (min 2, max 8) and
    // a Semaphore to prevent overloading the system.
    let parallel_workers = std::cmp::min(8, std::cmp::max(2,
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
    ));
"""

if old_parallel in content:
    content = content.replace(old_parallel, "")
    print("Fix 1: Removed parallel_workers dead code")
else:
    print("Fix 1: parallel_workers block not found - checking for remaining artifact...")
    # Check for partial match
    if 'PHASE 7: Parallel Normalization' in content:
        print("  Found Phase 7 comment - removing it")
        # Find and remove the whole block
        idx = content.find('    // ── PHASE 7: Parallel Normalization')
        if idx >= 0:
            end_idx = content.find('\n\n', idx)
            if end_idx < 0:
                end_idx = content.find('\n        ', idx)
            if end_idx > idx:
                content = content[:idx] + content[end_idx:]
                print(f"  Removed from position {idx}")
    
    if 'parallel_workers' in content:
        print("  Warning: parallel_workers still present in file")

# ======================================================================
# Fix 2: Move smart_mkv_breakdown variables outside the SmartMkv if block
# The variables need to be Option types declared BEFORE the if block
# ======================================================================

# Find the SmartMkv if block and add variable declarations before it
old_smart_mkv_start = """    if actual_mode == MergeMode::SmartMkv {
        let before_profile_count = analysis.outliers.len();"""

new_smart_mkv_start = """    // ── Smart MKV analysis variables (set inside block, used later for progress event) ──
    // These are declared outside the `if actual_mode == MergeMode::SmartMkv` block
    // because they're referenced later in the normalization progress event emission.
    let mut smart_mkv_breakdown: Option<SmartMkvBreakdown> = None;
    let mut normalize_count: usize = 0;
    let mut remux_count: usize = 0;
    let mut skip_count: usize = 0;

    if actual_mode == MergeMode::SmartMkv {
        let before_profile_count = analysis.outliers.len();"""

if old_smart_mkv_start in content:
    content = content.replace(old_smart_mkv_start, new_smart_mkv_start)
    print("Fix 2: Added variable declarations outside SmartMkv if block")
else:
    print("Fix 2: SmartMkv start not found")
    # Try to find it
    idx = content.find('    if actual_mode == MergeMode::SmartMkv')
    if idx >= 0:
        print(f"  Found at position {idx}")
        print(f"  Context: {content[idx:idx+100]}")

# ======================================================================
# Fix 3: Update the smart_mkv_breakdown assignment to use `let` -> remove `let`
# since the variable is now declared outside the block
# ======================================================================

# The old line was: let smart_mkv_breakdown = compute_smart_mkv_breakdown(&analysis);
# The new line should be: smart_mkv_breakdown = Some(compute_smart_mkv_breakdown(&analysis));

old_breakdown = "        let smart_mkv_breakdown = compute_smart_mkv_breakdown(&analysis);"
new_breakdown = "        smart_mkv_breakdown = Some(compute_smart_mkv_breakdown(&analysis));"

if old_breakdown in content:
    content = content.replace(old_breakdown, new_breakdown)
    print("Fix 3: Changed smart_mkv_breakdown to use Option assignment")
else:
    print("Fix 3: smart_mkv_breakdown assignment not found")

# ======================================================================
# Fix 4: Wrap the smartMkvBreakdown emission in a conditional
# Only emit when SmartMkv mode is active (smart_mkv_breakdown is Some)
# ======================================================================

# The smartMkvBreakdown was added to the progress event unconditionally.
# We need to make it conditional on SmartMkv mode (smart_mkv_breakdown is Some).
# 
# The simplest fix: check if smart_mkv_breakdown is Some and build the JSON differently

old_emit_smart = """                    "smartMkvBreakdown": {
                        "willNormalize": normalize_count,
                        "willRemux": remux_count,
                        "willSkip": skip_count,
                        "totalFiles": total_input_count,
                        "categories": {
                            "normalize": smart_mkv_breakdown.normalize.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                            "remux": smart_mkv_breakdown.remux.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                            "skip": smart_mkv_breakdown.skip.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                        }
                    },"""

# Since the progress event is built with serde_json::json! macro, we can't easily
# conditionally include a field. The cleanest approach is to add the field as a
# separate emit call only when SmartMkv is active.

# Remove the smartMkvBreakdown from the progress event entirely and replace it
# with a conditional emit right after the progress event.
new_emit_smart = """                    // smartMkvBreakdown is emitted separately below (conditional on SmartMkv mode)"""

if old_emit_smart in content:
    content = content.replace(old_emit_smart, new_emit_smart)
    print("Fix 4: Removed unconditional smartMkvBreakdown from progress event")
else:
    print("Fix 4: smartMkvBreakdown not found in emit (may already be fixed)")

# ======================================================================
# Fix 5: Add conditional smartMkvBreakdown emission after the progress event
# ======================================================================

# Find the emit later and add conditional emission after it
old_emit_end = """            }
        }));

        let mut already_normalized: std::collections::HashSet<usize> = std::collections::HashSet::new();"""

new_emit_end = """            }
        }));

        // ── Smart MKV Analysis Dashboard: emit breakdown data ──────────────
        if let Some(ref smb) = smart_mkv_breakdown {
            let _ = app_handle.emit("merge-progress", &serde_json::json!({
                "jobId": request.job_id,
                "progress": {
                    "phase": "normalizing",
                    "smartMkvBreakdown": {
                        "willNormalize": normalize_count,
                        "willRemux": remux_count,
                        "willSkip": skip_count,
                        "totalFiles": total_input_count,
                        "categories": {
                            "normalize": smb.normalize.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                            "remux": smb.remux.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                            "skip": smb.skip.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                        }
                    }
                }
            }));
        }

        let mut already_normalized: std::collections::HashSet<usize> = std::collections::HashSet::new();"""

if old_emit_end in content:
    content = content.replace(old_emit_end, new_emit_end)
    print("Fix 5: Added conditional smartMkvBreakdown emission")
else:
    print("Fix 5: Emit end section not found")

with open(MERGE_RS, 'w', encoding='utf-8') as f:
    f.write(content)

print("\n=== Scope Fix Summary ===")
print("1. Removed parallel_workers dead code")
print("2. Added Option<> declarations outside SmartMkv if block")
print("3. Changed assignment to use Some()")
print("4. Made smartMkvBreakdown emission conditional on SmartMkv mode")
