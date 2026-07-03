#!/usr/bin/env python3
"""Fix verify_normalized_audio_health calls missing the input_video_duration_ms argument."""
import re

FILE = "src-tauri/src/commands/merge.rs"

with open(FILE, "r", encoding="utf-8") as f:
    content = f.read()

# Pattern: verify_normalized_audio_health(&ff, &fp, &path, idx, &fname, c.clone(),\n                                         skip_vol)
# Should add: , input_video_duration_ms
# The variable input_video_duration_ms is already defined earlier in both scopes.

old_call1 = (
    "let vr = verify_normalized_audio_health(&ff, &fp, &path, idx, &fname, c.clone(),\n"
    "                                        skip_vol).await;"
)

new_call1 = (
    "let vr = verify_normalized_audio_health(&ff, &fp, &path, idx, &fname, c.clone(),\n"
    "                                        skip_vol, input_video_duration_ms).await;"
)

count1 = content.count(old_call1)
content = content.replace(old_call1, new_call1)
print(f"Fixed {count1} verify_normalized_audio_health call(s) with missing input_video_duration_ms argument")

with open(FILE, "w", encoding="utf-8") as f:
    f.write(content)

print(f"File written: {len(content)} chars")
