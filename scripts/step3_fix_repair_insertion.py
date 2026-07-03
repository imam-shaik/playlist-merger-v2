#!/usr/bin/env python3
"""Fix repair_single: remove from inside analyze_single, re-insert before impl closing brace."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()
    lines = content.split('\n')

print(f'Total lines: {len(lines)}')

# Step 1: Find the incorrectly inserted repair_single block
# It starts with "    /// Phase 2: Attempt repair on a single damaged file."
# and ends with "        state\n    }\n"
repair_start = None
repair_end = None

for i, line in enumerate(lines):
    if '/// Phase 2: Attempt repair on a single damaged file.' in line and i < 1600:
        repair_start = i
        break

if repair_start is None:
    print('[ERROR] Could not find repair_single start')
    exit(1)

# Find the end: look for "        state\n    }\n" which is the last return in repair_reencode_only
# or just find the pattern where the method ends
# The last helper method is repair_reencode_only which ends with "        state\n    }\n"
# But we need to find the exact end of the entire repair_single block

# Strategy: track brace depth from repair_start
brace_depth = 0
found_first_brace = False
for i in range(repair_start, len(lines)):
    for ch in lines[i]:
        if ch == '{':
            brace_depth += 1
            found_first_brace = True
        elif ch == '}':
            brace_depth -= 1
    if found_first_brace and brace_depth == 0:
        repair_end = i
        break

if repair_end is None:
    print('[ERROR] Could not find repair_single end')
    exit(1)

print(f'Incorrectly placed repair_single: lines {repair_start + 1} to {repair_end + 1}')
print(f'First line: {lines[repair_start][:100]}')
print(f'Last line: {lines[repair_end][:100]}')

# Step 2: Remove those lines
removed_block = '\n'.join(lines[repair_start:repair_end + 1])
removed_count = repair_end - repair_start + 1
del lines[repair_start:repair_end + 1]
print(f'Removed {removed_count} lines')

# Step 3: Find the impl block closing brace (last line of file)
impl_close = len(lines) - 1
while impl_close > 0 and lines[impl_close].strip() != '}':
    impl_close -= 1

print(f'Impl block closing brace at line {impl_close + 1}')

# Step 4: Insert the repair_single block before the impl closing brace
# Add a blank line before it
insert_lines = removed_block.split('\n')
lines[impl_close:impl_close] = [''] + insert_lines
print(f'Inserted {len(insert_lines)} lines before impl closing brace')

with open(path, 'w', encoding='utf-8') as f:
    f.write('\n'.join(lines))

print('[DONE] repair_single relocated inside impl block')
