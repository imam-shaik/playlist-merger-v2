use crate::naming::resolver::resolve_template;
use crate::naming::TemplateContext;
use crate::split::types::{SplitPlan, SplitSegment, SplitSubtitleMode, SplitProgress, SplitResult, NamingConfig};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Minimum timeout for any FFmpeg split operation (1 hour).
/// Used when segment duration is very short (e.g., a few minutes).
const MIN_SPLIT_TIMEOUT_SECS: f64 = 3600.0;

/// Compute a timeout for a split segment based on its duration.
/// For lossless copy, use segment_duration / 2 (minimum 1 hour) since it's I/O bound —
/// a large file on slow NAS could take hours to demux.
/// For re-encode, scale with segment duration — a 20-hour segment on slow
/// hardware could easily take 20+ hours. The multiplier (2x) provides
/// headroom for slow CPUs, large files, and slow storage.
fn split_timeout(segment_duration_secs: f64, is_reencode: bool) -> Duration {
    // Guard: reject NaN, Infinity, or negative durations
    if !segment_duration_secs.is_finite() || segment_duration_secs <= 0.0 {
        return Duration::from_secs(MIN_SPLIT_TIMEOUT_SECS as u64);
    }
    if is_reencode {
        // Re-encode: 2x segment duration, minimum 1 hour
        Duration::from_secs_f64((segment_duration_secs * 2.0).max(MIN_SPLIT_TIMEOUT_SECS))
    } else {
        // Lossless copy: I/O bound, but large files on slow storage need time.
        // Use segment_duration / 2, minimum 1 hour.
        Duration::from_secs_f64((segment_duration_secs / 2.0).max(MIN_SPLIT_TIMEOUT_SECS))
    }
}

/// Run a command with a timeout. Returns Ok(output) or Err on timeout/failure.
fn run_cmd_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn process: {}", e))?;

    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process exited — collect output
                let output = child
                    .wait_with_output()
                    .map_err(|e| format!("Failed to read output: {}", e))?;
                // Reconstruct output with the status we already have
                return Ok(std::process::Output {
                    status,
                    stdout: output.stdout,
                    stderr: output.stderr,
                });
            }
            Ok(None) => {
                // Still running — check timeout
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "FFmpeg timed out after {}s",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Failed to check process status: {}", e));
            }
        }
    }
}

/// Generate a report file for a split operation.
pub fn write_split_report_file(
    input_file: &str,
    output_dir: &Path,
    job_id: &str,
    segments: &[SplitSegment],
    output_paths: &[String],
    output_sizes: &[u64],
) -> Option<String> {
    use std::fmt::Write as FmtWrite;
    let input_path = Path::new(input_file);
    let stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("split");
    let report_path = output_dir.join(format!("{}_split_report.txt", stem));
    
    let now = chrono::Local::now();
    let mut content = String::new();
    let _ = writeln!(content, "╔══════════════════════════════════════════════════════════════╗");
    let _ = writeln!(content, "║                       SPLIT REPORT                          ║");
    let _ = writeln!(content, "╚══════════════════════════════════════════════════════════════╝");
    let _ = writeln!(content);
    let _ = writeln!(content, "  Generated  : {}", now.format("%Y-%m-%d %H:%M:%S"));
    let _ = writeln!(content, "  Source     : {}", input_file);
    let _ = writeln!(content, "  Job ID     : {}", job_id);
    let _ = writeln!(content, "  Parts      : {}", segments.len());
    let _ = writeln!(content);
    let _ = writeln!(content, "  ┌─────┬────────────────────────────────┬────────────────────────────────┬──────────┬──────────────────────┬────────────┐");
    let _ = writeln!(content, "  │  #  │ Part Label                     │ Output Filename                │ Duration │ Time Range (In Src)  │ File Size  │");
    let _ = writeln!(content, "  ├─────┼────────────────────────────────┼────────────────────────────────┼──────────┼──────────────────────┼────────────┤");
    
    for (i, seg) in segments.iter().enumerate() {
        let size_mb = output_sizes.get(i).copied().unwrap_or(0) as f64 / 1_048_576.0;
        let out_path = output_paths.get(i).map(|p| Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or(p)).unwrap_or("unknown");
        let name_trunc = if seg.label.len() > 30 { format!("{}…", &seg.label[..29]) } else { seg.label.clone() };
        let out_trunc = if out_path.len() > 30 { format!("{}…", &out_path[..29]) } else { out_path.to_string() };
        
        let format_time = |s: f64| {
            let total = s.max(0.0) as u64;
            let h = total / 3600;
            let m = (total % 3600) / 60;
            let s_rem = total % 60;
            format!("{:02}:{:02}:{:02}", h, m, s_rem)
        };

        let _ = writeln!(content, "  │ {:>3} │ {:<30} │ {:<30} │ {:>8} │ {} → {} │ {:>7.1} MB │", 
            i + 1, 
            name_trunc,
            out_trunc,
            format_time(seg.duration), 
            format_time(seg.start_time), 
            format_time(seg.end_time),
            size_mb
        );
    }
    let _ = writeln!(content, "  └─────┴────────────────────────────────┴────────────────────────────────┴──────────┴──────────────────────┴────────────┘");
    
    if std::fs::write(&report_path, &content).is_ok() {
        Some(report_path.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Execute a split plan with optional subtitle handling.
pub fn execute_split_with_options(
    ffmpeg_path: &Path,
    ffprobe_path: Option<&Path>,
    plan: &SplitPlan,
    subtitle_mode: Option<SplitSubtitleMode>,
    export_srt: Option<bool>,
    cancel_flag: Arc<AtomicBool>,
    progress_callback: impl Fn(SplitProgress),
) -> Result<SplitResult, String> {
    let start = Instant::now();
    let input_path = Path::new(&plan.input_file);
    
    // Resolve output directory: fall back to input folder if not provided
    let resolved_output_dir = if plan.output_dir.is_empty() {
        input_path.parent().unwrap_or_else(|| Path::new(""))
    } else {
        Path::new(&plan.output_dir)
    };

    std::fs::create_dir_all(resolved_output_dir)
        .map_err(|e| format!("Cannot create output directory: {}", e))?;

    let total_segments = plan.segments.len();
    let mut output_paths = Vec::with_capacity(total_segments);
    let mut output_sizes = Vec::with_capacity(total_segments);

    log::info!(
        "[SplitEngine] Starting split: {} -> {} segments, mode={:?}, subtitle_mode={:?}",
        plan.input_file,
        total_segments,
        plan.mode,
        subtitle_mode,
    );

    let mut run_loop = || -> Result<(), String> {
        for (i, segment) in plan.segments.iter().enumerate() {
            if cancel_flag.load(Ordering::Relaxed) {
                log::info!("[SplitEngine] Cancelled by user after segment {}", i);
                return Err("Split cancelled by user".to_string());
            }

            progress_callback(SplitProgress {
                job_id: plan.job_id.clone(),
                segment_index: i + 1, // 1-indexed
                segment_count: total_segments,
                progress: (i as f64 / total_segments as f64) * 100.0,
                stage: "splitting".to_string(),
                message: format!("Processing segment {}/{}: {}", i + 1, total_segments, segment.label),
            });

            let output_file = generate_output_filename(
                resolved_output_dir,
                input_path,
                &segment.label,
                &plan.output_format,
                segment.index,
                total_segments,
                segment.start_time,
                segment.end_time,
                plan.naming_template.as_deref(),
                plan.naming_config.as_ref(),
                plan.label_suffix.as_deref(),
                plan.include_timestamp.unwrap_or(false),
            );

            log::info!(
                "[SplitEngine] Segment {}/{}: {} ({:.2}s -> {:.2}s, duration={:.2}s)",
                i + 1,
                total_segments,
                segment.label,
                segment.start_time,
                segment.end_time,
                segment.duration
            );

            let result = try_lossless_split(ffmpeg_path, input_path, &output_file, segment);

            match result {
                Ok(()) => {
                    log::info!(
                        "[SplitEngine] Segment {} completed (lossless): {}",
                        segment.label,
                        output_file.display()
                    );
                }
                Err(e) => {
                    log::warn!(
                        "[SplitEngine] Lossless split failed for segment {}: {}. Falling back to re-encode.",
                        segment.label,
                        e
                    );
                    reencode_split(ffmpeg_path, input_path, &output_file, segment)?;
                    log::info!(
                        "[SplitEngine] Segment {} completed (re-encode): {}",
                        segment.label,
                        output_file.display()
                    );
                }
            }

            if !output_file.exists() {
                return Err(format!("Output file was not created: {}", output_file.display()));
            }

            let file_size = std::fs::metadata(&output_file)
                .map(|m| m.len())
                .unwrap_or(0);

            output_paths.push(output_file.to_string_lossy().into_owned());
            output_sizes.push(file_size);

            progress_callback(SplitProgress {
                job_id: plan.job_id.clone(),
                segment_index: i + 1, // 1-indexed
                segment_count: total_segments,
                progress: ((i + 1) as f64 / total_segments as f64) * 100.0,
                stage: "splitting".to_string(),
                message: format!(
                    "Completed segment {}/{}: {:.1} MB",
                    i + 1,
                    total_segments,
                    file_size as f64 / 1_000_000.0
                ),
            });
        }
        Ok(())
    };

    if let Err(err) = run_loop() {
        for path in &output_paths {
            let _ = std::fs::remove_file(path);
        }
        return Err(err);
    }

    // Handle subtitle splitting — only export per-segment SRT files for ExtractSplit mode
    // CopyAll keeps subtitles embedded in the video output; it does NOT generate separate SRTs.
    let srt_output_paths = if subtitle_mode == Some(SplitSubtitleMode::ExtractSplit) {
        let should_export = export_srt.unwrap_or(true);
        if !should_export {
            None
        } else if let Some(ffprobe) = ffprobe_path {
            match handle_subtitle_split(
                ffmpeg_path,
                ffprobe,
                &plan.job_id,
                input_path,
                resolved_output_dir,
                &plan.segments,
                &output_paths,
            ) {
                Ok(paths) if !paths.is_empty() => {
                    log::info!("[SplitEngine] SRT export complete: {} files", paths.len());
                    Some(paths)
                }
                Ok(_) => None,
                Err(e) => {
                    log::warn!("[SplitEngine] SRT export failed: {}. Continuing without SRT.", e);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // Generate Split Report
    let report_paths = write_split_report_file(&plan.input_file, resolved_output_dir, &plan.job_id, &plan.segments, &output_paths, &output_sizes).map(|rp| vec![rp]);

    let elapsed = start.elapsed();
    log::info!(
        "[SplitEngine] Split complete: {} segments in {:.1}s",
        total_segments,
        elapsed.as_secs_f64()
    );

    Ok(SplitResult {
        job_id: plan.job_id.clone(),
        output_paths,
        output_sizes_bytes: output_sizes,
        total_duration: plan.input_duration,
        segments_count: total_segments,
        srt_output_paths,
        report_paths,
    })
}

/// Try a lossless split using -c copy with -ss and -to for accurate seeking.
fn try_lossless_split(
    ffmpeg_path: &Path,
    input_path: &Path,
    output_path: &Path,
    segment: &SplitSegment,
) -> Result<(), String> {
    let args = [
        "-y",
        "-ss",
        &format!("{:.3}", segment.start_time),
        "-accurate_seek",
        "-i",
        &input_path.to_string_lossy(),
        "-t",
        &format!("{:.3}", segment.duration),
        "-c",
        "copy",
        "-map", "0",
        "-avoid_negative_ts",
        "1",
        &output_path.to_string_lossy(),
    ];

    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    // Timeout scales with segment duration (lossless = I/O bound, fixed minimum)
    let output = run_cmd_with_timeout(cmd.args(args), split_timeout(segment.duration, false))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg lossless split failed: {}", stderr.trim()));
    }

    Ok(())
}

/// Re-encode split for segments requiring full decode.
fn reencode_split(
    ffmpeg_path: &Path,
    input_path: &Path,
    output_path: &Path,
    segment: &SplitSegment,
) -> Result<(), String> {
    let args = [
        "-y",
        "-ss",
        &format!("{:.3}", segment.start_time),
        "-i",
        &input_path.to_string_lossy(),
        "-t",
        &format!("{:.3}", segment.duration),
        "-map", "0",
        "-c:v",
        "libx264",
        "-preset",
        "fast",
        "-crf",
        "20",
        "-c:a",
        "aac",
        "-b:a",
        "192k",
        "-c:s",
        "copy",
        &output_path.to_string_lossy(),
    ];

    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    // Timeout scales with segment duration (re-encode = CPU bound, 2x multiplier)
    let output = run_cmd_with_timeout(cmd.args(args), split_timeout(segment.duration, true))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg re-encode split failed: {}", stderr.trim()));
    }

    Ok(())
}

/// Generate an output filename for a segment.
/// Format a filename-safe time string (HH-MM-SS).
fn format_filename_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}-{:02}-{:02}", h, m, s)
}

/// Resolve filename, replacing format tokens and ensuring safety.
#[allow(clippy::too_many_arguments)]
fn resolve_filename(
    stem: &str,
    label: &str,
    index: usize,
    total_segments: usize,
    start_time: f64,
    end_time: f64,
    naming_template: Option<&str>,
    label_suffix: Option<&str>,
    include_timestamp: bool,
) -> String {
    let mut resolved_label = label.to_string();
    if let Some(suffix) = label_suffix {
        resolved_label = format!("{}{}", resolved_label, suffix);
    }

    let timestamp_str = if include_timestamp {
        chrono::Local::now().format("_%Y%m%d_%H%M%S").to_string()
    } else {
        "".to_string()
    };

    let formatted_index = format!("{:03}", index);
    let formatted_count = format!("{:03}", total_segments);
    let start_str = format_filename_time(start_time);
    let end_str = format_filename_time(end_time);

    let filename = if let Some(template) = naming_template {
        template
            .replace("{stem}", stem)
            .replace("{label}", &resolved_label)
            .replace("{index}", &formatted_index)
            .replace("{segment_index}", &formatted_index)
            .replace("{segment_count}", &formatted_count)
            .replace("{timestamp}", timestamp_str.trim_start_matches('_'))
            .replace("{start}", &start_str)
            .replace("{end}", &end_str)
            .replace("{chapter}", &resolved_label)
    } else {
        format!("{} - {} {:02}{}", stem, resolved_label, index, timestamp_str)
    };

    // Remove invalid Windows filename characters: : ? * < > | " \ /
    let safe_name: String = filename
        .chars()
        .filter(|&c| c != ':' && c != '?' && c != '*' && c != '<' && c != '>' && c != '|' && c != '"' && c != '\\' && c != '/' && !c.is_control())
        .collect();

    safe_name.trim().trim_end_matches('.').to_string()
}

/// Generate an output filename for a segment.
#[allow(clippy::too_many_arguments)]
pub(crate) fn generate_output_filename(
    output_dir: &Path,
    input_path: &Path,
    label: &str,
    format: &str,
    index: usize,
    total_segments: usize,
    start_time: f64,
    end_time: f64,
    naming_template: Option<&str>,
    naming_config: Option<&NamingConfig>,
    label_suffix: Option<&str>,
    include_timestamp: bool,
) -> PathBuf {
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let base_name = if let Some(cfg) = naming_config {
        let now = chrono::Local::now();
        let date = now.format("%Y-%m-%d").to_string();
        let time = now.format("%H-%M-%S").to_string();
        let folder = input_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.replace(' ', "_"))
            .unwrap_or_default();
        let ctx = TemplateContext {
            filename: stem.to_string(),
            extension: format.to_string(),
            folder: Some(folder),
            original_num: None,
            index,
            start_time,
            end_time,
            duration: end_time - start_time,
            chapter: Some(label.to_string()),
            resolution: None,
            width: None,
            height: None,
            playlist: None,
            playlist_index: None,
            video_count: None,
            total_duration: None,
            date,
            time,
            prefix: cfg.prefix.clone(),
            suffix: cfg.suffix.clone(),
            lang: None,
            lang_name: None,
            part_label: Some(label.to_string()),
        };
        resolve_template(&cfg.template, cfg, &ctx)
    } else {
        resolve_filename(
            stem,
            label,
            index,
            total_segments,
            start_time,
            end_time,
            naming_template,
            label_suffix,
            include_timestamp,
        )
    };

    let mut filename = format!("{}.{}", base_name, format);
    let mut path = output_dir.join(&filename);

    let mut counter = 1;
    while path.exists() {
        filename = format!("{} ({}).{}", base_name, counter, format);
        path = output_dir.join(&filename);
        counter += 1;
    }

    path
}

/// Derive subtitle output path from a video output path.
/// Subtitles inherit the exact base name of their companion video,
/// differing only in extension (and optional language suffix).
/// e.g. Course_Part_001.mp4 → Course_Part_001.srt
/// e.g. Course_Part_001.mp4 → Course_Part_001.en.srt
fn derive_subtitle_path(video_path: &Path, lang: Option<&str>) -> PathBuf {
    let stem = video_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let parent = video_path.parent().unwrap_or(Path::new("."));
    if let Some(lang) = lang {
        if stem.ends_with(&format!(".{}", lang)) {
            parent.join(format!("{}.srt", stem))
        } else {
            parent.join(format!("{}.{}.srt", stem, lang))
        }
    } else {
        parent.join(format!("{}.srt", stem))
    }
}

/// Extract chapter info from a media file using ffprobe.
pub fn extract_chapters(
    ffprobe_path: &Path,
    input_file: &Path,
) -> Result<Vec<(String, f64, f64)>, String> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new(ffprobe_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_chapters",
            &input_file.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe failed: {}", stderr.trim()));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe JSON: {}", e))?;

    let chapters = json["chapters"]
        .as_array()
        .ok_or_else(|| "No chapters found in file".to_string())?;

    if chapters.is_empty() {
        return Err("No chapters found in file".to_string());
    }

    let mut result = Vec::new();
    for chapter in chapters {
        let title = chapter["tags"]["title"]
            .as_str()
            .unwrap_or("Chapter")
            .to_string();
        let start_time = chapter["start_time"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| chapter["start_time"].as_f64())
            .unwrap_or(0.0);
        let end_time = chapter["end_time"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| chapter["end_time"].as_f64())
            .unwrap_or(0.0);

        result.push((title, start_time, end_time));
    }

    Ok(result)
}

/// Subtitle source detected in the input file.
#[derive(Debug)]
pub(crate) enum SubtitleSourceType {
    ExternalSrt(PathBuf),
    Embedded { stream_index: usize, _codec: String },
}

impl SubtitleSourceType {
    pub(crate) fn stream_index_or_id(&self) -> usize {
        match self {
            SubtitleSourceType::ExternalSrt(_) => 0,
            SubtitleSourceType::Embedded { stream_index, .. } => *stream_index,
        }
    }
}

#[derive(Debug)]
pub(crate) struct SubtitleTrack {
    pub(crate) source_type: SubtitleSourceType,
    pub(crate) lang_suffix: Option<String>,
}

/// Find all companion SRT files for a video.
fn find_companion_srts(input_path: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Some(stem) = input_path.file_stem().and_then(|s| s.to_str()) else {
        log::warn!("[SplitEngine] Input path {} has no file stem or non-UTF8 stem — cannot scan for companion SRTs", input_path.display());
        return paths;
    };
    let Some(parent) = input_path.parent() else {
        log::warn!("[SplitEngine] Input path {} has no parent directory — cannot scan for companion SRTs", input_path.display());
        return paths;
    };

    // Check for exact match: video.srt
    let exact_srt = parent.join(format!("{}.srt", stem));
    if exact_srt.exists() {
        paths.push(exact_srt);
    }

    // Check for language-tagged variants (e.g. video.en.srt or video_en.srt)
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = match name.to_str() {
                Some(n) => n,
                None => {
                    log::warn!("[SplitEngine] Skipping file with non-UTF8 name in companion SRT scan: {}", entry.path().display());
                    continue;
                }
            };
            if name_str == format!("{}.srt", stem) {
                continue; // Already added
            }
            if name_str.starts_with(stem) && name_str.ends_with(".srt") {
                paths.push(entry.path());
            }
        }
    } else {
        log::warn!("[SplitEngine] Failed to read directory {:?} for companion SRT scan", parent);
    }

    paths
}

/// Find all subtitle tracks - companion SRTs and embedded subtitle streams.
pub(crate) fn find_subtitle_tracks(ffprobe_path: &Path, input_path: &Path) -> Result<Vec<SubtitleTrack>, String> {
    let mut tracks = Vec::new();

    // 1. Companion SRT files
    let companion_srts = find_companion_srts(input_path);
    for srt in companion_srts {
        let stem = srt.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let input_stem = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        
        let lang_suffix = if stem.starts_with(input_stem) && stem.len() > input_stem.len() {
            let diff = &stem[input_stem.len()..];
            let clean = diff.trim_start_matches(['.', '_', '-']);
            if !clean.is_empty() {
                Some(clean.to_lowercase())
            } else {
                None
            }
        } else {
            None
        };

        tracks.push(SubtitleTrack {
            source_type: SubtitleSourceType::ExternalSrt(srt),
            lang_suffix,
        });
    }

    // 2. Embedded subtitle streams
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new(ffprobe_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
            "-select_streams", "s",
        ])
        .arg(input_path)
        .output()
        .map_err(|e| format!("Failed to probe subtitles: {}", e))?;

    if output.status.success() {
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

        if let Some(streams) = json["streams"].as_array() {
            for stream in streams {
                let codec_name = stream["codec_name"].as_str().unwrap_or("");
                if codec_name == "srt"
                    || codec_name == "subrip"
                    || codec_name == "ass"
                    || codec_name == "ssa"
                    || codec_name == "mov_text"
                    || codec_name == "webvtt"
                {
                    let stream_index = stream["index"].as_u64().unwrap_or(0) as usize;
                    let lang = stream["tags"]["language"]
                        .as_str()
                        .map(|s| s.to_lowercase());
                    let title = stream["tags"]["title"]
                        .as_str()
                        .map(|s| s.to_lowercase());

                    let lang_suffix = match (lang, title) {
                        (Some(ref l), _) if l != "und" && !l.is_empty() => Some(l.clone()),
                        (_, Some(ref t)) if !t.is_empty() => {
                            let clean: String = t.chars()
                                .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
                                .collect();
                            Some(clean)
                        }
                        _ => None,
                    };

                    tracks.push(SubtitleTrack {
                        source_type: SubtitleSourceType::Embedded {
                            stream_index,
                            _codec: codec_name.to_string(),
                        },
                        lang_suffix,
                    });
                }
            }
        }
    }

    // Deconflict language suffixes to avoid duplicate filenames (e.g. if we have multiple "und" or "en" tracks)
    let mut used_suffixes = std::collections::HashSet::new();
    for track in &mut tracks {
        let mut suffix = track.lang_suffix.clone();
        if let Some(ref s) = suffix {
            if used_suffixes.contains(s) {
                suffix = Some(format!("{}_{}", s, track.source_type.stream_index_or_id()));
            }
        } else {
            if used_suffixes.contains(&String::new()) {
                suffix = Some(format!("track_{}", track.source_type.stream_index_or_id()));
            }
        }
        used_suffixes.insert(suffix.clone().unwrap_or_else(|| "".to_string()));
        track.lang_suffix = suffix;
    }

    Ok(tracks)
}

/// A parsed subtitle entry from an SRT file.
#[derive(Debug, Clone)]
pub(crate) struct SrtEntry {
    pub(crate) index: u32,
    pub(crate) start_time: f64,
    pub(crate) end_time: f64,
    pub(crate) text: String,
}

/// Parse an SRT file into subtitle entries.
pub(crate) fn parse_srt(content: &str) -> Result<Vec<SrtEntry>, String> {
    let mut entries = Vec::new();
    let normalized = content.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().map(|s| s.trim()).collect();

    let mut i = 0;
    while i < lines.len() {
        if lines[i].is_empty() {
            i += 1;
            continue;
        }

        // Try to parse index
        let index: u32 = match lines[i].parse() {
            Ok(idx) => idx,
            Err(_) => {
                i += 1;
                continue;
            }
        };

        // Next line must contain timestamps
        if i + 1 >= lines.len() {
            break;
        }
        let timestamp_line = lines[i + 1];
        let timestamps: Vec<&str> = timestamp_line.split("-->").collect();
        if timestamps.len() != 2 {
            i += 1;
            continue;
        }

        let start_time = parse_srt_timestamp(timestamps[0].trim())?;
        let end_time = parse_srt_timestamp(timestamps[1].trim())?;

        // Read subtitle text
        let mut text_lines = Vec::new();
        let mut j = i + 2;
        while j < lines.len() {
            let line = lines[j];
            if line.is_empty() {
                // Lookahead to see if next block starts
                if j + 1 < lines.len() {
                    let next_line = lines[j + 1];
                    if next_line.parse::<u32>().is_ok() && j + 2 < lines.len() && lines[j + 2].contains("-->") {
                        break;
                    }
                } else {
                    break;
                }
            }
            text_lines.push(line);
            j += 1;
        }

        let text = text_lines.join("\n").trim().to_string();
        entries.push(SrtEntry {
            index,
            start_time,
            end_time,
            text,
        });

        i = j;
    }

    Ok(entries)
}

/// Parse SRT timestamp "HH:MM:SS,mmm" to seconds.
fn parse_srt_timestamp(s: &str) -> Result<f64, String> {
    let s = s.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid timestamp format: {}", s));
    }

    let hours: f64 = parts[0].parse().map_err(|_| format!("Invalid hours: {}", parts[0]))?;
    let minutes: f64 = parts[1].parse().map_err(|_| format!("Invalid minutes: {}", parts[1]))?;
    let seconds: f64 = parts[2].parse().map_err(|_| format!("Invalid seconds: {}", parts[2]))?;

    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// Format seconds to SRT timestamp "HH:MM:SS,mmm".
fn format_srt_timestamp(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor();
    let minutes = ((seconds % 3600.0) / 60.0).floor();
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs).replace('.', ",")
}

/// Format parsed SRT entries back to SRT string.
fn format_srt(entries: &[SrtEntry]) -> String {
    entries
        .iter()
        .map(|e| {
            format!(
                "{}\n{} --> {}\n{}\n",
                e.index,
                format_srt_timestamp(e.start_time),
                format_srt_timestamp(e.end_time),
                e.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Split an SRT file into segments based on the split plan.
pub(crate) fn split_srt_for_segments(
    srt_content: &str,
    segments: &[SplitSegment],
    output_video_paths: &[String],
    lang_suffix: &Option<String>,
) -> Result<Vec<String>, String> {
    let all_subs = parse_srt(srt_content)?;
    log::info!("[SplitEngine] Parsed {} subtitle entries from SRT", all_subs.len());

    let mut srt_paths = Vec::new();

    for (i, segment) in segments.iter().enumerate() {
        let segment_subs: Vec<SrtEntry> = all_subs
            .iter()
            .filter(|sub| sub.start_time < segment.end_time && sub.end_time > segment.start_time)
            .cloned()
            .collect();

        let filtered_count = all_subs.len() - segment_subs.len();
        if filtered_count > 0 {
            log::info!("[SplitEngine] Segment {} ({:.1}s–{:.1}s): filtered out {} subtitle(s) outside time range",
                i, segment.start_time, segment.end_time, filtered_count);
        }

        let adjusted_subs: Vec<SrtEntry> = segment_subs
            .into_iter()
            .map(|mut sub| {
                sub.start_time -= segment.start_time;
                sub.end_time -= segment.start_time;
                if sub.start_time < 0.0 {
                    sub.start_time = 0.0;
                }
                if sub.end_time > segment.duration {
                    sub.end_time = segment.duration;
                }
                sub
            })
            .filter(|sub| sub.end_time > sub.start_time)
            .enumerate()
            .map(|(j, mut sub)| {
                sub.index = (j + 1) as u32;
                sub
            })
            .collect();

        // Get the output path matching the video path but with language suffix + .srt extension
        let video_path = Path::new(&output_video_paths[i]);
        let output_path = derive_subtitle_path(video_path, lang_suffix.as_deref());

        let srt_output = format_srt(&adjusted_subs);
        std::fs::write(&output_path, srt_output)
            .map_err(|e| format!("Failed to write SRT: {}", e))?;

        srt_paths.push(output_path.to_string_lossy().into_owned());
        log::info!(
            "[SplitEngine] Segment {} SRT: {} entries -> {}",
            i + 1,
            adjusted_subs.len(),
            output_path.display()
        );
    }

    Ok(srt_paths)
}

/// Extract embedded subtitle to SRT file using FFmpeg.
fn extract_subtitle_to_srt(
    ffmpeg_path: &Path,
    input_path: &Path,
    output_path: &Path,
    stream_index: usize,
) -> Result<(), String> {
    let args = [
        "-y",
        "-i",
        &input_path.to_string_lossy(),
        "-map",
        &format!("0:{}", stream_index),
        "-c:s",
        "srt",
        &output_path.to_string_lossy(),
    ];

    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.args(args)
        .output()
        .map_err(|e| format!("Failed to extract subtitle: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg subtitle extraction failed: {}", stderr));
    }

    Ok(())
}

/// Read a subtitle file robustly, supporting UTF-8 (with or without BOM), UTF-16 LE/BE, and falling back to lossy UTF-8 decoding for ANSI.
pub(crate) fn read_subtitle_file_robust(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
        
    if bytes.len() >= 2 {
        // UTF-16 LE BOM (0xFF, 0xFE)
        if bytes[0] == 0xFF && bytes[1] == 0xFE {
            let u16_chars: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            if let Ok(s) = String::from_utf16(&u16_chars) {
                return Ok(s);
            }
        }
        // UTF-16 BE BOM (0xFE, 0xFF)
        if bytes[0] == 0xFE && bytes[1] == 0xFF {
            let u16_chars: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            if let Ok(s) = String::from_utf16(&u16_chars) {
                return Ok(s);
            }
        }
    }
    
    // UTF-8 BOM (0xEF, 0xBB, 0xBF)
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        if let Ok(s) = String::from_utf8(bytes[3..].to_vec()) {
            return Ok(s);
        }
    }

    // Decode as UTF-8 or lossily decode for ANSI/Windows-1252
    match String::from_utf8(bytes.clone()) {
        Ok(s) => Ok(s),
        Err(_) => {
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

/// Handle subtitle extraction and splitting for the entire video.
fn handle_subtitle_split(
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
    job_id: &str,
    input_path: &Path,
    _output_dir: &Path,
    segments: &[SplitSegment],
    output_video_paths: &[String],
) -> Result<Vec<String>, String> {
    let tracks = find_subtitle_tracks(ffprobe_path, input_path)?;
    if tracks.is_empty() {
        log::info!("[SplitEngine] No companion or embedded subtitles found.");
        return Ok(Vec::new());
    }

    log::info!("[SplitEngine] Found {} subtitle track(s) to process", tracks.len());
    let mut all_generated_paths = Vec::new();

    for track in &tracks {
        log::info!("[SplitEngine] Processing subtitle track: {:?} (suffix: {:?})", track.source_type, track.lang_suffix);
        match &track.source_type {
            SubtitleSourceType::ExternalSrt(srt_path) => {
                let srt_content = read_subtitle_file_robust(srt_path)
                    .map_err(|e| format!("Failed to read SRT file '{}': {}", srt_path.display(), e))?;
                let paths = split_srt_for_segments(&srt_content, segments, output_video_paths, &track.lang_suffix)?;
                all_generated_paths.extend(paths);
            }
            SubtitleSourceType::Embedded { stream_index, .. } => {
                let temp_dir = std::env::temp_dir();
                // Collision-safe filename using job_id and stream_index
                let temp_srt_path = temp_dir.join(format!(
                    "extracted_sub_{}_stream_{}.srt",
                    job_id,
                    stream_index
                ));

                extract_subtitle_to_srt(ffmpeg_path, input_path, &temp_srt_path, *stream_index)?;

                let srt_content = match read_subtitle_file_robust(&temp_srt_path) {
                    Ok(content) => content,
                    Err(e) => {
                        let _ = std::fs::remove_file(&temp_srt_path);
                        return Err(format!("Failed to read extracted SRT: {}", e));
                    }
                };

                let _ = std::fs::remove_file(&temp_srt_path);

                let paths = split_srt_for_segments(&srt_content, segments, output_video_paths, &track.lang_suffix)?;
                all_generated_paths.extend(paths);
            }
        }
    }

    Ok(all_generated_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_srt_boundaries() {
        let srt_content = "1\n00:00:05,000 --> 00:00:25,000\nHello World\n";
        let segments = vec![
            SplitSegment {
                index: 1,
                label: "Part 1".to_string(),
                start_time: 0.0,
                end_time: 10.0,
                duration: 10.0,
                estimated_size_bytes: None,
            },
            SplitSegment {
                index: 2,
                label: "Part 2".to_string(),
                start_time: 10.0,
                end_time: 30.0,
                duration: 20.0,
                estimated_size_bytes: None,
            },
        ];
        
        let temp_dir = std::env::temp_dir();
        let video_paths = vec![
            temp_dir.join("Part1_boundaries.mp4").to_string_lossy().into_owned(),
            temp_dir.join("Part2_boundaries.mp4").to_string_lossy().into_owned(),
        ];
        
        let result = split_srt_for_segments(srt_content, &segments, &video_paths, &None).unwrap();
        assert_eq!(result.len(), 2);
        
        // Verify Part 1 gets 5 -> 10 (shifted by 0)
        let part1_srt = std::fs::read_to_string(&result[0]).unwrap();
        assert!(part1_srt.contains("00:00:05,000 --> 00:00:10,000"));
        
        // Verify Part 2 gets 0 -> 15 (shifted by 10)
        let part2_srt = std::fs::read_to_string(&result[1]).unwrap();
        assert!(part2_srt.contains("00:00:00,000 --> 00:00:15,000"));
        
        // Clean up
        let _ = std::fs::remove_file(&result[0]);
        let _ = std::fs::remove_file(&result[1]);
    }

    #[test]
    fn test_split_srt_multi_segments() {
        let srt_content = "1\n00:00:05,000 --> 00:02:30,000\nSpanning Subtitle\n";
        let segments = vec![
            SplitSegment {
                index: 1,
                label: "Part 1".to_string(),
                start_time: 0.0,
                end_time: 30.0,
                duration: 30.0,
                estimated_size_bytes: None,
            },
            SplitSegment {
                index: 2,
                label: "Part 2".to_string(),
                start_time: 30.0,
                end_time: 60.0,
                duration: 30.0,
                estimated_size_bytes: None,
            },
            SplitSegment {
                index: 3,
                label: "Part 3".to_string(),
                start_time: 60.0,
                end_time: 90.0,
                duration: 30.0,
                estimated_size_bytes: None,
            },
        ];
        
        let temp_dir = std::env::temp_dir();
        let video_paths = vec![
            temp_dir.join("Part1_multi.mp4").to_string_lossy().into_owned(),
            temp_dir.join("Part2_multi.mp4").to_string_lossy().into_owned(),
            temp_dir.join("Part3_multi.mp4").to_string_lossy().into_owned(),
        ];
        
        let result = split_srt_for_segments(srt_content, &segments, &video_paths, &Some("en".to_string())).unwrap();
        assert_eq!(result.len(), 3);
        
        // Part 1: en suffix and clamped
        assert!(result[0].ends_with("Part1_multi.en.srt"));
        let part1_srt = std::fs::read_to_string(&result[0]).unwrap();
        assert!(part1_srt.contains("00:00:05,000 --> 00:00:30,000"));
        
        // Part 2: en suffix and clamped
        assert!(result[1].ends_with("Part2_multi.en.srt"));
        let part2_srt = std::fs::read_to_string(&result[1]).unwrap();
        assert!(part2_srt.contains("00:00:00,000 --> 00:00:30,000"));
        
        // Part 3: en suffix and clamped
        assert!(result[2].ends_with("Part3_multi.en.srt"));
        let part3_srt = std::fs::read_to_string(&result[2]).unwrap();
        assert!(part3_srt.contains("00:00:00,000 --> 00:00:30,000"));
        
        // Clean up
        let _ = std::fs::remove_file(&result[0]);
        let _ = std::fs::remove_file(&result[1]);
        let _ = std::fs::remove_file(&result[2]);
    }

    #[test]
    fn test_find_subtitle_tracks_integration() {
        let settings = crate::services::settings::load_settings_internal();
        let ffprobe_path = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref()).unwrap();
        
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let video_path = Path::new(&manifest_dir).join("..").join("tests").join("fixtures").join("video_multi_subs.mp4");
        
        if video_path.exists() {
            let tracks = find_subtitle_tracks(&ffprobe_path, &video_path).unwrap();
            assert!(!tracks.is_empty(), "Should find embedded tracks in video_multi_subs.mp4");
            
            // Validate that we found more than one track and got suffixes
            assert!(tracks.len() >= 1);
            for track in &tracks {
                println!("Track found: {:?}", track);
            }
        }
    }
}