#!/usr/bin/env python3
"""Fix profiler gaps: phase_start bug for skipped phases + counter increments."""

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

print('=== PROFILER GAP FIXES ===\n')

# ── FIX 1: Add SKIPPED logging + phase_start reset for media validation ──
replace(
    '    log::info!("[MEDIA_VALIDATION] Media Validation Engine DISABLED (enable in settings)");',
    '    log::info!("[MEDIA_VALIDATION] Media Validation Engine DISABLED (enable in settings)");\n    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | SKIPPED (disabled)");\n    phase_start = std::time::Instant::now();',
    '1. Media validation SKIPPED reset'
)

# ── FIX 2: Add SKIPPED logging + phase_start reset for timestamp cert ──
replace(
    '        log::info!("[CERT:PACKET_TS] Packet timestamp certification DISABLED (enable in settings to check)");',
    '        log::info!("[CERT:PACKET_TS] Packet timestamp certification DISABLED (enable in settings to check)");\n        log::info!("[STAGE_TIMING] TIMESTAMP_CERT | SKIPPED (disabled)");\n        phase_start = std::time::Instant::now();',
    '2. Timestamp cert SKIPPED reset'
)

# ── FIX 3: Add SKIPPED logging for subtitles when not processing ──
replace(
    '    if should_process_subs {',
    '    if should_process_subs {\n        phase_start = std::time::Instant::now();',
    '3. Subtitles phase_start reset at start'
)

# ── FIX 4: Add files_total counter increment ──
# The total file count is available right after dedup
replace(
    '    log::info!("[STAGE_TIMING] PROBE | duration={:.2}s | files={}", dur_probe, working_input_files.len());\n    phase_start = std::time::Instant::now();',
    '    log::info!("[STAGE_TIMING] PROBE | duration={:.2}s | files={}", dur_probe, working_input_files.len());\n    files_total = working_input_files.len();\n    phase_start = std::time::Instant::now();',
    '4. files_total counter'
)

# ── FIX 5: Add files_healthy increment after validation ──
# After media validation, count quarantined files
replace(
    '    dur_validation = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | duration={:.2}s", dur_validation);\n    phase_start = std::time::Instant::now();',
    '    dur_validation = phase_start.elapsed().as_secs_f64();\n    files_healthy = working_input_files.len();\n    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | duration={:.2}s | files_after={}", dur_validation, working_input_files.len());\n    phase_start = std::time::Instant::now();',
    '5. files_healthy counter (initial count — quarantined files removed before this)'
)

# ── FIX 6: Reset phase_start before normalization starts ──
# Find the normalization phase start marker
replace(
    '        log::info!("[STAGE_TIMING] NORMALIZATION | duration={:.2}s", dur_normalization);\n            phase_start = std::time::Instant::now();',
    '        log::info!("[STAGE_TIMING] NORMALIZATION | duration={:.2}s", dur_normalization);\n            phase_start = std::time::Instant::now();',
    '6. Check normalization phase_start (already correct)'
)

print(f'\nTotal fixes: {edits}')

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print('File written successfully.')
