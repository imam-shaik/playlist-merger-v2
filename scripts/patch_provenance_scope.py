#!/usr/bin/env python3
"""Fix media_report scope issue in merge.rs.

The provenance logging code at ~line 5715 tries to access media_report.file_results
but media_report is declared inside an `if settings.enable_media_validation { }` block
and goes out of scope.
"""

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'rb') as f:
        content = f.read()
    
    had_crlf = b'\r\n' in content
    if had_crlf:
        content = content.replace(b'\r\n', b'\n')
    
    fixes = 0
    
    # ── FIX 1a: Add persistent collections declarations ──
    marker1 = b'    let mut validation_revalidation_duration_ms: u128 = 0;\n    // Per-damage repair effectiveness'
    insert1 = (
        b'    let mut validation_revalidation_duration_ms: u128 = 0;\n'
        b'    // P0-4: Provenance tracking collections (populated inside media validation if block,\n'
        b'    // used by MERGE_INPUT_PROVENANCE logging before final merge)\n'
        b'    let mut validation_fixed_paths: std::collections::HashSet<String> = std::collections::HashSet::new();\n'
        b'    let mut validation_repair_methods: std::collections::HashMap<String, String> = std::collections::HashMap::new();\n'
        b'    // Per-damage repair effectiveness'
    )
    
    if marker1 in content:
        content = content.replace(marker1, insert1, 1)
        fixes += 1
        print("FIX 1a: Added provenance tracking collections: OK")
    else:
        print("FIX 1a: FAILED - marker not found")
        idx = content.find(b'validation_revalidation_duration_ms')
        if idx >= 0:
            print(f"  Found at offset {idx}")
    
    # ── FIX 1b: Populate collections inside the if block ──
    # Find: `_ => {}\n            }\n        }\n\n        log::info!("[MEDIA_VALIDATION] ══`
    # The ═ character is U+2550 encoded as \xe2\x95\x90
    old1b = (
        b'_ => {}\n'
        b'            }\n'
        b'        }\n'
        b'\n'
        b'        log::info!(\"[MEDIA_VALIDATION] \xe2\x95\x90\xe2\x95\x90'
    )
    new1b = (
        b'_ => {}\n'
        b'            }\n'
        b'        }\n'
        b'\n'
        b'        // P0-4: Populate provenance tracking from media_report\n'
        b'        // (media_report goes out of scope when this if-block ends,\n'
        b'        // but the merge input provenance log needs this data)\n'
        b'        for r in &media_report.file_results {\n'
        b'            if r.is_fixed() {\n'
        b'                if let Some(ref path) = r.repaired_path {\n'
        b'                    validation_fixed_paths.insert(path.clone());\n'
        b'                    if let Some(ref fix) = r.fix_applied {\n'
        b'                        validation_repair_methods.insert(path.clone(), format!("{:?}", fix));\n'
        b'                    }\n'
        b'                }\n'
        b'            }\n'
        b'        }\n'
        b'\n'
        b'        log::info!(\"[MEDIA_VALIDATION] \xe2\x95\x90\xe2\x95\x90'
    )
    
    if old1b in content:
        content = content.replace(old1b, new1b, 1)
        fixes += 1
        print("FIX 1b: Added provenance collection population: OK")
    elif b'validation_fixed_paths.insert' in content:
        fixes += 1
        print("FIX 1b: Already applied")
    else:
        print("FIX 1b: FAILED - marker not found")
        idx = content.find(b'log::info!(\"[MEDIA_VALIDATION] ')
        if idx >= 0:
            # Show the 100 bytes before this
            prev = content[max(0,idx-200):idx]
            print(f"  200 bytes before MEDIA_VALIDATION log: {prev[-150:]}")
    
    # ── FIX 2: Replace media_report in provenance log ──
    old2 = (
        b'        // P0-4: Match by file PATH (not index) because index-based matching\n'
        b'        // breaks after apply_validation_results() removes quarantined files.\n'
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
    new2 = (
        b'        // P0-4: Match by file PATH using pre-extracted provenance data.\n'
        b'        // media_report is scoped inside the validation if-block above,\n'
        b'        // so we use persistent collections populated from it instead.\n'
        b'        let (source, method_str, revalidated_str) = '
        b'if validation_fixed_paths.contains(fpath) {\n'
        b'            let method = validation_repair_methods.get(fpath)\n'
        b'                .cloned()\n'
        b'                .unwrap_or_default();\n'
        b'            ("REPAIRED", method, "PASS")\n'
        b'        } else {\n'
        b'            ("ORIGINAL", "none".to_string(), "N/A".to_string())\n'
        b'        };'
    )
    
    if old2 in content:
        content = content.replace(old2, new2, 1)
        fixes += 1
        print("FIX 2: Replaced media_report with persistent collections: OK")
    elif b'validation_fixed_paths.contains(fpath)' in content:
        print("FIX 2: Already applied")
        fixes += 1
    else:
        print("FIX 2: FAILED - pattern not found")
    
    # Write the file
    if had_crlf:
        content = content.replace(b'\n', b'\r\n')
    with open(path, 'wb') as f:
        f.write(content)
    
    print(f"\n=== RESULT: {fixes} fixes applied ===")
    return fixes >= 2

if __name__ == '__main__':
    success = main()
    exit(0 if success else 1)
