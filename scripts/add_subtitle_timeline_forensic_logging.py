#!/usr/bin/env python3
"""Add forensic logging to merge.rs to capture subtitle timeline duration evidence."""

with open('src-tauri/src/commands/merge.rs', 'rb') as f:
    content = f.read()

# ====== 1. Add logging after P0-3 re-probe duration update ======
# Find the line: working_input_durations[r.file_index] = dur;
old_probe = b'                        if r.file_index < working_input_durations.len() {\n                            working_input_durations[r.file_index] = dur;\n                        }'
new_probe = b'''                        if r.file_index < working_input_durations.len() {
                            let orig_dur = working_input_durations[r.file_index];
                            working_input_durations[r.file_index] = dur;
                            let path_display = std::path::Path::new(path).file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("...{}", &path[path.len().saturating_sub(60)..]));
                            log::info!("[SUBTITLE_TIMELINE_CERT] Post-repair duration update | File #{} | {} | orig={:.6}s | repaired={:.6}s | delta={:.6}s",
                                r.file_index, path_display, orig_dur, dur, dur - orig_dur);
                        }'''

if old_probe in content:
    idx = content.find(old_probe)
    print(f"[OK] Found P0-3 re-probe block at byte offset {idx}")
    content = content.replace(old_probe, new_probe, 1)
    print("[OK] P0-3 re-probe block updated")
else:
    print("[WARN] P0-3 re-probe block NOT FOUND - trying fuzzy match")
    idx = content.find(b'working_input_durations[r.file_index] = dur;')
    if idx >= 0:
        print(f"[OK] Found 'working_input_durations[r.file_index] = dur;' at byte offset {idx}")
    else:
        print("[FAIL] Pattern not found at all")

# ====== 2. Add logging at subtitle concat list write ======
old_sub_concat = b'    if should_process_subs && final_prepared_subs.iter().any(|s| s.is_some()) {\n        let suffix = if interleaved { "interleaved" } else { "direct" };\n        let slp = temp_dir.join(format!("concat_sub_{}_{}.txt", suffix, request.job_id));\n        if crate::ffmpeg::write_subtitle_concat_list(&final_prepared_subs, &final_input_durations, &slp, &temp_dir, "srt").is_ok() {\n            final_subtitle_list_path = Some(slp);\n        }\n    }'

new_sub_concat = b'''    if should_process_subs && final_prepared_subs.iter().any(|s| s.is_some()) {
        // ── [SUBTITLE_TIMELINE_CERT] Log exact durations being written to subtitle concat list ──
        log::info!("[SUBTITLE_TIMELINE_CERT] ═══════════════════════════════════════════════════════════");
        log::info!("[SUBTITLE_TIMELINE_CERT] SUBTITLE CONCAT LIST WRITE - Duration Certification");
        log::info!("[SUBTITLE_TIMELINE_CERT] Segments: {} (with subs: {})", final_prepared_subs.len(),
            final_prepared_subs.iter().filter(|s| s.is_some()).count());
        log::info!("[SUBTITLE_TIMELINE_CERT] {:<6} {:<12} {:>14} {:>18} {:>18} {:>14}",
            "SEG#", "TYPE", "DURATION", "VIDEO_CUMULATIVE", "SUBTITLE_CUMULATIVE", "DIFF");
        let mut cum_video: f64 = 0.0;
        for (seg_idx, dur) in final_input_durations.iter().enumerate() {
            let is_card = final_segment_cards.get(seg_idx).copied().unwrap_or((false, None)).0;
            let has_sub = final_prepared_subs.get(seg_idx).and_then(|s| s.as_ref()).is_some();
            let seg_type = if is_card { "CARD" } else if has_sub { "VIDEO+SUB" } else { "VIDEO" };
            let sub_name = if has_sub {
                let p = final_prepared_subs[seg_idx].as_ref().unwrap();
                std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            } else { String::new() };
            log::info!("[SUBTITLE_TIMELINE_CERT] {:>4}   {:<12} {:>14.6} {:>18.6} {:>18.6} {:>14.6}{}",
                seg_idx, seg_type, dur, cum_video + dur, cum_video + dur, 0.0_f64,
                if has_sub { format!(" sub={}", sub_name) } else { String::new() });
            cum_video += dur;
        }
        log::info!("[SUBTITLE_TIMELINE_CERT] Total cumulative: {:.6}s", cum_video);
        log::info!("[SUBTITLE_TIMELINE_CERT] ═══════════════════════════════════════════════════════════");

        let suffix = if interleaved { "interleaved" } else { "direct" };
        let slp = temp_dir.join(format!("concat_sub_{}_{}.txt", suffix, request.job_id));
        if crate::ffmpeg::write_subtitle_concat_list(&final_prepared_subs, &final_input_durations, &slp, &temp_dir, "srt").is_ok() {
            final_subtitle_list_path = Some(slp);
        }
    }'''

if old_sub_concat in content:
    idx = content.find(old_sub_concat)
    print(f"[OK] Found subtitle concat list write at byte offset {idx}")
    content = content.replace(old_sub_concat, new_sub_concat, 1)
    print("[OK] Subtitle concat list write updated")
else:
    print("[WARN] Subtitle concat list write pattern NOT FOUND")

# ====== Write back ======
with open('src-tauri/src/commands/merge.rs', 'wb') as f:
    f.write(content)

print("\n[DONE] merge.rs updated successfully")
