// Phase F: Audio Certification
// Tests audio codec handling via production run_merge_blocking().

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

    let audio_files = ["aac_stereo_44100", "aac_51_48000"];
    if !media_assets.has(audio_files[0]) || !media_assets.has(audio_files[1]) {
        results.push(TestResult {
            phase: "F".to_string(), test_name: "phase_f_prerequisites".to_string(),
            name: "Phase F prerequisites check".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec![],
            error: Some("Missing audio test media".to_string()), metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    let ffmpeg_path = find_ffmpeg(None::<&str>).unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress = |_: MergeProgress| {};

    // Helper to run a single-file merge test
    let run_single_file_test = |test_name: &str, display_name: &str, path: &Path, mode: MergeMode, ext: &str| -> Result<TestResult> {
        use playlist_merger_lib::certification_api::*;
        let start = Instant::now();
        let is_custom = mode == MergeMode::Custom;
        // Canonicalize path to absolute for concat list
        let abs_path = std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf());
        let dur = validator.probe(&abs_path).ok().and_then(|p| p.total_duration()).unwrap_or(0.0);
        let output_path = temp_path.join(format!("{}.{}", test_name, ext));
        let concat_list = temp_path.join(format!("{}_concat.txt", test_name));
        let durs = vec![dur];

        let path_refs: Vec<&Path> = vec![&abs_path];
        let include_duration = is_custom; // true for re-encode, false for stream copy
        write_concat_list_with_durations(&path_refs, Some(&durs), &concat_list, include_duration)
            .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: vec![abs_path.to_string_lossy().to_string()],
            input_names: vec![display_name.to_string()],
            input_durations: durs,
            subtitle_list_path: None,
            output_path: output_path.to_string_lossy().to_string(),
            total_duration: dur, mode,
            video_codec: if is_custom { Some("libx264".to_string()) } else { None },
            audio_codec: if is_custom { Some("aac".to_string()) } else { None },
            video_crf: Some(23), video_preset: Some("fast".to_string()),
            audio_bitrate: if is_custom { Some("128k".to_string()) } else { None },
            target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None, subtitle_files: vec![], card_config: None,
            segment_is_card: vec![], naming_config: None,
            subtitle_mode: SubtitleMode::None, export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list);

        let (status, error, logs) = if merge_result.is_ok() && output_path.exists() {
            match validator.probe(&output_path) {
                Ok(p) if p.audio_count() > 0 => {
                    let ac = p.primary_audio().map(|s| s.codec_name.clone()).unwrap_or_default();
                    let channels = p.primary_audio().and_then(|s| s.channels).unwrap_or(0);
                    (TestStatus::Pass, None,
                     vec![format!("{}: {}v/{}a, codec={}, {}ch, {}Hz",
                        display_name, p.video_count(), p.audio_count(), ac, channels,
                        p.primary_audio().and_then(|s| s.sample_rate).unwrap_or(0))])
                }
                Ok(_) => (TestStatus::Fail, Some("No audio in output".to_string()), vec![]),
                Err(e) => (TestStatus::Fail, Some(e.to_string()), vec![]),
            }
        } else { (TestStatus::Fail, Some("Output not created".to_string()), vec![]) };

        Ok(TestResult {
            phase: "F".to_string(), test_name: test_name.to_string(),
            name: display_name.to_string(), status,
            duration_secs: start.elapsed().as_secs_f64(),
            evidence: vec![Evidence::new(EvidenceType::AudioFile, display_name).with_path(output_path)],
            logs, error,
            metrics: TestMetrics { input_file_count: Some(1), output_file_count: Some(1), ..Default::default() },
        })
    };

    // Test 1: Stereo AAC
    if let Some(path) = media_assets.get_path("aac_stereo_44100") {
        results.push(run_single_file_test("stereo_aac", "Stereo AAC lossless merge", &path, MergeMode::Lossless, "mkv")?);
    }

    // Test 2: 5.1 AAC
    if let Some(path) = media_assets.get_path("aac_51_48000") {
        results.push(run_single_file_test("51_aac", "5.1 AAC lossless merge", &path, MergeMode::Lossless, "mkv")?);
    }

    // Test 3: Multi-audio MKV
    if media_assets.has("mkv_multi_audio_1") {
        if let Some(path) = media_assets.get_path("mkv_multi_audio_1") {
            results.push(run_single_file_test("multi_audio_mkv", "Multi-audio MKV merge", &path, MergeMode::Lossless, "mkv")?);
        }
    } else {
        results.push(TestResult {
            phase: "F".to_string(), test_name: "multi_audio_mkv".to_string(),
            name: "Multi-audio MKV merge".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec!["Not available".to_string()],
            error: Some("Media not available".to_string()), metrics: TestMetrics::default(),
        });
    }

    // Test 4: AAC re-encode
    if let Some(path) = media_assets.get_path("aac_stereo_44100") {
        results.push(run_single_file_test("audio_reencode", "AAC re-encode (custom mode)", &path, MergeMode::Custom, "mp4")?);
    }

    Ok(results)
}
