#!/usr/bin/env python3
"""Find specific patterns in merge.rs for editing."""

path = 'src-tauri/src/commands/merge.rs'
with open(path, 'rb') as f:
    content = f.read()

# Find the MergeConfig initializer (with clone() patterns)
idx = content.find(b'card_config: request.card_config.clone()')
if idx >= 0:
    snippet = content[idx:idx+200]
    print("MergeConfig initializer found!")
    print(repr(snippet))
else:
    print("MergeConfig initializer NOT found at expected pattern")
    # Try other patterns
    for pat in [b'card_config:', b'burn_subtitle_path:', b'mkvmerge_succeeded']:
        idx = content.find(pat)
        if idx >= 0:
            snippet = content[max(0,idx-50):idx+120]
            print(f"'{pat.decode()}' at {idx}: {repr(snippet)}")

# Find the MEDIA_VALIDATION | start log
idx = content.find(b'[STAGE_TIMING] MEDIA_VALIDATION')
if idx >= 0:
    bol = content.rfind(b'\\n', 0, idx)
    eol = content.find(b'\\n', idx)
    ln = content[bol:eol+1]
    print(f"\\nMEDIA_VALIDATION at {idx}:")
    print(repr(ln))
    next_line_end = content.find(b'\\n', eol+1)
    next_line = content[eol+1:next_line_end]
    print(f"Next line: {repr(next_line)}")
    next_line_end2 = content.find(b'\\n', next_line_end+1)
    next_line2 = content[next_line_end+1:next_line_end2]
    print(f"Line 2 after: {repr(next_line2)}")

# Find validation_revalidation_duration_ms
idx = content.find(b'validation_revalidation_duration_ms')
if idx >= 0:
    bol = content.rfind(b'\\n', 0, idx)
    eol = content.find(b'\\n', idx)
    snippet = content[bol:eol+200]
    print(f"\\nvalidation_revalidation_duration_ms at {idx}:")
    print(repr(snippet))

# Find MERGE_INPUT_PROVENANCE 
idx = content.find(b'MERGE_INPUT_PROVENANCE')
if idx >= 0:
    print(f"\\nMERGE_INPUT_PROVENANCE at {idx}")
    # Find the if let Some(result) = media_report part
    rest = content[idx:idx+800]
    print(repr(rest))

# Find all $ => {} patterns
import re
for m in re.finditer(b'                    _ => {}', content):
    print(f"\\n'_ => {{}}' at {m.start()}")
    ctx = content[m.start():m.start()+300]
    print(repr(ctx[:150]))
