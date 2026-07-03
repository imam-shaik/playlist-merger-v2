#!/usr/bin/env python3
"""Fix the temp_norm_files_arc compilation error by restoring the declaration."""

path = 'src-tauri/src/commands/merge.rs'

with open(path, 'rb') as f:
    lines = f.readlines()

# Find the line that references temp_norm_files_arc without declaring it
# Look for the line that says 'moved before MEDIA VALIDATION ENGINE'
for i, line in enumerate(lines):
    if b'moved before MEDIA VALIDATION ENGINE' in line:
        # Replace with the actual declaration
        lines[i] = b'    let temp_norm_files_arc = Arc::new(Mutex::new(Vec::new()));\n'
        print(f"Restored temp_norm_files_arc declaration at line {i+1}")
        break
else:
    print("Fix not needed - declaration not found")

with open(path, 'wb') as f:
    f.writelines(lines)

print("Done")
