#!/usr/bin/env python3
"""Fix 3 semantic issues in profiler counters."""
import sys

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    lines = content.split('\n')
    changes = 0

    # ═══════════════════════════════════════════════════════════════
    # FIX 1: Add files_container_copy variable declaration
    # ═══════════════════════════════════════════════════════════════
    for i, line in enumerate(lines):
        if 'let mut files_re_encoded: usize = 0;' in line:
            # Add files_container_copy after this line
            lines.insert(i + 1, '    let mut files_container_copy: usize = 0;')
            changes += 1
            print(f"FIX 1: Added files_container_copy declaration at line {i+2}")
            break

    # ═══════════════════════════════════════════════════════════════
    # FIX 2: Add files_container_copy increment in the normalization loop
    # ═══════════════════════════════════════════════════════════════
    for i, line in enumerate(lines):
        if 'if !only_remux {' in line and 'files_re_encoded' in lines[i+2] if i+2 < len(lines) else False:
            # Insert container_copy increment before the re-encode check
            insert_line = '                    if only_remux && dm.timescale_den.is_none() {\n                        files_container_copy += 1;\n                    }'
            lines.insert(i, insert_line)
            changes += 1
            print(f"FIX 2: Added files_container_copy increment at line {i+1}")
            break

    # ═══════════════════════════════════════════════════════════════
    # FIX 3: Update performance report labels for clarity
    # ═══════════════════════════════════════════════════════════════
    for i, line in enumerate(lines):
        if 'PERF_REPORT' in line and 'files_total' in line and 'files_healthy' in line:
            # Update the label line to include container_copy and clarify
            lines[i] = '                log::info!("[PERF_REPORT]  Files:       {} total | {} after quarantine | {} damaged | {} remuxed | {} re-encoded | {} container-copy", files_total, files_healthy, files_damaged, files_repaired, files_re_encoded, files_container_copy);'
            changes += 1
            print(f"FIX 3: Updated performance report labels at line {i+1}")
            break

    # ═══════════════════════════════════════════════════════════════
    # Write the file
    # ═══════════════════════════════════════════════════════════════
    with open(path, 'w', encoding='utf-8') as f:
        f.write('\n'.join(lines))
    
    print(f"\nTotal changes: {changes}")

if __name__ == '__main__':
    main()
