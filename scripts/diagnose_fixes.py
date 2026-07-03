#!/usr/bin/env python3
"""Diagnose why each fix in apply_all_p0_fixes_v2.py failed."""

path = 'src-tauri/src/commands/merge.rs'
with open(path, 'rb') as f:
    content = f.read()

# Normalize for searching
has_crlf = b'\r\n' in content
if has_crlf:
    content_lines = content.replace(b'\r\n', b'\n').split(b'\n')
else:
    content_lines = content.split(b'\n')

# Fix 1: Check for MEDIA_VALIDATION start log
print("=== Fix 1: MEDIA_VALIDATION | start ===")
for i, line in enumerate(content_lines):
    if b'MEDIA_VALIDATION | start' in line:
        print(f"  Found at line {i+1}: {repr(line[:80])}")
        # Show next 3 lines
        for j in range(1, 5):
            if i+j < len(content_lines):
                print(f"  Line {i+j+1}: {repr(content_lines[i+j][:100])}")
        break
else:
    print("  NOT FOUND")

# Fix 3: Check for validation_revalidation_duration_ms
print("\n=== Fix 3: validation counters area ===")
for i, line in enumerate(content_lines):
    if b'validation_revalidation_duration_ms' in line:
        print(f"  Found at line {i+1}")
        # Show previous line and this line
        if i > 0:
            print(f"  Prev: {repr(content_lines[i-1][:100])}")
        print(f"  Line: {repr(line[:100])}")
        print(f"  Next: {repr(content_lines[i+1][:100])}" if i+1 < len(content_lines) else "  END")
        break

# Fix 4: Check for _ => {} pattern
print("\n=== Fix 4: _ => {} pattern ===")
for i, line in enumerate(content_lines):
    if b'                    _ => {}' in line:
        print(f"  Found at line {i+1}")
        # Show next 20 lines
        for j in range(1, 25):
            if i+j < len(content_lines):
                ln = content_lines[i+j]
                if b'MEDIA_VALIDATION] ' in ln:
                    print(f"  Line {i+j+1} (MEDIA_VALIDATION): {repr(ln[:80])}")
                    break
        break

# Fix 5: Check MERGE_INPUT_PROVENANCE
print("\n=== Fix 5: MERGE_INPUT_PROVENANCE ===")
for i, line in enumerate(content_lines):
    if b'MERGE_INPUT_PROVENANCE' in line:
        print(f"  Found at line {i+1}")
        # Show next 15 lines
        for j in range(1, 20):
            if i+j < len(content_lines):
                ln = content_lines[i+j]
                if b'media_report.file_results' in ln or b'validation_fixed_paths' in ln or b'repaired' in ln.lower():
                    print(f"  Line {i+j+1}: {repr(ln[:120])}")
        break

# Fix 6: Check working_input_files = updated_files;
print("\n=== Fix 6: working_input_files after apply_validation_results ===")
for i, line in enumerate(content_lines):
    if b'working_input_files = updated_files;' in line:
        print(f"  Found at line {i+1}")
        print(f"  Line: {repr(line[:100])}")
        # Previous line
        if i > 0:
            print(f"  Prev: {repr(content_lines[i-1][:100])}")
        break

# Check working_total_duration after validation
print("\n=== working_total_duration in validation block ===")
for i, line in enumerate(content_lines):
    if b'working_total_duration = working_input_durations.iter().sum();' in line:
        # Check if this is near apply_validation_results
        for j in range(max(0, i-10), i):
            if b'apply_validation_results' in content_lines[j]:
                print(f"  Found at line {i+1} (after apply_validation_results at line {j+1})")
                print(f"  Context: {repr(content_lines[i][:100])}")
                break
