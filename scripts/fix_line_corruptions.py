#!/usr/bin/env python3
"""Fix specific line corruptions in merge.rs around lines 2056-2073."""

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'rb') as f:
        content = f.read()
    
    # Split into lines
    lines = content.split(b'\n')
    
    fixed = 0
    
    # Show what's around the problem area
    print("=== Lines 2050-2080 BEFORE fixes ===")
    for i in range(max(0, 2049), min(len(lines), 2080)):
        line = lines[i].rstrip(b'\r')
        print(f"{i+1}: {line[:120]}")
    
    # Fix 1: Line ~2056 - remove orphaned box-drawing chars
    # These appear as consecutive \xe2\x95\x90 bytes without a valid Rust statement
    for i in range(len(lines)):
        line = lines[i]
        # Check for a line that is ONLY box-drawing chars + closing paren + semicolon
        # Like: \xe2\x95\x90\xe2\x95\x90...");
        stripped = line.strip()
        if stripped.startswith(b'\xe2\x95\x90') and stripped.endswith(b'")'):
            # This is an orphaned log line remnant - remove it
            # Check if it's a complete log statement
            if b'log::info' not in line:
                print(f"\n  Removing orphaned box-drawing line {i+1}")
                lines[i] = b''
                fixed += 1
    
    # Fix 2: Reconstruct lines from 2055-2075
    # The structure should be:
    #     }  // close for loop
    #   }  // close if let
    # }  // close match
    # 
    # // P0-4 provenance population (if inserted)
    # 
    # log::info!("[MEDIA_VALIDATION] ══...");  
    # } else {
    # log::info!("[MEDIA_VALIDATION] Media Validation Engine DISABLED...");
    # log::info!("[STAGE_TIMING] MEDIA_VALIDATION | SKIPPED (disabled)");
    # phase_start = std::time::Instant::now();
    # }
    
    # Find the } else { line for MEDIA_VALIDATION disabled
    for i in range(len(lines)):
        if b'} else {' in lines[i] and b'MEDIA_VALIDATION' in lines[max(0,i+1)]:
            else_idx = i
            print(f"\nFound }} else {{ for MEDIA_VALIDATION at line {else_idx+1}")
            
            # Check what's before this
            if else_idx > 0:
                before = lines[else_idx-1].strip()
                # Check if there's a blank line or a log statement
                if b'MEDIA_VALIDATION' not in lines[else_idx-1]:
                    # May need to add the MEDIA_VALIDATION separator
                    print(f"  Line before: {lines[else_idx-1][:80]}")
                if b'MEDIA_VALIDATION' in lines[else_idx-1]:
                    print(f"  Separator found at line {else_idx}")
            break
    
    # Fix 3: Check for escaped character issues around the PACKET TIMESTAMP CERTIFICATION section
    for i in range(len(lines)):
        if b'PACKET TIMESTAMP CERTIFICATION' in lines[i]:
            # This section should have a comment like:
            # // ── PACKET TIMESTAMP CERTIFICATION ──
            line = lines[i]
            if b'\xe2\x94\x80' in line and b'//' not in line:
                # Add // prefix
                print(f"\n  Fixing PACKET TIMESTAMP line {i+1}")
                lines[i] = b'// ' + line.lstrip()
                fixed += 1
    
    # Write back
    content = b'\n'.join(lines)
    
    print(f"\n=== Lines after fixes ===")
    for i in range(max(0, 2049), min(len(lines), 2080)):
        line = lines[i].rstrip(b'\r')
        print(f"{i+1}: {line[:120]}")
    
    with open(path, 'wb') as f:
        f.write(content)
    
    print(f"\nTotal fixes: {fixed}")

if __name__ == '__main__':
    main()
