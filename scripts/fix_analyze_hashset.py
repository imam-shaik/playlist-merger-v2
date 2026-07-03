#!/usr/bin/env python3
"""Fix analyze_single to use HashSet types for stream checks."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# Fix: Replace the single get_stream_codec_types call with proper HashSet conversions
old = """        let video_audio_streams = self.get_stream_codec_types(file_path);

        // Timing & structural checks
        let pts_issues = self.check_pts_monotonic(file_path, &video_audio_streams);
        let dts_issues = self.check_dts_validity(file_path, &video_audio_streams);
        let timebase_issues = self.check_timebase_consistency(file_path);
        let vfr_instability = self.check_vfr_instability(file_path, &video_audio_streams);
        let deep_container_issues = self.check_container_corruption(file_path);
        let subtitle_issues = self.check_subtitle_validity(file_path, &subtitle_streams);"""

new = """        let stream_types = self.get_stream_codec_types(file_path);
        let video_audio_streams: std::collections::HashSet<usize> = stream_types.iter()
            .filter(|(_, ct)| *ct == "video" || *ct == "audio")
            .map(|(si, _)| *si)
            .collect();
        let subtitle_streams: std::collections::HashSet<usize> = stream_types.iter()
            .filter(|(_, ct)| *ct == "subtitle")
            .map(|(si, _)| *si)
            .collect();

        // Timing & structural checks
        let pts_issues = self.check_pts_monotonic(file_path, &video_audio_streams);
        let dts_issues = self.check_dts_validity(file_path, &video_audio_streams);
        let timebase_issues = self.check_timebase_consistency(file_path);
        let vfr_instability = self.check_vfr_instability(file_path, &video_audio_streams);
        let deep_container_issues = self.check_container_corruption(file_path);
        let subtitle_issues = self.check_subtitle_validity(file_path, &subtitle_streams);"""

if old in content:
    content = content.replace(old, new)
    print('[FIXED] Replaced get_stream_codec_types with proper HashSet conversions')
else:
    print('[ERROR] Could not find the old code block to replace')

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print('[DONE]')
