#!/usr/bin/env python3
"""Fix all 3 remaining profiler issues."""
path = 'src-tauri/src/commands/merge.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

changes = 0

# ═══════════════════════════════════════════════════════════════
# FIX 1: Remove duplicate phase_start reset in subtitle SKIPPED
# Line 2370 (0-based: 2369) is the duplicate
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'SUBTITLE_PROCESSING' in line and 'SKIPPED' in line:
        # The duplicate phase_start is the line after the log::info
        if i + 1 < len(lines) and 'phase_start = std::time::Instant::now()' in lines[i + 1]:
            lines[i + 1] = ''
            changes += 1
            print(f"FIX 1: Removed duplicate phase_start reset at line {i+2}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 2: Remove misplaced files_re_encoded += 1 from inside closure
# Line 4209 (0-based: 4208) is inside a map callback
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'files_re_encoded += 1' in line and 'map_or' in lines[i-1] if i > 0 else False:
        lines[i] = ''
        changes += 1
        print(f"FIX 2: Removed misplaced files_re_encoded from inside closure at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 3: Add all 3 normalization counters in the correct scope
# After the FORENSIC:NORMALIZE END log macro, before "match res {"
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'FORENSIC:NORMALIZE] END' in line:
        # Find the end of this log::info! macro (line ending with ');')
        macro_end = None
        for j in range(i, min(i + 10, len(lines))):
            if lines[j].rstrip().endswith(');'):
                macro_end = j
                break
        if macro_end is not None:
            # Check if counters are already there
            check = ''.join(lines[macro_end+1:macro_end+10])
            if 'files_repaired += 1' in check:
                # Remove old misplaced counters first
                for k in range(macro_end+1, min(macro_end+15, len(lines))):
                    if 'files_repaired += 1' in lines[k]:
                        # Remove the entire if block around it
                        # Find the opening if
                        for m in range(k, max(k-3, macro_end), -1):
                            if 'if only_remux && dm.timescale_den.is_some()' in lines[m]:
                                # Count braces to find end
                                depth = 0
                                for n in range(m, min(m+5, len(lines))):
                                    depth += lines[n].count('{') - lines[n].count('}')
                                    if depth == 0 and n > m:
                                        # Remove lines m to n inclusive
                                        for _ in range(n - m + 1):
                                            lines.pop(m)
                                        changes += 1
                                        print(f"FIX 3a: Removed old files_repaired block at line {m+1}")
                                        break
                                break
                    if 'files_re_encoded += 1' in lines[k]:
                        for m in range(k, max(k-3, macro_end), -1):
                            if 'if !only_remux' in lines[m]:
                                depth = 0
                                for n in range(m, min(m+5, len(lines))):
                                    depth += lines[n].count('{') - lines[n].count('}')
                                    if depth == 0 and n > m:
                                        for _ in range(n - m + 1):
                                            lines.pop(m)
                                        changes += 1
                                        print(f"FIX 3b: Removed old files_re_encoded block at line {m+1}")
                                        break
                                break
                    if 'match res' in lines[k]:
                        break
            
            # Now insert all 3 counters cleanly after the log macro
            # Re-find the macro end since lines may have shifted
            for j in range(i, min(i + 10, len(lines))):
                if lines[j].rstrip().endswith(');'):
                    macro_end = j
                    break
            
            counter_block = (
                '                    // -- Normalization counters --\n'
                '                    if only_remux && dm.timescale_den.is_some() {\n'
                '                        files_repaired += 1;\n'
                '                    }\n'
                '                    if only_remux && dm.timescale_den.is_none() {\n'
                '                        files_container_copy += 1;\n'
                '                    }\n'
                '                    if !only_remux {\n'
                '                        files_re_encoded += 1;\n'
                '                    }\n'
            )
            lines.insert(macro_end + 1, counter_block)
            changes += 1
            print(f"FIX 3c: Inserted all 3 normalization counters after line {macro_end+1}")
        break

# ═══════════════════════════════════════════════════════════════
# Write the file
# ═══════════════════════════════════════════════════════════════
with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f"\nTotal changes: {changes}")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

checks = [
    ('files_repaired += 1', 'files_repaired counter'),
    ('files_re_encoded += 1', 'files_re_encoded counter'),
    ('files_container_copy += 1', 'files_container_copy counter'),
    ('SUBTITLE_PROCESSING', 'subtitle SKIPPED logging'),
]

print("\nVerification:")
for pattern, label in checks:
    count = content.count(pattern)
    print(f"  [{'PASS' if count > 0 else 'FAIL'}] {label} (found {count}x)")

# Check no counters inside closures
lines_check = content.split('\n')
for i, line in enumerate(lines_check):
    if 'files_re_encoded += 1' in line and 'map_or' in line:
        print(f"  [WARN] files_re_encoded still inside closure at line {i+1}")
