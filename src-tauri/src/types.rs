use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Supported video container formats
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "mov", "avi", "webm", "m4v", "ts", "mts", "m2ts",
    "flv", "wmv", "3gp", "ogv",
];

/// Video stream metadata from ffprobe
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoStream {
    pub codec_name: String,
    pub codec_long_name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub bit_rate: Option<u64>,
    pub pixel_format: Option<String>,
    pub color_space: Option<String>,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub profile: Option<String>,
    pub level: Option<i32>,
    pub duration: Option<f64>,
    pub time_base: Option<String>,
    pub stream_index: u32,
    /// Raw frame rate from r_frame_rate (for VFR detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_frame_rate: Option<String>,
    /// Field order (progressive, interlaced, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_order: Option<String>,
    /// Average frame rate from avg_frame_rate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_frame_rate: Option<String>,
    /// Bit depth (bits per raw sample)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits_per_raw_sample: Option<u32>,
    /// Sample aspect ratio
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_aspect_ratio: Option<String>,
    /// Display aspect ratio
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_aspect_ratio: Option<String>,
    /// Rotation metadata (0, 90, 180, 270)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
    /// Video start time in seconds (for audio offset/delay comparison)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<f64>,
}

/// Audio stream metadata from ffprobe
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioStream {
    pub codec_name: String,
    pub codec_long_name: String,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub bit_rate: Option<u64>,
    pub duration: Option<f64>,
    pub stream_index: u32,
    /// Audio profile (e.g., "LC" for AAC-LC, "HE" for HE-AAC)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    /// Bits per raw sample
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits_per_raw_sample: Option<u32>,
    /// Audio start PTS (for audio offset/delay detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_pts: Option<i64>,
    /// Audio start time in seconds (for offset/delay detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<f64>,
    /// Audio language
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Subtitle stream metadata from ffprobe
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleStream {
    pub codec_name: String,
    pub codec_long_name: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub stream_index: u32,
    pub is_external: bool,
    pub path: Option<String>,
    /// True for bitmap-based subtitles (PGS, VobSub, DVB) that cannot be
    /// converted to text-based SRT. These require special handling: either
    /// stream copy (MKV) or burn-in (re-encode). Extraction to SRT will fail.
    #[serde(default)]
    pub is_bitmap: bool,
}

/// Bitmap subtitle codec names that cannot be converted to text-based SRT.
/// PGS (hdmv_pgs_subtitle), VobSub (dvd_subtitle), DVB (dvbsub) are image-based.
pub const BITMAP_SUBTITLE_CODECS: &[&str] = &[
    "hdmv_pgs_subtitle",
    "dvd_subtitle",
    "dvbsub",
    "dvd_subtitle",
];

/// Complete media file metadata from ffprobe
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub duration: f64,
    pub size: u64,
    pub format_name: String,
    pub format_long_name: String,
    pub bit_rate: Option<u64>,
    pub video_streams: Vec<VideoStream>,
    pub audio_streams: Vec<AudioStream>,
    pub subtitle_streams: Vec<SubtitleStream>,
    pub start_time: Option<f64>,
    pub creation_time: Option<DateTime<Utc>>,
}

/// Merge mode selection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum MergeMode {
    /// Stream copy — no re-encoding, ultra-fast, lossless
    Lossless,
    /// Re-encode to common format — fixes incompatibilities
    Custom,
    /// Fast MKV merge — stream copy into MKV container, no normalization/repair
    FastMkv,
    /// Smart MKV merge — analyze + repair only broken files, output MKV
    SmartMkv,
}

/// Subtitle handling mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SubtitleMode {
    /// No subtitle processing — skip all subtitle operations
    None,
    /// Mux subtitles as a subtitle track in the output container (default, current behavior)
    Embed,
    /// Burn subtitles into the video frames (requires re-encode)
    Burn,
    /// Only export a standalone merged SRT file, no video subtitle processing
    ExportSrt,
    /// Merge subtitles only, bypass all video processing
    SrtMergeOnly,
}

impl SubtitleMode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::None => "none",
            Self::Embed => "embed",
            Self::Burn => "burn",
            Self::ExportSrt => "exportSrt",
            Self::SrtMergeOnly => "srtMergeOnly",
        }
    }
}

/// Configuration for splitting merge output into multiple parts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SplitConfig {
    pub mode: SplitMode,
    /// Sub-mode for Folder mode: how to handle each folder's output
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_split_mode: Option<FolderSplitMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_duration_per_part: Option<f64>,
    /// Subtitle handling for split output: embed, export_srt, or ignore
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_mode: Option<SplitSubtitleMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FolderSplitMode {
    Single,
    Parts,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SplitSubtitleMode {
    Embed,
    ExportSrt,
    Ignore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SplitMode {
    None,
    Count,
    Duration,
    Folder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeProgress {
    pub percent: f32,
    pub current_time: f64,
    pub total_duration: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_segment_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_written: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<f32>,
    pub phase: MergePhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overall_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_files_in_stage: Option<usize>,
    /// Warning message to display in the UI (e.g. "Large playlist detected")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// Whether this is a large playlist (auto-detected: >300 files or >24h duration)
    pub is_large_playlist: bool,
}

impl Default for MergeProgress {
    fn default() -> Self {
        Self {
            percent: 0.0,
            current_time: 0.0,
            total_duration: 0.0,
            speed: None,
            fps: None,
            current_file: None,
            current_segment_index: None,
            remaining_duration: None,
            bytes_written: None,
            eta_seconds: None,
            phase: MergePhase::Preparing,
            overall_percent: None,
            stage_name: None,
            stage_percent: None,
            current_file_index: None,
            total_files_in_stage: None,
            warning: None,
            is_large_playlist: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub enum MergePhase {
    #[default]
    Preparing,
    Probing,
    Validating,
    Normalizing,
    Writing,
    Finalizing,
    Complete,
    Failed,
    Cancelled,
}


fn default_boundary_template() -> String {
    "🔁 Repeat {n}".to_string()
}

fn default_enable_media_validation() -> bool {
    true
}

/// Configuration for repeating the playlist output
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepeatConfig {
    /// Master enable/disable for repeat
    pub enabled: bool,
    /// Repeat by count: repeat the playlist N times
    pub by_count: bool,
    /// The repeat count value (when by_count is true)
    pub repeat_count: u32,
    /// Repeat until duration: extend until target total duration is reached
    pub until_duration: bool,
    /// Target total duration in seconds (when until_duration is true)
    pub target_duration_seconds: f64,
    /// Insert a boundary card at the start of each repeat cycle
    #[serde(default)]
    pub insert_boundary_cards: bool,
    /// Template for the boundary card label. {n} is replaced with cycle number.
    #[serde(default = "default_boundary_template")]
    pub boundary_card_template: String,
}

/// Configuration for canvas overlay cards between merged videos
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub enum CardFrequency {
    /// Show a card between every single video
    #[default]
    PerVideo,
    /// Show a card only when the parent folder changes
    PerFolder,
}


/// Configuration for canvas overlay cards between merged videos
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardConfig {
    /// Background color in hex format (e.g. "#3366FF")
    pub color: String,
    /// Font color in hex format (auto-computed from luminance)
    pub font_color: String,
    /// Duration of each card in seconds
    pub duration: f64,
    /// Whether to show canvas entries in the merge report
    pub show_in_report: bool,
    /// Frequency of cards (every video vs every folder change)
    pub frequency: CardFrequency,
}

/// Disk space info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskSpaceInfo {
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub path: String,
}

/// App settings persisted to disk
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub last_export_dir: Option<String>,
    pub thumbnail_cache_dir: Option<String>,
    pub max_thumbnail_cache_mb: u64,
    pub recent_exports: Vec<RecentExport>,
    pub default_merge_mode: MergeMode,
    pub check_compat_before_merge: bool,
    pub auto_save_playlist: bool,
    pub window_width: u32,
    pub window_height: u32,
    /// Default strategy when a large playlist is detected with Smart mode.
    /// If None, SmartLite is used (smart lite behavior, no dialog shown).
    pub large_playlist_default: Option<crate::commands::merge::LargePlaylistStrategy>,
    /// Historical merge stats for phase-weighted ETA learning. Kept to last 20 records.
    pub merge_stats_history: Vec<MergeStatsRecord>,
    /// Enable packet timestamp certification before merge.
    /// When enabled, runs ffprobe -show_packets on each file to check for
    /// missing/invalid PTS/DTS before starting the merge. This catches timestamp
    /// issues early (before 10+ hour merges fail at the final mux step).
    pub check_packet_timestamps: bool,
    /// Enable the MediaValidationEngine pre-merge validation layer.
    /// When enabled, runs comprehensive media validation (PTS/DTS checks,
    /// container corruption detection, VFR instability, subtitle validation)
    /// on all input files before they enter the merge pipeline.
    /// Auto-fixes repairable issues (remux, re-encode) and quarantines
    /// irreparably damaged files.
    #[serde(default = "default_enable_media_validation")]
    pub enable_media_validation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MergeStatsRecord {
    pub id: String,
    pub files: usize,
    pub total_media_duration_seconds: f64,
    pub mode: String,
    pub audio_repair_mode: String,
    pub large_playlist_strategy: Option<crate::commands::merge::LargePlaylistStrategy>,
    pub subtitle_mode: String,
    pub total_time_seconds: f64,
    /// Per-phase elapsed seconds. Keys are phase names: "probing", "validating", etc.
    pub phase_times: std::collections::BTreeMap<String, f64>,
    pub completed_at: i64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ffmpeg_path: None,
            ffprobe_path: None,
            last_export_dir: None,
            thumbnail_cache_dir: None,
            max_thumbnail_cache_mb: 500,
            recent_exports: Vec::new(),
            default_merge_mode: MergeMode::Lossless,
            check_compat_before_merge: true,
            auto_save_playlist: true,
            window_width: 1200,
            window_height: 780,
            large_playlist_default: None,
            merge_stats_history: Vec::new(),
            check_packet_timestamps: true,
            enable_media_validation: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentExport {
    pub path: String,
    pub timestamp: DateTime<Utc>,
    pub size_bytes: u64,
    pub file_count: usize,
    pub duration_seconds: f64,
    pub mode: String,
}

/// Type of normalization applied to a file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum NormalizationType {
    /// Video + audio re-encode (normalize_to_profile)
    Full,
    /// Audio-only re-encode (normalize_audio_only)
    Audio,
    /// Timescale/lossless remux (normalize_timescale_lossless)
    Timescale,
}

/// A single completed normalization file in the recovery checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedFile {
    /// Index in the original input files list
    pub index: usize,
    /// Original source file path
    pub source_path: String,
    /// Source file size in bytes (for change detection)
    pub source_size: u64,
    /// Source file modification time as Unix timestamp
    pub source_mtime: i64,
    /// Path to the normalized output file
    pub normalized_path: String,
    /// Type of normalization applied
    pub normalization_type: NormalizationType,
}

/// Dominant profile used for normalization (used to determine if resume is compatible)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DominantProfile {
    pub v_codec: Option<String>,
    pub v_width: Option<u32>,
    pub v_height: Option<u32>,
    pub v_fps: Option<f64>,
    pub a_codec: Option<String>,
    pub a_sample_rate: Option<u32>,
    pub a_channels: Option<u32>,
    pub timescale_den: Option<u64>,
}

/// Recovery checkpoint written after each successful normalization.
/// Stored in {local_data}/PlaylistMerger/recovery/{jobId}.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryCheckpoint {
    /// Schema version for future compatibility
    pub version: u32,
    /// Unique job identifier
    pub job_id: String,
    /// Current phase when checkpoint was written
    pub phase: MergePhase,
    /// Unix timestamp when merge started
    pub started_at: i64,
    /// Input files passed to the merge
    pub input_files: Vec<String>,
    /// Output path chosen by user
    pub output_path: String,
    /// Merge mode (lossless, custom, fastMkv, smartMkv)
    pub mode: String,
    /// Dominant profile used for normalization
    pub dominant_profile: DominantProfile,
    /// List of successfully normalized files
    pub completed_files: Vec<CompletedFile>,
    /// Indices of files not yet processed
    pub remaining_indices: Vec<usize>,
    /// Repeat configuration (if repeat was enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_config: Option<RepeatConfig>,
    /// Original file count before repeat expansion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_file_count: Option<usize>,
    /// Repeat count applied during expansion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_count: Option<u32>,
    // ── v2 fields ─────────────────────────────────────────────────────────────
    /// Subtitle handling mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_mode: Option<String>,
    /// Generate standalone merged SRT alongside video
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_merged_srt: Option<bool>,
    /// Per-file selected embedded subtitle stream indices
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_subtitle_stream_indices: Option<Vec<Option<u32>>>,
    // Custom encoding settings (used when mode = custom)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_crf: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_bitrate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_fps: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hw_accel: Option<String>,
    // Canvas overlay cards configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_config: Option<CardConfig>,
    // Split output configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_config: Option<SplitConfig>,
    // Output naming configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naming_config: Option<crate::split::types::NamingConfig>,
    // Audio repair and validation settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_repair_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validate_audio: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_playlist_strategy: Option<String>,
    // Fast/Smart MKV: convert output MKV to MP4
    #[serde(skip_serializing_if = "Option::is_none")]
    pub convert_to_mp4: Option<bool>,
    // ── Duration persistence for P0 recovery fix ─────────────────────────────
    // Per-file durations from original probe — allows resume without re-probing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_durations: Option<Vec<f64>>,
    // Total duration computed from input_durations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<f64>,
    // ── Section merge metadata (v3) ──────────────────────────────────────────
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_meta: Option<SectionMeta>,
}

/// Job type for recovery checkpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum JobType {
    StandardMerge,
    SectionMerge,
}

/// Result of a single completed section
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionResult {
    pub section_index: u32,
    pub output_path: String,
    pub duration_secs: f64,
    pub size_bytes: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// Metadata for section merge in recovery checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionMeta {
    pub completed_sections: Vec<u32>,
    pub current_section: Option<u32>,
    pub section_plans: Vec<SectionPlan>,
    pub section_results: Vec<SectionResult>,
    pub total_sections: u32,
}

/// Section boundary using folder index ranges (memory efficient)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionBoundary {
    pub section_index: u32,
    pub folder_start_idx: usize,
    pub folder_end_idx: usize,
    pub video_count: usize,
    pub duration_secs: f64,
    pub estimated_size_bytes: u64,
    pub folder_names: Vec<String>,
}

/// Section plan for preview and execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionPlan {
    pub section_index: u32,
    pub boundary: SectionBoundary,
    pub output_name: String,
}

/// Partition method for section generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PartitionMethod {
    SectionCount(u32),
    MaxDurationSecs(f64),
    MaxSizeBytes(u64),
}

/// Folder information used for section planning
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderForMerge {
    pub name: String,
    pub path: String,
    pub video_count: usize,
    pub duration_secs: f64,
    pub size_bytes: u64,
    pub file_paths: Vec<String>,
    pub include_in_sections: bool,
}

/// Base merge configuration inherited by each section
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionMergeConfig {
    pub method: PartitionMethod,
    pub name_template: String,
    pub output_subfolder: String,
    pub output_base_dir: String,
    pub base_merge_mode: String,
    pub normalize_audio: bool,
    pub quality: Option<String>,
    pub large_playlist_strategy: Option<String>,
}

/// Request to start a section merge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionMergeRequest {
    pub job_id: String,
    pub config: SectionMergeConfig,
    pub folders: Vec<FolderForMerge>,
}

/// Event emitted during section merge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionEvent {
    pub job_id: String,
    pub current_section: u32,
    pub total_sections: u32,
    pub section_name: String,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Subtitle extraction warning for a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleWarning {
    pub file_index: usize,
    pub file_path: String,
    pub reason: String,
}


