#!/usr/bin/env python3
"""Add stage-level timing instrumentation to start_merge() in merge.rs.
Inserts Instant::now() timers and elapsed logging at each major pipeline phase."""

import sys

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

edits = 0
found = 0

def insert_after(old, new, label):
    global content, edits, found
    if old in content:
        content = content.replace(old, new, 1)
        edits += 1
        found += 1
        print(f'  + {label}: INSERTED')
    else:
        found += 1
        print(f'  ! {label}: NOT FOUND - skipping')

print('=== STAGE TIMING INSTRUMENTATION ===')

# 1. Add merge_start timer after _forensic_guard creation
insert_after(
    '    };\n\n    log::info!("[Merge] ═══════════════════════════════════════════════════════════════");\n    log::info!("[Merge] Job ID: {}", request.job_id);',
    '    };\n    let merge_start = std::time::Instant::now();\n    log::info!("[STAGE_TIMING] ═══════════════════════════════════════════════════════════");\n    log::info!("[STAGE_TIMING] MERGE START — jobId={}", request.job_id);\n    log::info!("[STAGE_TIMING] ═══════════════════════════════════════════════════════════");\n\n    log::info!("[Merge] ═══════════════════════════════════════════════════════════════");\n    log::info!("[Merge] Job ID: {}", request.job_id);',
    '1. merge_start timer'
)

# 2. After probe — before MEDIA VALIDATION
insert_after(
    '    // \u2500\u2500 MEDIA VALIDATION ENGINE',
    '    log::info!("[STAGE_TIMING] PROBE_COMPLETE | elapsed={:.1}s | files={}", merge_start.elapsed().as_secs_f64(), working_input_files.len());\n\n    // \u2500\u2500 MEDIA VALIDATION ENGINE',
    '2. probe timing'
)

# 3. After media validation — before PACKET TIMESTAMP CERT
insert_after(
    '    // \u2500\u2500 PACKET TIMESTAMP CERTIFICATION',
    '    log::info!("[STAGE_TIMING] MEDIA_VALIDATION_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n\n    // \u2500\u2500 PACKET TIMESTAMP CERTIFICATION',
    '3. media validation timing'
)

# 4. After timestamp cert — before mode decisions
insert_after(
    '    let mut actual_mode = request.mode.clone();',
    '    log::info!("[STAGE_TIMING] TIMESTAMP_CERT_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n\n    let mut actual_mode = request.mode.clone();',
    '4. timestamp cert timing'
)

# 5. After subtitle processing — before temp_dir
insert_after(
    '    let temp_dir = get_temp_dir().map_err(|e| e.to_string())?;',
    '    log::info!("[STAGE_TIMING] SUBTITLES_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n\n    let temp_dir = get_temp_dir().map_err(|e| e.to_string())?;',
    '5. subtitle timing'
)

# 6. After audio validation decision tree — before AUDIO_MODE section
insert_after(
    '    log::info!("[AUDIO_MODE] \u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550");',
    '    log::info!("[STAGE_TIMING] AUDIO_VALIDATION_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n\n    log::info!("[AUDIO_MODE] \u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550");',
    '6. audio validation timing'
)

# 7. After normalization — search for NORMALIZATION_COMPLETE phase timing
insert_after(
    '            log::info!("[PHASE_TIMING] NORMALIZATION_COMPLETE | next=repeat_propagation_cards_mkvmerge_prep");',
    '            log::info!("[PHASE_TIMING] NORMALIZATION_COMPLETE | next=repeat_propagation_cards_mkvmerge_prep");\n            log::info!("[STAGE_TIMING] NORMALIZATION_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '7. normalization timing'
)

# 8. After mkvmerge prep — before FFmpeg concat
insert_after(
    '    log::info!("[PHASE_4] FFMPEG_CONCAT_PREP_START',
    '    log::info!("[STAGE_TIMING] MKVMERGE_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());\n    log::info!("[PHASE_4] FFMPEG_CONCAT_PREP_START',
    '8. mkvmerge timing'
)

# 9. After run_merge_blocking returns
insert_after(
    '        log::info!("[MERGE_TRACE] run_merge_blocking returned jobId={} result={:?}", job_id, result);',
    '        log::info!("[MERGE_TRACE] run_merge_blocking returned jobId={} result={:?}", job_id, result);\n        log::info!("[STAGE_TIMING] FFMPEG_CONCAT_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '9. ffmpeg concat timing'
)

# 10. Add final total to the MERGE JOB SUMMARY
insert_after(
    '    log::info!("[Merge] MERGE JOB SUMMARY");',
    '    log::info!("[STAGE_TIMING] \u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550");\n    log::info!("[STAGE_TIMING] TOTAL MERGE TIME: {:.1}s ({:.1}min)", merge_start.elapsed().as_secs_f64(), merge_start.elapsed().as_secs_f64() / 60.0);\n    log::info!("[STAGE_TIMING] \u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550\u2550");\n\n    log::info!("[Merge] MERGE JOB SUMMARY");',
    '10. final total timing'
)

print(f'\nTotal inserts attempted: {found}, successful: {edits}')

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print('File written successfully.')
