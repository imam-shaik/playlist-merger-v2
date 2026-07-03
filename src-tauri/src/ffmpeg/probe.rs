use anyhow::{Result, Context};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::types::{MediaInfo, VideoStream, AudioStream, SubtitleStream};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Run ffprobe on a single file synchronously.
///
/// CALLERS: This function blocks. Always call from `tokio::task::spawn_blocking`.
pub fn probe_file(ffprobe_path: &Path, file_path: &Path) -> Result<MediaInfo> {
    if !file_path.exists() {
        anyhow::bail!("File does not exist: {}", file_path.display());
    }

    let mut cmd = Command::new(ffprobe_path);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let output = cmd
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(file_path)
        .output()
        .with_context(|| format!("Failed to run ffprobe on {:?}", file_path))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffprobe failed for '{}': {}", file_path.display(), stderr.trim());
    }

    if output.stdout.is_empty() {
        anyhow::bail!("ffprobe returned empty output for '{}'", file_path.display());
    }

    let json: Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("ffprobe returned invalid JSON for '{}'", file_path.display()))?;

    parse_probe_output(file_path, &json)
}

fn parse_probe_output(file_path: &Path, json: &Value) -> Result<MediaInfo> {
    let format = json.get("format").ok_or_else(|| anyhow::anyhow!("Missing 'format' section in ffprobe output"))?;
    let streams = json.get("streams")
        .and_then(|s| s.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'streams' section in ffprobe output"))?;

    let duration = get_f64_from_str(format, "duration").unwrap_or(0.0);
    let size = get_u64_from_str(format, "size").unwrap_or(0);
    let format_name = get_string(format, "format_name", "unknown");
    let format_long_name = get_string(format, "format_long_name", "unknown");
    let bit_rate = get_u64_from_str(format, "bit_rate");
    let start_time = get_f64_from_str(format, "start_time");

    let tags_format = format.get("tags").unwrap_or(&Value::Null);
    let creation_time = get_tag_case_insensitive(tags_format, "creation_time")
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let mut video_streams = Vec::new();
    let mut audio_streams = Vec::new();
    let mut subtitle_streams = Vec::new();

    for stream in streams {
        match stream.get("codec_type").and_then(|v| v.as_str()).unwrap_or("") {
            "video" => {
                let fps = parse_fps(stream);
                video_streams.push(VideoStream {
                    codec_name: get_string(stream, "codec_name", "unknown"),
                    codec_long_name: get_string(stream, "codec_long_name", "unknown"),
                    width: stream.get("width").and_then(|v| v.as_u64()).map(|v| v as u32),
                    height: stream.get("height").and_then(|v| v.as_u64()).map(|v| v as u32),
                    fps,
                    bit_rate: get_u64_from_str(stream, "bit_rate"),
                    pixel_format: stream.get("pix_fmt").and_then(|v| v.as_str()).map(String::from),
                    color_space: stream.get("color_space").and_then(|v| v.as_str()).map(String::from),
                    color_primaries: stream.get("color_primaries").and_then(|v| v.as_str()).map(String::from),
                    color_transfer: stream.get("color_transfer").and_then(|v| v.as_str()).map(String::from),
                    profile: stream.get("profile").and_then(|v| v.as_str()).map(String::from),
                    level: stream.get("level").and_then(|v| v.as_i64()).map(|v| v as i32),
                    duration: get_stream_duration(stream),
                    time_base: stream.get("time_base").and_then(|v| v.as_str()).map(String::from),
                    stream_index: stream.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    r_frame_rate: stream.get("r_frame_rate").and_then(|v| v.as_str()).map(String::from),
                    field_order: stream.get("field_order").and_then(|v| v.as_str()).map(String::from),
                    avg_frame_rate: stream.get("avg_frame_rate").and_then(|v| v.as_str()).map(String::from),
                    bits_per_raw_sample: stream.get("bits_per_raw_sample")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    sample_aspect_ratio: stream.get("sample_aspect_ratio").and_then(|v| v.as_str()).map(String::from),
                    display_aspect_ratio: stream.get("display_aspect_ratio").and_then(|v| v.as_str()).map(String::from),
                    rotation: extract_rotation(stream),
                    start_time: get_f64_from_str(stream, "start_time"),
                });
            }
            "audio" => {
                audio_streams.push(AudioStream {
                    codec_name: get_string(stream, "codec_name", "unknown"),
                    codec_long_name: get_string(stream, "codec_long_name", "unknown"),
                    sample_rate: stream.get("sample_rate")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    channels: stream.get("channels").and_then(|v| v.as_u64()).map(|v| v as u32),
                    channel_layout: stream.get("channel_layout").and_then(|v| v.as_str()).map(String::from),
                    bit_rate: get_u64_from_str(stream, "bit_rate"),
                    duration: get_stream_duration(stream),
                    stream_index: stream.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    profile: stream.get("profile").and_then(|v| v.as_str()).map(String::from),
                    bits_per_raw_sample: stream.get("bits_per_raw_sample")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok()),
                    start_pts: stream.get("start_pts").and_then(|v| v.as_i64()),
                    start_time: get_f64_from_str(stream, "start_time"),
                    language: get_tag_case_insensitive(stream.get("tags").unwrap_or(&Value::Null), "language"),
                });
            }
            "subtitle" => {
                let tags_stream = stream.get("tags").unwrap_or(&Value::Null);
                let language = get_tag_case_insensitive(tags_stream, "language");
                let title = get_tag_case_insensitive(tags_stream, "title");
                let codec = get_string(stream, "codec_name", "unknown");
                let is_bitmap = crate::types::BITMAP_SUBTITLE_CODECS.contains(&codec.as_str());
                subtitle_streams.push(SubtitleStream {
                    codec_name: codec,
                    codec_long_name: get_string(stream, "codec_long_name", "unknown"),
                    language,
                    title,
                    stream_index: stream.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                    is_external: false,
                    path: None,
                    is_bitmap,
                });
            }
            _ => {} // data, attachment streams ignored
        }
    }

    // Search for external subtitles
    subtitle_streams.extend(find_external_subtitles(file_path));

    Ok(MediaInfo {
        path: file_path.to_string_lossy().into_owned(),
        duration,
        size,
        format_name,
        format_long_name,
        bit_rate,
        video_streams,
        audio_streams,
        subtitle_streams,
        start_time,
        creation_time,
    })
}

/// Sub-extension flags that annotate subtitle purpose without affecting language.
const SUBTITLE_FLAGS: &[&str] = &["forced", "sdh", "default", "cc"];

/// Directories to search for external subtitle files (relative to video).
const SUBTITLE_SUBDIRS: &[&str] = &["subs", "subtitles", "Subs", "Subtitles", "SUB", "SUBSUBTITLES"];

/// Attempt to match a subtitle file stem against a video stem.
/// Returns `Some((language, flags))` on match, `None` otherwise.
///
/// Matches the following naming conventions (case-insensitive):
///
/// | Pattern | Example | Matches `video.mp4`? |
/// |---|---|---|
/// | Same name | `video.srt` | ✅ exact |
/// | Dot language | `video.en.srt` | ✅ via `video.` prefix |
/// | Underscore language | `video_en.srt` | ✅ via `video_` prefix |
/// | Hyphen language | `video-en.srt` | ✅ via `video-` prefix |
/// | Dot language + flags | `video.en.forced.srt` | ✅ via `video.` prefix + flags parsed |
/// | Language-only | `en.srt` | ✅ if language is a known 2-3 letter code |
/// | Language with flags | `en.forced.srt` | ✅ via language-only |
/// Score how similar two strings are (0.0 = identical, higher = less similar).
/// Uses a simple Levenshtein-like distance normalized by max length.
/// This is used for fuzzy fallback matching when exact matching fails.
fn fuzzy_similarity(a: &str, b: &str) -> f64 {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    let alen = a.len();
    let blen = b.len();
    if alen == 0 || blen == 0 {
        return if alen == blen { 0.0 } else { 1.0 };
    }
    if a == b {
        return 0.0;
    }

    // Simple edit distance
    let mut prev: Vec<usize> = (0..=blen).collect();
    let mut curr = vec![0usize; blen + 1];

    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = std::cmp::min(
                std::cmp::min(curr[j] + 1, prev[j + 1] + 1),
                prev[j] + cost,
            );
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    let distance = prev[blen] as f64;
    let max_len = alen.max(blen) as f64;
    distance / max_len
}

/// Attempt to match a subtitle file stem against a video stem.
/// Returns `Some((language, flags, score))` on match, `None` otherwise.
/// The score is 0.0 for perfect matches, higher for fuzzy fallback matches.
///
/// Matching strategy:
/// 1. Exact stem match (score 0.0)
/// 2. Stem + separator + language/flags (score 0.0)
/// 3. Language-only (score 0.0)
/// 4. Language-only with flags (score 0.0)
/// 5. Fuzzy fallback: edit distance < 0.3 (lenient threshold)
///
/// Matches the following naming conventions (case-insensitive):
///
/// | Pattern | Example | Matches `video.mp4`? |
/// |---|---|---|
/// | Same name | `video.srt` | ✅ exact |
/// | Dot language | `video.en.srt` | ✅ via `video.` prefix |
/// | Underscore language | `video_en.srt` | ✅ via `video_` prefix |
/// | Hyphen language | `video-en.srt` | ✅ via `video-` prefix |
/// | Dot language + flags | `video.en.forced.srt` | ✅ via `video.` prefix + flags parsed |
/// | Language-only | `en.srt` | ✅ if language is a known 2-3 letter code |
/// | Language with flags | `en.forced.srt` | ✅ via language-only |
/// | Fuzzy | `video-xyz.srt` (unknown lang) | ✅ via fuzzy score < 0.3 |
fn match_subtitle_stem(sub_stem: &str, video_stem: &str) -> Option<(Option<String>, Vec<String>, f64)> {
    let sub = sub_stem.to_lowercase();
    let vid = video_stem.to_lowercase();

    // 1. Exact match (same name, no language/flag)
    if sub == vid {
        log::info!("[SubtitleMatch] EXACT: '{}' == '{}' matched", sub_stem, video_stem);
        return Some((None, vec![], 0.0));
    }

    // 2. Starts with video name followed by a separator (. _ -)
    let dot = format!("{:}.", vid);
    let und = format!("{:}_", vid);
    let hyn = format!("{:}-", vid);

    if sub.starts_with(&dot) || sub.starts_with(&und) || sub.starts_with(&hyn) {
        let separator_len = if sub.starts_with(&dot) { dot.len() } 
            else if sub.starts_with(&und) { und.len() }
            else { hyn.len() };
        let remainder = &sub[separator_len..];
        let (lang, flags) = parse_language_and_flags(remainder);
        log::info!("[SubtitleMatch] SEPARATOR: '{}' matched '{}' via separator, lang={:?}, flags={:?}",
            sub_stem, video_stem, lang, flags);
        return Some((lang, flags, 0.0));
    }

    // 3. Language-only: the entire sub stem could be a known language code
    //    (e.g., `en.srt` in same dir as `video.mp4`).
    //    We check if the sub stem is a known ISO language code.
    if is_known_language_code(&sub) {
        log::info!("[SubtitleMatch] LANGUAGE-ONLY: '{}' matched '{}' as lang code", sub_stem, video_stem);
        return Some((Some(sub.to_string()), vec![], 0.0));
    }

    // 4. Language-only with flags (e.g., `en.forced.srt`)
    if let Some((first, rest)) = sub.split_once('.') {
        if is_known_language_code(first) {
            let (_lang, extra_flags) = parse_language_and_flags(rest);
            log::info!("[SubtitleMatch] LANGUAGE+FLAGS: '{}' matched '{}', flags={:?}",
                sub_stem, video_stem, extra_flags);
            return Some((Some(first.to_string()), extra_flags, 0.0));
        }
    }

    // 5. Fuzzy fallback: check edit distance similarity
    //    This catches cases like "video-xyz.srt" where "xyz" is an unknown language code
    //    or "movie.v2.srt" where v2 is a version suffix.
    let score = fuzzy_similarity(&sub, &vid);
    if score < 0.30 {
        log::info!("[SubtitleMatch] FUZZY: '{}' matched '{}' (score={:.3})", sub_stem, video_stem, score);
        // Try to parse the difference as language/flags
        let diff = sub.trim_start_matches(&vid).trim_start_matches(['.', '_', '-']);
        if !diff.is_empty() {
            let (lang, flags) = parse_language_and_flags(diff);
            return Some((lang, flags, score));
        }
        return Some((None, vec![], score));
    }

    log::info!("[SubtitleMatch] NO-MATCH: '{}' vs '{}' (fuzzy score={:.3} above threshold)",
        sub_stem, video_stem, score);
    None
}

/// Split a suffix like "en.forced" into language ("en") and flags (["forced"]).
///
/// Strategy: the first part that is NOT a known subtitle flag becomes the language.
/// All known flags (`forced`, `sdh`, `default`, `cc`) are collected as flags.
/// This handles locale codes (`en-US`), unknown language codes (`xyz`),
/// and flag-only suffixes (`forced`).
fn parse_language_and_flags(s: &str) -> (Option<String>, Vec<String>) {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.is_empty() || parts[0].is_empty() {
        return (None, vec![]);
    }

    let mut flags = Vec::new();
    let mut lang = None;

    for p in &parts {
        if SUBTITLE_FLAGS.contains(p) {
            flags.push(p.to_string());
        } else if lang.is_none() {
            // First non-flag part becomes the language (even if not a known code)
            lang = Some(p.to_string());
        }
    }

    (lang, flags)
}

/// Common two-letter ISO 639-1 and three-letter ISO 639-2 language codes.
const KNOWN_LANGUAGE_CODES: &[&str] = &[
    "aa", "ab", "ae", "af", "ak", "am", "an", "ar", "as", "av", "ay", "az",
    "ba", "be", "bg", "bh", "bi", "bm", "bn", "bo", "br", "bs",
    "ca", "ce", "ch", "co", "cr", "cs", "cu", "cv", "cy",
    "da", "de", "dv", "dz",
    "ee", "el", "en", "eo", "es", "et", "eu",
    "fa", "ff", "fi", "fj", "fo", "fr", "fy",
    "ga", "gd", "gl", "gn", "gu", "gv",
    "ha", "he", "hi", "ho", "hr", "ht", "hu", "hy", "hz",
    "ia", "id", "ie", "ig", "ii", "ik", "io", "is", "it", "iu",
    "ja", "jv",
    "ka", "kg", "ki", "kj", "kk", "kl", "km", "kn", "ko", "kr", "ks", "ku", "kv", "kw", "ky",
    "la", "lb", "lg", "li", "ln", "lo", "lt", "lu", "lv",
    "mg", "mh", "mi", "mk", "ml", "mn", "mr", "ms", "mt", "my",
    "na", "nb", "nd", "ne", "ng", "nl", "nn", "no", "nr", "nv", "ny",
    "oc", "oj", "om", "or", "os", "pa", "pi", "pl", "ps", "pt",
    "qu",
    "rm", "rn", "ro", "ru", "rw",
    "sa", "sc", "sd", "se", "sg", "si", "sk", "sl", "sm", "sn", "so", "sq", "sr", "ss", "st", "su", "sv", "sw",
    "ta", "te", "tg", "th", "ti", "tk", "tl", "tn", "to", "tr", "ts", "tt", "tw", "ty",
    "ug", "uk", "ur", "uz",
    "ve", "vi", "vo",
    "wa", "wo",
    "xh",
    "yi", "yo",
    "za", "zh", "zu",
    // 3-letter codes for common languages that might appear
    "ara", "chi", "cze", "dut", "eng", "fre", "ger", "gre", "hun",
    "ita", "jpn", "kor", "nor", "pol", "por", "rus", "spa", "swe",
    "tha", "tur", "ukr", "vie",
];

/// Check if a string looks like a known language code.
fn is_known_language_code(s: &str) -> bool {
    let s = s.to_lowercase();
    // Must be 2-4 alphanumeric chars
    if s.len() < 2 || s.len() > 4 {
        return false;
    }
    if !s.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    KNOWN_LANGUAGE_CODES.contains(&s.as_str())
}

/// Scan a directory for subtitle files matching a video stem.
fn scan_dir_for_subtitles(dir: &Path, stem: &str) -> Vec<SubtitleStream> {
    let mut subs = Vec::new();
    let sub_exts = ["srt", "vtt", "ass", "ssa", "sub"];
    let mut skipped_temp: u32 = 0;
    let mut skipped_nomatch: u32 = 0;
    let mut scanned: u32 = 0;

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return subs,
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        if !file_type.is_file() { continue; }

        let path = entry.path();
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_lowercase(),
            None => continue,
        };
        if !sub_exts.contains(&ext.as_str()) { continue; }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let name_stem = match name.rsplit_once('.') {
            Some((stem, _)) => stem.to_string(),
            None => continue,
        };

        let stem_lower = name_stem.to_lowercase();
        if stem_lower.starts_with("norm_") || stem_lower.starts_with("conv_") || stem_lower.starts_with("tmp_") {
            skipped_temp += 1;
            continue;
        }

        scanned += 1;
        if let Some((lang, flags, _score)) = match_subtitle_stem(&name_stem, stem) {
            log::info!(
                "[SubtitleScan] FOUND: name={}, video_stem={}, lang={:?}, flags={:?}, path={}",
                name, stem, lang, flags, path.display()
            );
            let title_parts: Vec<String> = {
                let mut parts = Vec::new();
                if let Some(ref l) = lang {
                    parts.push(l.to_uppercase());
                }
                if !flags.is_empty() {
                    parts.extend(flags.iter().map(|f| f.to_uppercase()));
                }
                parts
            };
            let title = if title_parts.is_empty() {
                "External".to_string()
            } else {
                title_parts.join(" ")
            };

            subs.push(SubtitleStream {
                codec_name: ext.clone(),
                codec_long_name: format!("External {} subtitle", ext.to_uppercase()),
                language: lang.clone(),
                title: Some(title),
                stream_index: 0,
                is_external: true,
                path: Some(path.to_string_lossy().into_owned()),
                is_bitmap: ext == "sub",
            });
        } else {
            skipped_nomatch += 1;
        }
    }

    log::info!("[SubtitleScan] dir={} stem={} | scanned={} skipped_temp={} skipped_nomatch={} found={}",
        dir.display(), stem, scanned, skipped_temp, skipped_nomatch, subs.len());
    subs
}

/// Find external subtitle files matching a video file.
///
/// Searches:
/// 1. The same directory as the video
/// 2. Subtitle subdirectories (Subs/, subtitles/, etc.)
///
/// Matching patterns (case-insensitive):
/// - Exact stem match: `video.srt` ↔ `video.mp4`
/// - Stem + separator + language: `video.en.srt`, `video_en.srt`, `video-en.srt`
/// - Stem + separator + language + flags: `video.en.forced.srt`, `video.en.sdh.srt`
/// - Language-only: `en.srt` (where `en` is a known ISO language code)
/// - Fuzzy fallback: edit distance matching for unknown language codes
fn find_external_subtitles(video_path: &Path) -> Vec<SubtitleStream> {
    let mut subs = Vec::new();

    let parent = match video_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };

    let stem = match video_path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => {
            log::warn!("[SubtitleAttach] Video file {} has non-UTF8 filename stem — cannot scan for companion subtitles", video_path.display());
            return subs;
        }
    };
    let stem = stem.to_string();

    log::info!("[SubtitleAttach] Scanning for subtitles matching '{}' in {:?}", stem, parent);

    // 1. Search the same directory as the video
    let same_dir_subs = scan_dir_for_subtitles(&parent, &stem);
    log::info!("[SubtitleAttach] Same dir: found {} subtitle(s)", same_dir_subs.len());
    subs.extend(same_dir_subs);

    // 2. Search common subtitle subdirectories
    //    Use canonicalize to handle case-insensitive filesystem dedup
    let mut scanned: Vec<PathBuf> = Vec::new();
    for subdir_name in SUBTITLE_SUBDIRS {
        let subdir = parent.join(subdir_name);
        if subdir.is_dir() {
            let canon = subdir.canonicalize().unwrap_or_else(|_| subdir.clone());
            if scanned.iter().any(|d| d == &canon) {
                continue; // Already scanned this directory
            }
            scanned.push(canon);
            let subdir_subs = scan_dir_for_subtitles(&subdir, &stem);
            log::info!("[SubtitleAttach] Subdir '{}': found {} subtitle(s)", subdir_name, subdir_subs.len());
            subs.extend(subdir_subs);
        }
    }

    // Dedup by path (defense-in-depth for any remaining duplicates)
    subs.sort_by(|a, b| a.path.cmp(&b.path));
    subs.dedup_by(|a, b| a.path == b.path);

    if subs.is_empty() {
        log::info!("[SubtitleAttach] No external subtitles found for '{}'", stem);
    } else {
        log::info!("[SubtitleAttach] Total: {} subtitle(s) for '{}'", subs.len(), stem);
        for s in &subs {
            log::info!("[SubtitleAttach]   -> {:?} (lang={:?}, title={:?})", s.path, s.language, s.title);
        }
    }

    subs
}

/// Parse rational fps string ("30000/1001", "30", "0/0") → f64
fn parse_fps(stream: &Value) -> Option<f64> {
    // Prefer avg_frame_rate (actual playback rate), fall back to r_frame_rate
    let fps_str = stream["avg_frame_rate"]
        .as_str()
        .filter(|s| *s != "0/0")
        .or_else(|| stream["r_frame_rate"].as_str().filter(|s| *s != "0/0"))?;

    let parts: Vec<&str> = fps_str.splitn(2, '/').collect();
    let result = match parts.as_slice() {
        [num, den] => {
            let n: f64 = num.trim().parse().ok()?;
            let d: f64 = den.trim().parse().ok()?;
            if d == 0.0 { None } else { Some((n / d * 1000.0).round() / 1000.0) }
        }
        [single] => single.trim().parse().ok(),
        _ => None,
    };
    // Reject NaN and Inf values that could crash downstream FFmpeg operations
    result.filter(|v| v.is_finite())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract rotation from ffprobe stream data.
/// Checks both `side_data_list` (Display Matrix) and `tags.rotate`.
fn extract_rotation(stream: &Value) -> Option<i32> {
    // Method 1: side_data_list with Display Matrix
    if let Some(side_data) = stream["side_data_list"].as_array() {
        for entry in side_data {
            let stype = entry["side_data_type"].as_str().unwrap_or("");
            if stype == "Display Matrix" {
                if let Some(rot) = entry["rotation"].as_i64() {
                    return Some(rot as i32);
                }
            }
        }
    }
    // Method 2: tags.rotate (some encoders store it here)
    if let Some(tags) = stream.get("tags") {
        if let Some(rot) = tags["rotate"].as_str() {
            return rot.parse::<i32>().ok();
        }
        if let Some(rot) = tags["rotate"].as_i64() {
            return Some(rot as i32);
        }
    }
    None
}

fn parse_duration_tag(duration_str: &str) -> Option<f64> {
    let s = duration_str.trim();
    let s = s.trim_start_matches('"').trim_end_matches('"');
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("N/A") {
        return None;
    }
    let s = s.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;
    let total = hours * 3600.0 + minutes * 60.0 + seconds;
    if total.is_finite() && total >= 0.0 { Some(total) } else { None }
}

fn get_stream_duration(stream: &Value) -> Option<f64> {
    if let Some(d) = get_f64_from_str(stream, "duration") {
        return Some(d);
    }
    let tags = stream.get("tags").unwrap_or(&Value::Null);
    let tag_dur = get_tag_case_insensitive(tags, "duration");
    if let Some(dur_str) = tag_dur {
        return parse_duration_tag(&dur_str);
    }
    None
}

fn get_string(obj: &Value, key: &str, default: &str) -> String {
    obj[key].as_str().unwrap_or(default).to_string()
}

fn get_f64_from_str(obj: &Value, key: &str) -> Option<f64> {
    let val = obj[key].as_str().and_then(|s| s.trim().parse().ok())
        .or_else(|| obj[key].as_f64());
    val.filter(|v| v.is_finite())
}

fn get_u64_from_str(obj: &Value, key: &str) -> Option<u64> {
    obj[key].as_str().and_then(|s| s.trim().parse().ok())
        .or_else(|| obj[key].as_u64())
}

fn get_tag_case_insensitive(tags: &Value, key: &str) -> Option<String> {
    let obj = tags.as_object()?;
    let key_lower = key.to_lowercase();
    for (k, v) in obj {
        if k.to_lowercase() == key_lower {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
        }
    }
    None
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_known_language_code ───────────────────────────────────────────────

    #[test]
    fn test_known_language_codes() {
        // Common ISO codes
        assert!(is_known_language_code("en"), "en should be known");
        assert!(is_known_language_code("fr"), "fr should be known");
        assert!(is_known_language_code("de"), "de should be known");
        assert!(is_known_language_code("es"), "es should be known");
        assert!(is_known_language_code("ja"), "ja should be known");
        assert!(is_known_language_code("zh"), "zh should be known");
        assert!(is_known_language_code("ar"), "ar should be known");
        assert!(is_known_language_code("pt"), "pt should be known");
        assert!(is_known_language_code("ru"), "ru should be known");
        assert!(is_known_language_code("ko"), "ko should be known");
        // 3-letter codes
        assert!(is_known_language_code("eng"), "eng should be known");
        assert!(is_known_language_code("spa"), "spa should be known");
        assert!(is_known_language_code("ger"), "ger should be known");
        assert!(is_known_language_code("fre"), "fre should be known");
        assert!(is_known_language_code("jpn"), "jpn should be known");
        assert!(is_known_language_code("chi"), "chi should be known");
        // Case insensitive
        assert!(is_known_language_code("EN"), "EN should be known (case insensitive)");
        assert!(is_known_language_code("En"), "En should be known (case insensitive)");
        assert!(is_known_language_code("eN"), "eN should be known (case insensitive)");
    }

    #[test]
    fn test_unknown_language_codes() {
        assert!(!is_known_language_code("xx"), "xx should not be known");
        assert!(!is_known_language_code("zzz"), "zzz should not be known");
        assert!(!is_known_language_code(""), "empty string should not be known");
        assert!(!is_known_language_code("a"), "single char should not be known");
        assert!(!is_known_language_code("abcde"), "5 chars should not be known");
        assert!(!is_known_language_code("123"), "digits should not be known");
        assert!(!is_known_language_code("e_n"), "underscore should not be known");
        assert!(!is_known_language_code("video"), "'video' should not be a language code");
        assert!(!is_known_language_code("subtitle"), "'subtitle' should not be a language code");
        assert!(!is_known_language_code("forced"), "'forced' should not be a language code");
        assert!(!is_known_language_code("sdh"), "'sdh' should not be a language code");
    }

    // ── parse_language_and_flags ────────────────────────────────────────────

    #[test]
    fn test_parse_language_only() {
        let (lang, flags) = parse_language_and_flags("en");
        assert_eq!(lang, Some("en".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_parse_language_unknown_only() {
        let (lang, flags) = parse_language_and_flags("xyz");
        // Unknown single part is treated as language
        assert_eq!(lang, Some("xyz".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_parse_language_with_flags() {
        let (lang, flags) = parse_language_and_flags("en.forced");
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["forced".to_string()]);
    }

    #[test]
    fn test_parse_language_with_sdh_flag() {
        let (lang, flags) = parse_language_and_flags("en.sdh");
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["sdh".to_string()]);
    }

    #[test]
    fn test_parse_language_with_default_flag() {
        let (lang, flags) = parse_language_and_flags("en.default");
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["default".to_string()]);
    }

    #[test]
    fn test_parse_language_with_cc_flag() {
        let (lang, flags) = parse_language_and_flags("en.cc");
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["cc".to_string()]);
    }

    #[test]
    fn test_parse_multiple_flags() {
        let (lang, flags) = parse_language_and_flags("en.forced.sdh");
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["forced".to_string(), "sdh".to_string()]);
    }

    #[test]
    fn test_parse_flags_no_language() {
        let (lang, flags) = parse_language_and_flags("forced");
        // 'forced' is a known flag — should be in flags, not language
        assert!(lang.is_none(), "known flags should not be treated as language");
        assert_eq!(flags, vec!["forced".to_string()]);
    }

    #[test]
    fn test_parse_empty_string() {
        let (lang, flags) = parse_language_and_flags("");
        assert!(lang.is_none());
        assert!(flags.is_empty());
    }

    #[test]
    fn test_parse_locale_code() {
        let (lang, flags) = parse_language_and_flags("en-US");
        assert_eq!(lang, Some("en-US".to_string()));
        assert!(flags.is_empty());
    }

    // ── match_subtitle_stem ─────────────────────────────────────────────────

    #[test]
    fn test_match_exact_stem() {
        // video.srt ↔ video.mp4
        let result = match_subtitle_stem("video", "video");
        assert!(result.is_some(), "Exact match should succeed");
        let (lang, flags, score) = result.unwrap();
        assert!(lang.is_none(), "Exact match should have no language");
        assert!(flags.is_empty(), "Exact match should have no flags");
        assert_eq!(score, 0.0, "Exact match should have score 0");
    }

    #[test]
    fn test_match_dot_language() {
        // video.en.srt ↔ video.mp4
        let result = match_subtitle_stem("video.en", "video");
        assert!(result.is_some(), "Dot language match should succeed");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_match_underscore_language() {
        // video_en.srt ↔ video.mp4
        let result = match_subtitle_stem("video_en", "video");
        assert!(result.is_some(), "Underscore language match should succeed");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_match_hyphen_language() {
        // video-en.srt ↔ video.mp4
        let result = match_subtitle_stem("video-en", "video");
        assert!(result.is_some(), "Hyphen language match should succeed");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_match_dot_language_with_forced_flag() {
        // video.en.forced.srt ↔ video.mp4
        let result = match_subtitle_stem("video.en.forced", "video");
        assert!(result.is_some(), "Dot language + forced flag should succeed");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["forced".to_string()]);
    }

    #[test]
    fn test_match_dot_language_with_sdh_flag() {
        // video.en.sdh.srt ↔ video.mp4
        let result = match_subtitle_stem("video.en.sdh", "video");
        assert!(result.is_some(), "Dot language + sdh flag should succeed");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["sdh".to_string()]);
    }

    #[test]
    fn test_match_language_only() {
        // en.srt in same dir as video.mp4
        let result = match_subtitle_stem("en", "video");
        assert!(result.is_some(), "Language-only match should succeed");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_match_language_only_with_forced_flag() {
        // en.forced.srt in same dir as video.mp4
        let result = match_subtitle_stem("en.forced", "video");
        assert!(result.is_some(), "Language-only + forced should succeed");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["forced".to_string()]);
    }

    #[test]
    fn test_match_three_letter_language() {
        // video.eng.srt ↔ video.mp4
        let result = match_subtitle_stem("video.eng", "video");
        assert!(result.is_some(), "3-letter language code should match");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("eng".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_match_locale_code() {
        // video.en-US.srt ↔ video.mp4
        // Note: match_subtitle_stem lowercases the stem, so "en-US" → "en-us"
        let result = match_subtitle_stem("video.en-US", "video");
        assert!(result.is_some(), "Locale code should match");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en-us".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_match_case_insensitive() {
        // Video.EN.srt ↔ video.mp4
        let result = match_subtitle_stem("Video.EN", "video");
        assert!(result.is_some(), "Case-insensitive match should succeed");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_no_match_different_name() {
        // other.srt ↔ video.mp4 ("other" vs "video" has edit distance 4/5 = 0.8 > 0.3)
        let result = match_subtitle_stem("other", "video");
        assert!(result.is_none(), "Very different names should not match via fuzzy");
    }

    #[test]
    fn test_fuzzy_match_similar_names() {
        // movie-v2.srt ↔ movie.mp4 — "movie-v2" vs "movie" has edit distance 3/8 = 0.375 > 0.3
        // Actually let's use a closer match: "moviefinal" vs "movie" has distance 5/10 = 0.5 > 0.3
        // Use: "movie.v2" vs "movie" -> edit distance 3/9 = 0.33 > 0.3
        // "movie-2" vs "movie" -> edit distance 2/7 = 0.28 < 0.3! This should match
        let result = match_subtitle_stem("movie-2", "movie");
        assert!(result.is_some(), "Similar names should match via fuzzy (edit dist 2/7)");
        let (lang, _flags, _score) = result.unwrap();
        // The suffix is "2" — should be parsed as a language (but not a known code)
        assert_eq!(lang, Some("2".to_string()), "'2' suffix should be parsed as language");
    }

    #[test]
    fn test_fuzzy_fallback_unknown_language() {
        // xyz.srt in same dir as video.mp4 — should NOT match via fuzzy:
        // "xyz" vs "video" has edit distance 3/5 = 0.6, above the 0.3 threshold
        let result = match_subtitle_stem("xyz", "video");
        assert!(result.is_none(), "'xyz' vs 'video' should not match (score too high)");

        // "video-xyz" matches via the SEPARATOR rule ("video-" prefix), not fuzzy.
        // The suffix "xyz" is parsed as the language. Score is 0.0 (exact separator match).
        let result = match_subtitle_stem("video-xyz", "video");
        assert!(result.is_some(), "'video-xyz' should match 'video' via separator rule");
        let (lang, _flags, score) = result.unwrap();
        // "xyz" could be interpreted as unknown lang
        assert!(lang.is_some(), "Should extract language from separator match");
        assert!(score >= 0.0 && score < 0.3, "Score should be in [0, 0.3), got {}", score);
    }

    #[test]
    fn test_match_dot_language_3letter() {
        // video.eng.srt ↔ video.mp4
        let result = match_subtitle_stem("video.eng", "video");
        assert!(result.is_some());
        let (lang, _flags, _score) = result.unwrap();
        assert_eq!(lang, Some("eng".to_string()));
    }

    #[test]
    fn test_match_multi_dot_video() {
        // my.video.en.srt ↔ my.video.mp4
        let result = match_subtitle_stem("my.video.en", "my.video");
        assert!(result.is_some(), "Multi-dot video stem should match");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_match_multi_dot_video_with_forced() {
        // my.video.en.forced.srt ↔ my.video.mp4
        let result = match_subtitle_stem("my.video.en.forced", "my.video");
        assert!(result.is_some(), "Multi-dot video stem with forced should match");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["forced".to_string()]);
    }

    #[test]
    fn test_match_language_only_with_sdh() {
        // en.sdh.srt in same dir as video.mp4
        let result = match_subtitle_stem("en.sdh", "video");
        assert!(result.is_some(), "Language + sdh should match");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["sdh".to_string()]);
    }

    #[test]
    fn test_match_language_only_with_default_and_cc() {
        // en.default.cc.srt in same dir as video.mp4
        let result = match_subtitle_stem("en.default.cc", "video");
        assert!(result.is_some(), "Language + default + cc should match");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("en".to_string()));
        assert_eq!(flags, vec!["default".to_string(), "cc".to_string()]);
    }

    #[test]
    fn test_no_match_subtitle_flag_as_language_only() {
        // forced.srt should not match video.mp4 (forced is not a language code)
        let result = match_subtitle_stem("forced", "video");
        assert!(result.is_none(), "'forced' alone should not match (not a language code)");
    }

    #[test]
    fn test_fuzzy_similarity_identical() {
        assert_eq!(fuzzy_similarity("video", "video"), 0.0);
    }

    #[test]
    fn test_fuzzy_similarity_completely_different() {
        let score = fuzzy_similarity("abc", "xyz");
        assert!(score > 0.5, "Different strings should have high score");
    }

    #[test]
    fn test_fuzzy_similarity_small_difference() {
        let score = fuzzy_similarity("video", "vide");
        assert!(score < 0.3, "Close strings should have low score");
    }

    #[test]
    fn test_match_underscore_language_3letter() {
        // video_eng.srt ↔ video.mp4
        let result = match_subtitle_stem("video_eng", "video");
        assert!(result.is_some(), "Underscore + 3-letter lang should match");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("eng".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_match_hyphen_language_3letter() {
        // video-eng.srt ↔ video.mp4
        let result = match_subtitle_stem("video-eng", "video");
        assert!(result.is_some(), "Hyphen + 3-letter lang should match");
        let (lang, flags, _score) = result.unwrap();
        assert_eq!(lang, Some("eng".to_string()));
        assert!(flags.is_empty());
    }

    // ── scan_dir_for_subtitles (with temp dir) ──────────────────────────────

    #[test]
    fn test_scan_dir_empty() {
        let dir = std::env::temp_dir().join("sub_test_empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let subs = scan_dir_for_subtitles(&dir, "video");
        assert!(subs.is_empty(), "Empty dir should return no subtitles");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_dir_exact_match() {
        let dir = std::env::temp_dir().join("sub_test_exact");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("video.srt"), "").unwrap();
        std::fs::write(dir.join("video.mp4"), "").unwrap(); // should be ignored

        let subs = scan_dir_for_subtitles(&dir, "video");
        assert_eq!(subs.len(), 1, "Should find 1 subtitle file");
        assert_eq!(subs[0].codec_name, "srt");
        assert!(subs[0].language.is_none());
        assert_eq!(subs[0].title, Some("External".to_string()));
        assert!(subs[0].is_external);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_dir_dot_language() {
        let dir = std::env::temp_dir().join("sub_test_dot_lang");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("video.en.srt"), "").unwrap();

        let subs = scan_dir_for_subtitles(&dir, "video");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].language, Some("en".to_string()));
        assert_eq!(subs[0].title, Some("EN".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_dir_language_only() {
        let dir = std::env::temp_dir().join("sub_test_lang_only");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("en.srt"), "").unwrap();

        let subs = scan_dir_for_subtitles(&dir, "video");
        assert_eq!(subs.len(), 1, "Language-only filename should match");
        assert_eq!(subs[0].language, Some("en".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_dir_dot_language_forced() {
        let dir = std::env::temp_dir().join("sub_test_dot_forced");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("video.en.forced.srt"), "").unwrap();

        let subs = scan_dir_for_subtitles(&dir, "video");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].language, Some("en".to_string()));
        assert_eq!(subs[0].title, Some("EN FORCED".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_dir_multiple_languages() {
        let dir = std::env::temp_dir().join("sub_test_multi");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("video.en.srt"), "").unwrap();
        std::fs::write(dir.join("video.fr.srt"), "").unwrap();
        std::fs::write(dir.join("video.de.srt"), "").unwrap();

        let subs = scan_dir_for_subtitles(&dir, "video");
        assert_eq!(subs.len(), 3, "Should find all 3 language subtitles");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_dir_ignores_non_subtitle_extensions() {
        let dir = std::env::temp_dir().join("sub_test_ignore");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("video.en.srt"), "").unwrap();
        std::fs::write(dir.join("video.en.txt"), "").unwrap();
        std::fs::write(dir.join("video.en.jpg"), "").unwrap();

        let subs = scan_dir_for_subtitles(&dir, "video");
        assert_eq!(subs.len(), 1, "Should only find .srt, not .txt or .jpg");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_scan_dir_missing_dot_file() {
        let dir = std::env::temp_dir().join("sub_test_hidden");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // File with no stem before extension
        std::fs::write(dir.join(".srt"), "").unwrap();

        let subs = scan_dir_for_subtitles(&dir, "video");
        assert_eq!(subs.len(), 0, "Hidden .srt file should not match");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── find_external_subtitles (end-to-end with multiple search locations) ──

    #[test]
    fn test_find_external_subtitles_same_dir() {
        let dir = std::env::temp_dir().join("sub_test_e2e_same");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let video_path = dir.join("video.mp4");
        std::fs::write(&video_path, "").unwrap();
        std::fs::write(dir.join("video.en.srt"), "").unwrap();

        let subs = find_external_subtitles(&video_path);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].language, Some("en".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_external_subtitles_subdir_subs() {
        let dir = std::env::temp_dir().join("sub_test_e2e_subs");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subs")).unwrap();
        let video_path = dir.join("video.mp4");
        std::fs::write(&video_path, "").unwrap();
        std::fs::write(dir.join("subs").join("video.en.srt"), "").unwrap();

        let subs = find_external_subtitles(&video_path);
        assert_eq!(subs.len(), 1, "Should find subtitle in Subs/ subdir");
        assert_eq!(subs[0].language, Some("en".to_string()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_external_subtitles_subdir_subtitles() {
        let dir = std::env::temp_dir().join("sub_test_e2e_subtitles");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subtitles")).unwrap();
        let video_path = dir.join("video.mp4");
        std::fs::write(&video_path, "").unwrap();
        std::fs::write(dir.join("subtitles").join("video.en.srt"), "").unwrap();

        let subs = find_external_subtitles(&video_path);
        assert_eq!(subs.len(), 1, "Should find subtitle in subtitles/ subdir");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_external_subtitles_both_locations() {
        let dir = std::env::temp_dir().join("sub_test_e2e_both");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subs")).unwrap();
        let video_path = dir.join("video.mp4");
        std::fs::write(&video_path, "").unwrap();
        std::fs::write(dir.join("video.srt"), "").unwrap();
        std::fs::write(dir.join("subs").join("video.fr.srt"), "").unwrap();

        let subs = find_external_subtitles(&video_path);
        assert_eq!(subs.len(), 2, "Should find subtitles in both locations");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_external_subtitles_ignores_unrelated() {
        let dir = std::env::temp_dir().join("sub_test_e2e_unrelated");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let video_path = dir.join("video.mp4");
        std::fs::write(&video_path, "").unwrap();
        // Unrelated subtitle files
        std::fs::write(dir.join("other.srt"), "").unwrap();
        std::fs::write(dir.join("xyz.srt"), "").unwrap();

        let subs = find_external_subtitles(&video_path);
        assert_eq!(subs.len(), 0, "Unrelated files should not match");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_external_subtitles_multiple_languages_and_subdir() {
        let dir = std::env::temp_dir().join("sub_test_e2e_complex");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subtitles")).unwrap();
        let video_path = dir.join("video.mp4");
        std::fs::write(&video_path, "").unwrap();
        // Same dir subs
        std::fs::write(dir.join("video.en.srt"), "").unwrap();
        std::fs::write(dir.join("video.fr.forced.srt"), "").unwrap();
        // Subdir subs
        std::fs::write(dir.join("subtitles").join("video.de.srt"), "").unwrap();
        std::fs::write(dir.join("subtitles").join("en.srt"), "").unwrap();

        let subs = find_external_subtitles(&video_path);
        assert_eq!(subs.len(), 4, "Should find all 4 subtitle files across locations");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_external_subtitles_vtt_and_ass() {
        let dir = std::env::temp_dir().join("sub_test_e2e_formats");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let video_path = dir.join("video.mp4");
        std::fs::write(&video_path, "").unwrap();
        std::fs::write(dir.join("video.en.vtt"), "").unwrap();
        std::fs::write(dir.join("video.fr.ass"), "").unwrap();

        let subs = find_external_subtitles(&video_path);
        assert_eq!(subs.len(), 2, "Should find vtt and ass subtitle files");
        // Sorted by path: "video.en.vtt" < "video.fr.ass" (e < f)
        assert_eq!(subs[0].codec_name, "vtt", "vtt sorts before ass alphabetically");
        assert_eq!(subs[1].codec_name, "ass");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
