// Phase G: Subtitle Certification
// Tests subtitle handling via production write_subtitle_concat_list() and run_merge_blocking().

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

    if !media_assets.can_run_phase_g() {
        results.push(TestResult {
            phase: "G".to_string(), test_name: "phase_g_prerequisites".to_string(),
            name: "Phase G prerequisites check".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec![],
            error: Some("Missing subtitle test media".to_string()), metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    let ffmpeg_path = find_ffmpeg(None::<&str>).unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress = |_: MergeProgress| {};

    // Helper: run a merge with subtitle handling
    let run_sub_merge = |test_name: &str, display_name: &str, path: &Path,
                          sub_mode: SubtitleMode, sub_files: Vec<Option<String>>,
                          sub_list: Option<PathBuf>| -> Result<TestResult> {
        let start = Instant::now();
        // Canonicalize path to absolute for concat list
        let abs_path = std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf());
        let dur = validator.probe(&abs_path).ok().and_then(|p| p.total_duration()).unwrap_or(0.0);
        let output_path = temp_path.join(format!("{}.mkv", test_name));
        let concat_list = temp_path.join(format!("{}_concat.txt", test_name));
        let durs = vec![dur];

        let path_refs: Vec<&Path> = vec![&abs_path];
        // Lossless mode = stream copy, so no duration directives
        write_concat_list_with_durations(&path_refs, Some(&durs), &concat_list, false)
            .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: vec![abs_path.to_string_lossy().to_string()],
            input_names: vec![display_name.to_string()],
            input_durations: durs,
            subtitle_list_path: sub_list,
            output_path: output_path.to_string_lossy().to_string(),
            total_duration: dur, mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None, subtitle_files: sub_files, card_config: None,
            segment_is_card: vec![], naming_config: None,
            subtitle_mode: sub_mode, export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list);

        let (status, error, logs) = if merge_result.is_ok() && output_path.exists() {
            match validator.probe(&output_path) {
                Ok(p) => {
                    let sub_count = p.subtitle_count();
                    if sub_count > 0 {
                        (TestStatus::Pass, None, vec![format!("{} subtitle stream(s)", sub_count)])
                    } else {
                        (TestStatus::Warn, Some("Subtitle not preserved".to_string()),
                         vec![format!("{}v/{}a, no subs", p.video_count(), p.audio_count())])
                    }
                }
                Err(e) => (TestStatus::Fail, Some(e.to_string()), vec![]),
            }
        } else { (TestStatus::Fail, Some("Output not created".to_string()), vec![]) };

        Ok(TestResult {
            phase: "G".to_string(), test_name: test_name.to_string(),
            name: display_name.to_string(), status,
            duration_secs: start.elapsed().as_secs_f64(),
            evidence: vec![Evidence::new(EvidenceType::SubtitleFile, display_name).with_path(output_path)],
            logs, error,
            metrics: TestMetrics { input_file_count: Some(1), output_file_count: Some(1), ..Default::default() },
        })
    };

    // Test 1: SRT embedded (in MKV with subrip codec — compatible with Lossless stream copy to MKV output)
    let srt_embedded_path = media_path.join("srt_embedded.mkv");
    if srt_embedded_path.exists() {
        let abs_srt_path = std::fs::canonicalize(&srt_embedded_path)
            .unwrap_or_else(|_| srt_embedded_path.to_path_buf());
        results.push(run_sub_merge("srt_embed", "SRT Embedded subtitle merge", &abs_srt_path, SubtitleMode::Embed, vec![], None)?);
    } else if media_assets.has("srt_embedded") {
        if let Some(path) = media_assets.get_path("srt_embedded") {
            let abs_path = std::fs::canonicalize(&path)
                .unwrap_or_else(|_| path.to_path_buf());
            results.push(run_sub_merge("srt_embed", "SRT Embedded (mov_text) merge", &abs_path, SubtitleMode::Embed, vec![], None)?);
        }
    }

    // Test 2: PGS bitmap
    if media_assets.has("pgs_embedded") {
        if let Some(path) = media_assets.get_path("pgs_embedded") {
            results.push(run_sub_merge("pgs_embed", "PGS bitmap subtitle merge", &path, SubtitleMode::Embed, vec![], None)?);
        }
    } else {
        results.push(TestResult {
            phase: "G".to_string(), test_name: "pgs_embed".to_string(),
            name: "PGS bitmap subtitle merge".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec!["PGS not available".to_string()],
            error: Some("Media not available".to_string()), metrics: TestMetrics::default(),
        });
    }

    // Test 3: External SRT
    if media_assets.has("external_srt") && media_assets.has("h264_720p_1") {
        let srt_path = std::fs::canonicalize(media_assets.get_path("external_srt").unwrap())
            .unwrap_or_else(|_| media_assets.get_path("external_srt").unwrap());
        let video_path = std::fs::canonicalize(media_assets.get_path("h264_720p_1").unwrap())
            .unwrap_or_else(|_| media_assets.get_path("h264_720p_1").unwrap());
        let start = Instant::now();
        let dur = validator.probe(&video_path).ok().and_then(|p| p.total_duration()).unwrap_or(0.0);
        let output_path = temp_path.join("sub_external_srt_output.mkv");
        let concat_list_path = temp_path.join("sub_external_srt_concat.txt");
        let sub_list_path = temp_path.join("sub_external_srt_subs.txt");
        let durs = vec![dur];

        // Write subtitle concat list with absolute paths
        let sub_paths = vec![Some(srt_path.to_string_lossy().to_string())];
        write_subtitle_concat_list(&sub_paths, &durs, &sub_list_path, temp_path, "srt")
            .map_err(|e| anyhow::anyhow!("Failed to write subtitle concat list: {}", e))?;

        let path_refs: Vec<&Path> = vec![video_path.as_path()];
        // Lossless mode = stream copy, so no duration directives
        write_concat_list_with_durations(&path_refs, Some(&durs), &concat_list_path, false)
            .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: vec![video_path.to_string_lossy().to_string()],
            input_names: vec!["video_with_external_srt".to_string()],
            input_durations: durs, subtitle_list_path: Some(sub_list_path.clone()),
            output_path: output_path.to_string_lossy().to_string(),
            total_duration: dur, mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None,
            subtitle_files: vec![Some(srt_path.to_string_lossy().to_string())],
            card_config: None, segment_is_card: vec![], naming_config: None,
            subtitle_mode: SubtitleMode::Embed, export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list_path, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list_path);
        let _ = std::fs::remove_file(&sub_list_path);

        let (status, error, logs) = if merge_result.is_ok() && output_path.exists() {
            match validator.probe(&output_path) {
                Ok(p) if p.subtitle_count() > 0 => (TestStatus::Pass, None, vec![format!("External SRT: {} sub(s)", p.subtitle_count())]),
                Ok(_) => (TestStatus::Warn, Some("External SRT not embedded".to_string()), vec![]),
                Err(e) => (TestStatus::Fail, Some(e.to_string()), vec![]),
            }
        } else { (TestStatus::Fail, Some("Output not created".to_string()), vec![]) };

        results.push(TestResult {
            phase: "G".to_string(), test_name: "external_srt".to_string(),
            name: "External SRT file merge".to_string(), status,
            duration_secs: start.elapsed().as_secs_f64(),
            evidence: vec![Evidence::new(EvidenceType::SubtitleFile, "External SRT").with_path(output_path)],
            logs, error,
            metrics: TestMetrics { input_file_count: Some(2), output_file_count: Some(1), ..Default::default() },
        });
    } else {
        results.push(TestResult {
            phase: "G".to_string(), test_name: "external_srt".to_string(),
            name: "External SRT file merge".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec!["External SRT not available".to_string()],
            error: Some("Media not available".to_string()), metrics: TestMetrics::default(),
        });
    }

    // Test 4: VobSub
    if media_assets.has("vobsub_embedded") {
        if let Some(path) = media_assets.get_path("vobsub_embedded") {
            results.push(run_sub_merge("vobsub_embed", "VobSub bitmap subtitle merge", &path, SubtitleMode::Embed, vec![], None)?);
        }
    } else {
        results.push(TestResult {
            phase: "G".to_string(), test_name: "vobsub_embed".to_string(),
            name: "VobSub bitmap subtitle merge".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec!["VobSub not available".to_string()],
            error: Some("Media not available".to_string()), metrics: TestMetrics::default(),
        });
    }

    Ok(results)
}
