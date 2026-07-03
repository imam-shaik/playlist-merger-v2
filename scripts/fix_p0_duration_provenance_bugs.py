"""
Fix two runtime bugs in the media validation integration:
1. P0-3: Duration matching by path fails because repaired paths != original paths. Use file_index instead.
2. P0-4: Provenance table uses post-removal indices for file_results lookup - indices shift after quarantine removal.
Fix: Build file_index to result mapping before removal.
"""

filepath = 'src-tauri/src/commands/merge.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

has_crlf = '\r\n' in content

# Fix 1: Replace P0-3 path-matching code with index-based matching
old_p03 = '''    // P0-3: Re-probe repaired file durations
    // After repair, ffprobe the repaired file and update working_input_durations
    // so subtitle offsets and normalization use the correct duration.
    let mut duration_updates = 0usize;
    let mut validation_reprobe: Vec<(String, f64)> = Vec::new();
    for r in &media_report.file_results {
        if r.is_fixed() {
            if let Some(ref path) = r.repaired_path {
                // Probe the repaired file
                let probe_result = {
                    let path_buf = std::path::PathBuf::from(path);
                    let ffprobe = ffprobe_path_resolved.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::ffmpeg::probe::probe_file(&ffprobe, &path_buf)
                            .map_err(|e| format!("{:#}", e))
                    }).await.map_err(|e| format!("Reprobe task panicked: {}", e))
                        .and_then(|r| r.map(|info| info.duration))
                };

                match probe_result {
                    Ok(dur) if dur > 0.0 => {
                        // Update probe_cache with the new duration
                        let path_buf = std::path::PathBuf::from(path);
                        if let Some(Ok(info)) = probe_cache.get(&path_buf) {
                            let mut new_info = info.clone();
                            new_info.duration = dur;
                            probe_cache.insert(path_buf, Ok(new_info));
                        }
                        validation_reprobe.push((path.clone(), dur));
                        duration_updates += 1;
                    }
                    Ok(_) => {
                        log::warn!("[MEDIA_VALIDATION] P0-3: Repaired file {} has zero duration after repair", r.file_path);
                    }
                    Err(e) => {
                        log::warn!("[MEDIA_VALIDATION] P0-3: Failed to probe repaired file {}: {}", r.file_path, e);
                    }
                }
            }
        }
    }

    // Update working_input_durations for repaired files by matching path
    let mut repaired_paths = std::collections::HashMap::new();
    for (path, dur) in &validation_reprobe {
        repaired_paths.insert(path.clone(), *dur);
    }

    let mut updated_durations = Vec::new();
    for (i, file) in working_input_files.iter().enumerate() {
        if let Some(dur) = repaired_paths.get(file) {
            updated_durations.push(*dur);
        } else {
            updated_durations.push(working_input_durations[i]);
        }
    }
    working_input_durations = updated_durations;

    if duration_updates > 0 {
        log::info!("[MEDIA_VALIDATION] P0-3: Updated {} repaired file durations in working_input_durations", duration_updates);
    }'''

new_p03 = '''    // P0-3: Re-probe repaired file durations
    // After repair, ffprobe the repaired file and update working_input_durations
    // so subtitle offsets and normalization use the correct duration.
    // Uses file_index matching (repaired paths != original paths, so path matching would fail).
    let mut duration_updates = 0usize;
    for r in &media_report.file_results {
        if r.is_fixed() {
            if let Some(ref path) = r.repaired_path {
                // Probe the repaired file
                let probe_result = {
                    let path_buf = std::path::PathBuf::from(path);
                    let ffprobe = ffprobe_path_resolved.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::ffmpeg::probe::probe_file(&ffprobe, &path_buf)
                            .map_err(|e| format!("{:#}", e))
                    }).await.map_err(|e| format!("Reprobe task panicked: {}", e))
                        .and_then(|r| r.map(|info| info.duration))
                };

                match probe_result {
                    Ok(dur) if dur > 0.0 => {
                        // Update by file_index (reliable: original path -> original index)
                        if r.file_index < working_input_durations.len() {
                            working_input_durations[r.file_index] = dur;
                        }
                        // Also update probe_cache
                        let path_buf = std::path::PathBuf::from(path);
                        if let Some(Ok(info)) = probe_cache.get(&path_buf) {
                            let mut new_info = info.clone();
                            new_info.duration = dur;
                            probe_cache.insert(path_buf, Ok(new_info));
                        }
                        duration_updates += 1;
                    }
                    Ok(_) => {
                        log::warn!("[MEDIA_VALIDATION] P0-3: Repaired file {} has zero duration after repair", r.file_path);
                    }
                    Err(e) => {
                        log::warn!("[MEDIA_VALIDATION] P0-3: Failed to probe repaired file {}: {}", r.file_path, e);
                    }
                }
            }
        }
    }

    if duration_updates > 0 {
        log::info!("[MEDIA_VALIDATION] P0-3: Updated {} repaired file durations in working_input_durations (by file_index)", duration_updates);
    }'''

if old_p03 in content:
    content = content.replace(old_p03, new_p03, 1)
    print('Fix 1: P0-3 duration matching now uses file_index instead of path matching')
else:
    print('Fix 1 FAILED: P0-3 block not found')
    idx = content.find('P0-3: Re-probe repaired file durations')
    if idx >= 0:
        print(f'  Found at {idx}')
        print(f'  Context: {repr(content[idx:idx+100])}')

# Fix 2: Fix provenance index mapping - build a file_index -> result map BEFORE removal
old_prov = '''    // P0-4: Merge input provenance logging
    let fixed_paths: std::collections::HashSet<String> = media_report.file_results.iter()
        .filter(|r| r.is_fixed())
        .filter_map(|r| r.repaired_path.clone())
        .collect();

    let repair_methods: std::collections::HashMap<String, String> = media_report.file_results.iter()
        .filter(|r| r.is_fixed())
        .filter_map(|r| {
            r.repaired_path.as_ref().map(|p| {
                (p.clone(), format!("{:?}", r.fix_applied.as_ref().unwrap_or(&crate::ffmpeg::media_validation_engine::types::enums::FixType::None)))
            })
        })
        .collect();

    log::info!("[MEDIA_VALIDATION] P0-4: MERGE INPUT PROVENANCE");
    log::info!("[MEDIA_VALIDATION] {:<6} {:<50} {:<50} {:<10} {:<15} {:<10} {:<8} {:<10} {:<50}",
        "Index", "Original Path", "Merge Path", "Status", "Repair Type", "Repair Stat", "Reval", "Duration", "Final Merge Path");

    for (i, file) in working_input_files.iter().enumerate() {
        let original_path = &media_report.file_results.get(i)
            .map(|r| &r.file_path)
            .cloned()
            .unwrap_or_else(|| file.clone());'''

new_prov = '''    // P0-4: Merge input provenance logging
    // Build index-to-result map BEFORE quarantine removal so indices are correct
    let fixed_paths: std::collections::HashSet<String> = media_report.file_results.iter()
        .filter(|r| r.is_fixed())
        .filter_map(|r| r.repaired_path.clone())
        .collect();

    let repair_methods: std::collections::HashMap<String, String> = media_report.file_results.iter()
        .filter(|r| r.is_fixed())
        .filter_map(|r| {
            r.repaired_path.as_ref().map(|p| {
                (p.clone(), format!("{:?}", r.fix_applied.as_ref().unwrap_or(&crate::ffmpeg::media_validation_engine::types::enums::FixType::None)))
            })
        })
        .collect();

    // Build file_index -> result lookup (files may have been reordered after removal)
    let mut result_by_index: std::collections::HashMap<usize, &crate::ffmpeg::media_validation_engine::MediaValidationResult> = std::collections::HashMap::new();
    for r in &media_report.file_results {
        result_by_index.insert(r.file_index, r);
    }

    log::info!("[MEDIA_VALIDATION] P0-4: MERGE INPUT PROVENANCE");
    log::info!("[MEDIA_VALIDATION] {:<6} {:<50} {:<50} {:<10} {:<15} {:<10} {:<8} {:<10} {:<50}",
        "Index", "Original Path", "Merge Path", "Status", "Repair Type", "Repair Stat", "Reval", "Duration", "Final Merge Path");

    for (i, file) in working_input_files.iter().enumerate() {
        let r = result_by_index.get(&i);
        let original_path = r.map(|r| r.file_path.clone()).unwrap_or_else(|| file.clone());'''

if old_prov in content:
    content = content.replace(old_prov, new_prov, 1)
    print('Fix 2: Provenance now uses file_index map for correct lookup after quarantine removal')
else:
    print('Fix 2 FAILED: Provenance block not found')
    idx = content.find('P0-4: MERGE INPUT PROVENANCE')
    if idx >= 0:
        print(f'  Found at {idx}')
        print(f'  Context: {repr(content[idx:idx+100])}')

# Fix 3: Also update the QUARANTINED and HEALTHY checks in provenance to use result_by_index
old_check = '''        let (status_label, repair_type, repair_status, reval_label) = if fixed_paths.contains(file) {
            let rtype = repair_methods.get(file).cloned().unwrap_or_else(|| "N/A".to_string());
            ("REPAIRED", rtype, "Succeeded", "PASS")
        } else if media_report.file_results.get(i).map(|r| r.is_quarantined()).unwrap_or(false) {
            ("QUARANTINED", "N/A".to_string(), "Failed", "FAIL")
        } else {
            ("HEALTHY", "N/A".to_string(), "Skipped", "N/A")
        };

        let merge_path = if media_report.file_results.get(i).map(|r| r.is_quarantined()).unwrap_or(false) {
            "X REMOVED"
        } else {
            file.as_str()
        };'''

new_check = '''        let r_entry = result_by_index.get(&i);
        let (status_label, repair_type, repair_status, reval_label) = if fixed_paths.contains(file) {
            let rtype = repair_methods.get(file).cloned().unwrap_or_else(|| "N/A".to_string());
            ("REPAIRED", rtype, "Succeeded", "PASS")
        } else if r_entry.map(|r| r.is_quarantined()).unwrap_or(false) {
            ("QUARANTINED", "N/A".to_string(), "Failed", "FAIL")
        } else {
            ("HEALTHY", "N/A".to_string(), "Skipped", "N/A")
        };

        let merge_path = if r_entry.map(|r| r.is_quarantined()).unwrap_or(false) {
            "X REMOVED"
        } else {
            file.as_str()
        };'''

if old_check in content:
    content = content.replace(old_check, new_check, 1)
    print('Fix 3: Provenance status/merge_path checks use result_by_index map')
else:
    print('Fix 3 FAILED: Status check block not found')
    idx = content.find('(status_label, repair_type, repair_status, reval_label)')
    if idx >= 0:
        print(f'  Found at {idx}')
        print(f'  Context: {repr(content[idx:idx+300])}')

# Write the file back
with open(filepath, 'w', encoding='utf-8', newline='\r\n' if has_crlf else '\n') as f:
    f.write(content)

print(f'\nWritten to {filepath}')
print('Done!')
