#!/usr/bin/env python3
"""Fix repair_start scope errors in helper methods."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Fix line 8004: repair_start -> phase_start (in repair_timestamp_or_reencode)
fixed = 0
for i, line in enumerate(lines):
    if 'state.repair_duration_ms = repair_start.elapsed()' in line and i > 7900:
        # Check which function we're in by looking backward for the function signature
        for j in range(i, max(0, i - 100), -1):
            if 'fn repair_reencode_only' in lines[j]:
                # We're in repair_reencode_only - it has phase_start defined
                lines[i] = line.replace('repair_start.elapsed()', 'phase_start.elapsed()')
                fixed += 1
                print(f'[FIXED] Line {i+1}: repair_start -> phase_start (repair_reencode_only)')
                break
            elif 'fn repair_timestamp_or_reencode' in lines[j]:
                # We're in repair_timestamp_or_reencode - it has phase_start defined
                lines[i] = line.replace('repair_start.elapsed()', 'phase_start.elapsed()')
                fixed += 1
                print(f'[FIXED] Line {i+1}: repair_start -> phase_start (repair_timestamp_or_reencode)')
                break
            elif 'fn repair_single' in lines[j]:
                # We're in repair_single - repair_start IS defined here, skip
                break

print(f'Fixed {fixed} references')

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print('[DONE]')
