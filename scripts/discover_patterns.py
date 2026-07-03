#!/usr/bin/env python3
"""Discover exact byte patterns in merge.rs for targeted editing."""
import sys

path = 'src-tauri/src/commands/merge.rs'
with open(path, 'rb') as f:
    content = f.read()

print(f"File size: {len(content)} bytes")
print(f"Has CRLF: {b'\\r\\n' in content}")
print()

# Find MergeConfig initializer
idx = content.find(b'burn_subtitle_path:')
if idx >= 0:
    snippet = content[idx:idx+120]
    print(f"MergeConfig initializer (at offset {idx}):")
    print(repr(snippet))
    print()

# Find MEDIA_VALIDATION log
idx = content.find(b'STAGE_TIMING] MEDIA_VALIDATION | start')
if idx >= 0:
    # Get from the start of this line
    bol = content.rfind(b'\\n', 0, idx) + 1
    eol = content.find(b'\\n', idx)
    snippet = content[bol:eol+120]
    print(f"MEDIA_VALIDATION | start (at offset {idx}):")
    print(repr(snippet))
    print()

# Find validation_revalidation_duration_ms
idx = content.find(b'validation_revalidation_duration_ms')
if idx >= 0:
    bol = content.rfind(b'\\n', 0, idx)
    eol = content.find(b'\\n', idx)
    snippet = content[bol:eol+200]
    print(f"validation_revalidation_duration_ms (at offset {idx}):")
    print(repr(snippet))
    print()

# Find MERGE_INPUT_PROVENANCE
idx = content.find(b'MERGE_INPUT_PROVENANCE')
if idx >= 0:
    eol = content.find(b'\\n', idx)
    snippet = content[idx-200:idx+400]
    print(f"MERGE_INPUT_PROVENANCE (at offset {idx}):")
    print(repr(snippet))
    print()

# Find the for loop with damage classifications (the \x5f => {} pattern)
import re
for m in re.finditer(b'                    _ => {}', content):
    print(f"Found '_ => {{}}' at offset {m.start()}")
    # Show context around this
    ctx = content[m.start():m.start()+500]
    print(f"  Context: {repr(ctx[:200])}")
    print()
