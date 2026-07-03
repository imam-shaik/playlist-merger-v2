#!/usr/bin/env python3
"""Fix compilation errors in analyze_single method."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# Fix 1: Replace IssueSeverity::Critical with string comparison
old1 = 'crate::ffmpeg::media_validation_engine::IssueSeverity::Critical'
new1 = '"Critical"'
if old1 in content:
    content = content.replace(old1, new1)
    print(f'[FIXED] Replaced IssueSeverity::Critical with "Critical" string comparison')
else:
    print(f'[SKIP] IssueSeverity::Critical not found')

# Fix 2: Replace tuple destructuring with single variable
old2 = 'let (video_audio_streams, subtitle_streams) = self.get_stream_codec_types(file_path);'
new2 = 'let video_audio_streams = self.get_stream_codec_types(file_path);'
if old2 in content:
    content = content.replace(old2, new2)
    print(f'[FIXED] Removed tuple destructuring from get_stream_codec_types call')
else:
    print(f'[SKIP] Tuple destructuring not found')

# Also check if subtitle_streams is used anywhere after this point
if 'subtitle_streams' in content:
    # Find usages - they should reference video_audio_streams instead
    # Count occurrences
    import re
    count = len(re.findall(r'subtitle_streams', content))
    print(f'[INFO] subtitle_streams still referenced {count} times - may need further fixes')

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print('[DONE] Fixes applied')
