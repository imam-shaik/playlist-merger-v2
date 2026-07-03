#!/usr/bin/env python3
"""Fix the critical counter scope bug: counters incremented inside async move don't propagate."""

path = 'src-tauri/src/commands/merge.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

changes = 0

# ═══════════════════════════════════════════════════════════════
# FIX 1: Replace `let mut files_repaired: usize = 0;` etc.
# with Arc<AtomicUsize> declarations
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'let mut files_repaired: usize = 0;' in line:
        # Replace the 3 counter declarations with Arc<AtomicUsize>
        lines[i] = '    let files_repaired = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));\n'
        # Find and replace the next two
        for j in range(i+1, min(i+5, len(lines))):
            if 'let mut files_container_copy: usize = 0;' in lines[j]:
                lines[j] = '    let files_container_copy = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));\n'
            if 'let mut files_re_encoded: usize = 0;' in lines[j]:
                lines[j] = '    let files_re_encoded = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));\n'
        changes += 1
        print(f"FIX 1: Replaced counter declarations with Arc<AtomicUsize> at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 2: Remove the `let _ = files_*` suppressions
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'let _ = files_repaired;' in line:
        # Remove the entire suppression block (comment + 6 let _ = lines)
        start = i
        # Go back to find the comment
        for j in range(i-1, max(0, i-5), -1):
            if 'TODO:' in lines[j] or 'Counter variables' in lines[j]:
                start = j
                break
        # Find the end (last let _ = line)
        end = i
        for j in range(i, min(i+10, len(lines))):
            if 'let _ =' in lines[j]:
                end = j
            elif lines[j].strip() and 'let _ =' not in lines[j]:
                break
        # Remove lines start to end
        for _ in range(end - start + 1):
            lines.pop(start)
        changes += 1
        print(f"FIX 2: Removed counter suppression block at lines {start+1}-{end+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 3: Clone Arc counters before the spawned task and increment
# them inside the task using .fetch_add()
# ═══════════════════════════════════════════════════════════════
# Find where the spawned task is created and clone the Arcs before it
for i, line in enumerate(lines):
    if 'js.spawn(async move {' in line and i > 4150:
        # Insert Arc clones before this line
        clone_block = (
            '                let files_repaired_cl = files_repaired.clone();\n'
            '                let files_container_copy_cl = files_container_copy.clone();\n'
            '                let files_re_encoded_cl = files_re_encoded.clone();\n'
        )
        lines.insert(i, clone_block)
        changes += 1
        print(f"FIX 3: Added Arc clones before spawned task at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 4: Add counter increments using the cloned Arcs inside the task
# Find the NORMALIZATION COMPLETE timing log and add increments before it
# ═══════════════════════════════════════════════════════════════
# Actually, the counters need to be incremented per-file inside the spawned task.
# The best place is after the normalization result is determined.
# Find where only_remux is determined and add the fetch_add calls.

# First, find the counter block that was removed earlier (it had the if only_remux checks)
# We need to add it back using the cloned Arcs
for i, line in enumerate(lines):
    if '// -- Normalization counters --' in line:
        # This block still exists but uses the old variables
        # Replace the old counter block with Arc-based increments
        end = i
        for j in range(i, min(i + 12, len(lines))):
            if 'match res {' in lines[j] or 'Ok(path)' in lines[j]:
                end = j
                break
        # Replace the block
        new_block = (
            '                    // -- Normalization counters (Arc-based) --\n'
            '                    if only_remux && dm.timescale_den.is_some() {\n'
            '                        files_repaired_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
            '                    }\n'
            '                    if only_remux && dm.timescale_den.is_none() {\n'
            '                        files_container_copy_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
            '                    }\n'
            '                    if !only_remux {\n'
            '                        files_re_encoded_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
            '                    }\n'
        )
        for j in range(end - i):
            lines.pop(i)
        lines.insert(i, new_block)
        changes += 1
        print(f"FIX 4: Replaced counter block with Arc-based fetch_add at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 5: In the PERF_REPORT, read from the Arc instead of the variable
# Replace files_repaired with files_repaired.load(Ordering::Relaxed)
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'PERF_REPORT' in line and 'files_repaired' in line:
        # Replace the PERF_REPORT line to use .load()
        lines[i] = line.replace('files_repaired', 'files_repaired.load(std::sync::atomic::Ordering::Relaxed)')
        lines[i] = lines[i].replace('files_container_copy', 'files_container_copy.load(std::sync::atomic::Ordering::Relaxed)')
        lines[i] = lines[i].replace('files_re_encoded', 'files_re_encoded.load(std::sync::atomic::Ordering::Relaxed)')
        changes += 1
        print(f"FIX 5: Updated PERF_REPORT to use Arc.load() at line {i+1}")
        break

# Also fix any other references to these counters in log lines
for i, line in enumerate(lines):
    if 'files_repaired' in line and '.load(' not in line and 'let ' not in line and 'Arc' not in line and 'clone' not in line and 'fetch_add' not in line and 'AtomicUsize' not in line and 'PERF_REPORT' not in line:
        if 'log::info!' in line or 'log::warn!' in line:
            lines[i] = line.replace('files_repaired', 'files_repaired.load(std::sync::atomic::Ordering::Relaxed)')
            lines[i] = lines[i].replace('files_container_copy', 'files_container_copy.load(std::sync::atomic::Ordering::Relaxed)')
            lines[i] = lines[i].replace('files_re_encoded', 'files_re_encoded.load(std::sync::atomic::Ordering::Relaxed)')
            changes += 1
            print(f"FIX 5b: Updated log line to use Arc.load() at line {i+1}")

# Write the file
with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f"\nTotal changes: {changes}")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    verify = f.read()

checks = [
    ('AtomicUsize::new(0)', 'Arc<AtomicUsize> declarations'),
    ('files_repaired_cl.clone()', 'Arc clones before spawn'),
    ('files_repaired_cl.fetch_add', 'fetch_add in spawned task'),
    ('files_repaired.load(', 'Arc.load() in PERF_REPORT'),
]

print("\nVerification:")
for pattern, label in checks:
    count = verify.count(pattern)
    print(f"  [{'PASS' if count > 0 else 'FAIL'}] {label} (found {count}x)")

# Check no old patterns remain
bad_patterns = [
    ('let mut files_repaired: usize', 'old counter declaration'),
    ('let _ = files_repaired', 'suppression code'),
]
for pattern, label in bad_patterns:
    count = verify.count(pattern)
    print(f"  [{'PASS' if count == 0 else 'FAIL'}] No {label} (found {count}x)")
