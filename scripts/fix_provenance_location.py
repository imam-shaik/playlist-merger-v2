#!/usr/bin/env python3
"""Fix the incorrectly placed provenance population code.

The code was inserted into the `else` block where media_report doesn't exist.
Need to:
1. Remove it from the else block
2. Insert it at the correct location inside the if block
"""
import sys

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'rb') as f:
        content = f.read()
    
    has_crlf = b'\r\n' in content
    if has_crlf:
        content = content.replace(b'\r\n', b'\n')
    
    edits = 0
    
    # Step 1: Remove the incorrectly placed provenance block from else block
    # The block is in the else branch starting with:
    # `                // P0-4: Populate provenance tracking from media_report`
    # and ending at the blank line before the next log line or closing brace.
    
    needle = (
        b'                // P0-4: Populate provenance tracking from media_report\n'
        b'        // (media_report goes out of scope when this if-block ends)\n'
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
        b''
    )
    
    if needle in content:
        content = content.replace(needle, b'', 1)
        edits += 1
        print(f"[OK] Removed misplaced provenance block from else block")
    else:
        print(f"[FAIL] Could not find misplaced provenance block exactly")
        # Try to find partial match
        idx = content.find(b'// P0-4: Populate provenance tracking from media_report')
        if idx >= 0:
            # Show surrounding context for debugging
            snippet = content[idx:idx+500]
            # Print first 100 bytes and last 50 bytes
            print(f"  Found at offset {idx}")
            print(f"  Start: {snippet[:120]}")
            print(f"  End:   {snippet[-100:]}")
    
    # Step 2: Read the file around the correct insertion location
    # The correct location is inside the if settings.enable_media_validation block,
    # after the one `_ => {}` match arm, before the MEDIA_VALIDATION separator log.
    # There's only one `_ => {}` in the ENTIRE file.
    
    # Find the _ => {} followed by closing braces and then find the MEDIA_VALIDATION log
    import re
    all_underscore = [m.start() for m in re.finditer(b'                    _ => {}', content)]
    
    if all_underscore:
        us_idx = all_underscore[-1]
        # Search for MEDIA_VALIDATION log after this point, but BEFORE the } else {
        after_underscore = content[us_idx:]
        
        # Find the next MEDIA_VALIDATION log that's NOT in the else block
        # Strategy: find } else { after _ => {} and make sure we insert before it
        else_pos = after_underscore.find(b'    } else {')
        if else_pos < 0:
            else_pos = after_underscore.find(b'} else {')
        
        mv_log_marker = b'        log::info!("[MEDIA_VALIDATION] \xe2\x95\x90\xe2\x95\x90'
        mv_pos = after_underscore.find(mv_log_marker)
        
        if mv_pos >= 0 and else_pos >= 0:
            if mv_pos < else_pos:
                # The MEDIA_VALIDATION log is before the } else { - inside the if block
                # This is the correct insertion point
                insert_block = (
                    b'        // P0-4: Populate provenance tracking from media_report\n'
                    b'        // (media_report goes out of scope when this if-block ends)\n'
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
                # Insert at the MEDIA_VALIDATION log position (before it)
                abs_insert_pos = us_idx + mv_pos
                content = content[:abs_insert_pos] + insert_block + content[abs_insert_pos:]
                edits += 1
                print(f"[OK] Inserted provenance block at correct location (offset {abs_insert_pos})")
            else:
                print(f"[FAIL] MEDIA_VALIDATION log is AFTER } else {{ - in wrong block")
        elif mv_pos >= 0:
            insert_block = (
                b'        // P0-4: Populate provenance tracking from media_report\n'
                b'        // (media_report goes out of scope when this if-block ends)\n'
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
            abs_insert_pos = us_idx + mv_pos
            content = content[:abs_insert_pos] + insert_block + content[abs_insert_pos:]
            edits += 1
            print(f"[OK] Inserted at offset {abs_insert_pos} (no else block found)")
        else:
            print("[FAIL] MEDIA_VALIDATION log not found after _ => {}")
    else:
        print("[FAIL] No _ => {} found in file")
    
    # Restore CRLF and write
    if has_crlf:
        content = content.replace(b'\n', b'\r\n')
    
    with open(path, 'wb') as f:
        f.write(content)
    
    print(f"\nTotal edits: {edits}")
    return edits == 2

if __name__ == '__main__':
    success = main()
    exit(0 if success else 1)
