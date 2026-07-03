#!/usr/bin/env python3
"""Fix the broken classify_damage_extended call, remove stray }, add per-file log."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

changes = 0

# ═══════════════════════════════════════════════════════════════
# FIX 1: Remove stray } at line 3545 (0-based: 3544)
# ═══════════════════════════════════════════════════════════════
# Find the pattern: line ending with .ok() followed by two } lines
for i in range(len(lines) - 1, 0, -1):
    if lines[i].rstrip() == '}' and i > 3500:
        # Check if previous line is also }
        if i > 0 and lines[i-1].rstrip() == '}':
            # Check if the line before that ends with .ok()
            if i > 1 and '.ok()' in lines[i-2]:
                lines.pop(i)
                changes += 1
                print(f"FIX 1: Removed stray }} at line {i+1}")
                break

# ═══════════════════════════════════════════════════════════════
# FIX 2: Fix the mangled classify_damage_extended call
# Line 1494 has: _phase9_ms = ...;
# Line 1495 has: has_container_critical,  (missing function call prefix)
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'has_container_critical,' in line.strip() and i > 1480 and i < 1510:
        # Check if previous line has _phase9_ms
        if '_phase9_ms' in lines[i-1]:
            # This line is missing its function call prefix
            # The pattern should be: "        let damage = self.classify_damage_extended("
            lines[i] = '        let damage = self.classify_damage_extended(\n' + '            has_container_critical,\n'
            # Wait, we're replacing the line. Let me be more careful.
            # The original line is just "            has_container_critical,"
            # We need to prepend "        let damage = self.classify_damage_extended(\n"
            lines[i] = '        let damage = self.classify_damage_extended(\n            has_container_critical,\n'
            # But this adds an extra line. Let me handle it differently.
            # Actually the simplest fix: just prepend the function call to this line
            pass
    # Actually let me take a different approach
    break

# Better approach: find the exact broken pattern and fix it
# Line 1494 (0-based: 1493): "    _phase9_ms = _p9_start.elapsed().as_millis() as f64;\n"
# Line 1495 (0-based: 1494): "            has_container_critical,\n"
# We need to insert "        let damage = self.classify_damage_extended(\n" between them

for i, line in enumerate(lines):
    if '_phase9_ms = _p9_start.elapsed().as_millis() as f64;' in line:
        next_line = lines[i + 1] if i + 1 < len(lines) else ''
        if 'has_container_critical,' in next_line:
            # Insert the missing function call prefix
            insert_line = '        let damage = self.classify_damage_extended(\n'
            lines.insert(i + 1, insert_line)
            changes += 1
            print(f"FIX 2: Inserted missing classify_damage_extended call prefix at line {i+2}")
            break

# ═══════════════════════════════════════════════════════════════
# FIX 3: Remove the duplicate classify_damage_extended if it exists
# ═══════════════════════════════════════════════════════════════
# Count occurrences of "let damage = self.classify_damage_extended("
count = sum(1 for line in lines if 'let damage = self.classify_damage_extended(' in line)
if count > 1:
    # Remove the first occurrence (keep the last one)
    found_first = False
    for i, line in enumerate(lines):
        if 'let damage = self.classify_damage_extended(' in line:
            if found_first:
                # This is the second one - keep it
                break
            else:
                found_first = True
                # Check if this one already has the closing );
                # If so, remove the entire duplicate block
                for j in range(i, min(i + 15, len(lines))):
                    if '_cl_start' in lines[j] or '_classify_ms' in lines[j]:
                        # This is the second block with timing - keep it
                        break
                    if lines[j].rstrip().endswith(');'):
                        # This is the first block without timing - remove it
                        # Also remove _cl_start if it's right before
                        start = i
                        if '_cl_start' in lines[i-1]:
                            start = i - 1
                        for _ in range(j - start + 1):
                            lines.pop(start)
                        changes += 1
                        print(f"FIX 3: Removed duplicate classify_damage_extended at original line {start+1}")
                        break
                break

# ═══════════════════════════════════════════════════════════════
# FIX 4: Remove any orphaned _cl_start / _classify_ms lines
# ═══════════════════════════════════════════════════════════════
# Find and remove standalone _cl_start that's not before classify_damage_extended
for i in range(len(lines) - 1, -1, -1):
    if 'let _cl_start = std::time::Instant::now();' in lines[i]:
        # Check if next non-empty line has classify_damage_extended
        found_classify = False
        for j in range(i + 1, min(i + 5, len(lines))):
            if 'classify_damage_extended' in lines[j]:
                found_classify = True
                break
            if lines[j].strip() and '_cl_start' not in lines[j]:
                break
        if not found_classify:
            lines.pop(i)
            changes += 1
            print(f"FIX 4: Removed orphaned _cl_start at line {i+1}")

# ═══════════════════════════════════════════════════════════════
# FIX 5: Ensure _classify_ms is computed and per-file log exists
# ═══════════════════════════════════════════════════════════════
# Find the closing ");" of classify_damage_extended
for i, line in enumerate(lines):
    if 'let damage = self.classify_damage_extended(' in line:
        for j in range(i, min(i + 15, len(lines))):
            if lines[j].rstrip().endswith(');'):
                # Check if _classify_ms is already computed after this
                already_has = False
                for k in range(j + 1, min(j + 5, len(lines))):
                    if '_classify_ms' in lines[k]:
                        already_has = True
                        break
                    if lines[k].strip() and '_classify' not in lines[k]:
                        break
                
                if not already_has:
                    log_block = (
                        '    _classify_ms = _cl_start.elapsed().as_millis() as f64;\n'
                        '    // -- Per-file validation summary --\n'
                        '    let _total_ms = _file_start.elapsed().as_millis() as f64;\n'
                        '    let _file_name = std::path::Path::new(file_path)\n'
                        '        .file_name().map(|n| n.to_string_lossy().to_string())\n'
                        '        .unwrap_or_else(|| file_path.to_string());\n'
                        '    if _total_ms > 1000.0 {\n'
                        '        log::warn!("[STAGE_TIMING] SLOW_VALIDATION file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms classify={:.0}ms damage={:?}",\n'
                        '            _file_name, file_index, _total_ms, _phase1_ms, _phase2_ms, _phase9_ms, _classify_ms, damage);\n'
                        '    } else {\n'
                        '        log::trace!("[STAGE_TIMING] VALIDATION file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms classify={:.0}ms",\n'
                        '            _file_name, file_index, _total_ms, _phase1_ms, _phase2_ms, _phase9_ms, _classify_ms);\n'
                        '    }\n'
                    )
                    lines.insert(j + 1, log_block)
                    changes += 1
                    print(f"FIX 5: Inserted _classify_ms + per-file summary log after line {j+1}")
                else:
                    print(f"FIX 5: _classify_ms already exists after classify call")
                break
        break

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
    ('let damage = self.classify_damage_extended(', 'classify call intact'),
]

print("\nVerification:")
for pattern, label in checks:
    count = verify.count(pattern)
    print(f"  [{'PASS' if count > 0 else 'FAIL'}] {label} (found {count}x)")

# Count classify calls
classify_count = verify.count('let damage = self.classify_damage_extended(')
print(f"\n  classify_damage_extended calls: {classify_count} (should be 1)")
