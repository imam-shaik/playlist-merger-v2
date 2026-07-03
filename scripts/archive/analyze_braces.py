#!/usr/bin/env python3
"""Analyze brace depth around the problematic area in merge.rs."""
with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Track brace depth per line from line 4470 to 4760
brace_depth = 0
for i in range(4469, min(4759, len(lines))):
    line = lines[i]
    openers = line.count('{')
    closers = line.count('}')
    new_depth = brace_depth + openers - closers
    stripped = line.rstrip()
    if '{' in stripped or '}' in stripped or 'METADATA' in stripped or 'spawn_blocking' in stripped or 'fn ' in stripped:
        print(f'{i+1:5d} (depth {brace_depth:2d}->{new_depth:2d}): {stripped[:120]}')
    brace_depth = new_depth

print(f'\nFinal brace depth at line 4760: {brace_depth}')
