#!/usr/bin/env python3
"""Replace the write_report_file function body in merge.rs to use the new report module."""

import re

filepath = 'src-tauri/src/commands/merge.rs'

with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

# The file uses \r\n line endings, so let's normalize
has_crlf = '\r\n' in content
print(f"File has CRLF: {has_crlf}")

# Try multiple patterns for the function start
old_start_patterns = [
    # CRLF version
    'pub fn write_report_file(\r\n    output_path: &str,\r\n    segments: &[MergeSegment],\r\n    output_size_bytes: u64,\r\n    total_duration: f64,\r\n) -> Option<String> {',
    # LF version
    'pub fn write_report_file(\n    output_path: &str,\n    segments: &[MergeSegment],\n    output_size_bytes: u64,\n    total_duration: f64,\n) -> Option<String> {',
]

old_start = None
for pattern in old_start_patterns:
    idx = content.find(pattern)
    if idx != -1:
        old_start = (pattern, idx)
        print(f"Found with pattern ending: {repr(pattern[-20:])} at position {idx}")
        break

if old_start is None:
    print("ERROR: Could not find function start with any pattern")
    # Try a more relaxed regex search
    match = re.search(r'pub fn write_report_file\([^)]+\) -> Option<String> \{', content)
    if match:
        print(f"Regex found at position {match.start()}: {match.group()[:80]}")
    else:
        print("Even regex didn't find it")
    exit(1)

pattern, start_idx = old_start

# Find the opening brace of the function
brace_idx = content.find('{', start_idx)
if brace_idx == -1:
    print("ERROR: Could not find opening brace")
    exit(1)

# Find the matching close brace by tracking depth
i = brace_idx
depth = 1
while depth > 0 and i < len(content) - 1:
    i += 1
    if content[i] == '{':
        depth += 1
    elif content[i] == '}':
        depth -= 1

if depth != 0:
    print("ERROR: Unmatched braces")
    exit(1)

func_end = i + 1  # include the closing brace

# Extract the old function
old_func = content[start_idx:func_end]

# Check for old code patterns
has_ascii_box = '╔═' in old_func
has_fmt_write = 'use std::fmt::Write as FmtWrite' in old_func or 'use std::fmt::Write as FmtWrite;\r' in old_func
print(f"Function from {start_idx} to {func_end} ({func_end - start_idx} chars)")
print(f"Has ASCII art: {has_ascii_box}")
print(f"Has FmtWrite: {has_fmt_write}")

if (has_ascii_box or has_fmt_write):
    # Build the new function with the same line ending style
    nl = '\r\n' if has_crlf else '\n'
    indent = '    '
    
    new_func = f'pub fn write_report_file({nl}'
    new_func += f'{indent}output_path: &str,{nl}'
    new_func += f'{indent}segments: &[MergeSegment],{nl}'
    new_func += f'{indent}output_size_bytes: u64,{nl}'
    new_func += f'{indent}total_duration: f64,{nl}'
    new_func += f') -> Option<String> {{{nl}'
    new_func += f'{indent}use crate::report::{{build_report_data, render_txt_report}};{nl}'
    new_func += f'{indent}let report_data = build_report_data({nl}'
    new_func += f'{indent}    segments,{nl}'
    new_func += f'{indent}    None::<&[MergePartResult]>,{nl}'
    new_func += f'{indent}    output_path,{nl}'
    new_func += f'{indent}    output_size_bytes,{nl}'
    new_func += f'{indent}    total_duration,{nl}'
    new_func += f'{indent}    None,{nl}'
    new_func += f'{indent}    None,{nl}'
    new_func += f'{indent}    None,{nl}'
    new_func += f'{indent});{nl}'
    new_func += f'{indent}let output_path_s = output_path.to_string();{nl}'
    new_func += f'{indent}render_txt_report(&report_data, &output_path_s){nl}'
    new_func += f'}}'
    
    new_content = content[:start_idx] + new_func + content[func_end:]
    
    with open(filepath, 'w', encoding='utf-8', newline='') as f:
        f.write(new_content)
    
    print(f"SUCCESS: Replaced write_report_file. New file size: {len(new_content)} chars")
else:
    print("Function appears to already be updated. Skipping.")
    print(f"First 200 chars: {old_func[:200]}")
