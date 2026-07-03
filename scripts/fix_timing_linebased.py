#!/usr/bin/env python3
"""Fix remaining timing insertions using line-number-based approach.
These 3 insertions failed due to Unicode string matching issues."""

import sys

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    lines = f.readlines()

edits = 0

# 1. Media validation: insert timing log BEFORE line 1871 (the comment block)
# Line 1871 is the long comment line before media validation
# We need to find the exact line and insert before it
for i, line in enumerate(lines):
    if 'PROBE_COMPLETE' in line and 'STAGE_TIMING' in line:
        # Found the probe timing line. Media validation should be just after it.
        # Insert after the blank line following PROBE_COMPLETE
        for j in range(i+1, min(i+5, len(lines))):
            if lines[j].strip() == '' and j+1 < len(lines) and lines[j+1].strip().startswith('//'):
                # Found the blank line before the comment block
                insert_text = '    log::info!("[STAGE_TIMING] MEDIA_VALIDATION_START | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n\n'
                lines.insert(j+1, insert_text)
                edits += 1
                print(f'  + media validation start: INSERTED at line {j+2}')
                break
        break

# 2. Audio validation: find AUDIO_MODE section and insert before it
for i, line in enumerate(lines):
    if 'FORENSIC: Mode Summary' in line and 'FORENSIC' in line:
        # Insert before the FORENSIC comment line
        insert_text = '    log::info!("[STAGE_TIMING] AUDIO_VALIDATION_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n\n'
        lines.insert(i, insert_text)
        edits += 1
        print(f'  + audio validation complete: INSERTED at line {i+1}')
        break

# 3. Final total: find MERGE JOB SUMMARY box and insert before it
for i, line in enumerate(lines):
    if 'MERGE JOB SUMMARY' in line:
        # Search backwards for the box-drawing line
        for j in range(i, max(i-5, 0), -1):
            if lines[j].strip().startswith('log::info!'):
                insert_text = '                log::info!("[STAGE_TIMING] ========================================================");\n                log::info!("[STAGE_TIMING] TOTAL MERGE TIME: {:.1}s ({:.1}min)", merge_start.elapsed().as_secs_f64(), merge_start.elapsed().as_secs_f64() / 60.0);\n                log::info!("[STAGE_TIMING] ========================================================");\n\n'
                lines.insert(j, insert_text)
                edits += 1
                print(f'  + final total timing: INSERTED at line {j+1}')
                break
        break

print(f'\nTotal fixes applied: {edits}')

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.writelines(lines)

print('File written successfully.')
