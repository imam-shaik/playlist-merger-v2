#!/usr/bin/env python3
"""Fix misplaced timers and complete per-file validation timing."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

changes = 0

# ═══════════════════════════════════════════════════════════════
# FIX 1: Remove misplaced lines from ValidationTiming struct
# Line 206 (0-based: 205): "let _p2_start = ..."
# Line 212 (0-based: 211): "_phase2_ms = ..."
# ═══════════════════════════════════════════════════════════════
# Search from bottom to top to avoid index shifting
to_remove = []
for i, line in enumerate(lines):
    if 'let _p2_start = std::time::Instant::now();' in line and i < 500:
        to_remove.append(i)
        print(f"FIX 1a: Marking line {i+1} for removal: {line.strip()[:60]}")
    if '_phase2_ms = _p2_start.elapsed()' in line and i < 500:
        to_remove.append(i)
        print(f"FIX 1b: Marking line {i+1} for removal: {line.strip()[:60]}")

# Remove from bottom to top to preserve indices
for idx in sorted(to_remove, reverse=True):
    lines.pop(idx)
    changes += 1

# ═══════════════════════════════════════════════════════════════
# FIX 2: Insert Phase 2 timer BEFORE check_pts_monotonic call
# inside validate_single_impl (not in the struct)
# ═══════════════════════════════════════════════════════════════
# Find check_pts_monotonic CALL (not definition) - it's inside validate_single_impl
# We need the call that's indented (inside a function body)
for i, line in enumerate(lines):
    if '        let pts_issues = self.check_pts_monotonic(' in line:
        # This is the call inside validate_single_impl
        lines.insert(i, '    let _p2_start = std::time::Instant::now();')
        changes += 1
        print(f"FIX 2: Inserted _p2_start before check_pts_monotonic call at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 3: Insert Phase 2 duration BEFORE check_video_decode call
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if '        let video_decode_issues = self.check_video_decode(' in line:
        lines.insert(i, '    _phase2_ms = _p2_start.elapsed().as_millis() as f64;')
        changes += 1
        print(f"FIX 3: Inserted _phase2_ms before check_video_decode at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 4: Add _phase9_ms timing BEFORE classify_damage_extended
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if '        let damage = self.classify_damage_extended(' in line:
        lines.insert(i, '    _phase9_ms = _p2_start.elapsed().as_millis() as f64 - _phase2_ms;')
        changes += 1
        print(f"FIX 4: Inserted _phase9_ms before classify_damage_extended at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 5: Add classify timer + per-file summary log
# ═══════════════════════════════════════════════════════════════
# Find the line AFTER classify_damage_extended result is used
# Look for where damage is matched/checked
for i, line in enumerate(lines):
    if '        let damage = self.classify_damage_extended(' in line:
        # Find the closing of this let statement
        for j in range(i, min(i + 5, len(lines))):
            if lines[j].rstrip().endswith(';'):
                # Insert after the classify call
                log_block = (
                    '    _classify_ms = _p2_start.elapsed().as_millis() as f64 - _phase2_ms - _phase9_ms;\n'
                    '    // -- Per-file validation summary --\n'
                    '    let _total_ms = _file_start.elapsed().as_millis() as f64;\n'
                    '    let file_name = std::path::Path::new(file_path)\n'
                    '        .file_name().map(|n| n.to_string_lossy().to_string())\n'
                    '        .unwrap_or_else(|| file_path.to_string());\n'
                    '    if _total_ms > 1000.0 {\n'
                    '        log::warn!("[STAGE_TIMING] SLOW_VALIDATION file={} index={} total={{:.0}}ms | p1={{:.0}}ms p2={{:.0}}ms p9={{:.0}}ms classify={{:.0}}ms",\n'
                    '            file_name, file_index, _total_ms, _phase1_ms, _phase2_ms, _phase9_ms, _classify_ms);\n'
                    '    } else {\n'
                    '        log::debug!("[STAGE_TIMING] VALIDATION file={} index={} total={{:.0}}ms | p1={{:.0}}ms p2={{:.0}}ms p9={{:.0}}ms classify={{:.0}}ms",\n'
                    '            file_name, file_index, _total_ms, _phase1_ms, _phase2_ms, _phase9_ms, _classify_ms);\n'
                    '    }\n'
                )
                lines.insert(j + 1, log_block)
                changes += 1
                print(f"FIX 5: Inserted per-file timing log after line {j+1}")
                break
        break

# Write the file
with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f"\nTotal changes: {changes}")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    verify = f.read()

checks = [
    ('_file_start', 'file timer'),
    ('_p1_start', 'phase1 start'),
    ('_p2_start', 'phase2 start'),
    ('_phase1_ms', 'phase1 duration'),
    ('_phase2_ms', 'phase2 duration'),
    ('_phase9_ms', 'phase9 duration'),
    ('_classify_ms', 'classify duration'),
    ('SLOW_VALIDATION', 'slow file warning'),
    ('[STAGE_TIMING] VALIDATION file=', 'per-file debug log'),
]

print("\nVerification:")
for pattern, label in checks:
    count = verify.count(pattern)
    print(f"  [{'PASS' if count > 0 else 'FAIL'}] {label} (found {count}x)")

# Make sure no misplaced code in struct
struct_lines = verify.split('\n')[:500]
for i, line in enumerate(struct_lines):
    if '_p2_start' in line and 'struct' not in line and i < 500:
        if 'fn ' not in line and 'self.' not in line:
            print(f"  [WARN] Possible misplaced _p2_start at line {i+1}: {line.strip()[:80]}")
