#!/usr/bin/env python3
"""Fix repair_single: remove orphaned duplicate and fix state.identity references."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

print(f'Original total lines: {len(lines)}')

# Step 1: Remove the orphaned repair_reencode_only at line 3117 (and its doc comment at 3115)
# Find it precisely
orphan_start = None
for i, line in enumerate(lines):
    if '/// Phase C: Re-encode (last resort).' in line and i < 3200:
        # Check if this is the orphaned one (before line 3200)
        orphan_start = i
        break

if orphan_start is not None:
    # Find the end by tracking brace depth
    brace_depth = 0
    found_first = False
    orphan_end = None
    for i in range(orphan_start, len(lines)):
        for ch in lines[i]:
            if ch == '{':
                brace_depth += 1
                found_first = True
            elif ch == '}':
                brace_depth -= 1
        if found_first and brace_depth == 0:
            orphan_end = i
            break

    if orphan_end:
        print(f'Removing orphaned repair_reencode_only: lines {orphan_start + 1} to {orphan_end + 1}')
        # Also remove any blank lines before it
        while orphan_start > 0 and lines[orphan_start - 1].strip() == '':
            orphan_start -= 1
        del lines[orphan_start:orphan_end + 1]
        print(f'Removed {orphan_end - orphan_start + 1} lines')
    else:
        print('[WARN] Could not find end of orphaned repair_reencode_only')

# Step 2: Fix state.identity references
# state.identity.file_name -> state.original_name
# state.identity.original_path -> state.original_path
fixed_count = 0
for i, line in enumerate(lines):
    if 'state.identity.file_name' in line:
        lines[i] = line.replace('state.identity.file_name', 'state.original_name')
        fixed_count += 1
    if 'state.identity.original_path' in line:
        lines[i] = lines[i].replace('state.identity.original_path', 'state.original_path')
        fixed_count += 1
    # Also fix file_name references that should be original_name
    if 'let file_name = state.original_name.clone();' in line.strip():
        pass  # Already correct
    elif 'let file_name = state.identity.file_name.clone();' in line:
        lines[i] = line.replace('state.identity.file_name', 'state.original_name')
        fixed_count += 1

print(f'Fixed {fixed_count} state.identity references')

# Step 3: Verify the three repair methods exist in correct location
repair_methods_found = []
for i, line in enumerate(lines):
    s = line.strip()
    if 'fn repair_single' in s and 'pub fn' in s:
        repair_methods_found.append(('repair_single', i + 1))
    elif 'fn repair_timestamp_or_reencode' in s:
        repair_methods_found.append(('repair_timestamp_or_reencode', i + 1))
    elif 'fn repair_reencode_only' in s:
        repair_methods_found.append(('repair_reencode_only', i + 1))

print(f'Repair methods found: {repair_methods_found}')

# Step 4: Verify they are inside impl MediaValidationEngine
for name, line_num in repair_methods_found:
    # Find the impl block that contains this line
    impl_name = None
    for i in range(line_num - 1, -1, -1):
        s = lines[i].strip()
        if s.startswith('impl ') and 'for ' not in s:
            impl_name = s
            break
        elif s.startswith('impl ') and 'for ' in s:
            impl_name = s
            break
    print(f'  {name} at line {line_num} is inside: {impl_name}')

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f'New total lines: {len(lines)}')
print('[DONE]')
