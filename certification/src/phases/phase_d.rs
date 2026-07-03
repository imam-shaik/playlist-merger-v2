// Phase D: Cards Certification
// Tests card placement via production render_cards_for_merge() + interleave_cards_with_videos().

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

    if !media_assets.can_run_phase_a() {
        results.push(TestResult {
            phase: "D".to_string(), test_name: "phase_d_prerequisites".to_string(),
            name: "Phase D prerequisites check".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec![],
            error: Some("Missing required media".to_string()), metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    // Canonicalize media_path to absolute so concat list paths always resolve
    let abs_media_path = std::fs::canonicalize(media_path)
        .unwrap_or_else(|_| media_path.to_path_buf());

    let files: Vec<PathBuf> = (1..=2)
        .map(|i| abs_media_path.join(format!("h264_720p_{}.mp4", i)))
        .filter(|p| p.exists()).collect();
    if files.len() < 2 {
        results.push(TestResult {
            phase: "D".to_string(), test_name: "phase_d_setup".to_string(),
            name: "Phase D setup".to_string(), status: TestStatus::Skip,
            duration_secs: 0.0, evidence: vec![], logs: vec![],
            error: Some("Need 2 H.264 files".to_string()), metrics: TestMetrics::default(),
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
        .filter_map(|f| validator.probe(f).ok().and_then(|p| p.total_duration())).collect();
    let total_duration: f64 = input_durations.iter().sum();

    // CardConfig (production struct: color, font_color, duration, show_in_report, frequency)
    let card_config = CardConfig {
        color: "#415A77".to_string(),
        font_color: "#FFFFFF".to_string(),
        duration: 2.0,
        show_in_report: true,
        frequency: CardFrequency::PerVideo,
    };

    // render_cards_for_merge takes: ffmpeg_path, ffprobe_path, input_files, input_names, input_durations,
    // card_config, temp_dir, boundary_card_labels (Option<&HashMap<usize, String>>)
    // Returns: Result<Vec<(String, usize)>> — (card_path, video_index)

    // We need to call probe_first_input first to get dimensions... Actually
    // render_cards_for_merge handles probing internally.
    let cards_result = render_cards_for_merge(
        &ffmpeg_path,
        None, // ffprobe_path
        &input_strs,
        &input_names,
        &input_durations,
        &card_config,
        temp_path,
        None, // boundary_card_labels
    );

    let rendered_cards = match cards_result {
        Ok(cards) => cards, // Vec<(String, usize)> — (card_path, video_index)
        Err(e) => {
            results.push(TestResult {
                phase: "D".to_string(), test_name: "render_cards".to_string(),
                name: "Render cards via production pipeline".to_string(),
                status: TestStatus::Skip, duration_secs: 0.0,
                evidence: vec![], logs: vec![],
                error: Some(format!("Could not render cards: {}. Check ffmpeg.", e)),
                metrics: TestMetrics::default(),
            });
            return Ok(results);
        }
    };

    // interleave_cards_with_videos takes: input_files, input_durations, rendered, card_config
    // Returns: InterleaveResult { files: Vec<String>, durations: Vec<f64>, segment_cards: Vec<(bool, Option<String>)> }
    let interleave = interleave_cards_with_videos(
        &input_strs,
        &input_durations,
        &rendered_cards,
        &card_config,
    );

    // Build segment_is_card from interleave.segment_cards
    let segment_is_card: Vec<bool> = interleave.segment_cards.iter().map(|(is_card, _)| *is_card).collect();

    // Test 1: Merge with production cards
    {
        let test_start = Instant::now();
        let output_path = temp_path.join("cards_production_output.mkv");
        let concat_list_path = temp_path.join("cards_production_concat.txt");

        let total = interleave.durations.iter().sum::<f64>();
        let path_refs: Vec<&Path> = interleave.files.iter().map(|s| Path::new(s)).collect();
        write_concat_list_with_durations(&path_refs, Some(&interleave.durations), &concat_list_path, false) // Lossless = stream copy — no duration directives
            .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: interleave.files.clone(),
            input_names: (0..interleave.files.len()).map(|i| format!("seg_{}", i)).collect(),
            input_durations: interleave.durations.clone(),
            subtitle_list_path: None,
            output_path: output_path.to_string_lossy().to_string(),
            total_duration: total,
            mode: MergeMode::Lossless,
            video_codec: None, audio_codec: None, video_crf: None, video_preset: None,
            audio_bitrate: None, target_resolution: None, target_fps: None, hw_accel: None,
            split_config: None, subtitle_files: vec![],
            card_config: Some(card_config.clone()),
            segment_is_card: segment_is_card.clone(),
            naming_config: None,
            subtitle_mode: SubtitleMode::None,
            export_merged_srt: false, burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        let merge_result = run_merge_blocking(&ffmpeg_path, &config, &concat_list_path, cancel_flag.clone(), progress);
        let _ = std::fs::remove_file(&concat_list_path);

        let (status, error, logs) = if merge_result.is_ok() && output_path.exists() {
            match validator.probe(&output_path) {
                Ok(p) if p.video_count() > 0 => {
                    (TestStatus::Pass, None,
                     vec![format!("Cards merge: {}v/{}a, dur {:.1}s, {} total segments",
                        p.video_count(), p.audio_count(), p.total_duration().unwrap_or(0.0), interleave.files.len())])
                }
                Ok(_) => (TestStatus::Fail, Some("No video".to_string()), vec![]),
                Err(e) => (TestStatus::Fail, Some(e.to_string()), vec![]),
            }
        } else {
            (TestStatus::Fail, Some("Output not created".to_string()), vec![])
        };

        results.push(TestResult {
            phase: "D".to_string(), test_name: "production_cards".to_string(),
            name: "Production card merge".to_string(), status,
            duration_secs: test_start.elapsed().as_secs_f64(),
            evidence: vec![Evidence::new(EvidenceType::VideoFile, "Cards output").with_path(output_path)],
            logs, error,
            metrics: TestMetrics { input_file_count: Some(interleave.files.len()), output_file_count: Some(1), ..Default::default() },
        });
    }

    Ok(results)
}
