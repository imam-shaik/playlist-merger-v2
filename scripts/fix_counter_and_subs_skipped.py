#!/usr/bin/env python3
"""Fix three profiler gaps in merge.rs."""
import sys

def main():
    path = 'src-tauri/src/commands/merge.rs'
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()
    
    changes = 0
    
    # ═══════════════════════════════════════════════════════════════
    # FIX 1: Add files_healthy/damaged counters after media validation
    # ═══════════════════════════════════════════════════════════════
    # Insert right before the dur_validation line
    old1 = '    dur_validation = phase_start.elapsed().as_secs_f64();\n    log::info!("[STAGE_TIMING] MEDIA_VALIDATION'
    new1 = '''    dur_validation = phase_start.elapsed().as_secs_f64();
    log::info!("[STAGE_TIMING] MEDIA_VALIDATION'''
    
    # We need to insert the counter calculation BEFORE dur_validation
    # Find the line before dur_validation and insert there
    marker1 = '    dur_validation = phase_start.elapsed().as_secs_f64();'
    idx1 = content.find(marker1)
    if idx1 >= 0:
        # Find the previous blank line or statement
        insert1 = '''    // ── Validation counters ──
    files_total = original_file_count;
    files_damaged = media_report.quarantined_count;
    files_healthy = working_input_files.len();
    log::info!("[STAGE_TIMING] COUNTERS | total={} | healthy={} | damaged={}",
        files_total, files_healthy, files_damaged);

'''
        content = content[:idx1] + insert1 + content[idx1:]
        changes += 1
        print(f"FIX 1: Inserted counter calculation before dur_validation (at char {idx1})")
    else:
        print("FIX 1: FAILED - could not find dur_validation assignment")
    
    # ═══════════════════════════════════════════════════════════════
    # FIX 2: Add else branch with SKIPPED logging for subtitle processing
    # The if should_process_subs block ends at line 2358 (now shifted by inserted lines)
    # We need to find the closing brace after "should_process_subs" block
    # ═══════════════════════════════════════════════════════════════
    # Find the end of the subtitle block by searching for the pattern
    # The block starts with "if should_process_subs {" and we need its closing }
    # Strategy: find "if should_process_subs" then find the matching }
    
    lines = content.split('\n')
    
    # Find the line with "if should_process_subs"
    sub_start_line = None
    for i, line in enumerate(lines):
        if 'if should_process_subs {' in line and 'let' not in line:
            sub_start_line = i
            break
    
    if sub_start_line is not None:
        # Track brace depth to find the end
        depth = 0
        end_line = None
        for i in range(sub_start_line, len(lines)):
            depth += lines[i].count('{') - lines[i].count('}')
            if depth == 0 and i > sub_start_line:
                end_line = i
                break
        
        if end_line is not None:
            # Check if there's already an else block
            next_content = lines[end_line + 1].strip() if end_line + 1 < len(lines) else ''
            if next_content.startswith('else') or next_content.startswith('// ──'):
                print(f"FIX 2: SKIPPED - else block or next section already exists after line {end_line+2}")
            else:
                # Insert else branch after the closing brace
                else_block = ''' else {
        phase_start = std::time::Instant::now();
        log::info!("[STAGE_TIMING] SUBTITLE_PROCESSING | SKIPPED (mode={:?})", subtitle_mode);
        phase_start = std::time::Instant::now();
    }'''
                lines.insert(end_line + 1, else_block)
                content = '\n'.join(lines)
                changes += 1
                print(f"FIX 2: Inserted else branch at line {end_line+2}")
        else:
            print(f"FIX 2: FAILED - could not find closing brace for should_process_subs block")
    else:
        print("FIX 2: FAILED - could not find if should_process_subs")
    
    # ═══════════════════════════════════════════════════════════════
    # FIX 3: Add files_repaired and files_re_encoded counters in normalization loop
    # ═══════════════════════════════════════════════════════════════
    # Find the normalization decision point where only_remux is determined
    # and add counter increments in each branch
    
    # Find "let only_remux = " in normalization loop
    marker3a = 'let only_remux = '
    idx3a = content.find(marker3a)
    if idx3a >= 0:
        # Find the normalization_type check - we want to increment files_re_encoded
        # in the else (re-encode) branch and files_repaired in the timescale remux branch
        
        # Find "Video Re-encode" log line to add files_re_encoded increment
        marker3b = 'Video Re-encode'
        idx3b = content.find(marker3b)
        if idx3b >= 0:
            # Find the line after this log line
            line_end = content.find('\n', idx3b)
            if line_end >= 0:
                next_line_start = line_end + 1
                # Check if files_re_encoded += is already there
                check_region = content[next_line_start:next_line_start+200]
                if 'files_re_encoded' not in check_region:
                    insert3 = '                            files_re_encoded += 1;\n'
                    content = content[:next_line_start] + insert3 + content[next_line_start:]
                    changes += 1
                    print("FIX 3a: Added files_re_encoded increment after 'Video Re-encode' log")
                else:
                    print("FIX 3a: SKIPPED - files_re_encoded already incremented")
        else:
            print("FIX 3a: FAILED - could not find 'Video Re-encode' log")
        
        # Find "Timescale Remux (Lossless)" log to add files_repaired increment
        marker3c = 'Timescale Remux (Lossless)'
        idx3c = content.find(marker3c)
        if idx3c >= 0:
            line_end = content.find('\n', idx3c)
            if line_end >= 0:
                next_line_start = line_end + 1
                check_region = content[next_line_start:next_line_start+200]
                if 'files_repaired' not in check_region:
                    insert3b = '                            files_repaired += 1;\n'
                    content = content[:next_line_start] + insert3b + content[next_line_start:]
                    changes += 1
                    print("FIX 3b: Added files_repaired increment after 'Timescale Remux (Lossless)' log")
                else:
                    print("FIX 3b: SKIPPED - files_repaired already incremented")
        else:
            print("FIX 3b: FAILED - could not find 'Timescale Remux (Lossless)' log")
    else:
        print("FIX 3: FAILED - could not find let only_remux")
    
    # ═══════════════════════════════════════════════════════════════
    # Write the file
    # ═══════════════════════════════════════════════════════════════
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)
    
    print(f"\nTotal changes: {changes}")
    
    # Verify key patterns exist
    with open(path, 'r', encoding='utf-8') as f:
        verify = f.read()
    
    checks = [
        ('files_damaged = media_report.quarantined_count', 'Counter: files_damaged from media_report'),
        ('files_healthy = working_input_files.len()', 'Counter: files_healthy from working_input_files'),
        ('files_re_encoded += 1', 'Counter: files_re_encoded increment'),
        ('files_repaired += 1', 'Counter: files_repaired increment'),
        ('SUBTITLE_PROCESSING | SKIPPED', 'Subtitle SKIPPED logging'),
    ]
    
    print("\nVerification:")
    for pattern, label in checks:
        found = pattern in verify
        print(f"  {'✅' if found else '❌'} {label}")

if __name__ == '__main__':
    main()
