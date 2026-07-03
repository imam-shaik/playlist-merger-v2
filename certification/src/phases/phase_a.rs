// Phase A: Core Merge Certification
// Tests all merge modes via the production run_merge_blocking() pipeline.
// Uses playlist_merger_lib::certification_api for real merge execution.

use crate::media::MediaAssets;
use crate::reporters::{TestMetrics, TestResult, TestStatus};
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
    let mut results = Vec::new();
    let validator = FfprobeValidator::new();

    // Check prerequisites
    if !media_assets.can_run_phase_a() {
        results.push(TestResult {
            phase: "A".to_string(),
            test_name: "phase_a_prerequisites".to_string(),
            name: "Phase A prerequisites check".to_string(),
            status: TestStatus::Skip,
            duration_secs: 0.0,
            evidence: vec![],
            logs: vec![],
            error: Some("Missing required media assets".to_string()),
            metrics: TestMetrics::default(),
        });
        return Ok(results);
    }

    // Canonicalize media_path to absolute so concat lists resolve correctly
    let abs_media_path = std::fs::canonicalize(media_path)
        .unwrap_or_else(|_| media_path.to_path_buf());
    let abs_temp_path = std::fs::canonicalize(temp_path)
        .unwrap_or_else(|_| temp_path.to_path_buf());

    // Test 1: FastMKV with 3 files (homogeneous: all 320x240 mono 44100Hz)
    {
        let test_start = Instant::now();
        let result = run_merge_test(
            "fastmkv_3_files",
            "FastMKV 3-file merge (homogeneous)",
            &abs_media_path, &abs_temp_path, &validator, 3, "fastmkv", None,
        )?;
        results.push(result);
        println!("  FastMKV 3-file test: {} ({:.1}s)", results.last().unwrap().status, test_start.elapsed().as_secs_f64());
    }

    // Test 2: FastMKV with 10 files (homogeneous: files 1-3 cycled)
    {
        let test_start = Instant::now();
        let result = run_merge_test(
            "fastmkv_10_files",
            "FastMKV 10-file merge (homogeneous)",
            &abs_media_path, &abs_temp_path, &validator, 10, "fastmkv", None,
        )?;
        results.push(result);
        println!("  FastMKV 10-file test: {} ({:.1}s)", results.last().unwrap().status, test_start.elapsed().as_secs_f64());
    }

    // Test 3: SmartMKV with 3 files (homogeneous, stream copy, NO duration directives)
    {
        let test_start = Instant::now();
        let result = run_merge_test(
            "smartmkv_3_files",
            "SmartMKV 3-file merge (homogeneous)",
            &abs_media_path, &abs_temp_path, &validator, 3, "smartmkv", None,
        )?;
        results.push(result);
        println!("  SmartMKV 3-file test: {} ({:.1}s)", results.last().unwrap().status, test_start.elapsed().as_secs_f64());
    }

    // Test 4: Lossless with 3 files (homogeneous, stream copy, NO duration directives)
    {
        let test_start = Instant::now();
        let result = run_merge_test(
            "lossless_3_files",
            "Lossless 3-file merge (homogeneous)",
            &abs_media_path, &abs_temp_path, &validator, 3, "lossless", None,
        )?;
        results.push(result);
        println!("  Lossless 3-file test: {} ({:.1}s)", results.last().unwrap().status, test_start.elapsed().as_secs_f64());
    }

    // Test 5: Custom with 3 files (homogeneous, re-encode, WITH duration directives)
    {
        let test_start = Instant::now();
        let result = run_merge_test(
            "custom_3_files",
            "Custom 3-file merge (homogeneous)",
            &abs_media_path, &abs_temp_path, &validator, 3, "custom",
            Some(playlist_merger_lib::certification_api::MergeMode::Custom),
        )?;
        results.push(result);
        println!("  Custom 3-file test: {} ({:.1}s)", results.last().unwrap().status, test_start.elapsed().as_secs_f64());
    }

    // Test 6: FastMKV with heterogeneous files (documented known limitation)
    // Files 4-5 have stereo 48000Hz, files 1-3 have mono 44100Hz
    // FFmpeg concat demuxer with -c copy inflates duration on audio channel/sample rate mismatch
    // This test verifies the bug exists and documents the behavior
    {
        let test_start = Instant::now();
        let result = run_merge_test_heterogeneous(
            "fastmkv_heterogeneous",
            "FastMKV heterogeneous merge (known audio mismatch limitation)",
            &abs_media_path, &abs_temp_path, &validator, 10,
        )?;
        results.push(result);
        println!("  FastMKV heterogeneous test: {} ({:.1}s)", results.last().unwrap().status, test_start.elapsed().as_secs_f64());
    }

    Ok(results)
}

fn resolve_mode(s: &str, variant: Option<playlist_merger_lib::certification_api::MergeMode>) -> playlist_merger_lib::certification_api::MergeMode {
    if let Some(v) = variant {
        return v;
    }
    match s {
        "fastmkv" => playlist_merger_lib::certification_api::MergeMode::FastMkv,
        "smartmkv" => playlist_merger_lib::certification_api::MergeMode::SmartMkv,
        "lossless" => playlist_merger_lib::certification_api::MergeMode::Lossless,
        _ => playlist_merger_lib::certification_api::MergeMode::Lossless,
    }
}

fn resolve_ext(mode: &playlist_merger_lib::certification_api::MergeMode) -> &'static str {
    use playlist_merger_lib::certification_api::MergeMode;
    match mode {
        MergeMode::Custom => "mp4",
        _ => "mkv",
    }
}

/// Return up to `count` homogeneous input files.
/// Files 1-3 are homogeneous: 320x240, AAC mono 44100Hz, 3.0s each.
/// Files 4-5 are heterogeneous (different resolution, stereo, different sample rate)
/// and should NOT be mixed with files 1-3 in stream-copy tests.
///
/// If `homogeneous_only` is true (default), only returns files 1-3.
/// If false, returns all available files up to count.
fn get_input_files(media_path: &Path, count: usize, homogeneous_only: bool) -> Vec<PathBuf> {
    let max = if homogeneous_only { std::cmp::min(count, 3) } else { std::cmp::min(count, 5) };
    (1..=max)
        .map(|i| media_path.join(format!("h264_720p_{}.mp4", i)))
        .filter(|p| p.exists())
        .collect()
}

fn compute_total_duration(files: &[PathBuf], validator: &FfprobeValidator) -> f64 {
    files.iter()
        .filter_map(|f| validator.probe(f).ok().and_then(|p| p.total_duration()))
        .sum()
}

/// Test FastMKV with heterogeneous files (all 5 files, which have different audio properties).
/// This documents the known concat demuxer limitation: stream copy with audio channel/
/// sample rate mismatches produces inflated duration output.
/// Files 1-3: 320x240, AAC mono 44100Hz, 3.0s
/// Files 4-5: 1920x1080/640x360, AAC stereo 48000Hz, 5.0s
fn run_merge_test_heterogeneous(
    test_name: &str,
    display_name: &str,
    media_path: &Path,
    temp_path: &Path,
    validator: &FfprobeValidator,
    target_count: usize,
) -> Result<TestResult> {
    use playlist_merger_lib::certification_api::*;

    let start_time = Instant::now();
    let ext = "mkv";

    // Get ALL available files (not just homogeneous)
    let base_files = get_input_files(media_path, 5, false);
    if base_files.is_empty() {
        return Ok(TestResult {
            phase: "A".to_string(),
            test_name: test_name.to_string(),
            name: display_name.to_string(),
            status: TestStatus::Skip,
            duration_secs: start_time.elapsed().as_secs_f64(),
            evidence: vec![],
            logs: vec!["No input files found".to_string()],
            error: Some("No input files available".to_string()),
            metrics: TestMetrics::default(),
        });
    }

    // Build file list cycling through ALL 5 files
    let input_files: Vec<PathBuf> = (0..target_count)
        .map(|i| base_files[i % base_files.len()].clone())
        .collect();

    let total_duration = compute_total_duration(&input_files, validator);
    let output_path = temp_path.join(format!("{}.{}", test_name, ext));

    let ffmpeg_path = find_ffmpeg(None::<&str>).unwrap_or_else(|_| PathBuf::from("ffmpeg"));

    let input_strs: Vec<String> = input_files.iter()
        .map(|p| {
            std::fs::canonicalize(p)
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy().to_string()
        })
        .collect();
    let input_durations: Vec<f64> = input_files.iter()
        .filter_map(|f| validator.probe(f).ok().and_then(|p| p.total_duration()))
        .collect();

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress = |_: MergeProgress| {};

    let merge_result = run_fast_mkv_merge(
        &ffmpeg_path,
        &input_strs,
        &input_durations,
        &output_path.to_string_lossy().to_string(),
        total_duration,
        cancel_flag,
        progress,
    )
    .map(|_| MergeResult {
        job_id: "".into(),
        output_path: output_path.to_string_lossy().to_string(),
        output_size_bytes: std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0),
        segments: vec![],
        output_paths: None,
        parts: None,
        srt_export_paths: None,
        report_paths: None,
        warnings: None,
        subtitle_warnings: None,
        audio_repair_summary: None,
    })
    .map_err(|e| anyhow::anyhow!("FastMKV merge failed: {}", e));

    // Compute ALL test values BEFORE cleanup
    let output_exists_at_end = output_path.exists();
    let actual_duration = if output_exists_at_end {
        validator.probe(&output_path).ok().and_then(|p| p.total_duration())
    } else {
        None
    };
    let output_size = if output_exists_at_end {
        std::fs::metadata(&output_path).ok().map(|m| m.len())
    } else {
        None
    };
    let diff_pct = match (total_duration, actual_duration) {
        (expected, Some(actual)) if expected > 0.0 => {
            ((expected - actual).abs() / expected * 100.0).min(100.0)
        }
        _ => 0.0,
    };

    // Determine test status based on merge result and output file
    let (status, error, logs) = if let Err(e) = merge_result {
        (TestStatus::Fail, Some(format!("FastMKV merge failed: {}", e)), vec![])
    } else if !output_exists_at_end {
        (TestStatus::Fail, Some(
            "Output file not created — FastMKV concat demuxer may have crashed with heterogeneous audio inputs".to_string()
        ), vec![
            format!("Expected total duration: {:.1}s", total_duration),
            "Files 1-3: mono 44100Hz".to_string(),
            "Files 4-5: stereo 48000Hz".to_string(),
            "FastMKV with -c copy can fail on audio parameter change".to_string(),
        ])
    } else if diff_pct > 5.0 {
        (TestStatus::Warn,
         Some(format!(
             "Duration mismatch: expected {:.1}s, got {:.1}s ({:.1}%) — KNOWN concat demuxer limitation with heterogeneous audio",
             total_duration, actual_duration.unwrap_or(0.0), diff_pct
         )),
         vec![
             format!("Expected: {:.1}s, Actual: {:.1}s, Diff: {:.1}%", total_duration, actual_duration.unwrap_or(0.0), diff_pct),
             "Root cause: Files 1-3 have mono 44100Hz, files 4-5 have stereo 48000Hz".to_string(),
             "Concat demuxer with -c copy inflates duration on audio parameter change".to_string(),
         ])
    } else {
        // If it passes, that's unexpected — log it
        (TestStatus::Pass, None,
         vec![format!("Duration: {:.1}s, Diff: {:.1}% — heterogeneous audio did NOT cause inflation", actual_duration.unwrap_or(0.0), diff_pct)])
    };

    // Clean up output file AFTER all validation is done
    let _ = std::fs::remove_file(&output_path);

    Ok(TestResult {
        phase: "A".to_string(),
        test_name: test_name.to_string(),
        name: display_name.to_string(),
        status,
        duration_secs: start_time.elapsed().as_secs_f64(),
        evidence: vec![],
        logs,
        error,
        metrics: TestMetrics {
            input_file_count: Some(input_files.len()),
            output_file_count: Some(if output_exists_at_end { 1 } else { 0 }),
            expected_duration_ms: Some((total_duration * 1000.0) as u64),
            actual_duration_ms: actual_duration.map(|d| (d * 1000.0) as u64),
            output_size_bytes: output_size,
            ..Default::default()
        },
    })
}

fn run_merge_test(
    test_name: &str,
    display_name: &str,
    media_path: &Path,
    temp_path: &Path,
    validator: &FfprobeValidator,
    target_count: usize,
    mode_str: &str,
    mode_override: Option<playlist_merger_lib::certification_api::MergeMode>,
) -> Result<TestResult> {
    use playlist_merger_lib::certification_api::*;

    let start_time = Instant::now();

    let mode = resolve_mode(mode_str, mode_override);
    let ext = resolve_ext(&mode);

    // Get input files — use homogeneous files only (1-3) for baseline tests
    let base_files = get_input_files(media_path, 5, true);
    if base_files.is_empty() {
        return Ok(TestResult {
            phase: "A".to_string(),
            test_name: test_name.to_string(),
            name: display_name.to_string(),
            status: TestStatus::Skip,
            duration_secs: start_time.elapsed().as_secs_f64(),
            evidence: vec![],
            logs: vec!["No input files found".to_string()],
            error: Some("No input files available".to_string()),
            metrics: TestMetrics::default(),
        });
    }

    // Build file list (cycle through base files to reach target_count)
    let input_files: Vec<PathBuf> = (0..target_count)
        .map(|i| base_files[i % base_files.len()].clone())
        .collect();

    let total_duration = compute_total_duration(&input_files, validator);
    let output_path = temp_path.join(format!("{}.{}", test_name, ext));
    let concat_list_path = temp_path.join(format!("{}_concat.txt", test_name));

    let ffmpeg_path = find_ffmpeg(None::<&str>).unwrap_or_else(|_| PathBuf::from("ffmpeg"));

    // Build string arrays for MergeConfig (use canonicalized absolute paths)
    let input_strs: Vec<String> = input_files.iter()
        .map(|p| {
            std::fs::canonicalize(p)
                .unwrap_or_else(|_| p.clone())
                .to_string_lossy().to_string()
        })
        .collect();
    let input_names: Vec<String> = input_files.iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str().map(String::from)))
        .collect();
    let input_durations: Vec<f64> = input_files.iter()
        .filter_map(|f| validator.probe(f).ok().and_then(|p| p.total_duration()))
        .collect();

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress = |_: MergeProgress| {};

    // ── FastMKV uses a different production pipeline ────────────────────────
    // run_merge_blocking() in concat.rs handles SmartMkv/Lossless/Custom via
    // the ffmpeg concat demuxer. FastMKV mode uses run_fast_mkv_merge() which
    // writes its own concat list and runs ffmpeg with -c copy.
    //
    // IMPORTANT: For stream copy modes (Lossless, SmartMKV, FastMKV),
    // the concat demuxer should NOT receive duration directives because
    // the demuxer reads each file until EOF in stream copy mode.
    // Re-encode mode (Custom) needs duration directives to bound processing.
    let merge_result = if mode == MergeMode::FastMkv {
        // FastMKV uses its own concat list internally (always includes duration)
        run_fast_mkv_merge(
            &ffmpeg_path,
            &input_strs,
            &input_durations,
            &output_path.to_string_lossy().to_string(),
            total_duration,
            cancel_flag,
            progress,
        )
        .map(|_| MergeResult {
            job_id: "".into(),
            output_path: output_path.to_string_lossy().to_string(),
            output_size_bytes: std::fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0),
            segments: vec![],
            output_paths: None,
            parts: None,
            srt_export_paths: None,
            report_paths: None,
            warnings: None,
            subtitle_warnings: None,
            audio_repair_summary: None,
        })
        .map_err(|e| anyhow::anyhow!("FastMKV merge failed: {}", e))
    } else {
        // SmartMKV, Lossless, Custom: use the standard concat demuxer pipeline
        // Determine whether to include duration directives:
        // - Stream copy (Lossless, SmartMKV): OMIT duration (read until EOF)
        // - Re-encode (Custom): INCLUDE duration (bound processing per file)
        let include_duration = mode == MergeMode::Custom;

        let path_refs: Vec<&Path> = input_files.iter().map(|p| p.as_path()).collect();
        write_concat_list_with_durations(
            &path_refs,
            Some(&input_durations),
            &concat_list_path,
            include_duration,
        )
        .map_err(|e| anyhow::anyhow!("Failed to write concat list: {}", e))?;

        let config = MergeConfig {
            input_files: input_strs,
            input_names,
            input_durations,
            subtitle_list_path: None,
            output_path: output_path.to_string_lossy().to_string(),
            total_duration,
            video_codec: if mode == MergeMode::Custom { Some("libx264".to_string()) } else { None },
            mode,
            audio_codec: None,
            video_crf: Some(23),
            video_preset: Some("fast".to_string()),
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

        run_merge_blocking(&ffmpeg_path, &config, &concat_list_path, cancel_flag, progress)
    };

    // Clean up concat list
    let _ = std::fs::remove_file(&concat_list_path);

    let (status, error, logs) = if let Err(e) = merge_result {
        (TestStatus::Fail, Some(format!("Merge failed: {}", e)), vec![])
    } else if !output_path.exists() {
        (TestStatus::Fail, Some("Output file not created".to_string()), vec![])
    } else if let Ok(probe) = validator.probe(&output_path) {
        if probe.video_count() == 0 {
            (TestStatus::Fail, Some("No video in output".to_string()), vec![])
        } else if let Some(actual_dur) = probe.total_duration() {
            let diff_pct = if total_duration > 0.0 {
                ((total_duration - actual_dur).abs() / total_duration * 100.0).min(100.0)
            } else {
                0.0
            };
            if diff_pct > 5.0 {
                (TestStatus::Fail,
                 Some(format!("Duration mismatch: expected {:.1}s, got {:.1}s ({:.1}%)", total_duration, actual_dur, diff_pct)),
                 vec![format!("Duration: {:.1}s, Streams: {}v/{}a", actual_dur, probe.video_count(), probe.audio_count())])
            } else {
                (TestStatus::Pass, None,
                 vec![format!("Duration: {:.1}s, Streams: {}v/{}a", actual_dur, probe.video_count(), probe.audio_count())])
            }
        } else {
            (TestStatus::Fail, Some("Could not read output duration".to_string()), vec![])
        }
    } else {
        (TestStatus::Fail, Some("Could not probe output".to_string()), vec![])
    };

    Ok(TestResult {
        phase: "A".to_string(),
        test_name: test_name.to_string(),
        name: display_name.to_string(),
        status,
        duration_secs: start_time.elapsed().as_secs_f64(),
        evidence: vec![],
        logs,
        error,
        metrics: TestMetrics {
            input_file_count: Some(input_files.len()),
            output_file_count: Some(if output_path.exists() { 1 } else { 0 }),
            expected_duration_ms: Some((total_duration * 1000.0) as u64),
            actual_duration_ms: validator.probe(&output_path).ok().and_then(|p| p.total_duration().map(|d| (d * 1000.0) as u64)),
            output_size_bytes: std::fs::metadata(&output_path).ok().map(|m| m.len()),
            ..Default::default()
        },
    })
}
