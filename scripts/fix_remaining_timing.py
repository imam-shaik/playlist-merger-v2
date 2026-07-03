#!/usr/bin/env python3
"""Fix the 3 remaining timing instrumentation insertions."""

import sys

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

edits = 0

def insert_after(old, new, label):
    global content, edits
    if old in content:
        content = content.replace(old, new, 1)
        edits += 1
        print(f'  + {label}: INSERTED')
    else:
        print(f'  ! {label}: NOT FOUND')

# 1. Media validation timing - insert before the long comment block
insert_after(
    '    // ────────────────────────────────────────────────────────────────────────────\n    // Runs BEFORE packet timestamp certification and normalization.',
    '    log::info!("[STAGE_TIMING] MEDIA_VALIDATION_START | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n\n    // ────────────────────────────────────────────────────────────────────────────\n    // Runs BEFORE packet timestamp certification and normalization.',
    'media validation start timing'
)

# 2. Audio validation timing - insert before AUDIO_MODE section
insert_after(
    '    // ──── FORENSIC: Mode Summary ────────────────────────────────────────────────────────\n    log::info!("[AUDIO_MODE]',
    '    log::info!("[STAGE_TIMING] AUDIO_VALIDATION_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n\n    // ──── FORENSIC: Mode Summary ────────────────────────────────────────────────────────\n    log::info!("[AUDIO_MODE]',
    'audio validation timing'
)

# 3. Final total timing - insert before MERGE JOB SUMMARY box
insert_after(
    '                log::info!("╔════════════════════════════════════════════════════════════════════════════════════╗");\n                log::info!("║                         MERGE JOB SUMMARY',
    '                log::info!("[STAGE_TIMING] \u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550");\n                log::info!("[STAGE_TIMING] TOTAL MERGE TIME: {:.1}s ({:.1}min)", merge_start.elapsed().as_secs_f64(), merge_start.elapsed().as_secs_f64() / 60.0);\n                log::info!("[STAGE_TIMING] \u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550");\n\n                log::info!("╔════════════════════════════════════════════════════════════════════════════════════╗");\n                log::info!("║                         MERGE JOB SUMMARY',
    'final total timing'
)

print(f'\nFixes applied: {edits}')

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print('File written successfully.')
