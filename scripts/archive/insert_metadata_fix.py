#!/usr/bin/env python3
"""Insert METADATA_FIX section after Duration Consistency Check in merge.rs."""
import sys

filepath = 'src-tauri/src/commands/merge.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

# Find the insertion point: after the Duration Consistency Check's closing `}`
# The marker is: "        }" followed by blank line, followed by "        // Track validation start time"
target_marker = '        // Track validation start time for forensic timing'
insert_pos = content.find(target_marker)

if insert_pos == -1:
    print("ERROR: Could not find insertion marker")
    sys.exit(1)

# Go back to find the start of the line
line_start = content.rfind('\n', 0, insert_pos)
if line_start == -1:
    line_start = 0

# The insert position is at the start of the line containing the marker
insert_pos = line_start + 1

print(f"Found insertion point at position {insert_pos}")
print(f"Context before: ...{content[insert_pos-80:insert_pos].rstrip()}...")
print(f"Context after:  ...{content[insert_pos:insert_pos+80].rstrip()}...")

# The METADATA_FIX section to insert
metadata_fix_blob = """\
        // ── Post-mkvmerge container metadata fix ──────────────────────────────────────────────
        // When mkvmerge concatenates files with mixed audio sample rates (44100/48000 Hz) or
        // mixed video profiles (Main/High), the MKV container header duration is corrupted
        // (e.g., reports 942s instead of 31260s). The actual data IS present (confirmed by
        // file-size validation). Fix by running FFmpeg genpts remux which forces PTS/timestamp
        // recalculation and recalculates the MKV SegmentInfo.Duration from actual frame timestamps.
        if let Some(Ok(ref info)) = &output_probe_result {
            let drift_ratio = if final_total_duration > 0.0 { info.duration / final_total_duration } else { 1.0 };
            if drift_ratio < 0.5 || drift_ratio > 2.0 {
                log::info!("[METADATA_FIX] ═══════════════════════════════════════════════════════════");
                log::info!("[METADATA_FIX] Detected corrupted container duration: ffprobe={:.1}s, expected={:.1}s (ratio: {:.1}%)",
                    info.duration, final_total_duration, drift_ratio * 100.0);
                log::info!("[METADATA_FIX] Running FFmpeg genpts remux to fix corrupted container duration...");

                let temp_fix_path = format!("{}.metadata_fix{}", primary_output_path,
                    std::path::Path::new(&primary_output_path)
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default());

                let original_size = result.as_ref().map(|r| r.output_size_bytes).unwrap_or(0);

                // FFmpeg genpts remux: forces PTS/timestamp regeneration, which recalculates
                // the MKV SegmentInfo.Duration from actual frame timestamps rather than
                // from the (corrupted) container header that mkvmerge wrote.
                let ffmpeg_fix = std::process::Command::new(&ffmpeg_path_resolved)
                    .arg("-fflags")
                    .arg("+genpts")
                    .arg("-i")
                    .arg(&primary_output_path)
                    .arg("-c")
                    .arg("copy")
                    .arg("-map")
                    .arg("0")
                    .arg("-avoid_negative_ts")
                    .arg("make_zero")
                    .arg("-y")
                    .arg(&temp_fix_path)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output();

                match ffmpeg_fix {
                    Ok(ref cmd_output) if cmd_output.status.success() => {
                        if let Ok(temp_meta) = std::fs::metadata(&temp_fix_path) {
                            let temp_size = temp_meta.len();
                            let size_ratio = if original_size > 0 { (temp_size as f64) / (original_size as f64) * 100.0 } else { 100.0 };
                            log::info!("[METADATA_FIX] FFmpeg genpts remux complete: {} bytes -> {} bytes ({:.1}%)", original_size, temp_size, size_ratio);

                            if size_ratio >= 90.0 && size_ratio <= 110.0 {
                                // Remux produced valid output -- replace original
                                if let Err(e) = std::fs::rename(&temp_fix_path, &primary_output_path) {
                                    // rename may fail across volumes; fall back to remove + rename
                                    let _ = std::fs::remove_file(&primary_output_path);
                                    if let Err(e2) = std::fs::rename(&temp_fix_path, &primary_output_path) {
                                        log::warn!("[METADATA_FIX] Failed to replace output after ffmpeg fix: {} / {}", e, e2);
                                        let _ = std::fs::remove_file(&temp_fix_path);
                                    }
                                }

                                // Re-probe to get corrected duration
                                if let Ok(fixed_info) = crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, std::path::Path::new(&primary_output_path)) {
                                    log::info!("[METADATA_FIX] Fixed duration: {:.1}s (was {:.1}s, expected {:.1}s)",
                                        fixed_info.duration, info.duration, final_total_duration);
                                    actual_duration = fixed_info.duration;
                                    output_probe_result = Some(Ok(fixed_info));
                                }
                            } else {
                                log::warn!("[METADATA_FIX] FFmpeg remux size mismatch ({:.1}%), keeping original", size_ratio);
                                let _ = std::fs::remove_file(&temp_fix_path);
                            }
                        } else {
                            log::warn!("[METADATA_FIX] FFmpeg remux output not found, keeping original");
                            let _ = std::fs::remove_file(&temp_fix_path);
                        }
                    }
                    Ok(cmd_output) => {
                        log::warn!("[METADATA_FIX] FFmpeg genpts remux failed (exit {}): {}",
                            cmd_output.status.code().unwrap_or(-1),
                            String::from_utf8_lossy(&cmd_output.stderr).chars().take(200).collect::<String>());
                        let _ = std::fs::remove_file(&temp_fix_path);
                    }
                    Err(e) => {
                        log::warn!("[METADATA_FIX] Failed to spawn FFmpeg genpts remux: {}", e);
                    }
                }
                log::info!("[METADATA_FIX] ═══════════════════════════════════════════════════════════");
            }
        }

"""

# Insert the METADATA_FIX section
new_content = content[:insert_pos] + metadata_fix_blob + content[insert_pos:]

with open(filepath, 'w', encoding='utf-8') as f:
    f.write(new_content)

print("SUCCESS: METADATA_FIX section inserted with FFmpeg genpts approach")
