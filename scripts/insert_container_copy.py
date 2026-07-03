#!/usr/bin/env python3
"""Insert files_container_copy increment after files_re_encoded block."""
path = 'src-tauri/src/commands/merge.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Find "files_re_encoded += 1;" and insert after its closing }
for i, line in enumerate(lines):
    if 'files_re_encoded += 1;' in line:
        # Find the next } after this line
        for j in range(i, min(i + 5, len(lines))):
            if lines[j].strip() == '}':
                insert = '                    if only_remux && dm.timescale_den.is_none() {\n                        files_container_copy += 1;\n                    }\n'
                lines.insert(j + 1, insert)
                with open(path, 'w', encoding='utf-8') as f:
                    f.writelines(lines)
                print(f'Inserted 3 lines after line {j+1}')
                break
        break
