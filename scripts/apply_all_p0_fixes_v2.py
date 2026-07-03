#!/usr/bin/env python3
"""Apply all remaining P0 fixes to merge.rs. Uses line-based manipulation."""

path = 'src-tauri/src/commands/merge.rs'

with open(path, 'rb') as f:
    lines = f.readlines()

# Strip CRLF for consistent processing
crlf = b'\r\n'
lines = [l.replace(b'\r\n', b'\n') for l in lines]

fixes = []

# ═══════════════════════════════════════════════════════════════════
# FIX 1: Move temp_norm_files_arc creation BEFORE MEDIA VALIDATION
# ═══════════════════════════════════════════════════════════════════
# Find the MEDIA_VALIDATION log line
for i, line in enumerate(lines):
    if b'[STAGE_TIMING] MEDIA_VALIDATION | start' in line:
        # Add Arc creation AFTER this line
        insert = [
            b'\n',
            b'    // -- P0-2: Initialize temp file tracking BEFORE media validation --\n',
            b'    let temp_norm_files_arc = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));\n',
            b'    let temp_registry = TempFileRegistry {\n',
            b'        sub: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),\n',
            b'    };\n',
        ]
        lines[i+1:i+1] = insert
        fixes.append(f"Fix 1: Added temp_norm_files_arc before MEDIA VALIDATION (line {i+2})")
        break

# ═══════════════════════════════════════════════════════════════════
# FIX 2: Remove duplicate temp_norm_files_arc declaration later
# ═══════════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if b'let temp_norm_files_arc = Arc::new(Mutex::new(Vec::new()));' in line:
        # This is the old declaration - replace it with just a comment
        lines[i] = b'    // temp_norm_files_arc moved before MEDIA VALIDATION ENGINE (see above)\n'
        fixes.append(f"Fix 2: Removed duplicate temp_norm_files_arc at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════════
# FIX 3: Add validation_fixed_paths declarations
# ═══════════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if b'validation_revalidation_duration_ms' in line:
        insert = [
            b'    // P0-4: Provenance tracking collections\n',
            b'    let mut validation_fixed_paths: std::collections::HashSet<String> = std::collections::HashSet::new();\n',
            b'    let mut validation_repair_methods: std::collections::HashMap<String, String> = std::collections::HashMap::new();\n',
        ]
        lines[i+1:i+1] = insert
        fixes.append(f"Fix 3: Added provenance collections at line {i+2}")
        break

# ═══════════════════════════════════════════════════════════════════
# FIX 4: Add provenance population code inside the if-block
# ═══════════════════════════════════════════════════════════════════
# Find the location after the _ => {} match and closing braces
# Search for the pattern: multiple closing braces followed by MEDIA_VALIDATION log
for i, line in enumerate(lines):
    if b'                    _ => {}' in line:
        # Found _ => {} - look forward for the }}} pattern + MEDIA_VALIDATION separator
        for j in range(i, min(i+20, len(lines))):
            if b'}[MEDIA_VALIDATION]' in lines[j].replace(b' ', b''):
                # This is a MEDIA_VALIDATION log that's inside the if block
                # Found the closing braces right before it
                break
        else:
            # Just use i+5 as the location (after 3 closing braces + blank line)
            pass
        # Find the MEDIA_VALIDATION separator with ══ chars after this
        for k in range(i, min(i+30, len(lines))):
            if b'log::info!("[MEDIA_VALIDATION] ' in lines[k]:
                # Check if this line has a closing brace before it (i.e., it's inside the if block)
                # Insert provenance code before this line
                insert = [
                    b'\n',
                    b'        // P0-4: Populate provenance tracking from media_report\n',
                    b'        for r in &media_report.file_results {\n',
                    b'            if r.is_fixed() {\n',
                    b'                if let Some(ref path) = r.repaired_path {\n',
                    b'                    validation_fixed_paths.insert(path.clone());\n',
                    b'                    if let Some(ref fix) = r.fix_applied {\n',
                    b'                        validation_repair_methods.insert(path.clone(), format!("{:?}", fix));\n',
                    b'                    }\n',
                    b'                }\n',
                    b'            }\n',
                    b'        }\n',
                ]
                lines[k:k] = insert
                fixes.append(f"Fix 4: Added provenance population at line {k+1}")
                break
        break

# ═══════════════════════════════════════════════════════════════════
# FIX 5: Change provenance log to use persistent collections + type fix
# ═══════════════════════════════════════════════════════════════════
for i, line in enumerate(lines):
    if b'if let Some(result) = media_report.file_results.iter()' in line:
        old_line = line
        new_line = (
            b'        // P0-4: Use pre-extracted provenance data instead of out-of-scope media_report\n'
            b'        let (source, method_str, revalidated_str) = if validation_fixed_paths.contains(fpath) {\n'
            b'            let method = validation_repair_methods.get(fpath)\n'
            b'                .cloned()\n'
            b'                .unwrap_or_default();\n'
            b'            ("REPAIRED", method, "PASS".to_string())\n'
            b'        } else {\n'
            b'            ("ORIGINAL", "none".to_string(), "N/A".to_string())\n'
            b'        };'
        )
        lines[i] = new_line
        # Also skip the next lines that were part of the old pattern
        # but we need to remove the .find() and .unwrap_or_default() lines too
        # Let me find where the old block ends (the }; line)
        for j in range(i+1, min(i+10, len(lines))):
            if b'"none".to_string()' in lines[j]:
                # This is part of the old pattern - blank it out since it's now in the new code above
                lines[j] = b'\n'
            elif b'        };' in lines[j]:
                break
        fixes.append(f"Fix 5: Updated provenance log at line {i+1}")
        break

# ═══════════════════════════════════════════════════════════════════
# FIX 6: Add repaired file registration and duration re-probing
# ═══════════════════════════════════════════════════════════════════
# Find: working_input_files = updated_files; after apply_validation_results
for i, line in enumerate(lines):
    if b'working_input_files = updated_files;' in line and b'pre_update_paths' not in line:
        # Add pre_update_paths tracking BEFORE this line
        indent = b'                '
        lines[i] = indent + b'let pre_update_paths: std::collections::HashSet<String> = working_input_files.iter().cloned().collect();\n' + lines[i]
        fixes.append(f"Fix 6a: Added pre_update_paths at line {i+1}")
        break

# Find working_total_duration = ... after the input_paths update
for i, line in enumerate(lines):
    if b'working_total_duration = working_input_durations.iter().sum();' in line:
        # Check if this is inside the media validation block (after apply_validation_results)
        # by looking backwards for apply_validation_results
        for j in range(max(0, i-50), i):
            if b'apply_validation_results' in lines[j]:
                # Add the repaired file registration loop AFTER this line
                insert = [
                    b'\n',
                    b'                // P0-2: Register repaired temp files with TempCleanup\n',
                    b'                for (new_path, _) in working_input_files.iter().zip(0..) {\n',
                    b'                    if !pre_update_paths.contains(new_path) {\n',
                    b'                        if let Ok(mut files) = temp_norm_files_arc.lock() {\n',
                    b'                            files.push(std::path::PathBuf::from(new_path));\n',
                    b'                        }\n',
                    b'                    }\n',
                    b'                }\n',
                ]
                lines[i+1:i+1] = insert
                fixes.append(f"Fix 6b: Added temp file registration at line {i+2}")
                break
        break

# Restore CRLF
lines = [l.replace(b'\n', b'\r\n') for l in lines]

with open(path, 'wb') as f:
    f.writelines(lines)

print("Fixes applied:")
for f in fixes:
    print(f"  {f}")
print(f"\nTotal: {len(fixes)} fixes")
