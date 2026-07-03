#!/usr/bin/env python3
"""Fix the missing mkvmerge_succeeded_before_ffmpeg field in MergeConfig initializer."""

path = 'src-tauri/src/commands/merge.rs'

with open(path, 'rb') as f:
    lines = f.readlines()

# Fix 1: Add mkvmerge_succeeded_before_ffmpeg field to MergeConfig initializer
# Find the initializer that has card_config: request.card_config.clone()
for i, line in enumerate(lines):
    if b'card_config: request.card_config.clone()' in line:
        # Check the next line for burn_subtitle_path
        if i+1 < len(lines) and b'burn_subtitle_path:' in lines[i+1]:
            # The next line should be the closing }
            if i+2 < len(lines) and b'};\n' in lines[i+3] or b'        }\n' in lines[i+2]:
                # Add the field after burn_subtitle_path
                indent = b'            '
                lines.insert(i+2, indent + b'mkvmerge_succeeded_before_ffmpeg: false,\n')
                print(f"Fix 1: Added field at line {i+2}")
            else:
                print(f"Fix 1: Line {i+2} is {repr(lines[i+2])}")
                print(f"  Line {i+3} is {repr(lines[i+3])}")
        else:
            print(f"Fix 1: Line after card_config is {repr(lines[i+1])}")
        break
else:
    print("Fix 1: Pattern not found")

# Write back
with open(path, 'wb') as f:
    f.writelines(lines)

print("Done")
