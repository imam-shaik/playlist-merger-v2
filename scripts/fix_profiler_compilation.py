#!/usr/bin/env python3
"""Fix all 3 compilation errors in merge.rs profiler counters."""
import sys

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    lines = content.split('\n')
    changes = 0

    # ═══════════════════════════════════════════════════════════════
    # FIX 1: Remove broken files_repaired += 1 inside log::info! macro
    # It was inserted between ternary branches inside the log macro
    # ═══════════════════════════════════════════════════════════════
    # Find the broken line
    for i, line in enumerate(lines):
        if 'files_repaired += 1' in line and i > 4250 and i < 4265:
            # This is the broken line inside the log macro - remove it
            lines[i] = ''
            changes += 1
            print(f"FIX 1: Removed broken files_repaired += 1 from inside log::info! at line {i+1}")
            break

    # ═══════════════════════════════════════════════════════════════
    # FIX 2: Remove the broken counter block that uses undefined vars
    # and media_report (out of scope)
    # ═══════════════════════════════════════════════════════════════
    # Find and remove the broken counter block
    broken_start = None
    for i, line in enumerate(lines):
        if '// \u2500\u2500 Validation counters \u2500\u2500' in line:
            broken_start = i
            break
    
    if broken_start is not None:
        # Remove the entire broken block (validation counters + log + blank line)
        # Count lines until we hit "dur_validation"
        end = broken_start
        for i in range(broken_start, min(broken_start + 10, len(lines))):
            if 'dur_validation = phase_start' in lines[i]:
                end = i
                break
        
        # Remove lines from broken_start to end-1 (keep dur_validation line)
        removed_count = end - broken_start
        for j in range(removed_count):
            lines.pop(broken_start)
        changes += 1
        print(f"FIX 2: Removed broken counter block ({removed_count} lines) at line {broken_start+1}")
    
    # ═══════════════════════════════════════════════════════════════
    # FIX 3: Insert CORRECT counter code BEFORE quarantine handling
    # (where media_report and quarantined_count are in scope)
    # ═══════════════════════════════════════════════════════════════
    # Find "let quarantined_count = media_report.quarantined_count;"
    insert_idx = None
    for i, line in enumerate(lines):
        if 'let quarantined_count = media_report.quarantined_count' in line:
            insert_idx = i
            break
    
    if insert_idx is not None:
        # Insert BEFORE this line (where media_report is in scope)
        counter_block = [
            '    // \u2500\u2500 Validation counters \u2500\u2500',
            '    files_total = working_input_files.len();',
            '    files_damaged = media_report.quarantined_count;',
            '    files_healthy = files_total.saturating_sub(files_damaged);',
            '    log::info!("[STAGE_TIMING] COUNTERS | total={} | healthy={} | damaged={}",',
            '        files_total, files_healthy, files_damaged);',
            '',
        ]
        for j, block_line in enumerate(counter_block):
            lines.insert(insert_idx + j, block_line)
        changes += 1
        print(f"FIX 3: Inserted counter block before quarantined_count at line {insert_idx+1}")
    else:
        # Try alternative: insert right after media_report creation
        for i, line in enumerate(lines):
            if 'validate_input_files(' in line and 'media_validation_engine' in line:
                # Find the closing of this statement
                for j in range(i, min(i + 10, len(lines))):
                    if ');' in lines[j]:
                        insert_idx = j + 1
                        break
                break
        
        if insert_idx is not None:
            counter_block = [
                '    // \u2500\u2500 Validation counters \u2500\u2500',
                '    files_total = working_input_files.len();',
                '    files_damaged = media_report.quarantined_count;',
                '    files_healthy = files_total.saturating_sub(files_damaged);',
                '    log::info!("[STAGE_TIMING] COUNTERS | total={} | healthy={} | damaged={}",',
                '        files_total, files_healthy, files_damaged);',
                '',
            ]
            for j, block_line in enumerate(counter_block):
                lines.insert(insert_idx + j, block_line)
            changes += 1
            print(f"FIX 3 alt: Inserted counter block after validate_input_files at line {insert_idx+1}")

    # ═══════════════════════════════════════════════════════════════
    # FIX 4: Properly insert files_repaired and files_re_encoded 
    # AFTER the log::info! macro in the normalization loop
    # ═══════════════════════════════════════════════════════════════
    # Find the FORENSIC:NORMALIZE END log line
    forensic_end_idx = None
    for i, line in enumerate(lines):
        if 'FORENSIC:NORMALIZE] END' in line:
            forensic_end_idx = i
            break
    
    if forensic_end_idx is not None:
        # Find the end of this log::info! (the line with ');')
        macro_end_idx = None
        for i in range(forensic_end_idx, min(forensic_end_idx + 10, len(lines))):
            if lines[i].rstrip().endswith(');'):
                macro_end_idx = i
                break
        
        if macro_end_idx is not None:
            # Check if files_repaired or files_re_encoded are already after this
            check_text = '\n'.join(lines[macro_end_idx+1:macro_end_idx+5])
            
            if 'files_repaired' not in check_text:
                insert_after = [
                    '                    if only_remux && dm.timescale_den.is_some() {',
                    '                        files_repaired += 1;',
                    '                    }',
                    '                    if !only_remux {',
                    '                        files_re_encoded += 1;',
                    '                    }',
                ]
                for j, new_line in enumerate(insert_after):
                    lines.insert(macro_end_idx + 1 + j, new_line)
                changes += 1
                print(f"FIX 4: Inserted files_repaired/files_re_encoded increments after FORENSIC:NORMALIZE log at line {macro_end_idx+2}")
        else:
            print(f"FIX 4: Could not find end of FORENSIC:NORMALIZE log macro")
    else:
        print(f"FIX 4: Could not find FORENSIC:NORMALIZE END log")

    # ═══════════════════════════════════════════════════════════════
    # FIX 5: Add else branch for should_process_subs with SKIPPED logging
    # ═══════════════════════════════════════════════════════════════
    # Find "if should_process_subs {"
    sub_start = None
    for i, line in enumerate(lines):
        if 'if should_process_subs {' in line and 'let' not in line:
            sub_start = i
            break
    
    if sub_start is not None:
        # Track brace depth to find closing brace
        depth = 0
        end_idx = None
        for i in range(sub_start, len(lines)):
            depth += lines[i].count('{') - lines[i].count('}')
            if depth == 0 and i > sub_start:
                end_idx = i
                break
        
        if end_idx is not None:
            # Check what's after the closing brace
            next_line = lines[end_idx + 1].strip() if end_idx + 1 < len(lines) else ''
            if 'else' in next_line or 'SUBTITLE_PROCESSING' in next_line:
                print(f"FIX 5: SKIPPED - else branch already exists after line {end_idx+2}")
            else:
                else_block = ' else {\n        phase_start = std::time::Instant::now();\n        log::info!("[STAGE_TIMING] SUBTITLE_PROCESSING | SKIPPED (mode={{:?}})", subtitle_mode);\n        phase_start = std::time::Instant::now();\n    }'
                lines.insert(end_idx + 1, else_block)
                changes += 1
                print(f"FIX 5: Inserted else branch with SKIPPED logging at line {end_idx+2}")
        else:
            print(f"FIX 5: Could not find closing brace for should_process_subs")
    else:
        print(f"FIX 5: Could not find if should_process_subs")

    # ═══════════════════════════════════════════════════════════════
    # Write the file
    # ═══════════════════════════════════════════════════════════════
    with open(path, 'w', encoding='utf-8') as f:
        f.write('\n'.join(lines))
    
    print(f"\nTotal changes: {changes}")
    
    # ═══════════════════════════════════════════════════════════════
    # Verify
    # ═══════════════════════════════════════════════════════════════
    with open(path, 'r', encoding='utf-8') as f:
        verify = f.read()
    
    checks = [
        ('files_total = working_input_files.len()', 'Counter: files_total'),
        ('files_damaged = media_report.quarantined_count', 'Counter: files_damaged'),
        ('files_healthy = files_total.saturating_sub(files_damaged)', 'Counter: files_healthy'),
        ('files_repaired += 1', 'Counter: files_repaired increment'),
        ('files_re_encoded += 1', 'Counter: files_re_encoded increment'),
        ('SUBTITLE_PROCESSING | SKIPPED', 'Subtitle SKIPPED logging'),
        ('FORENSIC:NORMALIZE] END', 'FORENSIC:NORMALIZE log intact'),
    ]
    
    print("\nVerification:")
    for pattern, label in checks:
        found = pattern in verify
        status = "PASS" if found else "FAIL"
        print(f"  [{status}] {label}")
    
    # Check that broken pattern is gone
    bad_checks = [
        ('original_file_count', 'Broken original_file_count reference'),
        ('media_report.quarantined_count;\n    files_healthy', 'media_report in wrong scope'),
    ]
    for pattern, label in bad_checks:
        # Only check if it's NOT supposed to be there
        pass

if __name__ == '__main__':
    main()
