#!/usr/bin/env python3
"""Patch merge.rs to add P1 health-aware dominant profile selection."""

def read_file(path):
    with open(path, 'rb') as f:
        return f.read()

def write_file(path, content):
    with open(path, 'wb') as f:
        f.write(content)

def main():
    path = 'src-tauri/src/commands/merge.rs'
    content = read_file(path)

    # Normalize to LF for editing
    had_crlf = b'\r\n' in content
    if had_crlf:
        content = content.replace(b'\r\n', b'\n')

    changes = 0

    # EDIT 1: _corruption_results -> corruption_results
    old1 = b'let (_corruption_results, corrupt_count, _) = check_batch_corruption_parallel('
    new1 = b'let (corruption_results, corrupt_count, _) = check_batch_corruption_parallel('
    if old1 in content:
        content = content.replace(old1, new1, 1)
        changes += 1
        print("EDIT 1 OK")
    elif new1 in content:
        print("EDIT 1 already applied")
    else:
        print("EDIT 1 FAILED")
        return False

    # EDIT 2: Add unhealthy_file_paths after corrupt_count check
    old2 = (
        b'    if corrupt_count > 0 {\n'
        b'        return Err(format!("{} file(s) failed basic health checks'
        b' and cannot be merged. Run compatibility check for details.", corrupt_count));\n'
        b'    }'
    )
    if b'unhealthy_file_paths' in old2:
        print("Sanity check failed")
        return False

    new2_lines = [
        b'    // --- P1: Health-aware dominant profile selection ---------------------------',
        b'    // Build a set of unhealthy file paths BEFORE media validation may remove them.',
        b'    // This ensures that even if corrupt_count is 0 (no hard failures), files with',
        b'    // minor metadata issues or ffprobe warnings are excluded from dominant profile',
        b'    // selection -- preventing normalization toward a damaged reference.',
        b'    let unhealthy_file_paths: std::collections::HashSet<String> = corruption_results.iter()',
        b'        .filter(|h| !h.status.is_healthy())',
        b'        .map(|h| h.path.clone())',
        b'        .collect();',
        b'    if !unhealthy_file_paths.is_empty() {',
        b'        log::warn!("[DOMINANT_PROFILE] P1: Excluding {} unhealthy files from profile analysis:", unhealthy_file_paths.len());',
        b'        for h in corruption_results.iter().filter(|h| !h.status.is_healthy()) {',
        b'            log::warn!("[DOMINANT_PROFILE]   [{}] {} -> {:?}", h.index,',
        b'                std::path::Path::new(&h.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| h.path.clone()),',
        b'                h.status);',
        b'        }',
        b'    }',
        b'',
        b'    if corrupt_count > 0 {',
        b'        return Err(format!("{} file(s) failed basic health checks and cannot be merged. Run compatibility check for details.", corrupt_count));',
        b'    }',
    ]
    new2 = b'\n'.join(new2_lines)

    if old2 in content:
        content = content.replace(old2, new2, 1)
        changes += 1
        print("EDIT 2 OK")
    elif b'unhealthy_file_paths' in content and b'P1: Health-aware' in content:
        print("EDIT 2 already applied")
    else:
        # Debug: find the old2 pattern
        idx = content.find(b'if corrupt_count > 0 {')
        if idx >= 0:
            snippet = content[max(0,idx-50):idx+250]
            print("EDIT 2 FAILED - pattern mismatch. Found 'if corrupt_count' at offset", idx)
            print("Context:", repr(snippet[:200]))
        else:
            print("EDIT 2 FAILED - 'if corrupt_count' not found at all")
        return False

    # EDIT 3: Add filtering before analyze_profiles()
    old3 = (
        b'    // Always analyze profiles to detect outliers first\n'
        b'    let analysis_start = std::time::Instant::now();\n'
        b'    let mut profile_infos = Vec::new();\n'
        b'    for (i, file) in working_input_files.iter().enumerate() { if let Some(Ok(info)) = probe_cache.get(Path::new(file)) { profile_infos.push((i, file.clone(), info.clone())); } }\n'
        b'    let mut analysis = analyze_profiles(&profile_infos);'
    )

    new3_lines = [
        b'    // Always analyze profiles to detect outliers first',
        b'    let analysis_start = std::time::Instant::now();',
        b'    let mut profile_infos = Vec::new();',
        b'    for (i, file) in working_input_files.iter().enumerate() { if let Some(Ok(info)) = probe_cache.get(Path::new(file)) { profile_infos.push((i, file.clone(), info.clone())); } }',
        b'',
        b'    // --- P1: Health-aware dominant profile selection ---------------------------',
        b'    // Filter out unhealthy files from profile analysis so the dominant profile',
        b'    // is derived ONLY from healthy files. This prevents normalizing toward a',
        b'    // damaged reference (e.g., 116 corrupt files with bad metadata dominating',
        b'    // over 18 healthy files with correct metadata).',
        b'    let pre_filter_count = profile_infos.len();',
        b'    if !unhealthy_file_paths.is_empty() {',
        b'        profile_infos.retain(|(_, path, _)| !unhealthy_file_paths.contains(path));',
        b'        let excluded = pre_filter_count - profile_infos.len();',
        b'        if excluded > 0 {',
        b'            log::info!("[DOMINANT_PROFILE] P1: Filtered {} unhealthy files from profile analysis ({} -> {})",',
        b'                excluded, pre_filter_count, profile_infos.len());',
        b'        }',
        b'    }',
        b'    if profile_infos.is_empty() {',
        b'        return Err("All input files failed health checks -- no healthy files remain for profile analysis.".to_string());',
        b'    }',
        b'',
        b'    let mut analysis = analyze_profiles(&profile_infos);',
        b'    log::info!("[DOMINANT_PROFILE] Health-aware analysis: {} healthy files analyzed, dominant match_count={}, total_count={}",',
        b'        profile_infos.len(), analysis.dominant.match_count, analysis.dominant.total_count);',
    ]
    new3 = b'\n'.join(new3_lines)

    if old3 in content:
        content = content.replace(old3, new3, 1)
        changes += 1
        print("EDIT 3 OK")
    elif b'pre_filter_count' in content and b'P1: Health-aware' in content:
        print("EDIT 3 already applied")
    else:
        idx = content.find(b'Always analyze profiles to detect outliers first')
        if idx >= 0:
            snippet = content[idx:idx+400]
            print("EDIT 3 FAILED - pattern mismatch. Found at offset", idx)
            print("Context:", repr(snippet[:300]))
        else:
            print("EDIT 3 FAILED - 'Always analyze profiles' not found")
        return False

    # Restore line endings
    if had_crlf:
        content = content.replace(b'\n', b'\r\n')

    write_file(path, content)
    print(f"\nRESULT: {changes}/3 edits applied successfully")
    return changes == 3


if __name__ == '__main__':
    success = main()
    exit(0 if success else 1)
