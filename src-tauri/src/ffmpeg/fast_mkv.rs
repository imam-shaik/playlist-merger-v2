use anyhow::{Result, anyhow};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use crate::types::{MediaInfo, MergeProgress, MergePhase};
use crate::ffmpeg::progress::{ProgressBlockReader, parse_progress_block, calc_progress_percent};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Compatibility report for Fast MKV merge
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FastMkvCompatibility {
    pub compatible: bool,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub resolution: Option<String>,
    pub fps: Option<f64>,
    pub reasons: Vec<String>,
}

/// MP4 codec compatibility
struct Mp4CodecCompat {
    video_compatible: bool,
    audio_compatible: bool,
}

/// Check if input files are compatible for Fast MKV merge
pub fn check_fast_mkv_compatibility(media_infos: &[MediaInfo]) -> FastMkvCompatibility {
    if media_infos.is_empty() {
        return FastMkvCompatibility {
            compatible: false,
            video_codec: None,
            audio_codec: None,
            resolution: None,
            fps: None,
            reasons: vec!["No input files provided".to_string()],
        };
    }

    let first = &media_infos[0];
    let ref_video_codec = first.video_streams.first().map(|v| v.codec_name.clone());
    let ref_audio_codec = first.audio_streams.first().map(|a| a.codec_name.clone());
    let ref_resolution = first.video_streams.first().and_then(|v| {
        match (v.width, v.height) {
            (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
            _ => None,
        }
    });
    let ref_fps = first.video_streams.first().and_then(|v| v.fps);

    let mut reasons = Vec::new();

    for (i, info) in media_infos.iter().enumerate() {
        if i == 0 { continue; }

        let video_codec = info.video_streams.first().map(|v| v.codec_name.clone());
        let audio_codec = info.audio_streams.first().map(|a| a.codec_name.clone());
        let resolution = info.video_streams.first().and_then(|v| {
            match (v.width, v.height) {
                (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                _ => None,
            }
        });
        let fps = info.video_streams.first().and_then(|v| v.fps);

        if video_codec != ref_video_codec {
            reasons.push(format!("File #{}: video codec {:?} differs from reference {:?}", i + 1, video_codec, ref_video_codec));
        }
        if audio_codec != ref_audio_codec {
            reasons.push(format!("File #{}: audio codec {:?} differs from reference {:?}", i + 1, audio_codec, ref_audio_codec));
        }
        if resolution != ref_resolution {
            reasons.push(format!("File #{}: resolution {:?} differs from reference {:?}", i + 1, resolution, ref_resolution));
        }
        if let (Some(ref_fps), Some(fps)) = (ref_fps, fps) {
            if (ref_fps - fps).abs() > 0.1 {
                reasons.push(format!("File #{}: fps {:.2} differs from reference {:.2}", i + 1, fps, ref_fps));
            }
        }
    }

    FastMkvCompatibility {
        compatible: reasons.is_empty(),
        video_codec: ref_video_codec,
        audio_codec: ref_audio_codec,
        resolution: ref_resolution,
        fps: ref_fps,
        reasons,
    }
}

/// Run Fast MKV merge using FFmpeg concat demuxer (stream copy, no transcoding)
pub fn run_fast_mkv_merge(
    ffmpeg_path: &Path,
    input_files: &[String],
    input_durations: &[f64],
    output_path: &str,
    total_duration: f64,
    cancel_flag: Arc<AtomicBool>,
    on_progress: impl Fn(MergeProgress) + Send + 'static,
) -> Result<()> {
    let concat_list_path = std::env::temp_dir().join(format!("fast_mkv_concat_{}.txt",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_millis()
    ));

    let mut concat_content = String::with_capacity(input_files.len() * 120);
    for (i, file) in input_files.iter().enumerate() {
        // Normalize path for FFmpeg concat demuxer compatibility:
        // 1. Convert backslashes to forward slashes
        // 2. Strip \\?\ extended-length prefix (FFmpeg's C runtime fopen() doesn't support it)
        let mut raw = file.replace('\\', "/");
        if cfg!(windows) {
            if let Some(stripped) = raw.strip_prefix("//?/") {
                if let Some(unc_part) = stripped.strip_prefix("UNC/") {
                    raw = format!("/{}", unc_part);
                } else {
                    raw = stripped.to_string();
                }
            }
        }

        let escaped = raw.replace('\'', "'\\''");
        concat_content.push_str(&format!("file '{}'\n", escaped));

        // Add duration directive for segments to help concat demuxer with timing.
        // This is critical for short segments (like canvas cards) where the demuxer
        // may misalign timestamps without explicit duration hints.
        if i < input_durations.len() && input_durations[i] > 0.0 {
            concat_content.push_str(&format!("duration {}\n", input_durations[i]));
        }
    }

    log::info!("[FastMkv] Concat list written to: {}", concat_list_path.display());
    log::info!("[FastMkv] Concat list contents ({} entries):", input_files.len());
    for (i, file) in input_files.iter().enumerate() {
        let is_card = file.contains("card_") && file.contains(".mp4");
        let dur = input_durations.get(i).copied().unwrap_or(0.0);
        log::info!("[FastMkv]   [{:>3}] {} | dur={:.1}s | {}", i, if is_card { "CARD " } else { "VIDEO" }, dur, std::path::Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.clone()));
    }

    std::fs::write(&concat_list_path, &concat_content)
        .map_err(|e| anyhow!("Failed to write concat list: {}", e))?;

    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    // Strip \\?\ extended-length prefix from output path for FFmpeg compatibility
    let output_path_safe = crate::ffmpeg::cards::strip_extended_path_prefix(output_path);

    cmd.args([
        "-hide_banner",
        "-progress", "pipe:2",
        "-f", "concat",
        "-safe", "0",
        "-i", concat_list_path.to_str().unwrap_or_default(),
        "-c", "copy",
        "-y",
        &output_path_safe,
    ]);

    on_progress(MergeProgress {
        percent: 0.0,
        current_time: 0.0,
        total_duration,
        speed: None,
        fps: None,
        current_file: None,
        current_segment_index: None,
        remaining_duration: None,
        bytes_written: None,
        eta_seconds: None,
        phase: MergePhase::Writing,
        overall_percent: Some(0.0),
        stage_name: Some("Creating MKV container...".to_string()),
        stage_percent: Some(0.0),
        current_file_index: None,
        total_files_in_stage: None,
        warning: None,
        is_large_playlist: false,
    });

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn FFmpeg: {}", e))?;

    let stderr = child.stderr.take().ok_or_else(|| anyhow!("Failed to capture FFmpeg stderr"))?;
    let reader = BufReader::new(stderr);
    let mut block_reader = ProgressBlockReader::new();

    let mut last_progress_time = std::time::Instant::now();

    for line in reader.lines() {
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(&concat_list_path);
            anyhow::bail!("Merge cancelled");
        }

        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if let Some(block) = block_reader.feed_line(&line) {
            if let Some(parsed) = parse_progress_block(&block) {
                let elapsed = parsed.out_time_seconds.unwrap_or(0.0);
                let progress_pct = calc_progress_percent(elapsed, total_duration);

                let now = std::time::Instant::now();
                if now.duration_since(last_progress_time).as_millis() >= 250 {
                    on_progress(MergeProgress {
                        percent: progress_pct,
                        current_time: elapsed,
                        total_duration,
                        speed: parsed.speed,
                        fps: parsed.fps,
                        current_file: None,
                        current_segment_index: None,
                        remaining_duration: None,
                        bytes_written: parsed.total_size_bytes,
                        eta_seconds: None,
                        phase: MergePhase::Writing,
                        overall_percent: Some(progress_pct),
                        stage_name: Some("Creating MKV container...".to_string()),
                        stage_percent: Some(progress_pct),
                        current_file_index: None,
                        total_files_in_stage: None,
                        warning: None,
                        is_large_playlist: false,
                    });
                    last_progress_time = now;
                }
            }
        }
    }

    let fast_start = Instant::now();
    const FAST_TIMEOUT_SECS: u64 = 21600;
    loop {
        if fast_start.elapsed().as_secs() > FAST_TIMEOUT_SECS {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(&concat_list_path);
            anyhow::bail!("Fast MKV merge timed out after {} seconds", FAST_TIMEOUT_SECS);
        }
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(&concat_list_path);
            anyhow::bail!("Merge cancelled");
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => { let _ = child.kill(); let _ = child.wait(); anyhow::bail!("FFmpeg wait error: {}", e); }
        }
    };
    let _ = std::fs::remove_file(&concat_list_path);

    on_progress(MergeProgress {
        percent: 100.0,
        current_time: total_duration,
        total_duration,
        speed: None,
        fps: None,
        current_file: None,
        current_segment_index: None,
        remaining_duration: None,
        bytes_written: None,
        eta_seconds: None,
        phase: MergePhase::Writing,
        overall_percent: Some(100.0),
        stage_name: Some("MKV container created".to_string()),
        stage_percent: Some(100.0),
        current_file_index: None,
        total_files_in_stage: None,
        warning: None,
        is_large_playlist: false,
    });

    Ok(())
}

/// Convert MKV to MP4 with automatic codec detection
pub fn convert_mkv_to_mp4(
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
    mkv_path: &str,
    mp4_path: &str,
    total_duration: f64,
    cancel_flag: Arc<AtomicBool>,
    on_progress: impl Fn(MergeProgress) + Send + 'static,
) -> Result<()> {
    let media_info = crate::ffmpeg::probe::probe_file(ffprobe_path, std::path::Path::new(mkv_path))
        .map_err(|e| anyhow!("Failed to probe MKV file: {}", e))?;

    let compat = check_mp4_compatibility(&media_info);

    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    cmd.args(["-hide_banner", "-progress", "pipe:2", "-i", mkv_path]);

    if compat.video_compatible {
        cmd.args(["-c:v", "copy"]);
    } else {
        cmd.args(["-c:v", "libx264", "-preset", "ultrafast"]);
    }

    if compat.audio_compatible {
        cmd.args(["-c:a", "copy"]);
    } else {
        cmd.args(["-c:a", "aac", "-b:a", "192k"]);
    }

    cmd.args(["-c:s", "copy", "-y", mp4_path]);

    on_progress(MergeProgress {
        percent: 0.0,
        current_time: 0.0,
        total_duration,
        speed: None,
        fps: None,
        current_file: None,
        current_segment_index: None,
        remaining_duration: None,
        bytes_written: None,
        eta_seconds: None,
        phase: MergePhase::Writing,
        overall_percent: Some(0.0),
        stage_name: Some("Converting to MP4...".to_string()),
        stage_percent: Some(0.0),
        current_file_index: None,
        total_files_in_stage: None,
        warning: None,
        is_large_playlist: false,
    });

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn FFmpeg for MP4 conversion: {}", e))?;

    let stderr = child.stderr.take().ok_or_else(|| anyhow!("Failed to capture FFmpeg stderr"))?;
    let reader = BufReader::new(stderr);
    let mut block_reader = ProgressBlockReader::new();

    let mut last_progress_time = std::time::Instant::now();

    for line in reader.lines() {
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(mp4_path);
            anyhow::bail!("MP4 conversion cancelled");
        }

        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };

        if let Some(block) = block_reader.feed_line(&line) {
            if let Some(parsed) = parse_progress_block(&block) {
                let elapsed = parsed.out_time_seconds.unwrap_or(0.0);
                let progress_pct = calc_progress_percent(elapsed, total_duration);

                let now = std::time::Instant::now();
                if now.duration_since(last_progress_time).as_millis() >= 250 {
                    on_progress(MergeProgress {
                        percent: progress_pct,
                        current_time: elapsed,
                        total_duration,
                        speed: parsed.speed,
                        fps: parsed.fps,
                        current_file: None,
                        current_segment_index: None,
                        remaining_duration: None,
                        bytes_written: parsed.total_size_bytes,
                        eta_seconds: None,
                        phase: MergePhase::Writing,
                        overall_percent: Some(progress_pct),
                        stage_name: Some("Converting to MP4...".to_string()),
                        stage_percent: Some(progress_pct),
                        current_file_index: None,
                        total_files_in_stage: None,
                        warning: None,
                        is_large_playlist: false,
                    });
                    last_progress_time = now;
                }
            }
        }
    }

    let convert_start = Instant::now();
    const CONVERT_TIMEOUT_SECS: u64 = 21600;
    loop {
        if convert_start.elapsed().as_secs() > CONVERT_TIMEOUT_SECS {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(mp4_path);
            anyhow::bail!("MP4 conversion timed out after {} seconds", CONVERT_TIMEOUT_SECS);
        }
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(mp4_path);
            anyhow::bail!("MP4 conversion cancelled");
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => { let _ = child.kill(); let _ = child.wait(); anyhow::bail!("FFmpeg wait error: {}", e); }
        }
    }

    on_progress(MergeProgress {
        percent: 100.0,
        current_time: total_duration,
        total_duration,
        speed: None,
        fps: None,
        current_file: None,
        current_segment_index: None,
        remaining_duration: None,
        bytes_written: None,
        eta_seconds: None,
        phase: MergePhase::Writing,
        overall_percent: Some(100.0),
        stage_name: Some("MP4 conversion complete".to_string()),
        stage_percent: Some(100.0),
        current_file_index: None,
        total_files_in_stage: None,
        warning: None,
        is_large_playlist: false,
    });

    Ok(())
}

fn check_mp4_compatibility(media_info: &MediaInfo) -> Mp4CodecCompat {
    let mp4_video_compatible = ["h264", "hevc", "mpeg4", "mpeg2video"];
    let mp4_audio_compatible = ["aac", "mp3", "ac3", "eac3"];

    let video_compatible = media_info.video_streams.first()
        .map(|v| mp4_video_compatible.contains(&v.codec_name.as_str()))
        .unwrap_or(true);

    let audio_compatible = media_info.audio_streams.first()
        .map(|a| mp4_audio_compatible.contains(&a.codec_name.as_str()))
        .unwrap_or(true);

    Mp4CodecCompat { video_compatible, audio_compatible }
}
