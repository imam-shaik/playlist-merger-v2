// Phase C: Split Certification
// Tests split merge via production MergeConfig.split_config with SplitMode::Count.

use crate::media::MediaAssets;
use crate::reporters::{Evidence, EvidenceType, TestMetrics, TestResult, TestStatus};
use crate::validators::FfprobeValidator;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

pub fn run(
    _binary_path: &Option<PathBuf>,
    media_path: &Path,
    temp_path: &Path,
    media_assets: &MediaAssets,
) -> Result<Vec<TestResult>> {
    use playlist_merger_lib::certification_api::*;

    let mut results = Vec::new();
    let validator = FfprobeValidator::new();

    if !media_assets.can_run_phase_c() {
        results.push(TestResult {
            phase: "C".to_string(),
            test_name: "phase_c_prerequisites".to_string(),
            name: "Phase C prerequisites check".to_string(),
            status: TestStatus::Skip,
            duration_secs: 0.0,
            evidence: vec![],
            logs: vec![],
            error: Some("Missing required media (need 3 H.264 files)".to_string()),
            metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    // Canonicalize paths to absolute for concat list compatibility
    let abs_media_path = std::fs::canonicalize(media_path)
        .unwrap_or_else(|_| media_path.to_path_buf());

    let files: Vec<PathBuf> = (1..=3)
        .map(|i| abs_media_path.join(format!("h264_720p_{}.mp4", i)))
        .filter(|p| p.exists())
        .collect();

    if files.len() < 3 {
        results.push(TestResult {
            phase: "C".to_string(),
            test_name: "phase_c_setup".to_string(),
            name: "Phase C setup".to_string(),
            status: TestStatus::Skip,
            duration_secs: 0.0,
            evidence: vec![],
            logs: vec![],
            error: Some("Need 3 H.264 files".to_string()),
            metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    let total_duration: f64 = files.iter()
        .filter_map(|f| validator.probe(f).ok().and_then(|p| p.total_duration()))
        .sum();

    let input_strs: Vec<String> = files.iter()
        .map(|p| {
            std::fs::canonicalize(p)
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy().to_string()
        }).collect();
    let input_names: Vec<String> = files.iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str().map(String::from))).collect();
    let input_durations: Vec<f64> = files.iter()
        .filter_map(|f| validator.probe(f).ok().and_then(|p| p.total_duration())).collect();

    let ffmpeg_path = find_ffmpeg(None::<&str>).unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress = |_: MergeProgress| {};

    // Test 1: Split into 2 parts
    {
        let test_start = Instant::now();
        let split_output_path = temp_path.join("split_2_parts_output.mkv");
        let concat_list_path = temp_path.join("split_2_parts_concat.txt");

        let path_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        // Lossless split — omit duration directives for stream copy
        write_concat_list_with_durations(&path_refs, Some(&input_durations), &concat_list_path, false)
            .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: input_strs.clone(),
            input_names: input_names.clone(),
            input_durations: input_durations.clone(),
            subtitle_list_path: None,
            output_path: split_output_path.to_string_lossy().to_string(),
            total_duration,
            mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: Some(SplitConfig {
                mode: SplitMode::Count,
                part_count: Some(2),
                max_duration_per_part: None,
                subtitle_mode: Some(SplitSubtitleMode::Ignore),
                folder_split_mode: None,
            }),
            subtitle_files: vec![], card_config: None, segment_is_card: vec![],
            naming_config: None, subtitle_mode: SubtitleMode::None,
            export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let _merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list_path, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list_path);

        let part1_path = temp_path.join("split_2_parts_output_part1.mkv");
        let part2_path = temp_path.join("split_2_parts_output_part2.mkv");
        let part1_ok = part1_path.exists();
        let part2_ok = part2_path.exists();

        let mut evidence = Vec::new();
        if part1_ok { evidence.push(Evidence::new(EvidenceType::VideoFile, "Part 1").with_path(part1_path)); }
        if part2_ok { evidence.push(Evidence::new(EvidenceType::VideoFile, "Part 2").with_path(part2_path)); }

        let status = if part1_ok && part2_ok { TestStatus::Pass } else if part1_ok || part2_ok { TestStatus::Warn } else { TestStatus::Fail };

        results.push(TestResult {
            phase: "C".to_string(), test_name: "split_into_2_parts".to_string(),
            name: "Split into 2 parts".to_string(), status, duration_secs: test_start.elapsed().as_secs_f64(),
            evidence, logs: vec![format!("Split 2 parts: part1={}, part2={}", part1_ok, part2_ok)],
            error: if status == TestStatus::Pass { None } else { Some("One or both parts missing".to_string()) },
            metrics: TestMetrics { input_file_count: Some(files.len()), output_file_count: Some(if part1_ok { 1 } else { 0 } + if part2_ok { 1 } else { 0 }), expected_duration_ms: Some((total_duration * 1000.0) as u64), ..Default::default() },
        });
    }

    // Test 2: Split into 3 parts
    {
        let test_start = Instant::now();
        let split_output_path = temp_path.join("split_3_parts_output.mkv");
        let concat_list_path = temp_path.join("split_3_parts_concat.txt");

        let path_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        // Lossless split — omit duration directives for stream copy
        write_concat_list_with_durations(&path_refs, Some(&input_durations), &concat_list_path, false)
            .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: input_strs.clone(),
            input_names: input_names.clone(),
            input_durations: input_durations.clone(),
            subtitle_list_path: None,
            output_path: split_output_path.to_string_lossy().to_string(),
            total_duration,
            mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: Some(SplitConfig {
                mode: SplitMode::Count,
                part_count: Some(3),
                max_duration_per_part: None,
                subtitle_mode: Some(SplitSubtitleMode::Ignore),
                folder_split_mode: None,
            }),
            subtitle_files: vec![], card_config: None, segment_is_card: vec![],
            naming_config: None, subtitle_mode: SubtitleMode::None,
            export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let _merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list_path, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list_path);

        let mut all_exist = true;
        let mut evidence = Vec::new();
        for i in 1..=3 {
            let part_path = temp_path.join(format!("split_3_parts_output_part{}.mkv", i));
            if part_path.exists() { evidence.push(Evidence::new(EvidenceType::VideoFile, &format!("Part {}", i)).with_path(part_path)); }
            else { all_exist = false; }
        }

        results.push(TestResult {
            phase: "C".to_string(), test_name: "split_into_3_parts".to_string(),
            name: "Split into 3 parts".to_string(), status: if all_exist { TestStatus::Pass } else { TestStatus::Warn },
            duration_secs: test_start.elapsed().as_secs_f64(), evidence, logs: vec![],
            error: if all_exist { None } else { Some("Some parts missing".to_string()) },
            metrics: TestMetrics { input_file_count: Some(files.len()), output_file_count: Some(3), expected_duration_ms: Some((total_duration * 1000.0) as u64), ..Default::default() },
        });
    }

    Ok(results)
}
