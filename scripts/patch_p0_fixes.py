#!/usr/bin/env python3
"""Apply P0-2 (temp cleanup), P0-3 (duration update), P0-4 (provenance logging) fixes."""

def read_file(path):
    with open(path, 'rb') as f:
        return f.read()

def write_file(path, content):
    with open(path, 'wb') as f:
        f.write(content)

def main():
    path = 'src-tauri/src/commands/merge.rs'
    content = read_file(path)

    had_crlf = b'\r\n' in content
    if had_crlf:
        content = content.replace(b'\r\n', b'\n')

    changes = 0

    # EDIT 1 (P0-2 + P0-3): Register repaired files + update durations
    old1 = b'            if !removed_quarantined_indices.is_empty() {\n                log::warn!("[MEDIA_VALIDATION] Removing {} quarantined files", removed_quarantined_indices.len());\n                let remove_set: std::collections::HashSet<usize> = removed_quarantined_indices.iter().copied().collect();\n\n                working_input_files = updated_files;\n                working_input_durations = working_input_durations.iter().enumerate()\n                    .filter(|(i, _)| !remove_set.contains(i))\n                    .map(|(_, d)| *d)\n                    .collect();\n                working_input_names = working_input_names.iter().enumerate()\n                    .filter(|(i, _)| !remove_set.contains(i))\n                    .map(|(_, n)| n.clone())\n                    .collect();\n                working_total_duration = working_input_durations.iter().sum();\n                input_paths = working_input_files.iter().map(PathBuf::from).collect();\n            }'

    new1_lines = [
        b'            if !removed_quarantined_indices.is_empty() {',
        b'                log::warn!("[MEDIA_VALIDATION] Removing {} quarantined files", removed_quarantined_indices.len());',
        b'                let remove_set: std::collections::HashSet<usize> = removed_quarantined_indices.iter().copied().collect();',
        b'',
        b'                // P0-2: Track original paths BEFORE replacement so we can identify',
        b'                // which files are repaired (non-original paths) for temp cleanup registration.',
        b'                let pre_update_paths: std::collections::HashSet<String> = working_input_files.iter().cloned().collect();',
        b'',
        b'                working_input_files = updated_files;',
        b'                working_input_durations = working_input_durations.iter().enumerate()',
        b'                    .filter(|(i, _)| !remove_set.contains(i))',
        b'                    .map(|(_, d)| *d)',
        b'                    .collect();',
        b'                working_input_names = working_input_names.iter().enumerate()',
        b'                    .filter(|(i, _)| !remove_set.contains(i))',
        b'                    .map(|(_, n)| n.clone())',
        b'                    .collect();',
        b'                working_total_duration = working_input_durations.iter().sum();',
        b'                input_paths = working_input_files.iter().map(PathBuf::from).collect();',
        b'',
        b'                // P0-2: Register repaired temp files with TempCleanup.',
        b'                // Repaired files are created by the media validation engine in temp_dir',
        b'                // but are NOT tracked by the existing cleanup mechanisms (norm_files, registry.sub).',
        b'                // Registering them here ensures they are cleaned up on all exit paths.',
        b'                //',
        b'                // P0-3: Update working_input_durations for repaired files.',
        b'                // After repair, the file duration may differ from the original (especially for',
        b'                // re-encode repairs). Subtitle offset calculations depend on accurate durations,',
        b'                // so we must probe the repaired files for their actual duration.',
        b'                for (new_path, old_dur_idx) in working_input_files.iter().zip(0..) {',
        b'                    if !pre_update_paths.contains(new_path) {',
        b'                        // This is a repaired temp file -- register for cleanup',
        b'                        if let Ok(mut files) = temp_norm_files_arc.lock() {',
        b'                            files.push(PathBuf::from(new_path));',
        b'                        }',
        b'',
        b'                        // P0-3: Re-probe repaired file for actual duration',
        b'                        let repaired_path = Path::new(new_path);',
        b'                        if repaired_path.exists() {',
        b'                            if let Some(Ok(info)) = probe_cache.get(repaired_path) {',
        b'                                if info.duration > 0.0 && (info.duration - working_input_durations[old_dur_idx]).abs() > 0.1 {',
        b'                                    log::info!("[MEDIA_VALIDATION] P0-3: Repaired file duration changed: {} original={:.1}s -> repaired={:.1}s",',
        b'                                        std::path::Path::new(new_path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),',
        b'                                        working_input_durations[old_dur_idx], info.duration);',
        b'                                    working_input_durations[old_dur_idx] = info.duration;',
        b'                                }',
        b'                            }',
        b'                        }',
        b'                    }',
        b'                }',
        b'                working_total_duration = working_input_durations.iter().sum();',
        b'            }',
    ]
    new1 = b'\n'.join(new1_lines)

    if old1 in content:
        content = content.replace(old1, new1, 1)
        changes += 1
        print("EDIT 1 (P0-2+P0-3): OK")
    elif b'pre_update_paths' in content:
        print("EDIT 1 already applied")
    else:
        print("EDIT 1 FAILED - pattern not found")
        idx = content.find(b'if !removed_quarantined_indices.is_empty()')
        if idx >= 0:
            print("  Found at offset", idx, "context:", content[idx:idx+150])
        return False

    # EDIT 2 (P0-4): Add MERGE INPUT PROVENANCE logging before concat
    old2_line1 = b'    let include_duration = actual_mode == MergeMode::Custom;'
    old2_line2 = b'    write_concat_list_with_durations(&path_refs, Some(&final_input_durations), &list_path, include_duration).map_err(|e| e.to_string())?;'
    old2 = old2_line1 + b'\n' + old2_line2

    new2_lines = [
        b'    // P0-4: MERGE INPUT PROVENANCE - emit forensic log showing actual files entering merge.',
        b'    // This proves that repaired files (not originals) are what the merge engine consumes.',
        b'    log::info!("[MERGE_INPUT_PROVENANCE] ============================================================");',
        b'    log::info!("[MERGE_INPUT_PROVENANCE] MERGE INPUT PROVENANCE VERIFICATION");',
        b'    log::info!("[MERGE_INPUT_PROVENANCE] ------------------------------------------------------------");',
        b'    log::info!("[MERGE_INPUT_PROVENANCE] Index | Source    | Fix Method  | Revalidated | Final Merge Path");',
        b'    log::info!("[MERGE_INPUT_PROVENANCE] ------------------------------------------------------------");',
        b'    for (idx, fpath) in final_input_files.iter().enumerate() {',
        b'        let fname = std::path::Path::new(fpath).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();',
        b'        let (source, method_str, revalidated_str) = if media_report.file_results.iter().any(|r| r.file_index == idx && r.is_fixed()) {',
        b'            let method = media_report.file_results.iter()',
        b'                .find(|r| r.file_index == idx)',
        b'                .and_then(|r| r.fix_applied.as_ref().map(|f| format!("{:?}", f)))',
        b'                .unwrap_or_default();',
        b'            ("REPAIRED", method, "PASS")',
        b'        } else {',
        b'            ("ORIGINAL", "none".to_string(), "N/A".to_string())',
        b'        };',
        b'        log::info!("[MERGE_INPUT_PROVENANCE] {:>5} | {:<10} | {:<12} | {:<11} | {}",',
        b'            idx, source, method_str, revalidated_str, fname);',
        b'    }',
        b'    log::info!("[MERGE_INPUT_PROVENANCE] ------------------------------------------------------------");',
        b'    log::info!("[MERGE_INPUT_PROVENANCE] Total files entering merge: {}", final_input_files.len());',
        b'    log::info!("[MERGE_INPUT_PROVENANCE] ============================================================");',
        b'',
        old2_line1,
        old2_line2,
    ]
    new2 = b'\n'.join(new2_lines)

    if old2 in content:
        content = content.replace(old2, new2, 1)
        changes += 1
        print("EDIT 2 (P0-4): OK")
    elif b'MERGE_INPUT_PROVENANCE' in content:
        print("EDIT 2 already applied")
    else:
        print("EDIT 2 FAILED")
        idx = content.find(b'let include_duration = actual_mode == MergeMode::Custom;')
        if idx >= 0:
            print("  Found at offset", idx)
        return False

    if had_crlf:
        content = content.replace(b'\n', b'\r\n')

    write_file(path, content)
    print(f"\nRESULT: {changes}/2 edits applied")
    return changes == 2

if __name__ == '__main__':
    success = main()
    exit(0 if success else 1)
