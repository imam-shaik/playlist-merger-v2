#!/usr/bin/env python3
"""Fix the borrow-of-moved-value error in normalization.rs."""
import os

base = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'src-tauri', 'src')
path = os.path.join(base, 'ffmpeg', 'normalization.rs')

with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# The issue: path is moved into spawn_blocking closure, but unwrap_or_else also needs it.
# Fix: clone path before spawn_blocking so the error handler can use the clone.

old = """            let health = tokio::task::spawn_blocking(move || {
                check_file_corruption_quick(&ffprobe, idx, &path)
            }).await.unwrap_or_else(|e| {
                        log::error!(\"[Normalization] Corruption check task panicked for file #{}: {}\", idx, e);
                        FileHealth::unreadable(idx, path.clone(), format!(\"Corruption check task panicked: {}\", e))
                    });"""

new = """            let path_for_err = path.clone();
            let health = tokio::task::spawn_blocking(move || {
                check_file_corruption_quick(&ffprobe, idx, &path)
            }).await.unwrap_or_else(|e| {
                        log::error!(\"[Normalization] Corruption check task panicked for file #{}: {}\", idx, e);
                        FileHealth::unreadable(idx, path_for_err, format!(\"Corruption check task panicked: {}\", e))
                    });"""

if old in content:
    content = content.replace(old, new, 1)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)
    print("FIXED: path ownership issue - cloned path before spawn_blocking")
else:
    print("SKIP: pattern not found")
    # Debug: show context around spawn_blocking
    import re
    for m in re.finditer(r'spawn_blocking.*unwrap_or_else', content, re.DOTALL):
        start = max(0, m.start()-200)
        end = min(len(content), m.end()+200)
        print(f"Found at position {m.start()}:")
        print(repr(content[start:end]))
