#!/usr/bin/env python3
"""Remove orphaned only_remux lines from audio normalization task."""

path = 'src-tauri/src/commands/merge.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Find all lines with only_remux after line 4540 (audio task)
to_remove = []
for i, line in enumerate(lines):
    if i > 4540 and 'only_remux' in line:
        to_remove.append(i)
        print(f"Line {i+1}: {line.rstrip()[:100]}")

# Remove from bottom to top
for idx in sorted(to_remove, reverse=True):
    lines.pop(idx)
    print(f"Removed line {idx+1}")

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f"\nRemoved {len(to_remove)} orphaned only_remux lines")
