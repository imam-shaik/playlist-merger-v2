#!/usr/bin/env python3
"""Insert closing brace for impl MediaValidationEngine block."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Find the end of repair_reencode_only
for i, line in enumerate(lines):
    if 'fn repair_reencode_only' in line and i > 7500:
        depth = 0
        found = False
        for j in range(i, len(lines)):
            for ch in lines[j]:
                if ch == '{': depth += 1; found = True
                elif ch == '}': depth -= 1
            if found and depth == 0:
                # j is the line with the closing brace of repair_reencode_only
                print(f'repair_reencode_only ends at line {j+1}')
                # Check if next line already has the impl closing brace
                if j + 1 < len(lines) and lines[j + 1].strip() == '}':
                    print('[SKIP] Impl closing brace already exists')
                else:
                    # Insert closing brace after this line
                    lines.insert(j + 1, '}\n')
                    print(f'[FIXED] Inserted closing brace at line {j+2}')
                break
        break

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f'Total lines: {len(lines)}')
print('[DONE]')
