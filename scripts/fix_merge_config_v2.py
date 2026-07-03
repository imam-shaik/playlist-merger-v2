#!/usr/bin/env python3
"""Fix the missing mkvmerge_succeeded_before_ffmpeg field in the MergeConfig initializer."""

path = 'src-tauri/src/commands/merge.rs'

with open(path, 'rb') as f:
    content = f.read()

# The initializer is a single line with all fields comma-separated
# Add mkvmerge_succeeded_before_ffmpeg: false, before the closing );

old = b'burn_subtitle_path: burn_subtitle_path.clone(),\n    };'
new = b'burn_subtitle_path: burn_subtitle_path.clone(),\n            mkvmerge_succeeded_before_ffmpeg: false,\n    };'

if old in content:
    content = content.replace(old, new, 1)
    print("Fix: Added mkvmerge_succeeded_before_ffmpeg field")
else:
    print("Pattern with \\n not found, trying with \\r\\n")
    old_crlf = b'burn_subtitle_path: burn_subtitle_path.clone(),\r\n    };'
    new_crlf = b'burn_subtitle_path: burn_subtitle_path.clone(),\r\n            mkvmerge_succeeded_before_ffmpeg: false,\r\n    };'
    if old_crlf in content:
        content = content.replace(old_crlf, new_crlf, 1)
        print("Fix: Added mkvmerge_succeeded_before_ffmpeg field (CRLF)")
    else:
        # Single-line format - no newlines
        old_single = b'burn_subtitle_path: burn_subtitle_path.clone(),\n    };'
        print("Pattern not found")
        # Debug: find the context
        idx = content.find(b'burn_subtitle_path:')
        if idx >= 0:
            snippet = content[idx:idx+60]
            print(f"Found at {idx}: {repr(snippet)}")

with open(path, 'wb') as f:
    f.write(content)

print("Done")
