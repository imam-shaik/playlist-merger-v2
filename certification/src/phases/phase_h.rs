// Phase H: Large Playlist Stress Testing
// Tests with large playlists via production run_merge_blocking().

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

    // Canonicalize media_path to absolute so concat list paths always resolve
    let abs_media_path = std::fs::canonicalize(media_path)
        .unwrap_or_else(|_| media_path.to_path_buf());

    let available_files: Vec<PathBuf> = (1..=5)
        .map(|i| abs_media_path.join(format!("h264_720p_{}.mp4", i)))
        .filter(|p| p.exists()).collect();

    if available_files.len() < 2 {
        results.push(TestResult {
            phase: "H".to_string(), test_name: "phase_h_prerequisites".to_string(),
            name: "Phase H prerequisites check".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec!["Insufficient H.264 files".to_string()],
            error: Some("Need at least 2 H.264 files".to_string()), metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    let ffmpeg_path = find_ffmpeg(None::<&str>).unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress = |_: MergeProgress| {};

    let base_count = available_files.len();
    let base_durations: Vec<f64> = available_files.iter()
        .filter_map(|f| validator.probe(f).ok().and_then(|p| p.total_duration())).collect();
    let single_pass_duration: f64 = base_durations.iter().sum();

    let build_inputs = |target_count: usize| -> (Vec<String>, Vec<String>, Vec<f64>) {
        let mut strs = Vec::with_capacity(target_count);
        let mut names = Vec::with_capacity(target_count);
        let mut durs = Vec::with_capacity(target_count);
        for i in 0..target_count {
            let idx = i % base_count;
            strs.push(available_files[idx].to_string_lossy().to_string());
            names.push(format!("stress_{}", i));
            durs.push(base_durations[idx]);
        }
        (strs, names, durs)
    };

    let run_stress_test = |test_name: &str, display_name: &str, target_count: usize| -> Result<TestResult> {
        let start = Instant::now();
        let (input_files, input_names, input_durations) = build_inputs(target_count);
        let expected_duration = (target_count as f64 / base_count as f64) * single_pass_duration;
        let output_path = temp_path.join(format!("{}.mkv", test_name));
        let concat_list_path = temp_path.join(format!("{}_concat.txt", test_name));

        // Canonicalize each input file path for the concat list
        let canonical_inputs: Vec<String> = input_files.iter().map(|s| {
            std::fs::canonicalize(Path::new(s))
                .unwrap_or_else(|_| PathBuf::from(s))
                .to_string_lossy().to_string()
        }).collect();
        let path_refs: Vec<&Path> = canonical_inputs.iter().map(|s| Path::new(s)).collect();
        write_concat_list_with_durations(&path_refs, Some(&input_durations), &concat_list_path, false) // Lossless = stream copy — no duration directives
            .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: canonical_inputs, input_names, input_durations,
            subtitle_list_path: None, output_path: output_path.to_string_lossy().to_string(),
            total_duration: expected_duration, mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None, subtitle_files: vec![], card_config: None,
            segment_is_card: vec![], naming_config: None,
            subtitle_mode: SubtitleMode::None, export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list_path, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list_path);

        let (status, error, logs, metrics) = if merge_result.is_ok() && output_path.exists() {
            match validator.probe(&output_path) {
                Ok(p) => {
                    let dur = p.total_duration().unwrap_or(0.0);
                    let size = std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
                    (TestStatus::Pass, None,
                     vec![format!("Stress {} files: {}v/{}a, {:.1}s, {} MB", target_count, p.video_count(), p.audio_count(), dur, size / 1_000_000)],
                     TestMetrics { input_file_count: Some(target_count), output_file_count: Some(1), output_size_bytes: Some(size), ..Default::default() })
                }
                Err(e) => (TestStatus::Fail, Some(e.to_string()), vec![], TestMetrics::default()),
            }
        } else { (TestStatus::Fail, Some("Output not created".to_string()), vec![], TestMetrics::default()) };

        Ok(TestResult {
            phase: "H".to_string(), test_name: test_name.to_string(),
            name: display_name.to_string(), status,
            duration_secs: start.elapsed().as_secs_f64(),
            evidence: vec![Evidence::new(EvidenceType::VideoFile, display_name).with_path(output_path)],
            logs, error, metrics,
        })
    };

    // Test 1: 50 files
    results.push(run_stress_test("stress_50_files", "Stress test with 50 files", 50)?);
    // Test 2: 100 files
    results.push(run_stress_test("stress_100_files", "Stress test with 100 files", 100)?);

    // Test 3: 200 files if enough base files
    if base_count >= 5 {
        results.push(run_stress_test("stress_200_files", "Stress test with 200 files", 200)?);
    }

    // Test 4: Baseline
    results.push(run_stress_test("memory_performance", &format!("Baseline merge with {} files", base_count), base_count)?);

    Ok(results)
}
