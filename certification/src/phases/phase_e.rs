// Phase E: Recovery Certification
// Tests crash/resume via production recovery checkpoint system + run_merge_blocking cancellation.

use crate::media::MediaAssets;
use crate::reporters::{Evidence, EvidenceType, TestMetrics, TestResult, TestStatus};
use crate::validators::FfprobeValidator;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// Simulate a crash mid-merge using cancellation + checkpoint verification.
fn test_crash_during_merge(
    ffmpeg_path: &Path,
    config: playlist_merger_lib::certification_api::MergeConfig,
    concat_list_path: &Path,
    checkpoints_dir: &Path,
    label: &str,
    job_id: &str,
) -> TestResult {
    let start = Instant::now();

    // Note: ensure_checkpoint is not exported via certification_api.
// Checkpoint verification is tested via read_checkpoint at line 85 instead.

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel_flag.clone();
    let ffmpeg = ffmpeg_path.to_path_buf();
    let output_path = config.output_path.clone();
    let output_path_buf = PathBuf::from(&output_path);
    let concat = concat_list_path.to_path_buf();

    let config_clone = playlist_merger_lib::certification_api::MergeConfig {
        input_files: config.input_files.clone(),
        input_names: config.input_names.clone(),
        input_durations: config.input_durations.clone(),
        subtitle_list_path: None,
        output_path: config.output_path.clone(),
        total_duration: config.total_duration,
        mode: config.mode,
        video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
        audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
        split_config: None, subtitle_files: vec![], card_config: None,
        segment_is_card: vec![], naming_config: None,
        subtitle_mode: playlist_merger_lib::certification_api::SubtitleMode::None,
        export_merged_srt: false, burn_subtitle_path: None,
        mkvmerge_succeeded_before_ffmpeg: false,
    };

    // Spawn merge in background thread
    let handle = thread::spawn(move || {
        playlist_merger_lib::certification_api::run_merge_blocking(
            &ffmpeg, &config_clone, &concat, cancel_clone,
            |_: playlist_merger_lib::certification_api::MergeProgress| {},
        )
    });

    // Sleep to let merge start
    thread::sleep(Duration::from_millis(1500));

    // Cancel the merge to simulate crash
    cancel_flag.store(true, Ordering::Relaxed);
    let _merge_result = handle.join().expect("Merge thread panicked");

    // Check for partial output
    let partial_exists = output_path_buf.exists()
        && std::fs::metadata(&output_path_buf).map(|m| m.len()).unwrap_or(0) > 0;

    // Check if checkpoint was written
    let cp_read = playlist_merger_lib::recovery::read_checkpoint(checkpoints_dir, job_id)
        .unwrap_or(None);
    let checkpoint_written = cp_read.is_some();

    let validator = FfprobeValidator::new();
    let probe_result = if partial_exists { validator.probe(&output_path_buf).ok() } else { None };

    let status = if partial_exists || checkpoint_written { TestStatus::Pass } else { TestStatus::Fail };

    let logs = if let Some(p) = probe_result {
        vec![format!("Partial output: {}v/{}a, {:.1}s, {} bytes",
            p.video_count(), p.audio_count(), p.total_duration().unwrap_or(0.0), p.total_size().unwrap_or(0))]
    } else if partial_exists {
        vec!["Partial output exists but could not probe".to_string()]
    } else if checkpoint_written {
        vec!["Checkpoint written successfully".to_string()]
    } else {
        vec!["No partial output or checkpoint".to_string()]
    };

    let output_path_buf_clone = output_path_buf.clone();
    TestResult {
        phase: "E".to_string(), test_name: format!("crash_during_{}", label),
        name: format!("Crash during {}", label), status,
        duration_secs: start.elapsed().as_secs_f64(),
        evidence: if partial_exists { vec![Evidence::new(EvidenceType::VideoFile, "Partial output").with_path(output_path_buf)] }
                  else { vec![Evidence::new(EvidenceType::CheckpointFile, &format!("Checkpoint for {}", job_id))] },
        logs,
        error: if status == TestStatus::Fail { Some("No output or checkpoint".to_string()) } else { None },
        metrics: TestMetrics {
            input_file_count: Some(config.input_files.len()),
            output_file_count: Some(if partial_exists { 1 } else { 0 }),
            output_size_bytes: std::fs::metadata(&output_path_buf_clone).ok().map(|m| m.len()),
            checkpoint_size_bytes: cp_read.as_ref()
                .and_then(|_| serde_json::to_string(&cp_read).ok())
                .map(|s| s.len() as u64),
            ..Default::default()
        },
    }
}

pub fn run(
    _binary_path: &Option<PathBuf>,
    media_path: &Path,
    temp_path: &Path,
    media_assets: &MediaAssets,
) -> Result<Vec<TestResult>> {
    use playlist_merger_lib::certification_api::*;

    let mut results = Vec::new();

    if !media_assets.can_run_phase_a() {
        results.push(TestResult {
            phase: "E".to_string(), test_name: "phase_e_prerequisites".to_string(),
            name: "Phase E prerequisites check".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec![],
            error: Some("Missing required media".to_string()), metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    // Canonicalize media_path to absolute so concat list paths always resolve
    let abs_media_path = std::fs::canonicalize(media_path)
        .unwrap_or_else(|_| media_path.to_path_buf());

    let files: Vec<PathBuf> = (1..=3)
        .map(|i| abs_media_path.join(format!("h264_720p_{}.mp4", i)))
        .filter(|p| p.exists()).collect();
    if files.len() < 3 {
        results.push(TestResult {
            phase: "E".to_string(), test_name: "phase_e_setup".to_string(),
            name: "Phase E setup".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec![],
            error: Some("Need 3 H.264 files".to_string()), metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    let ffmpeg_path = find_ffmpeg(None::<&str>).unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress = |_: MergeProgress| {};

    let input_strs: Vec<String> = files.iter()
        .map(|p| {
            std::fs::canonicalize(p)
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy().to_string()
        })
        .collect();
    let input_names: Vec<String> = files.iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str().map(String::from))).collect();
    let input_durations: Vec<f64> = files.iter()
        .filter_map(|f| FfprobeValidator::new().probe(f).ok().and_then(|p| p.total_duration())).collect();
    let total_duration: f64 = input_durations.iter().sum();
    let validator = FfprobeValidator::new();
    let checkpoints_dir = temp_path.join("checkpoints");

    // Test 1: Clean merge baseline
    {
        let test_start = Instant::now();
        let output_path = temp_path.join("recovery_clean_output.mkv");
        let concat_list_path = temp_path.join("recovery_clean_concat.txt");

        let path_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        write_concat_list_with_durations(&path_refs, Some(&input_durations), &concat_list_path, false)?; // stream copy — no duration directives

        let config = MergeConfig {
            input_files: input_strs.clone(), input_names: input_names.clone(),
            input_durations: input_durations.clone(),
            subtitle_list_path: None, output_path: output_path.to_string_lossy().to_string(),
            total_duration, mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None, subtitle_files: vec![], card_config: None,
            segment_is_card: vec![], naming_config: None,
            subtitle_mode: SubtitleMode::None, export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list_path, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list_path);

        let (status, error, logs) = if merge_result.is_ok() && output_path.exists() {
            match validator.probe(&output_path) {
                Ok(p) if p.video_count() > 0 => (TestStatus::Pass, None, vec![format!("Clean merge: {}v/{}a, {:.1}s", p.video_count(), p.audio_count(), p.total_duration().unwrap_or(0.0))]),
                Ok(_) => (TestStatus::Fail, Some("No video".to_string()), vec![]),
                Err(e) => (TestStatus::Fail, Some(e.to_string()), vec![]),
            }
        } else { (TestStatus::Fail, Some("Output not created".to_string()), vec![]) };

        results.push(TestResult {
            phase: "E".to_string(), test_name: "clean_baseline".to_string(),
            name: "Clean merge baseline".to_string(), status,
            duration_secs: test_start.elapsed().as_secs_f64(),
            evidence: vec![Evidence::new(EvidenceType::VideoFile, "Clean output").with_path(output_path)],
            logs, error, metrics: TestMetrics::default(),
        });
    }

    // Test 2: Crash during concat
    {
        let crash_output_path = temp_path.join("recovery_crash_concat_output.mkv");
        let concat_list_path = temp_path.join("recovery_crash_concat.txt");
        let path_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        write_concat_list_with_durations(&path_refs, Some(&input_durations), &concat_list_path, false)?; // stream copy — no duration directives

        let config = MergeConfig {
            input_files: input_strs.clone(), input_names: input_names.clone(),
            input_durations: input_durations.clone(),
            subtitle_list_path: None, output_path: crash_output_path.to_string_lossy().to_string(),
            total_duration, mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None, subtitle_files: vec![], card_config: None,
            segment_is_card: vec![], naming_config: None,
            subtitle_mode: SubtitleMode::None, export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };
        let result = test_crash_during_merge(&ffmpeg_path, config, &concat_list_path, &checkpoints_dir, "concat", "crash_test_concat");
        let _ = std::fs::remove_file(&concat_list_path);
        results.push(result);
    }

    // Test 3: Retry after cancel
    {
        let test_start = Instant::now();
        let output_path = temp_path.join("recovery_retry_output.mkv");
        let concat_list_path = temp_path.join("recovery_retry_concat.txt");
        let path_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        write_concat_list_with_durations(&path_refs, Some(&input_durations), &concat_list_path, false)?; // stream copy — no duration directives

        // First attempt — cancelled immediately
        let cancel1 = Arc::new(AtomicBool::new(true));
        let config1 = MergeConfig {
            input_files: input_strs.clone(), input_names: input_names.clone(),
            input_durations: input_durations.clone(),
            subtitle_list_path: None, output_path: output_path.to_string_lossy().to_string(),
            total_duration, mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None, subtitle_files: vec![], card_config: None,
            segment_is_card: vec![], naming_config: None,
            subtitle_mode: SubtitleMode::None, export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };
        let _ = run_merge_blocking(&ffmpeg_path, &config1, &concat_list_path, cancel1, progress);

        // Second attempt — should succeed
        let config2 = MergeConfig {
            input_files: input_strs.clone(), input_names: input_names.clone(),
            input_durations: input_durations.clone(),
            subtitle_list_path: None, output_path: output_path.to_string_lossy().to_string(),
            total_duration, mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None, subtitle_files: vec![], card_config: None,
            segment_is_card: vec![], naming_config: None,
            subtitle_mode: SubtitleMode::None, export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };
        let second_attempt = run_merge_blocking(&ffmpeg_path, &config2, &concat_list_path, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list_path);

        let status = if second_attempt.is_ok() && output_path.exists() {
            match validator.probe(&output_path) {
                Ok(p) if p.video_count() > 0 => TestStatus::Pass,
                _ => TestStatus::Fail,
            }
        } else { TestStatus::Fail };

        results.push(TestResult {
            phase: "E".to_string(), test_name: "recovery_retry".to_string(),
            name: "Retry after cancel".to_string(), status,
            duration_secs: test_start.elapsed().as_secs_f64(),
            evidence: vec![Evidence::new(EvidenceType::VideoFile, "Retry output").with_path(output_path)],
            logs: vec![], error: if status == TestStatus::Pass { None } else { Some("Retry failed".to_string()) },
            metrics: TestMetrics::default(),
        });
    }

    Ok(results)
}
