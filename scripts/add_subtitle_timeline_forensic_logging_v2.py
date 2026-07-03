#!/usr/bin/env python3
"""Add forensic logging to merge.rs to capture subtitle timeline duration evidence."""

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# ====== 1. Add logging after P0-3 re-probe duration update ======
old_probe = '''                        if r.file_index < working_input_durations.len() {
                            working_input_durations[r.file_index] = dur;
                        }
                        // Also update probe_cache'''

new_probe = '''                        if r.file_index < working_input_durations.len() {
                            let orig_dur = working_input_durations[r.file_index];
                            working_input_durations[r.file_index] = dur;
                            let path_display = std::path::Path::new(path).file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("...{}", &path[path.len().saturating_sub(60)..]));
                            log::info!("[SUBTITLE_TIMELINE_CERT] Post-repair duration update | File #{} | {} | orig={:.6}s | repaired={:.6}s | delta={:.6}s",
                                r.file_index, path_display, orig_dur, dur, dur - orig_dur);
                        }
                        // Also update probe_cache'''

if old_probe in content:
    print("[OK] Found P0-3 re-probe block")
    content = content.replace(old_probe, new_probe, 1)
    print("[OK] P0-3 re-probe block updated")
else:
    print("[WARN] P0-3 re-probe block NOT FOUND - trying exact match...")
    # Find a smaller unique fragment
    idx = content.find("working_input_durations[r.file_index] = dur;")
    if idx >= 0:
        print(f"[OK] Found at position {idx}")
        print(repr(content[idx-50:idx+100]))
    else:
        print("[FAIL] Not found")

# ====== 2. Add logging at subtitle concat list write ======
old_sub = '''    if should_process_subs && final_prepared_subs.iter().any(|s| s.is_some()) {
        let suffix = if interleaved { "interleaved" } else { "direct" };
        let slp = temp_dir.join(format!("concat_sub_{}_{}.txt", suffix, request.job_id));
        if crate::ffmpeg::write_subtitle_concat_list(&final_prepared_subs, &final_input_durations, &slp, &temp_dir, "srt").is_ok() {
            final_subtitle_list_path = Some(slp);
        }
    }'''

new_sub = '''    if should_process_subs && final_prepared_subs.iter().any(|s| s.is_some()) {
        // -- [SUBTITLE_TIMELINE_CERT] Log exact durations being written to subtitle concat list --
        log::info!("[SUBTITLE_TIMELINE_CERT] ===== SUBTITLE CONCAT LIST WRITE - Duration Certification =====");
        log::info!("[SUBTITLE_TIMELINE_CERT] Segments: {} (with subs: {})", final_prepared_subs.len(),
            final_prepared_subs.iter().filter(|s| s.is_some()).count());
        let mut cum_video: f64 = 0.0;
        for (seg_idx, dur) in final_input_durations.iter().enumerate() {
            let is_card = final_segment_cards.get(seg_idx).copied().unwrap_or((false, None)).0;
            let has_sub = final_prepared_subs.get(seg_idx).and_then(|s| s.as_ref()).is_some();
            let seg_type = if is_card { "CARD" } else if has_sub { "VIDEO+SUB" } else { "VIDEO" };
            let sub_name = if has_sub {
                let p = final_prepared_subs[seg_idx].as_ref().unwrap();
                std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            } else { String::new() };
            log::info!("[SUBTITLE_TIMELINE_CERT]   [{:>4}] {:>12}  dur={:>12.6}s  cum={:>14.6}s{}",
                seg_idx, seg_type, dur, cum_video + dur,
                if has_sub { format!("  sub={}", sub_name) } else { String::new() });
            cum_video += dur;
        }
        log::info!("[SUBTITLE_TIMELINE_CERT] Total cumulative duration: {:.6}s", cum_video);
        log::info!("[SUBTITLE_TIMELINE_CERT] ============================================================");

        let suffix = if interleaved { "interleaved" } else { "direct" };
        let slp = temp_dir.join(format!("concat_sub_{}_{}.txt", suffix, request.job_id));
        if crate::ffmpeg::write_subtitle_concat_list(&final_prepared_subs, &final_input_durations, &slp, &temp_dir, "srt").is_ok() {
            final_subtitle_list_path = Some(slp);
        }
    }'''

if old_sub in content:
    print("[OK] Found subtitle concat list write")
    content = content.replace(old_sub, new_sub, 1)
    print("[OK] Subtitle concat list write updated")
else:
    print("[WARN] Subtitle concat list write pattern NOT FOUND")
    # Find the unique fragment
    for term in ["should_process_subs && final_prepared_subs.iter().any(|s| s.is_some())", "concat_sub_", "write_subtitle_concat_list"]:
        idx = content.find(term)
        if idx >= 0:
            print(f"[OK] Found '{term}' at position {idx}")
            print(repr(content[idx-20:idx+150]))
            break
    else:
        print("[FAIL] Not found")

# ====== Write back ======
with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("\n[DONE] merge.rs updated successfully")
