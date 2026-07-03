"""
Fix compilation errors in merge.rs:
1. Replace invalid inline async closure with async block
2. Fix type mismatch in provenance logging (String vs &str)
"""

filepath = 'src-tauri/src/commands/merge.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

has_crlf = '\r\n' in content

# Fix 1: Replace the inline async closure with a proper async block
old_closure = '''                // Probe the repaired file
                let probe_result = (|| -> Result<f64, String> {
                    let path_buf = std::path::PathBuf::from(path);
                    let ffprobe = ffprobe_path_resolved.clone();
                    let info = tokio::task::spawn_blocking(move || {
                        crate::ffmpeg::probe::probe_file(&ffprobe, &path_buf)
                            .map_err(|e| format!("{:#}", e))
                    }).await.map_err(|e| format!("Reprobe task panicked: {}", e))?;
                    Ok(info.duration)
                })().await;'''

new_async = '''                // Probe the repaired file
                let probe_result = {
                    let path_buf = std::path::PathBuf::from(path);
                    let ffprobe = ffprobe_path_resolved.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::ffmpeg::probe::probe_file(&ffprobe, &path_buf)
                            .map_err(|e| format!("{:#}", e))
                    }).await.map_err(|e| format!("Reprobe task panicked: {}", e))
                        .and_then(|r| r.map(|info| info.duration))
                };'''

if old_closure in content:
    content = content.replace(old_closure, new_async, 1)
    print('Fix 1: Replaced invalid inline async closure with proper async block')
else:
    print('Fix 1 FAILED: Old closure pattern not found')
    # Debug: find the closure
    idx = content.find('let probe_result = (|| -> Result<f64, String>')
    if idx >= 0:
        print(f'  Found at index {idx}')
        print(f'  Context: {repr(content[idx:idx+300])}')
    else:
        # Try finding spawn_blocking near the reprobe section
        idx2 = content.find('Reprobe task panicked')
        if idx2 >= 0:
            start = max(0, idx2 - 200)
            print(f'  Found \"Reprobe task panicked\" at {idx2}')
            print(f'  Context: {repr(content[start:idx2+100])}')

# Fix 2: Fix type mismatch - "N/A" should be String in the else if branch
old_mismatch = '        } else if media_report.file_results.get(i).map(|r| r.is_quarantined()).unwrap_or(false) {\n            ("QUARANTINED", "N/A", "Failed", "FAIL")'
new_mismatch = '        } else if media_report.file_results.get(i).map(|r| r.is_quarantined()).unwrap_or(false) {\n            ("QUARANTINED", "N/A".to_string(), "Failed", "FAIL")'

if old_mismatch in content:
    content = content.replace(old_mismatch, new_mismatch, 1)
    print('Fix 2: Fixed type mismatch in provenance logging')
else:
    print('Fix 2 FAILED: Type mismatch pattern not found')
    # Find the provenance section
    idx = content.find('media_report.file_results.get(i).map(|r| r.is_quarantined())')
    if idx >= 0:
        print(f'  Found provenance section at {idx}')
        print(f'  Context: {repr(content[idx:idx+200])}')

# Fix 3: Also fix the else branch if it has the same issue
old_else = '        } else {\n            ("HEALTHY", "N/A", "Skipped", "N/A")\n        };'
new_else = '        } else {\n            ("HEALTHY", "N/A".to_string(), "Skipped", "N/A")\n        };'

if old_else in content:
    content = content.replace(old_else, new_else, 1)
    print('Fix 3: Fixed type mismatch in HEALTHY branch')
else:
    print('Fix 3: No change needed (HEALTHY branch already correct)')

# Write the file back
with open(filepath, 'w', encoding='utf-8', newline='\r\n' if has_crlf else '\n') as f:
    f.write(content)

print(f'\nWritten to {filepath}')
print('Done!')
