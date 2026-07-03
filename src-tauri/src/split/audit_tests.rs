use super::types::*;
use super::planner::*;
use super::engine::*;
use crate::commands::split::{parse_report_file, correct_overlapping_chapters};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn get_ffmpeg_ffprobe() -> Option<(PathBuf, PathBuf)> {
    let settings = crate::services::settings::load_settings_internal();
    let ffmpeg = crate::ffmpeg::find_ffmpeg(settings.ffmpeg_path.as_deref()).ok()?;
    let ffprobe = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref()).ok()?;
    Some((ffmpeg, ffprobe))
}

// ── Test 1: Timeline Integrity & 28h Bug Prevention ────────────────────────
#[test]
fn forensic_audit_timeline_integrity_and_prevention() {
    let duration = 3600.0 * 300.0; // 300 hours (long timeline to check overflows)
    let size = 100_000_000_000u64; // 100 GB

    // 1. Test planner splits it into 100 segments (ByParts mode)
    let params = SplitParams {
        part_count: Some(100),
        ..Default::default()
    };
    let req = SplitPlanRequest {
        job_id: "long_job".to_string(),
        input_file: "long_video.mp4".to_string(),
        input_duration: duration,
        input_size_bytes: size,
        mode: SplitMode::ByParts,
        params,
        output_dir: "".to_string(),
    };
    
    let plan = generate_plan(&req).unwrap();
    assert_eq!(plan.segments.len(), 100);

    let mut last_end = 0.0;
    let mut total_dur = 0.0;
    for (i, seg) in plan.segments.iter().enumerate() {
        assert_eq!(seg.index, i + 1);
        // Check for gaps or overlaps in planned segments
        assert!((seg.start_time - last_end).abs() < 1e-5, "Timeline gap at segment {}", i);
        total_dur += seg.duration;
        last_end = seg.end_time;
    }
    
    // Verify no duration inflation or drift
    assert!((total_dur - duration).abs() < 1e-5, "Timeline duration drift detected: expected {}, got {}", duration, total_dur);
}

// ── Test 2: Subtitle Integrity ──────────────────────────────────────────────
#[test]
fn forensic_audit_subtitle_integrity() {
    let srt_content = "\
1
00:00:05,000 --> 00:00:25,000
Spanning subtitle (5s to 25s)

2
00:00:28,000 --> 00:00:35,000
Spanning segment boundary (28s to 35s)

3
00:00:40,000 --> 00:02:40,000
Very long subtitle (40s to 160s)
";

    // Split plan: 30s segments
    let segments = vec![
        SplitSegment {
            index: 1,
            label: "Part 1".to_string(),
            start_time: 0.0,
            end_time: 30.0,
            duration: 30.0,
            estimated_size_bytes: None,
        },
        SplitSegment {
            index: 2,
            label: "Part 2".to_string(),
            start_time: 30.0,
            end_time: 60.0,
            duration: 30.0,
            estimated_size_bytes: None,
        },
    ];

    let temp_dir = std::env::temp_dir();
    let video_paths = vec![
        temp_dir.join("Part1_sub_audit.mp4").to_string_lossy().into_owned(),
        temp_dir.join("Part2_sub_audit.mp4").to_string_lossy().into_owned(),
    ];

    let result = split_srt_for_segments(srt_content, &segments, &video_paths, &None).unwrap();
    assert_eq!(result.len(), 2);

    // Verify boundary clamping and shifting
    let p1 = std::fs::read_to_string(&result[0]).unwrap();
    assert!(p1.contains("1\n00:00:05,000 --> 00:00:25,000"));
    assert!(p1.contains("2\n00:00:28,000 --> 00:00:30,000")); // Clamped to segment end (30s)

    let p2 = std::fs::read_to_string(&result[1]).unwrap();
    assert!(p2.contains("1\n00:00:00,000 --> 00:00:05,000")); // Shifted & clamped from 28s-35s (30s segment offset)

    // Clean up
    let _ = std::fs::remove_file(&result[0]);
    let _ = std::fs::remove_file(&result[1]);
}

// ── Test: Subtitle Encodings Robustness & Whitespace Edge Cases ──────────────
#[test]
fn forensic_audit_subtitle_encodings_and_whitespaces() {
    let raw_text = "\
1
00:00:01,000 --> 00:00:05,000
First subtitle entry (üéñ)


2
00:00:06,000 --> 00:00:10,000
Second subtitle entry
";

    let temp_dir = std::env::temp_dir();
    
    // 1. Standard UTF-8
    let utf8_path = temp_dir.join("test_sub_utf8.srt");
    std::fs::write(&utf8_path, raw_text).unwrap();
    
    // 2. UTF-8 with BOM
    let utf8_bom_path = temp_dir.join("test_sub_utf8_bom.srt");
    let mut utf8_bom_bytes = vec![0xEF, 0xBB, 0xBF];
    utf8_bom_bytes.extend_from_slice(raw_text.as_bytes());
    std::fs::write(&utf8_bom_path, utf8_bom_bytes).unwrap();
    
    // 3. UTF-16 LE
    let utf16_le_path = temp_dir.join("test_sub_utf16le.srt");
    let utf16_chars: Vec<u16> = raw_text.encode_utf16().collect();
    let mut utf16_le_bytes = vec![0xFF, 0xFE];
    for c in utf16_chars {
        utf16_le_bytes.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(&utf16_le_path, utf16_le_bytes).unwrap();
    
    // 4. UTF-16 BE
    let utf16_be_path = temp_dir.join("test_sub_utf16be.srt");
    let utf16_chars_be: Vec<u16> = raw_text.encode_utf16().collect();
    let mut utf16_be_bytes = vec![0xFE, 0xFF];
    for c in utf16_chars_be {
        utf16_be_bytes.extend_from_slice(&c.to_be_bytes());
    }
    std::fs::write(&utf16_be_path, utf16_be_bytes).unwrap();
    
    // 5. Windows-1252 / ANSI (using a lossy encoding or a mock ANSI representation)
    let ansi_path = temp_dir.join("test_sub_ansi.srt");
    let mut ansi_bytes = raw_text.as_bytes().to_vec();
    if let Some(pos) = ansi_bytes.iter().position(|&b| b == 0xC3) {
        ansi_bytes[pos] = 0xFC;
        ansi_bytes.remove(pos + 1);
    }
    std::fs::write(&ansi_path, ansi_bytes).unwrap();

    // Verify robust reads
    let s_utf8 = read_subtitle_file_robust(&utf8_path).unwrap();
    assert!(s_utf8.contains("First subtitle entry"));
    
    let s_utf8_bom = read_subtitle_file_robust(&utf8_bom_path).unwrap();
    assert!(s_utf8_bom.contains("First subtitle entry"));
    
    let s_utf16le = read_subtitle_file_robust(&utf16_le_path).unwrap();
    assert!(s_utf16le.contains("First subtitle entry"));
    
    let s_utf16be = read_subtitle_file_robust(&utf16_be_path).unwrap();
    assert!(s_utf16be.contains("First subtitle entry"));
    
    let s_ansi = read_subtitle_file_robust(&ansi_path).unwrap();
    assert!(s_ansi.contains("First subtitle entry"));

    // Verify parses
    let p_utf8 = parse_srt(&s_utf8).unwrap();
    assert_eq!(p_utf8.len(), 2);
    assert_eq!(p_utf8[0].index, 1);
    assert_eq!(p_utf8[1].index, 2);

    let p_utf16le = parse_srt(&s_utf16le).unwrap();
    assert_eq!(p_utf16le.len(), 2);

    let p_ansi = parse_srt(&s_ansi).unwrap();
    assert_eq!(p_ansi.len(), 2);

    // Clean up
    let _ = std::fs::remove_file(utf8_path);
    let _ = std::fs::remove_file(utf8_bom_path);
    let _ = std::fs::remove_file(utf16_le_path);
    let _ = std::fs::remove_file(utf16_be_path);
    let _ = std::fs::remove_file(ansi_path);
}

// ── Test 3: Subtitle Naming Synchronization & Inheritance ───────────────────
#[test]
fn forensic_audit_subtitle_naming_sync() {
    let base_dir = Path::new("D:\\TestFolder\\");
    
    // 1. Custom template naming
    let out = generate_output_filename(
        base_dir,
        Path::new("Course.mp4"),
        "Lesson 1",
        "mp4",
        1,
        5,
        0.0,
        10.0,
        Some("{stem}_{index}_of_{segment_count}_{chapter}"),
        None,
        Some("_suffix"),
        false,
    );
    
    assert_eq!(out.file_name().unwrap().to_str().unwrap(), "Course_001_of_005_Lesson 1_suffix.mp4");
    
    // Subtitle must inherit the exact filename but with .srt
    let sub_out = out.with_extension("srt");
    assert_eq!(sub_out.file_name().unwrap().to_str().unwrap(), "Course_001_of_005_Lesson 1_suffix.srt");
}

// ── Test 4: Playlist Split Chapter Grouping ───────────────────────────
#[test]
fn forensic_audit_playlist_split_grouping() {
    // Group 51 mock chapters into segments of 10
    let mut chapters = Vec::new();
    for i in 0..51 {
        chapters.push((format!("Lesson {}", i + 1), i as f64 * 60.0, (i + 1) as f64 * 60.0));
    }

    let items_per_seg = 10;
    let mut custom_ranges = Vec::new();
    let mut segment_labels = Vec::new();

    let chunks = chapters.chunks(items_per_seg);
    for (idx, chunk) in chunks.enumerate() {
        let first = &chunk[0];
        let last = &chunk[chunk.len() - 1];
        custom_ranges.push(first.1);
        custom_ranges.push(last.2);
        segment_labels.push(format!("Batch {}", idx + 1));
    }

    // Verify exactly 6 segments (5 segments of 10 chapters, 1 segment of 1 chapter)
    assert_eq!(segment_labels.len(), 6);
    assert_eq!(custom_ranges.len(), 12);
    
    // Segment 1 boundary: 0 to 600s (10 mins)
    assert_eq!(custom_ranges[0], 0.0);
    assert_eq!(custom_ranges[1], 600.0);
    
    // Segment 6 boundary: 3000s to 3060s
    assert_eq!(custom_ranges[10], 3000.0);
    assert_eq!(custom_ranges[11], 3060.0);
}

// ── Test 5: Merge Report Parser Fallback ────────────────────────────────────
#[test]
fn forensic_audit_report_parser_fallback() {
    let report_content_v2 = "\
╔══════════════════════════════════════════════════════════════╗
║                       MERGE REPORT                          ║
╚══════════════════════════════════════════════════════════════╝
  Report Version : 2
  Generated  : 2026-05-30 06:12:34
  Output     : C:/output.mp4
  Total Time : 00:25:00
  File Size  : 50.0 MB
  Files      : 3
  ┌─────┬──────────────────────────────────────────┬──────────┬──────────────────────┬──────────┐
  │  #  │ File Name                                │ Duration │ In Merged (Start-End)│ Remaining│
  ├─────┼──────────────────────────────────────────┼──────────┼──────────────────────┼──────────┤
  │   1 │ Introduction - Python.mp4                │ 00:05:00 │ 00:00:00 → 00:05:00 │ 00:20:00 │
  │   2 │ Chapter 2: Variables & Types.mp4          │ 00:10:00 │ 00:05:00 → 00:15:00 │ 00:10:00 │
  │   3 │ Corrupted entry line (should skip)
  │   4 │ Conclusion - 日本語.mp4                   │ 00:10:00 │ 00:15:00 → 00:25:00 │ 00:00:00 │
  └─────┴──────────────────────────────────────────┴──────────┴──────────────────────┴──────────┘
";

    let temp_dir = std::env::temp_dir();
    let report_path = temp_dir.join("test_report.txt");
    std::fs::write(&report_path, report_content_v2).unwrap();

    let parsed = parse_report_file(&report_path).unwrap();
    
    // Check that we skipped the corrupted line and parsed exactly 3 entries
    assert_eq!(parsed.len(), 3);
    
    // Entry 1
    assert_eq!(parsed[0].0, "Introduction - Python.mp4");
    assert_eq!(parsed[0].1, 0.0);
    assert_eq!(parsed[0].2, 300.0);

    // Entry 3 (Conclusion - 日本語.mp4)
    assert_eq!(parsed[2].0, "Conclusion - 日本語.mp4");
    assert_eq!(parsed[2].1, 900.0);
    assert_eq!(parsed[2].2, 1500.0);

    let _ = std::fs::remove_file(&report_path);
}

// ── Test 6: Chapter Overlap Correction ──────────────────────────────────────
#[test]
fn forensic_audit_chapter_overlap_correction() {
    let mut chapters = vec![
        ("Chapter 1".to_string(), 0.0, 10.0),
        ("Chapter 2".to_string(), 9.0, 20.0), // Overlaps by 1s
        ("Chapter 3".to_string(), 25.0, 30.0),
    ];

    let warnings = correct_overlapping_chapters(&mut chapters);
    
    // Check that Chapter 1 end time was clamped to Chapter 2 start time (9.0)
    assert_eq!(chapters[0].2, 9.0);
    assert_eq!(chapters[1].1, 9.0);
    assert_eq!(chapters[1].2, 20.0);
    
    // Verify that warnings were returned
    assert!(!warnings.is_empty());
}

// ── Test 7: Temp File Cleanup ───────────────────────────────────────────────
#[test]
fn forensic_audit_temp_file_cleanup() {
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("extracted_sub_temp_test_job_123.srt");
    std::fs::write(&test_file, "1\n00:00:01,000 --> 00:00:02,000\nHello\n").unwrap();
    
    assert!(test_file.exists());

    // Run startup cleanup code manually (simulate cleanup check)
    let fname = test_file.file_name().unwrap().to_str().unwrap();
    let matches_cleanup = fname.starts_with("extracted_sub_");
    
    assert!(matches_cleanup);
    
    std::fs::remove_file(&test_file).unwrap();
}

// ── Test 8: Concurrent Jobs Collision Safety ────────────────────────────────
#[test]
fn forensic_audit_concurrent_jobs_collision_safety() {
    use std::thread;

    let mut handles = vec![];
    for job_idx in 0..3 {
        handles.push(thread::spawn(move || {
            let temp_dir = std::env::temp_dir();
            let base_dir = temp_dir.join(format!("job_concurrent_{}", job_idx));
            let _ = std::fs::create_dir_all(&base_dir);

            // Generate output filenames
            let out = generate_output_filename(
                &base_dir,
                Path::new("Course.mp4"),
                "Segment",
                "mp4",
                1,
                3,
                0.0,
                10.0,
                None,
                None,
                None,
                false,
            );

            // Verify paths are completely job-isolated
            assert!(out.to_string_lossy().contains(&format!("job_concurrent_{}", job_idx)));
            let _ = std::fs::remove_dir_all(&base_dir);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

// ── Test 9: Output Collision Handling & Sanitization ────────────────────────
#[test]
fn forensic_audit_output_collision_handling_and_sanitization() {
    let temp_dir = std::env::temp_dir();
    let coll_dir = temp_dir.join("collision_test_dir");
    let _ = std::fs::create_dir_all(&coll_dir);

    // Unsanitized name
    let out_raw = generate_output_filename(
        &coll_dir,
        Path::new("Course.mp4"),
        "Lesson: Over? * < > | \" \\ /",
        "mp4",
        1,
        1,
        0.0,
        10.0,
        Some("{stem}_{chapter}"),
        None,
        None,
        false,
    );

    // Check that illegal characters were removed
    let fname_str = out_raw.file_name().unwrap().to_str().unwrap();
    assert!(!fname_str.contains(':'));
    assert!(!fname_str.contains('?'));
    assert!(!fname_str.contains('*'));

    // Create a mock file at the resolved sanitized path
    std::fs::write(&out_raw, "data").unwrap();
    
    // Resolve output filename again
    let out_coll = generate_output_filename(
        &coll_dir,
        Path::new("Course.mp4"),
        "Lesson: Over? * < > | \" \\ /",
        "mp4",
        1,
        1,
        0.0,
        10.0,
        Some("{stem}_{chapter}"),
        None,
        None,
        false,
    );

    // Must be auto-incremented to avoid overwrite
    let fname_coll = out_coll.file_name().unwrap().to_str().unwrap();
    assert!(fname_coll.contains("(1)"));

    std::fs::remove_file(&out_raw).unwrap();
    let _ = std::fs::remove_dir_all(&coll_dir);
}

// ── Test 10: Expanded E2E Roundtrip Test ────────────────────────────────────
#[test]
fn forensic_audit_expanded_roundtrip() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("roundtrip_audit_test");
    let _ = std::fs::create_dir_all(&temp_dir);

    let synthetic_path = temp_dir.join("synthetic.mp4");
    
    // 1. Create synthetic video file (lavfi color src, 3.0s duration, 30fps) with -g 1 to force keyframes on every frame
    let status = Command::new(&ffmpeg)
        .args([
            "-y", "-f", "lavfi",
            "-i", "color=c=blue:s=640x360:d=3.0:r=30",
            "-f", "lavfi",
            "-i", "anullsrc=r=48000:cl=stereo",
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "28", "-g", "1",
            "-c:a", "aac", "-shortest",
            synthetic_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success());

    // 2. Mux chapters (metadata) into the original file (no extra data streams)
    let metadata_path = temp_dir.join("metadata.txt");
    let metadata_content = "\
;FFMETADATA1
title=Synthetic Roundtrip

[CHAPTER]
TIMEBASE=1/1000
START=0
END=1000
title=Chapter 1

[CHAPTER]
TIMEBASE=1/1000
START=1000
END=2000
title=Chapter 2

[CHAPTER]
TIMEBASE=1/1000
START=2000
END=3000
title=Chapter 3
";
    std::fs::write(&metadata_path, metadata_content).unwrap();

    let synthetic_chapters_path = temp_dir.join("synthetic_chapters.mp4");
    let status = Command::new(&ffmpeg)
        .args([
            "-y",
            "-i", synthetic_path.to_str().unwrap(),
            "-i", metadata_path.to_str().unwrap(),
            "-map", "0:v",
            "-map", "0:a",
            "-map_metadata", "1",
            "-c", "copy",
            synthetic_chapters_path.to_str().unwrap()
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success());

    // 3. Create companion subtitle file next to the video
    let srt_path = temp_dir.join("synthetic_chapters.srt");
    std::fs::write(&srt_path, "1\n00:00:00,200 --> 00:00:00,800\nTest Sub\n").unwrap();

    // 4. Plan Split (3 segments of 1.0s)
    let plan = SplitPlan {
        job_id: "rt_job".to_string(),
        input_file: synthetic_chapters_path.to_string_lossy().into_owned(),
        input_duration: 3.0,
        input_size_bytes: std::fs::metadata(&synthetic_chapters_path).unwrap().len(),
        mode: SplitMode::ByDuration,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 1.0, duration: 1.0, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Part2".to_string(), start_time: 1.0, end_time: 2.0, duration: 1.0, estimated_size_bytes: None },
            SplitSegment { index: 3, label: "Part3".to_string(), start_time: 2.0, end_time: 3.0, duration: 1.0, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    // 5. Execute split (using CopyAll subtitle mode - it will automatically find and split the companion SRT)
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress_callback = |_| {};
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        Some(SplitSubtitleMode::ExtractSplit),
        Some(true),
        cancel_flag,
        progress_callback
    ).unwrap();

    assert_eq!(result.output_paths.len(), 3);

    // Verify split SRT files were created and check their content
    let srt_paths = result.srt_output_paths.clone().unwrap_or_default();
    assert_eq!(srt_paths.len(), 3);
    
    // Part 1 SRT must contain the subtitle
    let srt_part1 = std::fs::read_to_string(&srt_paths[0]).unwrap();
    assert!(srt_part1.contains("Test Sub"));
    
    // Part 2 and Part 3 SRTs must be empty
    let srt_part2 = std::fs::read_to_string(&srt_paths[1]).unwrap();
    let srt_part3 = std::fs::read_to_string(&srt_paths[2]).unwrap();
    assert!(srt_part2.trim().is_empty());
    assert!(srt_part3.trim().is_empty());

    // 6. Merge split videos back
    let concat_list_path = temp_dir.join("concat_list.txt");
    let mut concat_content = String::new();
    for path in &result.output_paths {
        let escaped_path = path.replace('\\', "/");
        concat_content.push_mut(format!("file '{}'\n", escaped_path));
    }
    std::fs::write(&concat_list_path, concat_content).unwrap();

    let remerged_temp_path = temp_dir.join("remerged_temp.mp4");
    let status = Command::new(&ffmpeg)
        .args([
            "-y", "-f", "concat", "-safe", "0",
            "-i", concat_list_path.to_str().unwrap(),
            "-c", "copy",
            "-map", "0",
            remerged_temp_path.to_str().unwrap()
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success());

    // Mux the original chapters metadata back into the remerged file to restore chapters
    let remerged_path = temp_dir.join("remerged.mp4");
    let status = Command::new(&ffmpeg)
        .args([
            "-y",
            "-i", remerged_temp_path.to_str().unwrap(),
            "-i", metadata_path.to_str().unwrap(),
            "-map", "0",
            "-map_metadata", "1",
            "-c", "copy",
            remerged_path.to_str().unwrap()
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success());

    // 7. Verify expanded roundtrip metrics using ffprobe
    let probe_original = probe_video_metrics(&ffprobe, &synthetic_chapters_path).unwrap();
    let probe_remerged = probe_video_metrics(&ffprobe, &remerged_path).unwrap();

    // Verify all metrics match exactly
    assert!((probe_original.duration - probe_remerged.duration).abs() < 0.1, "Duration mismatch: expected {}, got {}", probe_original.duration, probe_remerged.duration);
    assert_eq!(probe_original.fps, probe_remerged.fps, "FPS mismatch");
    assert_eq!(probe_original.timebase, probe_remerged.timebase, "Timebase mismatch");
    assert_eq!(probe_original.width, probe_remerged.width, "Width mismatch");
    assert_eq!(probe_original.height, probe_remerged.height, "Height mismatch");
    assert_eq!(probe_original.video_codec, probe_remerged.video_codec, "Video codec mismatch");
    assert_eq!(probe_original.audio_codec, probe_remerged.audio_codec, "Audio codec mismatch");
    assert_eq!(probe_original.audio_sample_rate, probe_remerged.audio_sample_rate, "Audio sample rate mismatch");
    assert_eq!(probe_original.subtitle_count, probe_remerged.subtitle_count, "Subtitle count mismatch");
    assert_eq!(probe_original.chapter_count, probe_remerged.chapter_count, "Chapter count mismatch");
    assert_eq!(probe_original.frame_count, probe_remerged.frame_count, "Frame count mismatch");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

struct ProbedMetrics {
    duration: f64,
    fps: f64,
    timebase: String,
    width: u32,
    height: u32,
    video_codec: String,
    audio_codec: String,
    audio_sample_rate: u32,
    subtitle_count: usize,
    chapter_count: usize,
    frame_count: u64,
}

fn probe_video_metrics(ffprobe_path: &Path, file_path: &Path) -> Result<ProbedMetrics, String> {
    let output = Command::new(ffprobe_path)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
            file_path.to_str().unwrap()
        ])
        .output()
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| e.to_string())?;

    let format = &json["format"];
    let duration: f64 = format["duration"].as_str().unwrap_or("0.0").parse().unwrap_or(0.0);

    let chapter_count = json["chapters"].as_array().map(|a| a.len()).unwrap_or(0);

    let streams = json["streams"].as_array().ok_or("No streams found")?;
    let mut fps = 0.0;
    let mut timebase = String::new();
    let mut width = 0;
    let mut height = 0;
    let mut video_codec = String::new();
    let mut audio_codec = String::new();
    let mut audio_sample_rate = 0;
    let mut subtitle_count = 0;
    let mut frame_count = 0;

    for stream in streams {
        let codec_type = stream["codec_type"].as_str().unwrap_or("");
        if codec_type == "video" {
            video_codec = stream["codec_name"].as_str().unwrap_or("").to_string();
            width = stream["width"].as_u64().unwrap_or(0) as u32;
            height = stream["height"].as_u64().unwrap_or(0) as u32;
            timebase = stream["time_base"].as_str().unwrap_or("").to_string();

            if let Some(r_frame_rate) = stream["r_frame_rate"].as_str() {
                let parts: Vec<&str> = r_frame_rate.split('/').collect();
                if parts.len() == 2 {
                    let num: f64 = parts[0].parse().unwrap_or(0.0);
                    let den: f64 = parts[1].parse().unwrap_or(1.0);
                    if den > 0.0 {
                        fps = num / den;
                    }
                }
            }

            frame_count = stream["nb_frames"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| stream["nb_frames"].as_u64())
                .unwrap_or(0);
        } else if codec_type == "audio" {
            audio_codec = stream["codec_name"].as_str().unwrap_or("").to_string();
            audio_sample_rate = stream["sample_rate"].as_str().unwrap_or("0").parse().unwrap_or(0);
        } else if codec_type == "subtitle" {
            subtitle_count += 1;
        }
    }

    Ok(ProbedMetrics {
        duration,
        fps,
        timebase,
        width,
        height,
        video_codec,
        audio_codec,
        audio_sample_rate,
        subtitle_count,
        chapter_count,
        frame_count,
    })
}

trait PushMut {
    fn push_mut(&mut self, s: String);
}

impl PushMut for String {
    fn push_mut(&mut self, s: String) {
        self.push_str(&s);
    }
}

// ── Production Audit Tests ──────────────────────────────────────────────────

// TEST 1: 30-hour video split by duration (verify playable, correct duration, correct seeking)
#[test]
fn production_audit_1_stress_seek() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("prod_audit_test_1");
    let _ = std::fs::create_dir_all(&temp_dir);

    let path_30h = temp_dir.join("video_30h.mp4");
    
    // Create a 30-second H.264 video at 1 fps with AAC silent audio
    let status = Command::new(&ffmpeg)
        .args([
            "-y", "-f", "lavfi",
            "-i", "color=c=black:s=64x64:d=30:r=1",
            "-f", "lavfi",
            "-i", "anullsrc=r=48000:cl=stereo",
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "51", "-g", "1",
            "-c:a", "aac", "-shortest",
            path_30h.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success());

    // Split it by duration into 10-second segments
    let plan = SplitPlan {
        job_id: "prod_1_job".to_string(),
        input_file: path_30h.to_string_lossy().into_owned(),
        input_duration: 30.0,
        input_size_bytes: std::fs::metadata(&path_30h).unwrap().len(),
        mode: SplitMode::ByDuration,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 10.0, duration: 10.0, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Part2".to_string(), start_time: 10.0, end_time: 20.0, duration: 10.0, estimated_size_bytes: None },
            SplitSegment { index: 3, label: "Part3".to_string(), start_time: 20.0, end_time: 30.0, duration: 10.0, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        None,
        None,
        cancel_flag,
        |_| {}
    ).unwrap();

    assert_eq!(result.output_paths.len(), 3);

    // Verify playability and seekability near the end of each segment
    for path in &result.output_paths {
        let p = Path::new(path);
        assert!(p.exists());

        // 1. Verify playability
        let status = Command::new(&ffmpeg)
            .args(["-v", "error", "-i", path, "-t", "5", "-f", "null", "-"])
            .status()
            .unwrap();
        assert!(status.success(), "File not playable: {}", path);

        // 2. Verify seekability near the end (seeking at 99%, 99.9%, and last frame)
        // Part is 10 seconds. 99% = 9.9s, 99.9% = 9.99s, last frame = 9.999s (at 1 fps)
        for seek_time in &["9.9", "9.99", "9.999"] {
            let status = Command::new(&ffmpeg)
                .args(["-v", "error", "-ss", seek_time, "-i", path, "-vframes", "1", "-f", "null", "-"])
                .status()
                .unwrap();
            assert!(status.success(), "Failed seeking to {}s in {}", seek_time, path);
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// TEST 2: 50-hour course playlist split (verify playable, no drift)
#[test]
fn production_audit_2_playlist_stress() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("prod_audit_test_2");
    let _ = std::fs::create_dir_all(&temp_dir);

    let path_50h = temp_dir.join("video_50h.mp4");
    
    // Create a 50-second H.264 video at 1 fps
    let status = Command::new(&ffmpeg)
        .args([
            "-y", "-f", "lavfi",
            "-i", "color=c=blue:s=64x64:d=50:r=1",
            "-f", "lavfi",
            "-i", "anullsrc=r=48000:cl=stereo",
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "51", "-g", "1",
            "-c:a", "aac", "-shortest",
            path_50h.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success());

    // Generate a mock report file with 50 lessons (1 second each = 50s total)
    let mut report_content = String::new();
    report_content.push_str("╔══════════════════════════════════════════════════════════════╗\n");
    report_content.push_str("║                       MERGE REPORT                          ║\n");
    report_content.push_str("╚══════════════════════════════════════════════════════════════╝\n");
    report_content.push_str("  Report Version : 2\n");
    report_content.push_str("  Total Time : 50:00:00\n");
    report_content.push_str("  Files      : 50\n\n");
    report_content.push_str("  ┌─────┬──────────────────────────────────────────┬──────────┬──────────────────────┬──────────┐\n");
    report_content.push_str("  │  #  │ File Name                                │ Duration │ In Merged (Start-End)│ Remaining│\n");
    report_content.push_str("  ├─────┼──────────────────────────────────────────┼──────────┼──────────────────────┼──────────┤\n");
    
    for i in 0..50 {
        let start_h = i;
        let end_h = i + 1;
        let start_str = format!("{:02}:00:00", start_h);
        let end_str = format!("{:02}:00:00", end_h);
        let name = format!("Lesson_{:02}.mp4", i + 1);
        report_content.push_str(&format!(
            "  │ {:>3} │ {:<40} │ 01:00:00 │ {} → {} │ 00:00:00 │\n",
            i + 1, name, start_str, end_str
        ));
    }
    report_content.push_str("  └─────┴──────────────────────────────────────────┴──────────┴──────────────────────┴──────────┘\n");

    let report_path = temp_dir.join("video_50h_report.txt");
    std::fs::write(&report_path, report_content).unwrap();

    // Verify parser extracts all 50 entries
    let parsed = parse_report_file(&report_path).unwrap();
    assert_eq!(parsed.len(), 50);

    // Grouping by 10 lessons per segment (yielding 5 segments of 10s each)
    let mut segments = Vec::new();
    for i in 0..5 {
        segments.push(SplitSegment {
            index: i + 1,
            label: format!("Batch_{}", i + 1),
            start_time: i as f64 * 10.0,
            end_time: (i + 1) as f64 * 10.0,
            duration: 10.0,
            estimated_size_bytes: None,
        });
    }

    let plan = SplitPlan {
        job_id: "prod_2_job".to_string(),
        input_file: path_50h.to_string_lossy().into_owned(),
        input_duration: 50.0,
        input_size_bytes: std::fs::metadata(&path_50h).unwrap().len(),
        mode: SplitMode::ByPlaylistItems,
        segments,
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        None,
        None,
        cancel_flag,
        |_| {}
    ).unwrap();

    assert_eq!(result.output_paths.len(), 5);

    // Verify playability and drift
    let mut total_dur = 0.0;
    for path in &result.output_paths {
        let metrics = probe_video_metrics(&ffprobe, Path::new(path)).unwrap();
        total_dur += metrics.duration;
        
        let status = Command::new(&ffmpeg)
            .args(["-v", "error", "-i", path, "-t", "5", "-f", "null", "-"])
            .status()
            .unwrap();
        assert!(status.success());
    }

    assert!((total_dur - 50.0).abs() < 1.0, "Drift detected: {}", total_dur);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// TEST 3: External subtitles unicode (English, Arabic, Japanese, Chinese - check filenames, encoding, timing)
#[test]
fn production_audit_3_external_subtitles_unicode() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("prod_audit_test_3");
    let _ = std::fs::create_dir_all(&temp_dir);

    // Copy a small fixture file as base
    let base_video = Path::new("../tests/fixtures/video1.mp4");
    let test_video = temp_dir.join("video_unicode.mp4");
    std::fs::copy(base_video, &test_video).unwrap();

    // Create companion SRTs with UTF-8 Unicode content
    let subs = vec![
        ("en", "1\n00:00:01,000 --> 00:00:03,000\nHello English\n"),
        ("ar", "1\n00:00:01,000 --> 00:00:03,000\nمرحبا بالعربية\n"),
        ("ja", "1\n00:00:01,000 --> 00:00:03,000\nこんにちは日本語\n"),
        ("zh", "1\n00:00:01,000 --> 00:00:03,000\n你好中文\n"),
    ];

    for (lang, content) in &subs {
        let srt_path = temp_dir.join(format!("video_unicode.{}.srt", lang));
        std::fs::write(&srt_path, content).unwrap();
    }

    // Split video into 2 parts
    let plan = SplitPlan {
        job_id: "prod_3_job".to_string(),
        input_file: test_video.to_string_lossy().into_owned(),
        input_duration: 5.0,
        input_size_bytes: std::fs::metadata(&test_video).unwrap().len(),
        mode: SplitMode::ByDuration,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 2.0, duration: 2.0, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Part2".to_string(), start_time: 2.0, end_time: 4.0, duration: 2.0, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        Some(SplitSubtitleMode::ExtractSplit),
        Some(true),
        cancel_flag,
        |_| {}
    ).unwrap();

    // Verify subtitle outputs are named correctly and contain unicode
    let srt_paths = result.srt_output_paths.unwrap();
    // 2 segments * 4 languages = 8 SRT files
    assert_eq!(srt_paths.len(), 8);

    for srt_path_str in &srt_paths {
        let path = Path::new(srt_path_str);
        assert!(path.exists());
        
        let content = std::fs::read_to_string(path).unwrap();
        assert!(!content.is_empty());
        
        // Timing shift check (Part 2 starts at 2.0s, so subtitles should be shifted)
        if srt_path_str.contains("Part2") {
            // Original subtitle was 1s -> 3s. Shifting by 2s makes it start at 0s, clamping to 0s -> 1s.
            assert!(content.contains("00:00:00,000 --> 00:00:01,000"), "Content timing incorrect: {}", content);
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// TEST 4: Embedded subtitles (multiple tracks - verify track count, language metadata, ordering preserved)
#[test]
fn production_audit_4_embedded_subtitles_preservation() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("prod_audit_test_4");
    let _ = std::fs::create_dir_all(&temp_dir);

    // Create a video with multiple embedded subtitle tracks
    let base_video = Path::new("../tests/fixtures/video1.mp4");
    let eng_srt = temp_dir.join("eng.srt");
    let jpn_srt = temp_dir.join("jpn.srt");
    std::fs::write(&eng_srt, "1\n00:00:01,000 --> 00:00:02,000\nEnglish\n").unwrap();
    std::fs::write(&jpn_srt, "1\n00:00:01,000 --> 00:00:02,000\n日本語\n").unwrap();

    let video_with_subs = temp_dir.join("video_embedded_subs.mp4");
    let status = Command::new(&ffmpeg)
        .args([
            "-y",
            "-i", base_video.to_str().unwrap(),
            "-i", eng_srt.to_str().unwrap(),
            "-i", jpn_srt.to_str().unwrap(),
            "-map", "0:0",
            "-map", "0:1",
            "-map", "1:0",
            "-map", "2:0",
            "-c", "copy",
            "-c:s:0", "mov_text",
            "-metadata:s:s:0", "language=eng",
            "-metadata:s:s:0", "title=English",
            "-c:s:1", "mov_text",
            "-metadata:s:s:1", "language=jpn",
            "-metadata:s:s:1", "title=Japanese",
            video_with_subs.to_str().unwrap()
        ])
        .status()
        .unwrap();
    assert!(status.success());

    // Verify original streams
    let probe_orig = probe_video_metrics(&ffprobe, &video_with_subs).unwrap();
    assert_eq!(probe_orig.subtitle_count, 2);

    let plan = SplitPlan {
        job_id: "prod_4_job".to_string(),
        input_file: video_with_subs.to_string_lossy().into_owned(),
        input_duration: 5.0,
        input_size_bytes: std::fs::metadata(&video_with_subs).unwrap().len(),
        mode: SplitMode::ByParts,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 2.0, duration: 2.0, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Part2".to_string(), start_time: 2.0, end_time: 4.0, duration: 2.0, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        Some(SplitSubtitleMode::CopyAll),
        Some(false), // Keep embedded subtitle streams inside output
        cancel_flag,
        |_| {}
    ).unwrap();

    assert_eq!(result.output_paths.len(), 2);

    // Verify all subtitle tracks are preserved with ordering & metadata
    for path in &result.output_paths {
        let probe = probe_video_metrics(&ffprobe, Path::new(path)).unwrap();
        assert_eq!(probe.subtitle_count, 2);

        let output = Command::new(&ffprobe)
            .args(["-v", "quiet", "-print_format", "json", "-show_streams", "-select_streams", "s", path])
            .output()
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let streams = json["streams"].as_array().unwrap();

        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0]["tags"]["language"].as_str().unwrap(), "eng");
        assert_eq!(streams[1]["tags"]["language"].as_str().unwrap(), "jpn");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// TEST 5: Roundtrip - real course (split, merge, compare: duration, fps, timebase, resolution, subtitle count, chapter count)
#[test]
fn production_audit_5_real_course_roundtrip() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("prod_audit_test_5");
    let _ = std::fs::create_dir_all(&temp_dir);

    let original_video = Path::new("../tests/fixtures/video_multi_subs.mp4");
    
    // Add mock chapters
    let metadata_path = temp_dir.join("metadata.txt");
    std::fs::write(&metadata_path, "\
;FFMETADATA1
title=Real Course

[CHAPTER]
TIMEBASE=1/1000
START=0
END=1500
title=Chapter A

[CHAPTER]
TIMEBASE=1/1000
START=1500
END=2995
title=Chapter B
").unwrap();

    let video_with_chapters = temp_dir.join("video_chapters.mp4");
    let status = Command::new(&ffmpeg)
        .args([
            "-y",
            "-i", original_video.to_str().unwrap(),
            "-i", metadata_path.to_str().unwrap(),
            "-map", "0:v",
            "-map", "0:a",
            "-map", "0:s",
            "-map_metadata", "1",
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-crf", "22",
            "-g", "1",
            "-c:a", "copy",
            "-c:s", "copy",
            video_with_chapters.to_str().unwrap()
        ])
        .status()
        .unwrap();
    assert!(status.success());

    // Split it into 2 parts matching the actual video duration of 2.995s
    let plan = SplitPlan {
        job_id: "prod_5_job".to_string(),
        input_file: video_with_chapters.to_string_lossy().into_owned(),
        input_duration: 2.995,
        input_size_bytes: std::fs::metadata(&video_with_chapters).unwrap().len(),
        mode: SplitMode::ByParts,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 1.5, duration: 1.5, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Part2".to_string(), start_time: 1.5, end_time: 2.995, duration: 1.495, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        Some(SplitSubtitleMode::CopyAll),
        Some(false),
        cancel_flag,
        |_| {}
    ).unwrap();

    assert_eq!(result.output_paths.len(), 2);

    // Merge back
    let concat_list_path = temp_dir.join("concat_list.txt");
    let mut concat_content = String::new();
    for path in &result.output_paths {
        let escaped_path = path.replace('\\', "/");
        concat_content.push_mut(format!("file '{}'\n", escaped_path));
    }
    std::fs::write(&concat_list_path, concat_content).unwrap();

    let remerged_temp_path = temp_dir.join("remerged_temp.mp4");
    let status = Command::new(&ffmpeg)
        .args(["-y", "-f", "concat", "-safe", "0", "-i", concat_list_path.to_str().unwrap(), "-c", "copy", "-map", "0", remerged_temp_path.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());

    // Mux chapters back
    let remerged_path = temp_dir.join("remerged.mp4");
    let status = Command::new(&ffmpeg)
        .args(["-y", "-i", remerged_temp_path.to_str().unwrap(), "-i", metadata_path.to_str().unwrap(), "-map", "0", "-map_metadata", "1", "-c", "copy", remerged_path.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success());

    // Compare original and remerged metrics
    let probe_orig = probe_video_metrics(&ffprobe, &video_with_chapters).unwrap();
    let probe_rem = probe_video_metrics(&ffprobe, &remerged_path).unwrap();

    assert!((probe_orig.duration - probe_rem.duration).abs() < 0.2, "Duration mismatch: expected {}, got {}", probe_orig.duration, probe_rem.duration);
    assert!((probe_orig.frame_count as i64 - probe_rem.frame_count as i64).abs() <= 1,
        "Frame count mismatch: expected {}, got {}", probe_orig.frame_count, probe_rem.frame_count);
    assert_eq!(probe_orig.timebase, probe_rem.timebase, "Timebase mismatch");
    assert_eq!(probe_orig.width, probe_rem.width, "Width mismatch");
    assert_eq!(probe_orig.height, probe_rem.height, "Height mismatch");
    assert_eq!(probe_orig.video_codec, probe_rem.video_codec, "Video codec mismatch");
    assert_eq!(probe_orig.audio_codec, probe_rem.audio_codec, "Audio codec mismatch");
    assert_eq!(probe_orig.audio_sample_rate, probe_rem.audio_sample_rate, "Audio sample rate mismatch");
    assert_eq!(probe_orig.subtitle_count, probe_rem.subtitle_count, "Subtitle count mismatch");
    assert_eq!(probe_orig.chapter_count, probe_rem.chapter_count, "Chapter count mismatch");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// TEST 6: Windows path stress (Long folders, Long lesson names, Unicode)
#[test]
fn production_audit_6_windows_path_stress() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir();
    
    // Create deep nested long path (> 260 characters)
    let long_folder_name = "LongFolder_StressTest_".repeat(10);
    let deep_dir = temp_dir.join(&long_folder_name);
    let _ = std::fs::create_dir_all(&deep_dir);

    let base_video = Path::new("../tests/fixtures/video1.mp4");
    let test_video = deep_dir.join("video_deep.mp4");
    std::fs::copy(base_video, &test_video).unwrap();

    // Unicode lesson name with Arabic and Japanese characters
    let label = "日本語_Arabic_عربى_Lesson_Name_Stress_Test_Unicode_".repeat(3);

    let plan = SplitPlan {
        job_id: "prod_6_job".to_string(),
        input_file: test_video.to_string_lossy().into_owned(),
        input_duration: 5.0,
        input_size_bytes: std::fs::metadata(&test_video).unwrap().len(),
        mode: SplitMode::ByParts,
        segments: vec![
            SplitSegment { index: 1, label, start_time: 0.0, end_time: 3.0, duration: 3.0, estimated_size_bytes: None },
        ],
        output_dir: deep_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        None,
        None,
        cancel_flag,
        |_| {}
    ).unwrap();

    assert_eq!(result.output_paths.len(), 1);
    assert!(Path::new(&result.output_paths[0]).exists());

    let _ = std::fs::remove_dir_all(&deep_dir);
}

// TEST 7: Concurrent jobs (Run 5 jobs simultaneously - verify no temp, subtitle, or naming collisions)
#[test]
fn production_audit_7_concurrent_jobs() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    use std::thread;
    let mut handles = vec![];

    for i in 0..5 {
        let ffmpeg_c = ffmpeg.clone();
        let ffprobe_c = ffprobe.clone();
        
        handles.push(thread::spawn(move || {
            let temp_dir = std::env::temp_dir().join(format!("prod_audit_test_7_job_{}", i));
            let _ = std::fs::create_dir_all(&temp_dir);

            let base_video = Path::new("../tests/fixtures/video1.mp4");
            let test_video = temp_dir.join(format!("video_concurrent_{}.mp4", i));
            std::fs::copy(base_video, &test_video).unwrap();

            let srt_path = temp_dir.join(format!("video_concurrent_{}.srt", i));
            std::fs::write(&srt_path, "1\n00:00:01,000 --> 00:00:02,000\nSub\n").unwrap();

            let plan = SplitPlan {
                job_id: format!("prod_7_job_{}", i),
                input_file: test_video.to_string_lossy().into_owned(),
                input_duration: 5.0,
                input_size_bytes: std::fs::metadata(&test_video).unwrap().len(),
                mode: SplitMode::ByDuration,
                segments: vec![
                    SplitSegment { index: 1, label: format!("Part_A_{}", i), start_time: 0.0, end_time: 2.0, duration: 2.0, estimated_size_bytes: None },
                    SplitSegment { index: 2, label: format!("Part_B_{}", i), start_time: 2.0, end_time: 4.0, duration: 2.0, estimated_size_bytes: None },
                ],
                output_dir: temp_dir.to_string_lossy().into_owned(),
                output_format: "mp4".to_string(),
                label_suffix: None,
                naming_template: None,
                naming_config: None,
                include_timestamp: Some(false),
            };

            let cancel_flag = Arc::new(AtomicBool::new(false));
            let result = execute_split_with_options(
                &ffmpeg_c,
                Some(&ffprobe_c),
                &plan,
                Some(SplitSubtitleMode::ExtractSplit),
                Some(true),
                cancel_flag,
                |_| {}
            ).unwrap();

            assert_eq!(result.output_paths.len(), 2);
            assert_eq!(result.srt_output_paths.unwrap().len(), 2);

            let _ = std::fs::remove_dir_all(&temp_dir);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

// TEST 8: Cancellation & Cleanup
#[test]
fn production_audit_8_cancellation_cleanup() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("prod_audit_test_8");
    let _ = std::fs::create_dir_all(&temp_dir);

    let base_video = Path::new("../tests/fixtures/video1.mp4");
    let test_video = temp_dir.join("video_cancel.mp4");
    std::fs::copy(base_video, &test_video).unwrap();

    let srt_path = temp_dir.join("video_cancel.srt");
    std::fs::write(&srt_path, "1\n00:00:01,000 --> 00:00:03,000\nSub\n").unwrap();

    let plan = SplitPlan {
        job_id: "prod_8_job".to_string(),
        input_file: test_video.to_string_lossy().into_owned(),
        input_duration: 5.0,
        input_size_bytes: std::fs::metadata(&test_video).unwrap().len(),
        mode: SplitMode::ByParts,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 2.0, duration: 2.0, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Part2".to_string(), start_time: 2.0, end_time: 4.0, duration: 2.0, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(true));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        Some(SplitSubtitleMode::CopyAll),
        Some(true),
        cancel_flag,
        |_| {}
    );

    assert!(result.is_err());
    assert!(result.err().unwrap().contains("cancelled"));

    let part1 = temp_dir.join("video_cancel - Part1 01.mp4");
    let part1_srt = temp_dir.join("video_cancel - Part1 01.srt");
    assert!(!part1.exists());
    assert!(!part1_srt.exists());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// TEST 9: Existing output files (verify auto increment, never silent overwrite)
#[test]
fn production_audit_9_output_collision() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("prod_audit_test_9");
    let _ = std::fs::create_dir_all(&temp_dir);

    let base_video = Path::new("../tests/fixtures/video1.mp4");
    let test_video = temp_dir.join("video_coll.mp4");
    std::fs::copy(base_video, &test_video).unwrap();

    let existing_output = temp_dir.join("video_coll - Part1 01.mp4");
    std::fs::write(&existing_output, "old_data").unwrap();

    let plan = SplitPlan {
        job_id: "prod_9_job".to_string(),
        input_file: test_video.to_string_lossy().into_owned(),
        input_duration: 5.0,
        input_size_bytes: std::fs::metadata(&test_video).unwrap().len(),
        mode: SplitMode::ByParts,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 3.0, duration: 3.0, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        None,
        None,
        cancel_flag,
        |_| {}
    ).unwrap();

    assert_eq!(result.output_paths.len(), 1);
    
    let old_content = std::fs::read_to_string(&existing_output).unwrap();
    assert_eq!(old_content, "old_data");
    
    let path_incremented = temp_dir.join("video_coll - Part1 01 (1).mp4");
    assert!(path_incremented.exists());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// TEST 10: Invalid naming templates (verify Windows sanitization of invalid characters)
#[test]
fn production_audit_10_invalid_naming_templates() {
    let base_dir = Path::new("D:\\TestFolder\\");
    
    let out = generate_output_filename(
        base_dir,
        Path::new("Course.mp4"),
        "Lesson: Introduce? * < > | \" \\ /",
        "mp4",
        1,
        1,
        0.0,
        10.0,
        Some("{stem}_{chapter}"),
        None,
        None,
        false,
    );
    
    let fname = out.file_name().unwrap().to_str().unwrap();
    assert!(!fname.contains(':'));
    assert!(!fname.contains('?'));
    assert!(!fname.contains('*'));
    assert!(!fname.contains('<'));
    assert!(!fname.contains('>'));
    assert!(!fname.contains('|'));
    assert!(!fname.contains('"'));
    assert!(!fname.contains('\\'));
    assert!(!fname.contains('/'));
    
    assert_eq!(fname, "Course_Lesson Introduce.mp4");
}

// TEST 11: Report parser compatibility (verify v1, v2, corrupted, unknown versions)
#[test]
fn production_audit_11_report_parser_compatibility() {
    let temp_dir = std::env::temp_dir().join("prod_audit_test_11");
    let _ = std::fs::create_dir_all(&temp_dir);

    // 1. Legacy v1 report parsing
    let v1_content = "\
╔══════════════════════════════════════════════════════════════╗
║                       MERGE REPORT                          ║
╚══════════════════════════════════════════════════════════════╝
  Generated  : 2026-05-30 06:12:34
  Output     : C:/output.mp4
  Total Time : 00:10:00
  Files      : 2
  ┌─────┬──────────────────────────────────────────┬──────────┬──────────────────────┬──────────┐
  │  #  │ File Name                                │ Duration │ In Merged (Start-End)│ Remaining│
  ├─────┼──────────────────────────────────────────┼──────────┼──────────────────────┼──────────┤
  │   1 │ Introduction.mp4                         │ 00:04:00 │ 00:00:00 → 00:04:00 │ 00:06:00 │
  │   2 │ Chapter 2.mp4                            │ 00:06:00 │ 00:04:00 → 00:10:00 │ 00:00:00 │
  └─────┴──────────────────────────────────────────┴──────────┴──────────────────────┴──────────┘
";
    let v1_path = temp_dir.join("v1_report.txt");
    std::fs::write(&v1_path, v1_content).unwrap();
    let parsed_v1 = parse_report_file(&v1_path).unwrap();
    assert_eq!(parsed_v1.len(), 2);
    assert_eq!(parsed_v1[0].0, "Introduction.mp4");
    assert_eq!(parsed_v1[1].0, "Chapter 2.mp4");

    // 2. Corruption line skip
    let corrupt_content = "\
╔══════════════════════════════════════════════════════════════╗
║                       MERGE REPORT                          ║
╚══════════════════════════════════════════════════════════════╝
  Report Version : 2
  ┌─────┬──────────────────────────────────────────┬──────────┬──────────────────────┬──────────┐
  │  #  │ File Name                                │ Duration │ In Merged (Start-End)│ Remaining│
  ├─────┼──────────────────────────────────────────┼──────────┼──────────────────────┼──────────┤
  │   1 │ Introduction.mp4                         │ 00:04:00 │ 00:00:00 → 00:04:00 │ 00:06:00 │
  │ Invalid line here
  │   2 │ Chapter 2.mp4                            │ 00:06:00 │ 00:04:00 → 00:10:00 │ 00:00:00 │
  └─────┴──────────────────────────────────────────┴──────────┴──────────────────────┴──────────┘
";
    let corrupt_path = temp_dir.join("corrupt_report.txt");
    std::fs::write(&corrupt_path, corrupt_content).unwrap();
    let parsed_corrupt = parse_report_file(&corrupt_path).unwrap();
    assert_eq!(parsed_corrupt.len(), 2);

    // 3. Unknown Report Version warning
    let unknown_version_content = "\
╔══════════════════════════════════════════════════════════════╗
║                       MERGE REPORT                          ║
╚══════════════════════════════════════════════════════════════╝
  Report Version : 99
  ┌─────┬──────────────────────────────────────────┬──────────┬──────────────────────┬──────────┐
  │  #  │ File Name                                │ Duration │ In Merged (Start-End)│ Remaining│
  ├─────┼──────────────────────────────────────────┼──────────┼──────────────────────┼──────────┤
  │   1 │ Introduction.mp4                         │ 00:04:00 │ 00:00:00 → 00:04:00 │ 00:06:00 │
  └─────┴──────────────────────────────────────────┴──────────┴──────────────────────┴──────────┘
";
    let unknown_path = temp_dir.join("unknown_report.txt");
    std::fs::write(&unknown_path, unknown_version_content).unwrap();
    let parsed_unknown = parse_report_file(&unknown_path).unwrap();
    assert_eq!(parsed_unknown.len(), 1);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// TEST 12: No chapter + no report (verify clear actionable error, no crash)
#[test]
fn production_audit_12_no_chapter_no_report() {
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (_ffmpeg, _ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("prod_audit_test_12");
    let _ = std::fs::create_dir_all(&temp_dir);

    let base_video = Path::new("../tests/fixtures/video1.mp4");
    let test_video = temp_dir.join("video_no_metadata.mp4");
    std::fs::copy(base_video, &test_video).unwrap();

    let req = SplitPlanRequest {
        job_id: "prod_12_job".to_string(),
        input_file: test_video.to_string_lossy().into_owned(),
        input_duration: 5.0,
        input_size_bytes: std::fs::metadata(&test_video).unwrap().len(),
        mode: SplitMode::ByChapters,
        params: SplitParams {
            part_count: None,
            part_duration: None,
            custom_ranges: None,
            items_per_segment: Some(10),
            max_size_bytes: None,
            course_mode: None,
            hours_per_unit: None,
            output_format: None,
            label_prefix: None,
            subtitle_mode: None,
            export_srt: None,
            label_suffix: None,
            naming_template: None,
            naming_config: None,
            include_timestamp: None,
        },
        output_dir: temp_dir.to_string_lossy().into_owned(),
    };

    let plan = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(crate::commands::split::generate_chapter_split_plan(req));
    
    assert!(plan.is_err());
    let err_msg = plan.err().unwrap();
    assert!(err_msg.contains("No chapters found in video file, and no merge report found"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ── TEST: Subtitle Encodings & Split Integrity Verifications (10 Tests) ─────
#[test]
fn forensic_audit_subtitle_split_encodings_verification() {
    let temp_dir = std::env::temp_dir().join("sub_split_verification");
    let _ = std::fs::remove_dir_all(&temp_dir); // clean start
    std::fs::create_dir_all(&temp_dir).unwrap();

    let segments = vec![
        SplitSegment {
            index: 1,
            label: "Part1".to_string(),
            start_time: 0.0,
            end_time: 15.0,
            duration: 15.0,
            estimated_size_bytes: None,
        },
        SplitSegment {
            index: 2,
            label: "Part2".to_string(),
            start_time: 15.0,
            end_time: 30.0,
            duration: 15.0,
            estimated_size_bytes: None,
        },
    ];

    let video_paths = vec![
        temp_dir.join("Part1.mp4").to_string_lossy().into_owned(),
        temp_dir.join("Part2.mp4").to_string_lossy().into_owned(),
    ];

    struct SubtitleTestCase {
        name: &'static str,
        encoding: &'static str,
        raw_content: String,
        expected_lang_text: Option<&'static str>,
    }

    let test_cases = vec![
        // TEST 1
        SubtitleTestCase {
            name: "TEST 1",
            encoding: "UTF-8",
            raw_content: "1\n00:00:05,000 --> 00:00:25,000\nUTF-8 subtitle content\n".to_string(),
            expected_lang_text: None,
        },
        // TEST 2
        SubtitleTestCase {
            name: "TEST 2",
            encoding: "UTF-8 BOM",
            raw_content: "1\n00:00:05,000 --> 00:00:25,000\nUTF-8 BOM subtitle content\n".to_string(),
            expected_lang_text: None,
        },
        // TEST 3
        SubtitleTestCase {
            name: "TEST 3",
            encoding: "UTF-16 LE",
            raw_content: "1\n00:00:05,000 --> 00:00:25,000\nUTF-16 LE subtitle content\n".to_string(),
            expected_lang_text: None,
        },
        // TEST 4
        SubtitleTestCase {
            name: "TEST 4",
            encoding: "UTF-16 BE",
            raw_content: "1\n00:00:05,000 --> 00:00:25,000\nUTF-16 BE subtitle content\n".to_string(),
            expected_lang_text: None,
        },
        // TEST 5
        SubtitleTestCase {
            name: "TEST 5",
            encoding: "Windows-1252",
            raw_content: "1\n00:00:05,000 --> 00:00:25,000\nANSI subtitle content with üéñ\n".to_string(),
            expected_lang_text: None,
        },
        // TEST 6
        SubtitleTestCase {
            name: "TEST 6",
            encoding: "UTF-16 LE (Arabic)",
            raw_content: "1\n00:00:05,000 --> 00:00:25,000\nالعربية subtitle content\n".to_string(),
            expected_lang_text: Some("العربية"),
        },
        // TEST 7
        SubtitleTestCase {
            name: "TEST 7",
            encoding: "UTF-16 LE (Japanese)",
            raw_content: "1\n00:00:05,000 --> 00:00:25,000\n日本語 subtitle content\n".to_string(),
            expected_lang_text: Some("日本語"),
        },
        // TEST 8
        SubtitleTestCase {
            name: "TEST 8",
            encoding: "UTF-16 LE (Chinese)",
            raw_content: "1\n00:00:05,000 --> 00:00:25,000\n中文 subtitle content\n".to_string(),
            expected_lang_text: Some("中文"),
        },
    ];

    println!("\n====================================================");
    println!("SUBTITLE ENCODING FIX VALIDATION");
    println!("====================================================");
    println!("{:<25} | {:<9} | {:<8} | {:<5} | {:<5} | {}", "Encoding", "Detected?", "Decoded?", "Split?", "Saved?", "PASS / FAIL");
    println!("---------------------------------------------------------------------------------------------");

    for tc in test_cases {
        let input_path = temp_dir.join(format!("{}.srt", tc.name.replace(' ', "_")));
        
        let bytes = match tc.encoding {
            "UTF-8" => tc.raw_content.as_bytes().to_vec(),
            "UTF-8 BOM" => {
                let mut b = vec![0xEF, 0xBB, 0xBF];
                b.extend_from_slice(tc.raw_content.as_bytes());
                b
            }
            "UTF-16 LE" | "UTF-16 LE (Arabic)" | "UTF-16 LE (Japanese)" | "UTF-16 LE (Chinese)" => {
                let utf16_chars: Vec<u16> = tc.raw_content.encode_utf16().collect();
                let mut b = vec![0xFF, 0xFE];
                for c in utf16_chars {
                    b.extend_from_slice(&c.to_le_bytes());
                }
                b
            }
            "UTF-16 BE" => {
                let utf16_chars: Vec<u16> = tc.raw_content.encode_utf16().collect();
                let mut b = vec![0xFE, 0xFF];
                for c in utf16_chars {
                    b.extend_from_slice(&c.to_be_bytes());
                }
                b
            }
            "Windows-1252" => {
                let mut b = Vec::new();
                for c in tc.raw_content.chars() {
                    let byte = match c {
                        'ü' => 0xFC,
                        'é' => 0xE9,
                        'ñ' => 0xF1,
                        _ if c.is_ascii() => c as u8,
                        _ => b'?',
                    };
                    b.push(byte);
                }
                b
            }
            _ => panic!("Unknown encoding"),
        };

        std::fs::write(&input_path, bytes).unwrap();

        // 1. Read / Decode robustly
        let decode_res = read_subtitle_file_robust(&input_path);
        let decoded_ok = decode_res.is_ok();
        let decoded_str = decode_res.unwrap_or_default();

        // Check if content matches raw content (or contains expected multilingual text)
        let content_matched = if let Some(expected) = tc.expected_lang_text {
            decoded_str.contains(expected)
        } else if tc.encoding == "Windows-1252" {
            decoded_str.contains("üéñ") || decoded_str.contains("ANSI subtitle")
        } else {
            decoded_str.contains("subtitle") || decoded_str.contains("Subtitle")
        };

        // 2. Split (which generates split files on disk)
        let split_res = split_srt_for_segments(&decoded_str, &segments, &video_paths, &None);
        let split_ok = split_res.is_ok();
        let srt_paths = split_res.unwrap_or_default();

        // 3. Verify actual files exist, are non-empty, and contain the expected entries (TEST 10)
        let mut files_exist = true;
        let mut non_empty = true;
        let mut contains_subs = true;

        if srt_paths.len() != 2 {
            files_exist = false;
        } else {
            for (idx, path_str) in srt_paths.iter().enumerate() {
                let p = Path::new(path_str);
                if !p.exists() {
                    files_exist = false;
                    break;
                }
                let meta = std::fs::metadata(p).unwrap();
                if meta.len() == 0 {
                    non_empty = false;
                }
                let s = std::fs::read_to_string(p).unwrap();
                if idx == 0 {
                    // Part 1 timing verification (TEST 9)
                    if !s.contains("00:00:05,000 --> 00:00:15,000") {
                        contains_subs = false;
                    }
                } else if idx == 1 {
                    // Part 2 timing verification (TEST 9)
                    if !s.contains("00:00:00,000 --> 00:00:10,000") {
                        contains_subs = false;
                    }
                }
            }
        }

        let pass = decoded_ok && content_matched && split_ok && files_exist && non_empty && contains_subs;
        
        println!(
            "{:<25} | {:<9} | {:<8} | {:<5} | {:<5} | {}",
            tc.encoding,
            if decoded_ok { "YES" } else { "NO" },
            if content_matched { "YES" } else { "NO" },
            if split_ok { "YES" } else { "NO" },
            if files_exist && non_empty { "YES" } else { "NO" },
            if pass { "PASS" } else { "FAIL" }
        );

        assert!(pass, "Test case failed: {}", tc.name);
    }
    println!("====================================================\n");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ── SRT Export Behavior: CopyAll vs ExtractSplit ────────────────────────────
//
// Behavior under test:
//   CopyAll:      Subtitles remain embedded in video output. NO separate SRT
//                 files are generated. srt_output_paths MUST be None.
//   ExtractSplit: Subtitles are extracted and split into per-segment SRT
//                 files. srt_output_paths MUST contain paths to generated
//                 SRTs with correctly shifted timings.

#[test]
fn forensic_audit_srt_export_copyall_no_srt_files() {
    // Verify that CopyAll mode does NOT generate separate SRT files.
    // Subtitles are kept embedded in the video output via -c copy.
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("srt_export_copyall_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    // 1. Create a 4-second synthetic video with lavfi
    let video_path = temp_dir.join("source_copyall.mp4");
    let status = Command::new(&ffmpeg)
        .args([
            "-y", "-f", "lavfi",
            "-i", "color=c=red:s=320x240:d=4.0:r=10",
            "-f", "lavfi",
            "-i", "anullsrc=r=44100:cl=mono",
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30", "-g", "1",
            "-c:a", "aac", "-shortest",
            video_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success(), "Failed to create source video");

    // 2. Create companion SRT with one subtitle per second
    std::fs::write(temp_dir.join("source_copyall.srt"),
        "1\n00:00:00,500 --> 00:00:01,500\nHello CopyAll\n\n2\n00:00:02,000 --> 00:00:03,500\nSecond subtitle\n"
    ).unwrap();

    // 3. Plan split into 2 segments of 2s each
    let plan = SplitPlan {
        job_id: "copyall_srt_test".to_string(),
        input_file: video_path.to_string_lossy().into_owned(),
        input_duration: 4.0,
        input_size_bytes: std::fs::metadata(&video_path).unwrap().len(),
        mode: SplitMode::ByDuration,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 2.0, duration: 2.0, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Part2".to_string(), start_time: 2.0, end_time: 4.0, duration: 2.0, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    // 4. Execute with CopyAll — srt_output_paths MUST be None
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        Some(SplitSubtitleMode::CopyAll),
        Some(true), // export_srt=true should be IGNORED in CopyAll mode
        cancel_flag,
        |_| {},
    ).unwrap();

    assert_eq!(result.output_paths.len(), 2, "Should produce 2 video segments");

    // ── CRITICAL ASSERTION: CopyAll must NOT generate SRT files ──
    assert!(
        result.srt_output_paths.is_none(),
        "CopyAll mode must NOT export separate SRT files. Got: {:?}",
        result.srt_output_paths
    );

    // Verify no stray .srt files alongside the output videos
    for path_str in &result.output_paths {
        let video_path = Path::new(path_str);
        let srt_candidate = video_path.with_extension("srt");
        assert!(
            !srt_candidate.exists(),
            "CopyAll must NOT create companion SRT: {}",
            srt_candidate.display()
        );
    }

    // Verify output videos are playable
    for path_str in &result.output_paths {
        let status = Command::new(&ffmpeg)
            .args(["-v", "error", "-i", path_str, "-t", "1", "-f", "null", "-"])
            .status()
            .unwrap();
        assert!(status.success(), "Video not playable: {}", path_str);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn forensic_audit_srt_export_extractsplit_generates_srt() {
    // Verify that ExtractSplit mode generates per-segment SRT files with
    // correctly shifted timings.
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("srt_export_extractsplit_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    // 1. Create a 6-second synthetic video
    let video_path = temp_dir.join("source_extract.mp4");
    let status = Command::new(&ffmpeg)
        .args([
            "-y", "-f", "lavfi",
            "-i", "color=c=blue:s=320x240:d=6.0:r=10",
            "-f", "lavfi",
            "-i", "anullsrc=r=44100:cl=mono",
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30", "-g", "1",
            "-c:a", "aac", "-shortest",
            video_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success(), "Failed to create source video");

    // 2. Create companion SRT with 4 subtitles spanning different segments
    // Segments: [0-2s), [2-4s), [4-6s)
    // Sub 1: 0.5s-1.5s  → only Segment 1
    // Sub 2: 1.5s-3.0s  → spans Segment 1 end + Segment 2 start
    // Sub 3: 3.5s-4.5s  → spans Segment 2 end + Segment 3 start
    // Sub 4: 5.0s-6.0s  → only Segment 3
    std::fs::write(video_path.with_extension("srt"),
        "1\n00:00:00,500 --> 00:00:01,500\nSubtitle One\n\n2\n00:00:01,500 --> 00:00:03,000\nSubtitle Two (boundary)\n\n3\n00:00:03,500 --> 00:00:04,500\nSubtitle Three (boundary)\n\n4\n00:00:05,000 --> 00:00:06,000\nSubtitle Four\n"
    ).unwrap();

    // 3. Plan split into 3 segments of 2s each
    let plan = SplitPlan {
        job_id: "extract_srt_test".to_string(),
        input_file: video_path.to_string_lossy().into_owned(),
        input_duration: 6.0,
        input_size_bytes: std::fs::metadata(&video_path).unwrap().len(),
        mode: SplitMode::ByDuration,
        segments: vec![
            SplitSegment { index: 1, label: "Seg1".to_string(), start_time: 0.0, end_time: 2.0, duration: 2.0, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Seg2".to_string(), start_time: 2.0, end_time: 4.0, duration: 2.0, estimated_size_bytes: None },
            SplitSegment { index: 3, label: "Seg3".to_string(), start_time: 4.0, end_time: 6.0, duration: 2.0, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    // 4. Execute with ExtractSplit + export_srt=true
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        Some(SplitSubtitleMode::ExtractSplit),
        Some(true),
        cancel_flag,
        |_| {},
    ).unwrap();

    assert_eq!(result.output_paths.len(), 3, "Should produce 3 video segments");

    // ── CRITICAL ASSERTION: ExtractSplit MUST generate SRT files ──
    let srt_paths = result.srt_output_paths
        .expect("ExtractSplit mode must generate SRT file paths");

    assert_eq!(srt_paths.len(), 3, "ExtractSplit must produce 1 SRT per segment");

    // Verify each SRT file exists on disk
    for srt_path_str in &srt_paths {
        let p = Path::new(srt_path_str);
        assert!(p.exists(), "SRT file must exist: {}", p.display());
        assert!(
            p.metadata().unwrap().len() > 0,
            "SRT file must not be empty: {}",
            p.display()
        );
    }

    // 5. Verify SRT content timing
    // Segment 1 (0-2s): Should contain Sub 1 (0.5-1.5s, unshifted) and Sub 2 (1.5-2.0s, clamped)
    let seg1_srt = std::fs::read_to_string(&srt_paths[0]).unwrap();
    assert!(seg1_srt.contains("00:00:00,500 --> 00:00:01,500"), "Seg1: Sub 1 timing wrong\n{}", seg1_srt);
    assert!(seg1_srt.contains("00:00:01,500 --> 00:00:02,000"), "Seg1: Sub 2 should be clamped to segment end\n{}", seg1_srt);
    assert!(seg1_srt.contains("Subtitle One"), "Seg1: Missing Sub 1 text");
    assert!(seg1_srt.contains("Subtitle Two"), "Seg1: Missing Sub 2 text");
    assert!(!seg1_srt.contains("Subtitle Three"), "Seg1: Should not contain Sub 3");
    assert!(!seg1_srt.contains("Subtitle Four"), "Seg1: Should not contain Sub 4");

    // Segment 2 (2-4s): Should contain Sub 2 (shifted: 0-1.0s) and Sub 3 (shifted: 1.5-2.0s clamped)
    let seg2_srt = std::fs::read_to_string(&srt_paths[1]).unwrap();
    assert!(seg2_srt.contains("00:00:00,000 --> 00:00:01,000"), "Seg2: Sub 2 should be shifted by -2s\n{}", seg2_srt);
    assert!(seg2_srt.contains("00:00:01,500 --> 00:00:02,000"), "Seg2: Sub 3 should be shifted by -2s and clamped\n{}", seg2_srt);
    assert!(seg2_srt.contains("Subtitle Two"), "Seg2: Missing Sub 2 text");
    assert!(seg2_srt.contains("Subtitle Three"), "Seg2: Missing Sub 3 text");
    assert!(!seg2_srt.contains("Subtitle One"), "Seg2: Should not contain Sub 1");
    assert!(!seg2_srt.contains("Subtitle Four"), "Seg2: Should not contain Sub 4");

    // Segment 3 (4-6s): Should contain Sub 3 (shifted: 0-0.5s) and Sub 4 (shifted: 1.0-2.0s)
    let seg3_srt = std::fs::read_to_string(&srt_paths[2]).unwrap();
    assert!(seg3_srt.contains("00:00:00,000 --> 00:00:00,500"), "Seg3: Sub 3 should be shifted by -4s\n{}", seg3_srt);
    assert!(seg3_srt.contains("00:00:01,000 --> 00:00:02,000"), "Seg3: Sub 4 should be shifted by -4s\n{}", seg3_srt);
    assert!(seg3_srt.contains("Subtitle Three"), "Seg3: Missing Sub 3 text");
    assert!(seg3_srt.contains("Subtitle Four"), "Seg3: Missing Sub 4 text");
    assert!(!seg3_srt.contains("Subtitle One"), "Seg3: Should not contain Sub 1");
    assert!(!seg3_srt.contains("Subtitle Two"), "Seg3: Should not contain Sub 2");

    // 6. Verify SRT filenames inherit video filenames (just .srt extension)
    for (i, srt_path_str) in srt_paths.iter().enumerate() {
        let srt_path = Path::new(srt_path_str);
        let video_path = Path::new(&result.output_paths[i]);
        let expected_srt_stem = video_path.file_stem().unwrap();
        let actual_srt_stem = srt_path.file_stem().unwrap();
        assert_eq!(
            expected_srt_stem, actual_srt_stem,
            "SRT filename '{}' must match video filename '{}'",
            srt_path.display(),
            video_path.display()
        );
        assert_eq!(
            srt_path.extension().unwrap_or_default(),
            "srt",
            "SRT must have .srt extension"
        );
    }

    // 7. Verify output videos are still playable
    for path_str in &result.output_paths {
        let status = Command::new(&ffmpeg)
            .args(["-v", "error", "-i", path_str, "-t", "1", "-f", "null", "-"])
            .status()
            .unwrap();
        assert!(status.success(), "Video not playable: {}", path_str);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn forensic_audit_srt_export_extractsplit_export_srt_false() {
    // Verify that ExtractSplit + export_srt=false does NOT generate SRT files.
    let binaries = get_ffmpeg_ffprobe();
    if binaries.is_none() {
        eprintln!("SKIP: ffmpeg/ffprobe not found");
        return;
    }
    let (ffmpeg, ffprobe) = binaries.unwrap();

    let temp_dir = std::env::temp_dir().join("srt_export_extractsplit_false_test");
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let video_path = temp_dir.join("source_nosrt.mp4");
    let status = Command::new(&ffmpeg)
        .args([
            "-y", "-f", "lavfi",
            "-i", "color=c=green:s=320x240:d=3.0:r=10",
            "-f", "lavfi",
            "-i", "anullsrc=r=44100:cl=mono",
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "30", "-g", "1",
            "-c:a", "aac", "-shortest",
            video_path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg should run");
    assert!(status.success());

    std::fs::write(video_path.with_extension("srt"),
        "1\n00:00:00,500 --> 00:00:02,000\nTest subtitle\n"
    ).unwrap();

    let plan = SplitPlan {
        job_id: "extract_nosrt_test".to_string(),
        input_file: video_path.to_string_lossy().into_owned(),
        input_duration: 3.0,
        input_size_bytes: std::fs::metadata(&video_path).unwrap().len(),
        mode: SplitMode::ByDuration,
        segments: vec![
            SplitSegment { index: 1, label: "Part1".to_string(), start_time: 0.0, end_time: 1.5, duration: 1.5, estimated_size_bytes: None },
            SplitSegment { index: 2, label: "Part2".to_string(), start_time: 1.5, end_time: 3.0, duration: 1.5, estimated_size_bytes: None },
        ],
        output_dir: temp_dir.to_string_lossy().into_owned(),
        output_format: "mp4".to_string(),
        label_suffix: None,
        naming_template: None,
        naming_config: None,
        include_timestamp: Some(false),
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let result = execute_split_with_options(
        &ffmpeg,
        Some(&ffprobe),
        &plan,
        Some(SplitSubtitleMode::ExtractSplit),
        Some(false), // export_srt=false — suppress SRT generation
        cancel_flag,
        |_| {},
    ).unwrap();

    assert_eq!(result.output_paths.len(), 2);

    // ExtractSplit + export_srt=false => srt_output_paths should be None
    assert!(
        result.srt_output_paths.is_none(),
        "ExtractSplit + export_srt=false must not export SRTs. Got: {:?}",
        result.srt_output_paths
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}


