use super::enums::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileState {
    pub file_index: usize,
    pub original_path: String,
    pub original_name: String,
    pub original_size: u64,
    pub original_duration_secs: f64,
    pub disposition: FileDisposition,
    pub damage_classification: DamageClassification,
    pub confidence: f32,
    pub analysis_reason: String,
    pub analysis_timestamp_ms: u64,
    pub has_pts_issues: bool,
    pub has_dts_issues: bool,
    pub has_subtitle_issues: bool,
    pub has_container_issues: bool,
    pub has_video_decode_issues: bool,
    pub has_bitstream_issues: bool,
    pub has_packet_issues: bool,
    pub has_frame_issues: bool,
    pub has_attachment_issues: bool,
    pub has_timebase_issues: bool,
    pub has_vfr_instability: bool,
    pub analysis_duration_ms: f64,
    pub repair_status: RepairStatus,
    pub fix_applied: Option<FixType>,
    pub remux_type: Option<RemuxType>,
    pub repair_reason: Option<String>,
    pub repaired_path: Option<String>,
    pub repair_trace: Vec<RepairTraceEntry>,
    pub repair_duration_ms: f64,
    pub revalidation_status: RevalidationStatus,
    pub revalidation_duration_ms: f64,
    pub current_size: u64,
    pub current_duration_secs: f64,
    pub current_path: String,
    pub final_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineReport {
    pub file_states: Vec<FileState>,
    pub total_count: usize,
    pub healthy_count: usize,
    pub needs_normalization_count: usize,
    pub repaired_count: usize,
    pub failed_repair_count: usize,
    pub quarantined_count: usize,
    pub untouched_count: usize,
    pub compatibility_remux_count: usize,
    pub repair_remux_count: usize,
    pub subtitle_repair_count: usize,
    pub reencode_count: usize,
    pub total_analysis_duration_secs: f64,
    pub total_repair_duration_secs: f64,
    pub total_revalidation_duration_secs: f64,
    pub total_duration_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaValidationResult {
    pub file_index: usize,
    pub file_path: String,
    pub status: ValidationStatus,
    pub damage_classification: Option<DamageClassification>,
    pub confidence: f32,
    pub validation_reason: String,
    pub repaired_path: Option<String>,
    pub fix_applied: Option<FixType>,
    pub analysis_duration_ms: f64,
    pub repair_duration_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MediaValidationReport {
    pub total_files: usize,
    pub clean_count: usize,
    pub fixed_count: usize,
    pub quarantined_count: usize,
    pub quarantined_files: Vec<(usize, String)>,
    pub repaired_files: Vec<(usize, String, FixType)>,
    pub total_duration_ms: u128,
    pub timestamp_damage_count: usize,
    pub container_damage_count: usize,
    pub subtitle_damage_count: usize,
    pub reencode_count: usize,
    #[serde(skip)]
    pub file_results: Vec<MediaValidationResult>,
}

impl Default for MediaValidationReport {
    fn default() -> Self {
        Self {
            total_files: 0,
            clean_count: 0,
            fixed_count: 0,
            quarantined_count: 0,
            quarantined_files: Vec::new(),
            repaired_files: Vec::new(),
            total_duration_ms: 0,
            timestamp_damage_count: 0,
            container_damage_count: 0,
            subtitle_damage_count: 0,
            reencode_count: 0,
            file_results: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepairTraceEntry {
    pub function: String,
    pub outcome: String,
    pub output_path: Option<String>,
    pub details: String,
    pub effectiveness: Option<f32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamIdentity {
    pub video_count: usize,
    pub audio_count: usize,
    pub subtitle_count: usize,
    pub total_streams: usize,
    pub video_codecs: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub duration: Option<f64>,
    // P0-3: Rich metadata for post-repair identity verification
    #[serde(default)]
    pub video_languages: Vec<Option<String>>,
    #[serde(default)]
    pub audio_languages: Vec<Option<String>>,
    #[serde(default)]
    pub video_color_transfer: Vec<Option<String>>,
    #[serde(default)]
    pub video_color_space: Vec<Option<String>>,
    #[serde(default)]
    pub video_rotation: Vec<Option<i32>>,
}

impl StreamIdentity {
    pub fn empty() -> Self {
        Self {
            video_count: 0,
            audio_count: 0,
            subtitle_count: 0,
            total_streams: 0,
            video_codecs: Vec::new(),
            audio_codecs: Vec::new(),
            duration: None,
            video_languages: Vec::new(),
            audio_languages: Vec::new(),
            video_color_transfer: Vec::new(),
            video_color_space: Vec::new(),
            video_rotation: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimestampIssue {
    pub stream_index: usize,
    pub stream_type: String,
    pub packet_index: usize,
    pub issue_type: String,
    pub pts: Option<i64>,
    pub dts: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContainerIssue {
    pub severity: IssueSeverity,
    pub stream_index: Option<usize>,
    pub issue_type: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubtitleIssue {
    pub stream_index: usize,
    pub stream_type: String,
    pub issue_type: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RemuxType {
    Compatibility,
    Repair,
}