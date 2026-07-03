use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{Result, anyhow};
use crate::types::CardConfig;

/// Resolve a font file path for FFmpeg drawtext filter.
///
/// Priority order:
/// 1. Bundled font (arial.ttf next to the executable)
/// 2. System fonts (platform-specific paths)
/// 3. None (returns empty string, drawtext will fail silently)
///
/// Returns the font path as a String, or empty string if no font found.
fn resolve_font_path() -> String {
    // 1. Try bundled font (next to executable)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled = exe_dir.join("binaries").join("arial.ttf");
            if bundled.exists() {
                log::info!("[Cards] Using bundled font: {}", bundled.display());
                return bundled.to_string_lossy().into_owned();
            }
        }
    }

    // 2. Try system fonts (platform-specific)
    #[cfg(windows)]
    {
        let system_fonts = [
            "C:\\Windows\\Fonts\\arial.ttf",
            "C:\\Windows\\Fonts\\segoeui.ttf",
            "C:\\Windows\\Fonts\\tahoma.ttf",
            "C:\\Windows\\Fonts\\verdana.ttf",
        ];
        for font_path in &system_fonts {
            if Path::new(font_path).exists() {
                log::info!("[Cards] Using system font: {}", font_path);
                return font_path.to_string();
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let system_fonts = [
            "/System/Library/Fonts/Helvetica.ttc",
            "/System/Library/Fonts/SFNS.ttf",
            "/Library/Fonts/Arial.ttf",
        ];
        for font_path in &system_fonts {
            if Path::new(font_path).exists() {
                log::info!("[Cards] Using system font: {}", font_path);
                return font_path.to_string();
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let system_fonts = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/truetype/freefont/FreeSans.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ];
        for font_path in &system_fonts {
            if Path::new(font_path).exists() {
                log::info!("[Cards] Using system font: {}", font_path);
                return font_path.to_string();
            }
        }
    }

    log::warn!("[Cards] No font found — card text will not be visible");
    String::new()
}

/// Strip Windows `\\?\` extended-length path prefix for FFmpeg compatibility.
/// FFmpeg uses C runtime fopen() which doesn't support `\\?\` paths on Windows.
#[cfg(windows)]
pub fn strip_extended_path_prefix(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{}", rest)
    } else if let Some(rest) = p.strip_prefix("\\\\?\\") {
        rest.to_string()
    } else {
        p.to_string()
    }
}

#[cfg(not(windows))]
pub fn strip_extended_path_prefix(p: &str) -> String {
    p.to_string()
}

/// Normalize a path for FFmpeg/FFprobe compatibility:
/// 1. Strip `\\?\` extended-length prefix
/// 2. Convert backslashes to forward slashes
#[allow(dead_code)]
pub fn ffmpeg_safe_path(p: &str) -> String {
    let stripped = strip_extended_path_prefix(p);
    stripped.replace('\\', "/")
}

/// Probe the first input file to get its video/audio stream parameters.
/// Returns (width, height, video_codec, fps, pixel_format, audio_sample_rate, audio_channels, timebase_den, audio_codec).
#[allow(clippy::type_complexity)]
fn probe_first_input(_ffmpeg_path: &Path, ffprobe_path: Option<&Path>, input_files: &[String]) -> Result<(u32, u32, String, f64, String, u32, u32, u32, String)> {
    if input_files.is_empty() {
        return Err(anyhow!("No input files to probe for card dimensions"));
    }

    let ffprobe_path = match ffprobe_path {
        Some(p) => p.to_path_buf(),
        None => {
            let s = crate::services::settings::load_settings_internal();
            crate::ffmpeg::find_ffprobe(s.ffprobe_path.as_deref())
                .map_err(|e| anyhow!("ffprobe not found: {}", e))?
        },
    };

    let file = &input_files[0];
    // Strip `\\?\` prefix for ffprobe compatibility (same as concat.rs does)
    let ffprobe_path_str = strip_extended_path_prefix(file);
    log::info!("[CARDS] probe_first_input: probing '{}' (stripped: '{}')", file, ffprobe_path_str);
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new(&ffprobe_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .args(["-v", "quiet", "-print_format", "json", "-show_streams", &ffprobe_path_str])
        .output()
        .map_err(|e| {
            log::error!("[CARDS] Failed to probe first input for card: {} — path: '{}'", e, ffprobe_path_str);
            anyhow!("Failed to probe first input for card: {}", e)
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("[CARDS] ffprobe failed for '{}': stderr={}", ffprobe_path_str, stderr);
        return Err(anyhow!("ffprobe failed for '{}'", ffprobe_path_str));
    }
    log::info!("[CARDS] probe_first_input succeeded for '{}'", ffprobe_path_str);

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to parse ffprobe JSON: {}", e))?;

    let streams = json.get("streams").and_then(|s| s.as_array())
        .ok_or_else(|| anyhow!("No streams found in first input"))?;

    let mut width = 1920u32;
    let mut height = 1080u32;
    let mut video_codec = "libx264".to_string();
    let mut fps = 30.0;
    let mut pixel_format = "yuv420p".to_string();
    let mut sample_rate = 48000u32;
    let mut channels = 2u32;
    let mut timebase_den = 90000u32;
    let mut audio_codec = "aac".to_string();

    for stream in streams {
        match stream.get("codec_type").and_then(|c| c.as_str()) {
            Some("video") => {
                if let Some(w) = stream.get("width").and_then(|v| v.as_u64()) {
                    width = w as u32;
                }
                if let Some(h) = stream.get("height").and_then(|v| v.as_u64()) {
                    height = h as u32;
                }
                if let Some(c) = stream.get("codec_name").and_then(|v| v.as_str()) {
                    video_codec = c.to_string();
                }
                if let Some(pf) = stream.get("pix_fmt").and_then(|v| v.as_str()) {
                    pixel_format = pf.to_string();
                }
                let fps_str = stream.get("avg_frame_rate").and_then(|v| v.as_str()).unwrap_or("30/1");
                let fps_parts: Vec<&str> = fps_str.splitn(2, '/').collect();
                if fps_parts.len() == 2 {
                    let n: f64 = fps_parts[0].trim().parse().unwrap_or(30.0);
                    let d: f64 = fps_parts[1].trim().parse().unwrap_or(1.0);
                    if d > 0.0 { fps = n / d; }
                }
                if let Some(tb) = stream.get("time_base").and_then(|v| v.as_str()) {
                    let tb_parts: Vec<&str> = tb.splitn(2, '/').collect();
                    if tb_parts.len() == 2 {
                        if let Ok(den) = tb_parts[1].trim().parse() {
                            timebase_den = den;
                        } else {
                            log::debug!("[Cards] Failed to parse time_base denominator from '{}' — using default 90000", tb);
                        }
                    }
                }
            }
            Some("audio") => {
                if let Some(c) = stream.get("codec_name").and_then(|v| v.as_str()) {
                    audio_codec = c.to_string();
                }
                if let Some(sr) = stream.get("sample_rate").and_then(|v| v.as_str()) {
                    if let Ok(sr_val) = sr.parse::<u32>() {
                        sample_rate = sr_val;
                    } else {
                        log::debug!("[Cards] Failed to parse sample_rate from '{}' — using default 48000", sr);
                    }
                }
                if let Some(ch) = stream.get("channels").and_then(|v| v.as_u64()) {
                    channels = ch as u32;
                }
            }
            _ => {}
        }
    }

    width = width / 2 * 2;
    height = height / 2 * 2;

    Ok((width, height, video_codec, fps, pixel_format, sample_rate, channels, timebase_den, audio_codec))
}

/// Compute a readable label for the font color based on background luminance.
/// Returns the original font_color hex from CardConfig (pre-computed by frontend).
fn get_font_color(card_config: &CardConfig) -> &str {
    &card_config.font_color
}

/// Escape special characters in drawtext text values.
/// FFmpeg filter options use `:` as separator, so colons must be escaped as `\:`.
fn escape_drawtext(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace('\'', "\\'")
}

/// Render a single card video file using FFmpeg.
///
/// Creates a colored canvas with centered text overlay and silent audio track,
/// suitable for insertion into an FFmpeg concat demuxer list.
#[allow(clippy::too_many_arguments)]
fn render_single_card(
    ffmpeg_path: &Path,
    output_path: &Path,
    config: &CardConfig,
    width: u32,
    height: u32,
    video_codec: &str,
    fps: f64,
    pixel_format: &str,
    sample_rate: u32,
    channels: u32,
    timebase_den: u32,
    line1: &str,  // e.g. "Video 2"
    line2: &str,  // e.g. "My Video.mp4"
    line3: &str,  // e.g. "00:03:30"
    font_path: &str,  // resolved font path for drawtext
    audio_codec: &str,  // source audio codec (e.g. "aac", "mp3", "opus", "ac3")
) -> Result<()> {
    let duration = config.duration.max(0.5);
    let bg_color = &config.color;
    let font_color = get_font_color(config);

    // Map common codec names to FFmpeg encoders
    let codec = match video_codec {
        "h264" | "libx264" => "libx264",
        "hevc" | "h265" | "libx265" => "libx265",
        "vp9" | "libvpx-vp9" => "libvpx-vp9",
        "av1" | "libsvtav1" => "libsvtav1",
        _ => {
            log::warn!("[CARDS] Unsupported target codec '{}' for card rendering, falling back to libx264", video_codec);
            "libx264"
        }
    };

    let font_size_lg = (height as f64 / 12.0).round().clamp(24.0, 72.0) as u32;
    let font_size_md = (height as f64 / 18.0).round().clamp(18.0, 48.0) as u32;
    let font_size_sm = (height as f64 / 24.0).round().clamp(14.0, 36.0) as u32;

    // Escape font path for FFmpeg drawtext filter (colons and backslashes)
    let fontfile_arg = if !font_path.is_empty() {
        let escaped = font_path.replace('\\', "/").replace(':', "\\:");
        format!(":fontfile='{}'", escaped)
    } else {
        String::new()
    };

    // For non-integer fps (e.g., 29.97, 23.976), round for the color source
    // but preserve exact fps for output via -video_track_timescale
    let fps_int = fps.round().max(1.0) as u32;

    let mut vf_parts: Vec<String> = Vec::new();

    let channel_layout = if channels == 1 { "mono" } else { "stereo" };

    // Line 1: Index label (top third)
    if !line1.is_empty() {
        let escaped1 = escape_drawtext(line1);
        vf_parts.push(format!(
            "drawtext=text='{}':fontcolor={}:fontsize={}:x=(w-text_w)/2:y=(h/3-text_h/2){}",
            escaped1, font_color, font_size_lg, fontfile_arg
        ));
    }

    // Line 2: Name (middle)
    if !line2.is_empty() {
        let escaped2 = escape_drawtext(line2);
        vf_parts.push(format!(
            "drawtext=text='{}':fontcolor={}:fontsize={}:x=(w-text_w)/2:y=(h/2-text_h/2){}",
            escaped2, font_color, font_size_md, fontfile_arg
        ));
    }

    // Line 3: Duration (bottom third)
    if !line3.is_empty() {
        let escaped3 = escape_drawtext(line3);
        vf_parts.push(format!(
            "drawtext=text='{}':fontcolor={}:fontsize={}:x=(w-text_w)/2:y=(2*h/3-text_h/2){}",
            escaped3, font_color, font_size_sm, fontfile_arg
        ));
    }

    let filter_graph = vf_parts.join(",");

    log::info!("[Cards] Font path resolved: '{}'", font_path);
    log::info!("[Cards] Fontfile arg: '{}'", fontfile_arg);
    log::info!("[Cards] Filter graph: {}", filter_graph);

    let mut args: Vec<String> = Vec::new();
    args.push("-y".into());
    args.push("-f".into());
    args.push("lavfi".into());
    args.push("-i".into());
    args.push(format!("color=c={}:s={}x{}:d={}:r={}", bg_color, width, height, duration, fps_int));
    args.push("-f".into());
    args.push("lavfi".into());
    args.push("-i".into());
    args.push(format!("anullsrc=r={}:cl={}", sample_rate, channel_layout));
    args.push("-vf".into());
    args.push(filter_graph);
    args.push("-c:v".into());
    args.push(codec.into());
    args.push("-preset".into());
    args.push("ultrafast".into());
    args.push("-crf".into());
    args.push("23".into());
    args.push("-pix_fmt".into());
    args.push(pixel_format.into());
    args.push("-r".into());
    args.push(fps_int.to_string());
    args.push("-video_track_timescale".into());
    args.push(timebase_den.to_string());

    // Map source audio codec to FFmpeg encoder for card silent audio track.
    // This ensures the card audio is compatible with the source for concat demuxer.
    let audio_encoder = match audio_codec {
        "aac" | "mp4a" => "aac",
        "mp3" | "mp3float" => "libmp3lame",
        "opus" => "libopus",
        "ac3" => "ac3",
        "flac" => "flac",
        "vorbis" => "libvorbis",
        _ => {
            log::warn!("[CARDS] Unknown source audio codec '{}' — falling back to aac", audio_codec);
            "aac"
        }
    };
    let audio_bitrate = match audio_encoder {
        "libopus" => "128k",
        "ac3" => "192k",
        "flac" => "",  // FLAC doesn't need bitrate
        _ => "128k",
    };

    args.push("-c:a".into());
    args.push(audio_encoder.into());
    if !audio_bitrate.is_empty() {
        args.push("-b:a".into());
        args.push(audio_bitrate.into());
    }
    args.push("-shortest".into());
    // Limit FFmpeg threads for card rendering to prevent oversubscription
    let threads_per = std::thread::available_parallelism().map(|n| (n.get() / 4).max(1)).unwrap_or(2);
    args.push("-threads".into());
    args.push(threads_per.to_string());
    let output_path_ffmpeg = strip_extended_path_prefix(&output_path.to_string_lossy());
    args.push(output_path_ffmpeg);

    log::info!("[Cards] Rendering card: {} with text: {} / {} / {}", output_path.display(), line1, line2, line3);
    log::info!("[Cards] Full FFmpeg command: {} {}", ffmpeg_path.display(), args.join(" "));

    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .args(&args)
        .output()
        .map_err(|e| anyhow!("Failed to execute FFmpeg for card rendering: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("[Cards] FFmpeg card rendering FAILED for '{}': {}", output_path.display(), stderr);
        return Err(anyhow!("FFmpeg card rendering failed for '{}': {}", output_path.display(), stderr));
    }

    if !output_path.exists() || output_path.metadata().map(|m| m.len()).unwrap_or(0) == 0 {
        return Err(anyhow!("Card output file was not created or is empty: {}", output_path.display()));
    }

    let stderr_text = String::from_utf8_lossy(&output.stderr);
    if stderr_text.contains("Error") || stderr_text.contains("error") || stderr_text.contains("Invalid") || stderr_text.contains("cannot") {
        log::warn!("[Cards] FFmpeg stderr for '{}' (card may have issues): {}", output_path.display(), stderr_text);
    }

    log::info!("[CARDS] Successfully rendered card: {} ({}x{}, {:.1}s, {} fps, vcodec={}, acodec={}, pix_fmt={}:{}ch:{}Hz:{}, tb={})",
        output_path.display(), width, height, duration, fps_int, codec, audio_encoder, pixel_format, channels, sample_rate, channel_layout, timebase_den);
    
    // ── Verify card output file profile ──
    let output_size = output_path.metadata().map(|m| m.len()).unwrap_or(0);
    log::info!("[CARDS] Card file probe: path={}, size={} bytes, streams=1 video({}) + 1 audio({})",
        output_path.display(), output_size, codec, audio_encoder);

    Ok(())
}

/// Format seconds into HH:MM:SS string for display on cards.
fn format_card_duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

use std::collections::HashMap;

/// Result of interleaving cards between video segments.
pub struct InterleaveResult {
    pub files: Vec<String>,
    pub durations: Vec<f64>,
    pub segment_cards: Vec<(bool, Option<String>)>,
}

/// Core card interleaving logic shared by SmartMKV and FastMKV pipelines.
///
/// Iterates through input files, inserting rendered cards before videos based on
/// the card frequency mode (PerVideo or PerFolder). Skips the first card in PerVideo mode.
pub fn interleave_cards_with_videos(
    input_files: &[String],
    input_durations: &[f64],
    rendered: &[(String, usize)],
    card_config: &crate::types::CardConfig,
) -> InterleaveResult {
    let mut files = Vec::new();
    let mut durations = Vec::new();
    let mut segment_cards = Vec::new();
    let mut current_folder = String::new();

    for i in 0..input_files.len() {
        let folder = Path::new(&input_files[i])
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let should_insert_card = match card_config.frequency {
            crate::types::CardFrequency::PerVideo => true,
            crate::types::CardFrequency::PerFolder => folder != current_folder || i == 0,
        };

        if should_insert_card {
            let card_path = rendered.iter().find(|(_, video_idx)| *video_idx == i).map(|(p, _)| p);
            if let Some(path) = card_path {
                let skip_first_per_video = card_config.frequency == crate::types::CardFrequency::PerVideo && i == 0;
                if !skip_first_per_video {
                    files.push(path.clone());
                    durations.push(card_config.duration);
                    segment_cards.push((true, Some(card_config.color.clone())));
                }
            }
            current_folder = folder;
        }

        files.push(input_files[i].clone());
        durations.push(input_durations[i]);
        segment_cards.push((false, None));
    }

    InterleaveResult { files, durations, segment_cards }
}

/// Render all card videos for a merge job.
///
/// Returns a Vec of (card_path, segment_index) for each card that was rendered.
/// The segment_index points to the video that FOLLOWS the card (i.e., card before video N has index N).
/// The vector is in order of video segments (cards[i] goes before input_files[i+1]).
///
/// boundary_card_labels: Optional map of position -> label for boundary cards.
/// When provided, positions in the map use the provided label instead of folder-based label.
#[allow(clippy::too_many_arguments)]
pub fn render_cards_for_merge(
    ffmpeg_path: &Path,
    ffprobe_path: Option<&Path>,
    input_files: &[String],
    input_names: &[String],
    input_durations: &[f64],
    card_config: &CardConfig,
    temp_dir: &Path,
    boundary_card_labels: Option<&HashMap<usize, String>>,
) -> Result<Vec<(String, usize)>> {
    log::info!("[CARDS] render_cards_for_merge() called with {} files", input_files.len());
    log::info!("[CARDS] card_config: duration={}s, frequency={:?}, color={}", card_config.duration, card_config.frequency, card_config.color);

    if input_files.len() < 2 {
        log::info!("[CARDS] Skipping render: less than 2 files");
        return Ok(Vec::new()); // No need for cards with 0 or 1 files
    }

    if card_config.duration <= 0.0 {
        log::warn!("[CARDS] Skipping render: card duration ({}) is zero or negative", card_config.duration);
        return Ok(Vec::new());
    }

    // Create card_temp_dir if it doesn't exist
    let _ = std::fs::create_dir_all(temp_dir);

    // Probe first input for dimensions and codec info
    let (width, height, video_codec, fps, pixel_format, sample_rate, channels, timebase_den, audio_codec) =
        match probe_first_input(ffmpeg_path, ffprobe_path, input_files) {
            Ok(v) => v,
            Err(e) => {
                log::error!("[CARDS] probe_first_input failed: {:?}", e);
                return Err(e);
            }
        };

    // ── PHASE 7: CARD SOURCE TRACE ─────────────────────────────────────────
    log::info!("[FORENSIC:CARD] Source video profile (inherited by all cards):");
    log::info!("[FORENSIC:CARD]   source fps: {} (from first input)", fps);
    log::info!("[FORENSIC:CARD]   source video_codec: {}", video_codec);
    log::info!("[FORENSIC:CARD]   source audio_codec: {}", audio_codec);
    log::info!("[FORENSIC:CARD]   source timebase_den: {}", timebase_den);
    log::info!("[FORENSIC:CARD]   source resolution: {}x{}", width, height);
    log::info!("[FORENSIC:CARD]   source pixel_format: {}", pixel_format);
    log::info!("[FORENSIC:CARD]   source sample_rate: {} Hz", sample_rate);
    log::info!("[FORENSIC:CARD]   source channels: {}", channels);
    log::info!("[FORENSIC:CARD]   Card output will use: fps={}, vcodec={}, acodec={}, timebase_den={}", fps, video_codec, audio_codec, timebase_den);

    // Resolve font path for drawtext filters
    let font_path = resolve_font_path();
    if font_path.is_empty() {
        log::warn!("[CARDS] No font available — card text will not be visible. Please install a system font or bundle a font with the application.");
    } else {
        let font_exists = std::path::Path::new(&font_path).exists();
        let font_len = std::fs::metadata(&font_path).map(|m| m.len()).unwrap_or(0);
        log::info!("[CARDS] Font resolved: '{}' exists={} size={} bytes", font_path, font_exists, font_len);
        if !font_exists {
            log::error!("[CARDS] CRITICAL: Font file does NOT exist at '{}' — drawtext will produce blank cards!", font_path);
        }
    }

    let card_count = input_files.len();

    // Prepare all card render parameters upfront (labels, display names, durations)
    let card_params: Vec<(usize, String, String, String, PathBuf)> = (0..card_count)
        .map(|i| {
            let next_file_path = &input_files[i];
            let parent_folder = Path::new(next_file_path)
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let label = if let Some(labels) = boundary_card_labels {
                if let Some(boundary_label) = labels.get(&i) {
                    boundary_label.clone()
                } else if parent_folder.is_empty() {
                    format!("Video {}", i + 1)
                } else {
                    parent_folder.to_string()
                }
            } else if parent_folder.is_empty() {
                format!("Video {}", i + 1)
            } else {
                parent_folder.to_string()
            };

            let display_name = if input_names[i].len() > 35 {
                format!("{}…", &input_names[i][..34])
            } else {
                input_names[i].clone()
            };

            let duration_str = format_card_duration(input_durations[i]);
            let card_path = temp_dir.join(format!("card_{:04}.mp4", i));

            (i, label, display_name, duration_str, card_path)
        })
        .collect();

    // Render cards in parallel using scoped threads, batched to prevent process explosion.
    // Without batching: N files = N concurrent FFmpeg processes (unbounded).
    // With batching: max CARD_BATCH_SIZE concurrent FFmpeg processes at a time.
    const CARD_BATCH_SIZE: usize = 4;
    let mut card_paths: Vec<(String, usize)> = Vec::new();
    let mut created_files: Vec<PathBuf> = Vec::new();
    let ffmpeg_path_buf = ffmpeg_path.to_path_buf();
    let total_cards = card_params.len();

    let render_result: Result<()> = std::thread::scope(|s| {
        for batch_start in (0..total_cards).step_by(CARD_BATCH_SIZE) {
            let batch_end = (batch_start + CARD_BATCH_SIZE).min(total_cards);
            let batch = &card_params[batch_start..batch_end];
            log::info!("[Cards] Rendering batch {}-{} of {}", batch_start + 1, batch_end, total_cards);

            let handles: Vec<_> = batch
                .iter()
                .map(|(i, label, display_name, duration_str, card_path)| {
                    let ffmpeg_path = ffmpeg_path_buf.clone();
                    let card_path = card_path.clone();
                    let label = label.clone();
                    let display_name = display_name.clone();
                    let duration_str = duration_str.clone();
                    let video_codec = video_codec.clone();
                    let pixel_format = pixel_format.clone();
                    let font_path = font_path.clone();
                    let audio_codec = audio_codec.clone();
                    let card_config = card_config.clone();

                    s.spawn(move || {
                        render_single_card(
                            &ffmpeg_path,
                            &card_path,
                            &card_config,
                            width,
                            height,
                            &video_codec,
                            fps,
                            &pixel_format,
                            sample_rate,
                            channels,
                            timebase_den,
                            &label,
                            &display_name,
                            &duration_str,
                            &font_path,
                            &audio_codec,
                        )
                        .map(|_| (card_path, *i))
                    })
                })
                .collect();

            for handle in handles {
                match handle.join() {
                    Ok(Ok((card_path, idx))) => {
                        created_files.push(card_path.clone());
                        card_paths.push((card_path.to_string_lossy().into_owned(), idx));
                    }
                    Ok(Err(e)) => {
                        for path in &created_files {
                            log::info!("[CARDS] Cleanup: removing partial card file: {}", path.display());
                            let _ = std::fs::remove_file(path);
                        }
                        return Err(e);
                    }
                    Err(_) => {
                        for path in &created_files {
                            log::info!("[CARDS] Cleanup: removing partial card file: {}", path.display());
                            let _ = std::fs::remove_file(path);
                        }
                        return Err(anyhow!("Card rendering thread panicked"));
                    }
                }
            }
        }
        Ok(())
    });
    render_result?;

    log::info!("[CARDS] Rendered {} card(s) for merge", card_paths.len());
    for (path, seg_idx) in &card_paths {
        log::info!("[CARDS]   Card #{} (before video {}): path={}", 
            card_paths.iter().position(|(p, _)| p == path).unwrap_or(0), seg_idx, path);
    }
    Ok(card_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("CARGO_MANIFEST_DIR should have a parent")
            .to_path_buf()
    }

    fn bundled_ffmpeg() -> PathBuf {
        project_root().join("src-tauri").join("binaries").join("ffmpeg.exe")
    }

    #[test]
    fn test_render_single_card_creates_file() {
        let ffmpeg = bundled_ffmpeg();
        if !ffmpeg.exists() {
            eprintln!("SKIP: bundled ffmpeg not found");
            return;
        }

        let temp_dir = std::env::temp_dir().join("playlist_merger_card_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        let output_path = temp_dir.join("test_card.mp4");

        let config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: crate::types::CardFrequency::PerVideo,
        };

        let result = render_single_card(
            &ffmpeg,
            &output_path,
            &config,
            640,  // smaller res for speed
            360,
            "libx264",
            30.0,
            "yuv420p",
            48000,
            2,
            90000, // timebase_den
            "Video 2",
            "test_video.mp4",
            "00:03:30",
            "",  // font_path (empty for test, will use system fallback)
            "aac", // audio_codec
        );

        assert!(result.is_ok(), "Card rendering should succeed: {:?}", result.err());
        assert!(output_path.exists(), "Card output should exist");
        assert!(output_path.metadata().unwrap().len() > 1000, "Card should not be empty");

        // Verify it's a valid video file by probing
        let ffprobe = project_root().join("src-tauri").join("binaries").join("ffprobe.exe");
        if ffprobe.exists() {
            let probe_output = Command::new(&ffprobe)
                .args(["-v", "quiet", "-print_format", "json", "-show_streams", output_path.to_str().unwrap()])
                .output()
                .expect("ffprobe should run");
            if probe_output.status.success() {
                let json: serde_json::Value = serde_json::from_slice(&probe_output.stdout).unwrap_or_default();
                let streams = json.get("streams").and_then(|s| s.as_array()).cloned().unwrap_or_default();
                let video_stream = streams.iter().find(|s| s.get("codec_type") == Some(&serde_json::Value::String("video".to_string())));
                assert!(video_stream.is_some(), "Card should have a video stream");
                let audio_stream = streams.iter().find(|s| s.get("codec_type") == Some(&serde_json::Value::String("audio".to_string())));
                assert!(audio_stream.is_some(), "Card should have an audio stream (silent)");
            }
        }

        // Cleanup
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn test_render_cards_for_merge_empty() {
        let ffmpeg = bundled_ffmpeg();
        // Should return empty vec for 0 or 1 files (no cards needed)
        let temp_dir = std::env::temp_dir().join("playlist_merger_card_empty_test");
        let _ = std::fs::create_dir_all(&temp_dir);

        let config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: crate::types::CardFrequency::PerVideo,
        };

        let result = render_cards_for_merge(
            &ffmpeg,
            None,
            &[],  // no files
            &[],
            &[],
            &config,
            &temp_dir,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0, "Empty input should produce no cards");

        let result = render_cards_for_merge(
            &ffmpeg,
            None,
            &["video1.mp4".to_string()],  // single file
            &["video1.mp4".to_string()],
            &[30.0],
            &config,
            &temp_dir,
            None,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0, "Single file should produce no cards");
    }

    #[test]
    fn test_format_card_duration() {
        assert_eq!(format_card_duration(0.0), "00:00:00");
        assert_eq!(format_card_duration(30.0), "00:00:30");
        assert_eq!(format_card_duration(90.0), "00:01:30");
        assert_eq!(format_card_duration(3661.0), "01:01:01");
        assert_eq!(format_card_duration(86400.0), "24:00:00");
    }
}
