#!/usr/bin/env python3
"""Final fixes for P0 provenance:
1. Add validation_fixed_paths/validation_repair_methods population 
2. Fix type mismatch: "PASS" -> "PASS".to_string()
"""

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'rb') as f:
        content = f.read()
    
    # Normalize to LF
    has_crlf = b'\r\n' in content
    if has_crlf:
        content = content.replace(b'\r\n', b'\n')
    
    fixes = 0
    
    # ── FIX 1: Add provenance population code ──
    # Pattern: end of damage match + closing braces + blank line + MEDIA_VALIDATION log
    needle = b'                    _ => {}\n                }\n            }\n        }\n\n        log::info!("[MEDIA_VALIDATION] '
    
    if needle in content:
        # Replace the needle with just the closing section (no MEDIA_VALIDATION log)
        closing = b'                    _ => {}\n                }\n            }\n        }\n'
        content = content.replace(needle, closing, 1)
        
        # Re-find the MEDIA_VALIDATION log and insert the population code before it
        mv_idx = content.find(b'\n        log::info!("[MEDIA_VALIDATION] ')
        if mv_idx >= 0:
            before = content[:mv_idx]
            after = content[mv_idx:]
            insert_block = (
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
            )
            content = before + insert_block + after
            fixes += 1
            print("FIX 1 (provenance population): OK")
        else:
            # Try with different spacing
            mv_idx = content.find(b'log::info!("[MEDIA_VALIDATION] ')
            if mv_idx >= 0:
                before = content[:mv_idx]
                after = content[mv_idx:]
                insert_block = (
                    b'\n'
                    b'        // P0-4: Populate provenance tracking from media_report\n'
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
                )
                content = before + insert_block + after
                fixes += 1
                print("FIX 1 (provenance population): OK (alternate insertion)")
            else:
                print("FIX 1: FAILED - MEDIA_VALIDATION log not found")
    else:
        if b'validation_fixed_paths.insert' in content:
            fixes += 1
            print("FIX 1 (provenance population): already applied")
        else:
            print("FIX 1: FAILED - marker not found")
            idx = content.find(b'_ => {}\n                }\n            }')
            if idx >= 0:
                snippet = content[idx:idx+500]
                print(f"  Found partial match, context: {snippet[:300]}")
    
    # ── FIX 2: Fix type mismatch ──
    old2 = b'("REPAIRED", method, "PASS")\n        } else {\n            ("ORIGINAL", "none".to_string(), "N/A".to_string())'
    new2 = b'("REPAIRED", method, "PASS".to_string())\n        } else {\n            ("ORIGINAL", "none".to_string(), "N/A".to_string())'
    
    if old2 in content:
        content = content.replace(old2, new2, 1)
        fixes += 1
        print("FIX 2 (type mismatch): OK")
    elif b'"REPAIRED", method, "PASS".to_string()' in content:
        fixes += 1
        print("FIX 2 (type mismatch): already applied")
    else:
        print("FIX 2: FAILED - pattern not found")
        idx = content.find(b'REPAIRED')
        if idx >= 0:
            snippet = content[idx:idx+120]
            print(f"  REPAIRED context: {snippet}")
    
    # Restore CRLF
    if has_crlf:
        content = content.replace(b'\n', b'\r\n')
    
    with open(path, 'wb') as f:
        f.write(content)
    
    print(f"\n=== RESULT: {fixes}/2 fixes applied ===")
    return fixes == 2

if __name__ == '__main__':
    success = main()
    exit(0 if success else 1)
