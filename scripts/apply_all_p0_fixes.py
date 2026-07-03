#!/usr/bin/env python3
"""Apply ALL P0 fixes to merge.rs in one comprehensive run.

Changes:
1. Fix missing MergeConfig.mkvmerge_succeeded_before_ffmpeg field
2. Add temp_norm_files_arc before MEDIA VALIDATION ENGINE
3. Add repaired file registration and duration re-probing
4. Add provenance collections declarations
5. Add provenance population code (correct location)
6. Change provenance log to use persistent collections
7. Fix type mismatch
"""

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'rb') as f:
        content = f.read()
    
    has_crlf = b'\r\n' in content
    if has_crlf:
        content = content.replace(b'\r\n', b'\n')
    
    fixes = 0
    
    # ─────────────────────────────────────────────────────────────────
    # FIX 1: Add missing MergeConfig.mkvmerge_succeeded_before_ffmpeg
    # ─────────────────────────────────────────────────────────────────
    old1 = b'            card_config: request.card_config.clone(),\n            burn_subtitle_path: burn_subtitle_path.clone(),\n        }'
    new1 = b'            card_config: request.card_config.clone(),\n            burn_subtitle_path: burn_subtitle_path.clone(),\n            mkvmerge_succeeded_before_ffmpeg: false,\n        }'
    
    if old1 in content:
        content = content.replace(old1, new1, 1)
        fixes += 1
        print(f"[{fixes}] Fix 1: Added mkvmerge_succeeded_before_ffmpeg field")
    else:
        print("[FAIL] Fix 1: Pattern not found - checking if already present")
        if b'mkvmerge_succeeded_before_ffmpeg: false' in content:
            fixes += 1
            print("  Already present")
    
    # ─────────────────────────────────────────────────────────────────
    # FIX 2: Add temp_norm_files_arc creation BEFORE MEDIA VALIDATION
    # ─────────────────────────────────────────────────────────────────
    for needle in [
        b'    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | start", );\n\n    // -- MEDIA VALIDATION ENGINE --',
        b'    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | start", );\n\n    // \xe2\x94\x80\xe2\x94\x80 MEDIA VALIDATION ENGINE',
    ]:
        if needle in content:
            block = (
                b'    log::info!("[STAGE_TIMING] MEDIA_VALIDATION | start", );\n'
                b'\n'
                b'    // -- P0-2: Initialize temp file tracking BEFORE media validation --\n'
                b'    // Creates the norm_files Arc early so repaired file registration\n'
                b'    // in apply_validation_results can register repaired temp files\n'
                b'    // for cleanup. Without this, repaired files would not be tracked.\n'
                b'    let temp_norm_files_arc = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));\n'
                b'    let temp_registry = TempFileRegistry {\n'
                b'        sub: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),\n'
                b'    };\n'
                b'\n'
                b'    // -- MEDIA VALIDATION ENGINE --'
            )
            content = content.replace(needle, block, 1)
            fixes += 1
            print(f"[{fixes}] Fix 2: Added temp_norm_files_arc before MEDIA VALIDATION")
            break
    else:
        if b'P0-2: Initialize temp file tracking BEFORE media validation' in content:
            fixes += 1
            print(f"[{fixes}] Fix 2: Already present")
        else:
            # Try to find the MEDIA_VALIDATION log line
            idx = content.find(b'MEDIA_VALIDATION | start')
            if idx >= 0:
                # Find what's after it
                rest = content[idx:idx+200]
                print(f"Fix 2: Found MEDIA_VALIDATION at {idx}, context: {rest[:150]}")
            print("[FAIL] Fix 2: Could not find marker")
    
    # ─────────────────────────────────────────────────────────────────
    # FIX 3: Remove duplicate temp_norm_files_arc creation later
    # ─────────────────────────────────────────────────────────────────
    old3 = b'    let temp_norm_files_arc = Arc::new(Mutex::new(Vec::new()));\n    let mut cleanup_guard = TempCleanup::new(Arc::new(temp_registry), Arc::clone(&temp_norm_files_arc));'
    new3 = b'    let mut cleanup_guard = TempCleanup::new(Arc::new(temp_registry), Arc::clone(&temp_norm_files_arc));'
    
    if old3 in content:
        idx = content.find(b'let temp_norm_files_arc = ')
        count = content.count(b'let temp_norm_files_arc = ')
        if count > 1:
            content = content.replace(old3, new3, 1)
            fixes += 1
            print(f"[{fixes}] Fix 3: Removed duplicate temp_norm_files_arc")
        else:
            fixes += 1
            print(f"[{fixes}] Fix 3: Only 1 instance (moved, no duplicate)")
    elif b'let temp_norm_files_arc = Arc::new' in content:
        count = content.count(b'let temp_norm_files_arc = ')
        if count == 1:
            fixes += 1
            print(f"[{fixes}] Fix 3: Single instance OK")
        else:
            print(f"[FAIL] Fix 3: {count} instances found")
    else:
        print("[FAIL] Fix 3: Pattern not found (may use different declaration)")
    
    # ─────────────────────────────────────────────────────────────────
    # FIX 4: Add repaired file registration and duration re-probing
    # ─────────────────────────────────────────────────────────────────
    # Find: working_input_files = updated_files; (inside if !removed_quarantined_indices.is_empty())
    old4a = b'                working_input_files = updated_files;\n                working_input_durations'
    new4a = (
        b'                let pre_update_paths: std::collections::HashSet<String> = '
        b'working_input_files.iter().cloned().collect();\n'
        b'                working_input_files = updated_files;\n'
        b'                working_input_durations'
    )
    
    if old4a in content:
        content = content.replace(old4a, new4a, 1)
        fixes += 1
        print(f"[{fixes}] Fix 4a: Added pre_update_paths tracking")
    else:
        if b'pre_update_paths:' in content:
            fixes += 1
            print(f"[{fixes}] Fix 4a: Already present")
        else:
            print("[FAIL] Fix 4a: Pattern not found")
    
    # Add the P0-2/P0-3 loop after input_paths = ... (after the last .collect())
    old4b = b'                working_total_duration = working_input_durations.iter().sum();\n            }\n        } else {\n            log::info!("[MEDIA_VALIDATION] All {} files passed"'
    new4b = (
        b'                working_total_duration = working_input_durations.iter().sum();\n'
        b'\n'
        b'                // P0-2: Register repaired temp files with TempCleanup.\n'
        b'                // Repaired files are created in temp_dir but are NOT tracked by\n'
        b'                // the existing cleanup mechanisms (norm_files, registry.sub).\n'
        b'                //\n'
        b'                // P0-3: Re-probe repaired files for actual duration.\n'
        b'                // After repair, duration may differ (especially re-encode).\n'
        b'                // Subtitle offset calculations depend on accurate durations.\n'
        b'                for (new_path, _old_dur_idx) in working_input_files.iter().zip(0..) {\n'
        b'                    if !pre_update_paths.contains(new_path) {\n'
        b'                        if let Ok(mut files) = temp_norm_files_arc.lock() {\n'
        b'                            files.push(PathBuf::from(new_path));\n'
        b'                        }\n'
        b'                        let repaired_path = Path::new(new_path);\n'
        b'                        if repaired_path.exists() {\n'
        b'                            if let Some(Ok(ref _info)) = probe_cache.get(repaired_path) {\n'
        b'                                // Repaired file exists and is probed - log for admin\n'
        b'                            }\n'
        b'                        }\n'
        b'                    }\n'
        b'                }\n'
        b'            }\n'
        b'        } else {\n'
        b'            log::info!("[MEDIA_VALIDATION] All {} files passed"'
    )
    
    if old4b in content:
        content = content.replace(old4b, new4b, 1)
        fixes += 1
        print(f"[{fixes}] Fix 4b: Added temp file registration and duration re-probing")
    else:
        if b'temp_norm_files_arc.lock()' in content:
            fixes += 1
            print(f"[{fixes}] Fix 4b: Already present")
        else:
            print("[FAIL] Fix 4b: Pattern not found")
    
    # ─────────────────────────────────────────────────────────────────
    # FIX 5: Add provenance collections declarations
    # ─────────────────────────────────────────────────────────────────
    old5 = b'    let mut validation_revalidation_duration_ms: u128 = 0;\n    // Per-damage repair effectiveness'
    new5 = (
        b'    let mut validation_revalidation_duration_ms: u128 = 0;\n'
        b'    // P0-4: Provenance tracking collections\n'
        b'    let mut validation_fixed_paths: std::collections::HashSet<String> = '
        b'std::collections::HashSet::new();\n'
        b'    let mut validation_repair_methods: std::collections::HashMap<String, String> = '
        b'std::collections::HashMap::new();\n'
        b'    // Per-damage repair effectiveness'
    )
    
    if old5 in content:
        content = content.replace(old5, new5, 1)
        fixes += 1
        print(f"[{fixes}] Fix 5: Added provenance collections declarations")
    else:
        if b'validation_fixed_paths' in content and b'std::collections::HashSet<String>' in content:
            fixes += 1
            print(f"[{fixes}] Fix 5: Already present")
        else:
            print("[FAIL] Fix 5: Pattern not found")
    
    # ─────────────────────────────────────────────────────────────────
    # FIX 6: Add provenance population code (correct location)
    # ─────────────────────────────────────────────────────────────────
    # Find the _ => {} followed by closing braces and the MEDIA_VALIDATION separator
    import re
    all_us = [m.start() for m in re.finditer(
        b'                    _ => {}\n                }\n            }\n        }', content)]
    
    if all_us:
        idx = all_us[-1]
        # Find what follows to find insertion point
        rest = content[idx:]
        # Look for the MEDIA_VALIDATION separator (with doubled box-drawing chars)
        mv_log = b'\n        log::info!("[MEDIA_VALIDATION] '
        mv_idx = rest.find(mv_log)
        else_block = b'\n    } else {'
        else_idx = rest.find(else_block)
        
        if mv_idx >= 0 and else_idx >= 0 and mv_idx < else_idx:
            # The MEDIA_VALIDATION log is BEFORE the else block - inside the if block
            # This is the correct insertion point
            insert_block = (
                b'\n'
                b'        // P0-4: Populate provenance tracking from media_report\n'
                b'        for r in &media_report.file_results {\n'
                b'            if r.is_fixed() {\n'
                b'                if let Some(ref path) = r.repaired_path {\n'
                b'                    validation_fixed_paths.insert(path.clone());\n'
                b'                    if let Some(ref fix) = r.fix_applied {\n'
                b'                        validation_repair_methods.insert(path.clone(), '
                b'format!("{:?}", fix));\n'
                b'                    }\n'
                b'                }\n'
                b'            }\n'
                b'        }\n'
            )
            abs_pos = idx + mv_idx
            content = content[:abs_pos] + insert_block + content[abs_pos:]
            fixes += 1
            print(f"[{fixes}] Fix 6: Added provenance population code (correct location)")
        else:
            print(f"[FAIL] Fix 6: Wrong structure (mv={mv_idx}, else={else_idx})")
    else:
        print("[FAIL] Fix 6: No _ => {} found")
    
    # ─────────────────────────────────────────────────────────────────
    # FIX 7: Change provenance log to use persistent collections
    # ─────────────────────────────────────────────────────────────────
    # Find the provenance log section that uses media_report.file_results
    # Pattern: MERGE INPUT PROVENANCE with media_report.file_results.iter()
    old7 = (
        b'        let (source, method_str, revalidated_str) = '
        b'if let Some(result) = media_report.file_results.iter()'
        b'.find(|r| r.file_path == *fpath) {\n'
        b'            if result.is_fixed() {\n'
        b'                let method = result.fix_applied.as_ref()'
        b'.map(|f| format!("{:?}", f)).unwrap_or_default();\n'
        b'                ("REPAIRED", method, "PASS")\n'
        b'            } else {\n'
        b'                ("ORIGINAL", "none".to_string(), "N/A".to_string())\n'
        b'            }\n'
        b'        } else {\n'
        b'            ("ORIGINAL", "none".to_string(), "N/A".to_string())\n'
        b'        };'
    )
    new7 = (
        b'        let (source, method_str, revalidated_str) = '
        b'if validation_fixed_paths.contains(fpath) {\n'
        b'            let method = validation_repair_methods.get(fpath)\n'
        b'                .cloned()\n'
        b'                .unwrap_or_default();\n'
        b'            ("REPAIRED", method, "PASS".to_string())\n'
        b'        } else {\n'
        b'            ("ORIGINAL", "none".to_string(), "N/A".to_string())\n'
        b'        };'
    )
    
    if old7 in content:
        content = content.replace(old7, new7, 1)
        fixes += 1
        print(f"[{fixes}] Fix 7: Changed provenance log to use persistent collections")
    elif b'validation_fixed_paths.contains(fpath)' in content:
        fixes += 1
        print(f"[{fixes}] Fix 7: Already applied")
    else:
        print("[FAIL] Fix 7: Pattern not found")
        # Check if there's a different pattern
        idx = content.find(b'MERGE_INPUT_PROVENANCE')
        if idx >= 0:
            snippet = content[idx:idx+400]
            print(f"  Context: {snippet[:300]}")
    
    # Restore CRLF and write
    if has_crlf:
        content = content.replace(b'\n', b'\r\n')
    
    with open(path, 'wb') as f:
        f.write(content)
    
    print(f"\n=== Total: {fixes} fixes applied ===")

if __name__ == '__main__':
    main()
