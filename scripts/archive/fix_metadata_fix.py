#!/usr/bin/env python3
"""Replace the circular mkvmerge re-mux in METADATA_FIX with proper two-tier fix."""
import re

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Find the anchor line
anchor = 'log::info!("[METADATA_FIX] Running single-file mkvmerge re-mux to fix container headers...");'

if anchor not in content:
    print("ERROR: Anchor line not found")
    exit(1)

# Find the start and end of the old block
start = content.index(anchor)
block_start = content.rfind('\n', 0, start) + 1

# Find the closing METADATA_FIX log line after the main block
fallback_line = 'log::warn!("[METADATA_FIX] mkvmerge not available for re-mux, skipping metadata fix");'
fallback_pos = content.find(fallback_line, start)
if fallback_pos == -1:
    print("ERROR: Fallback line not found")
    exit(1)

closing_marker = 'log::info!("[METADATA_FIX]'
close_pos = content.find(closing_marker, fallback_pos)
if close_pos == -1:
    # Try the secondary path - should find closing line after the error handlers
    print("Trying secondary path for closing marker...")
    # Find next METADATA_FIX reference after fallback_pos
    for marker in ['log::info!("[METADATA_FIX]', 'log::warn!("[METADATA_FIX]']:
        p = content.find(marker, fallback_pos + 50)
        if p != -1:
            close_pos = p
            break

if close_pos == -1:
    print("ERROR: Closing METADATA_FIX line not found")
    exit(1)

# Extract old block
old_block = content[block_start:close_pos]
print(f"Old block length: {len(old_block)} chars")

# Count indentation from old block
first_line = old_block.split('\n')[0]
indent = first_line[:len(first_line) - len(first_line.lstrip())]
indent_str = indent  # e.g. 20 spaces

# Build new block WITHOUT using f-strings to avoid curly brace conflicts
L = []  # lines

L.append(indent_str + 'log::info!("[METADATA_FIX] Attempting to fix corrupted container duration metadata...");')
L.append('')
L.append(indent_str + '// Two-tier duration fix:')
L.append(indent_str + '//   Tier 1: mkvpropedit -- instant direct metadata edit of SegmentInfo.Duration')
L.append(indent_str + '//   Tier 2: FFmpeg genpts remux -- recalculates duration from frame timestamps')
L.append(indent_str + 'let original_size = result.as_ref().map(|r| r.output_size_bytes).unwrap_or(0);')
L.append(indent_str + 'let mut metadata_fixed = false;')
L.append('')
L.append(indent_str + '// -- Tier 1: mkvpropedit (instant, edits MKV SegmentInfo header in-place) --')
L.append(indent_str + 'if let Some(ref propedit_path) = crate::ffmpeg::mkvmerge::find_mkvpropedit() {')
L.append(indent_str + '    let duration_ns = (final_total_duration * 1_000_000_000.0) as u64;')
L.append(indent_str + '    log::info!("[METADATA_FIX] Running mkvpropedit to set SegmentInfo.Duration={}ns...", duration_ns);')
L.append('')
L.append(indent_str + '    let propedit_result = std::process::Command::new(propedit_path)')
L.append(indent_str + '        .arg("--edit")')
L.append(indent_str + '        .arg("info")')
L.append(indent_str + '        .arg("--set")')
L.append(indent_str + '        .arg(format!("duration={}", duration_ns))')
L.append(indent_str + '        .arg(&output_path)')
L.append(indent_str + '        .stdout(std::process::Stdio::null())')
L.append(indent_str + '        .stderr(std::process::Stdio::piped())')
L.append(indent_str + '        .output();')
L.append('')
L.append(indent_str + '    match propedit_result {')
L.append(indent_str + '        Ok(ref cmd_output) if cmd_output.status.success() => {')
L.append(indent_str + '            log::info!("[METADATA_FIX] mkvpropedit succeeded -- SegmentInfo.Duration set to {}ns", duration_ns);')
L.append(indent_str + '            metadata_fixed = true;')
L.append(indent_str + '        }')
L.append(indent_str + '        Ok(cmd_output) => {')
L.append(indent_str + '            log::warn!("[METADATA_FIX] mkvpropedit failed (exit {}): {}",')
L.append(indent_str + '                cmd_output.status.code().unwrap_or(-1),')
L.append(indent_str + '                String::from_utf8_lossy(&cmd_output.stderr).chars().take(200).collect::<String>());')
L.append(indent_str + '        }')
L.append(indent_str + '        Err(e) => {')
L.append(indent_str + '            log::warn!("[METADATA_FIX] Failed to spawn mkvpropedit: {}", e);')
L.append(indent_str + '        }')
L.append(indent_str + '    }')
L.append(indent_str + '} else {')
L.append(indent_str + '    log::info!("[METADATA_FIX] mkvpropedit not available, trying FFmpeg genpts...");')
L.append(indent_str + '}')
L.append('')
L.append(indent_str + '// -- Tier 2: FFmpeg genpts remux (recalculates duration from frame timestamps) --')
L.append(indent_str + 'if !metadata_fixed {')
L.append(indent_str + '    let temp_fix_path = format!("{}.metadata_fix{}", output_path,')
L.append(indent_str + '        std::path::Path::new(&output_path)')
L.append(indent_str + '            .extension()')
L.append(indent_str + '            .map(|e| format!(".{}", e.to_string_lossy()))')
L.append(indent_str + '            .unwrap_or_default());')
L.append('')
L.append(indent_str + '    log::info!("[METADATA_FIX] Running FFmpeg genpts remux to force PTS recalculation...");')
L.append('')
L.append(indent_str + '    let ffmpeg_fix = std::process::Command::new(&ffmpeg_path_resolved)')
L.append(indent_str + '        .arg("-fflags")')
L.append(indent_str + '        .arg("+genpts")')
L.append(indent_str + '        .arg("-i")')
L.append(indent_str + '        .arg(&output_path)')
L.append(indent_str + '        .arg("-c")')
L.append(indent_str + '        .arg("copy")')
L.append(indent_str + '        .arg("-avoid_negative_ts")')
L.append(indent_str + '        .arg("make_zero")')
L.append(indent_str + '        .arg("-y")')
L.append(indent_str + '        .arg(&temp_fix_path)')
L.append(indent_str + '        .stdout(std::process::Stdio::null())')
L.append(indent_str + '        .stderr(std::process::Stdio::piped())')
L.append(indent_str + '        .output();')
L.append('')
L.append(indent_str + '    match ffmpeg_fix {')
L.append(indent_str + '        Ok(ref cmd_output) if cmd_output.status.success() => {')
L.append(indent_str + '            if let Ok(temp_meta) = std::fs::metadata(&temp_fix_path) {')
L.append(indent_str + '                let temp_size = temp_meta.len();')
L.append(indent_str + '                let size_ratio = if original_size > 0 { (temp_size as f64) / (original_size as f64) * 100.0 } else { 100.0 };')
L.append('')
L.append(indent_str + '                log::info!("[METADATA_FIX] FFmpeg genpts remux complete: {} bytes -> {} bytes ({:.1}%)", original_size, temp_size, size_ratio);')
L.append('')
L.append(indent_str + '                if size_ratio >= 90.0 && size_ratio <= 110.0 {')
L.append(indent_str + '                    // Remux produced valid output -- replace original')
L.append(indent_str + '                    if let Err(e) = std::fs::rename(&temp_fix_path, &output_path) {')
L.append(indent_str + '                        let _ = std::fs::remove_file(&output_path);')
L.append(indent_str + '                        if let Err(e2) = std::fs::rename(&temp_fix_path, &output_path) {')
L.append(indent_str + '                            log::warn!("[METADATA_FIX] Failed to replace output after ffmpeg fix: {} / {}", e, e2);')
L.append(indent_str + '                            let _ = std::fs::remove_file(&temp_fix_path);')
L.append(indent_str + '                        }')
L.append(indent_str + '                    } else {')
L.append(indent_str + '                        metadata_fixed = true;')
L.append(indent_str + '                        if let Ok(new_meta) = std::fs::metadata(&output_path) {')
L.append(indent_str + '                            if let Ok(ref mut res) = result {')
L.append(indent_str + '                                res.output_size_bytes = new_meta.len();')
L.append(indent_str + '                            }')
L.append(indent_str + '                        }')
L.append(indent_str + '                    }')
L.append(indent_str + '                } else {')
L.append(indent_str + '                    log::warn!("[METADATA_FIX] FFmpeg remux size mismatch ({:.1}%), keeping original", size_ratio);')
L.append(indent_str + '                    let _ = std::fs::remove_file(&temp_fix_path);')
L.append(indent_str + '                }')
L.append(indent_str + '            } else {')
L.append(indent_str + '                log::warn!("[METADATA_FIX] FFmpeg remux output not found, keeping original");')
L.append(indent_str + '                let _ = std::fs::remove_file(&temp_fix_path);')
L.append(indent_str + '            }')
L.append(indent_str + '        }')
L.append(indent_str + '        Ok(cmd_output) => {')
L.append(indent_str + '            log::warn!("[METADATA_FIX] FFmpeg genpts remux failed (exit {}): {}",')
L.append(indent_str + '                cmd_output.status.code().unwrap_or(-1),')
L.append(indent_str + '                String::from_utf8_lossy(&cmd_output.stderr).chars().take(200).collect::<String>());')
L.append(indent_str + '            let _ = std::fs::remove_file(&temp_fix_path);')
L.append(indent_str + '        }')
L.append(indent_str + '        Err(e) => {')
L.append(indent_str + '            log::warn!("[METADATA_FIX] Failed to spawn FFmpeg genpts remux: {}", e);')
L.append(indent_str + '        }')
L.append(indent_str + '    }')
L.append(indent_str + '}')
L.append('')
L.append(indent_str + '// -- Re-probe after fix --')
L.append(indent_str + 'if metadata_fixed {')
L.append(indent_str + '    if let Ok(fixed_info) = crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, Path::new(&output_path)) {')
L.append(indent_str + '        log::info!("[METADATA_FIX] Fixed duration: {:.1}s (was {:.1}s, expected {:.1}s)",')
L.append(indent_str + '            fixed_info.duration, info.duration, final_total_duration);')
L.append(indent_str + '        actual_duration = fixed_info.duration;')
L.append(indent_str + '        output_probe_result = Some(Ok(fixed_info));')
L.append(indent_str + '    }')
L.append(indent_str + '} else {')
L.append(indent_str + '    log::warn!("[METADATA_FIX] All fix methods failed -- keeping original output with corrupted duration metadata");')
L.append(indent_str + '}')

new_block = '\n'.join(L)
print(f"New block length: {len(new_block)} chars")

# Replace
content = content[:block_start] + new_block + content[close_pos:]
print(f"New file length: {len(content)} chars")

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("SUCCESS: Replaced the METADATA_FIX block")
