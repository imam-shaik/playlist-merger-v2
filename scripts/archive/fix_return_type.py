#!/usr/bin/env python3
"""Fix the tokio::spawn closure return type mismatch in normalization.rs."""
import os

base = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'src-tauri', 'src')
path = os.path.join(base, 'ffmpeg', 'normalization.rs')

with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

old = 'return None;'
new = 'return;'

# Only replace the specific instance inside the semaphore match (not other return None patterns)
count = content.count(old)
print(f"Found {count} occurrences of 'return None;'")

# Check context around the first occurrence
idx = content.find(old)
if idx >= 0:
    context = content[idx-100:idx+100]
    if 'sem.acquire' in context or 'Semaphore closed' in context:
        content = content.replace(old, new, 1)
        with open(path, 'w', encoding='utf-8') as f:
            f.write(content)
        print("FIXED: return None -> return; in semaphore error handler")
    else:
        print("SKIP: found 'return None;' but not in semaphore context")
else:
    print("SKIP: 'return None;' not found")
