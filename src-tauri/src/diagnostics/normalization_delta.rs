use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizationDelta {
    pub original_path: String,
    pub normalized_path: String,
    pub stream_count: StreamCountDelta,
    pub video_streams: Vec<StreamDelta>,
    pub audio_streams: Vec<StreamDelta>,
    pub subtitle_streams: Vec<StreamDelta>,
    pub format_changes: Vec<FormatChange>,
    pub has_pts_changes: bool,
    pub has_dts_changes: bool,
    pub has_timebase_changes: bool,
    pub has_start_time_changes: bool,
    pub verdict: DeltaVerdict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamCountDelta {
    pub original: usize,
    pub normalized: usize,
    pub video_changed: bool,
    pub audio_changed: bool,
    pub subtitle_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamDelta {
    pub stream_index: usize,
    pub stream_type: StreamType,
    pub codec_name: Option<StringDelta>,
    pub profile: Option<StringDelta>,
    pub sample_rate: Option<StringDelta>,
    pub channels: Option<StringDelta>,
    pub time_base: Option<StringDelta>,
    pub start_time: Option<FloatDelta>,
    pub duration: Option<FloatDelta>,
    pub bit_rate: Option<StringDelta>,
    pub language: Option<StringDelta>,
    pub disposition: Option<StringDelta>,
    pub has_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamType {
    Video,
    Audio,
    Subtitle,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringDelta {
    pub original: Option<String>,
    pub normalized: Option<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatDelta {
    pub original: f64,
    pub normalized: f64,
    pub difference: f64,
    pub percent_change: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatChange {
    pub field: String,
    pub original: String,
    pub normalized: String,
    pub severity: ChangeSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeSeverity {
    Critical,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeltaVerdict {
    Identical,
    Safe,
    Suspicious,
    Dangerous,
}

impl std::fmt::Display for DeltaVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeltaVerdict::Identical => write!(f, "IDENTICAL (no changes)"),
            DeltaVerdict::Safe => write!(f, "SAFE (expected audio normalization)"),
            DeltaVerdict::Suspicious => write!(f, "SUSPICIOUS (verify manually)"),
            DeltaVerdict::Dangerous => write!(f, "DANGEROUS (may cause concat failures)"),
        }
    }
}

pub fn audit_normalization_delta(
    ffprobe_path: &Path,
    original_path: &Path,
    normalized_path: &Path,
) -> Result<NormalizationDelta, String> {
    let original_info = probe_file(ffprobe_path, original_path)?;
    let normalized_info = probe_file(ffprobe_path, normalized_path)?;

    let stream_count = compare_stream_counts(&original_info, &normalized_info);

    let video_streams = compare_video_streams(&original_info.video_streams, &normalized_info.video_streams);
    let audio_streams = compare_audio_streams(&original_info.audio_streams, &normalized_info.audio_streams);
    let subtitle_streams = compare_subtitle_streams(&original_info.subtitle_streams, &normalized_info.subtitle_streams);

    let format_changes = detect_format_changes(&original_info, &normalized_info);

    let has_pts_changes = check_pts_changes(&video_streams, &audio_streams);
    let has_dts_changes = false;
    let has_timebase_changes = check_timebase_changes(&video_streams, &audio_streams);
    let has_start_time_changes = check_start_time_changes(&video_streams, &audio_streams);

    let verdict = determine_verdict(
        &stream_count,
        &format_changes,
        has_pts_changes,
        has_timebase_changes,
        has_start_time_changes,
    );

    Ok(NormalizationDelta {
        original_path: original_path.to_string_lossy().into_owned(),
        normalized_path: normalized_path.to_string_lossy().into_owned(),
        stream_count,
        video_streams,
        audio_streams,
        subtitle_streams,
        format_changes,
        has_pts_changes,
        has_dts_changes,
        has_timebase_changes,
        has_start_time_changes,
        verdict,
    })
}

fn probe_file(ffprobe_path: &Path, file_path: &Path) -> Result<MediaInfo, String> {
    let output = Command::new(ffprobe_path)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            file_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

    parse_media_info(&json)
}

fn parse_media_info(json: &serde_json::Value) -> Result<MediaInfo, String> {
    let streams = json["streams"].as_array()
        .ok_or("Missing streams in ffprobe output")?;

    let mut video_streams = Vec::new();
    let mut audio_streams = Vec::new();
    let mut subtitle_streams = Vec::new();

    for stream in streams {
        let codec_type = stream["codec_type"].as_str().unwrap_or("");
        match codec_type {
            "video" => video_streams.push(parse_stream_info(stream)?),
            "audio" => audio_streams.push(parse_stream_info(stream)?),
            "subtitle" => subtitle_streams.push(parse_subtitle_stream_info(stream)?),
            _ => {}
        }
    }

    let format = &json["format"];
    Ok(MediaInfo {
        filename: format["filename"].as_str().map(|s| s.to_string()),
        format_name: format["format_name"].as_str().map(|s| s.to_string()),
        duration: format["duration"].as_str().and_then(|s| s.parse().ok()),
        start_time: format["start_time"].as_str().and_then(|s| s.parse().ok()),
        bit_rate: format["bit_rate"].as_str().map(|s| s.to_string()),
        video_streams,
        audio_streams,
        subtitle_streams,
    })
}

fn parse_stream_info(stream: &serde_json::Value) -> Result<StreamInfo, String> {
    Ok(StreamInfo {
        index: stream["index"].as_i64().unwrap_or(0) as usize,
        codec_name: stream["codec_name"].as_str().map(|s| s.to_string()),
        profile: stream["profile"].as_str().map(|s| s.to_string()),
        sample_rate: stream["sample_rate"].as_str().map(|s| s.to_string()),
        channels: stream["channels"].as_i64().map(|i| i as usize),
        time_base: stream["time_base"].as_str().map(|s| s.to_string()),
        start_time: stream["start_time"].as_str().and_then(|s| s.parse().ok()),
        duration: stream["duration"].as_str().and_then(|s| s.parse().ok()),
        bit_rate: stream["bit_rate"].as_str().map(|s| s.to_string()),
        language: stream["tags"]["language"].as_str().map(|s| s.to_string()),
        disposition: stream["disposition"].as_i64().map(|i| i as u32),
    })
}

fn parse_subtitle_stream_info(stream: &serde_json::Value) -> Result<SubtitleStreamInfo, String> {
    Ok(SubtitleStreamInfo {
        index: stream["index"].as_i64().unwrap_or(0) as usize,
        codec_name: stream["codec_name"].as_str().map(|s| s.to_string()),
        language: stream["tags"]["language"].as_str().map(|s| s.to_string()),
        disposition: stream["disposition"].as_i64().map(|i| i as u32),
    })
}

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub filename: Option<String>,
    pub format_name: Option<String>,
    pub duration: Option<f64>,
    pub start_time: Option<f64>,
    pub bit_rate: Option<String>,
    pub video_streams: Vec<StreamInfo>,
    pub audio_streams: Vec<StreamInfo>,
    pub subtitle_streams: Vec<SubtitleStreamInfo>,
}

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub index: usize,
    pub codec_name: Option<String>,
    pub profile: Option<String>,
    pub sample_rate: Option<String>,
    pub channels: Option<usize>,
    pub time_base: Option<String>,
    pub start_time: Option<f64>,
    pub duration: Option<f64>,
    pub bit_rate: Option<String>,
    pub language: Option<String>,
    pub disposition: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SubtitleStreamInfo {
    pub index: usize,
    pub codec_name: Option<String>,
    pub language: Option<String>,
    pub disposition: Option<u32>,
}

fn compare_stream_counts(original: &MediaInfo, normalized: &MediaInfo) -> StreamCountDelta {
    StreamCountDelta {
        original: original.video_streams.len() + original.audio_streams.len(),
        normalized: normalized.video_streams.len() + normalized.audio_streams.len(),
        video_changed: original.video_streams.len() != normalized.video_streams.len(),
        audio_changed: original.audio_streams.len() != normalized.audio_streams.len(),
        subtitle_changed: original.subtitle_streams.len() != normalized.subtitle_streams.len(),
    }
}

fn compare_video_streams(original: &[StreamInfo], normalized: &[StreamInfo]) -> Vec<StreamDelta> {
    let mut deltas = Vec::new();
    for (i, (orig, norm)) in original.iter().zip(normalized.iter()).enumerate() {
        deltas.push(StreamDelta {
            stream_index: i,
            stream_type: StreamType::Video,
            codec_name: compare_string(orig.codec_name.clone(), norm.codec_name.clone()),
            profile: compare_string(orig.profile.clone(), norm.profile.clone()),
            sample_rate: None,
            channels: None,
            time_base: compare_string(orig.time_base.clone(), norm.time_base.clone()),
            start_time: compare_float(orig.start_time, norm.start_time),
            duration: compare_float(orig.duration, norm.duration),
            bit_rate: compare_string(orig.bit_rate.clone(), norm.bit_rate.clone()),
            language: compare_string(orig.language.clone(), norm.language.clone()),
            disposition: compare_u32(orig.disposition, norm.disposition),
            has_changes: has_stream_changes(i, orig, norm),
        });
    }
    deltas
}

fn compare_audio_streams(original: &[StreamInfo], normalized: &[StreamInfo]) -> Vec<StreamDelta> {
    let mut deltas = Vec::new();
    for (i, (orig, norm)) in original.iter().zip(normalized.iter()).enumerate() {
        deltas.push(StreamDelta {
            stream_index: i,
            stream_type: StreamType::Audio,
            codec_name: compare_string(orig.codec_name.clone(), norm.codec_name.clone()),
            profile: compare_string(orig.profile.clone(), norm.profile.clone()),
            sample_rate: compare_string(orig.sample_rate.clone(), norm.sample_rate.clone()),
            channels: compare_usize(orig.channels, norm.channels),
            time_base: compare_string(orig.time_base.clone(), norm.time_base.clone()),
            start_time: compare_float(orig.start_time, norm.start_time),
            duration: compare_float(orig.duration, norm.duration),
            bit_rate: compare_string(orig.bit_rate.clone(), norm.bit_rate.clone()),
            language: compare_string(orig.language.clone(), norm.language.clone()),
            disposition: compare_u32(orig.disposition, norm.disposition),
            has_changes: has_stream_changes(i, orig, norm),
        });
    }
    deltas
}

fn compare_subtitle_streams(original: &[SubtitleStreamInfo], normalized: &[SubtitleStreamInfo]) -> Vec<StreamDelta> {
    let mut deltas = Vec::new();
    for (i, (orig, norm)) in original.iter().zip(normalized.iter()).enumerate() {
        deltas.push(StreamDelta {
            stream_index: i,
            stream_type: StreamType::Subtitle,
            codec_name: compare_string(orig.codec_name.clone(), norm.codec_name.clone()),
            profile: None,
            sample_rate: None,
            channels: None,
            time_base: None,
            start_time: None,
            duration: None,
            bit_rate: None,
            language: compare_string(orig.language.clone(), norm.language.clone()),
            disposition: compare_u32(orig.disposition, norm.disposition),
            has_changes: orig.codec_name != norm.codec_name || orig.language != norm.language,
        });
    }
    deltas
}

fn compare_string(orig: Option<String>, norm: Option<String>) -> Option<StringDelta> {
    Some(StringDelta {
        original: orig.clone(),
        normalized: norm.clone(),
        changed: orig != norm,
    })
}

fn compare_float(orig: Option<f64>, norm: Option<f64>) -> Option<FloatDelta> {
    match (orig, norm) {
        (Some(o), Some(n)) => {
            let diff = n - o;
            let pct = if o != 0.0 { (diff / o.abs()) * 100.0 } else { 0.0 };
            Some(FloatDelta {
                original: o,
                normalized: n,
                difference: diff,
                percent_change: pct,
            })
        }
        _ => None,
    }
}

fn compare_u32(orig: Option<u32>, norm: Option<u32>) -> Option<StringDelta> {
    compare_string(
        orig.map(|v| v.to_string()),
        norm.map(|v| v.to_string()),
    )
}

fn compare_usize(orig: Option<usize>, norm: Option<usize>) -> Option<StringDelta> {
    compare_string(
        orig.map(|v| v.to_string()),
        norm.map(|v| v.to_string()),
    )
}

fn has_stream_changes(_index: usize, orig: &StreamInfo, norm: &StreamInfo) -> bool {
    orig.codec_name != norm.codec_name
        || orig.profile != norm.profile
        || orig.sample_rate != norm.sample_rate
        || orig.channels != norm.channels
        || orig.time_base != norm.time_base
        || (orig.start_time.is_some() && norm.start_time.is_some() &&
            (orig.start_time.unwrap() - norm.start_time.unwrap()).abs() > 0.001)
        || orig.bit_rate != norm.bit_rate
}

fn detect_format_changes(original: &MediaInfo, normalized: &MediaInfo) -> Vec<FormatChange> {
    let mut changes = Vec::new();

    if original.format_name != normalized.format_name {
        changes.push(FormatChange {
            field: "format".to_string(),
            original: original.format_name.clone().unwrap_or_default(),
            normalized: normalized.format_name.clone().unwrap_or_default(),
            severity: ChangeSeverity::Info,
        });
    }

    let dur_diff = (original.duration.unwrap_or(0.0) - normalized.duration.unwrap_or(0.0)).abs();
    if dur_diff > 0.5 {
        changes.push(FormatChange {
            field: "duration".to_string(),
            original: format!("{:.3}s", original.duration.unwrap_or(0.0)),
            normalized: format!("{:.3}s", normalized.duration.unwrap_or(0.0)),
            severity: if dur_diff > 1.0 { ChangeSeverity::Warning } else { ChangeSeverity::Info },
        });
    }

    let start_diff = (original.start_time.unwrap_or(0.0) - normalized.start_time.unwrap_or(0.0)).abs();
    if start_diff > 0.001 {
        changes.push(FormatChange {
            field: "start_time".to_string(),
            original: format!("{:.6}s", original.start_time.unwrap_or(0.0)),
            normalized: format!("{:.6}s", normalized.start_time.unwrap_or(0.0)),
            severity: ChangeSeverity::Info,
        });
    }

    changes
}

fn check_pts_changes(_video: &[StreamDelta], _audio: &[StreamDelta]) -> bool {
    false
}

fn check_timebase_changes(video: &[StreamDelta], audio: &[StreamDelta]) -> bool {
    video.iter().any(|s| s.time_base.as_ref().map(|d| d.changed).unwrap_or(false))
        || audio.iter().any(|s| s.time_base.as_ref().map(|d| d.changed).unwrap_or(false))
}

fn check_start_time_changes(video: &[StreamDelta], audio: &[StreamDelta]) -> bool {
    video.iter().any(|s| s.start_time.as_ref().map(|d| d.difference.abs() > 0.001).unwrap_or(false))
        || audio.iter().any(|s| s.start_time.as_ref().map(|d| d.difference.abs() > 0.001).unwrap_or(false))
}

fn determine_verdict(
    stream_count: &StreamCountDelta,
    format_changes: &[FormatChange],
    has_pts_changes: bool,
    has_timebase_changes: bool,
    has_start_time_changes: bool,
) -> DeltaVerdict {
    if stream_count.video_changed || stream_count.subtitle_changed {
        return DeltaVerdict::Suspicious;
    }

    let critical_changes = format_changes.iter().filter(|c| matches!(c.severity, ChangeSeverity::Critical)).count();
    if critical_changes > 0 {
        return DeltaVerdict::Dangerous;
    }

    if has_pts_changes || has_timebase_changes {
        return DeltaVerdict::Suspicious;
    }

    if stream_count.audio_changed || has_start_time_changes {
        return DeltaVerdict::Safe;
    }

    if format_changes.is_empty() {
        DeltaVerdict::Identical
    } else {
        DeltaVerdict::Safe
    }
}

impl std::fmt::Display for NormalizationDelta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║           NORMALIZATION DELTA AUDIT REPORT                         ║")?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║ Original:  {}     ║", truncate_middle(&self.original_path, 40))?;
        writeln!(f, "║ Normalized: {}     ║", truncate_middle(&self.normalized_path, 40))?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║ Verdict: {}                                           ║", self.verdict)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║ Stream Counts:                                                   ║")?;
        writeln!(f, "║   Video: {} → {} (changed={})                                  ║",
            self.stream_count.original, self.stream_count.normalized, self.stream_count.video_changed)?;
        writeln!(f, "║   Audio: {} → {} (changed={})                                  ║",
            self.stream_count.original, self.stream_count.normalized, self.stream_count.audio_changed)?;
        writeln!(f, "║   Subtitle: {} → {} (changed={})                              ║",
            self.stream_count.original, self.stream_count.normalized, self.stream_count.subtitle_changed)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;

        if !self.format_changes.is_empty() {
            writeln!(f, "║ Format Changes:                                                 ║")?;
            for change in &self.format_changes {
                writeln!(f, "║   {}: {} → {}                               ║",
                    change.field, change.original, change.normalized)?;
            }
        }

        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║ Timestamp Integrity:                                            ║")?;
        writeln!(f, "║   PTS changed: {}                                               ║", self.has_pts_changes)?;
        writeln!(f, "║   DTS changed: {}                                               ║", self.has_dts_changes)?;
        writeln!(f, "║   Timebase changed: {}                                           ║", self.has_timebase_changes)?;
        writeln!(f, "║   Start time changed: {}                                         ║", self.has_start_time_changes)?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════╝")?;

        Ok(())
    }
}

fn truncate_middle(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let ellipsis = "...";
    let available = max_len - ellipsis.len();
    let front = available / 2;
    let back = available - front;
    format!("{}{}{}", &s[..front], ellipsis, &s[s.len() - back..])
}