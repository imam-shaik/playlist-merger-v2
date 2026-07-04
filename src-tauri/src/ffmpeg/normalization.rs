use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use crate::types::MediaInfo;
use crate::ffmpeg::norm_cache::NormalizationCache;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::sleep;

/// Complete media profile for a single file.
/// Captures ALL properties needed for merge compatibility analysis.
#[derive(Debug, Clone)]
pub struct MediaProfile {
    pub index: usize,
    pub path: String,
    // Video properties
    pub v_codec: Option<String>,
    pub v_profile: Option<String>,
    pub v_width: Option<u32>,
    pub v_height: Option<u32>,
    pub v_fps: Option<f64>,
    pub v_time_base: Option<String>,
    pub v_pixel_format: Option<String>,
    pub v_color_space: Option<String>,
    pub v_color_transfer: Option<String>,
    pub v_bit_depth: Option<u32>,
    pub v_field_order: Option<String>,
    pub v_sar: Option<String>,
    pub v_dar: Option<String>,
    // Audio properties
    pub a_codec: Option<String>,
    pub a_profile: Option<String>,
    pub a_sample_rate: Option<u32>,
    pub a_channels: Option<u32>,
    pub a_channel_layout: Option<String>,
    pub a_bit_depth: Option<u32>,
    // Derived
    pub is_vfr: bool,
    pub is_interlaced: bool,
    pub is_hdr: bool,
    pub has_multiple_video_streams: bool,
    pub has_multiple_audio_streams: bool,
    pub has_no_video: bool,
    pub has_no_audio: bool,
    // Rotation
    pub rotation: Option<i32>,
    // Audio offset/delay in seconds
    pub audio_start_offset_secs: Option<f64>,
    // Video vs audio duration drift (positive = audio shorter, negative = audio longer)
    pub audio_duration_drift_secs: Option<f64>,
    // Audio language
    pub audio_language: Option<String>,
    // Container format (mp4, mkv, mov, etc.)
    pub container_format: String,
    // Stream counts
    pub video_stream_count: usize,
    pub audio_stream_count: usize,
    // ── All audio streams (for intra-file mismatch detection) ──────────────
    pub all_audio_codecs: Vec<String>,
    pub all_audio_channels: Vec<u32>,
    pub all_audio_sample_rates: Vec<u32>,
    pub all_audio_profiles: Vec<String>,
    pub all_audio_channel_layouts: Vec<String>,
    pub has_intra_audio_channel_mismatch: bool,
    pub has_intra_audio_codec_mismatch: bool,
    pub has_intra_audio_sample_rate_mismatch: bool,
}

impl MediaProfile {
    /// Build a MediaProfile from a probed MediaInfo.
    pub fn from_media_info(index: usize, path: &str, info: &MediaInfo) -> Self {
        let v = info.video_streams.first();
        let a = info.audio_streams.first();

        // VFR detection: compare r_frame_rate vs avg_frame_rate
        let v_rfr = v.and_then(|s| s.r_frame_rate.as_deref());
        let v_afr = v.and_then(|s| s.avg_frame_rate.as_deref());
        let is_vfr = match (v_rfr, v_afr) {
            (Some(rfr), Some(afr)) => {
                let rfr_val = parse_frame_rate_str(rfr);
                let afr_val = parse_frame_rate_str(afr);
                match (rfr_val, afr_val) {
                    (Some(r), Some(a)) => (r - a).abs() > 0.1,
                    _ => false,
                }
            }
            _ => false,
        };

        // Interlaced detection
        let field_order = v.and_then(|s| s.field_order.clone());
        let is_interlaced = field_order.as_ref().is_some_and(|fo| {
            fo != "progressive" && fo != "unknown" && !fo.is_empty()
        });

        // HDR detection: check for PQ (ST 2084) or HLG transfer, or BT.2020 color primaries
        let color_transfer = v.and_then(|s| s.color_transfer.as_deref());
        let color_primaries = v.and_then(|s| s.color_primaries.as_deref());
        let is_hdr = matches!(
            (color_transfer, color_primaries),
            (Some("smpte2084"), _) | (Some("arib-std-b67"), _) | (_, Some("bt2020"))
        );

        // Audio start offset: detect if audio stream starts at a different time than video
        let audio_start_offset_secs = match (v.and_then(|s| s.start_time), a.and_then(|s| s.start_time)) {
            (Some(vst), Some(ast)) => Some(ast - vst),
            _ => None,
        };

        // Audio duration drift: compare video stream duration vs audio stream duration
        let audio_duration_drift_secs = match (v.and_then(|s| s.duration), a.and_then(|s| s.duration)) {
            (Some(vd), Some(ad)) if vd > 0.0 && ad > 0.0 => {
                let drift = vd - ad;
                if drift.abs() > 0.1 { Some(drift) } else { None }
            }
            _ => None,
        };

        // Rotation
        let rotation = v.and_then(|s| s.rotation);

        // Stream counts
        let video_stream_count = info.video_streams.len();
        let audio_stream_count = info.audio_streams.len();

        // Sanity-filtered dimensions: reject 0x0, > 16384, or non-even
        let sanitize_dim = |w: Option<u32>, h: Option<u32>| -> (Option<u32>, Option<u32>) {
            match (w, h) {
                (Some(w), Some(h)) if w > 0 && h > 0 && w <= 16384 && h <= 16384 => {
                    (Some(w / 2 * 2), Some(h / 2 * 2)) // enforce even dimensions
                }
                _ => (None, None),
            }
        };
        let (v_width_sane, v_height_sane) = sanitize_dim(v.and_then(|s| s.width), v.and_then(|s| s.height));

        // Sanity-filtered FPS: reject < 1 or > 240 fps (unrealistic for normal video)
        let sanitize_fps = |fps: Option<f64>| -> Option<f64> {
            match fps {
                Some(f) if (1.0..=240.0).contains(&f) => Some(f),
                _ => None,
            }
        };
        let v_fps_sane = sanitize_fps(v.and_then(|s| s.fps));

        // Collect ALL audio stream properties for intra-file mismatch detection
        let all_audio_codecs: Vec<String> = info.audio_streams.iter().map(|s| s.codec_name.clone()).collect();
        let all_audio_channels: Vec<u32> = info.audio_streams.iter().filter_map(|s| s.channels).collect();
        let all_audio_sample_rates: Vec<u32> = info.audio_streams.iter().filter_map(|s| s.sample_rate).collect();
        let all_audio_profiles: Vec<String> = info.audio_streams.iter().filter_map(|s| s.profile.clone()).collect();
        let all_audio_channel_layouts: Vec<String> = info.audio_streams.iter().filter_map(|s| s.channel_layout.clone()).collect();

        // Detect intra-file mismatches: multiple streams in the same file with different properties
        let unique_codecs: Vec<&str> = all_audio_codecs.iter().map(|s| s.as_str()).collect::<std::collections::HashSet<_>>().into_iter().collect();
        let has_intra_audio_codec_mismatch = unique_codecs.len() > 1;
        let has_intra_audio_channel_mismatch = all_audio_channels.len() > 1 && all_audio_channels.iter().min() != all_audio_channels.iter().max();
        let has_intra_audio_sample_rate_mismatch = all_audio_sample_rates.len() > 1 && all_audio_sample_rates.iter().min() != all_audio_sample_rates.iter().max();

        Self {
            index,
            path: path.to_string(),
            v_codec: v.map(|s| s.codec_name.clone()),
            v_profile: v.and_then(|s| s.profile.clone()),
            v_width: v_width_sane,
            v_height: v_height_sane,
            v_fps: v_fps_sane,
            v_time_base: v.and_then(|s| s.time_base.clone()),
            v_pixel_format: v.and_then(|s| s.pixel_format.clone()),
            v_color_space: v.and_then(|s| s.color_space.clone()),
            v_color_transfer: v.and_then(|s| s.color_transfer.clone()),
            v_bit_depth: v.and_then(|s| s.bits_per_raw_sample),
            v_field_order: field_order,
            v_sar: v.and_then(|s| s.sample_aspect_ratio.clone()),
            v_dar: v.and_then(|s| s.display_aspect_ratio.clone()),
            a_codec: a.map(|s| s.codec_name.clone()),
            a_profile: a.and_then(|s| s.profile.clone()),
            a_sample_rate: a.and_then(|s| s.sample_rate),
            a_channels: a.and_then(|s| s.channels),
            a_channel_layout: a.and_then(|s| s.channel_layout.clone()),
            a_bit_depth: a.and_then(|s| s.bits_per_raw_sample),
            is_vfr,
            is_interlaced,
            is_hdr,
            has_multiple_video_streams: video_stream_count > 1,
            has_multiple_audio_streams: audio_stream_count > 1,
            has_no_video: video_stream_count == 0,
            has_no_audio: audio_stream_count == 0,
            rotation,
            audio_start_offset_secs,
            audio_duration_drift_secs,
            audio_language: a.and_then(|s| s.language.clone()),
            container_format: info.format_name.clone(),
            video_stream_count,
            audio_stream_count,
            all_audio_codecs,
            all_audio_channels,
            all_audio_sample_rates,
            all_audio_profiles,
            all_audio_channel_layouts,
            has_intra_audio_channel_mismatch,
            has_intra_audio_codec_mismatch,
            has_intra_audio_sample_rate_mismatch,
        }
    }
}

/// Parse a frame rate string like "30000/1001" or "30" into f64.
fn parse_frame_rate_str(s: &str) -> Option<f64> {
    if s.contains('/') {
        let parts: Vec<&str> = s.splitn(2, '/').collect();
        if parts.len() == 2 {
            let num: f64 = parts[0].parse().ok()?;
            let den: f64 = parts[1].parse().ok()?;
            if den > 0.0 { return Some(num / den); }
        }
    }
    s.parse::<f64>().ok()
}

/// Count occurrences of each value in a property.
/// Returns a sorted histogram (value → count) using BTreeMap for deterministic iteration.
fn histogram<T: Eq + std::hash::Hash + Clone + Ord>(values: &[Option<T>]) -> std::collections::BTreeMap<T, usize> {
    let mut counts = std::collections::BTreeMap::new();
    for val in values.iter().flatten() {
        *counts.entry(val.clone()).or_insert(0) += 1;
    }
    counts
}

/// Find the dominant (most common) value in a histogram.
/// Uses BTreeMap for deterministic iteration - ties broken by key order (first alphabetically).
fn dominant<T: Eq + std::hash::Hash + Clone + Ord>(hist: &std::collections::BTreeMap<T, usize>) -> Option<T> {
    hist.iter()
        .max_by_key(|(&_, &count)| count)
        .map(|(val, _)| val.clone())
}

/// What type of normalization a file needs.
#[derive(Debug, Clone, PartialEq)]
pub enum NormalizationType {
    /// No normalization needed — file matches dominant profile
    None,
    /// Lossless remux only (change timescale/container, no re-encode)
    RemuxOnly,
    /// Re-encode video only (audio is compatible)
    VideoReencode,
    /// Re-encode audio only (video is compatible)
    AudioReencode,
    /// Full re-encode (both video and audio are incompatible)
    FullReencode,
}

/// Encoding profile for video+audio normalization.
/// Groups all target encoding parameters.
#[derive(Debug, Clone)]
pub struct EncodingProfile {
    pub video_codec: String,
    pub audio_codec: String,
    pub sample_rate: u32,
    pub fps: Option<f64>,
    pub timescale: Option<u32>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub channels: Option<u32>,
    pub bitrate: Option<String>,
}

impl EncodingProfile {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        video_codec: &str,
        audio_codec: &str,
        sample_rate: u32,
        fps: Option<f64>,
        timescale: Option<u32>,
        width: Option<u32>,
        height: Option<u32>,
        channels: Option<u32>,
    ) -> Self {
        Self {
            video_codec: video_codec.to_string(),
            audio_codec: audio_codec.to_string(),
            sample_rate,
            fps,
            timescale,
            width,
            height,
            channels,
            bitrate: None,
        }
    }

    /// Set the audio bitrate for encoding.
    pub fn with_bitrate(mut self, bitrate: Option<String>) -> Self {
        self.bitrate = bitrate;
        self
    }
}

/// Audio-only encoding profile for audio normalization.
#[derive(Debug, Clone)]
pub struct AudioProfile {
    pub audio_codec: String,
    pub sample_rate: u32,
    pub timescale: Option<u32>,
    pub channels: Option<u32>,
    pub bitrate: Option<String>,
}

impl AudioProfile {
    pub fn new(
        audio_codec: &str,
        sample_rate: u32,
        timescale: Option<u32>,
        channels: Option<u32>,
    ) -> Self {
        Self {
            audio_codec: audio_codec.to_string(),
            sample_rate,
            timescale,
            channels,
            bitrate: None,
        }
    }

    /// Set the audio bitrate for encoding.
    pub fn with_bitrate(mut self, bitrate: Option<String>) -> Self {
        self.bitrate = bitrate;
        self
    }
}

/// Certification audit record for a single normalization operation.
/// Captures the exact transformation applied and before/after properties
/// for debugging audio quality issues.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NormalizationAudit {
    pub file_index: usize,
    pub input_path: String,
    pub output_path: String,
    pub normalization_type: String,
    // Input properties
    pub input_codec: String,
    pub input_profile: Option<String>,
    pub input_sample_rate: Option<u32>,
    pub input_channels: Option<u32>,
    pub input_bitrate: Option<u64>,
    pub input_duration: f64,
    // Target properties
    pub target_codec: String,
    pub target_sample_rate: u32,
    pub target_channels: Option<u32>,
    // Output properties (probed after normalization)
    pub output_codec: Option<String>,
    pub output_profile: Option<String>,
    pub output_sample_rate: Option<u32>,
    pub output_channels: Option<u32>,
    pub output_bitrate: Option<u64>,
    pub output_duration: Option<f64>,
    // Transformation summary
    pub profile_changed: bool,
    pub sample_rate_changed: bool,
    pub channels_changed: bool,
    pub bitrate_changed: bool,
    pub resample_applied: bool,
    pub ffprobe_available: bool,
}

impl NormalizationAudit {
    /// Log the full certification audit report for this normalization.
    pub fn log_report(&self) {
        log::info!("╔══════════════════════════════════════════════════════════════════════════════╗");
        log::info!("║  NORMALIZATION CERTIFICATION AUDIT — File #{}                               ║", self.file_index);
        log::info!("╠══════════════════════════════════════════════════════════════════════════════╣");
        log::info!("║  INPUT                                                                     ║");
        log::info!("║    Path:         {}║", truncate_path(&self.input_path, 56));
        log::info!("║    Codec:        {:<54}║", self.input_codec);
        log::info!("║    Profile:      {:<54}║", self.input_profile.as_deref().unwrap_or("N/A"));
        log::info!("║    Sample Rate:  {:<54}║", self.input_sample_rate.map(|r| format!("{} Hz", r)).unwrap_or_else(|| "N/A".to_string()));
        log::info!("║    Channels:     {:<54}║", self.input_channels.map(|c| c.to_string()).unwrap_or_else(|| "N/A".to_string()));
        log::info!("║    Bitrate:      {:<54}║", self.input_bitrate.map(|b| format!("{} bps", b)).unwrap_or_else(|| "N/A (FFmpeg will use default)".to_string()));
        log::info!("║    Duration:     {:<54.3}║", self.input_duration);
        log::info!("║                                                                              ║");
        log::info!("║  TARGET                                                                     ║");
        log::info!("║    Codec:        {:<54}║", self.target_codec);
        log::info!("║    Sample Rate:  {:<54}║", format!("{} Hz", self.target_sample_rate));
        log::info!("║    Channels:     {:<54}║", self.target_channels.map(|c| c.to_string()).unwrap_or_else(|| "unchanged".to_string()));
        log::info!("║                                                                              ║");
        if self.ffprobe_available {
            log::info!("║  OUTPUT (probed)                                                            ║");
            log::info!("║    Codec:        {:<54}║", self.output_codec.as_deref().unwrap_or("N/A"));
            log::info!("║    Profile:      {:<54}║", self.output_profile.as_deref().unwrap_or("N/A"));
            log::info!("║    Sample Rate:  {:<54}║", self.output_sample_rate.map(|r| format!("{} Hz", r)).unwrap_or_else(|| "N/A".to_string()));
            log::info!("║    Channels:     {:<54}║", self.output_channels.map(|c| c.to_string()).unwrap_or_else(|| "N/A".to_string()));
            log::info!("║    Bitrate:      {:<54}║", self.output_bitrate.map(|b| format!("{} bps", b)).unwrap_or_else(|| "N/A".to_string()));
            log::info!("║    Duration:     {:<54}║", self.output_duration.map(|d| format!("{:.3}s", d)).unwrap_or_else(|| "N/A".to_string()));
        } else {
            log::info!("║  OUTPUT (not probed — ffprobe unavailable)                                  ║");
        }
        log::info!("║                                                                              ║");
        log::info!("║  TRANSFORMATION                                                             ║");
        log::info!("║    Profile changed:     {:<47}║", if self.profile_changed { "YES" } else { "no" });
        log::info!("║    Sample rate changed: {:<47}║", if self.sample_rate_changed { "YES" } else { "no" });
        log::info!("║    Channels changed:    {:<47}║", if self.channels_changed { "YES" } else { "no" });
        log::info!("║    Bitrate changed:     {:<47}║", if self.bitrate_changed { "YES" } else { "no (preserved)" });
        log::info!("║    Resample applied:    {:<47}║", if self.resample_applied { "YES" } else { "no" });
        log::info!("╚══════════════════════════════════════════════════════════════════════════════╝");
    }
}

fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}

/// Granular audio normalization categories for precise UI display.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioNormalizationType {
    SampleRateMismatch,
    ChannelMismatch,
    ChannelLayoutMismatch,
    CodecMismatch,
    BitDepthMismatch,
    AACProfileMismatch,
    DurationDriftRepair,
    CorruptionRepair,
    /// Intra-file: same file has streams with different codecs
    IntraFileCodecMismatch,
    /// Intra-file: same file has streams with different channel counts
    IntraFileChannelMismatch,
    /// Intra-file: same file has streams with different sample rates
    IntraFileSampleRateMismatch,
}

/// Specifies which merge backend will be used, as they have different stream-copy capabilities.
/// This affects which outliers are considered "safe" to skip during normalization.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MergeBackend {
    /// mkvmerge from MKVToolNix - has stricter requirements for stream concatenation.
    /// Cannot safely handle: mixed sample rate AAC streams (causes 100+ second A/V mismatch).
    MkvMerge,
    /// FFmpeg concat demuxer - handles more variations natively.
    FfmpegConcat,
}

impl MergeBackend {
    /// Returns true if this backend supports mixed sample rate audio streams.
    /// mkvmerge cannot safely concatenate AAC streams with different sample rates.
    pub fn supports_mixed_sample_rate(&self) -> bool {
        match self {
            MergeBackend::MkvMerge => false,
            MergeBackend::FfmpegConcat => true,
        }
    }
}

impl AudioNormalizationType {
    /// Human-readable label for UI display.
    pub fn label(&self) -> &'static str {
        match self {
            AudioNormalizationType::SampleRateMismatch => "Sample Rate Fix",
            AudioNormalizationType::ChannelMismatch => "Channel Fix",
            AudioNormalizationType::ChannelLayoutMismatch => "Channel Layout Fix",
            AudioNormalizationType::CodecMismatch => "Audio Codec Fix",
            AudioNormalizationType::BitDepthMismatch => "Bit Depth Fix",
            AudioNormalizationType::AACProfileMismatch => "AAC Profile Fix",
            AudioNormalizationType::DurationDriftRepair => "Duration Drift Repair",
            AudioNormalizationType::CorruptionRepair => "Corruption Repair",
            AudioNormalizationType::IntraFileCodecMismatch => "Intra-File Codec Mismatch",
            AudioNormalizationType::IntraFileChannelMismatch => "Intra-File Channel Mismatch",
            AudioNormalizationType::IntraFileSampleRateMismatch => "Intra-File Sample Rate Mismatch",
        }
    }

    /// Short code for embedding in repairReason strings.
    pub fn code(&self) -> &'static str {
        match self {
            AudioNormalizationType::SampleRateMismatch => "SampleRateMismatch",
            AudioNormalizationType::ChannelMismatch => "ChannelMismatch",
            AudioNormalizationType::ChannelLayoutMismatch => "LayoutMismatch",
            AudioNormalizationType::CodecMismatch => "CodecMismatch",
            AudioNormalizationType::BitDepthMismatch => "BitDepthMismatch",
            AudioNormalizationType::AACProfileMismatch => "AACProfileMismatch",
            AudioNormalizationType::DurationDriftRepair => "DurationDriftRepair",
            AudioNormalizationType::CorruptionRepair => "CorruptionRepair",
            AudioNormalizationType::IntraFileCodecMismatch => "IntraFileCodecMismatch",
            AudioNormalizationType::IntraFileChannelMismatch => "IntraFileChannelMismatch",
            AudioNormalizationType::IntraFileSampleRateMismatch => "IntraFileSampleRateMismatch",
        }
    }
}

/// An audio-specific outlier with granular normalization type.
#[derive(Debug, Clone)]
pub struct AudioOutlier {
    pub index: usize,
    pub path: String,
    pub audio_type: AudioNormalizationType,
    pub dominant_value: String,
    pub actual_value: String,
}

/// An outlier file that doesn't match the dominant profile.
#[derive(Debug, Clone)]
pub struct Outlier {
    pub index: usize,
    pub path: String,
    pub reason: String,
    pub normalization_type: NormalizationType,
    /// The dominant value this file deviates from
    pub dominant_value: String,
    /// This file's value
    pub actual_value: String,
    /// Which property is mismatched
    pub property: String,
}

/// The dominant profile for a batch of files.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DominantProfile {
    pub v_codec: Option<String>,
    pub v_profile: Option<String>,
    pub v_width: Option<u32>,
    pub v_height: Option<u32>,
    pub v_fps: Option<f64>,
    pub v_time_base: Option<String>,
    pub v_pixel_format: Option<String>,
    pub v_color_space: Option<String>,
    pub v_color_transfer: Option<String>,
    pub v_bit_depth: Option<u32>,
    pub a_codec: Option<String>,
    pub a_profile: Option<String>,
    pub a_sample_rate: Option<u32>,
    pub a_channels: Option<u32>,
    pub a_channel_layout: Option<String>,
    pub a_bit_depth: Option<u32>,
    pub a_language: Option<String>,
    pub v_rotation: Option<String>,
    pub container_format: Option<String>,
    pub dar: Option<String>,
    /// Timescale denominator extracted from v_time_base (e.g., 30000 from "1/30000")
    pub timescale_den: Option<u32>,
    /// Number of files that match this profile exactly
    pub match_count: usize,
    /// Total number of files
    pub total_count: usize,
    /// Dominant VFR state
    pub is_vfr: bool,
    /// Dominant interlaced state
    pub is_interlaced: bool,
}

/// Complete analysis result for a batch of files.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ProfileAnalysis {
    pub profiles: Vec<MediaProfile>,
    pub dominant: DominantProfile,
    pub outliers: Vec<Outlier>,
    pub audio_outliers: Vec<AudioOutlier>,
    pub histograms: PropertyHistograms,
}

/// Histograms for all audited properties.
/// Uses BTreeMap for deterministic iteration order.
#[derive(Debug)]
#[allow(dead_code)]
pub struct PropertyHistograms {
    pub v_codec: std::collections::BTreeMap<String, usize>,
    pub v_profile: std::collections::BTreeMap<String, usize>,
    pub v_resolution: std::collections::BTreeMap<String, usize>,
    pub v_fps: std::collections::BTreeMap<String, usize>,
    pub v_time_base: std::collections::BTreeMap<String, usize>,
    pub v_pixel_format: std::collections::BTreeMap<String, usize>,
    pub v_color_space: std::collections::BTreeMap<String, usize>,
    pub v_color_transfer: std::collections::BTreeMap<String, usize>,
    pub v_field_order: std::collections::BTreeMap<String, usize>,
    pub v_sar: std::collections::BTreeMap<String, usize>,
    pub v_bit_depth: std::collections::BTreeMap<u32, usize>,
    pub a_codec: std::collections::BTreeMap<String, usize>,
    pub a_profile: std::collections::BTreeMap<String, usize>,
    pub a_sample_rate: std::collections::BTreeMap<u32, usize>,
    pub a_channels: std::collections::BTreeMap<u32, usize>,
    pub a_channel_layout: std::collections::BTreeMap<String, usize>,
    pub a_bit_depth: std::collections::BTreeMap<u32, usize>,
    pub vfr_count: usize,
    pub interlaced_count: usize,
    pub rotation: std::collections::BTreeMap<String, usize>,
    pub audio_start_offset: std::collections::BTreeMap<String, usize>,
    pub audio_language: std::collections::BTreeMap<String, usize>,
    pub audio_duration_drift: std::collections::BTreeMap<String, usize>,
    pub container_format: std::collections::BTreeMap<String, usize>,
    pub hdr_count: usize,
    pub no_video_count: usize,
    pub no_audio_count: usize,
    pub multi_video_count: usize,
    pub multi_audio_count: usize,
    pub dar: std::collections::BTreeMap<String, usize>,
}

/// Analyze a batch of files and generate comprehensive profile report.
pub fn analyze_profiles(infos: &[(usize, String, MediaInfo)]) -> ProfileAnalysis {
    let profiles: Vec<MediaProfile> = infos.iter()
        .map(|(idx, path, info)| MediaProfile::from_media_info(*idx, path, info))
        .collect();

    // Build histograms
    let histograms = PropertyHistograms {
        v_codec: histogram(&profiles.iter().map(|p| p.v_codec.clone()).collect::<Vec<_>>()),
        v_profile: histogram(&profiles.iter().map(|p| p.v_profile.clone()).collect::<Vec<_>>()),
        v_resolution: histogram(&profiles.iter().map(|p| {
            match (p.v_width, p.v_height) {
                (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                _ => None,
            }
        }).collect::<Vec<_>>()),
        v_fps: histogram(&profiles.iter().map(|p| {
            p.v_fps.map(|f| format!("{:.3}", f))
        }).collect::<Vec<_>>()),
        v_time_base: histogram(&profiles.iter().map(|p| p.v_time_base.clone()).collect::<Vec<_>>()),
        v_pixel_format: histogram(&profiles.iter().map(|p| p.v_pixel_format.clone()).collect::<Vec<_>>()),
        v_color_space: histogram(&profiles.iter().map(|p| p.v_color_space.clone()).collect::<Vec<_>>()),
        v_color_transfer: histogram(&profiles.iter().map(|p| p.v_color_transfer.clone()).collect::<Vec<_>>()),
        v_field_order: histogram(&profiles.iter().map(|p| p.v_field_order.clone()).collect::<Vec<_>>()),
        v_sar: histogram(&profiles.iter().map(|p| p.v_sar.clone()).collect::<Vec<_>>()),
        v_bit_depth: histogram(&profiles.iter().map(|p| p.v_bit_depth).collect::<Vec<_>>()),
        a_codec: histogram(&profiles.iter().map(|p| p.a_codec.clone()).collect::<Vec<_>>()),
        a_profile: histogram(&profiles.iter().map(|p| p.a_profile.clone()).collect::<Vec<_>>()),
        a_sample_rate: histogram(&profiles.iter().map(|p| p.a_sample_rate).collect::<Vec<_>>()),
        a_channels: histogram(&profiles.iter().map(|p| p.a_channels).collect::<Vec<_>>()),
        a_channel_layout: histogram(&profiles.iter().map(|p| p.a_channel_layout.clone()).collect::<Vec<_>>()),
        a_bit_depth: histogram(&profiles.iter().map(|p| p.a_bit_depth).collect::<Vec<_>>()),
        vfr_count: profiles.iter().filter(|p| p.is_vfr).count(),
        interlaced_count: profiles.iter().filter(|p| p.is_interlaced).count(),
        rotation: histogram(&profiles.iter().map(|p| {
            p.rotation.map(|r| r.to_string())
        }).collect::<Vec<_>>()),
        audio_start_offset: histogram(&profiles.iter().map(|p| {
            p.audio_start_offset_secs.map(|o| {
                if o.abs() < 0.001 { "0ms".to_string() }
                else if o.abs() < 1.0 { format!("{}ms", (o * 1000.0).round() as i64) }
                else { format!("{:.1}s", o) }
            })
        }).collect::<Vec<_>>()),
        audio_language: histogram(&profiles.iter().map(|p| p.audio_language.clone()).collect::<Vec<_>>()),
        audio_duration_drift: histogram(&profiles.iter().map(|p| {
            p.audio_duration_drift_secs.map(|d| {
                if d.abs() < 0.5 { "ok".to_string() }
                else if d > 0.0 { format!("audio_{:.0}ms_shorter", d * 1000.0) }
                else { format!("audio_{:.0}ms_longer", d.abs() * 1000.0) }
            })
        }).collect::<Vec<_>>()),
        container_format: histogram(&profiles.iter().map(|p| {
            if p.container_format == "unknown" { None } else { Some(p.container_format.clone()) }
        }).collect::<Vec<_>>()),
        hdr_count: profiles.iter().filter(|p| p.is_hdr).count(),
        no_video_count: profiles.iter().filter(|p| p.has_no_video).count(),
        no_audio_count: profiles.iter().filter(|p| p.has_no_audio).count(),
        multi_video_count: profiles.iter().filter(|p| p.has_multiple_video_streams).count(),
        multi_audio_count: profiles.iter().filter(|p| p.has_multiple_audio_streams).count(),
        dar: histogram(&profiles.iter().map(|p| p.v_dar.clone()).collect::<Vec<_>>()),
    };

    // Find dominant profile
    let dom_v_time_base = dominant(&histograms.v_time_base);
    let timescale_den = dom_v_time_base.as_ref().and_then(|tb| {
        if tb.contains('/') {
            tb.split('/').nth(1).and_then(|s| s.parse::<u32>().ok())
        } else {
            tb.parse::<u32>().ok()
        }
    });

    let dominant = DominantProfile {
        v_codec: dominant(&histograms.v_codec),
        v_profile: dominant(&histograms.v_profile),
        v_width: dominant(&histograms.v_resolution).and_then(|r| {
            r.split('x').next().and_then(|w| w.parse().ok())
        }),
        v_height: dominant(&histograms.v_resolution).and_then(|r| {
            r.split('x').nth(1).and_then(|h| h.parse().ok())
        }),
        v_fps: dominant(&histograms.v_fps).and_then(|f| f.parse().ok()),
        v_time_base: dom_v_time_base,
        v_pixel_format: dominant(&histograms.v_pixel_format),
        v_color_space: dominant(&histograms.v_color_space),
        v_color_transfer: dominant(&histograms.v_color_transfer),
        v_bit_depth: dominant(&histograms.v_bit_depth),
        a_codec: dominant(&histograms.a_codec),
        a_profile: dominant(&histograms.a_profile),
        a_sample_rate: dominant(&histograms.a_sample_rate),
        a_channels: dominant(&histograms.a_channels),
        a_channel_layout: dominant(&histograms.a_channel_layout),
        a_bit_depth: dominant(&histograms.a_bit_depth),
        a_language: dominant(&histograms.audio_language),
        v_rotation: dominant(&histograms.rotation),
        container_format: dominant(&histograms.container_format),
        dar: dominant(&histograms.dar),
        timescale_den,
        match_count: 0, // calculated below
        total_count: profiles.len(),
        is_vfr: histograms.vfr_count > profiles.len() / 2,
        is_interlaced: histograms.interlaced_count > profiles.len() / 2,
        };

    // Detect outliers
    let mut outliers = Vec::new();
    let mut audio_outliers = Vec::new();
    for p in &profiles {
        check_property(p, &dominant, &histograms, &mut outliers);
        collect_audio_outliers(p, &dominant, &mut audio_outliers);
    }

    // Compute match count (files with zero outliers)
    let outlier_indices: std::collections::HashSet<usize> = outliers.iter().map(|o| o.index).collect();
    let match_count = profiles.iter().filter(|p| !outlier_indices.contains(&p.index)).count();

    let dominant = DominantProfile { match_count, ..dominant };

    ProfileAnalysis { profiles, dominant, outliers, audio_outliers, histograms }
}

/// Check a single file against the dominant profile and record any mismatches.
fn check_property(
    p: &MediaProfile,
    dom: &DominantProfile,
    hist: &PropertyHistograms,
    outliers: &mut Vec<Outlier>,
) {
    // Video codec
    check_opt_str(p.index, &p.path, "v_codec", &p.v_codec, &dom.v_codec, outliers, NormalizationType::FullReencode);

    // Video profile (within same codec)
    check_opt_str(p.index, &p.path, "v_profile", &p.v_profile, &dom.v_profile, outliers, NormalizationType::FullReencode);

    // Resolution
    let p_res = match (p.v_width, p.v_height) {
        (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
        _ => None,
    };
    let dom_res = match (dom.v_width, dom.v_height) {
        (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
        _ => None,
    };
    check_opt_string(p.index, &p.path, "resolution", &p_res, &dom_res, outliers, NormalizationType::VideoReencode);

    // FPS - with NUMERIC tolerance comparison instead of exact string match
    // Tolerance: 0.1% (~0.03 fps at 30fps, ~0.024 fps at 24fps)
    // This allows 29.970 ≈ 30.000 and 23.976 ≈ 24.000 without triggering re-encode
    let p_fps_val = p.v_fps;
    let dom_fps_val = dom.v_fps;
    check_fps_tolerance(p.index, &p.path, p_fps_val, dom_fps_val, outliers);

    // Timebase
    check_opt_str(p.index, &p.path, "time_base", &p.v_time_base, &dom.v_time_base, outliers, NormalizationType::RemuxOnly);

    // Pixel format
    check_opt_str(p.index, &p.path, "pixel_format", &p.v_pixel_format, &dom.v_pixel_format, outliers, NormalizationType::VideoReencode);

    // Color space
    check_opt_str(p.index, &p.path, "color_space", &p.v_color_space, &dom.v_color_space, outliers, NormalizationType::VideoReencode);

    // Color transfer
    check_opt_str(p.index, &p.path, "color_transfer", &p.v_color_transfer, &dom.v_color_transfer, outliers, NormalizationType::VideoReencode);

    // Bit depth (critical for HDR and lossless concat)
    check_opt_u32(p.index, &p.path, "v_bit_depth", p.v_bit_depth, dom.v_bit_depth, outliers, NormalizationType::VideoReencode);

    // VFR detection
    if p.is_vfr != dom.is_vfr {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: if p.is_vfr {
                "Variable Frame Rate (VFR) detected in CFR playlist — can cause timeline drift and audio sync issues".to_string()
            } else {
                "Constant Frame Rate (CFR) detected in VFR playlist — normalization recommended for stability".to_string()
            },
            normalization_type: NormalizationType::VideoReencode,
            dominant_value: if dom.is_vfr { "VFR".to_string() } else { "CFR".to_string() },
            actual_value: if p.is_vfr { "VFR".to_string() } else { "CFR".to_string() },
            property: "frame_rate_type".to_string(),
        });
    }

    // Interlaced detection
    if p.is_interlaced != dom.is_interlaced {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: if p.is_interlaced {
                "Interlaced content detected in progressive playlist — may cause artifacts in concat".to_string()
            } else {
                "Progressive content detected in interlaced playlist — normalization recommended for consistency".to_string()
            },
            normalization_type: NormalizationType::VideoReencode,
            dominant_value: if dom.is_interlaced { "interlaced".to_string() } else { "progressive".to_string() },
            actual_value: if p.is_interlaced { "interlaced".to_string() } else { "progressive".to_string() },
            property: "field_order".to_string(),
        });
    }

    // Audio codec
    check_opt_str(p.index, &p.path, "a_codec", &p.a_codec, &dom.a_codec, outliers, NormalizationType::AudioReencode);

    // Audio profile (e.g., AAC-LC vs HE-AAC)
    check_opt_str(p.index, &p.path, "a_profile", &p.a_profile, &dom.a_profile, outliers, NormalizationType::AudioReencode);

    // Audio sample rate
    check_opt_u32(p.index, &p.path, "a_sample_rate", p.a_sample_rate, dom.a_sample_rate, outliers, NormalizationType::AudioReencode);

    // Audio channels
    check_opt_u32(p.index, &p.path, "a_channels", p.a_channels, dom.a_channels, outliers, NormalizationType::AudioReencode);

    // Audio channel layout
    check_opt_str(p.index, &p.path, "a_channel_layout", &p.a_channel_layout, &dom.a_channel_layout, outliers, NormalizationType::AudioReencode);

    // Audio bit depth
    check_opt_u32(p.index, &p.path, "a_bit_depth", p.a_bit_depth, dom.a_bit_depth, outliers, NormalizationType::AudioReencode);

    // Multi-channel audio (>2 channels) — critical for concat
    if let Some(ch) = p.a_channels {
        if ch > 2 {
            outliers.push(Outlier {
                index: p.index,
                path: p.path.clone(),
                reason: format!("Multi-channel audio ({}ch) causes 'rematrix is needed' errors in lossless concat", ch),
                normalization_type: NormalizationType::AudioReencode,
                dominant_value: format!("{}ch", dom.a_channels.unwrap_or(2)),
                actual_value: format!("{}ch", ch),
                property: "audio_channels_critical".to_string(),
            });
        }
    }

    // ── New checks ─────────────────────────────────────────────────────────────

    // Rotation mismatch
    let p_rot = p.rotation.map(|r| r.to_string());
    check_opt_string(p.index, &p.path, "rotation", &p_rot, &dom.v_rotation, outliers, NormalizationType::VideoReencode);

    // Audio start offset (significant delay > 500ms)
    if let Some(offset) = p.audio_start_offset_secs {
        if offset.abs() > 0.5 {
            let dom_offset = dominant(&hist.audio_start_offset).unwrap_or_else(|| "0ms".to_string());
            outliers.push(Outlier {
                index: p.index,
                path: p.path.clone(),
                reason: format!("Audio start offset of {:.0}ms detected — can cause audio/video sync drift in concat", offset * 1000.0),
                normalization_type: NormalizationType::FullReencode,
                dominant_value: dom_offset,
                actual_value: format!("{:.0}ms", offset * 1000.0),
                property: "audio_start_offset".to_string(),
            });
        }
    }

    // Audio language mismatch
    check_opt_string(p.index, &p.path, "audio_language", &p.audio_language, &dom.a_language, outliers, NormalizationType::AudioReencode);

    // Audio duration drift (video stream duration vs audio stream duration)
    if let Some(drift) = p.audio_duration_drift_secs {
        if drift.abs() > 0.5 {
            outliers.push(Outlier {
                index: p.index,
                path: p.path.clone(),
                reason: format!("Audio stream is {:.0}ms {} than video stream — possible corruption or truncation", drift.abs() * 1000.0, if drift > 0.0 { "shorter" } else { "longer" }),
                normalization_type: NormalizationType::AudioReencode,
                dominant_value: "aligned".to_string(),
                actual_value: format!("{:+.0}ms", drift * 1000.0),
                property: "audio_duration_drift".to_string(),
            });
        }
    }

    // Container format mismatch
    let cf = if p.container_format == "unknown" { None } else { Some(p.container_format.clone()) };
    check_opt_string(p.index, &p.path, "container_format", &cf, &dom.container_format, outliers, NormalizationType::RemuxOnly);

    // Display aspect ratio (DAR) mismatch
    check_opt_string(p.index, &p.path, "dar", &p.v_dar, &dom.dar, outliers, NormalizationType::VideoReencode);

    // HDR detection — flag HDR files in an SDR-dominant playlist
    if p.is_hdr && dom.v_color_transfer.as_deref() != Some("smpte2084") && dom.v_color_transfer.as_deref() != Some("arib-std-b67") {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: "HDR content detected in SDR-dominant playlist — will cause color mismatch".to_string(),
            normalization_type: NormalizationType::VideoReencode,
            dominant_value: "SDR".to_string(),
            actual_value: "HDR".to_string(),
            property: "hdr".to_string(),
        });
    }

    // Missing video stream
    if p.has_no_video {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: "File has no video stream — cannot be merged with video files".to_string(),
            normalization_type: NormalizationType::FullReencode,
            dominant_value: dom.v_codec.clone().unwrap_or_else(|| "has_video".to_string()),
            actual_value: "no_video".to_string(),
            property: "missing_video_stream".to_string(),
        });
    }

    // Missing audio stream
    if p.has_no_audio && dom.a_codec.is_some() {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: "File has no audio stream while dominant profile has audio — silence will be inserted".to_string(),
            normalization_type: NormalizationType::AudioReencode,
            dominant_value: dom.a_codec.clone().unwrap_or_else(|| "has_audio".to_string()),
            actual_value: "no_audio".to_string(),
            property: "missing_audio_stream".to_string(),
        });
    }

    // Multiple video streams
    if p.has_multiple_video_streams {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: format!("File has {} video streams — may cause stream selection issues in concat", p.video_stream_count),
            normalization_type: NormalizationType::RemuxOnly,
            dominant_value: "1 video stream".to_string(),
            actual_value: format!("{} video streams", p.video_stream_count),
            property: "multiple_video_streams".to_string(),
        });
    }

    // Multiple audio streams
    if p.has_multiple_audio_streams {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: format!("File has {} audio streams — may cause stream selection issues in concat", p.audio_stream_count),
            normalization_type: NormalizationType::RemuxOnly,
            dominant_value: "1 audio stream".to_string(),
            actual_value: format!("{} audio streams", p.audio_stream_count),
            property: "multiple_audio_streams".to_string(),
        });
    }

    // ── Intra-file audio stream mismatches ──────────────────────────────────
    // These detect when a SINGLE file has multiple audio streams with DIFFERENT properties.
    // This is critical: FFmpeg concat demuxer requires consistent stream properties
    // across all segments. If one file has stereo AAC + 5.1 AAC, the concat may fail
    // or produce garbled output depending on which stream is selected.
    if p.has_intra_audio_codec_mismatch {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: format!("File has multiple audio streams with different codecs ({} — may cause concat failures or audio dropouts",
                p.all_audio_codecs.join(", ")),
            normalization_type: NormalizationType::AudioReencode,
            dominant_value: "uniform_codec".to_string(),
            actual_value: p.all_audio_codecs.join(", "),
            property: "intra_audio_codec_mismatch".to_string(),
        });
    }
    if p.has_intra_audio_channel_mismatch {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: format!("File has audio streams with different channel counts ({} — channel mismatch may cause 'rematrix is needed' errors in lossless concat",
                p.all_audio_channels.iter().map(|c| format!("{}ch", c)).collect::<Vec<_>>().join(", ")),
            normalization_type: NormalizationType::AudioReencode,
            dominant_value: "uniform_channels".to_string(),
            actual_value: p.all_audio_channels.iter().map(|c| format!("{}ch", c)).collect::<Vec<_>>().join(", "),
            property: "intra_audio_channel_mismatch".to_string(),
        });
    }
    if p.has_intra_audio_sample_rate_mismatch {
        outliers.push(Outlier {
            index: p.index,
            path: p.path.clone(),
            reason: format!("File has audio streams with different sample rates ({} — sample rate mismatch can cause pitch/speed drift",
                p.all_audio_sample_rates.iter().map(|r| format!("{}Hz", r)).collect::<Vec<_>>().join(", ")),
            normalization_type: NormalizationType::AudioReencode,
            dominant_value: "uniform_sample_rate".to_string(),
            actual_value: p.all_audio_sample_rates.iter().map(|r| format!("{}Hz", r)).collect::<Vec<_>>().join(", "),
            property: "intra_audio_sample_rate_mismatch".to_string(),
        });
    }
}

/// Collect granular audio-specific outliers with specific AudioNormalizationType.
/// This complements the general `outliers` list by providing per-property categorization.
fn collect_audio_outliers(p: &MediaProfile, dom: &DominantProfile, audio_outliers: &mut Vec<AudioOutlier>) {
    if p.has_no_audio && dom.a_codec.is_none() { return; }

    check_audio_opt_u32(p.index, &p.path, AudioNormalizationType::SampleRateMismatch, p.a_sample_rate, dom.a_sample_rate, audio_outliers);
    check_audio_opt_u32(p.index, &p.path, AudioNormalizationType::ChannelMismatch, p.a_channels, dom.a_channels, audio_outliers);
    check_audio_opt_u32(p.index, &p.path, AudioNormalizationType::BitDepthMismatch, p.a_bit_depth, dom.a_bit_depth, audio_outliers);
    check_audio_opt_str(p.index, &p.path, AudioNormalizationType::CodecMismatch, &p.a_codec, &dom.a_codec, audio_outliers, false);
    check_audio_opt_str(p.index, &p.path, AudioNormalizationType::AACProfileMismatch, &p.a_profile, &dom.a_profile, audio_outliers, true);
    check_audio_opt_str(p.index, &p.path, AudioNormalizationType::ChannelLayoutMismatch, &p.a_channel_layout, &dom.a_channel_layout, audio_outliers, false);

    if let Some(drift) = p.audio_duration_drift_secs {
        if drift.abs() > 0.5 {
            audio_outliers.push(AudioOutlier {
                index: p.index,
                path: p.path.clone(),
                audio_type: AudioNormalizationType::DurationDriftRepair,
                dominant_value: "aligned".to_string(),
                actual_value: format!("{:+.0}ms", drift * 1000.0),
            });
        }
    }

    if p.has_no_audio && dom.a_codec.is_some() {
        audio_outliers.push(AudioOutlier {
            index: p.index,
            path: p.path.clone(),
            audio_type: AudioNormalizationType::CorruptionRepair,
            dominant_value: format!("has_audio({})", dom.a_codec.clone().unwrap_or_default()),
            actual_value: "no_audio_stream".to_string(),
        });
    }

    // ── Intra-file audio stream mismatches ──────────────────────────────────
    // When a single file has multiple audio streams with different properties,
    // FFmpeg concat may fail or produce garbled output.
    if p.has_intra_audio_codec_mismatch && p.all_audio_codecs.len() > 1 {
        audio_outliers.push(AudioOutlier {
            index: p.index,
            path: p.path.clone(),
            audio_type: AudioNormalizationType::IntraFileCodecMismatch,
            dominant_value: "uniform_codec".to_string(),
            actual_value: p.all_audio_codecs.to_vec().join(", "),
        });
    }
    if p.has_intra_audio_channel_mismatch && p.all_audio_channels.len() > 1 {
        audio_outliers.push(AudioOutlier {
            index: p.index,
            path: p.path.clone(),
            audio_type: AudioNormalizationType::IntraFileChannelMismatch,
            dominant_value: "uniform_channels".to_string(),
            actual_value: p.all_audio_channels.iter().map(|c| format!("{}ch", c)).collect::<Vec<_>>().join(", "),
        });
    }
    if p.has_intra_audio_sample_rate_mismatch && p.all_audio_sample_rates.len() > 1 {
        audio_outliers.push(AudioOutlier {
            index: p.index,
            path: p.path.clone(),
            audio_type: AudioNormalizationType::IntraFileSampleRateMismatch,
            dominant_value: "uniform_sample_rate".to_string(),
            actual_value: p.all_audio_sample_rates.iter().map(|r| format!("{}Hz", r)).collect::<Vec<_>>().join(", "),
        });
    }
}

fn check_audio_opt_str(
    idx: usize, path: &str, audio_type: AudioNormalizationType,
    actual: &Option<String>, dominant: &Option<String>,
    audio_outliers: &mut Vec<AudioOutlier>,
    is_metadata_only: bool,
) {
    match (actual, dominant) {
        (Some(a), Some(d)) if a != d => {
            audio_outliers.push(AudioOutlier {
                index: idx, path: path.to_string(), audio_type,
                dominant_value: d.clone(), actual_value: a.clone(),
            });
        }
        (None, Some(d)) if !is_metadata_only => {
            audio_outliers.push(AudioOutlier {
                index: idx, path: path.to_string(), audio_type,
                dominant_value: d.clone(), actual_value: "missing".to_string(),
            });
        }
        _ => {}
    }
}

fn check_audio_opt_u32(
    idx: usize, path: &str, audio_type: AudioNormalizationType,
    actual: Option<u32>, dominant: Option<u32>,
    audio_outliers: &mut Vec<AudioOutlier>,
) {
    match (actual, dominant) {
        (Some(a), Some(d)) if a != d => {
            audio_outliers.push(AudioOutlier {
                index: idx, path: path.to_string(), audio_type,
                dominant_value: d.to_string(), actual_value: a.to_string(),
            });
        }
        (None, Some(d)) => {
            audio_outliers.push(AudioOutlier {
                index: idx, path: path.to_string(), audio_type,
                dominant_value: d.to_string(), actual_value: "missing".to_string(),
            });
        }
        _ => {}
    }
}

/// Tier 3 fields: Metadata-only fields that should NEVER trigger normalization
/// just because their value is None (missing). These don't affect concat or playback
/// compatibility — they're cosmetic or informational metadata only.
fn is_metadata_only_field(prop: &str) -> bool {
    matches!(
        prop,
        "color_space"
            | "color_transfer"
            | "color_primaries"
            | "v_profile"     // Within same codec, rarely affects playback
            | "a_profile"     // AAC profile — MP3/Opus/etc have no profile; None vs Some is not a real mismatch
            | "a_bit_depth"   // Metadata only, doesn't affect audio rendering
            | "dar"           // Display aspect ratio, purely cosmetic
            | "rotation"      // Display orientation only
            | "audio_language" // Metadata only, doesn't affect concat
    )
}

/// Tier 2 fields: These should NOT trigger on (None vs Some) alone because
/// missing metadata is not a real mismatch. They only trigger when there's
/// an actual value mismatch (Some vs Some where values differ).
fn is_prefer_match_field(prop: &str) -> bool {
    matches!(
        prop,
        "pixel_format"
            | "v_bit_depth"
            | "container_format"
            | "field_order"
    )
}

fn check_opt_str(
    idx: usize, path: &str, prop: &str,
    actual: &Option<String>, dominant: &Option<String>,
    outliers: &mut Vec<Outlier>, norm_type: NormalizationType,
) {
    // Tier 3 fields: Never trigger on None vs Some (metadata only)
    if is_metadata_only_field(prop) {
        // Only trigger if there's an actual mismatch (Some vs Some)
        if let (Some(a), Some(d)) = (actual, dominant) {
            if a != d {
                outliers.push(Outlier {
                    index: idx,
                    path: path.to_string(),
                    reason: format!("{} mismatch: '{}' vs dominant '{}'", prop, a, d),
                    normalization_type: norm_type,
                    dominant_value: d.clone(),
                    actual_value: a.clone(),
                    property: prop.to_string(),
                });
            }
        }
        return;
    }

    match (actual, dominant) {
        (Some(a), Some(d)) if a != d => {
            outliers.push(Outlier {
                index: idx,
                path: path.to_string(),
                reason: format!("{} mismatch: '{}' vs dominant '{}'", prop, a, d),
                normalization_type: norm_type,
                dominant_value: d.clone(),
                actual_value: a.clone(),
                property: prop.to_string(),
            });
        }
        (None, Some(_d)) if is_prefer_match_field(prop) => {
            // Tier 2 fields: Missing metadata is not a mismatch
            // Only trigger on actual value mismatch
        }
        (None, Some(d)) => {
            outliers.push(Outlier {
                index: idx,
                path: path.to_string(),
                reason: format!("{} missing (dominant is '{}')", prop, d),
                normalization_type: norm_type,
                dominant_value: d.clone(),
                actual_value: "None".to_string(),
                property: prop.to_string(),
            });
        }
        _ => {}
    }
}

fn check_opt_string(
    idx: usize, path: &str, prop: &str,
    actual: &Option<String>, dominant: &Option<String>,
    outliers: &mut Vec<Outlier>, norm_type: NormalizationType,
) {
    check_opt_str(idx, path, prop, actual, dominant, outliers, norm_type);
}

fn check_opt_u32(
    idx: usize, path: &str, prop: &str,
    actual: Option<u32>, dominant: Option<u32>,
    outliers: &mut Vec<Outlier>, norm_type: NormalizationType,
) {
    // Tier 3 fields: Never trigger on None vs Some
    if is_metadata_only_field(prop) {
        // Only trigger if there's an actual mismatch (Some vs Some)
        if let (Some(a), Some(d)) = (actual, dominant) {
            if a != d {
                outliers.push(Outlier {
                    index: idx,
                    path: path.to_string(),
                    reason: format!("{} mismatch: {} vs dominant {}", prop, a, d),
                    normalization_type: norm_type,
                    dominant_value: d.to_string(),
                    actual_value: a.to_string(),
                    property: prop.to_string(),
                });
            }
        }
        return;
    }

    match (actual, dominant) {
        (Some(a), Some(d)) if a != d => {
            outliers.push(Outlier {
                index: idx,
                path: path.to_string(),
                reason: format!("{} mismatch: {} vs dominant {}", prop, a, d),
                normalization_type: norm_type,
                dominant_value: d.to_string(),
                actual_value: a.to_string(),
                property: prop.to_string(),
            });
        }
        (None, Some(_d)) if is_prefer_match_field(prop) => {
            // Tier 2 fields: Missing metadata is not a mismatch
            // Only trigger on actual value mismatch
        }
        (None, Some(d)) => {
            outliers.push(Outlier {
                index: idx,
                path: path.to_string(),
                reason: format!("{} missing (dominant is {})", prop, d),
                normalization_type: norm_type,
                dominant_value: d.to_string(),
                actual_value: "None".to_string(),
                property: prop.to_string(),
            });
        }
        _ => {}
    }
}

// ── Configuration Constants for SmartMKV ───────────────────────────────────────

/// FPS comparison tolerance as percentage of dominant FPS.
/// 0.5% allows:
///   - 29.970 ≈ 30.000 (NTSC variant)
///   - 23.976 ≈ 24.000 (film variant)
///   - 59.940 ≈ 60.000 (NTSC progressive)
/// But rejects:
///   - 24 vs 60 (genuinely different)
const FPS_TOLERANCE_PERCENT: f64 = 0.5;

/// Check FPS with NUMERIC tolerance instead of exact string comparison.
///
/// Logs the decision for forensic traceability:
/// ```
/// [FPS_CHECK] File #18
///   Source FPS: 29.970
///   Target FPS: 30.000
///   Difference: 0.10%
///   Tolerance: 0.50%
///   Decision: COMPATIBLE (within tolerance)
/// ```
fn check_fps_tolerance(
    idx: usize, path: &str,
    actual: Option<f64>, dominant: Option<f64>,
    outliers: &mut Vec<Outlier>,
) {
    match (actual, dominant) {
        (Some(a), Some(d)) => {
            let diff = (a - d).abs();
            let threshold = d * (FPS_TOLERANCE_PERCENT / 100.0);
            let diff_percent = if d > 0.0 { (diff / d) * 100.0 } else { 0.0 };

            let filename = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");

            if diff > threshold {
                // FPS mismatch - trigger normalization
                log::info!("[FPS_CHECK] File #{} ({})", idx, filename);
                log::info!("[FPS_CHECK]   Source FPS: {:.3}", a);
                log::info!("[FPS_CHECK]   Target FPS: {:.3}", d);
                log::info!("[FPS_CHECK]   Difference: {:.3}s ({:.3}%)", diff, diff_percent);
                log::info!("[FPS_CHECK]   Tolerance: {:.3}s ({:.2}%)", threshold, FPS_TOLERANCE_PERCENT);
                log::info!("[FPS_CHECK]   Decision: REQUIRES NORMALIZATION");

                outliers.push(Outlier {
                    index: idx,
                    path: path.to_string(),
                    reason: format!(
                        "fps mismatch: {:.3} vs {:.3} (diff={:.3}s, {:.2}%, threshold={:.3}s)",
                        a, d, diff, diff_percent, threshold
                    ),
                    normalization_type: NormalizationType::VideoReencode,
                    dominant_value: format!("{:.3}", d),
                    actual_value: format!("{:.3}", a),
                    property: "fps".to_string(),
                });
            } else {
                // FPS within tolerance - considered compatible
                log::info!("[FPS_CHECK] File #{} ({})", idx, filename);
                log::info!("[FPS_CHECK]   Source FPS: {:.3}", a);
                log::info!("[FPS_CHECK]   Target FPS: {:.3}", d);
                log::info!("[FPS_CHECK]   Difference: {:.3}s ({:.3}%)", diff, diff_percent);
                log::info!("[FPS_CHECK]   Tolerance: {:.3}s ({:.2}%)", threshold, FPS_TOLERANCE_PERCENT);
                log::info!("[FPS_CHECK]   Decision: COMPATIBLE (no normalization needed)");
            }
        }
        (None, Some(d)) => {
            // Missing FPS is not a critical mismatch - log but don't trigger
            log::info!("[FPS_CHECK] File #{} - Source FPS unknown, Target FPS: {:.3} - COMPATIBLE (FPS not required for mkvmerge)", idx, d);
        }
        _ => {}
    }
}

/// Generate a human-readable audit report.
pub fn format_audit_report(analysis: &ProfileAnalysis) -> String {
    let mut report = String::new();

    report.push_str("═══════════════════════════════════════════════════════════════\n");
    report.push_str("                 MEDIA PROFILE AUDIT REPORT\n");
    report.push_str("═══════════════════════════════════════════════════════════════\n\n");

    report.push_str(&format!("Total files: {}\n", analysis.dominant.total_count));
    report.push_str(&format!("Matching dominant profile: {}\n", analysis.dominant.match_count));
    report.push_str(&format!("Outliers requiring normalization: {}\n\n", analysis.outliers.len()));

    report.push_str("─── DOMINANT PROFILE ───────────────────────────────────────────\n");
    report.push_str(&format!("  Video Codec:       {:?}\n", analysis.dominant.v_codec));
    report.push_str(&format!("  Video Profile:     {:?}\n", analysis.dominant.v_profile));
    report.push_str(&format!("  Resolution:        {:?}x{:?}\n", analysis.dominant.v_width, analysis.dominant.v_height));
    report.push_str(&format!("  FPS:               {:?}\n", analysis.dominant.v_fps));
    report.push_str(&format!("  Timebase:          {:?}\n", analysis.dominant.v_time_base));
    report.push_str(&format!("  Pixel Format:      {:?}\n", analysis.dominant.v_pixel_format));
    report.push_str(&format!("  Color Space:       {:?}\n", analysis.dominant.v_color_space));
    report.push_str(&format!("  Audio Codec:       {:?}\n", analysis.dominant.a_codec));
    report.push_str(&format!("  Audio Profile:     {:?}\n", analysis.dominant.a_profile));
    report.push_str(&format!("  Sample Rate:       {:?} Hz\n", analysis.dominant.a_sample_rate));
    report.push_str(&format!("  Channels:          {:?}\n", analysis.dominant.a_channels));
    report.push_str(&format!("  Channel Layout:    {:?}\n", analysis.dominant.a_channel_layout));
    report.push_str(&format!("  Bit Depth:         {:?} bit\n", analysis.dominant.a_bit_depth));
    report.push_str(&format!("  Timescale:         {:?}\n", analysis.dominant.timescale_den));
    report.push_str(&format!("  Rotation:          {:?}\n", analysis.dominant.v_rotation));
    report.push_str(&format!("  Audio Language:   {:?}\n", analysis.dominant.a_language));
    report.push_str(&format!("  Container:        {:?}\n", analysis.dominant.container_format));
    report.push_str(&format!("  DAR:              {:?}\n", analysis.dominant.dar));
    report.push_str(&format!("  VFR files:         {}\n", analysis.histograms.vfr_count));
    report.push_str(&format!("  Interlaced files:  {}\n", analysis.histograms.interlaced_count));
    report.push_str(&format!("  HDR files:         {}\n", analysis.histograms.hdr_count));
    report.push_str(&format!("  Missing video:     {}\n", analysis.histograms.no_video_count));
    report.push_str(&format!("  Missing audio:     {}\n", analysis.histograms.no_audio_count));
    report.push_str(&format!("  Multi-video:       {}\n", analysis.histograms.multi_video_count));
    report.push_str(&format!("  Multi-audio:       {}\n\n", analysis.histograms.multi_audio_count));

    report.push_str("─── HISTOGRAMS ─────────────────────────────────────────────────\n");

    report.push_str("\n  Video Codec:\n");
    for (val, count) in &analysis.histograms.v_codec {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Resolution:\n");
    for (val, count) in &analysis.histograms.v_resolution {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  FPS:\n");
    for (val, count) in &analysis.histograms.v_fps {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Timebase:\n");
    for (val, count) in &analysis.histograms.v_time_base {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Pixel Format:\n");
    for (val, count) in &analysis.histograms.v_pixel_format {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio Codec:\n");
    for (val, count) in &analysis.histograms.a_codec {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio Sample Rate:\n");
    for (val, count) in &analysis.histograms.a_sample_rate {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio Channels:\n");
    for (val, count) in &analysis.histograms.a_channels {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio Channel Layout:\n");
    for (val, count) in &analysis.histograms.a_channel_layout {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio Bit Depth:\n");
    for (val, count) in &analysis.histograms.a_bit_depth {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio AAC Profile:\n");
    for (val, count) in &analysis.histograms.a_profile {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio Duration Drift:\n");
    for (val, count) in &analysis.histograms.audio_duration_drift {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio Language:\n");
    for (val, count) in &analysis.histograms.audio_language {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Container Format:\n");
    for (val, count) in &analysis.histograms.container_format {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Rotation:\n");
    for (val, count) in &analysis.histograms.rotation {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Audio Start Offset:\n");
    for (val, count) in &analysis.histograms.audio_start_offset {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Display Aspect Ratio (DAR):\n");
    for (val, count) in &analysis.histograms.dar {
        report.push_str(&format!("    {:<30} {:>5} files\n", val, count));
    }

    report.push_str("\n  Stream Presence:\n");
    report.push_str(&format!("    {:<30} {:>5} files\n", "HDR content", analysis.histograms.hdr_count));
    report.push_str(&format!("    {:<30} {:>5} files\n", "Missing video stream", analysis.histograms.no_video_count));
    report.push_str(&format!("    {:<30} {:>5} files\n", "Missing audio stream", analysis.histograms.no_audio_count));
    report.push_str(&format!("    {:<30} {:>5} files\n", "Multiple video streams", analysis.histograms.multi_video_count));
    report.push_str(&format!("    {:<30} {:>5} files\n", "Multiple audio streams", analysis.histograms.multi_audio_count));

    if !analysis.outliers.is_empty() {
        report.push_str("\n─── OUTLIERS ───────────────────────────────────────────────────\n");
        for o in &analysis.outliers {
            report.push_str(&format!("\n  [{}] {}\n", o.index, std::path::Path::new(&o.path).file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| o.path.clone())));
            report.push_str(&format!("    Property:   {}\n", o.property));
            report.push_str(&format!("    Actual:     {}\n", o.actual_value));
            report.push_str(&format!("    Dominant:   {}\n", o.dominant_value));
            report.push_str(&format!("    Action:     {:?}\n", o.normalization_type));
            report.push_str(&format!("    Reason:     {}\n", o.reason));
        }

        if !analysis.audio_outliers.is_empty() {
            report.push_str("\n─── AUDIO OUTLIERS (GRANULAR) ──────────────────────────────────\n");
            for ao in &analysis.audio_outliers {
                report.push_str(&format!("\n  [{}] {}\n", ao.index, std::path::Path::new(&ao.path).file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| ao.path.clone())));
                report.push_str(&format!("    Audio Type: {}\n", ao.audio_type.label()));
                report.push_str(&format!("    Actual:     {}\n", ao.actual_value));
                report.push_str(&format!("    Dominant:   {}\n", ao.dominant_value));
            }
        }
    }

    report.push_str("\n═══════════════════════════════════════════════════════════════\n");
    report
}

/// Intrinsic file health — does not depend on playlist or merge mode.
/// Derived from ffprobe output and quick structural checks.
/// Confidence score reflects certainty of classification (0-100).
#[derive(Debug, Clone, PartialEq)]
pub enum FileHealthStatus {
    /// All checks passed. Confidence 100 if ffprobe + seek test clean.
    Healthy { confidence: u8 },
    /// File plays normally but seeking has artifacts (MPEG-TS decoder warnings).
    /// Common in MPEG-TS streams. Threshold: < 50% of seek points fail.
    /// Confidence reflects artifact severity (55-75).
    HealthyWithWarnings { errors: Vec<String>, fail_count: usize, total_count: usize, confidence: u8 },
    /// File plays normally but seeking has more significant issues.
    /// Threshold: 50%+ of seek points fail but file is readable.
    /// Confidence 60-75.
    SeekabilityIssue { errors: Vec<String>, fail_count: usize, total_count: usize, confidence: u8 },
    /// Metadata read issue (0x0 resolution, invalid fps, etc.).
    /// File is readable but ffprobe returned clearly invalid metadata.
    /// Confidence 95.
    MinorMetadataIssue { field: String, raw_value: String, fallback: String, confidence: u8 },
    /// Cannot be read at all (missing file, permission, unsupported codec).
    /// Confidence 100 (binary).
    Unreadable { reason: String, confidence: u8 },
    /// Real structural damage detected (PTS discontinuity, broken container).
    /// File should not be merged. Confidence 90-95.
    Corrupted { reason: String, first_error: String, confidence: u8 },
}

impl FileHealthStatus {
    pub fn can_merge_lossless(&self) -> bool {
        matches!(self, FileHealthStatus::Healthy { .. })
    }
    pub fn can_merge_custom(&self) -> bool {
        !matches!(self, FileHealthStatus::Unreadable { .. } | FileHealthStatus::Corrupted { .. })
    }
    pub fn auto_repair_applies(&self) -> bool {
        matches!(self, FileHealthStatus::HealthyWithWarnings { .. }
                 | FileHealthStatus::SeekabilityIssue { .. }
                 | FileHealthStatus::MinorMetadataIssue { .. })
    }
    pub fn is_healthy(&self) -> bool {
        matches!(self, FileHealthStatus::Healthy { .. })
    }
}

/// Result of a file health check — combines status with classification metadata.
#[derive(Debug, Clone)]
pub struct FileHealth {
    pub index: usize,
    pub path: String,
    pub status: FileHealthStatus,
}

impl FileHealth {
    pub fn healthy(index: usize, path: String) -> Self {
        Self { index, path, status: FileHealthStatus::Healthy { confidence: 100 } }
    }
    pub fn healthy_with_warnings(index: usize, path: String, errors: Vec<String>, fail_count: usize, total_count: usize) -> Self {
        Self { index, path, status: FileHealthStatus::HealthyWithWarnings { errors, fail_count, total_count, confidence: 70 } }
    }
    pub fn seekability_issue(index: usize, path: String, errors: Vec<String>, fail_count: usize, total_count: usize) -> Self {
        Self { index, path, status: FileHealthStatus::SeekabilityIssue { errors, fail_count, total_count, confidence: 65 } }
    }
    pub fn minor_metadata(index: usize, path: String, field: String, raw_value: String, fallback: String) -> Self {
        Self { index, path, status: FileHealthStatus::MinorMetadataIssue { field, raw_value, fallback, confidence: 95 } }
    }
    pub fn unreadable(index: usize, path: String, reason: String) -> Self {
        Self { index, path, status: FileHealthStatus::Unreadable { reason, confidence: 100 } }
    }
    pub fn corrupted(index: usize, path: String, reason: String, first_error: String) -> Self {
        Self { index, path, status: FileHealthStatus::Corrupted { reason, first_error, confidence: 95 } }
    }
}

impl From<FileHealthStatus> for bool {
    fn from(status: FileHealthStatus) -> bool {
        !matches!(status, FileHealthStatus::Healthy { .. })
    }
}

/// Check a single file for structural integrity using ffprobe.
/// This is a fast header-only check that can detect UNREADABLE and CORRUPTED files.
/// It does NOT perform seeking, so it cannot detect MPEG-TS seeking artifacts
/// (those require ffmpeg seek tests — see validate_audio_streams_parallel).
///
/// Returns FileHealth with a status appropriate to what ffprobe reported.
pub fn check_file_corruption_quick(
    ffprobe_path: &std::path::Path,
    index: usize,
    file_path: &str,
) -> FileHealth {
    let start = std::time::Instant::now();
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = std::process::Command::new(ffprobe_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .args(["-v", "error", "-i", file_path])
        .output();

    let filename = std::path::Path::new(file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string());

    let health = match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !out.status.success() {
                let reason = if stderr.contains("No such file") || stderr.contains("does not exist") {
                    "file not found"
                } else if stderr.contains("Permission denied") {
                    "permission denied"
                } else if stderr.contains("Invalid data") || stderr.contains("moov atom not found") {
                    "corrupt container"
                } else {
                    "probe failed"
                };
                FileHealth::unreadable(index, file_path.to_string(), reason.to_string())
            } else if !stderr.is_empty() {
                let err_lines: Vec<String> = stderr.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect();
                FileHealth::corrupted(
                    index, file_path.to_string(),
                    format!("ffprobe stderr warnings: {}", stderr.trim()),
                    err_lines.first().cloned().unwrap_or_default(),
                )
            } else {
                FileHealth::healthy(index, file_path.to_string())
            }
        }
        Err(e) => {
            let reason = if e.kind() == std::io::ErrorKind::NotFound {
                "ffprobe not found"
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                "permission denied"
            } else {
                "cannot execute ffprobe"
            };
            FileHealth::unreadable(index, file_path.to_string(), format!("{}: {}", reason, e))
        }
    };

    log::info!("[FileHealth] [{}] {} | {:?} ({:.1}ms)",
        index, filename, health.status, start.elapsed().as_secs_f64() * 1000.0);

    health
}

/// Check a batch of files for structural integrity in parallel using tokio Semaphore (max 6 concurrent).
/// Returns summary statistics.
///
/// Note: This performs ffprobe header checks only — it cannot detect seeking artifacts
/// (MPEG-TS PPS errors) which require ffmpeg seek tests via validate_audio_streams_parallel.
///
/// Accepts an optional progress callback: `on_progress(completed_count, total_count)`
pub async fn check_batch_corruption_parallel<F>(
    ffprobe_path: &std::path::Path,
    files: &[(usize, String)],
    on_progress: Option<F>,
) -> (Vec<FileHealth>, usize, Vec<String>)
where
    F: Fn(usize, usize) + Send + Sync + 'static,
{
    log::info!("[FORENSIC:VALIDATE] ENTER check_batch_corruption_parallel ({} files)", files.len());
    if files.is_empty() {
        return (vec![], 0, vec![]);
    }

    let ffprobe = std::sync::Arc::new(ffprobe_path.to_path_buf());
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(6));
    let total = files.len();
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let all_errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let bad_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let on_progress = std::sync::Arc::new(on_progress);

    let handles: Vec<_> = files.iter().map(|(idx, path)| {
        let sem = semaphore.clone();
        let ffprobe = ffprobe.clone();
        let idx = *idx;
        let path = path.clone();
        let results = results.clone();
        let all_errors = all_errors.clone();
        let bad_count = bad_count.clone();
        let completed = completed.clone();
        let on_progress = on_progress.clone();

        tokio::spawn(async move {
            let _permit = match sem.acquire().await {
                        Ok(p) => p,
                        Err(_) => {
                            log::error!("[Normalization] Semaphore closed during corruption check");
                            return;
                        }
                    };
            let path_for_err = path.clone();
            let health = tokio::task::spawn_blocking(move || {
                check_file_corruption_quick(&ffprobe, idx, &path)
            }).await.unwrap_or_else(|e| {
                        log::error!("[Normalization] Corruption check task panicked for file #{}: {}", idx, e);
                        FileHealth::unreadable(idx, path_for_err, format!("Corruption check task panicked: {}", e))
                    });

            if !health.status.is_healthy() {
                bad_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut errs) = all_errors.lock() {
                    match &health.status {
                        FileHealthStatus::Unreadable { reason, .. } => errs.push(reason.clone()),
                        FileHealthStatus::Corrupted { reason, .. } => errs.push(reason.clone()),
                        _ => {}
                    }
                }
            }
            if let Ok(mut res) = results.lock() {
                res.push(health);
            }

            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if let Some(ref cb) = on_progress.as_ref() {
                cb(done, total);
            }
        })
    }).collect();

    for handle in handles {
        if let Err(e) = handle.await {
            log::error!("[FileHealth] Corrupt check task panicked: {} — file health result may be missing", e);
        }
    }

    let results = std::sync::Arc::into_inner(results)
        .map(|arc| arc.into_inner().unwrap_or_default())
        .unwrap_or_default();
    let all_errors = std::sync::Arc::into_inner(all_errors)
        .map(|arc| arc.into_inner().unwrap_or_default())
        .unwrap_or_default();
    let bad = bad_count.load(std::sync::atomic::Ordering::Relaxed);

    log::info!("[FileHealth] Batch complete: {} problematic out of {} files", bad, total);

    (results, bad, all_errors)
}

/// PTS/DTS continuity check result for a single file.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PtsContinuityResult {
    pub index: usize,
    pub path: String,
    pub has_negative_pts: bool,
    pub v_audio_start_gap_secs: Option<f64>,
    pub issues: Vec<String>,
}

/// Lightweight PTS/DTS continuity check using stream-level metadata.
///
/// This checks:
/// 1. Negative PTS (any stream with start_pts < 0)
/// 2. Audio/video start time gap (audio starts before video or vice versa)
/// 3. Format-level start_time vs stream-level start_time discrepancies
#[allow(dead_code)]
pub fn check_pts_continuity_lightweight(
    info: &crate::types::MediaInfo,
    index: usize,
) -> PtsContinuityResult {
    let mut issues = Vec::new();
    let mut has_negative_pts = false;
    let mut v_audio_start_gap_secs = None;

    // Check for negative start_pts on video streams
    for vs in &info.video_streams {
        if let Some(start_time) = vs.duration {
            if start_time < 0.0 {
                has_negative_pts = true;
                issues.push(format!("Video stream {} has negative duration: {}", vs.stream_index, start_time));
            }
        }
    }

    // Check for negative start_pts on audio streams
    for as_ in &info.audio_streams {
        if let Some(start_time) = as_.start_time {
            if start_time < 0.0 {
                has_negative_pts = true;
                issues.push(format!("Audio stream {} has negative start_time: {}s", as_.stream_index, start_time));
            }
        }
    }

    // Check audio/video start time gap
    let v_start = info.start_time.unwrap_or(0.0);

    let a_start = info.audio_streams.first()
        .and_then(|a| a.start_time)
        .unwrap_or(0.0);

    let gap = a_start - v_start;
    if gap.abs() > 0.5 {
        v_audio_start_gap_secs = Some(gap);
        if gap > 0.0 {
            issues.push(format!("Audio starts {:.1}s after video — possible sync issue", gap));
        } else {
            issues.push(format!("Audio starts {:.1}s before video — possible sync issue", -gap));
        }
    }

    PtsContinuityResult {
        index,
        path: info.path.clone(),
        has_negative_pts,
        v_audio_start_gap_secs,
        issues,
    }
}

/// Result of a subtitle file check.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SubtitleCheckResult {
    pub index: usize,
    pub subtitle_path: String,
    pub is_valid: bool,
    pub cue_count: usize,
    pub negative_timestamps: Vec<String>,
    pub overlapping_cues: Vec<(usize, usize)>,
    pub broken_numbering: Vec<usize>,
    pub malformed_timestamps: Vec<String>,
    pub errors: Vec<String>,
}

/// Check a single SRT subtitle file for common issues.
///
/// Checks:
/// - Negative timestamps
/// - Overlapping cues
/// - Broken numbering (non-sequential, gaps)
/// - Malformed timestamp format
/// - Very large timestamp gaps (>10s between sequential cues)
#[allow(dead_code)]
pub fn check_subtitle_file(
    index: usize,
    subtitle_path: &str,
) -> SubtitleCheckResult {
    log::info!("[FORENSIC:SUB] ENTER check_subtitle_file | File #{} ({})", index, subtitle_path);
    let mut negative_timestamps = Vec::new();
    let mut overlapping_cues = Vec::new();
    let mut broken_numbering = Vec::new();
    let mut malformed_timestamps = Vec::new();
    let mut errors = Vec::new();

    let content = match std::fs::read_to_string(subtitle_path) {
        Ok(c) => c,
        Err(e) => {
            errors.push(format!("Cannot read subtitle file: {}", e));
            return SubtitleCheckResult {
                index,
                subtitle_path: subtitle_path.to_string(),
                is_valid: false,
                cue_count: 0,
                negative_timestamps,
                overlapping_cues,
                broken_numbering,
                malformed_timestamps,
                errors,
            };
        }
    };

    // Parse SRT format
    let mut cues: Vec<(usize, f64, f64)> = Vec::new(); // (number, start_secs, end_secs)
    let mut expected_number = 1;
    let mut in_cue = false;
    let mut current_number = 0;
    let mut current_start = 0.0;
    let mut current_end = 0.0;
    let mut collected_text = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if in_cue && collected_text {
                // End of this cue
                cues.push((current_number, current_start, current_end));
                in_cue = false;
                collected_text = false;
            }
            continue;
        }

        if !in_cue {
            // Expect cue number
            if let Ok(num) = trimmed.parse::<usize>() {
                current_number = num;
                if num != expected_number {
                    broken_numbering.push(expected_number);
                }
                expected_number = num + 1;
                in_cue = true;
            } else {
                // Skip lines that aren't cue numbers (comment lines, etc.)
                // Check if it looks like a timestamp line (contains "-->")
                if trimmed.contains("-->") {
                    // No preceding number — broken numbering
                    broken_numbering.push(expected_number);
                    // Parse as timestamp line directly
                    if let Some((start, end)) = parse_srt_timestamp(trimmed) {
                        current_start = start;
                        current_end = end;
                        in_cue = true;
                        collected_text = true; // Will be collected on next empty line
                    } else {
                        malformed_timestamps.push(trimmed.to_string());
                    }
                }
            }
        } else if trimmed.contains("-->") {
            // Timestamp line
            if let Some((start, end)) = parse_srt_timestamp(trimmed) {
                current_start = start;
                current_end = end;

                // Check for negative timestamps
                if start < 0.0 {
                    negative_timestamps.push(format!("Cue {}: start={}s", current_number, start));
                }
                if end < 0.0 {
                    negative_timestamps.push(format!("Cue {}: end={}s", current_number, end));
                }

                // Check for malformed end < start
                if end <= start {
                    malformed_timestamps.push(format!("Cue {}: end ({}) <= start ({})", current_number, end, start));
                }
            } else {
                malformed_timestamps.push(trimmed.to_string());
            }
        } else {
            // Text content line
            collected_text = true;
        }
    }

    // Don't forget last cue
    if in_cue && collected_text {
        cues.push((current_number, current_start, current_end));
    }

    // Check for overlapping cues
    for i in 1..cues.len() {
        let prev = &cues[i - 1];
        let curr = &cues[i];
        if curr.1 < prev.2 {
            overlapping_cues.push((prev.0, curr.0));
        }
        // Check for large gaps (>10 seconds)
        if curr.1 - prev.2 > 10.0 {
            errors.push(format!(
                "Large gap ({:.0}s) between cue {} and cue {}",
                curr.1 - prev.2, prev.0, curr.0
            ));
        }
    }

    let is_valid = negative_timestamps.is_empty()
        && overlapping_cues.is_empty()
        && broken_numbering.is_empty()
        && malformed_timestamps.is_empty();

    log::info!("[FORENSIC:SUB] EXIT check_subtitle_file | valid: {} | cues: {}", is_valid, cues.len());
    SubtitleCheckResult {
        index,
        subtitle_path: subtitle_path.to_string(),
        is_valid,
        cue_count: cues.len(),
        negative_timestamps,
        overlapping_cues,
        broken_numbering,
        malformed_timestamps,
        errors,
    }
}

/// Parse an SRT timestamp line like "00:01:23,456 --> 00:01:25,678"
/// Returns (start_seconds, end_seconds).
#[allow(dead_code)]
fn parse_srt_timestamp(line: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return None;
    }

    let start = parse_srt_timecode(parts[0].trim())?;
    let end = parse_srt_timecode(parts[1].trim())?;
    Some((start, end))
}

/// Parse a single SRT timecode like "00:01:23,456" or "00:01:23.456"
#[allow(dead_code)]
fn parse_srt_timecode(tc: &str) -> Option<f64> {
    let tc = tc.replace(',', ".");
    let parts: Vec<&str> = tc.split(':').collect();
    match parts.len() {
        3 => {
            let h: f64 = parts[0].parse().ok()?;
            let m: f64 = parts[1].parse().ok()?;
            let s: f64 = parts[2].parse().ok()?;
            Some(h * 3600.0 + m * 60.0 + s)
        }
        _ => None,
    }
}

/// Format seconds as SRT timecode "HH:MM:SS,mmm"
fn format_srt_timecode(secs: f64) -> String {
    let total_ms = (secs * 1000.0).round() as i64;
    let h = total_ms / 3600000;
    let m = (total_ms % 3600000) / 60000;
    let s = (total_ms % 60000) / 1000;
    let ms = total_ms % 1000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, ms)
}

/// Internal struct for a parsed SRT cue with preserved text.
#[derive(Debug, Clone)]
struct SrtCue {
    number: usize,
    start_secs: f64,
    end_secs: f64,
    text: String,
}

/// Parse an SRT file into a vector of SrtCue preserving all text content.
fn parse_srt_file(content: &str) -> Vec<SrtCue> {
    let mut cues: Vec<SrtCue> = Vec::new();
    let mut current_number: usize = 0;
    let mut current_start: f64 = 0.0;
    let mut current_end: f64 = 0.0;
    let mut text_lines: Vec<String> = Vec::new();
    let mut in_cue = false;
    let mut found_timestamp = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if in_cue && found_timestamp {
                cues.push(SrtCue {
                    number: current_number,
                    start_secs: current_start,
                    end_secs: current_end,
                    text: text_lines.join("\n"),
                });
                text_lines.clear();
                in_cue = false;
                found_timestamp = false;
            }
            continue;
        }

        if !in_cue {
            if let Ok(num) = trimmed.parse::<usize>() {
                current_number = num;
                in_cue = true;
                text_lines.clear();
            }
        } else if trimmed.contains("-->") {
            if let Some((start, end)) = parse_srt_timestamp(trimmed) {
                current_start = start;
                current_end = end;
                found_timestamp = true;
            }
        } else if found_timestamp {
            text_lines.push(trimmed.to_string());
        }
    }

    if in_cue && found_timestamp {
        cues.push(SrtCue {
            number: current_number,
            start_secs: current_start,
            end_secs: current_end,
            text: text_lines.join("\n"),
        });
    }

    cues
}

/// Serialize cues back to SRT format.
fn serialize_srt(cues: &[SrtCue]) -> String {
    let mut output = String::new();
    for (i, cue) in cues.iter().enumerate() {
        output.push_str(&format!("{}\n{} --> {}\n{}\n\n",
            i + 1,
            format_srt_timecode(cue.start_secs),
            format_srt_timecode(cue.end_secs),
            cue.text));
    }
    output
}

/// Repair subtitle file issues (negative timestamps, overlapping cues).
///
/// Returns `Ok(Some(repaired_path))` if repairs were needed,
/// `Ok(None)` if no repairs were needed.
#[allow(dead_code)]
pub fn repair_subtitle_file(
    subtitle_path: &str,
    temp_dir: &std::path::Path,
    job_id: &str,
    index: usize,
) -> Result<Option<String>, String> {
    let content = std::fs::read_to_string(subtitle_path)
        .map_err(|e| format!("Cannot read subtitle {}: {}", subtitle_path, e))?;

    let mut cues = parse_srt_file(&content);
    if cues.is_empty() {
        return Ok(None);
    }

    let mut repaired = false;

    // Fix negative timestamps — clamp to 0.0
    for cue in &mut cues {
        if cue.start_secs < 0.0 {
            log::info!("[SubtitleRepair] [{}] Cue {}: start {:.3}s → 0.0s (negative timestamp)", index, cue.number, cue.start_secs);
            cue.start_secs = 0.0;
            repaired = true;
        }
        if cue.end_secs < 0.0 {
            log::info!("[SubtitleRepair] [{}] Cue {}: end {:.3}s → 0.0s (negative timestamp)", index, cue.number, cue.end_secs);
            cue.end_secs = 0.0;
            repaired = true;
        }
        if cue.end_secs <= cue.start_secs {
            cue.end_secs = cue.start_secs + 1.0;
            log::info!("[SubtitleRepair] [{}] Cue {}: end ≤ start, adjusted to {:.3}s–{:.3}s", index, cue.number, cue.start_secs, cue.end_secs);
            repaired = true;
        }
    }

    // Fix overlapping cues — trim previous end to current start
    for i in 1..cues.len() {
        let prev_end = cues[i - 1].end_secs;
        let curr_start = cues[i].start_secs;
        if curr_start < prev_end {
            let new_end = curr_start - 0.001;
            let clamped_end = if new_end < cues[i - 1].start_secs {
                cues[i - 1].start_secs
            } else {
                new_end
            };
            log::info!("[SubtitleRepair] [{}] Cue {} end {:.3}s → {:.3}s (overlap with Cue {})",
                index, cues[i - 1].number, cues[i - 1].end_secs, clamped_end, cues[i].number);
            cues[i - 1].end_secs = clamped_end;
            repaired = true;
        }
    }

    if !repaired {
        return Ok(None);
    }

    let output_name = format!("repaired_sub_{}_{}.srt", job_id, index);
    let output_path = temp_dir.join(&output_name);
    let output_str = output_path.to_string_lossy().into_owned();
    let serialized = serialize_srt(&cues);
    std::fs::write(&output_path, &serialized)
        .map_err(|e| format!("Failed to write repaired subtitle {}: {}", output_name, e))?;

    log::info!("[SubtitleRepair] [{}] Repaired subtitle saved to {}", index, output_str);
    Ok(Some(output_str))
}

/// Smart MKV analysis breakdown for the dashboard UI.
/// Categorizes each outlier property into normalize, remux, or skip.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SmartMkvBreakdown {
    pub normalize: Vec<PropertyCount>,
    pub remux: Vec<PropertyCount>,
    pub skip: Vec<PropertyCount>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PropertyCount {
    pub property: String,
    pub count: usize,
}

/// Compute the Smart MKV breakdown: which properties get normalized, remuxed, or skipped.
/// Uses the same filter logic as `filter_outliers_for_mkv` and returns a categorized summary.
pub fn compute_smart_mkv_breakdown(analysis: &ProfileAnalysis) -> SmartMkvBreakdown {
    use std::collections::HashMap;

    let (filtered_outliers, _filtered_audio_outliers) = filter_outliers_for_mkv(analysis, MergeBackend::MkvMerge);

    let kept_normalize: std::collections::HashSet<usize> = filtered_outliers.iter()
        .map(|o| o.index).collect();

    let mut normalize_props: HashMap<String, usize> = HashMap::new();
    let mut remux_props: HashMap<String, usize> = HashMap::new();
    let mut skip_props: HashMap<String, usize> = HashMap::new();

    for o in &analysis.outliers {
        if kept_normalize.contains(&o.index) {
            let prop = o.property.as_str();
            if prop == "time_base" {
                *remux_props.entry("Timebase".to_string()).or_insert(0) += 1;
            } else {
                *normalize_props.entry(friendly_property_name(prop)).or_insert(0) += 1;
            }
        } else {
            *skip_props.entry(friendly_property_name(&o.property)).or_insert(0) += 1;
        }
    }

    let mut normalize: Vec<PropertyCount> = normalize_props.into_iter()
        .map(|(p, c)| PropertyCount { property: p, count: c })
        .collect();
    normalize.sort_by_key(|b| std::cmp::Reverse(b.count));

    let mut remux: Vec<PropertyCount> = remux_props.into_iter()
        .map(|(p, c)| PropertyCount { property: p, count: c })
        .collect();
    remux.sort_by_key(|b| std::cmp::Reverse(b.count));

    let mut skip: Vec<PropertyCount> = skip_props.into_iter()
        .map(|(p, c)| PropertyCount { property: p, count: c })
        .collect();
    skip.sort_by_key(|b| std::cmp::Reverse(b.count));

    SmartMkvBreakdown { normalize, remux, skip }
}

/// Map internal property names to user-friendly labels.
fn friendly_property_name(prop: &str) -> String {
    match prop {
        "v_codec" => "Video codec",
        "v_profile" => "Video profile",
        "resolution" => "Resolution",
        "fps" => "FPS",
        "pixel_format" => "Pixel format",
        "color_space" => "Color space",
        "color_transfer" => "Color transfer",
        "v_bit_depth" => "Bit depth",
        "field_order" => "Interlacing",
        "rotation" => "Rotation",
        "dar" => "DAR",
        "hdr" => "HDR",
        "frame_rate_type" => "VFR",
        "a_sample_rate" => "Sample rate",
        "a_channels" => "Audio channels",
        "a_profile" => "AAC profile",
        "a_codec" => "Audio codec",
        "a_channel_layout" => "Channel layout",
        "a_bit_depth" => "Audio bit depth",
        "a_language" => "Audio language",
        "container_format" => "Container",
        "multiple_video_streams" => "Multi video streams",
        "multiple_audio_streams" => "Multi audio streams",
        "missing_video_stream" => "Missing video stream",
        "missing_audio_stream" => "Missing audio stream",
        "audio_duration_drift" => "Audio duration drift",
        "audio_start_offset" => "Audio start offset",
        "time_base" => "Timebase",
        "audio_channels_critical" => "Audio channels (multi)",
        _ => prop,
    }.to_string()
}

/// Filter outliers for Smart MKV mode.
///
/// MKV supports many codecs and stream parameters natively that MP4 does not.
/// This function removes outliers that are safe to pass through in MKV,
/// keeping only the ones that would actually break FFmpeg concat.
///
/// # Arguments
/// * `analysis` - The profile analysis containing outliers
/// * `use_mkvmerge` - If true, sample rate mismatch is NOT filtered as safe because
///                    mkvmerge cannot properly concatenate AAC streams with different
///                    sample rates (48000Hz vs 44100Hz causes duration corruption).
///                    If false (FFmpeg concat), sample rate is MKV-safe.
///
/// Returns filtered (outliers, audio_outliers) tuples.
pub fn filter_outliers_for_mkv(
    analysis: &ProfileAnalysis,
    backend: MergeBackend,
) -> (Vec<Outlier>, Vec<AudioOutlier>) {
    // Properties that are SAFE to skip for MKV output.
    // MKV natively supports mixed resolutions, FPS, pixel formats,
    // color metadata, bit depths, VFR, interlaced, sample rates,
    // and metadata-only fields.
    //
    // NOTE: a_sample_rate is MKV-safe for FFmpeg concat because FFmpeg's concat
    // demuxer handles mixed sample rates correctly. But mkvmerge CANNOT handle
    // mixed sample rate AAC streams - it causes duration corruption (111+ seconds
    // audio/video mismatch observed). So we exclude a_sample_rate from MKV-safe
    // when backend is MkvMerge.
    let mut mkv_safe_properties = vec![
        "resolution",
        "fps",
        // time_base is NOT MKV-safe to skip — playlist_equivalence.rs marks
        // time_base mismatches as CRITICAL. Both components must agree.
        "pixel_format",
        "color_space",
        "color_transfer",
        "v_bit_depth",
        "frame_rate_type",   // VFR
        "field_order",        // interlaced
        "rotation",
        "v_profile",
        "dar",
        "hdr",
        // "a_sample_rate", // NOT safe for mkvmerge - see above
        "a_bit_depth",
        "a_profile",         // AAC profile — MP3/Opus have no profile; None vs Some is cosmetic
        "a_channel_layout",
        "container_format",
        "multiple_video_streams",
        "multiple_audio_streams",
        "audio_language",
    ];

    // If using FFmpeg concat (not mkvmerge), sample rate is safe to skip
    if backend.supports_mixed_sample_rate() {
        mkv_safe_properties.push("a_sample_rate");
    }

    let filtered_outliers: Vec<Outlier> = analysis.outliers.iter()
        .filter(|o| !mkv_safe_properties.contains(&o.property.as_str()))
        .cloned()
        .collect();

    // AudioNormalizationType values that are SAFE to skip for MKV.
    // These are metadata-only or handled natively by MKV.
    use AudioNormalizationType;
    let mut mkv_safe_audio = vec![
        // SampleRateMismatch: safe for FFmpeg concat, but NOT for mkvmerge
        // (mkvmerge cannot concatenate 48000Hz + 44100Hz AAC without duration corruption)
        AudioNormalizationType::ChannelLayoutMismatch,
        AudioNormalizationType::BitDepthMismatch,
    ];

    // If using FFmpeg concat, sample rate mismatch is safe to skip
    if backend.supports_mixed_sample_rate() {
        mkv_safe_audio.push(AudioNormalizationType::SampleRateMismatch);
    }

    let filtered_audio_outliers: Vec<AudioOutlier> = analysis.audio_outliers.iter()
        .filter(|ao| !mkv_safe_audio.contains(&ao.audio_type))
        .cloned()
        .collect();

    (filtered_outliers, filtered_audio_outliers)
}

/// Simplified version that assumes MKVToolNix mkvmerge.
/// Kept for backward compatibility with existing callers.
#[allow(dead_code)]
pub fn filter_outliers_for_mkv_default(analysis: &ProfileAnalysis) -> (Vec<Outlier>, Vec<AudioOutlier>) {
    filter_outliers_for_mkv(analysis, MergeBackend::MkvMerge)
}

/// Simplified version that assumes FFmpeg concat (not mkvmerge).
/// Kept for backward compatibility with existing callers.
#[allow(dead_code)]
pub fn filter_outliers_for_mkv_ffmpeg(analysis: &ProfileAnalysis) -> (Vec<Outlier>, Vec<AudioOutlier>) {
    filter_outliers_for_mkv(analysis, MergeBackend::FfmpegConcat)
}

/// Run an FFmpeg command with cancellation support.
/// Polls the cancel flag every 200ms and kills the child process if cancelled.
///
/// `timeout_secs` sets the per-operation timeout.
pub async fn run_ffmpeg_cmd_with_cancel(
    ffmpeg_path: &Path,
    args: &[&str],
    cancel_flag: Arc<AtomicBool>,
    timeout_secs: u64,
) -> Result<(), String> {
    #[cfg(windows)]
    #[allow(unused_imports)]
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let start = Instant::now();
    let args_str = args.join(" ");

    // Extract audio parameters from args for better diagnostics
    let mut audio_codec = "unknown";
    let mut sample_rate = "unknown";
    let mut channels = "unknown";
    let mut audio_filter = "none";
    for (i, arg) in args.iter().enumerate() {
        match *arg {
            "-c:a" | "-c:v" | "-c:" => {
                if i + 1 < args.len() { audio_codec = args[i + 1]; }
            }
            "-ar" => {
                if i + 1 < args.len() { sample_rate = args[i + 1]; }
            }
            "-ac" => {
                if i + 1 < args.len() { channels = args[i + 1]; }
            }
            "-af" => {
                if i + 1 < args.len() { audio_filter = args[i + 1]; }
            }
            _ => {}
        }
    }
    log::info!("[NORM_EXEC_START] ffmpeg {} (timeout={}s) | codec={} sr={} ch={} filter={}",
        ffmpeg_path.display(), timeout_secs, audio_codec, sample_rate, channels, audio_filter);

    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = match cmd
        .args(args)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::error!("[NORM_EXEC_SPAWN_FAIL] ffmpeg {} {} failed: {}", ffmpeg_path.display(), args_str, e);
            return Err(format!("Failed to spawn ffmpeg: {}", e));
        }
    };

    let pid = child.id();
    let pid_str = pid.map(|p| p.to_string()).unwrap_or_else(|| "unknown".to_string());
    log::info!("[NORM_EXEC_PID] PID={} started at {:?}", pid_str, start);

    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();

    // Drain stdout in background to prevent pipe deadlock (64KB buffer on Windows)
    // FFmpeg writes progress to stdout; if not drained, it blocks after 64KB
    let stdout_drain = tokio::spawn(async move {
        if let Some(mut stdout) = stdout_handle {
            let mut devnull = tokio::io::sink();
            let _ = tokio::io::copy(&mut stdout, &mut devnull).await;
        }
    });

    let mut last_progress_log = Instant::now();
    let progress_interval_secs = 30; // Log progress every 30 seconds
    // Extract output path from args (last element) for file size monitoring
    let output_path_for_progress = args.last().map(|s| s.to_string()).unwrap_or_default();
    let mut last_output_size: u64 = 0;

    loop {
        let elapsed = start.elapsed().as_secs();
        if elapsed > timeout_secs {
            let _ = child.kill().await;
            let _ = child.wait().await;
            log::error!("[NORM_EXEC_TIMEOUT] PID={} timed out after {}s (limit={}s) output={} elapsed={:?}",
                pid_str, elapsed, timeout_secs, output_path_for_progress, start.elapsed());
            return Err(format!("FFmpeg process timed out after {} seconds", timeout_secs));
        }

        // Log progress every 30 seconds so we know where FFmpeg is stuck
        if last_progress_log.elapsed().as_secs() >= progress_interval_secs {
            // Check output file size to verify FFmpeg is actually producing output
            let output_size = if !output_path_for_progress.is_empty() {
                std::fs::metadata(&output_path_for_progress).map(|m| m.len()).unwrap_or(0)
            } else { 0 };
            let size_growth = if output_size > last_output_size {
                format!("+{:.1}MB", (output_size - last_output_size) as f64 / 1_048_576.0)
            } else if last_output_size > 0 {
                format!(" stagnant for {:.0}s", last_progress_log.elapsed().as_secs_f64())
            } else {
                " (file not created yet)".to_string()
            };
            log::info!("[NORM_EXEC_PROGRESS] PID={} elapsed={:?} output_size={:.1}MB {}",
                pid_str, start.elapsed(), output_size as f64 / 1_048_576.0, size_growth);
            last_output_size = output_size;
            last_progress_log = Instant::now();
        }

        if cancel_flag.load(Ordering::Relaxed) {
            let _ = child.kill().await;
            let _ = child.wait().await;
            log::info!("[NORM_EXEC_CANCEL] PID={} cancelled after {:?}", pid_str, start.elapsed());
            return Err("Merge cancelled by user".to_string());
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                let elapsed = start.elapsed();
                let _ = stdout_drain.await;
                if status.success() {
                    log::info!("[NORM_EXEC_COMPLETE] PID={} success after {:?} (exit=0)", pid_str, elapsed);
                    return Ok(());
                } else {
                    let mut err_bytes = Vec::new();
                    if let Some(mut stderr_reader) = stderr_handle {
                        let _ = stderr_reader.read_to_end(&mut err_bytes).await;
                    }
                    let stderr_str = String::from_utf8_lossy(&err_bytes);
                    log::error!("[NORM_EXEC_FAIL] PID={} after {:?} exit={} stderr_len={} stderr={}",
                        pid_str, elapsed, status.code().unwrap_or(-1), err_bytes.len(), stderr_str.chars().take(500).collect::<String>());
                    return Err(stderr_str.into_owned());
                }
            }
            Ok(None) => {
                sleep(Duration::from_millis(200)).await;
            }
            Err(e) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                log::error!("[NORM_EXEC_ERROR] PID={} error after {:?}: {}", pid_str, start.elapsed(), e);
                return Err(format!("FFmpeg process error: {}", e));
            }
        }
    }
}

/// Probe audio properties of a file for certification audit.
/// Returns (codec, profile, sample_rate, channels, bitrate, duration).
fn probe_audio_properties_sync(ffprobe_path: &Path, file_path: &str) -> Option<(String, Option<String>, Option<u32>, Option<u32>, Option<u64>, f64)> {
    let path = std::path::Path::new(file_path);
    if !path.exists() {
        return None;
    }
    let info = crate::ffmpeg::probe::probe_file(ffprobe_path, path).ok()?;
    let audio = info.audio_streams.first()?;
    Some((
        audio.codec_name.clone(),
        audio.profile.clone(),
        audio.sample_rate,
        audio.channels,
        audio.bit_rate,
        info.duration,
    ))
}

/// Create a certification audit record for a normalization operation.
/// Probes input and output files, compares properties, and logs the full report.
fn create_normalization_audit(
    ffprobe_path: Option<&Path>,
    file_index: usize,
    input_path: &str,
    output_path: &str,
    normalization_type: &str,
    target_codec: &str,
    target_sample_rate: u32,
    target_channels: Option<u32>,
) -> NormalizationAudit {
    let ffprobe_available = ffprobe_path.is_some();

    // Probe input properties
    let (input_codec, input_profile, input_sample_rate, input_channels, input_bitrate, input_duration) =
        if let Some(fp) = ffprobe_path {
            probe_audio_properties_sync(fp, input_path)
                .unwrap_or_else(|| ("unknown".to_string(), None, None, None, None, 0.0))
        } else {
            ("unknown".to_string(), None, None, None, None, 0.0)
        };

    // Probe output properties
    let (output_codec, output_profile, output_sample_rate, output_channels, output_bitrate, output_duration) =
        if let Some(fp) = ffprobe_path {
            probe_audio_properties_sync(fp, output_path)
                .map(|(c, p, sr, ch, br, dur)| (Some(c), p, sr, ch, br, Some(dur)))
                .unwrap_or_else(|| (None, None, None, None, None, None))
        } else {
            (None, None, None, None, None, None)
        };

    // Compute transformation summary
    let profile_changed = input_profile != output_profile && input_profile.is_some() && output_profile.is_some();
    let sample_rate_changed = input_sample_rate != output_sample_rate && input_sample_rate.is_some() && output_sample_rate.is_some();
    let channels_changed = input_channels != output_channels && input_channels.is_some() && output_channels.is_some();
    let bitrate_changed = input_bitrate != output_bitrate && input_bitrate.is_some() && output_bitrate.is_some();

    NormalizationAudit {
        file_index,
        input_path: input_path.to_string(),
        output_path: output_path.to_string(),
        normalization_type: normalization_type.to_string(),
        input_codec,
        input_profile,
        input_sample_rate,
        input_channels,
        input_bitrate,
        input_duration,
        target_codec: target_codec.to_string(),
        target_sample_rate,
        target_channels,
        output_codec,
        output_profile,
        output_sample_rate,
        output_channels,
        output_bitrate,
        output_duration,
        profile_changed,
        sample_rate_changed,
        channels_changed,
        bitrate_changed,
        resample_applied: sample_rate_changed,
        ffprobe_available,
    }
}

/// Re-encode a single video file to match the dominant profile.
/// Produces `norm_prof_{job_id}_{index}.{ext}` in temp_dir.
#[allow(clippy::too_many_arguments)]
pub async fn normalize_to_profile(
    ffmpeg_path: &Path,
    input_path: &str,
    profile: &EncodingProfile,
    temp_dir: &Path,
    job_id: &str,
    index: usize,
    has_audio: bool,
    cancel_flag: Arc<AtomicBool>,
    norm_cache: Option<Arc<NormalizationCache>>,
    input_duration: Option<f64>,
    input_video_duration_ms: Option<u64>,
    ffprobe_path: Option<&Path>,
    input_audio_sample_rate: Option<u32>,
) -> Result<String, String> {
    let input = Path::new(input_path);
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let output_path = temp_dir.join(format!("norm_prof_{}_{}.{}", job_id, index, ext));
    let input_for_ffmpeg = input_path.strip_prefix("\\\\?\\").unwrap_or(input_path);
    let mut args = vec!["-y".to_string(), "-fflags".to_string(), "+genpts+discardcorrupt".to_string(), "-err_detect".to_string(), "ignore_err".to_string(), "-i".to_string(), input_for_ffmpeg.to_string(), "-map".to_string(), "0:v:0".to_string()];
    if has_audio { args.push("-map".to_string()); args.push("0:a".to_string()); }
    args.push("-map_metadata".to_string()); args.push("0".to_string());
    args.push("-c:v".to_string()); args.push(profile.video_codec.clone());
    let mut video_filters = Vec::new();
    if let (Some(w), Some(h)) = (profile.width, profile.height) { video_filters.push(format!("scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black", w, h, w, h)); }
    if let Some(fps) = profile.fps { video_filters.push(format!("fps=fps={}", fps)); }
    if !video_filters.is_empty() { args.push("-vf".to_string()); args.push(video_filters.join(",")); }
    if has_audio {
        args.push("-c:a".to_string());
        args.push(profile.audio_codec.clone());
        if profile.audio_codec == "aac" {
            args.push("-profile:a".to_string());
            args.push("aac_low".to_string());
        }
        // Only add -ar if sample rate differs from input (skip if already matching)
        if input_audio_sample_rate != Some(profile.sample_rate) {
            args.push("-ar".to_string());
            args.push(profile.sample_rate.to_string());
        }
        if let Some(ch) = profile.channels {
            args.push("-ac".to_string());
            args.push(ch.to_string());
        }
        // Add explicit bitrate if specified (preserves target bitrate instead of FFmpeg default)
        if let Some(ref br) = profile.bitrate {
            args.push("-b:a".to_string());
            args.push(br.clone());
        }
        let af_chain = if let Some(whole_dur) = input_video_duration_ms {
            if whole_dur > 0 {
                let whole_dur_sec = whole_dur as f64 / 1000.0;
                log::info!("[FORENSIC:NORMALIZE] Profile re-encode | File #{} | video_dur={}ms → apad=whole_dur={} (pads audio to video length)",
                    index, whole_dur, whole_dur_sec);
                format!("apad=whole_dur={},aresample=first_pts=0", whole_dur_sec)
            } else {
                "aresample=first_pts=0".to_string()
            }
        } else {
            "aresample=first_pts=0".to_string()
        };
        args.push("-af".to_string());
        args.push(af_chain);
    } else {
        args.push("-an".to_string());
    }
    if let Some(ts) = profile.timescale { args.push("-video_track_timescale".to_string()); args.push(ts.to_string()); }
    args.push("-avoid_negative_ts".to_string()); args.push("make_zero".to_string());
    let out_str = output_path.to_string_lossy().into_owned();
    args.push(out_str.clone());
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    log::info!("[FORENSIC:NORMALIZE] Profile re-encode | File #{} | Input: {} | Output: {} | vcodec={} | acodec={} | sr={} | fps={:?} | timescale={:?} | {}x{} | ch={:?} | bitrate={:?} | whole_dur={:?}",
        index, input_path, out_str, profile.video_codec, profile.audio_codec, profile.sample_rate, profile.fps, profile.timescale, profile.width.unwrap_or(0), profile.height.unwrap_or(0), profile.channels, profile.bitrate, input_video_duration_ms);

    let profile_sig = crate::ffmpeg::norm_cache::NormSignature::Profile {
        vcodec: profile.video_codec.clone(),
        acodec: profile.audio_codec.clone(),
        sample_rate: profile.sample_rate,
        fps_milli: profile.fps.map(|f| (f * 1000.0) as u64),
        timescale: profile.timescale,
        width: profile.width,
        height: profile.height,
        channels: profile.channels,
        whole_dur_ms: input_video_duration_ms,
    };

    if let Some(ref _cache) = norm_cache {
        if let Some(cached_path) = _cache.get(input_path, &profile_sig) {
            log::info!("[NormCache] HIT: Reusing normalization of {} from {}", input_path, cached_path.display());
            std::fs::copy(&cached_path, &output_path).map_err(|e| format!("Failed to copy cached normalized file: {}", e))?;
            return Ok(out_str);
        }
    }
    let adaptive_timeout = input_duration.map_or(3600u64, |dur| {
        let computed = (dur * 10.0).ceil() as u64;
        computed.clamp(600, 36000)
    });
    log::info!("[FORENSIC:NORMALIZE] Profile norm File #{} | timeout={}s (input_duration={:?}s)", index, adaptive_timeout, input_duration);
    if let Err(e) = run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, adaptive_timeout).await {
        let _ = std::fs::remove_file(&output_path);
        return Err(e);
    }

    // ── CERTIFICATION AUDIT: Log before/after comparison ──────────────────
    let audit = create_normalization_audit(
        ffprobe_path,
        index,
        input_path,
        &out_str,
        "profile_reencode",
        &profile.audio_codec,
        profile.sample_rate,
        profile.channels,
    );
    audit.log_report();

    if let Some(ref _cache) = norm_cache {
        _cache.insert(input_path, profile_sig, output_path.clone());
        log::info!("[NormCache] Cached normalization of {}", input_path);
    }
    Ok(out_str)
}

/// Lossless timescale fix via remux.
/// Produces `norm_ts_{job_id}_{index}.{ext}` in temp_dir.
#[allow(clippy::too_many_arguments)]
pub async fn normalize_timescale_lossless(
    ffmpeg_path: &Path, input_path: &str, target_timescale: u32, temp_dir: &Path, job_id: &str, index: usize, cancel_flag: Arc<AtomicBool>,
    norm_cache: Option<Arc<NormalizationCache>>,
) -> Result<String, String> {
    let input = Path::new(input_path);
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let output_path = temp_dir.join(format!("norm_ts_{}_{}.{}", job_id, index, ext));
    let output_path_str = output_path.to_string_lossy().into_owned();
    let input_for_ffmpeg = input_path.strip_prefix("\\\\?\\").unwrap_or(input_path);
    let args = vec![
        "-y".to_string(),
        "-i".to_string(),
        input_for_ffmpeg.to_string(),
        "-c".to_string(),
        "copy".to_string(),
        "-video_track_timescale".to_string(),
        target_timescale.to_string(),
        "-avoid_negative_ts".to_string(),
        "make_zero".to_string(),
        output_path_str.clone(),
    ];
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    log::info!("[FORENSIC:NORMALIZE] Timescale lossless remux | File #{} | Input: {} | Output: {} | target_timescale={}",
        index, input_path, output_path_str, target_timescale);

    if let Some(ref _cache) = norm_cache {
        let sig = crate::ffmpeg::norm_cache::NormSignature::Timescale {
            target_timescale,
        };
        if let Some(cached_path) = _cache.get(input_path, &sig) {
            log::info!("[NormCache] HIT: Reusing timescale remux of {} from {}", input_path, cached_path.display());
            std::fs::copy(&cached_path, &output_path).map_err(|e| format!("Failed to copy cached timescale file: {}", e))?;
            return Ok(output_path_str);
        }
    }
    if let Err(e) = run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 3600).await {
        let _ = std::fs::remove_file(&output_path);
        return Err(e);
    }

    if let Some(ref _cache) = norm_cache {
        let sig = crate::ffmpeg::norm_cache::NormSignature::Timescale {
            target_timescale,
        };
        _cache.insert(input_path, sig, output_path.clone());
        log::info!("[NormCache] Cached timescale remux of {}", input_path);
    }
    Ok(output_path_str)
}

/// Audio-only normalization (keeps video stream copy, re-encodes audio only).
/// Produces `norm_audio_{job_id}_{index}.{ext}` in temp_dir.
#[allow(clippy::too_many_arguments)]
pub async fn normalize_audio_only(
    ffmpeg_path: &Path,
    input_path: &str,
    audio_profile: &AudioProfile,
    temp_dir: &Path,
    job_id: &str,
    index: usize,
    cancel_flag: Arc<AtomicBool>,
    norm_cache: Option<Arc<NormalizationCache>>,
    input_video_duration_ms: Option<u64>,
    ffprobe_path: Option<&Path>,
    input_audio_sample_rate: Option<u32>,
) -> Result<String, String> {
    let input = Path::new(input_path);
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let output_path = temp_dir.join(format!("norm_audio_{}_{}.{}", job_id, index, ext));
    let output_path_str = output_path.to_string_lossy().into_owned();
    let input_for_ffmpeg = input_path.strip_prefix("\\\\?\\").unwrap_or(input_path);
    let mut args = vec![
        "-y".to_string(),
        "-fflags".to_string(),
        "+genpts+discardcorrupt".to_string(),
        "-err_detect".to_string(),
        "ignore_err".to_string(),
        "-i".to_string(),
        input_for_ffmpeg.to_string(),
        "-map".to_string(),
        "0:v:0".to_string(),
        "-map".to_string(),
        "0:a".to_string(),
        "-map_metadata".to_string(),
        "0".to_string(),
        "-c:v".to_string(),
        "copy".to_string(),
        "-c:a".to_string(),
        audio_profile.audio_codec.clone(),
    ];
    if audio_profile.audio_codec == "aac" {
        args.push("-profile:a".to_string());
        args.push("aac_low".to_string());
    }
    // Only add -ar if sample rate differs from input (skip if already matching)
    if input_audio_sample_rate != Some(audio_profile.sample_rate) {
        args.push("-ar".to_string());
        args.push(audio_profile.sample_rate.to_string());
    }
    if let Some(ch) = audio_profile.channels {
        args.push("-ac".to_string());
        args.push(ch.to_string());
    }
    // Add explicit bitrate if specified (preserves target bitrate instead of FFmpeg default)
    if let Some(ref br) = audio_profile.bitrate {
        args.push("-b:a".to_string());
        args.push(br.clone());
    }
    let af_chain = if let Some(whole_dur) = input_video_duration_ms {
        if whole_dur > 0 {
            let whole_dur_sec = whole_dur as f64 / 1000.0;
            log::info!("[FORENSIC:NORMALIZE] Audio-only | File #{} | video_dur={}ms → apad=whole_dur={} (pads audio to video length)",
                index, whole_dur, whole_dur_sec);
            format!("apad=whole_dur={},aresample=first_pts=0", whole_dur_sec)
        } else {
            "aresample=first_pts=0".to_string()
        }
    } else {
        "aresample=first_pts=0".to_string()
    };
    args.push("-af".to_string());
    args.push(af_chain);
    if let Some(ts) = audio_profile.timescale { args.push("-video_track_timescale".to_string()); args.push(ts.to_string()); }
    args.push("-avoid_negative_ts".to_string()); args.push("make_zero".to_string());
    args.push(output_path_str.clone());
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    log::info!("[FORENSIC:NORMALIZE] Audio-only | File #{} | Input: {} | Output: {} | target_acodec={} | sr={} | ts={:?} | ch={:?} | bitrate={:?} | whole_dur={:?}",
        index, input_path, output_path_str, audio_profile.audio_codec, audio_profile.sample_rate, audio_profile.timescale, audio_profile.channels, audio_profile.bitrate, input_video_duration_ms);

    let cache_sig = crate::ffmpeg::norm_cache::NormSignature::AudioOnly {
        acodec: audio_profile.audio_codec.clone(),
        sample_rate: audio_profile.sample_rate,
        timescale: audio_profile.timescale,
        channels: audio_profile.channels,
        whole_dur_ms: input_video_duration_ms,
    };
    if let Some(ref _cache) = norm_cache {
        if let Some(cached_path) = _cache.get(input_path, &cache_sig) {
            log::info!("[NormCache] HIT: Reusing audio-only normalization of {} from {}", input_path, cached_path.display());
            std::fs::copy(&cached_path, &output_path).map_err(|e| format!("Failed to copy cached audio file: {}", e))?;
            return Ok(output_path.to_string_lossy().into_owned());
        }
    }
    if let Err(e) = run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 3600).await {
        let _ = std::fs::remove_file(&output_path);
        return Err(e);
    }

    // ── CERTIFICATION AUDIT: Log before/after comparison ──────────────────
    let audit = create_normalization_audit(
        ffprobe_path,
        index,
        input_path,
        &output_path_str,
        "audio_only_reencode",
        &audio_profile.audio_codec,
        audio_profile.sample_rate,
        audio_profile.channels,
    );
    audit.log_report();

    if let Some(ref _cache) = norm_cache {
        _cache.insert(input_path, cache_sig, output_path.clone());
        log::info!("[NormCache] Cached audio-only normalization of {}", input_path);
    }
    Ok(output_path_str)
}

/// Per-file decision record for SmartMKV decision explainer.
#[derive(Debug)]
pub struct SmartMkvFileDecision {
    pub file_index: usize,
    pub file_path: String,
    pub detected_outliers: Vec<String>,
    pub filtered_outliers: Vec<String>,
    pub remaining_outliers: Vec<String>,
    pub decision: SmartMkvDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SmartMkvDecision {
    StreamCopy,
    RemuxOnly,
    AudioNormalize,
    VideoNormalize,
    FullNormalize,
}

impl SmartMkvDecision {
    pub fn label(&self) -> &'static str {
        match self {
            SmartMkvDecision::StreamCopy => "STREAM_COPY",
            SmartMkvDecision::RemuxOnly => "REMUX_ONLY",
            SmartMkvDecision::AudioNormalize => "AUDIO_NORMALIZE",
            SmartMkvDecision::VideoNormalize => "VIDEO_NORMALIZE",
            SmartMkvDecision::FullNormalize => "FULL_NORMALIZE",
        }
    }
}

fn get_mkv_safe_properties(backend: MergeBackend) -> Vec<&'static str> {
    let mut props = vec![
        "resolution", "fps", "time_base", "pixel_format", "color_space", "color_transfer",
        "v_bit_depth", "frame_rate_type", "field_order", "rotation", "v_profile",
        "dar", "hdr", "a_bit_depth", "a_profile",
        "a_channel_layout", "container_format", "multiple_video_streams",
        "multiple_audio_streams", "audio_language",
    ];
    // Sample rate is safe only for FFmpeg concat, not for mkvmerge
    if backend.supports_mixed_sample_rate() {
        props.push("a_sample_rate");
    }
    props
}

fn get_mkv_safe_audio_types(backend: MergeBackend) -> Vec<AudioNormalizationType> {
    let mut types = vec![
        AudioNormalizationType::ChannelLayoutMismatch,
        AudioNormalizationType::BitDepthMismatch,
    ];
    // Sample rate mismatch is safe only for FFmpeg concat, not for mkvmerge
    if backend.supports_mixed_sample_rate() {
        types.push(AudioNormalizationType::SampleRateMismatch);
    }
    types
}

/// Build per-file decision records for the SmartMKV decision explainer.
/// This shows for each file: what was detected, what was filtered, and the final decision.
pub fn build_smartmkv_file_decisions(
    analysis: &ProfileAnalysis,
    _outlier_by_index: &std::collections::HashMap<usize, Vec<Outlier>>,
    backend: MergeBackend,
) -> Vec<SmartMkvFileDecision> {
    use std::collections::BTreeSet;
    let mkv_safe = get_mkv_safe_properties(backend);
    let mkv_safe_audio = get_mkv_safe_audio_types(backend);

    let mut decisions = Vec::new();

    // Get all unique file indices - use BTreeSet for deterministic iteration
    let mut all_indices: BTreeSet<usize> = analysis.outliers.iter()
        .map(|o| o.index)
        .chain(analysis.audio_outliers.iter().map(|a| a.index))
        .collect();

    // Also include files that matched the dominant profile (no outliers)
    for i in 0..analysis.dominant.total_count {
        let idx = if analysis.profiles.is_empty() { i } else { i };
        all_indices.insert(idx);
    }

    for &file_idx in &all_indices {
        // Get all detected outliers for this file
        let mut detected: Vec<String> = Vec::new();
        let mut filtered: Vec<String> = Vec::new();
        let mut remaining: Vec<String> = Vec::new();

        // Profile outliers for this file
        for o in analysis.outliers.iter().filter(|o| o.index == file_idx) {
            detected.push(format!("{}: {}→{}", o.property, o.actual_value, o.dominant_value));
        }

        // Audio outliers for this file
        for a in analysis.audio_outliers.iter().filter(|a| a.index == file_idx) {
            detected.push(format!("{}: {}→{}", a.audio_type.label(), a.actual_value, a.dominant_value));
        }

        // Now separate into filtered vs remaining using MKV-safe rules
        // Check profile outliers
        for o in analysis.outliers.iter().filter(|o| o.index == file_idx) {
            if mkv_safe.contains(&o.property.as_str()) {
                filtered.push(format!("✓ {} (MKV-safe)", o.property));
            } else {
                remaining.push(format!("{}: {}→{}", o.property, o.actual_value, o.dominant_value));
            }
        }

        // Check audio outliers
        for ao in analysis.audio_outliers.iter().filter(|a| a.index == file_idx) {
            if mkv_safe_audio.contains(&ao.audio_type) {
                filtered.push(format!("✓ {} (MKV-safe)", ao.audio_type.label()));
            } else {
                remaining.push(format!("{}: {}→{}", ao.audio_type.label(), ao.actual_value, ao.dominant_value));
            }
        }

        // Determine decision
        let decision = if remaining.is_empty() && detected.is_empty() {
            SmartMkvDecision::StreamCopy
        } else if remaining.is_empty() {
            SmartMkvDecision::StreamCopy // All filtered, no remaining outliers
        } else {
            // Check what type of normalization is needed
            let has_video = analysis.outliers.iter()
                .any(|o| o.index == file_idx && !mkv_safe.contains(&o.property.as_str()));
            let has_audio = analysis.audio_outliers.iter()
                .any(|a| a.index == file_idx && !mkv_safe_audio.contains(&a.audio_type));

            let video_outlier = analysis.outliers.iter()
                .find(|o| o.index == file_idx && !mkv_safe.contains(&o.property.as_str()));

            let is_remux_only = video_outlier.map(|o| o.property == "time_base").unwrap_or(false)
                && !has_audio;

            if is_remux_only {
                SmartMkvDecision::RemuxOnly
            } else if has_video && has_audio {
                SmartMkvDecision::FullNormalize
            } else if has_video {
                SmartMkvDecision::VideoNormalize
            } else if has_audio {
                SmartMkvDecision::AudioNormalize
            } else {
                SmartMkvDecision::StreamCopy
            }
        };

        // Get file path
        let file_path = analysis.profiles.get(file_idx)
            .map(|p| p.path.clone())
            .unwrap_or_else(|| format!("File #{}", file_idx));

        decisions.push(SmartMkvFileDecision {
            file_index: file_idx,
            file_path,
            detected_outliers: detected,
            filtered_outliers: filtered,
            remaining_outliers: remaining,
            decision,
        });
    }

    // Sort by file index
    decisions.sort_by_key(|d| d.file_index);
    decisions
}

/// Log the SmartMKV decision explainer for all files.
pub fn log_smartmkv_decision_explainer(decisions: &[SmartMkvFileDecision]) {
    log::info!("");
    log::info!("[SMARTMKV_DECISION] ╔══════════════════════════════════════════════════════════════════╗");
    log::info!("[SMARTMKV_DECISION] ║          SMART MKV DECISION EXPLAINER — Per-File Rationale       ║");
    log::info!("[SMARTMKV_DECISION] ╚══════════════════════════════════════════════════════════════════╝");

    if decisions.is_empty() {
        log::info!("[SMARTMKV_DECISION]   No files to analyze.");
        log::info!("[SMARTMKV_DECISION] ═══════════════════════════════════════════════════════════════");
        return;
    }

    for (i, decision) in decisions.iter().enumerate() {
        if i > 0 {
            log::info!("[SMARTMKV_DECISION] ─────────────────────────────────────────────────────────────────");
        }

        log::info!("[SMARTMKV_DECISION] File #{} | Decision: {}", decision.file_index, decision.decision.label());
        log::info!("[SMARTMKV_DECISION]   Path: {}", decision.file_path);

        if !decision.detected_outliers.is_empty() {
            log::info!("[SMARTMKV_DECISION]   Detected Differences:");
            for det in &decision.detected_outliers {
                log::info!("[SMARTMKV_DECISION]     • {}", det);
            }
        }

        if !decision.filtered_outliers.is_empty() {
            log::info!("[SMARTMKV_DECISION]   Filtered (MKV-safe):");
            for filt in &decision.filtered_outliers {
                log::info!("[SMARTMKV_DECISION]     ✓ {}", filt);
            }
        }

        if !decision.remaining_outliers.is_empty() {
            log::info!("[SMARTMKV_DECISION]   Remaining Actions:");
            for rem in &decision.remaining_outliers {
                log::info!("[SMARTMKV_DECISION]     ! {}", rem);
            }
        } else if decision.detected_outliers.is_empty() {
            log::info!("[SMARTMKV_DECISION]   Status: PERFECT MATCH — no differences detected");
        } else {
            log::info!("[SMARTMKV_DECISION]   Status: ALL DIFFERENCES FILTERED — stream copy possible");
        }
    }

    log::info!("[SMARTMKV_DECISION] ═══════════════════════════════════════════════════════════════");

    // Summary by decision type
    let mut copy_count = 0;
    let mut remux_count = 0;
    let mut audio_count = 0;
    let mut video_count = 0;
    let mut full_count = 0;

    for d in decisions {
        match d.decision {
            SmartMkvDecision::StreamCopy => copy_count += 1,
            SmartMkvDecision::RemuxOnly => remux_count += 1,
            SmartMkvDecision::AudioNormalize => audio_count += 1,
            SmartMkvDecision::VideoNormalize => video_count += 1,
            SmartMkvDecision::FullNormalize => full_count += 1,
        }
    }

    log::info!("[SMARTMKV_DECISION] DECISION SUMMARY:");
    log::info!("[SMARTMKV_DECISION]   Stream Copy:     {:>5} files", copy_count);
    log::info!("[SMARTMKV_DECISION]   Remux Only:     {:>5} files", remux_count);
    log::info!("[SMARTMKV_DECISION]   Audio Normalize: {:>5} files", audio_count);
    log::info!("[SMARTMKV_DECISION]   Video Normalize: {:>5} files", video_count);
    log::info!("[SMARTMKV_DECISION]   Full Normalize:  {:>5} files", full_count);
    log::info!("[SMARTMKV_DECISION] ═══════════════════════════════════════════════════════════════");
}

/// Statistics for the SmartMKV summary report.
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct SmartMkvSummaryStats {
    pub total_files: usize,
    pub stream_copy_count: usize,
    pub remux_count: usize,
    pub audio_normalize_count: usize,
    pub video_normalize_count: usize,
    pub full_normalize_count: usize,
    pub normalize_time_secs: f64,
    pub merge_time_secs: f64,
    pub total_input_size_bytes: u64,
    pub total_output_size_bytes: u64,
    pub before_filter_outliers: usize,
    pub after_filter_outliers: usize,
}

/// Log the SmartMKV summary report at end of merge.
pub fn log_smartmkv_summary_report(stats: &SmartMkvSummaryStats) {
    let saved_time_estimate = if stats.full_normalize_count > 0 || stats.video_normalize_count > 0 {
        let full_reencode_files = stats.full_normalize_count + stats.video_normalize_count;
        let audio_only_files = stats.audio_normalize_count;
        let remux_files = stats.remux_count;
        let copy_files = stats.stream_copy_count;
        let baseline_time = (full_reencode_files + audio_only_files + remux_files + copy_files) as f64 * 60.0;
        let actual_time = (full_reencode_files as f64 * 60.0)
            + (audio_only_files as f64 * 10.0)
            + (remux_files as f64 * 1.0)
            + (copy_files as f64 * 0.1);
        baseline_time - actual_time
    } else {
        0.0
    };

    let input_gb = stats.total_input_size_bytes as f64 / 1_073_741_824.0;
    let output_gb = stats.total_output_size_bytes as f64 / 1_073_741_824.0;
    let size_ratio = if stats.total_input_size_bytes > 0 {
        stats.total_output_size_bytes as f64 / stats.total_input_size_bytes as f64
    } else { 1.0 };

    let pct = |count: usize| {
        if stats.total_files > 0 {
            (count as f64 / stats.total_files as f64) * 100.0
        } else { 0.0 }
    };

    let no_processing = stats.stream_copy_count + stats.remux_count;
    let needs_processing = stats.audio_normalize_count + stats.video_normalize_count + stats.full_normalize_count;
    let efficiency_score = if stats.total_files > 0 {
        (no_processing as f64 / stats.total_files as f64) * 100.0
    } else { 0.0 };

    let filter_reduction_pct = if stats.before_filter_outliers > 0 {
        (stats.before_filter_outliers - stats.after_filter_outliers) as f64 / stats.before_filter_outliers as f64 * 100.0
    } else { 0.0 };

    log::info!("");
    log::info!("[SMARTMKV_SUMMARY] ╔══════════════════════════════════════════════════════════════════════╗");
    log::info!("[SMARTMKV_SUMMARY] ║                    SMART MKV SUMMARY REPORT                            ║");
    log::info!("[SMARTMKV_SUMMARY] ╠══════════════════════════════════════════════════════════════════════╣");
    log::info!("[SMARTMKV_SUMMARY] ║  CERTIFICATION STATUS                                                       ║");
    log::info!("[SMARTMKV_SUMMARY] ║  ─────────────────────────────────────────────────────────────────────  ║");
    log::info!("[SMARTMKV_SUMMARY] ║  Decision Engine:        ✅ CERTIFIED (classification tests)              ║");
    log::info!("[SMARTMKV_SUMMARY] ║  Runtime Merge Validation: ⏳ PENDING (mkvmerge success verification)  ║");
    log::info!("[SMARTMKV_SUMMARY] ║  Output Integrity:         ⏳ PENDING (duration, streams, timeline)      ║");
    log::info!("[SMARTMKV_SUMMARY] ║  Playback Certification:   ⏳ PENDING (seeking, audio sync)             ║");
    log::info!("[SMARTMKV_SUMMARY] ╠══════════════════════════════════════════════════════════════════════╣");
    log::info!("[SMARTMKV_SUMMARY] ║  FILE PROCESSING                                                           ║");
    log::info!("[SMARTMKV_SUMMARY] ║  ─────────────────────────────────────────────────────────────────────  ║");
    log::info!("[SMARTMKV_SUMMARY] ║  Total Files:          {:>6}", stats.total_files);
    log::info!("[SMARTMKV_SUMMARY] ║  ─────────────────────────────────────────────────────────────────────  ║");
    log::info!("[SMARTMKV_SUMMARY] ║  No Processing:        {:>6} ({:>5.1}%) — stream copy + remux", no_processing, pct(no_processing));
    log::info!("[SMARTMKV_SUMMARY] ║    Stream Copy:        {:>6} ({:>5.1}%)", stats.stream_copy_count, pct(stats.stream_copy_count));
    log::info!("[SMARTMKV_SUMMARY] ║    Remux Only:         {:>6} ({:>5.1}%)", stats.remux_count, pct(stats.remux_count));
    log::info!("[SMARTMKV_SUMMARY] ║  Needs Processing:     {:>6} ({:>5.1}%)", needs_processing, pct(needs_processing));
    log::info!("[SMARTMKV_SUMMARY] ║    Audio Normalize:    {:>6} ({:>5.1}%)", stats.audio_normalize_count, pct(stats.audio_normalize_count));
    log::info!("[SMARTMKV_SUMMARY] ║    Video Normalize:    {:>6} ({:>5.1}%)", stats.video_normalize_count, pct(stats.video_normalize_count));
    log::info!("[SMARTMKV_SUMMARY] ║    Full Normalize:     {:>6} ({:>5.1}%)", stats.full_normalize_count, pct(stats.full_normalize_count));
    log::info!("[SMARTMKV_SUMMARY] ╠══════════════════════════════════════════════════════════════════════╣");
    log::info!("[SMARTMKV_SUMMARY] ║  EFFICIENCY METRICS                                                        ║");
    log::info!("[SMARTMKV_SUMMARY] ║  ─────────────────────────────────────────────────────────────────────  ║");
    log::info!("[SMARTMKV_SUMMARY] ║  Efficiency Score:     {:>6.1}% (files not re-encoded)", efficiency_score);
    log::info!("[SMARTMKV_SUMMARY] ║  Filter Reduction:     {:>6.1}% (outliers removed as MKV-safe)", filter_reduction_pct);
    if saved_time_estimate > 0.0 {
        log::info!("[SMARTMKV_SUMMARY] ║  Time Saved:           {:>6.0}s (~{:.0} min vs full normalize)", saved_time_estimate, saved_time_estimate / 60.0);
    }
    log::info!("[SMARTMKV_SUMMARY] ║  ─────────────────────────────────────────────────────────────────────  ║");
    log::info!("[SMARTMKV_SUMMARY] ║  Outliers Before Filter: {:>5}", stats.before_filter_outliers);
    log::info!("[SMARTMKV_SUMMARY] ║  Outliers After Filter:  {:>5}", stats.after_filter_outliers);
    log::info!("[SMARTMKV_SUMMARY] ║  Filtered as MKV-safe:    {:>5}", stats.before_filter_outliers - stats.after_filter_outliers);
    log::info!("[SMARTMKV_SUMMARY] ╠══════════════════════════════════════════════════════════════════════╣");
    log::info!("[SMARTMKV_SUMMARY] ║  OUTPUT                                                                       ║");
    log::info!("[SMARTMKV_SUMMARY] ║  ─────────────────────────────────────────────────────────────────────  ║");
    log::info!("[SMARTMKV_SUMMARY] ║  Input Size:  {:>12.3} GB", input_gb);
    log::info!("[SMARTMKV_SUMMARY] ║  Output Size: {:>12.3} GB", output_gb);
    log::info!("[SMARTMKV_SUMMARY] ║  Size Ratio:  {:>12.2}x", size_ratio);
    log::info!("[SMARTMKV_SUMMARY] ╚══════════════════════════════════════════════════════════════════════╝");
}

/// Complete merge plan for certification.
/// This struct captures ALL decision information needed to execute a merge,
/// making it suitable for idempotency certification.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergePlan {
    pub backend: String,
    pub dominant_profile: DominantProfilePlan,
    pub file_decisions: Vec<FileDecisionPlan>,
    pub smartmkv_breakdown: SmartMkvBreakdown,
    pub analysis_summary: AnalysisSummary,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DominantProfilePlan {
    pub v_codec: Option<String>,
    pub v_profile: Option<String>,
    pub v_resolution: Option<String>,
    pub v_fps: Option<String>,
    pub v_time_base: Option<String>,
    pub v_pixel_format: Option<String>,
    pub v_color_space: Option<String>,
    pub v_color_transfer: Option<String>,
    pub a_codec: Option<String>,
    pub a_sample_rate: Option<u32>,
    pub a_channels: Option<u32>,
    pub a_channel_layout: Option<String>,
    pub container_format: Option<String>,
    pub match_count: usize,
    pub total_count: usize,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FileDecisionPlan {
    pub file_index: usize,
    pub file_path: String,
    pub decision: SmartMkvDecision,
    pub detected_outliers: Vec<String>,
    pub filtered_outliers: Vec<String>,
    pub remaining_outliers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnalysisSummary {
    pub total_files: usize,
    pub outlier_count: usize,
    pub audio_outlier_count: usize,
}

impl MergePlan {
    /// Build a complete merge plan from probed file information.
    /// This is a PURE FUNCTION - same inputs always produce identical outputs.
    pub fn build(files: &[(usize, String, MediaInfo)], backend: MergeBackend) -> Self {
        let analysis = analyze_profiles(files);
        let decisions = build_smartmkv_file_decisions(&analysis, &std::collections::HashMap::new(), backend);
        let breakdown = compute_smart_mkv_breakdown(&analysis);

        let dominant_profile = DominantProfilePlan {
            v_codec: analysis.dominant.v_codec.clone(),
            v_profile: analysis.dominant.v_profile.clone(),
            v_resolution: match (analysis.dominant.v_width, analysis.dominant.v_height) {
                (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                _ => None,
            },
            v_fps: analysis.dominant.v_fps.map(|f| format!("{:.3}", f)),
            v_time_base: analysis.dominant.v_time_base.clone(),
            v_pixel_format: analysis.dominant.v_pixel_format.clone(),
            v_color_space: analysis.dominant.v_color_space.clone(),
            v_color_transfer: analysis.dominant.v_color_transfer.clone(),
            a_codec: analysis.dominant.a_codec.clone(),
            a_sample_rate: analysis.dominant.a_sample_rate,
            a_channels: analysis.dominant.a_channels,
            a_channel_layout: analysis.dominant.a_channel_layout.clone(),
            container_format: analysis.dominant.container_format.clone(),
            match_count: analysis.dominant.match_count,
            total_count: analysis.dominant.total_count,
        };

        let file_decisions = decisions.into_iter().map(|d| FileDecisionPlan {
            file_index: d.file_index,
            file_path: d.file_path,
            decision: d.decision,
            detected_outliers: d.detected_outliers,
            filtered_outliers: d.filtered_outliers,
            remaining_outliers: d.remaining_outliers,
        }).collect();

        let analysis_summary = AnalysisSummary {
            total_files: files.len(),
            outlier_count: analysis.outliers.len(),
            audio_outlier_count: analysis.audio_outliers.len(),
        };

        MergePlan {
            backend: format!("{:?}", backend),
            dominant_profile,
            file_decisions,
            smartmkv_breakdown: breakdown,
            analysis_summary,
        }
    }

    /// Serialize to JSON for comparison.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| String::new())
    }

    /// Compare two merge plans for equality.
    pub fn equals(&self, other: &MergePlan) -> bool {
        self.backend == other.backend
            && self.dominant_profile == other.dominant_profile
            && self.file_decisions == other.file_decisions
            && self.analysis_summary == other.analysis_summary
    }
}
