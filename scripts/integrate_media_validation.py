"""
Integrate the media validation engine into merge.rs pipeline.
Inserts the validation call after normalization cache refresh, before cards insertion.
Implements P0-2 (repaired temp file cleanup), P0-3 (duration re-probing), P0-4 (provenance logging).
"""

import os

filepath = 'src-tauri/src/commands/merge.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

has_crlf = '\r\n' in content

# Fix 1: Add imports for media_validation_engine at the top
import_marker = 'use crate::ffmpeg::normalization::{normalize_to_profile as ffmpeg_normalize_to_profile, normalize_audio_only as ffmpeg_normalize_audio_only, EncodingProfile, AudioProfile};'
new_import = 'use crate::ffmpeg::normalization::{normalize_to_profile as ffmpeg_normalize_to_profile, normalize_audio_only as ffmpeg_normalize_audio_only, EncodingProfile, AudioProfile};\nuse crate::ffmpeg::media_validation_engine::{validate_input_files, apply_validation_results};'

if import_marker in content:
    content = content.replace(import_marker, new_import, 1)
    print('Fix 1: Added media_validation_engine imports')
else:
    print('Fix 1 FAILED: Import marker not found')

# Fix 2: Insert media validation block before cards insertion
# Insert BEFORE: "    let working_durations = working_input_durations.clone();\n    let mut card_temp_files = Vec::new();"

insert_marker = '    let working_durations = working_input_durations.clone();\n    let mut card_temp_files = Vec::new();'

validation_block = '''    // MEDIA VALIDATION ENGINE --- analyze and repair damaged files
    // Runs after normalization so it validates the files that will merge.
    // Quarantined files are removed from the pipeline.
    log::info!("[MEDIA_VALIDATION] Starting media validation on {} files", working_input_files.len());

    let quarantine_dir = temp_dir.join("quarantine");
    let _ = std::fs::create_dir_all(&quarantine_dir);

    let media_report = validate_input_files(
        &ffprobe_path_resolved,
        &ffmpeg_path_resolved,
        &working_input_files,
        &quarantine_dir,
        Some(cancel_flag.clone()),
    );

    log::info!("[MEDIA_VALIDATION] Report: {}", media_report.summary());

    // P0-2: Register repaired temp files with TempCleanup
    // Every repaired file registers with temp_norm_files_arc so cleanup_guard
    // removes them on success, failure, cancel, or panic.
    let mut repair_count = 0usize;
    for r in &media_report.file_results {
        if r.is_fixed() {
            if let Some(ref path) = r.repaired_path {
                if let Ok(mut nf) = temp_norm_files_arc.lock() {
                    nf.push(std::path::PathBuf::from(path));
                    repair_count += 1;
                }
            }
        }
    }
    if repair_count > 0 {
        log::info!("[MEDIA_VALIDATION] P0-2: Registered {} repaired files with TempCleanup", repair_count);
    }

    // P0-3: Re-probe repaired file durations
    // After repair, ffprobe the repaired file and update working_input_durations
    // so subtitle offsets and normalization use the correct duration.
    let mut duration_updates = 0usize;
    let mut validation_reprobe: Vec<(String, f64)> = Vec::new();
    for r in &media_report.file_results {
        if r.is_fixed() {
            if let Some(ref path) = r.repaired_path {
                // Probe the repaired file
                let probe_result = (|| -> Result<f64, String> {
                    let path_buf = std::path::PathBuf::from(path);
                    let ffprobe = ffprobe_path_resolved.clone();
                    let info = tokio::task::spawn_blocking(move || {
                        crate::ffmpeg::probe::probe_file(&ffprobe, &path_buf)
                            .map_err(|e| format!("{:#}", e))
                    }).await.map_err(|e| format!("Reprobe task panicked: {}", e))?;
                    Ok(info.duration)
                })().await;

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
    }

    // Apply validation results: update working files
    let (updated_files, removed_indices) = apply_validation_results(&working_input_files, &media_report);

    if !removed_indices.is_empty() {
        log::warn!("[MEDIA_VALIDATION] {} files quarantined (removed from pipeline)", removed_indices.len());
        // Rebuild working arrays excluding quarantined files
        let remove_set: std::collections::HashSet<usize> = removed_indices.into_iter().collect();
        let mut new_files = Vec::new();
        let mut new_durations = Vec::new();
        let mut new_names = Vec::new();
        for (i, file) in working_input_files.iter().enumerate() {
            if !remove_set.contains(&i) {
                new_files.push(file.clone());
                new_durations.push(working_input_durations[i]);
                new_names.push(working_input_names[i].clone());
            }
        }
        working_input_files = new_files;
        working_input_durations = new_durations;
        working_input_names = new_names;
        working_total_duration = working_input_durations.iter().sum();
    }

    // Apply path updates (repaired files may have new paths)
    let mut reparsed_count = 0usize;
    for (i, file) in working_input_files.iter_mut().enumerate() {
        if i < updated_files.len() && updated_files[i] != *file {
            *file = updated_files[i].clone();
            reparsed_count += 1;
        }
    }
    if reparsed_count > 0 {
        log::info!("[MEDIA_VALIDATION] Updated {} file paths from repaired paths", reparsed_count);
    }

    // P0-4: Merge input provenance logging
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
            .unwrap_or_else(|| file.clone());
        let original_name = std::path::Path::new(original_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| original_path.clone());

        let merge_name = std::path::Path::new(file)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| file.clone());

        let (status_label, repair_type, repair_status, reval_label) = if fixed_paths.contains(file) {
            let rtype = repair_methods.get(file).cloned().unwrap_or_else(|| "N/A".to_string());
            ("REPAIRED", rtype, "Succeeded", "PASS")
        } else if media_report.file_results.get(i).map(|r| r.is_quarantined()).unwrap_or(false) {
            ("QUARANTINED", "N/A", "Failed", "FAIL")
        } else {
            ("HEALTHY", "N/A", "Skipped", "N/A")
        };

        let merge_path = if media_report.file_results.get(i).map(|r| r.is_quarantined()).unwrap_or(false) {
            "X REMOVED"
        } else {
            file.as_str()
        };

        let dur = working_input_durations.get(i).copied().unwrap_or(0.0);

        log::info!("[MEDIA_VALIDATION] {:<6} {:<50} {:<50} {:<10} {:<15} {:<10} {:<8} {:<10.1} {:<50}",
            i,
            if original_name.len() > 48 { format!("{}...", &original_name[..45]) } else { original_name },
            if merge_name.len() > 48 { format!("{}...", &merge_name[..45]) } else { merge_name },
            status_label,
            repair_type,
            repair_status,
            reval_label,
            dur,
            merge_path
        );
    }

    log::info!("[MEDIA_VALIDATION] Summary: {} files -> {} after validation ({} repaired, {} quarantined)",
        media_report.total_files, working_input_files.len(), media_report.fixed_count, media_report.quarantined_count);

    let working_durations = working_input_durations.clone();
    let mut card_temp_files = Vec::new();
'''

if insert_marker in content:
    content = content.replace(insert_marker, validation_block, 1)
    print('Fix 2: Inserted media validation block before cards insertion')
else:
    print('Fix 2 FAILED: Insert marker not found')
    # Debug
    idx = content.find('let working_durations = working_input_durations.clone()')
    if idx >= 0:
        print(f'  Found marker at index {idx}')
        print(f'  Context: {repr(content[idx-20:idx+100])}')

# Write the file back
with open(filepath, 'w', encoding='utf-8', newline='\r\n' if has_crlf else '\n') as f:
    f.write(content)

print(f'\nWritten to {filepath}')
print('Done!')
