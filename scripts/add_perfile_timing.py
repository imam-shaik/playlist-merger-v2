#!/usr/bin/env python3
"""Add per-file validation timing to validate_single_impl."""
import re

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

lines = content.split('\n')
changes = 0

# ═══════════════════════════════════════════════════════════════
# STEP 1: Add timer at start of validate_single_impl
# Insert after "let mut repair_trace: Vec<RepairTraceEntry> = Vec::new();"
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'fn validate_single_impl(' in line:
        # Find the repair_trace initialization
        for j in range(i, min(i + 10, len(lines))):
            if 'repair_trace' in lines[j] and 'Vec::new()' in lines[j]:
                # Insert timer after this line
                timer_block = (
                    '\n'
                    '    // -- Per-file validation timing --\n'
                    '    let _file_start = std::time::Instant::now();\n'
                    '    let mut _phase1_ms: f64 = 0.0;\n'
                    '    let mut _phase2_ms: f64 = 0.0;\n'
                    '    let mut _phase9_ms: f64 = 0.0;\n'
                    '    let mut _classify_ms: f64 = 0.0;'
                )
                lines.insert(j + 1, timer_block)
                changes += 1
                print(f"STEP 1: Added per-file timer after line {j+1}")
                break
        break

# ═══════════════════════════════════════════════════════════════
# STEP 2: Add Phase 1 timing around quick_container_check
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if '// Phase 1: Quick structural check' in line:
        # Insert phase1 timer before the comment
        lines.insert(i, '    let _p1_start = std::time::Instant::now();')
        # Now find the end of Phase 1 (where Phase 2 begins or deep analysis starts)
        for j in range(i + 2, min(i + 150, len(lines))):
            if '// Phase 2:' in lines[j] or 'Deep packet-level' in lines[j] or 'check_pts_monotonic' in lines[j]:
                # Insert phase1 duration log before Phase 2
                lines.insert(j, '    _phase1_ms = _p1_start.elapsed().as_millis() as f64;')
                changes += 1
                print(f"STEP 2: Added Phase 1 timing around quick_container_check (before line {j+1})")
                break
        break

# ═══════════════════════════════════════════════════════════════
# STEP 3: Add Phase 2 timing around deep checks
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'check_pts_monotonic' in line:
        # Insert phase2 timer before the first deep check
        lines.insert(i, '    let _p2_start = std::time::Instant::now();')
        # Find Phase 9 (check_video_decode)
        for j in range(i + 2, min(i + 20, len(lines))):
            if 'check_video_decode' in lines[j]:
                # Insert phase2 duration before Phase 9
                lines.insert(j, '    _phase2_ms = _p2_start.elapsed().as_millis() as f64;')
                changes += 1
                print(f"STEP 3: Added Phase 2 timing (before line {j+1})")
                break
        break

# ═══════════════════════════════════════════════════════════════
# STEP 4: Add Phase 9 timing around additional validation
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'check_video_decode' in line and 'Phase 9' not in line:
        # Find the classification section (after attachment_integrity)
        for j in range(i, min(i + 15, len(lines))):
            if 'classify_damage_extended' in lines[j] or 'classify_damage' in lines[j] or 'has_pts_issues' in lines[j]:
                # Insert phase9 duration before classification
                lines.insert(j, '    _phase9_ms = _p2_start.elapsed().as_millis() as f64 - _phase2_ms;')
                changes += 1
                print(f"STEP 4: Added Phase 9 timing (before line {j+1})")
                break
        break

# ═══════════════════════════════════════════════════════════════
# STEP 5: Add classify timing and final per-file log
# Find the final return point for healthy files
# ═══════════════════════════════════════════════════════════════
# Find classify_damage_extended and add timing around it
for i, line in enumerate(lines):
    if 'classify_damage_extended' in line and 'fn ' not in line:
        # Insert classify timer before this call
        lines.insert(i, '    let _cl_start = std::time::Instant::now();')
        # Find the next use of the result (damage variable)
        for j in range(i + 1, min(i + 10, len(lines))):
            if 'damage' in lines[j] and ('==' in lines[j] or 'match' in lines[j] or 'Healthy' in lines[j]):
                lines.insert(j, '    _classify_ms = _cl_start.elapsed().as_millis() as f64;')
                changes += 1
                print(f"STEP 5: Added classify timing (before line {j+1})")
                break
        break

# ═══════════════════════════════════════════════════════════════
# STEP 6: Add final per-file log just before each return
# We'll add it before the classify_damage_extended call since all paths converge there
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'classify_damage_extended' in line and 'fn ' not in line:
        # Insert the per-file timing log after classification
        # Find the line after classify_ms is set
        for j in range(i + 1, min(i + 20, len(lines))):
            if '_classify_ms' in lines[j] and '_cl_start' in lines[j]:
                # Insert the final log after this line
                log_block = (
                    '\n'
                    '    // -- Per-file validation summary --\n'
                    '    let _total_ms = _file_start.elapsed().as_millis() as f64;\n'
                    '    let file_name = std::path::Path::new(file_path)\n'
                    '        .file_name().map(|n| n.to_string_lossy().to_string())\n'
                    '        .unwrap_or_else(|| file_path.to_string());\n'
                    '    if _total_ms > 1000.0 {\n'
                    '        log::warn!("[STAGE_TIMING] SLOW_VALIDATION file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms classify={:.0}ms",\n'
                    '            file_name, file_index, _total_ms, _phase1_ms, _phase2_ms, _phase9_ms, _classify_ms);\n'
                    '    } else {\n'
                    '        log::debug!("[STAGE_TIMING] VALIDATION file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms classify={:.0}ms",\n'
                    '            file_name, file_index, _total_ms, _phase1_ms, _phase2_ms, _phase9_ms, _classify_ms);\n'
                    '    }'
                )
                lines.insert(j + 1, log_block)
                changes += 1
                print(f"STEP 6: Added per-file timing log after line {j+1}")
                break
        break

# Write the file
with open(path, 'w', encoding='utf-8') as f:
    f.write('\n'.join(lines))

print(f"\nTotal changes: {changes}")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    verify = f.read()

checks = [
    ('_file_start', 'file timer'),
    ('_phase1_ms', 'phase1 timer'),
    ('_phase2_ms', 'phase2 timer'),
    ('_phase9_ms', 'phase9 timer'),
    ('_classify_ms', 'classify timer'),
    ('SLOW_VALIDATION', 'slow file warning'),
    ('[STAGE_TIMING] VALIDATION file=', 'per-file debug log'),
]

print("\nVerification:")
for pattern, label in checks:
    count = verify.count(pattern)
    print(f"  [{'PASS' if count > 0 else 'FAIL'}] {label} (found {count}x)")
