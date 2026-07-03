use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum NamingModeId {
    #[default]
    Sequential,
    Prefix,
    Suffix,
    Custom,
    Timestamp,
    Chapter,
    Playlist,
    Date,
    SmartCourse,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingConfig {
    pub mode: NamingModeId,
    pub template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zero_padding: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub separator: Option<String>,
}

impl Default for NamingConfig {
    fn default() -> Self {
        Self {
            mode: NamingModeId::Sequential,
            template: "{filename}_Part_{num3}".to_string(),
            prefix: None,
            suffix: None,
            zero_padding: Some(3),
            separator: None,
        }
    }
}

/// The method used to split the video.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SplitMode {
    /// Split into N equal-duration parts.
    ByParts,
    /// Split into chunks of a fixed duration (seconds).
    ByDuration,
    /// Split at chapter boundaries from metadata.
    ByChapters,
    /// Split at user-defined time ranges.
    CustomRanges,
    /// Split a concatenated playlist by original item count.
    ByPlaylistItems,
    /// Split so each output file is ≤ a max size (bytes).
    ByOutputSize,
    /// Smart split based on daily/weekly learning goals.
    SmartCourse,
}

/// Subtitle handling mode for direct split.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SplitSubtitleMode {
    /// Copy all subtitle streams (current behavior).
    CopyAll,
    /// Extract embedded subtitles or use companion SRT, split to separate files.
    ExtractSplit,
    /// Ignore subtitles - don't include in output.
    Ignore,
}

/// A single segment in a split plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitSegment {
    pub index: usize,
    pub label: String,
    pub start_time: f64,
    pub end_time: f64,
    pub duration: f64,
    pub estimated_size_bytes: Option<u64>,
}

/// The complete plan for splitting a video.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPlan {
    pub job_id: String,
    pub input_file: String,
    pub input_duration: f64,
    pub input_size_bytes: u64,
    pub mode: SplitMode,
    pub segments: Vec<SplitSegment>,
    pub output_dir: String,
    pub output_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naming_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naming_config: Option<NamingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_timestamp: Option<bool>,
}

/// Parameters for each split mode.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SplitParams {
    /// Number of parts (ByParts mode).
    pub part_count: Option<u32>,
    /// Duration per part in seconds (ByDuration mode).
    pub part_duration: Option<f64>,
    /// Custom ranges: alternating [start, end, start, end, ...] in seconds.
    pub custom_ranges: Option<Vec<f64>>,
    /// Items per segment (ByPlaylistItems mode).
    pub items_per_segment: Option<u32>,
    /// Max output file size in bytes (ByOutputSize mode).
    pub max_size_bytes: Option<u64>,
    /// Smart course mode: "daily" or "weekly".
    pub course_mode: Option<String>,
    /// Smart course: hours per day/week.
    pub hours_per_unit: Option<f64>,
    /// Output format extension (default "mp4").
    pub output_format: Option<String>,
    /// Label prefix for output files.
    pub label_prefix: Option<String>,
    /// Subtitle handling mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_mode: Option<SplitSubtitleMode>,
    /// Whether to export SRT files (only applies when subtitle_mode = ExtractSplit).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_srt: Option<bool>,
    /// Suffix for output files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_suffix: Option<String>,
    /// Custom naming template (e.g. "{stem}_{label}_{index}").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naming_template: Option<String>,
    /// Full naming configuration (replaces naming_template when present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naming_config: Option<NamingConfig>,
    /// Whether to include timestamp in the output filename.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_timestamp: Option<bool>,
}

/// Request from the frontend to generate a split plan (preview).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPlanRequest {
    pub job_id: String,
    pub input_file: String,
    pub input_duration: f64,
    pub input_size_bytes: u64,
    pub mode: SplitMode,
    pub params: SplitParams,
    pub output_dir: String,
}

/// Request from the frontend to execute a split.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitExecuteRequest {
    pub job_id: String,
    pub plan: SplitPlan,
    /// Subtitle mode for this split (overrides params from plan).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_mode: Option<SplitSubtitleMode>,
    /// Whether to export SRT files per segment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_srt: Option<bool>,
}

/// Result of a split execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitResult {
    pub job_id: String,
    pub output_paths: Vec<String>,
    pub output_sizes_bytes: Vec<u64>,
    pub total_duration: f64,
    pub segments_count: usize,
    /// Paths to generated SRT files (one per segment), if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub srt_output_paths: Option<Vec<String>>,
    /// Paths to generated report files, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_paths: Option<Vec<String>>,
}

/// Progress event emitted during split execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitProgress {
    pub job_id: String,
    pub segment_index: usize,
    pub segment_count: usize,
    pub progress: f64, // 0.0 – 100.0 for the current segment
    pub stage: String, // "planning", "splitting", "validating", "done", "error"
    pub message: String,
}
