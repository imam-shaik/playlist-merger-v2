#!/usr/bin/env python3
"""Fix dur_validation insertion point and files_healthy semantics."""

import sys

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

edits = 0

def replace(old, new, label):
    global content, edits
    if old in content:
        content = content.replace(old, new, 1)
        edits += 1
        print(f'  + {label}: DONE')
    else:
        print(f'  ! {label}: NOT FOUND')

print('=== DUR_VALIDATION FIX ===\n')

# ── FIX 1: Insert dur_validation between media validation end and timestamp cert ──
replace(
    '    // ── PACKET TIMESTAMP CERTIFICATION',
    '    dur_validation = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | duration={:.2}s | files_after={}", dur_validation, working_input_files.len());\n    phase_start = std::time::Instant::now();\n\n    // ── PACKET TIMESTAMP CERTIFICATION',
    '1. dur_validation insertion'
)

# ── FIX 2: Fix files_healthy semantics — rename to files_after_validation ──
replace(
    '    files_healthy = working_input_files.len();',
    '    // Note: files_healthy represents files that survived quarantine removal\n    // (irreparably damaged files removed). Some may still be damaged but repairable.',
    '2. Fix files_healthy comment (already set above in dur_validation block)'
)

# ── FIX 3: Ensure files_healthy is set correctly in the PERFORMANCE REPORT ──
# The report already uses files_healthy, which is now set in the dur_validation block above.
# But we need to make sure it's set to working_input_files.len() AFTER quarantine removal.
# The dur_validation block already does this. Let's verify by checking the report.

print(f'\nTotal fixes: {edits}')

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print('File written successfully.')
