#!/usr/bin/env python3
"""Add mkvmerge_succeeded flag declaration and assignment."""
import sys
sys.stdout.reconfigure(encoding='utf-8')

filepath = 'src-tauri/src/commands/merge.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

# Change 1: Add let mut mkvmerge_succeeded = false before the dispatch
marker = '    // ── mkvmerge zero-copy dispatch (SmartMKV / FastMKV only) ──'
line_start = content.rfind('\n', 0, content.find(marker)) + 1
flag_decl = (
    '    // Track whether mkvmerge was used and succeeded (SmartMKV/FastMKV only)\n'
    '    let mut mkvmerge_succeeded = false;\n\n'
)
content = content[:line_start] + flag_decl + content[line_start:]
print('Change 1: Flag declaration added')

# Change 2: Add mkvmerge_succeeded = true in the Ok(Ok(())) branch
old_success = (
    '            Ok(Ok(())) => {\n'
    '                log::info!("[PERF] mkvmerge completed successfully: {}", normalized_output_path);\n'
    '            }'
)
new_success = (
    '            Ok(Ok(())) => {\n'
    '                log::info!("[PERF] mkvmerge completed successfully: {}", normalized_output_path);\n'
    '                mkvmerge_succeeded = true;\n'
    '            }'
)

if old_success in content:
    content = content.replace(old_success, new_success, 1)
    print('Change 2: mkvmerge_succeeded = true added in Ok(Ok(())) branch')
else:
    print('WARNING: Could not find Ok(Ok(())) branch pattern')
    # Try searching more broadly
    import re
    pattern = r'Ok\(Ok\(\(\)\)\) => \{\s*\n\s+log::info!\(.*mkvmerge completed successfully'
    match = re.search(pattern, content)
    if match:
        print(f'Found match at position {match.start()}: {match.group(0)[:80]}')
    sys.exit(1)

with open(filepath, 'w', encoding='utf-8') as f:
    f.write(content)

print('SUCCESS')
