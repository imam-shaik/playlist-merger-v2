#!/usr/bin/env python3
"""Upgrade timing instrumentation to per-phase durations with per-file timing."""

import sys

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

edits = 0

def replace(old, new, label, count=1):
    global content, edits
    if old in content:
        content = content.replace(old, new, count)
        edits += 1
        print(f'  + {label}: DONE')
    else:
        print(f'  ! {label}: NOT FOUND')

print('=== P0 PROFILER UPGRADE ===\n')

# ── 1. Add per-phase Instant declarations after merge_start ──
replace(
    '    let merge_start = std::time::Instant::now();\n    log::info!("[STAGE_TIMING] ═══════════════════════════════════════════════════════════");\n    log::info!("[STAGE_TIMING] MERGE START — jobId={}", request.job_id);\n    log::info!("[STAGE_TIMING] ═══════════════════════════════════════════════════════════");',
    '    let merge_start = std::time::Instant::now();\n    let mut phase_start = std::time::Instant::now();\n    // Per-phase durations for the final performance report\n    let mut dur_probe: f64 = 0.0;\n    let mut dur_validation: f64 = 0.0;\n    let mut dur_timestamp_cert: f64 = 0.0;\n    let mut dur_subtitles: f64 = 0.0;\n    let mut dur_audio_validation: f64 = 0.0;\n    let mut dur_normalization: f64 = 0.0;\n    let mut dur_mkvmerge: f64 = 0.0;\n    let mut dur_ffmpeg_concat: f64 = 0.0;\n    // Per-file counters\n    let mut files_total: usize = 0;\n    let mut files_healthy: usize = 0;\n    let mut files_damaged: usize = 0;\n    let mut files_repaired: usize = 0;\n    let mut files_re_encoded: usize = 0;\n    log::info!("[STAGE_TIMING] ═══════════════════════════════════════════════════════════");\n    log::info!("[STAGE_TIMING] MERGE START — jobId={}", request.job_id);\n    log::info!("[STAGE_TIMING] ═══════════════════════════════════════════════════════════");',
    '1. Per-phase Instant declarations + counters'
)

# ── 2. Replace PROBE_COMPLETE with per-phase duration ──
replace(
    '    log::info!("[STAGE_TIMING] PROBE_COMPLETE | elapsed={:.1}s | files={}", merge_start.elapsed().as_secs_f64(), working_input_files.len());',
    '    dur_probe = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] PROBE | duration={:.2}s | files={}", dur_probe, working_input_files.len());\n    phase_start = std::time::Instant::now();',
    '2. Probe per-phase duration'
)

# ── 3. Replace MEDIA_VALIDATION_START ──
replace(
    '    log::info!("[STAGE_TIMING] MEDIA_VALIDATION_START | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | start", );',
    '3. Media val start (no change needed, phase_start tracks it)'
)

# ── 4. Replace MEDIA_VALIDATION_COMPLETE (insert before TIMESTAMP_CERT) ──
replace(
    '    log::info!("[STAGE_TIMING] MEDIA_VALIDATION_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '    dur_validation = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | duration={:.2}s", dur_validation);\n    phase_start = std::time::Instant::now();',
    '4. Media validation per-phase duration'
)

# ── 5. Replace TIMESTAMP_CERT_COMPLETE ──
replace(
    '    log::info!("[STAGE_TIMING] TIMESTAMP_CERT_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '    dur_timestamp_cert = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] TIMESTAMP_CERT | duration={:.2}s", dur_timestamp_cert);\n    phase_start = std::time::Instant::now();',
    '5. Timestamp cert per-phase duration'
)

# ── 6. Replace AUDIO_VALIDATION_COMPLETE ──
replace(
    '    log::info!("[STAGE_TIMING] AUDIO_VALIDATION_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '    dur_audio_validation = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] AUDIO_VALIDATION | duration={:.2}s", dur_audio_validation);\n    phase_start = std::time::Instant::now();',
    '6. Audio validation per-phase duration'
)

# ── 7. Replace SUBTITLES_COMPLETE ──
replace(
    '    log::info!("[STAGE_TIMING] SUBTITLES_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '    dur_subtitles = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] SUBTITLES | duration={:.2}s", dur_subtitles);\n    phase_start = std::time::Instant::now();',
    '7. Subtitles per-phase duration'
)

# ── 8. Replace NORMALIZATION_COMPLETE ──
replace(
    '            log::info!("[STAGE_TIMING] NORMALIZATION_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '            dur_normalization = phase_start.elapsed().as_secs_f64();\n            log::info!("[STAGE_TIMING] NORMALIZATION | duration={:.2}s", dur_normalization);\n            phase_start = std::time::Instant::now();',
    '8. Normalization per-phase duration'
)

# ── 9. Replace MKVMERGE_COMPLETE ──
replace(
    '    log::info!("[STAGE_TIMING] MKVMERGE_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '    dur_mkvmerge = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] MKVMERGE | duration={:.2}s", dur_mkvmerge);\n    phase_start = std::time::Instant::now();',
    '9. mkvmerge per-phase duration'
)

# ── 10. Replace FFMPEG_CONCAT_COMPLETE ──
replace(
    '        log::info!("[STAGE_TIMING] FFMPEG_CONCAT_COMPLETE | elapsed={:.1}s", merge_start.elapsed().as_secs_f64());',
    '        dur_ffmpeg_concat = phase_start.elapsed().as_secs_f64();\n        log::info!("[STAGE_TIMING] FFMPEG_CONCAT | duration={:.2}s", dur_ffmpeg_concat);',
    '10. FFmpeg concat per-phase duration'
)

# ── 11. Replace final TOTAL MERGE TIME with full performance report ──
old_total = '''                log::info!("[STAGE_TIMING] ========================================================");
                log::info!("[STAGE_TIMING] TOTAL MERGE TIME: {:.1}s ({:.1}min)", merge_start.elapsed().as_secs_f64(), merge_start.elapsed().as_secs_f64() / 60.0);
                log::info!("[STAGE_TIMING] ========================================================");'''

new_report = '''                let total_secs = merge_start.elapsed().as_secs_f64();
                let total_pct = |d: f64| if total_secs > 0.0 { (d / total_secs * 100.0) } else { 0.0 };
                log::info!("[PERF_REPORT] ========================================================================");
                log::info!("[PERF_REPORT]                        MERGE PERFORMANCE REPORT");
                log::info!("[PERF_REPORT] ========================================================================");
                log::info!("[PERF_REPORT]  Files:       {} total | {} healthy | {} damaged | {} repaired | {} re-encoded", files_total, files_healthy, files_damaged, files_repaired, files_re_encoded);
                log::info!("[PERF_REPORT] ------------------------------------------------------------------------");
                log::info!("[PERF_REPORT]  Probe:            {:>7.2}s  ({:>5.1}%)", dur_probe, total_pct(dur_probe));
                log::info!("[PERF_REPORT]  Validation:       {:>7.2}s  ({:>5.1}%)", dur_validation, total_pct(dur_validation));
                log::info!("[PERF_REPORT]  Timestamp Cert:   {:>7.2}s  ({:>5.1}%)", dur_timestamp_cert, total_pct(dur_timestamp_cert));
                log::info!("[PERF_REPORT]  Subtitles:        {:>7.2}s  ({:>5.1}%)", dur_subtitles, total_pct(dur_subtitles));
                log::info!("[PERF_REPORT]  Audio Validation: {:>7.2}s  ({:>5.1}%)", dur_audio_validation, total_pct(dur_audio_validation));
                log::info!("[PERF_REPORT]  Normalization:    {:>7.2}s  ({:>5.1}%)", dur_normalization, total_pct(dur_normalization));
                log::info!("[PERF_REPORT]  mkvmerge:         {:>7.2}s  ({:>5.1}%)", dur_mkvmerge, total_pct(dur_mkvmerge));
                log::info!("[PERF_REPORT]  FFmpeg Concat:    {:>7.2}s  ({:>5.1}%)", dur_ffmpeg_concat, total_pct(dur_ffmpeg_concat));
                log::info!("[PERF_REPORT] ------------------------------------------------------------------------");
                log::info!("[PERF_REPORT]  TOTAL:            {:>7.2}s  ({:.1} min)", total_secs, total_secs / 60.0);
                log::info!("[PERF_REPORT] ========================================================================");'''

replace(old_total, new_report, '11. Full performance report')

print(f'\nTotal edits: {edits}')

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print('File written successfully.')
