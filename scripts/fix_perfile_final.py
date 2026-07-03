#!/usr/bin/env python3
"""Fix all per-file timing issues: own timers, log format, per-file summary."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

changes = 0

# ═══════════════════════════════════════════════════════════════
# FIX 1: Replace the broken _phase9_ms line with proper p9_start + duration
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if '_phase9_ms = _p2_start.elapsed()' in line:
        lines[i] = '    _phase9_ms = _p2_start.elapsed().as_millis() as f64 - _phase2_ms;\n'
        # Don't replace yet - we need p9_start instead
        # Actually let's use a different approach: add _p9_start before Phase 9
        break

# ═══════════════════════════════════════════════════════════════
# FIX 2: Add _p9_start before check_video_decode and replace _phase9_ms
# ═══════════════════════════════════════════════════════════════
# First, add _p9_start before check_video_decode
for i, line in enumerate(lines):
    if '        let video_decode_issues = self.check_video_decode(' in line:
        # Check if _p9_start is already there
        if '_p9_start' not in lines[i-1]:
            lines.insert(i, '    let _p9_start = std::time::Instant::now();')
            changes += 1
            print(f"FIX 2a: Added _p9_start before check_video_decode at line {i+1}")
        break

# Now replace the _phase9_ms calculation to use _p9_start
for i, line in enumerate(lines):
    if '_phase9_ms = _p2_start.elapsed()' in line:
        lines[i] = '    _phase9_ms = _p9_start.elapsed().as_millis() as f64;\n'
        changes += 1
        print(f"FIX 2b: Fixed _phase9_ms to use _p9_start at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 3: Add _cl_start before classify_damage_extended and fix _classify_ms
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if '        let damage = self.classify_damage_extended(' in line:
        if '_cl_start' not in lines[i-1]:
            lines.insert(i, '    let _cl_start = std::time::Instant::now();')
            changes += 1
            print(f"FIX 3a: Added _cl_start before classify_damage_extended at line {i+1}")
        break

# Fix _classify_ms to use _cl_start
for i, line in enumerate(lines):
    if '_classify_ms = _p2_start.elapsed()' in line:
        lines[i] = '    _classify_ms = _cl_start.elapsed().as_millis() as f64;\n'
        changes += 1
        print(f"FIX 3b: Fixed _classify_ms to use _cl_start at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 4: Add per-file summary log after classify_damage_extended result
# Insert after the closing ");" of classify_damage_extended
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if '        let damage = self.classify_damage_extended(' in line:
        # Find the closing ");" of this call
        for j in range(i, min(i + 15, len(lines))):
            if lines[j].rstrip().endswith(');'):
                # Insert the per-file log after this line
                log_block = (
                    '    _classify_ms = _cl_start.elapsed().as_millis() as f64;\n'
                    '    // -- Per-file validation summary --\n'
                    '    let _total_ms = _file_start.elapsed().as_millis() as f64;\n'
                    '    let file_name = std::path::Path::new(file_path)\n'
                    '        .file_name().map(|n| n.to_string_lossy().to_string())\n'
                    '        .unwrap_or_else(|| file_path.to_string());\n'
                    '    if _total_ms > 1000.0 {\n'
                    '        log::warn!("[STAGE_TIMING] SLOW_VALIDATION file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms classify={:.0}ms damage={:?}",\n'
                    '            file_name, file_index, _total_ms, _phase1_ms, _phase2_ms, _phase9_ms, _classify_ms, damage);\n'
                    '    } else {\n'
                    '        log::debug!("[STAGE_TIMING] VALIDATION file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms classify={:.0}ms",\n'
                    '            file_name, file_index, _total_ms, _phase1_ms, _phase2_ms, _phase9_ms, _classify_ms);\n'
                    '    }\n'
                )
                lines.insert(j + 1, log_block)
                changes += 1
                print(f"FIX 4: Inserted per-file summary log after line {j+1}")
                break
        break

# ═══════════════════════════════════════════════════════════════
# FIX 5: Remove any duplicate _classify_ms = _cl_start lines
# ═══════════════════════════════════════════════════════════════
seen_classify_ms = False
for i in range(len(lines) - 1, -1, -1):
    if '_classify_ms = _cl_start.elapsed()' in lines[i]:
        if seen_classify_ms:
            lines.pop(i)
            changes += 1
            print(f"FIX 5: Removed duplicate _classify_ms at line {i+1}")
        else:
            seen_classify_ms = True

# Write
with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f"\nTotal changes: {changes}")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    verify = f.read()

checks = [
    ('let _file_start = std::time::Instant::now();', 'file timer init'),
    ('let _p1_start = std::time::Instant::now();', 'phase1 start'),
    ('let _p2_start = std::time::Instant::now();', 'phase2 start'),
    ('let _p9_start = std::time::Instant::now();', 'phase9 start'),
    ('let _cl_start = std::time::Instant::now();', 'classify start'),
    ('_phase1_ms = _p1_start.elapsed()', 'phase1 duration'),
    ('_phase2_ms = _p2_start.elapsed()', 'phase2 duration'),
    ('_phase9_ms = _p9_start.elapsed()', 'phase9 duration'),
    ('_classify_ms = _cl_start.elapsed()', 'classify duration'),
    ('SLOW_VALIDATION', 'slow file warning'),
    ('[STAGE_TIMING] VALIDATION file=', 'per-file debug log'),
]

print("\nVerification:")
for pattern, label in checks:
    count = verify.count(pattern)
    print(f"  [{'PASS' if count > 0 else 'FAIL'}] {label} (found {count}x)")
