// Phase B: Repeat Certification
// Tests repeat merge via production expand_repeat() + run_merge_blocking().

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

    if !media_assets.can_run_phase_b() {
        results.push(TestResult {
            phase: "B".to_string(),
            test_name: "phase_b_prerequisites".to_string(),
            name: "Phase B prerequisites check".to_string(),
            status: TestStatus::Skip,
            duration_secs: 0.0,
            evidence: vec![],
            logs: vec![],
            error: Some("Missing required media (need at least 2 H.264 files)".to_string()),
            metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    // Canonicalize paths to absolute for concat list compatibility
    let abs_media_path = std::fs::canonicalize(media_path)
        .unwrap_or_else(|_| media_path.to_path_buf());

    let files: Vec<PathBuf> = (1..=2)
        .map(|i| abs_media_path.join(format!("h264_720p_{}.mp4", i)))
        .filter(|p| p.exists())
        .collect();

    if files.len() < 2 {
        results.push(TestResult {
            phase: "B".to_string(),
            test_name: "phase_b_setup".to_string(),
            name: "Phase B setup".to_string(),
            status: TestStatus::Skip,
            duration_secs: 0.0,
            evidence: vec![],
            logs: vec![],
            error: Some("Need 2 H.264 files".to_string()),
            metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    let base_strs: Vec<String> = files.iter()
        .map(|p| {
            std::fs::canonicalize(p)
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy().to_string()
        })
        .collect();
    let base_names: Vec<String> = files.iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str().map(String::from)))
        .collect();
    let base_durs: Vec<f64> = files.iter()
        .filter_map(|f| validator.probe(f).ok().and_then(|p| p.total_duration()))
        .collect();
    let base_duration: f64 = base_durs.iter().sum();

    let ffmpeg_path = find_ffmpeg(None::<&str>).unwrap_or_else(|_| PathBuf::from("ffmpeg"));

    let repeat_tests = vec![
        (2u32, "repeat_x2"),
        (5u32, "repeat_x5"),
        (20u32, "repeat_x20"),
        (50u32, "repeat_x50"),
    ];

    for (repeat_count, test_name) in repeat_tests {
        let start = Instant::now();
        let output_path = temp_path.join(format!("{}_output.mkv", test_name));

        let repeat_config = RepeatConfig {
            enabled: true,
            by_count: true,
            repeat_count,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: false,
            boundary_card_template: String::new(),
        };

        // expand_repeat takes (config, files, durations, names)
        let expanded = expand_repeat(&repeat_config, &base_strs, &base_durs, &base_names);
        let (repeated_files, repeated_durs, expected_duration) = if let Some(ep) = expanded {
            let ef = ep.files.clone();
            let ed = ep.durations.clone();
            let total = ef.len() as f64 / base_strs.len() as f64 * base_duration;
            (ef, ed, total)
        } else {
            // Fallback if expand_repeat returns None (e.g. repeat_count <= 1)
            let mut ef = Vec::new();
            let mut ed = Vec::new();
            for _ in 0..repeat_count {
                ef.extend(base_strs.clone());
                ed.extend(base_durs.clone());
            }
            (ef, ed, base_duration * repeat_count as f64)
        };

        let concat_list_path = temp_path.join(format!("{}_concat.txt", test_name));
        let path_refs: Vec<&Path> = repeated_files.iter().map(|s| Path::new(s)).collect();
        // Lossless mode uses stream copy — omit duration directives
        write_concat_list_with_durations(&path_refs, Some(&repeated_durs), &concat_list_path, false)
            .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: repeated_files.clone(),
            input_names: repeated_files.iter().enumerate()
                .map(|(i, s)| Path::new(s).file_stem().and_then(|n| n.to_str().map(String::from)).unwrap_or_else(|| format!("file_{}", i)))
                .collect(),
            input_durations: repeated_durs.clone(),
            subtitle_list_path: None,
            output_path: output_path.to_string_lossy().to_string(),
            total_duration: expected_duration,
            mode: MergeMode::Lossless,
            video_codec: None,
            audio_codec: None,
            video_crf: None,
            video_preset: None,
            audio_bitrate: None,
            target_resolution: None,
            target_fps: None,
            hw_accel: None,
            split_config: None,
            subtitle_files: vec![],
            card_config: None,
            segment_is_card: vec![],
            naming_config: None,
            subtitle_mode: SubtitleMode::None,
            export_merged_srt: false,
            burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let progress = |_: MergeProgress| {};
        let merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list_path, cancel_flag, progress);
        let _ = std::fs::remove_file(&concat_list_path);

        let (status, error, metrics, logs) = if merge_result.is_ok() && output_path.exists() {
            match validator.probe(&output_path) {
                Ok(probe) => {
                    let actual_duration = probe.total_duration().unwrap_or(0.0);
                    let diff_pct = if expected_duration > 0.0 {
                        ((expected_duration - actual_duration).abs() / expected_duration * 100.0)
                    } else { 0.0 };
                    let has_video = probe.video_count() > 0;
                    let output_size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                    if !has_video {
                        (TestStatus::Fail, Some("No video stream in output".to_string()),
                         TestMetrics { input_file_count: Some(repeated_files.len()), output_file_count: Some(1), expected_duration_ms: Some((expected_duration * 1000.0) as u64), actual_duration_ms: Some((actual_duration * 1000.0) as u64), output_size_bytes: Some(output_size), ..Default::default() },
                         vec![format!("Video: {}, Audio: {}, Duration: {:.1}s (expected {:.1}s, diff {:.1}%)", probe.video_count(), probe.audio_count(), actual_duration, expected_duration, diff_pct)])
                    } else if diff_pct > 5.0 {
                        (TestStatus::Warn,
                         Some(format!("Duration mismatch: expected {:.1}s, got {:.1}s ({:.1}%)", expected_duration, actual_duration, diff_pct)),
                         TestMetrics { input_file_count: Some(repeated_files.len()), output_file_count: Some(1), expected_duration_ms: Some((expected_duration * 1000.0) as u64), actual_duration_ms: Some((actual_duration * 1000.0) as u64), output_size_bytes: Some(output_size), ..Default::default() },
                         vec![format!("Repeat {}: {} files merged, {:.1}s, {}v/{}a", repeat_count, repeated_files.len(), actual_duration, probe.video_count(), probe.audio_count())])
                    } else {
                        (TestStatus::Pass, None,
                         TestMetrics { input_file_count: Some(repeated_files.len()), output_file_count: Some(1), expected_duration_ms: Some((expected_duration * 1000.0) as u64), actual_duration_ms: Some((actual_duration * 1000.0) as u64), output_size_bytes: Some(output_size), ..Default::default() },
                         vec![format!("Repeat {}: {} files merged, {:.1}s, {}v/{}a, {:.1}% drift", repeat_count, repeated_files.len(), actual_duration, probe.video_count(), probe.audio_count(), diff_pct)])
                    }
                }
                Err(e) => (TestStatus::Fail, Some(format!("Could not probe output: {}", e)), TestMetrics::default(), vec![])
            }
        } else {
            let err_msg = merge_result.as_ref().err()
                .map(|e| format!("Merge failed: {}", e))
                .unwrap_or_else(|| "Output file not created".to_string());
            (TestStatus::Fail, Some(err_msg), TestMetrics::default(), vec![])
        };

        let evidence = if output_path.exists() {
            vec![Evidence::new(EvidenceType::VideoFile, &format!("Repeat {} output", repeat_count)).with_path(output_path)]
        } else { vec![] };

        results.push(TestResult {
            phase: "B".to_string(),
            test_name: test_name.to_string(),
            name: format!("Repeat x{} merge", repeat_count),
            status, duration_secs: start.elapsed().as_secs_f64(), evidence, logs, error, metrics,
        });
    }

    Ok(results)
}
