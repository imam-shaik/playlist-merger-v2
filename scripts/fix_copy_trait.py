#!/usr/bin/env python3
"""Fix the Copy trait compilation error in merge.rs."""

with open('src-tauri/src/commands/merge.rs', 'r', encoding='utf-8') as f:
    content = f.read()

old = "final_segment_cards.get(seg_idx).copied().unwrap_or((false, None)).0"
new = "final_segment_cards.get(seg_idx).map(|c| c.0).unwrap_or(false)"

if old in content:
    content = content.replace(old, new, 1)
    print("[OK] Fixed Copy trait error")
else:
    print("[WARN] Pattern not found - checking for variants...")
    if ".copied().unwrap_or((false, None)).0" in content:
        print("Found .copied() pattern elsewhere in file")
    else:
        print("Pattern not present in file")

with open('src-tauri/src/commands/merge.rs', 'w', encoding='utf-8') as f:
    f.write(content)

print("[DONE] Fix applied")
