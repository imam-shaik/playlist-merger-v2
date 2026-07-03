#!/usr/bin/env python3
"""Fix corrupted merge.rs - remove misplaced provenance code and restore structure."""

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'rb') as f:
        content = f.read()
    
    # Normalize to LF
    if b'\r\n' in content:
        content = content.replace(b'\r\n', b'\n')
    
    # Fix 1: Remove misplaced provenance block from else block
    # Pattern: inside else block, after separator log
    old1 = b'        // P0-4: Populate provenance tracking from media_report\n        // (media_report goes out of scope when this if-block ends)\n        for r in &media_report.file_results {\n            if r.is_fixed() {\n                if let Some(ref path) = r.repaired_path {\n                    validation_fixed_paths.insert(path.clone());\n                    if let Some(ref fix) = r.fix_applied {\n                        validation_repair_methods.insert(path.clone(), format!("{:?}", fix));\n                    }\n                }\n            }\n        }\n        '
    
    if old1 in content:
        content = content.replace(old1, b'', 1)
        print("Fix 1: Removed misplaced provenance block")
    else:
        print("Fix 1: Pattern not found - checking for partial match")
        idx = content.find(b'P0-4: Populate provenance tracking from media_report')
        if idx >= 0:
            # Find the end of the block (the blank line or next log statement)
            end = content.find(b'\n\n        log::info!', idx)
            if end > 0:
                # Remove from the comment to the blank line before the next log
                before = content[:idx-8]  # Remove the leading spaces and comment
                after = content[end:]
                content = before + after
                print("Fix 1: Removed via partial match")
            else:
                print("Fix 1: Could not find end of block")
        else:
            print("Fix 1: Not found - may already be fixed")
    
    # Fix 2: Check for orphaned box-drawing chars (line 2057)
    # These would be ═ characters that are not part of a valid statement
    # They appear as sequential \xe2\x95\x90 bytes
    idx2 = content.find(b'\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90"')
    if idx2 >= 0:
        # Find the start of this line
        line_start = content.rfind(b'\n', 0, idx2)
        line_end = content.find(b'\n', idx2)
        if line_start >= 0 and line_end > line_start:
            orphaned_line = content[line_start+1:line_end]
            print(f"Found orphaned box-drawing line: {orphaned_line[:80]}")
            # Remove this line
            content = content[:line_start] + content[line_end:]
            print("Fix 2: Removed orphaned box-drawing line")
    else:
        print("Fix 2: No orphaned box-drawing chars found")
    
    # Fix 3: Check for double MEDIA_VALIDATION separator lines
    # After fix 1, there might be a blank line followed by the separator
    # followed by the else block
    old3 = b'log::info!("[MEDIA_VALIDATION] \xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90"\n    } else {\n        log::info!'
    new3 = b'log::info!("[MEDIA_VALIDATION] \xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90\xe2\x95\x90"\n    } else {\n        log::info!'
    
    # Actually let me just check the overall structure by finding the else block
    idx_else = content.find(b'    } else {\n        log::info!("[MEDIA_VALIDATION] Media Validation Engine DISABLED')
    if idx_else >= 0:
        # Check what's directly before the }
        before_else = content[idx_else-100:idx_else]
        print(f"Before else block: {before_else[-80:]}")
        if b'log::info!("[MEDIA_VALIDATION] ' in before_else:
            print("Fix 3: Structure looks correct - separator before else")
        else:
            print("Fix 3: Structure may be broken - separator missing before else")
    
    # Restore CRLF
    if b'\n' in content and b'\r\n' not in content:
        content = content.replace(b'\n', b'\r\n')
    
    with open(path, 'wb') as f:
        f.write(content)
    
    print("\nDone")

if __name__ == '__main__':
    main()
