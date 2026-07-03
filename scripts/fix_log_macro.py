#!/usr/bin/env python3
"""Fix the broken log::info! macro and properly place container_copy counter."""

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'r', encoding='utf-8') as f:
        lines = f.readlines()
    
    # Line 4260 (0-based: 4259) is the broken one-liner
    # It should be: the ternary starts with "if only_remux && dm.timescale_den.is_some() { \"Timescale Remux (Lossless)\" }"
    # But instead it has the container_copy counter prepended
    
    broken_line = 4259  # 0-based index
    line_content = lines[broken_line]
    
    # Check if this line contains the broken pattern
    if 'files_container_copy' in line_content and 'FORENSIC:NORMALIZE' not in line_content:
        # This line was corrupted. We need to replace it with just the ternary expression
        # The original should be: "                    if only_remux && dm.timescale_den.is_some() { \"Timescale Remux (Lossless)\" }"
        
        # Find where the actual ternary starts (after the injected if block)
        marker = 'if only_remux && dm.timescale_den.is_some() {'
        idx = line_content.find(marker)
        if idx >= 0:
            # Extract the clean ternary from this point
            clean_ternary = line_content[idx:]
            lines[broken_line] = '                    ' + clean_ternary
            print(f"Fixed broken line {broken_line+1}: restored ternary expression")
        else:
            # Can't find the ternary - write it manually
            lines[broken_line] = '                    if only_remux && dm.timescale_den.is_some() { "Timescale Remux (Lossless)" }\n'
            print(f"Replaced broken line {broken_line+1} with clean ternary")
    
    # Now add container_copy increment after the files_re_encoded block
    # Find the line with "files_re_encoded += 1;"
    for i, line in enumerate(lines):
        if 'files_re_encoded += 1;' in line:
            # Check if container_copy is already there
            check_region = ''.join(lines[max(0,i-5):i+5])
            if 'files_container_copy' not in check_region:
                # Insert container_copy increment after the re_encoded closing brace
                # Find the } after files_re_encoded += 1;
                for j in range(i, min(i+3, len(lines))):
                    if lines[j].strip() == '}' and j > i:
                        lines.insert(j + 1, '                    if only_remux && dm.timescale_den.is_none() {\n                        files_container_copy += 1;\n                    }\n')
                        print(f"Inserted files_container_copy increment at line {j+2}")
                        break
            else:
                print("files_container_copy already exists near files_re_encoded")
            break
    
    with open(path, 'w', encoding='utf-8') as f:
        f.writelines(lines)
    
    # Verify
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    checks = [
        ('files_container_copy += 1', 'container_copy counter'),
        ('"Timescale Remux (Lossless)"', 'ternary expression intact'),
        ('files_re_encoded += 1', 're_encoded counter intact'),
    ]
    
    print("\nVerification:")
    for pattern, label in checks:
        found = pattern in content
        print(f"  [{'PASS' if found else 'FAIL'}] {label}")
    
    # Check the broken pattern is gone
    if 'files_container_copy' in content and 'FORENSIC:NORMALIZE' in content:
        # Make sure no line has both
        for i, line in enumerate(content.split('\n')):
            if 'files_container_copy' in line and 'FORENSIC:NORMALIZE' in line:
                print(f"\n  [WARN] Line {i+1} still has container_copy inside log macro!")
                break
        else:
            print("  [PASS] No lines with container_copy inside log macro")

if __name__ == '__main__':
    main()
