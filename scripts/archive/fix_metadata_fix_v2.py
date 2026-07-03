#!/usr/bin/env python3
"""Fix METADATA_FIX v2:
- Remove mkvpropedit tier (not bundled, wrong TimestampScale unit)
- Use only FFmpeg genpts (bundled, always correct)
- Fix ffmpeg_path_resolved capture by cloning before spawn_blocking
- Add back opening border log line
"""
import re

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Find the current METADATA_FIX block - look for "Attempting to fix" anchor
anchor = 'log::info!("[METADATA_FIX] Attempting to fix corrupted container duration metadata...");'

if anchor not in content:
    print("ERROR: New anchor line not found")
    # Try old anchor
    old_anchor = 'log::info!("[METADATA_FIX] Running single-file mkvmerge re-mux to fix container headers...");'
    if old_anchor in content:
        print("Old anchor still present - script needs re-run")
    exit(1)

# Find the complete METADATA_FIX block
block_start = content.rfind('\n', 0, content.index(anchor)) + 1

# Find the closing border line after the block
closing_marker = 'log::info!("[METADATA_FIX]'
close_search_start = content.index(anchor) + len(anchor)
close_pos = content.find(closing_marker, close_search_start)
if close_pos == -1:
    print("ERROR: Closing METADATA_FIX line not found")
    exit(1)

# Extract old block for indentation
old_block = content[block_start:close_pos]
print(f"Old block length: {len(old_block)} chars")

# Count indentation
first_line = old_block.split('\n')[0]
indent = first_line[:len(first_line) - len(first_line.lstrip())]
indent_str = indent

# Build new block - simpler, just FFmpeg genpts
L = []
L.append(indent_str + 'log::info!("[METADATA_FIX] Running FFmpeg genpts remux to fix corrupted container duration...");')
L.append('')
L.append(indent_str + '// FFmpeg genpts remux: forces PTS/timestamp regeneration, which recalculates')
L.append(indent_str + '// the MKV SegmentInfo.Duration from actual frame timestamps rather than')
L.append(indent_str + '// from the (corrupted) container header that mkvmerge wrote.')
L.append(indent_str + 'let original_size = result.as_ref().map(|r| r.output_size_bytes).unwrap_or(0);')
L.append('')
L.append(indent_str + 'let temp_fix_path = format!("{}.metadata_fix{}", output_path,')
L.append(indent_str + '    std::path::Path::new(&output_path)')
L.append(indent_str + '        .extension()')
L.append(indent_str + '        .map(|e| format!(".{}", e.to_string_lossy()))')
L.append(indent_str + '        .unwrap_or_default());')
L.append('')
L.append(indent_str + 'log::info!("[METADATA_FIX] Running FFmpeg genpts remux to force PTS recalculation...");')
L.append('')
L.append(indent_str + 'let ffmpeg_fix = std::process::Command::new(&ffmpeg_path_for_fix)')
L.append(indent_str + '    .arg("-fflags")')
L.append(indent_str + '    .arg("+genpts")')
L.append(indent_str + '    .arg("-i")')
L.append(indent_str + '    .arg(&output_path)')
L.append(indent_str + '    .arg("-c")')
L.append(indent_str + '    .arg("copy")')
L.append(indent_str + '    .arg("-avoid_negative_ts")')
L.append(indent_str + '    .arg("make_zero")')
L.append(indent_str + '    .arg("-y")')
L.append(indent_str + '    .arg(&temp_fix_path)')
L.append(indent_str + '    .stdout(std::process::Stdio::null())')
L.append(indent_str + '    .stderr(std::process::Stdio::piped())')
L.append(indent_str + '    .output();')
L.append('')
L.append(indent_str + 'match ffmpeg_fix {')
L.append(indent_str + '    Ok(ref cmd_output) if cmd_output.status.success() => {')
L.append(indent_str + '        if let Ok(temp_meta) = std::fs::metadata(&temp_fix_path) {')
L.append(indent_str + '            let temp_size = temp_meta.len();')
L.append(indent_str + '            let size_ratio = if original_size > 0 { (temp_size as f64) / (original_size as f64) * 100.0 } else { 100.0 };')
L.append('')
L.append(indent_str + '            log::info!("[METADATA_FIX] FFmpeg genpts remux complete: {} bytes -> {} bytes ({:.1}%)", original_size, temp_size, size_ratio);')
L.append('')
L.append(indent_str + '            if size_ratio >= 90.0 && size_ratio <= 110.0 {')
L.append(indent_str + '                // Remux produced valid output -- replace original')
L.append(indent_str + '                if let Err(e) = std::fs::rename(&temp_fix_path, &output_path) {')
L.append(indent_str + '                    let _ = std::fs::remove_file(&output_path);')
L.append(indent_str + '                    if let Err(e2) = std::fs::rename(&temp_fix_path, &output_path) {')
L.append(indent_str + '                        log::warn!("[METADATA_FIX] Failed to replace output after ffmpeg fix: {} / {}", e, e2);')
L.append(indent_str + '                        let _ = std::fs::remove_file(&temp_fix_path);')
L.append(indent_str + '                    }')
L.append(indent_str + '                } else {')
L.append(indent_str + '                    if let Ok(new_meta) = std::fs::metadata(&output_path) {')
L.append(indent_str + '                        if let Ok(ref mut res) = result {')
L.append(indent_str + '                            res.output_size_bytes = new_meta.len();')
L.append(indent_str + '                        }')
L.append(indent_str + '                    }')
L.append('')
L.append(indent_str + '                    // Re-probe to get corrected duration')
L.append(indent_str + '                    if let Ok(fixed_info) = crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, Path::new(&output_path)) {')
L.append(indent_str + '                        log::info!("[METADATA_FIX] Fixed duration: {:.1}s (was {:.1}s, expected {:.1}s)",')
L.append(indent_str + '                            fixed_info.duration, info.duration, final_total_duration);')
L.append(indent_str + '                        actual_duration = fixed_info.duration;')
L.append(indent_str + '                        output_probe_result = Some(Ok(fixed_info));')
L.append(indent_str + '                    }')
L.append(indent_str + '                }')
L.append(indent_str + '            } else {')
L.append(indent_str + '                log::warn!("[METADATA_FIX] FFmpeg remux size mismatch ({:.1}%), keeping original", size_ratio);')
L.append(indent_str + '                let _ = std::fs::remove_file(&temp_fix_path);')
L.append(indent_str + '            }')
L.append(indent_str + '        } else {')
L.append(indent_str + '            log::warn!("[METADATA_FIX] FFmpeg remux output not found, keeping original");')
L.append(indent_str + '            let _ = std::fs::remove_file(&temp_fix_path);')
L.append(indent_str + '        }')
L.append(indent_str + '    }')
L.append(indent_str + '    Ok(cmd_output) => {')
L.append(indent_str + '        log::warn!("[METADATA_FIX] FFmpeg genpts remux failed (exit {}): {}",')
L.append(indent_str + '            cmd_output.status.code().unwrap_or(-1),')
L.append(indent_str + '            String::from_utf8_lossy(&cmd_output.stderr).chars().take(200).collect::<String>());')
L.append(indent_str + '        let _ = std::fs::remove_file(&temp_fix_path);')
L.append(indent_str + '    }')
L.append(indent_str + '    Err(e) => {')
L.append(indent_str + '        log::warn!("[METADATA_FIX] Failed to spawn FFmpeg genpts remux: {}", e);')
L.append(indent_str + '    }')
L.append(indent_str + '}')

new_block = '\n'.join(L)
print(f"New block length: {len(new_block)} chars")

# Replace the METADATA_FIX block
content = content[:block_start] + new_block + content[close_pos:]

# Now fix the ffmpeg_path_resolved capture issue.
# Before the spawn_blocking, add a clone for use inside the closure.
# Find the spawn_blocking start: we need to add the clone line just before it.
# The spawn_blocking is at a line like: tokio::task::spawn_blocking(move || {
# Let me find a unique signature
spawn_anchor = 'tokio::task::spawn_blocking(move || {'
# We need a unique occurrence. Let me search for lines that have this pattern
# and are near the METADATA_FIX code (which is around line 4484 in original)

# Find all occurrences
all_spawns = [m.start() for m in re.finditer(re.escape(spawn_anchor), content)]
print(f"Found {len(all_spawns)} spawn_blocking occurrences")

for pos in all_spawns:
    context = content[pos:pos+200]
    if 'let _guard = cleanup_guard;' in context:
        # This is the post-merge spawn_blocking (line 4484)
        spawn_pos = pos
        # Find the line start
        line_start = content.rfind('\n', 0, spawn_pos) + 1
        # Insert ffmpeg_path_for_fix clone before this line
        clone_line = indent_str + 'let ffmpeg_path_for_fix = ffmpeg_path_resolved.clone();\n'
        content = content[:line_start] + clone_line + content[line_start:]
        print(f"Added ffmpeg_path_for_fix clone at position {line_start}")
        break
else:
    print("ERROR: Could not find the post-merge spawn_blocking")
    exit(1)

print(f"New file length: {len(content)} chars")

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("SUCCESS: Updated METADATA_FIX to simplified FFmpeg genpts approach")
print("        Added ffmpeg_path_for_fix clone before spawn_blocking")
