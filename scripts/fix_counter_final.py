#!/usr/bin/env python3
"""Fix the counter scope bug properly: insert fetch_add inside spawned tasks."""

path = 'src-tauri/src/commands/merge.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

changes = 0

# ═══════════════════════════════════════════════════════════════
# FIX 1: Move Arc clones INSIDE the loop (before each js.spawn)
# Currently they're at lines 4180-4182 but may be outside the loop
# ═══════════════════════════════════════════════════════════════
# First, remove the misplaced clones
for i, line in enumerate(lines):
    if 'files_repaired_cl = files_repaired.clone()' in line:
        # Remove this line and the next 2 (container_copy_cl and re_encoded_cl)
        for j in range(3):
            if i < len(lines) and 'files_container_copy_cl' in lines[i] or 'files_re_encoded_cl' in lines[i]:
                lines.pop(i)
            elif i < len(lines):
                lines.pop(i)
        changes += 1
        print(f"FIX 1a: Removed misplaced Arc clones at line {i+1}")
        break

# Now find the first js.spawn and insert clones right before it (inside the loop)
for i, line in enumerate(lines):
    if 'js.spawn(async move {' in line and i > 4150 and i < 4200:
        clone_block = (
            '                let files_repaired_cl = files_repaired.clone();\n'
            '                let files_container_copy_cl = files_container_copy.clone();\n'
            '                let files_re_encoded_cl = files_re_encoded.clone();\n'
        )
        lines.insert(i, clone_block)
        changes += 1
        print(f"FIX 1b: Inserted Arc clones inside loop before first js.spawn at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 2: Insert fetch_add calls inside the spawned task
# Best place: after the match res block, inside Ok(path) handler
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'record_file_norm_end(idx, &path)' in line and i > 4260 and i < 4280:
        # Insert fetch_add calls after this line
        fetch_add_block = (
            '                            // -- Normalization counters (Arc) --\n'
            '                            if only_remux && dm.timescale_den.is_some() {\n'
            '                                files_repaired_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
            '                            }\n'
            '                            if only_remux && dm.timescale_den.is_none() {\n'
            '                                files_container_copy_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
            '                            }\n'
            '                            if !only_remux {\n'
            '                                files_re_encoded_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
            '                            }\n'
        )
        lines.insert(i + 1, fetch_add_block)
        changes += 1
        print(f"FIX 2: Inserted fetch_add calls inside spawned task at line {i+2}")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 3: Add Arc clones for the second spawn (audio normalization)
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'js.spawn(async move {' in line and i > 4450 and i < 4480:
        # Check if clones already exist
        has_clones = False
        for j in range(max(0, i-5), i):
            if 'files_repaired_cl' in lines[j]:
                has_clones = True
                break
        if not has_clones:
            clone_block = (
                '                let files_repaired_cl = files_repaired.clone();\n'
                '                let files_container_copy_cl = files_container_copy.clone();\n'
                '                let files_re_encoded_cl = files_re_encoded.clone();\n'
            )
            lines.insert(i, clone_block)
            changes += 1
            print(f"FIX 3: Inserted Arc clones for audio normalization spawn at line {i+1}")
        else:
            print(f"FIX 3: Arc clones already exist for audio normalization spawn")
        break

# ═══════════════════════════════════════════════════════════════
# FIX 4: Also add fetch_add calls inside the audio normalization task
# Find the audio normalization's Ok(path) handler
# ═══════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if 'record_file_norm_end(idx, &path)' in line and i > 4500:
        # Check if fetch_add already exists nearby
        has_fetch = False
        for j in range(i, min(i+10, len(lines))):
            if 'fetch_add' in lines[j]:
                has_fetch = True
                break
        if not has_fetch:
            fetch_add_block = (
                '                            // -- Normalization counters (Arc) --\n'
                '                            if only_remux && dm.timescale_den.is_some() {\n'
                '                                files_repaired_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
                '                            }\n'
                '                            if only_remux && dm.timescale_den.is_none() {\n'
                '                                files_container_copy_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
                '                            }\n'
                '                            if !only_remux {\n'
                '                                files_re_encoded_cl.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n'
                '                            }\n'
            )
            lines.insert(i + 1, fetch_add_block)
            changes += 1
            print(f"FIX 4: Inserted fetch_add calls in audio normalization task at line {i+2}")
        else:
            print(f"FIX 4: fetch_add already exists in audio normalization task")
        break

# Write the file
with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f"\nTotal changes: {changes}")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    verify = f.read()

checks = [
    ('files_repaired_cl.fetch_add', 'fetch_add for files_repaired'),
    ('files_container_copy_cl.fetch_add', 'fetch_add for files_container_copy'),
    ('files_re_encoded_cl.fetch_add', 'fetch_add for files_re_encoded'),
    ('files_repaired_cl = files_repaired.clone()', 'Arc clone for files_repaired'),
    ('files_repaired.load(', 'Arc.load() in PERF_REPORT'),
]

print("\nVerification:")
for pattern, label in checks:
    count = verify.count(pattern)
    print(f"  [{'PASS' if count > 0 else 'FAIL'}] {label} (found {count}x)")
