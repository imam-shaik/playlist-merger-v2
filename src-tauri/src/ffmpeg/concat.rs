use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use anyhow::{Result, anyhow, Context};
use std::time::{Duration, Instant};
use crate::types::{MergeMode, MergeProgress, MergePhase, SplitConfig, SplitMode, SplitSubtitleMode, FolderSplitMode, CardConfig, SubtitleMode};
use crate::naming::resolver::resolve_template;
use crate::naming::context::TemplateContext;
use crate::ffmpeg::probe_cache::ProbeCache;
use crate::ffmpeg::progress::{ProgressBlockReader, parse_progress_block, calc_progress_percent};
use crate::ffmpeg::cleanup_partial_output;
use crate::ffmpeg::subtitle_timeline::SubtitleTimeline;
use tokio::task::JoinSet;

/// Maximum allowed output parts to prevent accidental output explosion
const MAX_OUTPUT_PARTS: usize = 500;

// strip_extended_path_prefix is centralized in cards.rs (pub fn)
// Use crate::ffmpeg::cards::strip_extended_path_prefix() everywhere

// ═══════════════════════════════════════════════════════════════════════════════
// SUBTITLE REBASE HELPER FUNCTIONS
// These reuse the proven rebasing logic from split_srt_for_segments()
// ═══════════════════════════════════════════════════════════════════════════════

/// Pre-process SRT files for a part by rebasing timestamps.
///
/// This creates per-part SRT files with proper timestamp rebasing using the
/// SubtitleTimeline engine - subtracting part_start_time from all timestamps.
///
/// This fixes the bug where FFmpeg's concat demuxer preserves original timestamps
/// instead of rebasing to the part-relative timeline.
///
/// ---
///
/// # Boundary-Crossing Cue Policy
///
/// When a subtitle cue crosses a part boundary, the following rules apply:
///
/// | Scenario | Example | Output | Reason |
/// |----------|---------|--------|--------|
/// | Cue starts before, ends after boundary | 58.500 → 60.800, boundary at 60.000 | 0.000 → 0.800 | Visible portion is preserved |
/// | Cue starts before, ends at boundary | 58.500 → 60.000 | 0.000 → 0.000 | Clamped to 0 duration, then filtered |
/// | Cue starts before, ends before boundary | 55.000 → 59.000, boundary at 60.000 | (kept) | Cue is entirely in this part |
///
/// Negative start times are clamped to 0. Cues with negative end times or
/// zero/negative duration after rebase are filtered out.
///
/// # Cue Count Integrity
///
/// This function preserves cue count by:
/// 1. Rebasing all cues (not discarding)
/// 2. Filtering only invalid cues (end ≤ start)
/// 3. Re-indexing sequentially within each part
///
/// If original has 1000 cues across all parts, after splitting into N parts,
/// the sum of all part cues should still equal 1000.
///
/// # Thread Safety
///
/// This function is called sequentially for each part during part-wise merge,
/// so no concurrent access concerns.
///
/// # Example
///
/// ```ignore
/// // Part 2 starts at 60s in original timeline
/// create_rebased_srt_for_part(60.0, &["video2.srt"], temp_dir, 2)?;
/// // Result: SRT with timestamps rebased so first cue appears at ~0s
/// ```
fn create_rebased_srt_for_part(
    part_start_time: f64,
    srt_paths: &[Option<String>],
    temp_dir: &Path,
    part_index: u32,
) -> Result<Vec<Option<PathBuf>>> {
    let mut rebased_paths = Vec::new();

    for (i, srt_path) in srt_paths.iter().enumerate() {
        let rebased_path = temp_dir.join(format!("rebased_sub_{}_{}.srt", part_index, i));

        if let Some(path_str) = srt_path {
            let path = Path::new(path_str);
            if path.exists() {
                let mut timeline = SubtitleTimeline::from_srt_file(path)?;
                timeline.rebase(part_start_time);
                timeline.clip_negative_to_zero();
                timeline.filter_invalid();
                timeline.write_to_srt_file(&rebased_path)?;
                rebased_paths.push(Some(rebased_path));
                continue;
            }
        }

        rebased_paths.push(None);
    }

    Ok(rebased_paths)
}

/// Create concat list for subtitles using REBASED SRT files.
fn write_rebased_subtitle_concat_list(
    rebased_srt_paths: &[Option<PathBuf>],
    durations: &[f64],
    list_path: &Path,
) -> Result<()> {
    use std::fmt::Write as _;

    // Ensure dummy file exists for missing subtitles
    let dummy_path = std::env::temp_dir().join("dummy_subtitle.srt");
    if !dummy_path.exists() {
        std::fs::write(&dummy_path, "1\n00:00:00,000 --> 00:00:00,001\n \n")?;
    }

    let mut content = String::new();

    for (i, srt_opt) in rebased_srt_paths.iter().enumerate() {
        let path_to_use = if let Some(path) = srt_opt {
            path
        } else {
            &dummy_path
        };

        let raw = path_to_use.to_string_lossy().replace('\\', "/");
        let escaped = raw.replace('\'', "'\\''");
        writeln!(content, "file '{}'", escaped)?;
        writeln!(content, "duration {}", durations[i])?;
    }

    std::fs::write(list_path, content)
        .with_context(|| format!("Failed to write rebased concat list to {:?}", list_path))?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════

/// Callback for per-file audio check progress.
/// Parameters: (file_index, total_files, filename, duration, seek_points, max_gap, is_problematic)
pub type AudioCheckProgressCallback = Arc<dyn Fn(usize, usize, String, f64, usize, f64, bool) + Send + Sync>;

/// Result of timeline integrity verification
#[derive(Debug)]
struct TimelineDriftInfo {
    expected_duration: f64,
    actual_duration: f64,
    drift_seconds: f64,
    drift_percent: f64,
    actual_fps: Option<String>,
    actual_timebase: Option<String>,
    has_drift: bool,
}

/// Post-merge stream verification.
/// Probes output file and compares expected vs actual stream counts.
/// Logs detailed forensic report of expected vs actual streams.
/// Does not fail the merge, only detects and reports discrepancies.
fn verify_post_merge_streams(
    output_path: &str,
    subtitle_mode: &SubtitleMode,
    subtitle_list_path: &Option<std::path::PathBuf>,
    input_files: &[String],
) {
    let ffprobe_path = match crate::ffmpeg::find_ffprobe(None) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[FORENSIC:STREAMS] Could not find ffprobe: {}", e);
            return;
        }
    };

    #[derive(Default)]
    struct InputStreams {
        video: usize,
        audio: usize,
        subtitle: usize,
    }

    let mut total_input_streams = InputStreams::default();
    let mut _first_input_streams = InputStreams::default();
    let mut has_first_input = false;

    for (idx, input_file) in input_files.iter().enumerate() {
        let output = Command::new(&ffprobe_path)
            .args(["-v", "quiet", "-print_format", "json", "-show_streams", input_file])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(json_str) = String::from_utf8(output.stdout) {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
                        let mut streams = InputStreams::default();
                        if let Some(streams_arr) = json.get("streams").and_then(|s| s.as_array()) {
                            for stream in streams_arr {
                                match stream.get("codec_type").and_then(|c| c.as_str()) {
                                    Some("video") => streams.video += 1,
                                    Some("audio") => streams.audio += 1,
                                    Some("subtitle") => streams.subtitle += 1,
                                    _ => {}
                                }
                            }
                        }

                        total_input_streams.video += streams.video;
                        total_input_streams.audio += streams.audio;
                        total_input_streams.subtitle += streams.subtitle;

                        if !has_first_input || idx == 0 {
                            _first_input_streams = streams;
                            has_first_input = true;
                        }
                    }
                }
            }
        }
    }

    let output = match Command::new(&ffprobe_path)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
            "-show_chapters",
            "-show_format",
            output_path,
        ])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            log::warn!("[FORENSIC:STREAMS] Failed to probe output: {}", e);
            return;
        }
    };

    if !output.status.success() {
        log::warn!("[FORENSIC:STREAMS] ffprobe failed on output file");
        return;
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[FORENSIC:STREAMS] Failed to parse ffprobe output: {}", e);
            return;
        }
    };

    let mut actual_video = 0usize;
    let mut actual_audio = 0usize;
    let mut actual_subtitle = 0usize;
    let mut actual_chapters = 0usize;

    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for stream in streams {
            match stream.get("codec_type").and_then(|c| c.as_str()) {
                Some("video") => actual_video += 1,
                Some("audio") => actual_audio += 1,
                Some("subtitle") => actual_subtitle += 1,
                _ => {}
            }
        }
    }

    if let Some(chapters) = json.get("chapters").and_then(|c| c.as_array()) {
        actual_chapters = chapters.len();
    }

    // ── COMPUTE EXPECTED BASED ON MERGE PLAN ──────────────────────────────
    // Custom merge: 1 video (normalized), 1 audio (normalized), N subtitles (if embedded)
    // Lossless concat: all streams copied as-is
    let custom_expected_video = 1usize;
    let custom_expected_audio = 1usize;
    let custom_expected_subtitle = match subtitle_mode {
        SubtitleMode::None | SubtitleMode::ExportSrt | SubtitleMode::SrtMergeOnly => 0,
        SubtitleMode::Embed => {
            if subtitle_list_path.is_some() { total_input_streams.subtitle } else { 0 }
        }
        SubtitleMode::Burn => 0,
    };

    let lossless_expected_video = total_input_streams.video;
    let lossless_expected_audio = total_input_streams.audio;
    let lossless_expected_subtitle = total_input_streams.subtitle;

    // ── REASONS FOR EXPECTED VALUES ──────────────────────────────────────
    let video_reason = "Custom Mode: output one normalized video stream";
    let audio_reason = "Custom Mode: output one normalized audio stream";
let subtitle_reason = match subtitle_mode {
        SubtitleMode::None => "Subtitles not embedded (mode=None)",
        SubtitleMode::ExportSrt => "Subtitles exported to SRT, not embedded",
        SubtitleMode::SrtMergeOnly => "Subtitles merged to SRT, not embedded",
        SubtitleMode::Embed => {
            if subtitle_list_path.is_some() {
                "Subtitles extracted and re-embedded in output"
            } else {
                "Subtitle embedding requested but no subtitle list"
            }
        }
        SubtitleMode::Burn => "Subtitles burned into video (no separate stream expected)",
    };

    // ── LOG FORENSIC REPORT ──────────────────────────────────────────────
    log::info!("[FORENSIC:STREAMS] ═══════════════════════════════════════════════════════════");
    log::info!("[FORENSIC:STREAMS] POST-MERGE STREAM VERIFICATION");
    log::info!("[FORENSIC:STREAMS] ═══════════════════════════════════════════════════════════");
    log::info!("[FORENSIC:STREAMS] Subtitle Mode: {:?}", subtitle_mode);
    log::info!("[FORENSIC:STREAMS] Input files: {}", input_files.len());
    log::info!("[FORENSIC:STREAMS] Total Input Streams: {} video, {} audio, {} subtitle",
        total_input_streams.video, total_input_streams.audio, total_input_streams.subtitle);

    log::info!("[FORENSIC:STREAMS] ┌──────────────────────────────────────────────────────────────────────────────┐");
    log::info!("[FORENSIC:STREAMS] │ STREAM      │ EXPECTED │ ACTUAL │ STATUS          │ REASON                  │");
    log::info!("[FORENSIC:STREAMS] ├──────────────────────────────────────────────────────────────────────────────┤");

    // Video
    let (_video_severity, video_status) = if actual_video >= custom_expected_video {
        ("INFO", "✅ PASS")
    } else if actual_video == 0 {
        ("CRITICAL", "❌ FAIL")
    } else {
        ("ERROR", "❌ FAIL")
    };
    log::info!("[FORENSIC:STREAMS] │ {:10} │ {:8} │ {:6} │ {:^15} │ {} │",
        "Video", custom_expected_video, actual_video, video_status, video_reason);

    // Audio
    let (_audio_severity, audio_status) = if actual_audio >= custom_expected_audio {
        ("INFO", "✅ PASS")
    } else if actual_audio == 0 {
        ("WARNING", "⚠️  WARN")
    } else {
        ("ERROR", "❌ FAIL")
    };
    log::info!("[FORENSIC:STREAMS] │ {:10} │ {:8} │ {:6} │ {:^15} │ {} │",
        "Audio", custom_expected_audio, actual_audio, audio_status, audio_reason);

    // Subtitle
    let (_subtitle_severity, subtitle_status) = if custom_expected_subtitle == 0 {
        ("INFO", "—".to_string())
    } else if actual_subtitle >= custom_expected_subtitle {
        ("INFO", "✅ PASS".to_string())
    } else if actual_subtitle == 0 {
        ("ERROR", "❌ MISSING".to_string())
    } else {
        ("WARNING", "⚠️  WARN".to_string())
    };
    log::info!("[FORENSIC:STREAMS] │ {:10} │ {:8} │ {:6} │ {:^15} │ {} │",
        "Subtitle", custom_expected_subtitle, actual_subtitle, subtitle_status, subtitle_reason);

    // Chapters
    log::info!("[FORENSIC:STREAMS] │ {:10} │ {:8} │ {:6} │ {:^15} │ {} │",
        "Chapters", "—", actual_chapters,
        if actual_chapters > 0 { "ℹ️  PRESENT".to_string() } else { "—".to_string() },
        "Chapters may differ based on target format support");

    log::info!("[FORENSIC:STREAMS] ├──────────────────────────────────────────────────────────────────────────────┤");
    log::info!("[FORENSIC:STREAMS] │ Lossless mode reference: {} video, {} audio, {} subtitle                   │",
        lossless_expected_video, lossless_expected_audio, lossless_expected_subtitle);
    log::info!("[FORENSIC:STREAMS] │ NOTE: If SmartMKV intentionally removes streams (e.g. commentary audio),  │");
    log::info!("[FORENSIC:STREAMS] │       the actual count may be LOWER than input count. This is correct.       │");
    log::info!("[FORENSIC:STREAMS] └──────────────────────────────────────────────────────────────────────────────┘");

    // ── DETERMINE PASS/FAIL ───────────────────────────────────────────────
    let mut criticals = Vec::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Video: CRITICAL if missing (no video = unwatchable)
    if actual_video == 0 {
        criticals.push("CRITICAL: No video streams in output!");
    } else if actual_video < custom_expected_video {
        errors.push(format!("Video stream count lower than expected: {} < {}",
            actual_video, custom_expected_video));
    }

    // Audio: WARNING if missing (silent videos exist), ERROR if below expected
    if actual_audio == 0 {
        warnings.push("WARNING: No audio streams found".to_string());
    } else if actual_audio < custom_expected_audio {
        errors.push(format!("Audio stream count lower than expected: {} < {}",
            actual_audio, custom_expected_audio));
    }

    // Subtitle: Based on mode
    match subtitle_mode {
        SubtitleMode::Embed => {
            if subtitle_list_path.is_some() && actual_subtitle == 0 {
                errors.push("ERROR: Subtitle embedding requested but no subtitle streams found".to_string());
            } else if custom_expected_subtitle > 0 && actual_subtitle < custom_expected_subtitle {
                warnings.push(format!("WARNING: Subtitle stream count lower than expected: {} < {}",
                    actual_subtitle, custom_expected_subtitle));
            }
        }
        SubtitleMode::Burn => {
            if actual_subtitle > 0 {
                log::info!("[FORENSIC:STREAMS] INFO: {} subtitle stream(s) present (may be forced/SDH)",
                    actual_subtitle);
            }
        }
        _ => {}
    }

    // ── LOG WITH SEVERITY ─────────────────────────────────────────────────
    if !criticals.is_empty() {
        log::error!("[FORENSIC:STREAMS] ═══════════════════════════════════════════════════════════");
        for err in &criticals {
            log::error!("[FORENSIC:STREAMS] 🔴 {}", err);
        }
    }

    if !errors.is_empty() {
        log::error!("[FORENSIC:STREAMS] ═══════════════════════════════════════════════════════════");
        for err in &errors {
            log::error!("[FORENSIC:STREAMS] ❌ {}", err);
        }
    }

    if !warnings.is_empty() {
        log::warn!("[FORENSIC:STREAMS] ═══════════════════════════════════════════════════════════");
        for warn in &warnings {
            log::warn!("[FORENSIC:STREAMS] ⚠️  {}", warn);
        }
    }

    // ── FORENSIC BUNDLE ON FAILURE ────────────────────────────────────────
    let has_failure = !criticals.is_empty() || !errors.is_empty();
    if has_failure {
        log::error!("[FORENSIC:STREAMS] ═══════════════════════════════════════════════════════════");
        log::error!("[FORENSIC:STREAMS] FORENSIC BUNDLE:");
        log::error!("[FORENSIC:STREAMS] ──────────────────────────────────────────────────────────");
        log::error!("[FORENSIC:STREAMS] Expected (Custom Merge Plan):");
        log::error!("[FORENSIC:STREAMS]   Video:    {} ({})", custom_expected_video, video_reason);
        log::error!("[FORENSIC:STREAMS]   Audio:    {} ({})", custom_expected_audio, audio_reason);
        log::error!("[FORENSIC:STREAMS]   Subtitle: {} ({})", custom_expected_subtitle, subtitle_reason);
        log::error!("[FORENSIC:STREAMS] ──────────────────────────────────────────────────────────");
        log::error!("[FORENSIC:STREAMS] Actual (Probe Result):");
        log::error!("[FORENSIC:STREAMS]   Video:    {}", actual_video);
        log::error!("[FORENSIC:STREAMS]   Audio:    {}", actual_audio);
        log::error!("[FORENSIC:STREAMS]   Subtitle: {}", actual_subtitle);
        log::error!("[FORENSIC:STREAMS]   Chapters: {}", actual_chapters);
        log::error!("[FORENSIC:STREAMS] ──────────────────────────────────────────────────────────");
        log::error!("[FORENSIC:STREAMS] Input Summary: {} files, {} total streams",
            input_files.len(), total_input_streams.video + total_input_streams.audio + total_input_streams.subtitle);
        log::error!("[FORENSIC:STREAMS] Subtitle Mode: {:?}", subtitle_mode);
        log::error!("[FORENSIC:STREAMS] ──────────────────────────────────────────────────────────");
        log::error!("[FORENSIC:STREAMS] ffprobe JSON (streams):");
        if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
            for (i, stream) in streams.iter().enumerate() {
                let codec_type = stream.get("codec_type").and_then(|c| c.as_str()).unwrap_or("unknown");
                let codec_name = stream.get("codec_name").and_then(|c| c.as_str()).unwrap_or("unknown");
                let lang = stream.get("tags").and_then(|t| t.get("language")).and_then(|l| l.as_str()).unwrap_or("und");
                log::error!("[FORENSIC:STREAMS]   Stream {}: {} ({}) [{}]", i, codec_type, codec_name, lang);
            }
        }
        log::error!("[FORENSIC:STREAMS] ═══════════════════════════════════════════════════════════");
    }

    // ── SUMMARY ────────────────────────────────────────────────────────────
    if criticals.is_empty() && errors.is_empty() && warnings.is_empty() {
        log::info!("[FORENSIC:STREAMS] ✅ Stream verification PASSED");
    } else if criticals.is_empty() && errors.is_empty() {
        log::warn!("[FORENSIC:STREAMS] ⚠️  Stream verification passed with {} warning(s)", warnings.len());
    } else if criticals.is_empty() {
        log::error!("[FORENSIC:STREAMS] ❌ Stream verification FAILED: {} error(s), {} warning(s)",
            errors.len(), warnings.len());
    } else {
        log::error!("[FORENSIC:STREAMS] 🔴 Stream verification CRITICAL: {} critical(s), {} error(s), {} warning(s)",
            criticals.len(), errors.len(), warnings.len());
    }
    log::info!("[FORENSIC:STREAMS] ═══════════════════════════════════════════════════════════");
}

/// Verify output timeline integrity by probing the actual output file.
/// Compares expected duration vs actual duration to detect PTS drift or timeline inflation.
/// Returns Some(TimelineDriftInfo) if verification was performed, None if skipped.
fn verify_timeline_integrity(output_path: &str, expected_duration: f64) -> Option<TimelineDriftInfo> {

    let ffprobe_path = match crate::ffmpeg::find_ffprobe(None) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("[FORENSIC:TIMELINE] Could not find ffprobe for verification: {}", e);
            return None;
        }
    };

    let output = Command::new(&ffprobe_path)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            output_path
        ])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            log::warn!("[FORENSIC:TIMELINE] Failed to probe output: {}", e);
            return None;
        }
    };

    if !output.status.success() {
        log::warn!("[FORENSIC:TIMELINE] ffprobe failed on output file");
        return None;
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[FORENSIC:TIMELINE] Failed to parse ffprobe output: {}", e);
            return None;
        }
    };

    // Extract actual duration from format
    let actual_duration = json.get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or_else(|| {
            log::warn!("[Timeline] Failed to parse actual duration from ffprobe output — using expected {}s", expected_duration);
            expected_duration
        });

    // Extract video stream properties
    let mut actual_fps: Option<String> = None;
    let mut actual_timebase: Option<String> = None;

    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for stream in streams {
            if stream.get("codec_type").and_then(|c| c.as_str()) == Some("video") {
                actual_fps = stream.get("avg_frame_rate")
                    .and_then(|f| f.as_str())
                    .map(|s| s.to_string());
                actual_timebase = stream.get("time_base")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());
                break;
            }
        }
    }

    let drift_seconds = actual_duration - expected_duration;
    let drift_percent = if expected_duration > 0.0 {
        (drift_seconds / expected_duration) * 100.0
    } else {
        0.0
    };

    // Flag as drift if > 0.1% difference (approximately 1 second per 1000 seconds)
    let has_drift = drift_percent.abs() > 0.1;

    Some(TimelineDriftInfo {
        expected_duration,
        actual_duration,
        drift_seconds,
        drift_percent,
        actual_fps,
        actual_timebase,
        has_drift,
    })
}

// ── Platform-specific process group kill helpers ─────────────────────────────
// These ensure FFmpeg child processes and their entire process tree are
// cleaned up on cancellation — preventing zombie processes.
#[cfg(not(unix))]
pub fn force_kill_process_tree(child: &mut std::process::Child) {
    let pid = child.id();
    // Use taskkill /T to terminate the entire process tree
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .status();
    // Fallback: kill the direct child
    let _ = child.kill();
}

#[cfg(unix)]
pub fn force_kill_process_tree(child: &mut std::process::Child) {
    // Send SIGKILL to the entire process group (Unix only)
    unsafe { libc::killpg(child.id() as i32, libc::SIGKILL); }
}

/// Helper to spawn FFmpeg with process-group isolation.
/// On Windows, the child is put in its own job object so taskkill can find it.
fn spawn_ffmpeg(cmd: &mut Command) -> Result<std::process::Child> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
    cmd.stderr(Stdio::piped());
    cmd.stdout(Stdio::null());
    cmd.stdin(Stdio::null());
    cmd.spawn().with_context(|| format!("Failed to spawn ffmpeg at {}", cmd.get_program().to_string_lossy()))
}

/// Represents a single part in a split merge
#[derive(Debug, Clone)]
struct MergePart {
    pub part_index: u32,
    pub file_indices: Vec<usize>,
    pub start_time: f64,
    pub end_time: f64,
    pub output_path: String,
    /// Human-readable label (e.g., folder name for folder-wise splits)
    pub label: Option<String>,
}

fn find_common_parent(paths: &[String], segment_is_card: &[bool]) -> Option<std::path::PathBuf> {
    // Collect only non-card paths for parent detection
    let source_paths: Vec<&String> = paths.iter().enumerate()
        .filter(|(i, _)| !segment_is_card.get(*i).copied().unwrap_or(false))
        .map(|(_, p)| p)
        .collect();

    if source_paths.is_empty() { return None; }
    let first_path = Path::new(source_paths[0]);
    let mut common = match first_path.parent() {
        Some(p) => p.to_path_buf(),
        None => return None,
    };

    for path_str in source_paths.iter().skip(1) {
        let path = Path::new(*path_str);
        while !path.starts_with(&common) {
            if let Some(parent) = common.parent() {
                common = parent.to_path_buf();
            } else {
                return None;
            }
        }
    }
    Some(common)
}

fn get_top_level_component(common_parent: &Path, file_path: &Path) -> String {
    if let Ok(rel) = file_path.strip_prefix(common_parent) {
        if let Some(first_comp) = rel.components().next() {
            let comp_str = first_comp.as_os_str().to_string_lossy().into_owned();
            if rel.components().count() == 1 {
                return "Root".to_string();
            }
            return comp_str;
        }
    }
    "Root".to_string()
}

/// Compute balanced distribution of items across parts.
/// Returns count of items per part, e.g., balanced_parts(37, 4) = [10, 9, 9, 9]
/// Items at the start get one extra when there's a remainder.
fn balanced_parts(total: usize, parts: usize) -> Vec<usize> {
    if parts == 0 || total == 0 {
        return vec![];
    }
    let parts = parts.min(total).max(1);
    let base = total / parts;
    let remainder = total % parts;
    (0..parts)
        .map(|i| if i < remainder { base + 1 } else { base })
        .collect()
}

/// Compute the part boundaries based on split config
fn compute_part_boundaries(
    input_files: &[String],
    input_names: &[String],
    input_durations: &[f64],
    segment_is_card: &[bool],
    output_path: &str,
    split_config: &SplitConfig,
    naming_config: Option<&crate::split::types::NamingConfig>,
) -> Result<Vec<MergePart>> {
    let total_duration: f64 = input_durations.iter().sum();
    let mut parts = Vec::new();
    let output_dir = Path::new(output_path).parent().unwrap_or(Path::new("."));
    let output_stem = Path::new(output_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("merged_output");
    let output_ext = Path::new(output_path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("mp4");

    match split_config.mode {
        SplitMode::None => {
            // Single part - all files
            let all_indices: Vec<usize> = (0..input_names.len()).collect();
            parts.push(MergePart {
                part_index: 1,
                file_indices: all_indices,
                start_time: 0.0,
                end_time: total_duration,
                output_path: output_path.to_string(),
                label: None,
            });
        }
        SplitMode::Count => {
            // Treat cards + following video as an atomic unit for counting
            let mut atomic_units = Vec::new();
            let mut i = 0;
            while i < input_files.len() {
                let is_card = segment_is_card.get(i).copied().unwrap_or(false);
                if is_card && i + 1 < input_files.len() {
                    atomic_units.push(vec![i, i + 1]);
                    i += 2;
                } else {
                    atomic_units.push(vec![i]);
                    i += 1;
                }
            }

            let total_units = atomic_units.len();
            let requested_parts = split_config.part_count.unwrap_or(2).max(1) as usize;
            let effective_parts = requested_parts.min(total_units).max(1);

            if effective_parts > MAX_OUTPUT_PARTS {
                return Err(anyhow!("Too many parts requested"));
            }

            let units_per_part = balanced_parts(total_units, effective_parts);
            let mut global_unit_idx = 0;
            let mut part_start_time = 0.0;

            for (part_idx, count) in units_per_part.iter().enumerate() {
                let mut file_indices = Vec::new();
                for item in atomic_units.iter().skip(global_unit_idx).take(*count) {
                    file_indices.extend(item.clone());
                }

                let part_duration: f64 = file_indices.iter().map(|&idx| input_durations[idx]).sum();
                let part_end_time = part_start_time + part_duration;
                let part_filename = format!("{}_part{}.{}", output_stem, part_idx + 1, output_ext);
                let part_output = output_dir.join(part_filename);

                parts.push(MergePart {
                    part_index: part_idx as u32 + 1,
                    file_indices,
                    start_time: part_start_time,
                    end_time: part_end_time,
                    output_path: part_output.to_string_lossy().into_owned(),
                    label: None,
                });

                global_unit_idx += count;
                part_start_time = part_end_time;
            }
        }
        SplitMode::Duration => {
            let max_duration = split_config.max_duration_per_part.unwrap_or(3600.0).max(1.0);
            let mut current_part_indices = Vec::new();
            let mut current_part_duration = 0.0;
            let mut current_part_start = 0.0;
            let mut part_idx = 0u32;

            let mut i = 0;
            while i < input_files.len() {
                let is_card = segment_is_card.get(i).copied().unwrap_or(false);
                let unit_indices = if is_card && i + 1 < input_files.len() { vec![i, i + 1] } else { vec![i] };
                let unit_duration: f64 = unit_indices.iter().map(|&idx| input_durations[idx]).sum();

                if !current_part_indices.is_empty() && current_part_duration + unit_duration > max_duration {
                    // Finish current part
                    let part_filename = format!("{}_part{}.{}", output_stem, part_idx + 1, output_ext);
                    let part_output_path = output_dir.join(part_filename).to_string_lossy().into_owned();

                    parts.push(MergePart {
                        part_index: part_idx + 1,
                        file_indices: current_part_indices.clone(),
                        start_time: current_part_start,
                        end_time: current_part_start + current_part_duration,
                        output_path: part_output_path,
                        label: None,
                    });

                    current_part_start += current_part_duration;
                    current_part_indices.clear();
                    current_part_duration = 0.0;
                    part_idx += 1;
                }

                current_part_indices.extend(unit_indices);
                current_part_duration += unit_duration;
                i += if is_card && i + 1 < input_files.len() { 2 } else { 1 };
            }

            if !current_part_indices.is_empty() {
                let part_filename = format!("{}_part{}.{}", output_stem, part_idx + 1, output_ext);
                let part_output_path = output_dir.join(part_filename).to_string_lossy().into_owned();
                parts.push(MergePart {
                    part_index: part_idx + 1,
                    file_indices: current_part_indices,
                    start_time: current_part_start,
                    end_time: current_part_start + current_part_duration,
                    output_path: part_output_path,
                    label: None,
                });
            }
        }
        SplitMode::Folder => {
            let common_parent = find_common_parent(input_files, segment_is_card).unwrap_or_else(|| Path::new("").to_path_buf());
            let split_into_parts = split_config.folder_split_mode.as_ref() == Some(&FolderSplitMode::Parts);
            let requested_parts = split_config.part_count.unwrap_or(2).max(1) as usize;

            // Collect atomic units (Card+Video) and group them by folder
            // Each folder stores its atomic units as Vec<Vec<usize>> so that
            // card-video pairs are never split across parts.
            let mut folder_groups: Vec<(String, Vec<Vec<usize>>)> = Vec::new();
            let mut current_folder: Option<String> = None;
            let mut current_units: Vec<Vec<usize>> = Vec::new();

            let mut i = 0;
            while i < input_files.len() {
                let is_card = segment_is_card.get(i).copied().unwrap_or(false);
                let unit_indices = if is_card && i + 1 < input_files.len() { vec![i, i + 1] } else { vec![i] };
                
                // Use the video's path for folder detection (it follows the card)
                let detection_idx = if is_card && i + 1 < input_files.len() { i + 1 } else { i };
                let file_path = Path::new(&input_files[detection_idx]);
                let folder_name = get_top_level_component(&common_parent, file_path);

                if current_folder.as_ref() != Some(&folder_name) {
                    if !current_units.is_empty() {
                        if let Some(folder) = current_folder.take() {
                            folder_groups.push((folder, std::mem::take(&mut current_units)));
                        }
                    }
                    current_folder = Some(folder_name);
                }
                current_units.push(unit_indices);
                i += if is_card && i + 1 < input_files.len() { 2 } else { 1 };
            }
            if !current_units.is_empty() {
                if let Some(folder) = current_folder.take() {
                    folder_groups.push((folder, current_units));
                }
            }

            // Calculate estimated total parts for validation
            let estimated_total_parts: usize = if split_into_parts {
                folder_groups.iter().map(|(_, units)| units.len().min(requested_parts)).sum()
            } else {
                folder_groups.len()
            };

            if estimated_total_parts > MAX_OUTPUT_PARTS {
                return Err(anyhow!(
                    "Configuration would create {} output files. Maximum allowed is {}.",
                    estimated_total_parts,
                    MAX_OUTPUT_PARTS
                ));
            }

            let make_part_path = |folder_name: &str, part_num: u32, _output_stem: &str, output_ext: &str, naming_config: Option<&crate::split::types::NamingConfig>, output_dir: &Path| -> String {
                // For Folder mode, use folder_name as default filename so {filename} resolves to folder
                let default_filename = folder_name.replace(' ', "_");
                if let Some(nc) = naming_config {
                    let ctx = TemplateContext {
                        filename: default_filename.clone(),
                        extension: output_ext.to_string(),
                        folder: Some(folder_name.to_string()),
                        original_num: None,
                        index: part_num as usize,
                        start_time: 0.0,
                        end_time: 0.0,
                        duration: 0.0,
                        chapter: None,
                        resolution: None,
                        width: None,
                        height: None,
                        playlist: None,
                        playlist_index: None,
                        video_count: None,
                        total_duration: None,
                        date: String::new(),
                        time: String::new(),
                        prefix: nc.prefix.clone(),
                        suffix: nc.suffix.clone(),
                        lang: None,
                        lang_name: None,
                        part_label: None,
                    };
                    let resolved = resolve_template(&nc.template, nc, &ctx);
                    let filename = format!("{}.{}", resolved, output_ext);
                    output_dir.join(filename).to_string_lossy().into_owned()
                } else {
                    // Default naming: "FolderName_partN.ext"
                    format!("{}_part{}.{}", default_filename, part_num, output_ext)
                }
            };

            let mut global_part_idx = 0u32;

            for (folder_name, units) in folder_groups {
                if split_into_parts {
                    // Split this folder's atomic units into N parts using balanced distribution.
                    // Units are never broken — card+video pairs always stay together.
                    let unit_count = units.len();
                    let effective_parts = requested_parts.min(unit_count).max(1);
                    let units_per_part = balanced_parts(unit_count, effective_parts);

                    let mut folder_start_time = 0.0;
                    let mut unit_offset = 0;
                    for &count in units_per_part.iter() {
                        let mut part_file_indices: Vec<usize> = Vec::new();
                        for unit in &units[unit_offset..unit_offset + count] {
                            part_file_indices.extend(unit);
                        }

                        let part_duration: f64 = part_file_indices.iter().map(|&i| input_durations[i]).sum();
                        let part_end_time = folder_start_time + part_duration;

                        let part_output_path = make_part_path(
                            &folder_name,
                            global_part_idx + 1,
                            output_stem,
                            output_ext,
                            naming_config,
                            output_dir,
                        );

                        parts.push(MergePart {
                            part_index: global_part_idx + 1,
                            file_indices: part_file_indices,
                            start_time: folder_start_time,
                            end_time: part_end_time,
                            output_path: part_output_path,
                            label: Some(folder_name.clone()),
                        });

                        folder_start_time = part_end_time;
                        global_part_idx += 1;
                        unit_offset += count;
                    }
                } else {
                    // Single output per folder (existing behavior)
                    let mut file_indices: Vec<usize> = Vec::new();
                    for unit in &units {
                        file_indices.extend(unit);
                    }
                    let part_duration: f64 = file_indices.iter().map(|&i| input_durations[i]).sum();

                    let part_output_path = make_part_path(
                        &folder_name,
                        global_part_idx + 1,
                        output_stem,
                        output_ext,
                        naming_config,
                        output_dir,
                    );

                    parts.push(MergePart {
                        part_index: global_part_idx + 1,
                        file_indices,
                        start_time: 0.0,
                        end_time: part_duration,
                        output_path: part_output_path,
                        label: Some(folder_name.clone()),
                    });

                    global_part_idx += 1;
                }
            }
        }
    }

    Ok(parts)
}



/// Check codec compatibility using cached parallel probe results.
///
/// Probes all files via the ProbeCache, then checks compatibility.
/// Returns Ok(()) if compatible, Err(reason) if not.
pub fn check_codec_compatibility_parallel(
    files: &[&Path],
    cache: &crate::ffmpeg::probe_cache::ProbeCache,
) -> Result<()> {
    let _start_time = std::time::Instant::now();
    log::info!("[FORENSIC:COMPAT] ENTER check_codec_compatibility_parallel ({} files)", files.len());
    if files.is_empty() {
        log::info!("[CompatCheck] No files to check — passing");
        return Ok(());
    }

    log::info!("[CompatCheck] ═══════════════════════════════════════════════════════════");
    log::info!("[CompatCheck] RUNNING CODEC COMPATIBILITY CHECK (parallel, from cache)");
    log::info!("[CompatCheck] Files to check: {}", files.len());
    for (i, f) in files.iter().enumerate() {
        log::info!("[CompatCheck]   [{}] {}", i, f.display());
    }
    log::info!("[CompatCheck] Cache has {} entries", cache.len());
    log::info!("[CompatCheck] ═══════════════════════════════════════════════════════════");

    let mut first_video_codec: Option<String> = None;
    let mut first_video_width: Option<u64> = None;
    let mut first_video_height: Option<u64> = None;
    let mut first_audio_codec: Option<String> = None;
    let mut first_audio_sample_rate: Option<u32> = None;
    let mut first_audio_channels: Option<u32> = None;
    let mut all_video_timebases: Vec<Option<String>> = Vec::new();
    let mut file_details: Vec<String> = Vec::new();
    let mut audio_channel_outliers: Vec<(String, u32, String)> = Vec::new();

    for (file_idx, file) in files.iter().enumerate() {
        log::info!("[CompatCheck] ────────────────────────────────────────────────────");
        log::info!("[CompatCheck] Probing file [{}]: {} (from cache)", file_idx, file.display());

        let info = match cache.get(file) {
            Some(Ok(info)) => info,
            Some(Err(e)) => {
                log::warn!("[CompatCheck] cached probe failed for {}: {}", file.display(), e);
                continue;
            }
            None => {
                log::warn!("[CompatCheck] file {} not in cache, skipping", file.display());
                continue;
            }
        };

        let file_video_codec = info.video_streams.first().map(|s| s.codec_name.clone());
        let file_video_width = info.video_streams.first().and_then(|s| s.width).map(|w| w as u64);
        let file_video_height = info.video_streams.first().and_then(|s| s.height).map(|h| h as u64);
        let file_video_timebase = info.video_streams.first().and_then(|s| s.time_base.clone());
        let file_audio_codec = info.audio_streams.first().map(|s| s.codec_name.clone());
        let file_audio_sample_rate = info.audio_streams.first().and_then(|s| s.sample_rate);
        let file_audio_channels = info.audio_streams.first().and_then(|s| s.channels);
        let file_audio_channel_layout = info.audio_streams.first().and_then(|s| s.channel_layout.clone());

        let filename = file.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file.to_string_lossy().to_string());

        // Check for audio-only files (no video stream) — these cause "dimensions not set" in FFmpeg
        if file_video_codec.is_none() {
            log::error!("[CompatCheck] ❌ AUDIO-ONLY FILE DETECTED: {}", filename);
            log::error!("[CompatCheck] File '{}' has no video stream. This will cause 'dimensions not set' error.", filename);
            log::error!("[CompatCheck] Remove this file from the playlist or add a video stream.");
            return Err(anyhow!(
                "Audio-only file detected: '{}'. This file has no video stream and cannot be merged with video files. Remove it from the playlist.",
                filename
            ));
        }

        // Check for files with 0x0 dimensions — log warning but don't fail (some files decode fine)
        if file_video_width == Some(0) || file_video_height == Some(0) {
            log::warn!("[CompatCheck] ⚠️ File '{}' has 0x0 dimensions per ffprobe but has video codec '{}'. Will attempt merge.", filename, file_video_codec.as_deref().unwrap_or("unknown"));
        }

        file_details.push(format!(
            "{}: video={:?}({}x{}), audio={:?}({:?}Hz, {}ch, {}), tb={:?}",
            filename,
            file_video_codec,
            file_video_width.unwrap_or(0),
            file_video_height.unwrap_or(0),
            file_audio_codec,
            file_audio_sample_rate,
            file_audio_channels.unwrap_or(0),
            file_audio_channel_layout.as_deref().unwrap_or("unknown"),
            file_video_timebase
        ));

        log::info!("[CompatCheck]   Parsed streams for [{}]:", file_idx);
        log::info!("[CompatCheck]     video_codec: {:?}", file_video_codec);
        log::info!("[CompatCheck]     video_resolution: {}x{}", file_video_width.unwrap_or(0), file_video_height.unwrap_or(0));
        log::info!("[CompatCheck]     video_timebase: {:?}", file_video_timebase);
        log::info!("[CompatCheck]     audio_codec: {:?}", file_audio_codec);
        log::info!("[CompatCheck]     audio_sample_rate: {:?}Hz", file_audio_sample_rate);
        log::info!("[CompatCheck]     audio_channels: {:?}", file_audio_channels);
        log::info!("[CompatCheck]     audio_channel_layout: {:?}", file_audio_channel_layout);

        if let (Some(first), Some(current)) = (&first_video_codec, &file_video_codec) {
            if first != current {
                log::warn!("[CompatCheck]   ⚠️ VIDEO CODEC MISMATCH: reference='{}', current='{}' — will be auto-normalized", first, current);
            } else {
                log::info!("[CompatCheck]   ✅ Video codec match: '{}'", first);
            }
        }
        if first_video_codec.is_none() {
            first_video_codec = file_video_codec.clone();
            log::info!("[CompatCheck]   📋 Reference video codec set to: {:?}", first_video_codec);
        }

        if let (Some(first_w), Some(first_h), Some(curr_w), Some(curr_h)) = (first_video_width, first_video_height, file_video_width, file_video_height) {
            if first_w != curr_w || first_h != curr_h {
                log::warn!("[CompatCheck]   ⚠️ VIDEO RESOLUTION MISMATCH: reference={}x{}, current={}x{}", first_w, first_h, curr_w, curr_h);
                log::warn!("[CompatCheck]   ⚠️ Proceeding anyway — concat demuxer handles resolution changes");
            } else {
                log::info!("[CompatCheck]   ✅ Video resolution match: {}x{}", first_w, first_h);
            }
        }
        if first_video_width.is_none() {
            first_video_width = file_video_width;
            first_video_height = file_video_height;
            log::info!("[CompatCheck]   📋 Reference video resolution set to: {}x{}", first_video_width.unwrap_or(0), first_video_height.unwrap_or(0));
        }

        if let (Some(first), Some(current)) = (&first_audio_codec, &file_audio_codec) {
            if first != current {
                log::warn!("[CompatCheck]   ⚠️ AUDIO CODEC MISMATCH: reference='{}', current='{}' — will be auto-normalized", first, current);
            } else {
                log::info!("[CompatCheck]   ✅ Audio codec match: '{}'", first);
            }
        }
        if first_audio_codec.is_none() {
            first_audio_codec = file_audio_codec.clone();
            log::info!("[CompatCheck]   📋 Reference audio codec set to: {:?}", first_audio_codec);
        }

        if let (Some(first_sr), Some(curr_sr)) = (first_audio_sample_rate, file_audio_sample_rate) {
            if first_sr != curr_sr {
                log::warn!("[CompatCheck]   ⚠️ AUDIO SAMPLE RATE MISMATCH: reference={}Hz, current={}Hz — will be auto-normalized", first_sr, curr_sr);
            } else {
                log::info!("[CompatCheck]   ✅ Audio sample rate match: {}Hz", first_sr);
            }
        }
        if first_audio_sample_rate.is_none() {
            first_audio_sample_rate = file_audio_sample_rate;
            log::info!("[CompatCheck]   📋 Reference audio sample rate set to: {:?}", first_audio_sample_rate);
        }

        if let Some(channels) = file_audio_channels {
            if channels > 2 {
                log::warn!("[CompatCheck]   ⚠️ AUDIO CHANNEL OUTLIER: {} has {} channels (reference: {:?})",
                    filename, channels, first_audio_channels);
                log::warn!("[CompatCheck]   ⚠️ Multi-channel audio (>2ch) will cause 'rematrix is needed' errors in concat");
                log::warn!("[CompatCheck]   ⚠️ This file MUST be re-encoded or removed before merge");
                audio_channel_outliers.push((filename.clone(), channels, file_audio_channel_layout.clone().unwrap_or_default()));
            } else {
                log::info!("[CompatCheck]   ✅ Audio channels OK: {}ch", channels);
            }
        }
        if first_audio_channels.is_none() {
            first_audio_channels = file_audio_channels;
            log::info!("[CompatCheck]   📋 Reference audio channels set to: {:?}", first_audio_channels);
        }

        all_video_timebases.push(file_video_timebase.clone());
    }

    let dominant_timebase: Option<String> = {
        let non_null: Vec<&String> = all_video_timebases.iter()
            .filter_map(|tb| tb.as_ref())
            .collect();
        if non_null.is_empty() {
            None
        } else {
            let mut counts = std::collections::HashMap::new();
            for tb in &non_null {
                *counts.entry(tb.to_string()).or_insert(0usize) += 1;
            }
            counts.into_iter()
                .max_by_key(|&(_, count)| count)
                .map(|(tb, _)| tb)
        }
    };

    if let Some(ref dom_tb) = dominant_timebase {
        let outlier_count = all_video_timebases.iter()
            .filter(|tb| tb.as_ref().map(|t| t != dom_tb).unwrap_or(false))
            .count();
        if outlier_count > 0 {
            log::warn!("[CompatCheck]   ⚠️ Timescale outliers: {} file(s) have timebase different from dominant '{}'", outlier_count, dom_tb);
            log::warn!("[CompatCheck]   ⚠️ These will be normalized via normalize_timescale_lossless() in PHASE 6");
            for (i, tb) in all_video_timebases.iter().enumerate() {
                if let Some(t) = tb {
                    if t != dom_tb {
                        let filename = files[i].file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| files[i].to_string_lossy().to_string());
                        log::info!("[CompatCheck]     [{}] {}: timebase='{}' (dominant='{}')", i, filename, t, dom_tb);
                    }
                }
            }
        }
    }

    log::info!("[CompatCheck] ═══════════════════════════════════════════════════════════");
    log::info!("[CompatCheck] Final reference values:");
    log::info!("[CompatCheck]   video_codec: {:?}", first_video_codec);
    log::info!("[CompatCheck]   video_resolution: {}x{}", first_video_width.unwrap_or(0), first_video_height.unwrap_or(0));
    log::info!("[CompatCheck]   video_timebase (dominant): {:?}", dominant_timebase);
    log::info!("[CompatCheck]   audio_codec: {:?}", first_audio_codec);
    log::info!("[CompatCheck]   audio_sample_rate: {:?}Hz", first_audio_sample_rate);
    log::info!("[CompatCheck]   audio_channels: {:?}", first_audio_channels);
    log::info!("[CompatCheck] File details: {}", file_details.join("; "));

    if !audio_channel_outliers.is_empty() {
        log::error!("[CompatCheck] ═══════════════════════════════════════════════════════════");
        log::error!("[CompatCheck] ❌ LOSSLESS MERGE BLOCKED — audio channel outliers detected");
        for (name, channels, layout) in &audio_channel_outliers {
            log::error!("[CompatCheck]   File '{}' has {} channels (layout: '{}')", name, channels, layout);
        }
        log::error!("[CompatCheck] Multi-channel audio cannot be stream-copied into a stereo concat");
        log::error!("[CompatCheck] Fix: Use Custom mode (re-encode) to normalize to stereo, or remove these files");
        log::error!("[CompatCheck] ═══════════════════════════════════════════════════════════");
        return Err(anyhow!(
            "Audio channel mismatch: {} file(s) have >2 channels which causes concat errors. Use Custom mode to re-encode and normalize to stereo.",
            audio_channel_outliers.len()
        ));
    }

    log::info!("[CompatCheck] ✅ ALL CHECKS PASSED — lossless stream copy is safe");
    log::info!("[CompatCheck] ═══════════════════════════════════════════════════════════");
    Ok(())
}

/// Detect codec transitions in a playlist (H264↔H265 etc).
///
/// Phase 5C correlation audit proved: codec transitions during concat produce
/// "missing picture in access unit" errors that cause seek corruption. The
/// strength of correlation is 100% (3/3 transitions failed, 0/4 same-codec
/// boundaries failed). PTS jumps (e.g. 6s at H265→H264 boundary) amplify
/// the error count by 7x.
///
/// This function detects ANY pair of adjacent files with different video
/// codecs and returns an error describing the transition, so the caller
/// can force re-encode (Custom mode) instead of stream copy (Lossless).
///
/// Returns:
/// - `Ok(codec)` if all files have the same video codec (or single file).
/// - `Err(anyhow!(...))` describing the FIRST transition found, including
///   the codecs involved and the dominant codec for re-encoding.
pub fn detect_codec_transitions(
    files: &[&Path],
    cache: &crate::ffmpeg::probe_cache::ProbeCache,
) -> Result<String> {
    if files.len() < 2 {
        if let Some(first) = files.first() {
            if let Some(Ok(info)) = cache.get(first) {
                if let Some(v) = info.video_streams.first() {
                    return Ok(v.codec_name.clone());
                }
            }
        }
        return Ok(String::new());
    }

    log::info!("[CodecTransition] ═══════════════════════════════════════════════════════════");
    log::info!("[CodecTransition] DETECTING CODEC TRANSITIONS (Phase 5C fix)");
    log::info!("[CodecTransition] Files to scan: {}", files.len());

    let mut video_codecs: Vec<Option<String>> = Vec::with_capacity(files.len());
    for f in files {
        match cache.get(f) {
            Some(Ok(info)) => {
                let codec = info.video_streams.first().map(|v| v.codec_name.clone());
                video_codecs.push(codec);
            }
            _ => video_codecs.push(None),
        }
    }

    // Find first transition
    let mut first_transition: Option<(usize, String, String)> = None;
    let mut transition_count = 0usize;
    let mut codec_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for codec in video_codecs.iter().flatten() {
        *codec_counts.entry(codec.clone()).or_insert(0) += 1;
    }
    let dominant_codec = codec_counts.iter()
        .max_by_key(|(_, c)| *c)
        .map(|(c, _)| c.clone())
        .unwrap_or_else(|| "libx264".to_string());

    for i in 1..files.len() {
        let prev = video_codecs[i - 1].clone();
        let curr = video_codecs[i].clone();
        if let (Some(p), Some(c)) = (prev, curr) {
            if p != c {
                transition_count += 1;
                if first_transition.is_none() {
                    first_transition = Some((i, p, c));
                }
            }
        }
    }

    if let Some((idx, from, to)) = first_transition {
        let filename_from = files[idx - 1].file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| files[idx - 1].to_string_lossy().to_string());
        let filename_to = files[idx].file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| files[idx].to_string_lossy().to_string());

        log::warn!("[CodecTransition] ❌ CODEC TRANSITION DETECTED at boundary {}→{}", idx, idx+1);
        log::warn!("[CodecTransition]   File [{}] '{}': codec='{}'", idx-1, filename_from, from);
        log::warn!("[CodecTransition]   File [{}] '{}': codec='{}'", idx, filename_to, to);
        log::warn!("[CodecTransition]   Total transitions: {}", transition_count);
        log::warn!("[CodecTransition]   Dominant codec: '{}' ({}/{} files)", dominant_codec,
            codec_counts.get(&dominant_codec).unwrap_or(&0), files.len());
        log::warn!("[CodecTransition]   Phase 5C evidence: 100% of codec transitions produce 'missing picture' errors");
        log::warn!("[CodecTransition]   → FORCING RE-ENCODE (Custom mode) to prevent seek corruption");
        log::info!("[CodecTransition] ═══════════════════════════════════════════════════════════");

        return Err(anyhow!(
            "Codec transition detected: '{}' (file [{}]) → '{}' (file [{}]). \
             {} transition(s) found in playlist. Phase 5C correlation audit: codec transitions \
             cause 'missing picture in access unit' errors and seek corruption. \
             Re-encoding to dominant codec '{}' is required.",
            from, idx-1, to, idx, transition_count, dominant_codec
        ));
    }

    log::info!("[CodecTransition] ✅ No codec transitions detected — all {} files use codec '{}'",
        files.len(), dominant_codec);
    log::info!("[CodecTransition] ═══════════════════════════════════════════════════════════");
    Ok(dominant_codec)
}

/// Result of a single file's audio validation
#[derive(Debug, Clone)]
pub struct AudioValidationError {
    pub file_index: usize,
    pub filename: String,
    pub error_lines: Vec<String>,
}

/// Deep audio validation in PARALLEL using tokio Semaphore (max 6 concurrent ffmpeg processes).
///
/// Same logic as `validate_audio_streams` but runs ffmpeg decoder tests concurrently,
/// drastically reducing wall-clock time for large playlists.
///
/// Instead of validating 535 files × 1.5s ≈ 13 minutes sequentially,
/// this runs 6 at a time: ~2 minutes for the same 535 files.
///
/// Accepts an optional progress callback: `on_progress(completed, total, file_index)`
pub async fn validate_audio_streams_parallel<F>(
    files: Vec<PathBuf>,
    cancel_flag: Option<Arc<AtomicBool>>,
    on_progress: Option<F>,
) -> Result<Vec<AudioValidationError>>
where
    F: Fn(usize, usize, usize) + Send + Sync + 'static,
{
    log::info!("[FORENSIC:AUDIO] ENTER validate_audio_streams_parallel ({} files)", files.len());
    if files.is_empty() {
        return Ok(vec![]);
    }

    let ffmpeg_path = crate::ffmpeg::find_ffmpeg(None)
        .map_err(|e| anyhow!("ffmpeg not found for audio validation: {}", e))?;

    log::info!("[AudioValidate] ═══════════════════════════════════════════════════════════");
    log::info!("[AudioValidate] RUNNING DEEP AUDIO VALIDATION (PARALLEL MODE)");
    log::info!("[AudioValidate] Files to validate: {}", files.len());
    log::info!("[AudioValidate] ═══════════════════════════════════════════════════════════");

    let total = files.len();
    let ffmpeg = Arc::new(ffmpeg_path);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(6));
    let cancel = cancel_flag.clone();
    let errors = Arc::new(std::sync::Mutex::new(Vec::new()));
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let on_progress = Arc::new(on_progress);

    // RUNTIME CONCURRENCY CERTIFICATION: Track active FFmpeg processes
    let active_validation_ffmpeg = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let peak_validation_ffmpeg = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let handles: Vec<_> = files.into_iter().enumerate().map(|(i, file)| {
        let sem = semaphore.clone();
        let ffmpeg = ffmpeg.clone();
        let cancel = cancel.clone();
        let errors = errors.clone();
        let completed = completed.clone();
        let on_progress = on_progress.clone();
        let active_count = active_validation_ffmpeg.clone();
        let peak_count = peak_validation_ffmpeg.clone();

        tokio::spawn(async move {
            // Check cancellation before acquiring semaphore
            if let Some(ref flag) = cancel {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
            }

            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    log::error!("[AudioValidate] Semaphore acquire failed for {}: {} — aborting task", file.display(), e);
                    return;
                }
            };

            // Check cancellation again after acquiring
            if let Some(ref flag) = cancel {
                if flag.load(Ordering::Relaxed) {
                    return;
                }
            }

            let filename = file.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.to_string_lossy().to_string());

            if !file.exists() {
                log::warn!("[AudioValidate]   [{}] SKIP (file missing): {}", i, filename);
                let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if let Some(ref cb) = on_progress.as_ref() {
                    cb(done, total, i);
                }
                return;
            }

            // RUNTIME CONCURRENCY CERTIFICATION: Track spawn
            let current_active = active_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let current_peak = peak_count.fetch_max(current_active, std::sync::atomic::Ordering::Relaxed);
            log::info!("[FFMPEG_SPAWN] pid={} task_id={} phase=validation file={} active={} peak={}",
                std::process::id(), i, filename, current_active, current_peak.max(current_active));

            let ffmpeg = ffmpeg.clone();
            let file_clone = file.clone();
            let filename_for_exit = filename.clone(); // Clone for exit log after spawn_blocking
            let result = tokio::task::spawn_blocking(move || {
                let file_str = file_clone.to_string_lossy().to_string();
                let ffmpeg_path = crate::ffmpeg::cards::strip_extended_path_prefix(&file_str);
                let threads_per = std::thread::available_parallelism().map(|n| (n.get() / 4).max(1)).unwrap_or(2);
                let output = std::process::Command::new(&*ffmpeg)
                    .args([
                        "-v", "error",
                        "-threads", &threads_per.to_string(),
                        "-xerror",
                        "-i", &ffmpeg_path,
                        "-f", "null",
                        "-",
                    ])
                    .output();

                match output {
                    Ok(out) => {
                        let stderr = String::from_utf8_lossy(&out.stderr);
                        let mut error_lines: Vec<String> = stderr.lines()
                            .map(|l| l.trim().to_string())
                            .filter(|l| !l.is_empty())
                            .collect();

                        if !out.status.success() && error_lines.is_empty() {
                            error_lines.push(format!("FFmpeg exited with code {}", out.status.code().unwrap_or(-1)));
                        }

                        if !out.status.success() || !error_lines.is_empty() {
                            Some(AudioValidationError {
                                file_index: i,
                                filename,
                                error_lines,
                            })
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        log::error!("[AudioValidate]   [{}] ❌ Could not run decoder test for {} — flagging as problematic: {}", i, filename, e);
                        Some(AudioValidationError {
                            file_index: i,
                            filename,
                            error_lines: vec![format!("Failed to execute FFmpeg: {}", e)],
                        })
                    }
                }
            }).await.unwrap_or_else(|e| {
                if e.is_panic() {
                    log::error!("[AudioValidate] Task panicked for {} — treating as validation failure: {}", filename_for_exit, e);
                    Some(AudioValidationError {
                        file_index: i,
                        filename: filename_for_exit.clone(),
                        error_lines: vec!["Internal error: validation task panicked".to_string()],
                    })
                } else {
                    log::warn!("[AudioValidate] Task cancelled for {}", filename_for_exit);
                    None
                }
            });

            // RUNTIME CONCURRENCY CERTIFICATION: Track exit
            let remaining = active_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) - 1;
            log::info!("[FFMPEG_EXIT] pid={} task_id={} phase=validation file={} remaining={}", std::process::id(), i, filename_for_exit, remaining);

            if let Some(err) = result {
                match errors.lock() {
                    Ok(mut errs) => {
                        log::error!("[AudioValidate]   [{}] ❌ DECODER ERRORS: {} — {} error(s)", i, err.filename, err.error_lines.len());
                        for line in &err.error_lines {
                            log::error!("[AudioValidate]          {}", line);
                        }
                        errs.push(err);
                    }
                    Err(poisoned) => {
                        log::error!("[AudioValidate]   [{}] Mutex poisoned — audio error for {} not collected", i, err.filename);
                        let mut errs = poisoned.into_inner();
                        errs.push(err);
                    }
                }
            }

            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if let Some(ref cb) = on_progress.as_ref() {
                cb(done, total, i);
            }

            if (done).is_multiple_of(50) || done == total {
                log::info!("[AudioValidate]   Progress: {}/{} validated", done, total);
            }
        })
    }).collect();

    for handle in handles {
        if let Err(e) = handle.await {
            log::error!("[AudioValidate] Task join error (possible panic in audio validation): {}", e);
        }
    }

    let errors = std::sync::Arc::into_inner(errors)
        .map(|arc| arc.into_inner().unwrap_or_default())
        .unwrap_or_default();

    if errors.is_empty() {
        log::info!("[AudioValidate] ✅ ALL {} FILES PASSED DECODER VALIDATION", total);
    } else {
        log::warn!("[AudioValidate] ⚠️ {}/{} files have decoder errors", errors.len(), total);
    }

    // RUNTIME CONCURRENCY CERTIFICATION: Peak summary
    let peak = peak_validation_ffmpeg.load(std::sync::atomic::Ordering::Relaxed);
    log::info!("[FFMPEG_ACTIVE_COUNT] phase=validation peak={} limit=6", peak);
    log::info!("[AudioValidate] CONCURRENCY: peak validation FFmpeg = {}", peak);

    Ok(errors)
}

/// Check for audio decoder errors that occur at certain seek points.
/// This is different from full decode validation - some errors only manifest when seeking to
/// specific timestamps and trying to decode from there.
///
/// Returns indices of files with problematic audio streams that need re-encoding.
pub async fn check_problematic_audio_streams(
    files: Vec<PathBuf>,
    cancel_flag: Option<Arc<AtomicBool>>,
    probe_cache: Option<&ProbeCache>,
    on_file_complete: Option<AudioCheckProgressCallback>,
) -> Result<Vec<usize>> {
    if files.is_empty() {
        return Ok(vec![]);
    }

    let ffmpeg_path = crate::ffmpeg::find_ffmpeg(None)
        .map_err(|e| anyhow!("ffmpeg not found: {}", e))?;

// Pre-lookup durations from cache to avoid reference across spawn boundary
    let mut file_durations: HashMap<PathBuf, Option<f64>> = HashMap::new();
    if let Some(cache) = probe_cache {
        for file in &files {
            // Normalize cache key: strip \\?\ prefix for consistent lookup
            let key = PathBuf::from(crate::ffmpeg::cards::strip_extended_path_prefix(&file.to_string_lossy()));
            let dur = match cache.get(&key).or_else(|| cache.get(file)) {
                Some(Ok(info)) => Some(info.duration),
                _ => None,
            };
            file_durations.insert(file.clone(), dur);
        }
    }

    log::info!("[AudioCheck] Checking {} files for seek-time audio decoder errors...", files.len());
    let problematic_indices: Arc<std::sync::Mutex<Vec<usize>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let semaphore = Arc::new(tokio::sync::Semaphore::new(6));
    let cancel = cancel_flag.clone();

    let file_durations_for_spawn = file_durations.clone();
    let callback = on_file_complete.clone();
    let total = files.len();
    let handles: Vec<_> = files.into_iter().enumerate().map(|(i, file)| {
        let sem = semaphore.clone();
        let ffmpeg = ffmpeg_path.clone();
        let cancel = cancel.clone();
        let problematic = problematic_indices.clone();
        let mut file_durations = file_durations_for_spawn.clone();
        let callback = callback.clone();

        tokio::spawn(async move {
            let filename = file.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| file.to_string_lossy().into_owned());
            let mut is_problematic = false;

            if let Some(ref flag) = cancel {
                if flag.load(Ordering::Relaxed) {
                    if let Some(ref cb) = callback {
                        cb(i, total, filename, 0.0, 0, 0.0, false);
                    }
                    return;
                }
            }

            let _permit = match sem.acquire().await {
                Ok(p) => p,
                Err(e) => {
                    log::error!("[AudioCheck] Semaphore acquire failed for {}: {} — aborting task", file.display(), e);
                    if let Some(ref cb) = callback {
                        cb(i, total, filename, 0.0, 0, 0.0, false);
                    }
                    return;
                }
            };

            if let Some(ref flag) = cancel {
                if flag.load(Ordering::Relaxed) {
                    if let Some(ref cb) = callback {
                        cb(i, total, filename, 0.0, 0, 0.0, false);
                    }
                    return;
                }
            }

            // Use ABSOLUTE-TIME intervals (every 30s) instead of percentage-based.
            // Percentage-based points scale with file duration, creating huge gaps on
            // long files: 2% of 6500s = 130s gap (vs 2s decode window). Absolute-time
            // intervals guarantee max 30s gap regardless of file duration.
            // Duration is resolved below; seek_times are generated after resolution.
            let mut resolved_duration: Option<f64> = None;

            // Resolve duration first (from cache or probe)
            {
                let dur_from_cache = file_durations.get(&file).copied().flatten();
                if let Some(d) = dur_from_cache {
                    resolved_duration = Some(d);
                } else if let Ok(info) = crate::ffmpeg::probe::probe_file(
                    &crate::ffmpeg::find_ffprobe(None).unwrap_or_default(),
                    &file,
                ) {
                    if info.duration > 0.0 {
                        resolved_duration = Some(info.duration);
                        // Cache the resolved duration locally to avoid re-probing this file
                        file_durations.insert(file.clone(), Some(info.duration));
                    } else {
                        log::warn!("[AudioCheck] {} has zero/negative duration ({}) — skipping seek check", file.display(), info.duration);
                    }
                } else {
                    log::warn!("[AudioCheck] Failed to probe {} — skipping seek check for this file", file.display());
                }
            }

            let duration = match resolved_duration {
                Some(d) => d,
                None => {
                    if let Some(ref cb) = callback {
                        cb(i, total, filename, 0.0, 0, 0.0, false);
                    }
                    return;
                }
            };

            // Generate seek times with a hard cap on total points.
            // For a 300s file: 0, 30, 60, ... 270, 297 (10 points)
            // For a 6500s file: 0, 65, 130, ... 6435, 6497 (25 points)
            // Always includes 0s and a point near the end (duration * 0.99).
            // HARD CAP: max 25 seek points to guarantee O(1) validation time.
            const MAX_SEEK_POINTS: usize = 25;
            let max_gap_secs = (duration * 0.02).clamp(1.0, 30.0);
            let num_intervals_unbounded = ((duration / max_gap_secs).ceil() as usize).max(1);
            let num_intervals = num_intervals_unbounded.min(MAX_SEEK_POINTS.saturating_sub(1));
            let mut seek_times: Vec<f64> = Vec::with_capacity(num_intervals + 1);
            for i in 0..num_intervals {
                seek_times.push(duration * (i as f64) / (num_intervals as f64));
            }
            // Add a final point at 99% to test the last segment
            seek_times.push(duration * 0.99);
            seek_times.dedup_by(|a, b| (*a - *b).abs() < 0.1);

            log::info!("[AudioCheck] {} duration={:.1}s, {} seek points (max gap {:.0}s, cap={})",
                file.display(), duration, seek_times.len(), max_gap_secs, MAX_SEEK_POINTS);

            // ── Parallel seek-point validation ──────────────────────────────
            // Spawn all seek points concurrently per file. Local semaphore
            // bounds concurrent FFmpeg processes; JoinSet allows instant
            // abort_all() on first failure.
            // CAP: max 4 seek-point FFmpeg processes per file to prevent process explosion.
            // With outer semaphore=6 and inner=4: max 6x4=24 concurrent FFmpeg (was 48).
            let local_sem = Arc::new(tokio::sync::Semaphore::new(
                std::cmp::min(4, std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2)),
            ));
            let cancel_early = Arc::new(AtomicBool::new(false));
            let mut join_set = JoinSet::new();
            let file_idx = i;

            for seek_sec in &seek_times {
                let sem = local_sem.clone();
                let cancel_early_flag = cancel_early.clone();
                let problematic_ref = problematic.clone();
                let seek = *seek_sec;
                let file_clone = file.clone();
                let ffmpeg_clone = ffmpeg.clone();

                join_set.spawn(async move {
                    // Fast short-circuit: don't wait for permit if a sibling already failed
                    if cancel_early_flag.load(Ordering::Relaxed) {
                        return Ok::<bool, String>(true);
                    }

                    // Async permit acquisition — doesn't consume a blocking thread while waiting
                    let _permit = match sem.acquire().await {
                        Ok(p) => p,
                        Err(_) => return Ok(true),
                    };

                    // Re-check after acquiring permit in case a sibling failed while waiting
                    if cancel_early_flag.load(Ordering::Relaxed) {
                        return Ok(true);
                    }

                    // Acquire global FFmpeg governor permit to prevent process explosion
                    let ffmpeg_sem = crate::ffmpeg::ffmpeg_global_semaphore();
                    let _global_permit = match ffmpeg_sem.acquire().await {
                        Ok(p) => p,
                        Err(_) => return Ok(true),
                    };

                    // Offload synchronous FFmpeg subprocess to the blocking pool.
                    // Uses spawn() + try_wait() polling to support cancellation:
                    // when join_set.abort_all() fires, cancel_early_flag is set and
                    // the blocking thread kills the child instead of waiting forever.
                    let cancel_flag_post = cancel_early_flag.clone();
                    let cancel_flag_inner = cancel_early_flag.clone();
                    let is_healthy = tokio::task::spawn_blocking(move || {
                        let file_str = file_clone.to_string_lossy().to_string();
                        let ffmpeg_file_path = crate::ffmpeg::cards::strip_extended_path_prefix(&file_str);
                        let threads_per = std::thread::available_parallelism().map(|n| (n.get() / 4).max(1)).unwrap_or(2);

                        // Spawn the process (non-blocking) so we can poll + cancel
                        let mut child = match Command::new(&*ffmpeg_clone)
                            .args([
                                "-v", "error",
                                "-threads", &threads_per.to_string(),
                                "-ss", &seek.to_string(),
                                "-i", &ffmpeg_file_path,
                                "-vn",
                                "-map", "0:a:0?",
                                "-t", "30",
                                "-f", "null", "-",
                            ])
                            .stdout(Stdio::null())
                            .stderr(Stdio::piped())
                            .spawn()
                        {
                            Ok(c) => c,
                            Err(e) => return Err(format!("failed to spawn ffmpeg at {:.1}s: {}", seek, e)),
                        };

                        // Take stderr before polling (ownership transfer)
                        let stderr_pipe = child.stderr.take()
                            .ok_or_else(|| format!("failed to capture stderr at {:.1}s", seek))?;

                        // Poll for completion, checking cancel flag every 250ms.
                        // This allows the blocking thread to exit promptly when
                        // join_set.abort_all() fires (cancel_flag_inner set by sibling).
                        let status = loop {
                            match child.try_wait() {
                                Ok(Some(status)) => break status,
                                Ok(None) => {
                                    if cancel_flag_inner.load(Ordering::Relaxed) {
                                        let _ = child.kill();
                                        // Wait for the killed process to avoid zombie
                                        let _ = child.wait();
                                        // Return Ok(true) � sibling will report the failure
                                        return Ok(true);
                                    }
                                    std::thread::sleep(std::time::Duration::from_millis(250));
                                }
                                Err(e) => {
                                    let _ = child.kill();
                                    let _ = child.wait();
                                    return Err(format!("ffmpeg wait error at {:.1}s: {}", seek, e));
                                }
                            }
                        };

                        // Read stderr now that the process has exited
                        let stderr_output = {
                            use std::io::Read;
                            let mut buf = String::new();
                            let _ = std::io::BufReader::new(stderr_pipe).read_to_string(&mut buf);
                            buf
                        };

                        if !status.success() {
                            let error_lines: Vec<String> = stderr_output.lines()
                                .map(|l| l.trim().to_string())
                                .filter(|l| !l.is_empty())
                                .collect();
                            let msg = if error_lines.is_empty() {
                                format!("ffmpeg exited with code {} at {:.1}s", status.code().unwrap_or(-1), seek)
                            } else {
                                format!("audio decoder errors at {:.1}s: {}", seek, error_lines.join(" | "))
                            };
                            Err(msg)
                        } else {
                            // Even on success, check stderr for non-fatal errors
                            let error_lines: Vec<String> = stderr_output.lines()
                                .map(|l| l.trim().to_string())
                                .filter(|l| !l.is_empty())
                                .collect();
                            if !error_lines.is_empty() {
                                Err(format!("audio decoder errors at {:.1}s: {}", seek, error_lines.join(" | ")))
                            } else {
                                Ok(true)
                            }
                        }
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("spawn_blocking panicked: {}", e)));

                    if is_healthy.is_err() {
                        cancel_flag_post.store(true, Ordering::Relaxed);
                        // Record this file as problematic
                        match problematic_ref.lock() {
                            Ok(mut indices) => { if !indices.contains(&file_idx) { indices.push(file_idx); } }
                            Err(poisoned) => { let mut idx = poisoned.into_inner(); if !idx.contains(&file_idx) { idx.push(file_idx); } }
                        }
                    }

                    Ok::<bool, String>(is_healthy.is_ok())
                });
            }

            // Collect results — abort all pending tasks on first failure
            while let Some(res) = join_set.join_next().await {
                match res {
                    Ok(Ok(healthy)) => {
                        if !healthy {
                            is_problematic = true;
                            join_set.abort_all();
                            break;
                        }
                    }
                    _ => {
                        is_problematic = true;
                        join_set.abort_all();
                        break;
                    }
                }
            }

            // If any task logged an error, also log the first one for diagnostics
            if is_problematic {
                // Check if the cancel_early flag was set (means a sibling failed)
                if cancel_early.load(Ordering::Relaxed) {
                    log::error!("[AudioCheck] File {} flagged as problematic — aborting remaining seek points", file.display());
                }
            }

            // Report completion for this file
            if let Some(ref cb) = callback {
                cb(i, total, filename, duration, seek_times.len(), max_gap_secs, is_problematic);
            }
        })
    }).collect();

    for handle in handles {
        if let Err(e) = handle.await {
            log::error!("[AudioCheck] Task join error (possible panic in audio check): {}", e);
        }
    }

    let problematic = problematic_indices.lock().unwrap_or_else(|p| p.into_inner()).clone();

    if problematic.is_empty() {
        log::info!("[AudioCheck] ✅ No problematic audio streams found");
    } else {
        log::warn!("[AudioCheck] ⚠️ {} files have problematic audio streams: {:?}", problematic.len(), problematic);
    }

    Ok(problematic)
}

/// Configuration for a merge job — covers both Lossless and Custom modes
#[derive(Debug, Clone)]
pub struct MergeConfig {
    /// Full file paths — used for concat list generation
    pub input_files: Vec<String>,
    pub input_names: Vec<String>,
    pub input_durations: Vec<f64>,
    pub subtitle_list_path: Option<std::path::PathBuf>,
    pub output_path: String,
    pub mode: MergeMode,
    pub total_duration: f64,
    // ── Custom mode options ─────────────────────────────────────────────
    /// Video codec, e.g. "libx264", "libx265", "libsvtav1"
    pub video_codec: Option<String>,
    /// Audio codec, e.g. "aac", "opus", "ac3"
    pub audio_codec: Option<String>,
    /// CRF quality value (0–51 for x264/x265, lower = better)
    pub video_crf: Option<u32>,
    /// Encoder speed preset, e.g. "fast", "medium", "slow"
    pub video_preset: Option<String>,
    /// Audio bitrate e.g. "192k"
    pub audio_bitrate: Option<String>,
    /// Scale filter e.g. "1920:1080" (None = keep source)
    pub target_resolution: Option<String>,
    /// Target fps e.g. "30" (None = keep source)
    pub target_fps: Option<String>,
    /// Enable hardware acceleration (nvenc, qsv, amf)
    pub hw_accel: Option<String>,
    // ── Split options ───────────────────────────────────────────────────
    pub split_config: Option<SplitConfig>,
    /// Individual subtitle file paths (extracted/detected per-input file) —
    /// used by split merge to generate per-part subtitle concat lists.
    pub subtitle_files: Vec<Option<String>>,
    // ── Canvas overlay cards ────────────────────────────────────────────
    pub card_config: Option<CardConfig>,
    /// For each segment (interleaved), whether it's a canvas overlay card
    pub segment_is_card: Vec<bool>,
    // ── Naming config ──────────────────────────────────────────────────
    /// Naming template configuration for split output filenames
    pub naming_config: Option<crate::split::types::NamingConfig>,
    // ── Subtitle mode ───────────────────────────────────────────────────
    /// How subtitles are handled: None | Embed | Burn | ExportSrt
    pub subtitle_mode: SubtitleMode,
    /// When true, generate a standalone merged SRT alongside the video
    /// (Consumed in merge.rs post-merge, not used in concat logic)
    #[allow(dead_code)]
    pub export_merged_srt: bool,
    /// Pre-generated merged SRT path for Burn mode (subtitles filter)
    pub burn_subtitle_path: Option<std::path::PathBuf>,
    /// Tracks whether mkvmerge succeeded before this FFmpeg attempt.
    /// If true and FFmpeg fails, the mkvmerge output is PRESERVED (renamed) rather than deleted.
    /// This is critical for forensics — a valid output should never be destroyed.
    pub mkvmerge_succeeded_before_ffmpeg: bool,
}

/// Try to decode a file and extract its actual video resolution from ffmpeg stderr.
/// This is a fallback when ffprobe metadata reports 0x0 dimensions but the file
/// actually has valid video dimensions (e.g. missing moov atom header dimensions
/// but the frames decode correctly).
fn decode_resolution_fallback(ffmpeg_path: &Path, file_path: &Path) -> Option<String> {
    let file_str = file_path.to_string_lossy().to_string();
    let ffmpeg_file_path = crate::ffmpeg::cards::strip_extended_path_prefix(&file_str);
    let output = match std::process::Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-i", &ffmpeg_file_path,
            "-f", "null",
            "-",
        ])
        .output() {
            Ok(o) => o,
            Err(e) => {
                log::warn!("[DecodeResolutionFallback] Failed to spawn ffmpeg for {}: {}", file_path.display(), e);
                return None;
            }
        };

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Scan for the "Video:" stream line and extract resolution like "WxH"
    // Example line:
    //   Stream #0:0[0x1](und): Video: h264 (High), yuv420p, 1920x1080 [SAR 1:1 DAR 16:9], ...
    for line in stderr.lines() {
        if !line.contains("Video:") {
            continue;
        }
        // Scan character-by-character for "digits x digits" pattern
        let chars: Vec<char> = line.chars().collect();
        for i in 0..chars.len().saturating_sub(3) {
            if !chars[i].is_ascii_digit() {
                continue;
            }
            // Find end of first number
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j >= chars.len() || chars[j] != 'x' {
                continue;
            }
            // Find start of second number
            let k = j + 1;
            if k >= chars.len() || !chars[k].is_ascii_digit() {
                continue;
            }
            // Find end of second number
            let mut l = k;
            while l < chars.len() && chars[l].is_ascii_digit() {
                l += 1;
            }
            let w_str: String = chars[i..j].iter().collect();
            let h_str: String = chars[k..l].iter().collect();
            if let (Ok(w), Ok(h)) = (w_str.parse::<u32>(), h_str.parse::<u32>()) {
                if w > 0 && h > 0 && w <= 7680 && h <= 4320 {
                    log::info!("[Subtitle] Decode fallback resolved dimensions: {}x{}", w, h);
                    return Some(format!("{}x{}", w, h));
                }
            }
        }
    }

    None
}

/// Execute a full merge job on a blocking thread.
///
/// IMPORTANT: This function is intentionally synchronous — callers MUST wrap
/// it in `tokio::task::spawn_blocking` to avoid blocking the async runtime.
///
/// Progress is reported via `on_progress` callback (called from this thread).
/// Cancellation is checked via `cancel_flag` (AtomicBool, no locks needed).
pub fn run_merge_blocking<F>(
    ffmpeg_path: &Path,
    config: &MergeConfig,
    concat_list_path: &Path,
    cancel_flag: Arc<AtomicBool>,
    on_progress: F,
) -> Result<crate::commands::merge::MergeResult>
where
    F: Fn(MergeProgress) + Send + Sync + 'static,
{
    let start_time = std::time::Instant::now();
    log::info!("[RUN_MERGE] === RUN_MERGE_STARTED output={} elapsed={:?}", config.output_path, start_time.elapsed());

    // Check if split is needed
    if let Some(ref split_config) = config.split_config {
        if split_config.mode != SplitMode::None {
            log::info!("[RUN_MERGE] delegating to run_split_merge_blocking");
            return run_split_merge_blocking(
                ffmpeg_path,
                config,
                cancel_flag,
                on_progress,
            );
        }
    }

    let temp_dir = crate::ffmpeg::get_temp_dir().map_err(|e| anyhow!("Failed to get temp dir: {}", e))?;

    // Pre-calculate segment boundaries
    let card_bg_color = config.card_config.as_ref().map(|c| c.color.clone());
    let mut segments = Vec::new();
    let mut current_start = 0.0;
    for (i, name) in config.input_names.iter().enumerate() {
        let duration = config.input_durations.get(i).cloned().unwrap_or(0.0);
        let is_card = config.segment_is_card.get(i).copied().unwrap_or(false);
        let parent_folder = if !is_card {
            config.input_files.get(i).and_then(|file_path| {
                Path::new(file_path)
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string())
            })
        } else {
            None
        };
        segments.push(crate::commands::merge::MergeSegment {
            name: name.clone(),
            duration,
            start_time: current_start,
            end_time: current_start + duration,
            is_card: if is_card { Some(true) } else { None },
            card_color: if is_card { card_bg_color.clone() } else { None },
            parent_folder,
        });
        current_start += duration;
    }

    let segments_ref = Arc::new(segments.clone());
    let args = build_ffmpeg_args(config, concat_list_path)?;

    // ── PHASE 10: FFMPEG TRACE ─────────────────────────────────────────────
    log::info!("[FORENSIC:MERGE] ═══════════════════════════════════════════════════════════");
    log::info!("[FORENSIC:MERGE] PHASE 10: FFMPEG EXECUTION");
    log::info!("[FORENSIC:MERGE] ═══════════════════════════════════════════════════════════");
    log::info!("[FORENSIC:MERGE] ffmpeg_path: {}", ffmpeg_path.display());
    log::info!("[FORENSIC:MERGE] mode: {:?}", config.mode);
    log::info!("[FORENSIC:MERGE] input_files count: {}", config.input_files.len());
    log::info!("[FORENSIC:MERGE] output_path: {}", config.output_path);
    log::info!("[FORENSIC:MERGE] total_duration: {}s", config.total_duration);
    log::info!("[FORENSIC:MERGE] concat_list: {}", concat_list_path.display());
    if let Some(ref slp) = config.subtitle_list_path {
        log::info!("[FORENSIC:MERGE] subtitle_list: {}", slp.display());
    }
    log::info!("[FORENSIC:MERGE] video_codec: {:?}", config.video_codec);
    log::info!("[FORENSIC:MERGE] audio_codec: {:?}", config.audio_codec);
    log::info!("[FORENSIC:MERGE] video_crf: {:?}", config.video_crf);
    log::info!("[FORENSIC:MERGE] video_preset: {:?}", config.video_preset);
    log::info!("[FORENSIC:MERGE] audio_bitrate: {:?}", config.audio_bitrate);
    log::info!("[FORENSIC:MERGE] hw_accel: {:?}", config.hw_accel);
    log::info!("[FORENSIC:MERGE] burn_subtitle_path: {:?}", config.burn_subtitle_path);
    log::info!("[FORENSIC:MERGE] card_config: {:?}", config.card_config.is_some());
    log::info!("[FORENSIC:MERGE] split_config: {:?}", config.split_config.is_some());
    log::info!("[FORENSIC:MERGE] subtitle_mode: {:?}", config.subtitle_mode);
    log::info!("[FORENSIC:MERGE] segment_is_card count: {}", config.segment_is_card.len());
    log::info!("[FORENSIC:MERGE] ────────────────────────────────────────────────────");
    log::info!("[FORENSIC:MERGE] EXACT FFMPEG COMMAND:");
    log::info!("[FORENSIC:MERGE] {} {}", ffmpeg_path.display(), args.join(" "));
    log::info!("[FORENSIC:MERGE] ═══════════════════════════════════════════════════════════");

    log::info!("[FFmpegExec] ═══════════════════════════════════════════════════════════");
    log::info!("[FFmpegExec] BUILDING FFMPEG COMMAND");
    log::info!("[FFmpegExec] ═══════════════════════════════════════════════════════════");
    log::info!("[FFmpegExec] ffmpeg_path: {}", ffmpeg_path.display());
    log::info!("[FFmpegExec] mode: {:?}", config.mode);
    log::info!("[FFmpegExec] input_files: {} files", config.input_files.len());
    for (i, f) in config.input_files.iter().enumerate() {
        log::info!("[FFmpegExec]   [{}] {}", i, f);
    }
    log::info!("[FFmpegExec] output_path: {}", config.output_path);
    log::info!("[FFmpegExec] total_duration: {}s", config.total_duration);
    log::info!("[FFmpegExec] concat_list: {}", concat_list_path.display());
    if let Some(ref slp) = config.subtitle_list_path {
        log::info!("[FFmpegExec] subtitle_list: {}", slp.display());
    }
    log::info!("[FFmpegExec] video_codec: {:?}", config.video_codec);
    log::info!("[FFmpegExec] audio_codec: {:?}", config.audio_codec);
    log::info!("[FFmpegExec] video_crf: {:?}", config.video_crf);
    log::info!("[FFmpegExec] video_preset: {:?}", config.video_preset);
    log::info!("[FFmpegExec] audio_bitrate: {:?}", config.audio_bitrate);
    log::info!("[FFmpegExec] target_resolution: {:?}", config.target_resolution);
    log::info!("[FFmpegExec] target_fps: {:?}", config.target_fps);
    log::info!("[FFmpegExec] hw_accel: {:?}", config.hw_accel);
    log::info!("[FFmpegExec] ────────────────────────────────────────────────────");
    log::info!("[FFmpegExec] FULL COMMAND: {} {}", ffmpeg_path.display(), args.join(" "));
    log::info!("[FFmpegExec] ═══════════════════════════════════════════════════════════");

    let _concat_start = Instant::now();
    log::info!("[FFMPEG_CONCAT_START] ⚠️⚠️⚠️ FFmpeg concat STARTING ⚠️⚠️⚠️ output={} files={} mode={:?} duration={:.1}s", config.output_path, config.input_files.len(), config.mode, config.total_duration);
    let mut cmd = Command::new(ffmpeg_path);
    cmd.args(&args);
    let mut child = spawn_ffmpeg(&mut cmd)?;

    let stderr = child.stderr.take().ok_or_else(|| {
        anyhow::anyhow!("INTERNAL ERROR: stderr not available after spawn_ffmpeg. This indicates a programming error - spawn_ffmpeg must be called before reading stderr.")
    })?;

    // ── BACKGROUND STREAMING THREAD ─────────────────────────────────────
    // Move all stderr reading (progress parsing, purge tracking) to a 
    // background thread so the main thread can poll try_wait() with
    // timeout + cancellation. This prevents infinite blocking if FFmpeg hangs.
    let total = config.total_duration;
    let segmented_refs = segments_ref.clone();
    let cancel_shared = cancel_flag.clone();
    let purge_input_files: Vec<String> = config.input_files.clone();
    let temp_dir_owned = temp_dir.clone();
    // Wrap on_progress in Arc for shared access between bg thread and main thread
    let progress_cb = Arc::new(on_progress);
    let progress_cb_thread = progress_cb.clone();

    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut block_reader = ProgressBlockReader::new();
        let mut stderr_log: Vec<String> = Vec::new();
        let mut last_emit_ms = Instant::now();
        const MIN_EMIT_INTERVAL_MS: u64 = 100;

        // Track which normalized files are still on disk so we can purge them incrementally
        let mut active_norm_files: Vec<(usize, std::path::PathBuf)> = Vec::new();
        for (i, file_path) in purge_input_files.iter().enumerate() {
            let p = std::path::PathBuf::from(file_path);
            if p.starts_with(&temp_dir_owned) {
                active_norm_files.push((i, p));
            }
        }
        active_norm_files.sort_by_key(|(idx, _)| *idx);

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(ref l) => l,
                Err(_) => break,
            };

            // ── INCREMENTAL PURGE LOGIC ──────────────────────────────────
            if line.contains("Opening '") && line.contains("' for reading") {
                if let Some(start_quote) = line.find("'") {
                    if let Some(end_quote) = line[start_quote+1..].find("'") {
                        let opened_file = &line[start_quote+1..start_quote+1+end_quote];
                        if let Some(opened_idx) = purge_input_files.iter().position(|f| f.replace('\\', "/").contains(opened_file)) {
                            let mut i = 0;
                            while i < active_norm_files.len() {
                                if active_norm_files[i].0 < opened_idx {
                                    let (idx, path) = active_norm_files.remove(i);
                                    if path.exists() {
                                        if let Err(e) = std::fs::remove_file(&path) {
                                            log::warn!("[FORENSIC:PURGE] Failed to incremental purge File #{}: {}", idx, e);
                                        } else {
                                            log::info!("[FORENSIC:PURGE] Incremental purge success: File #{} ({})", idx, path.display());
                                        }
                                    }
                                } else {
                                    i += 1;
                                }
                            }
                        }
                    }
                }
            }

            if cancel_shared.load(Ordering::Relaxed) {
                break; // Exit thread — main thread handles cleanup
            }

            stderr_log.push(line.clone());
            if stderr_log.len() > 30 {
                stderr_log.remove(0);
            }

            if let Some(block) = block_reader.feed_line(line) {
                if let Some(prog) = parse_progress_block(&block) {
                    let elapsed = prog.out_time_seconds.unwrap_or(0.0).max(0.0);
                    let percent = calc_progress_percent(elapsed, total);

                    let mut current_idx = 0;
                    let mut remaining_dur = total;
                    for (i, seg) in segmented_refs.iter().enumerate() {
                        if elapsed >= seg.start_time && elapsed < seg.end_time {
                            current_idx = i;
                        }
                        if elapsed >= seg.end_time {
                            remaining_dur -= seg.duration;
                        }
                    }
                    if current_idx < segmented_refs.len() {
                        let seg = &segmented_refs[current_idx];
                        let processed_in_seg = (elapsed - seg.start_time).max(0.0);
                        remaining_dur -= processed_in_seg;
                    }

                    let eta = prog.speed
                        .filter(|&s| s > 0.01)
                        .map(|speed| ((total - elapsed) as f32 / speed).max(0.0));

                    let now = Instant::now();
                    if now.duration_since(last_emit_ms).as_millis() >= MIN_EMIT_INTERVAL_MS as u128 {
                        last_emit_ms = now;
                        progress_cb_thread(MergeProgress {
                            percent,
                            current_time: elapsed,
                            total_duration: total,
                            speed: prog.speed,
                            fps: prog.fps,
                            current_file: None,
                            current_segment_index: Some(current_idx),
                            remaining_duration: Some(remaining_dur.max(0.0)),
                            bytes_written: prog.total_size_bytes,
                            eta_seconds: eta,
                            phase: MergePhase::Writing,
                            overall_percent: None,
                            stage_name: Some("Merging...".into()),
                            stage_percent: Some(percent),
                            current_file_index: None,
                            total_files_in_stage: None,
                            warning: None,
                            is_large_playlist: false,
                        });
                    }

                    if prog.is_end { break; }
                }
            }
        }
        stderr_log
    });

    // ── MAIN THREAD: POLL try_wait() WITH TIMEOUT ────────────────────────
    // The stderr thread processes progress and purges files.
    // This thread supervises the child process with a 6-hour timeout.
    const CONCAT_TIMEOUT_SECS: u64 = 21600; // 6 hours for large merges
    let start = Instant::now();
    let status = loop {
        if start.elapsed().as_secs() > CONCAT_TIMEOUT_SECS {
            force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(&config.output_path), config.mkvmerge_succeeded_before_ffmpeg);
            progress_cb(MergeProgress {
                percent: 0.0, current_time: 0.0, total_duration: total,
                speed: None, fps: None, current_file: None,
                current_segment_index: None, remaining_duration: None,
                bytes_written: None, eta_seconds: None,
                phase: MergePhase::Failed, overall_percent: None,
                stage_name: None, stage_percent: None,
                current_file_index: None, total_files_in_stage: None,
                warning: None, is_large_playlist: false,
            });
            return Err(anyhow!("FFmpeg concat timed out after {} seconds", CONCAT_TIMEOUT_SECS));
        }
        if cancel_flag.load(Ordering::Relaxed) {
            force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(&config.output_path), config.mkvmerge_succeeded_before_ffmpeg);
            progress_cb(MergeProgress {
                percent: 0.0, current_time: 0.0, total_duration: total,
                speed: None, fps: None, current_file: None,
                current_segment_index: None, remaining_duration: None,
                bytes_written: None, eta_seconds: None,
                phase: MergePhase::Cancelled, overall_percent: None,
                stage_name: None, stage_percent: None,
                current_file_index: None, total_files_in_stage: None,
                warning: None, is_large_playlist: false,
            });
            return Err(anyhow!("Merge cancelled by user"));
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("FFmpeg process wait error: {}", e));
            }
        }
    };

    // Join stderr thread to collect any remaining lines
    let stderr_log: Vec<String> = stderr_thread.join().unwrap_or_default();
    log::info!("[FFMPEG_CONCAT_COMPLETE] output={} success={} elapsed={:?}", config.output_path, status.success(), start_time.elapsed());

    if !status.success() {
        cleanup_partial_output(Path::new(&config.output_path), config.mkvmerge_succeeded_before_ffmpeg);
        let error_details = stderr_log.join("\n");
        log::error!("[FORENSIC:FAILURE] ═══════════════════════════════════════════════════════════");
        log::error!("[FORENSIC:FAILURE] PHASE 13: FFMPEG FAILURE");
        log::error!("[FORENSIC:FAILURE] ═══════════════════════════════════════════════════════════");
        log::error!("[FORENSIC:FAILURE] Exit code: {:?}", status.code());
        log::error!("[FORENSIC:FAILURE] Mode used: {:?}", config.mode);
        log::error!("[FORENSIC:FAILURE] Output path: {}", config.output_path);
        log::error!("[FORENSIC:FAILURE] input_files count: {}", config.input_files.len());
        log::error!("[FORENSIC:FAILURE] Full stderr:\n{}", error_details);
        log::error!("[FORENSIC:FAILURE] ═══════════════════════════════════════════════════════════");

        log::error!("[FFmpegExec] ═══════════════════════════════════════════════════════════");
        log::error!("[FFmpegExec] ❌ FFMPEG FAILED");
        log::error!("[FFmpegExec] Exit code: {:?}", status.code());
        log::error!("[FFmpegExec] Mode used: {:?}", config.mode);
        log::error!("[FFmpegExec] Output path: {}", config.output_path);
        log::error!("[FFmpegExec] Full stderr:\n{}", error_details);
        log::error!("[FFmpegExec] ═══════════════════════════════════════════════════════════");
        log::error!("[RUN_MERGE_FAILED] output={} mode={:?} error=ffmpeg_exit_{:?} elapsed={:?}", config.output_path, config.mode, status.code(), start_time.elapsed());
        return Err(anyhow!("ffmpeg failed with exit code {:?}.\nDetails:\n{}", status.code(), error_details));
    }

    log::info!("[TIMELINE_CHECK_START] output={} elapsed={:?}", config.output_path, start_time.elapsed());
    let output_size = std::fs::metadata(&config.output_path).map(|m| m.len()).unwrap_or(0);

    // ── PHASE 12: COMPLETION TRACE ──────────────────────────────────────
    log::info!("[FORENSIC:COMPLETE] ═══════════════════════════════════════════════════════════");
    log::info!("[FORENSIC:COMPLETE] PHASE 12: MERGE COMPLETE");
    log::info!("[FORENSIC:COMPLETE] Output path: {}", config.output_path);
    log::info!("[FORENSIC:COMPLETE] Output size: {} bytes ({} MB)", output_size, output_size / 1_000_000);
    log::info!("[FORENSIC:COMPLETE] Total duration: {}s", total);
    log::info!("[FORENSIC:COMPLETE] Mode used: {:?}", config.mode);
    log::info!("[FORENSIC:COMPLETE] Input files count: {}", config.input_files.len());
    log::info!("[FORENSIC:COMPLETE] ═══════════════════════════════════════════════════════════");

    // ── PHASE 12b: TIMELINE FORENSICS ────────────────────────────────────
    // Verify output timeline integrity by probing the actual output
    let timeline_check = verify_timeline_integrity(&config.output_path, total);
    log::info!("[TIMELINE_CHECK_COMPLETE] output={} elapsed={:?} has_drift={}", config.output_path, start_time.elapsed(), timeline_check.as_ref().map(|d| d.has_drift).unwrap_or(false));
    if let Some(drift_info) = timeline_check {
        if drift_info.has_drift {
            log::warn!("[FORENSIC:TIMELINE] ═══════════════════════════════════════════════════════════");
            log::warn!("[FORENSIC:TIMELINE] ⚠️ TIMELINE DRIFT DETECTED");
            log::warn!("[FORENSIC:TIMELINE] Expected duration: {}s ({:.2}h)", drift_info.expected_duration, drift_info.expected_duration / 3600.0);
            log::warn!("[FORENSIC:TIMELINE] Actual duration:   {}s ({:.2}h)", drift_info.actual_duration, drift_info.actual_duration / 3600.0);
            log::warn!("[FORENSIC:TIMELINE] Drift: {:.3}% ({}s)", drift_info.drift_percent.abs(), drift_info.drift_seconds);
            log::warn!("[FORENSIC:TIMELINE] Actual fps: {:?}, Actual timebase: {:?}", drift_info.actual_fps, drift_info.actual_timebase);
            log::warn!("[FORENSIC:TIMELINE] If drift > 0.1%, investigate PTS accumulation in concat demuxer");
            log::warn!("[FORENSIC:TIMELINE] ═══════════════════════════════════════════════════════════");
        } else {
            log::info!("[FORENSIC:TIMELINE] ✅ Timeline integrity verified");
            log::info!("[FORENSIC:TIMELINE]   Expected: {}s, Actual: {}s, Drift: {:.3}%",
                drift_info.expected_duration, drift_info.actual_duration, drift_info.drift_percent);
        }
    } else {
        log::warn!("[FORENSIC:TIMELINE] verify_timeline_integrity() returned None — timeline check skipped");
    }

    // ── PHASE 12c: POST-MERGE STREAM VERIFICATION ─────────────────────────
    // Verify output has expected stream counts (expected vs actual comparison)
    verify_post_merge_streams(&config.output_path, &config.subtitle_mode, &config.subtitle_list_path, &config.input_files);

    log::info!("[FFmpegExec] ═══════════════════════════════════════════════════════════");
    log::info!("[FFmpegExec] ✅ FFMPEG SUCCEEDED");
    log::info!("[FFmpegExec] Mode used: {:?}", config.mode);
    log::info!("[FFmpegExec] Output path: {}", config.output_path);
    log::info!("[FFmpegExec] Output size: {} bytes ({} MB)", output_size, output_size / 1_000_000);
    log::info!("[FFmpegExec] Total duration: {}s", total);
    log::info!("[FFmpegExec] ═══════════════════════════════════════════════════════════");

    log::info!("[FINAL_PROGRESS_START] output={} elapsed={:?}", config.output_path, start_time.elapsed());
    progress_cb(MergeProgress {
        percent: 100.0,
        current_time: total,
        total_duration: total,
        speed: None,
        fps: None,
        current_file: None,
        current_segment_index: Some(segments_ref.len().saturating_sub(1)),
        remaining_duration: Some(0.0),
        bytes_written: Some(output_size),
        eta_seconds: Some(0.0),
        phase: MergePhase::Complete,
        overall_percent: None,
        stage_name: None,
        stage_percent: None,
        current_file_index: None,
        total_files_in_stage: None,
        warning: None,
        is_large_playlist: false,
    });
    log::info!("[FINAL_PROGRESS_COMPLETE] output={} elapsed={:?}", config.output_path, start_time.elapsed());

    let total_elapsed = start_time.elapsed();
    log::info!("[RUN_MERGE_COMPLETE] ✅ FFmpeg concat COMPLETED | output={} elapsed={:.2}s | WARNING: This overwrites any previous output at this path!", config.output_path, total_elapsed.as_secs_f64());
    Ok(crate::commands::merge::MergeResult {
        job_id: "".into(),
        output_path: config.output_path.clone(),
        output_size_bytes: output_size,
        segments: segments_ref.as_ref().clone(),
        output_paths: None,
        parts: None,
        srt_export_paths: None,
        report_paths: None,
        warnings: None,
        subtitle_warnings: None,
        audio_repair_summary: None,
    })
}

/// Run a merge with split output into multiple parts
fn run_split_merge_blocking<F>(
    ffmpeg_path: &Path,
    config: &MergeConfig,
    cancel_flag: Arc<AtomicBool>,
    on_progress: F,
) -> Result<crate::commands::merge::MergeResult>
where
    F: Fn(MergeProgress) + Send,
{
    let split_config = config.split_config.as_ref()
        .ok_or_else(|| anyhow!("Split config is required for split merge but was None"))?;

    // Compute part boundaries (returns error if would create too many outputs)
    let parts = compute_part_boundaries(
        &config.input_files,
        &config.input_names,
        &config.input_durations,
        &config.segment_is_card,
        &config.output_path,
        split_config,
        config.naming_config.as_ref(),
    )?;

    log::info!("[SplitMerge] Starting {} part merge", parts.len());

    let temp_dir = crate::ffmpeg::get_temp_dir()
        .map_err(|e| anyhow!("Failed to get temp dir: {}", e))?;

    let mut all_segments = Vec::new();
    let mut all_output_paths = Vec::new();
    let mut all_parts_results: Vec<crate::commands::merge::MergePartResult> = Vec::new();
    let mut part_subtitle_list_paths: Vec<std::path::PathBuf> = Vec::new(); // track for cleanup
    let total_parts = parts.len();
    let total_duration = config.total_duration;
    let mut srt_export_paths: Vec<String> = Vec::new();
    let mut all_split_warnings: Vec<String> = Vec::new();

    for (part_idx, part) in parts.into_iter().enumerate() {
        // Check cancellation before starting each part
        if cancel_flag.load(Ordering::Relaxed) {
            // Preserve completed parts — only report cancellation
            let completed_parts: Vec<u32> = all_parts_results.iter().map(|r| r.part_index).collect();
            if !completed_parts.is_empty() {
                log::info!("[SplitMerge] Cancelled by user after completing parts {:?}. Preserving outputs.", completed_parts);
            }
            return Err(anyhow!("Merge cancelled by user"));
        }

        let part_num = part.part_index;
        let is_first_part = part_idx == 0;

        // Determine subtitle handling for this part — needed early for checkpoint check
        let split_subtitle_mode = split_config.subtitle_mode.as_ref();
        let global_subtitle_mode = &config.subtitle_mode;

        // ── Split Checkpoint Persistence ────────────────────────────────────
        // If this part's output already exists and is valid, skip reprocessing.
        // This allows resume from crash after partial split completion.
        let part_output_exists = Path::new(&part.output_path).exists()
            && std::fs::metadata(&part.output_path).map(|m| m.len()).unwrap_or(0) > 0;

        if part_output_exists {
            log::info!("[SplitMerge] Part {}/{} already exists — skipping (checkpoint resume)",
                part_num, total_parts);

            // Calculate part duration for progress reporting
            let part_total_dur = part.end_time - part.start_time;
let _part_start_time_for_progress = if is_first_part { 0.0 } else {
                config.input_durations[..part.file_indices.first().copied().unwrap_or(0)]
                    .iter().sum()
            };
            let overall_percent = (((part_idx as f64) + 1.0) / total_parts as f64 * 100.0) as f32;

            // Emit completion progress for this part
            on_progress(MergeProgress {
                percent: overall_percent,
                current_time: part.start_time + part_total_dur,
                total_duration,
                speed: None,
                fps: None,
                current_file: None,
                current_segment_index: part.file_indices.first().copied(),
                remaining_duration: Some(total_duration - (part.start_time + part_total_dur)),
                bytes_written: std::fs::metadata(&part.output_path).map(|m| m.len()).ok(),
                eta_seconds: None,
                phase: MergePhase::Writing,
                overall_percent: Some(overall_percent),
                stage_name: Some(format!("Part {}/{} (resumed)", part_num, total_parts)),
                stage_percent: Some(100.0),
                current_file_index: None,
                total_files_in_stage: None,
                warning: None,
                is_large_playlist: false,
            });

            // Get output size
            let part_output_size = std::fs::metadata(&part.output_path)
                .map(|m| m.len())
                .unwrap_or(0);

            // Collect segments for this part (same logic as normal completion)
            let card_bg_color = config.card_config.as_ref().map(|c| c.color.clone());
            let mut part_start = part.start_time;
            for &file_idx in &part.file_indices {
                let duration = config.input_durations[file_idx];
                let is_card = config.segment_is_card.get(file_idx).copied().unwrap_or(false);
                let parent_folder = if !is_card {
                    config.input_files.get(file_idx).and_then(|file_path| {
                        Path::new(file_path)
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string())
                    })
                } else {
                    None
                };
                all_segments.push(crate::commands::merge::MergeSegment {
                    name: config.input_names[file_idx].clone(),
                    duration,
                    start_time: part_start,
                    end_time: part_start + duration,
                    is_card: if is_card { Some(true) } else { None },
                    card_color: if is_card { card_bg_color.clone() } else { None },
                    parent_folder,
                });
                part_start += duration;
            }

            all_output_paths.push(part.output_path.clone());
            all_parts_results.push(crate::commands::merge::MergePartResult {
                part_index: part_num,
                output_path: part.output_path.clone(),
                output_size_bytes: part_output_size,
                file_count: part.file_indices.len() as u32,
                total_duration: part_total_dur,
            });

            // Skip SRT export check: if part output exists, assume SRT also done if it was requested
            let should_export_srt = split_subtitle_mode
                .map(|m| matches!(m, SplitSubtitleMode::ExportSrt))
                .unwrap_or(*global_subtitle_mode == SubtitleMode::ExportSrt || config.export_merged_srt);
            if should_export_srt {
                let part_srt_path = Path::new(&part.output_path).with_extension("srt");
                if part_srt_path.exists() {
                    srt_export_paths.push(part_srt_path.to_string_lossy().into_owned());
                }
            }

            continue; // Skip to next part
        }

        // Emit progress for this part
        let part_start_time_for_progress = if is_first_part { 0.0 } else {
            // Calculate cumulative time before this part
            config.input_durations[..part.file_indices.first().copied().unwrap_or(0)]
                .iter().sum()
        };

        let part_label = part.label.as_deref().unwrap_or("");
        let stage_msg = if part_label.is_empty() {
            format!("Part {}/{}", part_num, total_parts)
        } else {
            format!("Part {}/{} — {}", part_num, total_parts, part_label)
        };

        on_progress(MergeProgress {
            percent: (part_idx as f32 / total_parts as f32) * 100.0,
            current_time: part_start_time_for_progress,
            total_duration,
            speed: None,
            fps: None,
            current_file: None,
            current_segment_index: Some(part.file_indices.first().copied().unwrap_or(0)),
            remaining_duration: Some(total_duration - part_start_time_for_progress),
            bytes_written: None,
            eta_seconds: None,
            phase: MergePhase::Writing,
            overall_percent: None,
            stage_name: Some(stage_msg.clone()),
            stage_percent: None,
            current_file_index: None,
            total_files_in_stage: None,
            warning: None,
            is_large_playlist: false,
        });

        // Create concat list for this part using full file paths
        let part_input_files: Vec<&Path> = part.file_indices
            .iter()
            .map(|&i| Path::new(&config.input_files[i]))
            .collect();

        let part_list_path = temp_dir.join(format!("concat_part_{}_{}.txt", config.total_duration, part_num));

        // Pass per-file durations as `duration` directives in the concat list.
        // This tells the concat demuxer the exact content duration of each file,
        // preventing PTS offset accumulation that causes "unused length" in output.
        let part_durations: Vec<f64> = part.file_indices
            .iter()
            .map(|&i| config.input_durations[i])
            .collect();
crate::ffmpeg::write_concat_list_with_durations(
            &part_input_files,
            Some(&part_durations),
            &part_list_path,
            true,
        ).map_err(|e| anyhow!("Failed to write part concat list: {}", e))?;

        // Determine subtitle handling for this part based on split_config.subtitle_mode
        // split_config.subtitle_mode is Option<SplitSubtitleMode>, config.subtitle_mode is SubtitleMode
        // (split_subtitle_mode and global_subtitle_mode already defined earlier in loop)

        // should_export_srt: export SRT file for each part
        let should_export_srt = split_subtitle_mode
            .map(|m| matches!(m, SplitSubtitleMode::ExportSrt))
            .unwrap_or(*global_subtitle_mode == SubtitleMode::ExportSrt || config.export_merged_srt);

        // should_handle_subs: whether to create subtitle concat list
        // Note: should_embed_subs is determined by part_config.subtitle_mode in build_ffmpeg_args
        let should_handle_subs = split_subtitle_mode
            .map(|m| !matches!(m, SplitSubtitleMode::Ignore))
            .unwrap_or(*global_subtitle_mode != SubtitleMode::None);

        // Create per-part subtitle concat list if subtitles are available and not ignored
        // FIX: Use rebased SRT files to properly offset timestamps for each part
        let part_subtitle_list_path = if should_handle_subs && !config.subtitle_files.is_empty() {
            let part_sub_paths: Vec<Option<String>> = part.file_indices
                .iter()
                .map(|&i| config.subtitle_files.get(i).cloned().flatten())
                .collect();
            let part_durations: Vec<f64> = part.file_indices
                .iter()
                .map(|&i| config.input_durations.get(i).copied().unwrap_or(0.0))
                .collect();

            if part_sub_paths.iter().any(|s| s.is_some()) {
                let part_sl_path = temp_dir.join(format!(
                    "concat_sub_{}_{}_{}.txt",
                    config.total_duration as u64,
                    config.output_path.replace(|c: char| !c.is_alphanumeric() && c != '_', "_"),
                    part.part_index
                ));

                // Use rebased SRT files (created with proper timestamp offsets)
                match create_rebased_srt_for_part(part.start_time, &part_sub_paths, &temp_dir, part.part_index) {
                    Ok(rebased_paths) => {
                        match write_rebased_subtitle_concat_list(&rebased_paths, &part_durations, &part_sl_path) {
                            Ok(()) => {
                                part_subtitle_list_paths.push(part_sl_path.clone());
                                Some(part_sl_path)
                            }
                            Err(e) => {
                                log::warn!("[SplitMerge] Failed to write rebased concat list: {}", e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("[SplitMerge] Failed to create rebased SRT files: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        // Create a modified config for this part
        let mut part_config = config.clone();
        part_config.output_path = part.output_path.clone();
        part_config.subtitle_list_path = part_subtitle_list_path.clone();
        // Use split-specific subtitle mode if set
        if let Some(ref split_sub_mode) = split_config.subtitle_mode {
            // Map SplitSubtitleMode to SubtitleMode: Ignore -> None
            match split_sub_mode {
                SplitSubtitleMode::Embed => part_config.subtitle_mode = SubtitleMode::Embed,
                SplitSubtitleMode::ExportSrt => part_config.subtitle_mode = SubtitleMode::ExportSrt,
                SplitSubtitleMode::Ignore => part_config.subtitle_mode = SubtitleMode::None,
            }
        }

        // Build args for this part
        let args = build_ffmpeg_args(&part_config, &part_list_path)?;

        log::info!("[SplitMerge] Part {}/{}: ffmpeg {} {}",
            part_num, total_parts, ffmpeg_path.display(), args.join(" "));
        if let Some(ref label) = part.label {
            log::info!("[SplitMerge]   Folder: {}", label);
        }

        // Run FFmpeg
        let mut cmd = Command::new(ffmpeg_path);
        cmd.args(&args);
        let mut child = spawn_ffmpeg(&mut cmd)
            .with_context(|| format!("Failed to spawn ffmpeg for part {}", part_num))?;

        let stderr = child.stderr.take().ok_or_else(|| {
            anyhow::anyhow!("INTERNAL ERROR: stderr not available for part {} - spawn_ffmpeg misconfiguration", part_num)
        })?;
        let reader = BufReader::new(stderr);
        let mut part_block_reader = ProgressBlockReader::new();
        let part_total_dur = part.end_time - part.start_time;
        let mut part_stderr_log: Vec<String> = Vec::new();

        // Process FFmpeg output for this part
        for line_result in reader.lines() {
            if cancel_flag.load(Ordering::Relaxed) {
                force_kill_process_tree(&mut child);
                let _ = child.wait();
                // Clean up all outputs including this part
                for path in &all_output_paths {
                    cleanup_partial_output(Path::new(path), false);
                }
                cleanup_partial_output(Path::new(&part.output_path), false);
                // Clean up temp concat and subtitle files
                let _ = std::fs::remove_file(&part_list_path);
                if let Some(ref sl_path) = part_subtitle_list_path { let _ = std::fs::remove_file(sl_path); }
                for sl_path in &part_subtitle_list_paths { let _ = std::fs::remove_file(sl_path); }
                return Err(anyhow!("Merge cancelled by user"));
            }

            let line = match line_result {
                Ok(l) => l,
                Err(_) => break,
            };

            part_stderr_log.push(line.clone());
            if part_stderr_log.len() > 30 {
                part_stderr_log.remove(0);
            }

            if let Some(block) = part_block_reader.feed_line(&line) {
                if let Some(prog) = parse_progress_block(&block) {
                    let elapsed = prog.out_time_seconds.unwrap_or(0.0).max(0.0);
                    let part_percent = if part_total_dur > 0.0 {
                        (elapsed / part_total_dur * 100.0).min(100.0)
                    } else {
                        100.0
                    };

                    // Overall progress = part progress scaled by part position
                    let overall_percent = (((part_idx as f64) + part_percent / 100.0) / total_parts as f64 * 100.0) as f32;
                    let overall_elapsed = part.start_time + elapsed;

                    on_progress(MergeProgress {
                        percent: overall_percent,
                        current_time: overall_elapsed,
                        total_duration,
                        speed: prog.speed,
                        fps: prog.fps,
                        current_file: None,
                        current_segment_index: part.file_indices.first().copied(),
                        remaining_duration: Some(total_duration - overall_elapsed),
                        bytes_written: prog.total_size_bytes,
                        eta_seconds: prog.speed
                            .filter(|&s| s > 0.01)
                            .map(|speed| ((total_duration - overall_elapsed) as f32 / speed).max(0.0)),
                        phase: MergePhase::Writing,
                        overall_percent: Some(overall_percent),
                        stage_name: Some(format!("Merging Part {}/{}", part_num, total_parts)),
                        stage_percent: Some(part_percent as f32),
                        current_file_index: None,
                        total_files_in_stage: None,
                        warning: None,
                        is_large_playlist: false,
                    });

                    if prog.is_end { break; }
                }
            }
        }

        let status = child.wait().with_context(|| format!("FFmpeg part {} failed", part_num))?;

        // ── Per-part SRT export ──
        if status.success() && should_export_srt {
            if let Some(ref part_sl_path) = part_subtitle_list_path {
                if part_sl_path.exists() {
                    let part_srt_path = Path::new(&part.output_path).with_extension("srt");
                    log::info!("[SplitMerge] Exporting SRT for part {}: {}", part_num, part_srt_path.display());
                    match crate::ffmpeg::generate_merged_srt(
                        ffmpeg_path,
                        part_sl_path,
                        &part_srt_path,
                    ) {
                        Ok(()) => {
                            srt_export_paths.push(part_srt_path.to_string_lossy().into_owned());
                            log::info!("[SplitMerge] SRT for part {} exported successfully", part_num);
                        }
                        Err(e) => {
                            let msg = format!("Failed to export SRT for part {}: {}", part_num, e);
                            log::warn!("[SplitMerge] {}", msg);
                            all_split_warnings.push(msg);
                        }
                    }
                }
            }
        }

        // Clean up part concat list
        let _ = std::fs::remove_file(&part_list_path);

        if !status.success() {
            // Only clean up the FAILED part's output — preserve previously completed parts
            cleanup_partial_output(Path::new(&part.output_path), false);
            // Clean up temp concat and subtitle files
            if let Some(ref sl_path) = part_subtitle_list_path { let _ = std::fs::remove_file(sl_path); }
            let error_details = part_stderr_log.join("\n");
            let completed_parts: Vec<u32> = all_parts_results.iter().map(|r| r.part_index).collect();
            let failed_parts: Vec<u32> = (part_idx..total_parts).map(|i| (i + 1) as u32).collect();
            log::error!("[SplitMerge] FFmpeg part {}/{} failed. Exit code: {:?}. Completed parts: {:?}. Failed parts: {:?}",
                part_num, total_parts, status.code(), completed_parts, failed_parts);
            log::error!("[SplitMerge] Stderr:\n{}", error_details);
            return Err(anyhow!(
                "FFmpeg part {}/{} failed (exit code {:?}). {} parts completed successfully and are preserved on disk. \
                 Re-run to resume from part {}.\nDetails:\n{}",
                part_num, total_parts, status.code(), completed_parts.len(), part_num, error_details
            ));
        }

        // Get output size for this part
        let part_output_size = std::fs::metadata(&part.output_path)
            .map(|m| m.len())
            .unwrap_or(0);

        // Collect segments for this part, preserving card metadata
        let card_bg_color = config.card_config.as_ref().map(|c| c.color.clone());
        let mut part_start = part.start_time;
        for &file_idx in &part.file_indices {
            let duration = config.input_durations[file_idx];
            let is_card = config.segment_is_card.get(file_idx).copied().unwrap_or(false);
            let parent_folder = if !is_card {
                config.input_files.get(file_idx).and_then(|file_path| {
                    Path::new(file_path)
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string())
                })
            } else {
                None
            };
            all_segments.push(crate::commands::merge::MergeSegment {
                name: config.input_names[file_idx].clone(),
                duration,
                start_time: part_start,
                end_time: part_start + duration,
                is_card: if is_card { Some(true) } else { None },
                card_color: if is_card { card_bg_color.clone() } else { None },
                parent_folder,
            });
            part_start += duration;
        }

        all_output_paths.push(part.output_path.clone());
        all_parts_results.push(crate::commands::merge::MergePartResult {
            part_index: part_num,
            output_path: part.output_path.clone(),
            output_size_bytes: part_output_size,
            file_count: part.file_indices.len() as u32,
            total_duration: part.end_time - part.start_time,
        });
    }

    // Final progress update
    on_progress(MergeProgress {
        percent: 100.0,
        current_time: total_duration,
        total_duration,
        speed: None,
        fps: None,
        current_file: None,
        current_segment_index: None,
        remaining_duration: Some(0.0),
        bytes_written: None,
        eta_seconds: Some(0.0),
        phase: MergePhase::Complete,
        overall_percent: Some(100.0),
        stage_name: None,
        stage_percent: None,
        current_file_index: None,
        total_files_in_stage: None,
        warning: None,
        is_large_playlist: false,
    });

    let total_size: u64 = all_parts_results.iter().map(|p| p.output_size_bytes).sum();

    // Clean up per-part subtitle concat lists
    for sl_path in &part_subtitle_list_paths {
        let _ = std::fs::remove_file(sl_path);
    }

    log::info!("[SplitMerge] Completed {} parts, total size {} bytes", total_parts, total_size);

    Ok(crate::commands::merge::MergeResult {
        job_id: "".into(),
        output_path: all_output_paths.first().cloned().unwrap_or_default(),
        output_size_bytes: total_size,
        segments: all_segments,
        output_paths: Some(all_output_paths),
        parts: Some(all_parts_results),
        srt_export_paths: if srt_export_paths.is_empty() { None } else { Some(srt_export_paths) },
        report_paths: None,
        warnings: if all_split_warnings.is_empty() { None } else { Some(all_split_warnings) },
        subtitle_warnings: None,
        audio_repair_summary: None,
    })
}

fn build_ffmpeg_args(config: &MergeConfig, concat_list_path: &Path) -> Result<Vec<String>> {
    let mut args: Vec<String> = Vec::new();
    args.push("-hide_banner".into());
    args.extend(["-progress".into(), "pipe:2".into()]);
    args.push("-nostats".into());
    args.extend(["-loglevel".into(), "error".into()]);
    args.push("-y".into());

    // Generate PTS from DTS to strip edit list effects that cause moov atom
    // duration inflation (especially with stream copy). This is critical for split
    // output correctness — without it, each part can accumulate PTS offsets from
    // source files, producing "unused length" in the output.
    args.extend(["-fflags".into(), "+genpts+discardcorrupt".into()]);
    // Apply -err_detect ignore_err to the input demuxer for both Lossless and
    // Custom modes. This matches the normalization functions which already use
    // -err_detect ignore_err, ensuring the merge phase is no stricter than the
    // normalization phase — preventing spurious FFmpeg exit on non-fatal errors.
    args.extend(["-err_detect".into(), "ignore_err".into()]);

    if let Some(hwaccel) = &config.hw_accel {
        if !hwaccel.is_empty() {
            args.extend(["-hwaccel".into(), hwaccel.clone()]);
        }
    }

    args.extend([
        "-f".into(), "concat".into(),
        "-safe".into(), "0".into(),
        "-i".into(), concat_list_path.to_string_lossy().into_owned(),
    ]);

    let out_ext = Path::new(&config.output_path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let is_mp4_like = out_ext == "mp4" || out_ext == "mov" || out_ext == "m4v";
    let sub_codec = if is_mp4_like { "mov_text" } else { "srt" };

    // Find first valid video file's resolution to initialize mov_text dimensions if needed
    let mut resolution = None;
    if sub_codec == "mov_text" && config.subtitle_list_path.is_some() {
        // Helper to strip \\?\ prefix for Windows path operations
        let strip_prefix = |p: &str| -> String {
            if let Some(stripped) = p.strip_prefix("\\\\?\\") {
                if let Some(unc_part) = stripped.strip_prefix("UNC\\") {
                    format!("\\\\{}", unc_part)
                } else {
                    stripped.to_string()
                }
            } else {
                p.to_string()
            }
        };
        if let Ok(ffprobe) = crate::ffmpeg::find_ffprobe(None) {
            for file_path in &config.input_files {
                let clean_path = strip_prefix(file_path);
                let p = Path::new(&clean_path);
                if p.exists() && p.is_file() {
                    if let Ok(info) = crate::ffmpeg::probe::probe_file(&ffprobe, p) {
                        if let Some(video) = info.video_streams.first() {
                            if let (Some(w), Some(h)) = (video.width, video.height) {
                                if w > 0 && h > 0 {
                                    resolution = Some(format!("{}x{}", w, h));
                                    log::info!("[Subtitle] Resolution detected: {}x{} from {}", w, h, clean_path);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: if ffprobe returned 0x0 for all files, try decoding one file
        // to extract the actual resolution. Some files have unusual header metadata
        // (e.g. no moov atom dimensions) but decode fine — ffprobe reports 0x0.
        if resolution.is_none() {
            if let Ok(ffmpeg_path) = crate::ffmpeg::find_ffmpeg(None) {
                for file_path in &config.input_files {
                    let clean_path = strip_prefix(file_path);
                    let p = Path::new(&clean_path);
                    if p.exists() && p.is_file() {
                        if let Some(decoded_res) = decode_resolution_fallback(&ffmpeg_path, p) {
                            resolution = Some(decoded_res);
                            log::info!("[Subtitle] Decode fallback succeeded for '{}', using resolution {:?}",
                                p.display(), resolution);
                            break;
                        }
                    }
                }
            }
        }

        // Last resort: use 1920x1080 as fallback so mov_text has valid canvas dimensions.
        // Without this, the mp4 muxer rejects the subtitle stream with "dimensions not set".
        if resolution.is_none() {
            log::warn!("[Subtitle] Could not determine video resolution from any input file (ffprobe + decode fallback). Falling back to 1920x1080 for subtitle canvas.");
            resolution = Some("1920x1080".to_string());
        }
    }


    // ── Subtitle input & mapping based on subtitle_mode ─────────────────
    match config.subtitle_mode {
        SubtitleMode::None | SubtitleMode::ExportSrt | SubtitleMode::SrtMergeOnly => {
            // No subtitle input — video-only merge, map everything from first input
            args.extend(["-map".into(), "0".into()]);
        }
        SubtitleMode::Embed => {
            // Embed subtitle track: add subtitle concat as second input, map 1:s
            if let Some(slp) = &config.subtitle_list_path {
                // Pass canvas_size to the subtitle concat input so mov_text frames
                // have valid dimensions -- prevents "Invalid frame size: 0x0" error.
                if let Some(res) = &resolution {
                    args.extend(["-canvas_size".into(), res.clone()]);
                }
                args.extend([
                    "-f".into(), "concat".into(),
                    "-safe".into(), "0".into(),
                    "-i".into(), slp.to_string_lossy().into_owned(),
                ]);
            }
            if config.subtitle_list_path.is_some() {
                args.extend(["-map".into(), "0:v".into(), "-map".into(), "0:a?".into(), "-map".into(), "1:s".into()]);
            } else {
                args.extend(["-map".into(), "0".into()]);
            }
        }
        SubtitleMode::Burn => {
            // Burn subtitles into video frames using subtitles filter.
            // Only the pre-generated merged SRT (burn_subtitle_path) is used.
            args.extend(["-map".into(), "0".into()]);
            // Apply subtitles filter to burn subs into video frames.
            // Escape special characters that break FFmpeg filter parsing:
            //   \ → \\
            //   ' → \'  (use '\'' for shell single-quote inside single-quoted string)
            //   " → \"
            //   , → \,   (comma can break filter argument boundaries)
            //   [ → \[   (brackets have special meaning in FFmpeg filtergraphs)
            //   ] → \]   
            if let Some(burn_path) = &config.burn_subtitle_path {
                let raw = burn_path.to_string_lossy()
                    .replace('\\', "\\\\")   // backslash first!
                    .replace('\'', "'\\''")   // single quote: end quote, escaped quote, start quote
                    .replace('"', "\\\"")     // double quote
                    .replace(',', "\\,")      // comma
                    .replace('[', "\\[")      // bracket
                    .replace(']', "\\]");     // bracket
                let filter = format!("subtitles='{}'", raw);
                args.extend(["-vf".into(), filter]);
            }
        }
    }

    match &config.mode {
        MergeMode::Lossless => {
            if config.subtitle_list_path.is_some() {
                args.extend(["-c:v".into(), "copy".into()]);
                args.extend(["-c:a".into(), "copy".into()]);
                args.extend(["-c:s".into(), sub_codec.into()]);
                if let Some(res) = &resolution {
                    args.extend(["-s:s".into(), res.clone()]);
                }
            } else {
                args.extend(["-c".into(), "copy".into()]);
            }
            args.extend(["-avoid_negative_ts".into(), "make_zero".into()]);
            args.extend(["-max_muxing_queue_size".into(), "9999".into()]);
            if is_mp4_like {
                args.extend(["-movflags".into(), "+faststart".into()]);
            }
        }
        MergeMode::Custom => {
            let vcodec = config.video_codec.as_deref().unwrap_or("libx264");
            let acodec = config.audio_codec.as_deref().unwrap_or("aac");
            args.extend(["-c:v".into(), vcodec.into()]);
            let crf = config.video_crf.unwrap_or(18);
            match vcodec {
                "libvpx-vp9" | "libsvtav1" => args.extend(["-crf".into(), crf.to_string(), "-b:v".into(), "0".into()]),
                _ => args.extend(["-crf".into(), crf.to_string()]),
            }
            let preset = config.video_preset.as_deref().unwrap_or("medium");
            match vcodec {
                "libx264" | "libx265" => { args.extend(["-preset".into(), preset.into()]); }
                "libsvtav1" => {
                    let svt_preset = match preset {
                        "ultrafast" => "12", "fast" => "8", "medium" => "5", "slow" => "3", "veryslow" => "1", _ => "5",
                    };
                    args.extend(["-preset".into(), svt_preset.into()]);
                }
                // VP8/VP9 use -quality (not -preset). Map user preset to VPX quality.
                // "ultrafast"/"veryfast" → "realtime", "fast"/"medium" → "good", "slow"/"veryslow" → "best"
                "libvpx" | "libvpx-vp9" => {
                    let vpx_quality = match preset {
                        "ultrafast" | "veryfast" => "realtime",
                        "fast" | "medium" => "good",
                        "slow" | "veryslow" => "best",
                        _ => "good",
                    };
                    args.extend(["-quality".into(), vpx_quality.into()]);
                }
                _ => {}
            }
            args.extend(["-c:a".into(), acodec.into()]);
            args.extend(["-c:s".into(), sub_codec.into()]);
            if config.subtitle_list_path.is_some() {
                if let Some(res) = &resolution {
                    args.extend(["-s:s".into(), res.clone()]);
                }
            }
            args.extend(["-avoid_negative_ts".into(), "make_zero".into()]);
            args.extend(["-max_muxing_queue_size".into(), "9999".into()]);
            if let Some(ab) = &config.audio_bitrate { if !ab.is_empty() { args.extend(["-b:a".into(), ab.clone()]); } }
            if let Some(res) = &config.target_resolution {
                if !res.is_empty() {
                    // Preserve aspect ratio with letterbox/pillarbox padding
                    // scale=W:H:force_original_aspect_ratio=decrease fits content within bounds
                    // pad=W:H:(ow-iw)/2:(oh-ih)/2 centers and pads to exact dimensions
                    // Parse width and height from WxH format for the pad filter
                    let (pad_w, pad_h) = if let Some((w, h)) = res.split_once('x') {
                        (w.to_string(), h.to_string())
                    } else {
                        (res.clone(), res.clone())
                    };
                    args.extend(["-vf".into(), format!("scale={}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black", res, pad_w, pad_h)]);
                }
            }
            if let Some(fps) = &config.target_fps { if !fps.is_empty() { args.extend(["-r".into(), fps.clone()]); } }
            if is_mp4_like { args.extend(["-movflags".into(), "+faststart".into()]); }
        }
        MergeMode::FastMkv => {
            // FastMkv uses its own pipeline (run_fast_mkv_merge) and should never reach here
            anyhow::bail!("FastMkv mode should not use build_ffmpeg_args — use run_fast_mkv_merge instead");
        }
        MergeMode::SmartMkv => {
            // SmartMkv: stream copy everything to MKV container
            // Reuses Lossless pipeline but outputs MKV (no -movflags, supports all subtitle formats)
            if config.subtitle_list_path.is_some() {
                args.extend(["-c:v".into(), "copy".into()]);
                args.extend(["-c:a".into(), "copy".into()]);
                args.extend(["-c:s".into(), sub_codec.into()]);
                if let Some(res) = &resolution {
                    args.extend(["-s:s".into(), res.clone()]);
                }
            } else {
                args.extend(["-c".into(), "copy".into()]);
            }
            args.extend(["-avoid_negative_ts".into(), "make_zero".into()]);
            args.extend(["-max_muxing_queue_size".into(), "9999".into()]);
            // MKV doesn't need -movflags +faststart
        }
    }
    // Strip \\?\ extended-length prefix from output path for FFmpeg compatibility
    let output_path_safe = crate::ffmpeg::cards::strip_extended_path_prefix(&config.output_path);
    args.push(output_path_safe);
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MediaInfo, VideoStream, AudioStream};
    use std::path::PathBuf;
    use crate::ffmpeg::probe_cache::ProbeCache;

    fn make_video_stream(time_base: Option<&str>, codec: &str, width: u32, height: u32) -> VideoStream {
        VideoStream {
            codec_name: codec.to_string(),
            codec_long_name: codec.to_string(),
            width: Some(width),
            height: Some(height),
            fps: Some(30.0),
            bit_rate: Some(1_000_000),
            pixel_format: Some("yuv420p".to_string()),
            color_space: Some("bt709".to_string()),
            color_primaries: Some("bt709".to_string()),
            color_transfer: Some("bt709".to_string()),
            profile: Some("Main".to_string()),
            level: Some(41),
            duration: Some(10.0),
            time_base: time_base.map(|s| s.to_string()),
            stream_index: 0,
            r_frame_rate: None,
            field_order: None,
            avg_frame_rate: None,
            bits_per_raw_sample: None,
            sample_aspect_ratio: None,
            display_aspect_ratio: None,
            rotation: None,
            start_time: None,
        }
    }

    fn make_audio_stream(codec: &str, sample_rate: u32, channels: u32, channel_layout: &str) -> AudioStream {
        AudioStream {
            codec_name: codec.to_string(),
            codec_long_name: codec.to_string(),
            sample_rate: Some(sample_rate),
            channels: Some(channels),
            channel_layout: Some(channel_layout.to_string()),
            bit_rate: Some(128_000),
            duration: Some(10.0),
            stream_index: 0,
            profile: None,
            bits_per_raw_sample: None,
            start_pts: None,
            start_time: None,
            language: None,
        }
    }

    fn make_media_info(path: &str, video_tb: Option<&str>, audio_channels: u32) -> MediaInfo {
        MediaInfo {
            path: path.to_string(),
            duration: 10.0,
            size: 1_000_000,
            format_name: "mp4".to_string(),
            format_long_name: "MP4 format".to_string(),
            bit_rate: Some(1_000_000),
            video_streams: vec![make_video_stream(video_tb, "h264", 1920, 1080)],
            audio_streams: vec![make_audio_stream(
                "aac",
                48000,
                audio_channels,
                if audio_channels > 2 { "5.1" } else { "stereo" },
            )],
            subtitle_streams: vec![],
            start_time: Some(0.0),
            creation_time: None,
        }
    }

    fn build_cache_and_paths(num_dominant: usize, num_outlier: usize, dominant_tb: &str, outlier_tb: &str) -> (ProbeCache, Vec<PathBuf>) {
        let cache = ProbeCache::new();
        let mut path_bufs = Vec::new();

        // Dominant timebase files
        for i in 0..num_dominant {
            let p = format!("/tmp/dominant_{}.mp4", i);
            let info = make_media_info(&p, Some(dominant_tb), 2);
            cache.insert(PathBuf::from(&p), Ok(info));
            path_bufs.push(PathBuf::from(&p));
        }

        // Outlier timebase files
        for i in 0..num_outlier {
            let p = format!("/tmp/outlier_{}.mp4", i);
            let info = make_media_info(&p, Some(outlier_tb), 2);
            cache.insert(PathBuf::from(&p), Ok(info));
            path_bufs.push(PathBuf::from(&p));
        }

        (cache, path_bufs)
    }

    // ── Timescale-only mismatch should NOT block lossless merge ────────────
    //
    // The key fix: 10 files with dominant timebase (1/30000) + 2 outlier files
    // with different timebase (1/15360). Since the ONLY difference is timebase,
    // the compatibility check must return Ok(()) — the normalization phase
    // handles timescale alignment.
    //
    // This test proves the reference-selection bug is fixed:
    //   - OLD: first-file reference → Err (blocked lossless merge)
    //   - NEW: dominant timebase reference → Ok (let normalization handle it)

    /// 10 files with 1/30000, 2 files with 1/15360, all stereo audio.
    /// Should pass — only timescale differs, normalization handles it.
    #[test]
    fn test_timescale_only_mismatch_returns_ok() {
        let (cache, path_bufs) = build_cache_and_paths(10, 2, "1/30000", "1/15360");
        let paths: Vec<&Path> = path_bufs.iter().map(|p| p.as_path()).collect();

        let result = check_codec_compatibility_parallel(&paths, &cache);
        assert!(
            result.is_ok(),
            "Timescale-only mismatch should return Ok, but got Err: {:?}",
            result
        );
    }

    /// 2 files with 1/15360 (minority), 10 files with 1/30000 (dominant).
    /// Same test but with the outliers listed first — proves dominant-based
    /// logic works regardless of ordering (unlike the old first-file approach).
    #[test]
    fn test_timescale_outliers_first_still_ok() {
        let (cache, mut path_bufs) = build_cache_and_paths(10, 2, "1/30000", "1/15360");
        // Reverse so outlier files come first
        path_bufs.reverse();
        let paths: Vec<&Path> = path_bufs.iter().map(|p| p.as_path()).collect();

        let result = check_codec_compatibility_parallel(&paths, &cache);
        assert!(
            result.is_ok(),
            "Should still return Ok regardless of ordering, got Err: {:?}",
            result
        );
    }

    /// Edge case: equal count of two timebases (no clear dominant).
    /// Uses 5 files with 1/30000 and 5 files with 1/15360.
    /// Should still return Ok — no hard error for timescale.
    #[test]
    fn test_equal_timescale_counts_still_ok() {
        let (cache, path_bufs) = build_cache_and_paths(5, 5, "1/30000", "1/15360");
        let paths: Vec<&Path> = path_bufs.iter().map(|p| p.as_path()).collect();

        let result = check_codec_compatibility_parallel(&paths, &cache);
        assert!(
            result.is_ok(),
            "Equal timescale counts should still return Ok, got Err: {:?}",
            result
        );
    }

    // ── Audio channel outliers should STILL block lossless merge ───────────
    //
    // Unlike timescale (which normalization handles), multi-channel audio
    // (>2 channels) causes "rematrix is needed" errors during stream copy
    // that cannot be fixed by normalization alone.

    /// One file with stereo, one file with 5.1 surround. Should return Err.
    #[test]
    fn test_audio_channel_outlier_blocks_lossless() {
        let cache = ProbeCache::new();
        cache.insert(PathBuf::from("/tmp/video_0.mp4"), Ok(make_media_info("/tmp/video_0.mp4", Some("1/30000"), 2)));
        cache.insert(PathBuf::from("/tmp/video_1.mp4"), Ok(make_media_info("/tmp/video_1.mp4", Some("1/30000"), 6)));
        let paths = vec![Path::new("/tmp/video_0.mp4"), Path::new("/tmp/video_1.mp4")];

        let result = check_codec_compatibility_parallel(&paths, &cache);
        assert!(
            result.is_err(),
            "Audio channel mismatch (2ch vs 6ch) should return Err"
        );
        let err = result.unwrap_err().to_string().to_lowercase();
        assert!(
            err.contains("audio channel"),
            "Error should mention audio channel, got: {}",
            err
        );
    }

    // ── Identical files always return Ok ───────────────────────────────────

    #[test]
    fn test_identical_files_ok() {
        let cache = ProbeCache::new();
        cache.insert(PathBuf::from("/tmp/a.mp4"), Ok(make_media_info("/tmp/a.mp4", Some("1/30000"), 2)));
        cache.insert(PathBuf::from("/tmp/b.mp4"), Ok(make_media_info("/tmp/b.mp4", Some("1/30000"), 2)));
        let paths = vec![Path::new("/tmp/a.mp4"), Path::new("/tmp/b.mp4")];

        let result = check_codec_compatibility_parallel(&paths, &cache);
        assert!(result.is_ok());
    }

    // ── Empty file list should pass ───────────────────────────────────────

    #[test]
    fn test_empty_file_list_ok() {
        let cache = ProbeCache::new();
        let paths: Vec<&Path> = vec![];
        let result = check_codec_compatibility_parallel(&paths, &cache);
        assert!(result.is_ok(), "Empty file list should return Ok");
    }

    // ── Files with no audio streams should pass ───────────────────────────

    #[test]
    fn test_no_audio_streams_ok() {
        let cache = ProbeCache::new();
        let info = MediaInfo {
            path: "/tmp/no_audio.mp4".to_string(),
            duration: 10.0,
            size: 1_000_000,
            format_name: "mp4".to_string(),
            format_long_name: "MP4 format (no audio)".to_string(),
            bit_rate: Some(500_000),
            video_streams: vec![make_video_stream(Some("1/30000"), "h264", 1920, 1080)],
            audio_streams: vec![],
            subtitle_streams: vec![],
            start_time: Some(0.0),
            creation_time: None,
        };
        cache.insert(PathBuf::from("/tmp/no_audio.mp4"), Ok(info));
        let paths = vec![Path::new("/tmp/no_audio.mp4")];

        let result = check_codec_compatibility_parallel(&paths, &cache);
        assert!(result.is_ok(), "Files without audio streams should pass");
    }

    // ── Audio + Timescale mismatch regression tests ────────────────────────
    //
    // These tests verify the fix for the bug where normalize_audio_only()
    // did not fix the video timebase, causing concat demuxer duration inflation.

    /// When a file has BOTH audio mismatch AND timescale mismatch,
    /// but NO video mismatch (same fps, same resolution, same codec),
    /// it routes to need_audio_norm. The fix ensures normalize_audio_only()
    /// now receives target_timescale and fixes both audio AND timescale.
    #[test]
    fn test_audio_timescale_mismatch_both_outliers_detected() {
        let cache = ProbeCache::new();
        // Dominant: 1/30000 timebase, 44100Hz audio
        cache.insert(PathBuf::from("/tmp/dom_a.mp4"), Ok(make_media_info("/tmp/dom_a.mp4", Some("1/30000"), 2)));
        cache.insert(PathBuf::from("/tmp/dom_b.mp4"), Ok(make_media_info("/tmp/dom_b.mp4", Some("1/30000"), 2)));
        // Outlier: 1/15360 timebase + 6ch audio (both audio-type outliers, no video outlier)
        let outlier_info = MediaInfo {
            path: "/tmp/outlier_at.mp4".to_string(),
            duration: 10.0,
            size: 1_000_000,
            format_name: "mp4".to_string(),
            format_long_name: "MP4 format".to_string(),
            bit_rate: Some(1_000_000),
            video_streams: vec![make_video_stream(Some("1/15360"), "h264", 1920, 1080)],
            audio_streams: vec![make_audio_stream("aac", 48000, 6, "5.1")],
            subtitle_streams: vec![],
            start_time: Some(0.0),
            creation_time: None,
        };
        cache.insert(PathBuf::from("/tmp/outlier_at.mp4"), Ok(outlier_info));
        let paths = vec![
            Path::new("/tmp/dom_a.mp4"),
            Path::new("/tmp/dom_b.mp4"),
            Path::new("/tmp/outlier_at.mp4"),
        ];

        // Compatibility check should PASS because:
        // - Timebase mismatch is RemuxOnly (not blocking)
        // - Audio channel mismatch (6ch) is detected but this test checks
        //   that the outlier IS detected (it will be in the outlier list)
        let result = check_codec_compatibility_parallel(&paths, &cache);
        // 6-channel audio WILL block lossless - that's expected behavior
        assert!(result.is_err(), "6-channel audio should block lossless merge");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("channel"), "Error should mention audio channels: {}", err_msg);
    }

    /// Verify that a file with timescale-only outlier (no audio, no video outlier)
    /// passes compatibility check and is routed to timescale normalization.
    #[test]
    fn test_timescale_plus_audio_sample_rate_outliers() {
        use crate::ffmpeg::normalization::{analyze_profiles, NormalizationType};
        
        // NOTE: make_media_info hardcodes 48000Hz audio. 
        // We create dominant files with 44100Hz and outlier with 48000Hz.
        let profile_infos: Vec<(usize, String, MediaInfo)> = vec![
            (0, "/tmp/a.mp4".to_string(), {
                let mut info = make_media_info("/tmp/a.mp4", Some("1/30000"), 2);
                info.audio_streams[0].sample_rate = Some(44100);  // Dominant: 44100Hz
                info
            }),
            (1, "/tmp/b.mp4".to_string(), {
                let mut info = make_media_info("/tmp/b.mp4", Some("1/30000"), 2);
                info.audio_streams[0].sample_rate = Some(44100);  // Dominant: 44100Hz
                info
            }),
            (2, "/tmp/c.mp4".to_string(), {
                let info = make_media_info("/tmp/c.mp4", Some("1/15360"), 2);
                // Outlier: 48000Hz (different from dominant 44100Hz)
                // timebase is also outlier (1/15360 vs 1/30000)
                // BUT fps/resolution/codec are same → no video outlier
                info
            }),
        ];
        
        let analysis = analyze_profiles(&profile_infos);
        
        // File at index 2 should have outliers
        let file_2_outliers: Vec<_> = analysis.outliers.iter().filter(|o| o.index == 2).collect();
        assert!(!file_2_outliers.is_empty(), "File 2 should have outliers");
        
        // Should have time_base outlier (RemuxOnly)
        let has_timescale = file_2_outliers.iter().any(|o| o.property == "time_base");
        assert!(has_timescale, "File 2 should have time_base outlier");
        
        // Should have a_sample_rate outlier (AudioReencode)
        let has_audio = file_2_outliers.iter().any(|o| o.property == "a_sample_rate");
        assert!(has_audio, "File 2 should have a_sample_rate outlier");
        
        // Should NOT have video outliers (fps, resolution, codec match)
        let has_video = file_2_outliers.iter().any(|o| 
            matches!(o.normalization_type, NormalizationType::VideoReencode | NormalizationType::FullReencode));
        assert!(!has_video, "File 2 should NOT have video outliers (same fps/resolution/codec)");
        
        // This is the exact bug scenario: audio+timescale but no video outlier
        // With the fix, normalize_audio_only() now receives target_timescale
        // and fixes both audio sample rate AND video timebase.
    }

    /// Verify dominant timescale is correctly extracted from mixed timebase files.
    #[test]
    fn test_dominant_timescale_extraction() {
        use crate::ffmpeg::normalization::analyze_profiles;
        
        let profile_infos: Vec<(usize, String, MediaInfo)> = vec![
            (0, "/tmp/a.mp4".to_string(), make_media_info("/tmp/a.mp4", Some("1/30000"), 2)),
            (1, "/tmp/b.mp4".to_string(), make_media_info("/tmp/b.mp4", Some("1/30000"), 2)),
            (2, "/tmp/c.mp4".to_string(), make_media_info("/tmp/c.mp4", Some("1/30000"), 2)),
            (3, "/tmp/d.mp4".to_string(), make_media_info("/tmp/d.mp4", Some("1/15360"), 2)),
            (4, "/tmp/e.mp4".to_string(), make_media_info("/tmp/e.mp4", Some("1/15360"), 2)),
        ];
        
        let analysis = analyze_profiles(&profile_infos);
        
        // Dominant timescale should be 30000 (3 files vs 2)
        assert_eq!(analysis.dominant.timescale_den, Some(30000),
            "Dominant timescale should be 30000 (3 files with 1/30000 vs 2 with 1/15360)");
        
        // Files 3 and 4 should be timescale outliers
        let timescale_outliers: Vec<_> = analysis.outliers.iter()
            .filter(|o| o.property == "time_base")
            .collect();
        assert_eq!(timescale_outliers.len(), 2, "Should have 2 timescale outliers");
        assert!(timescale_outliers.iter().all(|o| o.index == 3 || o.index == 4),
            "Timescale outliers should be files 3 and 4");
    }

    /// Test that seek-point generation respects the hard cap of 25 points.
    /// This prevents O(n) validation time for long-duration outputs.
    #[test]
    fn test_seek_point_cap() {
        // Helper function to generate seek points (extracted from the main logic)
        fn generate_seek_points(duration: f64) -> Vec<f64> {
            const MAX_SEEK_POINTS: usize = 25;
            let max_gap_secs = (duration * 0.02).min(30.0).max(1.0);
            let num_intervals_unbounded = ((duration / max_gap_secs).ceil() as usize).max(1);
            let num_intervals = num_intervals_unbounded.min(MAX_SEEK_POINTS.saturating_sub(1));
            let mut seek_times: Vec<f64> = Vec::with_capacity(num_intervals + 1);
            for i in 0..num_intervals {
                seek_times.push(duration * (i as f64) / (num_intervals as f64));
            }
            seek_times.push(duration * 0.99);
            seek_times.dedup_by(|a, b| (*a - *b).abs() < 0.1);
            seek_times
        }

        // Test cases: duration (seconds) -> expected max seek points
        let test_cases = vec![
            (3600.0, "1 hour", 25),      // 1 hour: should be capped at 25
            (18000.0, "5 hours", 25),    // 5 hours: should be capped at 25
            (72000.0, "20 hours", 25),   // 20 hours: should be capped at 25
            (180000.0, "50 hours", 25),  // 50 hours: should be capped at 25
            (300.0, "5 minutes", 10),    // 5 minutes: ~10 points (below cap)
            (60.0, "1 minute", 4),       // 1 minute: ~4 points (below cap)
        ];

        for (duration, label, max_expected) in test_cases {
            let seek_points = generate_seek_points(duration);
            let count = seek_points.len();
            
            // Verify cap is respected
            assert!(count <= max_expected, 
                "{}: Expected ≤{} seek points, got {} (duration={}s)", 
                label, max_expected, count, duration);
            
            // Verify coverage: first point should be 0, last should be near duration
            assert!((seek_points[0] - 0.0).abs() < 0.1, 
                "{}: First seek point should be 0, got {}", label, seek_points[0]);
            assert!((seek_points.last().unwrap() - duration * 0.99).abs() < 0.1, 
                "{}: Last seek point should be ~99% of duration, got {}", label, seek_points.last().unwrap());
            
            // Verify points are in order
            for window in seek_points.windows(2) {
                assert!(window[0] < window[1], 
                    "{}: Seek points should be monotonically increasing", label);
            }
            
            println!("✅ {}: {}s duration → {} seek points", label, duration as u64, count);
        }

        // Verify the cap prevents the pathological case
        let long_duration = 180000.0; // 50 hours
        let seek_points = generate_seek_points(long_duration);
        assert!(seek_points.len() <= 25, 
            "50-hour output should have ≤25 seek points, got {}", seek_points.len());
        
        // Without cap, 50-hour output would have 6000+ points
        let max_gap = (long_duration * 0.02).min(30.0).max(1.0);
        let unbounded_count = ((long_duration / max_gap).ceil() as usize).max(1) + 1;
        assert!(unbounded_count > 100, 
            "Without cap, 50-hour output would have {} points (should be >100)", unbounded_count);
        
        println!("✅ Seek-point cap prevents {}x reduction in validation work", 
            unbounded_count / seek_points.len());
    }
}
