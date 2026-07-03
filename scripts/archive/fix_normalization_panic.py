#!/usr/bin/env python3
"""Fix the normalization.rs spawn_blocking type mismatch."""
import os

base = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'src-tauri', 'src')
path = os.path.join(base, 'ffmpeg', 'normalization.rs')

with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# Fix the spawn_blocking unwrap_or_else that returns None instead of FileHealth
old = """            }).await.unwrap_or_else(|e| {
                        log::error!(\"[Normalization] Corruption check task panicked: {}\", e);
                        None
                    });

            if !health.status.is_healthy()"""

new = """            }).await.unwrap_or_else(|e| {
                        log::error!(\"[Normalization] Corruption check task panicked for file #{}: {}\", idx, e);
                        FileHealth::unreadable(idx, path.clone(), format!(\"Corruption check task panicked: {}\", e))
                    });

            if !health.status.is_healthy()"""

if old in content:
    content = content.replace(old, new, 1)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)
    print("FIXED: spawn_blocking None -> FileHealth::unreadable")
else:
    print("SKIP: pattern not found (may have been fixed already)")
    # Try to find alternative patterns
    if "unwrap_or_else" in content:
        import re
        for m in re.finditer(r'unwrap_or_else\([^)]*\)', content):
            line_num = content[:m.start()].count('\n') + 1
            print(f"  Found unwrap_or_else at line {line_num}: {m.group()[:80]}")
