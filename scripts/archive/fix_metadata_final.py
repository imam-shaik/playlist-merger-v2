#!/usr/bin/env python3
"""Surgically replace the corrupted METADATA_FIX section in merge.rs.

The current section (lines ~4644-4848) has overlapping/duplicate code from
failed script edits. This script replaces the entire corrupted block with
a clean, single FFmpeg genpts remux approach.
"""

import os
import re

project_root = r"C:\Users\IMAM\Desktop\playlist-merger-v2"
filepath = os.path.join(project_root, "src-tauri", "src", "commands", "merge.rs")

with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Find the start marker: the comment block about post-mkvmerge container metadata fix
start_marker = "// ── Post-mkvmerge container metadata fix ──"
start_idx = content.find(start_marker)
if start_idx == -1:
    # Try without the extra dash
    start_marker = "// ── Post-mkvmerge container metadata fix"
    start_idx = content.find(start_marker)

if start_idx == -1:
    print("ERROR: Could not find start marker")
    exit(1)

# Find the line start (beginning of the line containing the start marker)
line_start = content.rfind("\n", 0, start_idx)
if line_start == -1:
    line_start = 0
else:
    line_start += 1  # skip the newline

# Find the end marker: the block of code ends with the final closing brace 
# of the outer `if` block that follows "log.info(METADATA_FIX HEADER)"
# We need to find the end of the corrupted section.
# The final line should be: `log::info!("[METADATA_FIX] ════════════════════════════════════════");`
# followed by a closing `}`

end_marker = 'log::info!("[METADATA_FIX] ═══════════════════════════════════════════════════════════");'
end_idx = content.rfind(end_marker, start_idx)

if end_idx == -1:
    print("ERROR: Could not find end marker")
    exit(1)

# Go past the closing brace after the end marker
# Find the next '}' after the end_marker line
after_end = content.find("}", end_idx + len(end_marker))
if after_end == -1:
    print("ERROR: Could not find closing brace after end marker")
    exit(1)

# The corrupted section is from line_start to after_end + 1 (include the brace)
# But we need to also include the blank line after it
end_of_section = after_end + 1

# Now, let's look at what comes before the start marker to understand indentation
before_section = content[content.rfind("\n", 0, line_start-2)+1:line_start]
indent_match = re.match(r"^(\s*)", before_section)
base_indent = indent_match.group(1) if indent_match else "                    "

print(f"Found start marker at position {start_idx}")
print(f"Found end marker at position {end_idx}")
print(f"Section ends at position {end_of_section}")
print(f"Base indentation: {repr(base_indent)}")

# Build the replacement code
replacement = f"""        // ── Post-mkvmerge container metadata fix ──────────────────────────────────────────────
        // When mkvmerge concatenates files with mixed audio sample rates (44100/48000 Hz) or
        // mixed video profiles (Main/High), the MKV container header duration is corrupted
        // (e.g., reports 942s instead of 31260s). The actual data IS present (confirmed by
        // file-size validation). Fix by running FFmpeg genpts remux which forces PTS/timestamp
        // recalculation and recalculates the MKV SegmentInfo.Duration from actual frame timestamps.
        if result.is_ok() && mkvmerge_succeeded {{
            if let Some(Ok(ref info)) = &output_probe_result {{
                let drift_ratio = if final_total_duration > 0.0 {{ info.duration / final_total_duration }} else {{ 1.0 }};
                if drift_ratio < 0.5 || drift_ratio > 2.0 {{
                    log::info!("[METADATA_FIX] ═══════════════════════════════════════════════════════════");
                    log::info!("[METADATA_FIX] Detected corrupted container duration: ffprobe={{:.1}}s, expected={{:.1}}s (ratio: {{:.1}}%)",
                        info.duration, final_total_duration, drift_ratio * 100.0);
                    log::info!("[METADATA_FIX] Running FFmpeg genpts remux to fix corrupted container duration...");

                    // FFmpeg genpts remux: forces PTS/timestamp regeneration, which recalculates
                    // the MKV SegmentInfo.Duration from actual frame timestamps rather than
                    // from the (corrupted) container header that mkvmerge wrote.
                    let temp_fix_path = format!("{{}}.metadata_fix{{}}", output_path,
                        std::path::Path::new(&output_path)
                            .extension()
                            .map(|e| format!(".{{}}", e.to_string_lossy()))
                            .unwrap_or_default());

                    let original_size = result.as_ref().map(|r| r.output_size_bytes).unwrap_or(0);

                    // Use ffmpeg_path_for_fix (cloned before spawn_blocking to avoid move capture issues)
                    let ffmpeg_fix = std::process::Command::new(&ffmpeg_path_for_fix)
                        .arg("-fflags")
                        .arg("+genpts")
                        .arg("-i")
                        .arg(&output_path)
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

                    match ffmpeg_fix {{
                        Ok(ref cmd_output) if cmd_output.status.success() => {{
                            if let Ok(temp_meta) = std::fs::metadata(&temp_fix_path) {{
                                let temp_size = temp_meta.len();
                                let size_ratio = if original_size > 0 {{ (temp_size as f64) / (original_size as f64) * 100.0 }} else {{ 100.0 }};

                                log::info!("[METADATA_FIX] FFmpeg genpts remux complete: {{}} bytes -> {{}} bytes ({{:.1}}%)", original_size, temp_size, size_ratio);

                                if size_ratio >= 90.0 && size_ratio <= 110.0 {{
                                    // Remux produced valid output -- replace original
                                    if let Err(e) = std::fs::rename(&temp_fix_path, &output_path) {{
                                        // rename may fail across volumes; fall back to copy+delete
                                        let _ = std::fs::remove_file(&output_path);
                                        if let Err(e2) = std::fs::rename(&temp_fix_path, &output_path) {{
                                            log::warn!("[METADATA_FIX] Failed to replace output after ffmpeg fix: {{}} / {{}}", e, e2);
                                            let _ = std::fs::remove_file(&temp_fix_path);
                                        }}
                                    }}

                                    // Update output_size_bytes after replacement
                                    if let Ok(new_meta) = std::fs::metadata(&output_path) {{
                                        if let Ok(ref mut res) = result {{
                                            res.output_size_bytes = new_meta.len();
                                        }}
                                    }}

                                    // Re-probe to get corrected duration
                                    if let Ok(fixed_info) = crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, Path::new(&output_path)) {{
                                        log::info!("[METADATA_FIX] Fixed duration: {{:.1}}s (was {{:.1}}s, expected {{:.1}}s)",
                                            fixed_info.duration, info.duration, final_total_duration);
                                        actual_duration = fixed_info.duration;
                                        output_probe_result = Some(Ok(fixed_info));
                                    }}
                                }} else {{
                                    log::warn!("[METADATA_FIX] FFmpeg remux size mismatch ({{:.1}}%), keeping original", size_ratio);
                                    let _ = std::fs::remove_file(&temp_fix_path);
                                }}
                            }} else {{
                                log::warn!("[METADATA_FIX] FFmpeg remux output not found, keeping original");
                                let _ = std::fs::remove_file(&temp_fix_path);
                            }}
                        }}
                        Ok(cmd_output) => {{
                            log::warn!("[METADATA_FIX] FFmpeg genpts remux failed (exit {{}}): {{}}",
                                cmd_output.status.code().unwrap_or(-1),
                                String::from_utf8_lossy(&cmd_output.stderr).chars().take(200).collect::<String>());
                            let _ = std::fs::remove_file(&temp_fix_path);
                        }}
                        Err(e) => {{
                            log::warn!("[METADATA_FIX] Failed to spawn FFmpeg genpts remux: {{}}", e);
                        }}
                    }}
                    log::info!("[METADATA_FIX] ═══════════════════════════════════════════════════════════");
                }}
            }}
        }}"""

# Perform the replacement
new_content = content[:line_start] + replacement + content[end_of_section:]

# Write the result
with open(filepath, "w", encoding="utf-8") as f:
    f.write(new_content)

print("SUCCESS: Corrupted METADATA_FIX section replaced with clean FFmpeg genpts approach")
print(f"Replaced {end_of_section - line_start} characters with clean implementation")

# Also verify there's no duplicate METADATA_FIX header left
count = new_content.count(end_marker)
print(f"METADATA_FIX end markers remaining: {count}")
if count != 1:
    print("WARNING: Expected exactly 1 METADATA_FIX end marker!")
    exit(1)

print("OK: Clean METADATA_FIX section verified")
