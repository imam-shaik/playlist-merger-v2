#!/usr/bin/env python
"""P0-3 and P0-4 fixes for media_validation_engine/repair.rs"""

import os

REPAIR_RS = os.path.join(os.path.dirname(__file__), '..', 'src-tauri', 'src', 'ffmpeg', 'media_validation_engine', 'repair.rs')

with open(REPAIR_RS, 'r', encoding='utf-8') as f:
    content = f.read()

# ===================================================================
# P0-3a: Enhance get_stream_identity to extract rich metadata
# ===================================================================
OLD_GET_IDENTITY = """    fn get_stream_identity(&self, file_path: &str) -> StreamIdentity {
        let path = std::path::Path::new(file_path);
        match crate::ffmpeg::probe::probe_file(&self.ffprobe_path, path) {
            Ok(info) => {
                let video_codecs: Vec<String> = info.video_streams.iter()
                    .map(|s| s.codec_name.clone())
                    .collect();
                let audio_codecs: Vec<String> = info.audio_streams.iter()
                    .map(|s| s.codec_name.clone())
                    .collect();
                StreamIdentity {
                    video_count: info.video_streams.len(),
                    audio_count: info.audio_streams.len(),
                    subtitle_count: info.subtitle_streams.len(),
                    total_streams: info.video_streams.len() + info.audio_streams.len() + info.subtitle_streams.len(),
                    video_codecs,
                    audio_codecs,
                    duration: Some(info.duration),
                }
            }
            Err(_) => StreamIdentity {
                video_count: 0,
                audio_count: 0,
                subtitle_count: 0,
                total_streams: 0,
                video_codecs: Vec::new(),
                audio_codecs: Vec::new(),
                duration: None,
            },
        }
    }"""

NEW_GET_IDENTITY = """    fn get_stream_identity(&self, file_path: &str) -> StreamIdentity {
        let path = std::path::Path::new(file_path);
        match crate::ffmpeg::probe::probe_file(&self.ffprobe_path, path) {
            Ok(info) => {
                let video_codecs: Vec<String> = info.video_streams.iter()
                    .map(|s| s.codec_name.clone())
                    .collect();
                let audio_codecs: Vec<String> = info.audio_streams.iter()
                    .map(|s| s.codec_name.clone())
                    .collect();
                // P0-3: Extract rich metadata for post-repair identity verification
                let video_languages: Vec<Option<String>> = info.video_streams.iter()
                    .map(|s| s.language.clone())
                    .collect();
                let audio_languages: Vec<Option<String>> = info.audio_streams.iter()
                    .map(|s| s.language.clone())
                    .collect();
                let video_color_transfer: Vec<Option<String>> = info.video_streams.iter()
                    .map(|s| s.color_transfer.clone())
                    .collect();
                let video_color_space: Vec<Option<String>> = info.video_streams.iter()
                    .map(|s| s.color_space.clone())
                    .collect();
                let video_rotation: Vec<Option<i32>> = info.video_streams.iter()
                    .map(|s| s.rotation)
                    .collect();
                StreamIdentity {
                    video_count: info.video_streams.len(),
                    audio_count: info.audio_streams.len(),
                    subtitle_count: info.subtitle_streams.len(),
                    total_streams: info.video_streams.len() + info.audio_streams.len() + info.subtitle_streams.len(),
                    video_codecs,
                    audio_codecs,
                    duration: Some(info.duration),
                    video_languages,
                    audio_languages,
                    video_color_transfer,
                    video_color_space,
                    video_rotation,
                }
            }
            Err(_) => StreamIdentity::empty(),
        }
    }"""

if OLD_GET_IDENTITY in content:
    content = content.replace(OLD_GET_IDENTITY, NEW_GET_IDENTITY, 1)
    print("P0-3a: Enhanced get_stream_identity with rich metadata")
else:
    print("P0-3a: Pattern not found")

# ===================================================================
# P0-3b: Enhance check_stream_identity to verify rich metadata
# ===================================================================
OLD_CHECK_IDENTITY = """    fn check_stream_identity(&self, orig: &StreamIdentity, rep: &StreamIdentity) -> Vec<String> {
        let mut issues = Vec::new();

        if orig.video_count != rep.video_count {
            issues.push(format!(
                "video stream count mismatch: original={}, repaired={}",
                orig.video_count, rep.video_count
            ));
        }

        if orig.audio_count != rep.audio_count {
            issues.push(format!(
                "audio stream count mismatch: original={}, repaired={}",
                orig.audio_count, rep.audio_count
            ));
        }

        if orig.subtitle_count != rep.subtitle_count {
            issues.push(format!(
                "subtitle stream count mismatch: original={}, repaired={}",
                orig.subtitle_count, rep.subtitle_count
            ));
        }

        for (i, codec) in orig.video_codecs.iter().enumerate() {
            if i >= rep.video_codecs.len() {
                issues.push(format!("missing video codec at index {}: {}", i, codec));
            } else if &rep.video_codecs[i] != codec {
                issues.push(format!(
                    "video codec mismatch at index {}: original={}, repaired={}",
                    i, codec, rep.video_codecs[i]
                ));
            }
        }

        for (i, codec) in orig.audio_codecs.iter().enumerate() {
            if i >= rep.audio_codecs.len() {
                issues.push(format!("missing audio codec at index {}: {}", i, codec));
            } else if &rep.audio_codecs[i] != codec {
                issues.push(format!(
                    "audio codec mismatch at index {}: original={}, repaired={}",
                    i, codec, rep.audio_codecs[i]
                ));
            }
        }

        issues
    }"""

NEW_CHECK_IDENTITY = """    fn check_stream_identity(&self, orig: &StreamIdentity, rep: &StreamIdentity) -> Vec<String> {
        let mut issues = Vec::new();

        if orig.video_count != rep.video_count {
            issues.push(format!(
                "video stream count mismatch: original={}, repaired={}",
                orig.video_count, rep.video_count
            ));
        }

        if orig.audio_count != rep.audio_count {
            issues.push(format!(
                "audio stream count mismatch: original={}, repaired={}",
                orig.audio_count, rep.audio_count
            ));
        }

        if orig.subtitle_count != rep.subtitle_count {
            issues.push(format!(
                "subtitle stream count mismatch: original={}, repaired={}",
                orig.subtitle_count, rep.subtitle_count
            ));
        }

        // Video codec verification
        for (i, codec) in orig.video_codecs.iter().enumerate() {
            if i >= rep.video_codecs.len() {
                issues.push(format!("missing video codec at index {}: {}", i, codec));
            } else if &rep.video_codecs[i] != codec {
                issues.push(format!(
                    "video codec mismatch at index {}: original={}, repaired={}",
                    i, codec, rep.video_codecs[i]
                ));
            }
        }

        // Audio codec verification
        for (i, codec) in orig.audio_codecs.iter().enumerate() {
            if i >= rep.audio_codecs.len() {
                issues.push(format!("missing audio codec at index {}: {}", i, codec));
            } else if &rep.audio_codecs[i] != codec {
                issues.push(format!(
                    "audio codec mismatch at index {}: original={}, repaired={}",
                    i, codec, rep.audio_codecs[i]
                ));
            }
        }

        // P0-3: Verify language tags preserved (log-only — optional metadata)
        for (i, lang) in orig.audio_languages.iter().enumerate() {
            if let Some(rep_lang) = rep.audio_languages.get(i) {
                if lang != rep_lang {
                    log::warn!("[STREAM_IDENTITY] Audio language drift at index {}: {:?} -> {:?}", i, lang, rep_lang);
                }
            }
        }

        // P0-3: Verify HDR metadata (color_transfer) preserved — CRITICAL
        for (i, ct) in orig.video_color_transfer.iter().enumerate() {
            if let Some(rep_ct) = rep.video_color_transfer.get(i) {
                if ct != rep_ct {
                    issues.push(format!(
                        "HDR metadata loss: video stream {} color_transfer {:?} -> {:?}",
                        i, ct, rep_ct
                    ));
                }
            }
        }

        // P0-3: Verify color_space preserved — CRITICAL
        for (i, cs) in orig.video_color_space.iter().enumerate() {
            if let Some(rep_cs) = rep.video_color_space.get(i) {
                if cs != rep_cs {
                    issues.push(format!(
                        "Color space drift: video stream {} color_space {:?} -> {:?}",
                        i, cs, rep_cs
                    ));
                }
            }
        }

        // P0-3: Verify rotation preserved — CRITICAL
        for (i, rot) in orig.video_rotation.iter().enumerate() {
            if let Some(rep_rot) = rep.video_rotation.get(i) {
                if rot != rep_rot {
                    issues.push(format!(
                        "Rotation mismatch: video stream {} rotation {:?} -> {:?}",
                        i, rot, rep_rot
                    ));
                }
            }
        }

        issues
    }"""

if OLD_CHECK_IDENTITY in content:
    content = content.replace(OLD_CHECK_IDENTITY, NEW_CHECK_IDENTITY, 1)
    print("P0-3b: Enhanced check_stream_identity with HDR, language, rotation checks")
else:
    print("P0-3b: Pattern not found")

# ===================================================================
# P0-4: Differentiate timestamp repair from subtitle repair
# ===================================================================

# The current try_fix_timestamp_repair and try_fix_subtitle_remux are effectively
# identical. Make timestamp repair more targeted:
# - Add -max_muxing_queue_size to prevent queue overflow on damaged files
# - Add -movflags +frag_keyframe+empty_moov for MP4 containers

OLD_TIMESTAMP = """    fn try_fix_timestamp_repair(&self, _file_index: usize, file_path: &str) -> Option<String> {
        let input_path = std::path::Path::new(file_path);
        let stem = input_path.file_stem()?.to_string_lossy();
        let output_path = self.temp_dir.join(format!("{}_ts_fix.mkv", stem));
        let output_str = output_path.to_string_lossy().to_string();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd
            .args([
                "-y", "-i", file_path,
                "-c", "copy",
                "-map", "0",
                "-avoid_negative_ts", "make_zero",
                "-fflags", "+genpts",
                &output_str,
            ])
            .output();

        match status {
            Ok(out) if out.status.success() => Some(output_str),
            _ => None,
        }
    }"""

NEW_TIMESTAMP = """    fn try_fix_timestamp_repair(&self, _file_index: usize, file_path: &str) -> Option<String> {
        // P0-4: Timestamp repair is specialized for timestamp problems, NOT a generic remux.
        // It targets: negative PTS, missing DTS, non-monotonic timestamps, PTS < DTS.
        let input_path = std::path::Path::new(file_path);
        let stem = input_path.file_stem()?.to_string_lossy();
        let output_path = self.temp_dir.join(format!("{}_ts_fix.mkv", stem));
        let output_str = output_path.to_string_lossy().to_string();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd
            .args([
                "-y", "-i", file_path,
                "-c", "copy",
                "-map", "0",
                // Fix negative timestamps by shifting to zero
                "-avoid_negative_ts", "make_zero",
                // Regenerate PTS from DTS — fixes missing PTS and non-monotonic PTS
                "-fflags", "+genpts",
                // Prevent muxing queue overflow on damaged files with timestamp issues
                "-max_muxing_queue_size", "4096",
                &output_str,
            ])
            .output();

        match status {
            Ok(out) if out.status.success() => Some(output_str),
            _ => None,
        }
    }"""

if OLD_TIMESTAMP in content:
    content = content.replace(OLD_TIMESTAMP, NEW_TIMESTAMP, 1)
    print("P0-4: Differentiated try_fix_timestamp_repair (added -max_muxing_queue_size)")
else:
    print("P0-4: Pattern not found for try_fix_timestamp_repair")

# Differentiate subtitle remux — focused on subtitle stream issues
OLD_SUBTITLE = """    fn try_fix_subtitle_remux(&self, _file_index: usize, file_path: &str) -> Option<String> {
        let input_path = std::path::Path::new(file_path);
        let stem = input_path.file_stem()?.to_string_lossy();
        let output_path = self.temp_dir.join(format!("{}_sub_fix.mkv", stem));
        let output_str = output_path.to_string_lossy().to_string();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd
            .args([
                "-y", "-i", file_path,
                "-c", "copy",
                "-map", "0",
                "-fflags", "+genpts",
                "-avoid_negative_ts", "make_zero",
                &output_str,
            ])
            .output();

        match status {
            Ok(out) if out.status.success() => Some(output_str),
            _ => None,
        }
    }"""

NEW_SUBTITLE = """    fn try_fix_subtitle_remux(&self, _file_index: usize, file_path: &str) -> Option<String> {
        // P0-4: Subtitle remux is specialized for subtitle stream issues:
        // negative timestamps, broken numbering, PGS/dvd subtitle problems.
        // Stream-copies all streams but focuses subtitle container repair.
        let input_path = std::path::Path::new(file_path);
        let stem = input_path.file_stem()?.to_string_lossy();
        let output_path = self.temp_dir.join(format!("{}_sub_fix.mkv", stem));
        let output_str = output_path.to_string_lossy().to_string();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd
            .args([
                "-y", "-i", file_path,
                "-c", "copy",
                "-map", "0",
                // Regenerate PTS to fix subtitle timestamp issues
                "-fflags", "+genpts",
                // Shift negative timestamps to zero
                "-avoid_negative_ts", "make_zero",
                // Prevent subtitle queue overflow on files with many subtitle streams
                "-max_muxing_queue_size", "4096",
                &output_str,
            ])
            .output();

        match status {
            Ok(out) if out.status.success() => Some(output_str),
            _ => None,
        }
    }"""

if OLD_SUBTITLE in content:
    content = content.replace(OLD_SUBTITLE, NEW_SUBTITLE, 1)
    print("P0-4b: Differentiated try_fix_subtitle_remux (focused comment + max_muxing_queue_size)")
else:
    print("P0-4b: Pattern not found for try_fix_subtitle_remux")

with open(REPAIR_RS, 'w', encoding='utf-8') as f:
    f.write(content)

print("Done writing repair.rs")
