// ──────────────────────────────────────────────────────────────────────────────
// Certification API Surface
// ──────────────────────────────────────────────────────────────────────────────
// Targeted re-exports from private modules — only what the certification
// framework (certification/) needs to call the real production pipeline.
//
// No internal functions, no private invariants, no blanket `pub mod` exports.
// ──────────────────────────────────────────────────────────────────────────────

// ── FFmpeg binary location & temp management ──
pub use crate::ffmpeg::{
    find_ffmpeg,
    find_ffprobe,
    get_temp_dir,
    cleanup_partial_output,
    force_kill_process_tree,
};

// ── Concat list writing ──
pub use crate::ffmpeg::{
    write_concat_list_with_durations,
    write_subtitle_concat_list,
    generate_merged_srt,
};

// ── Merge execution ──
pub use crate::ffmpeg::concat::{
    run_merge_blocking,
    MergeConfig,
    check_codec_compatibility_parallel,
    detect_codec_transitions,
};

// ── Merge result types (from commands/merge.rs) ──
pub use crate::commands::merge::{
    MergeResult,
    MergeSegment,
    MergePartResult,
    AudioRepairSummary,
};

// ── Repeat expansion ──
pub use crate::ffmpeg::repeat::{
    expand_repeat,
    ExpandedPlaylist,
};
pub use crate::ffmpeg::repeat_merge::apply_repeat_expansion;

// ── Canvas overlay cards ──
pub use crate::ffmpeg::cards::{
    render_cards_for_merge,
    interleave_cards_with_videos,
    InterleaveResult,
};

// ── FastMKV ──────────────────────────────────────────────────────────────────
pub use crate::ffmpeg::fast_mkv::run_fast_mkv_merge;

// ── Normalization / Audio profile analysis ──
// Note: normalization.rs has its own NormalizationType (for outlier classification)
// and DominantProfile (for profile analysis). These are distinct from the identically-named
// types in crate::types used by the recovery checkpoint system.
pub use crate::ffmpeg::normalization::{
    analyze_profiles,
    ProfileAnalysis,
    MediaProfile,
    DominantProfile as NormDominantProfile,
    Outlier,
    AudioOutlier,
    AudioNormalizationType,
    NormalizationType as NormNormalizationType,
    format_audit_report,
    filter_outliers_for_mkv,
    compute_smart_mkv_breakdown,
    check_batch_corruption_parallel,
    check_subtitle_file,
    repair_subtitle_file,
    check_pts_continuity_lightweight,
    FileHealth,
    FileHealthStatus,
    SmartMkvBreakdown,
    PropertyCount,
};

// ── Probe cache ──
pub use crate::ffmpeg::probe_cache::ProbeCache;

// ── Core types ──
pub use crate::types::{
    MergeMode,
    MergeProgress,
    MergePhase,
    MediaInfo,
    SubtitleMode,
    SubtitleStream,
    CardConfig,
    CardFrequency,
    RepeatConfig,
    SplitConfig,
    SplitMode,
    SplitSubtitleMode,
    FolderSplitMode,
    RecoveryCheckpoint,
    CompletedFile,
    VideoStream,
    AudioStream,
    // Renamed to avoid conflict with NormNormalizationType from normalization.rs
    NormalizationType as CheckpointNormalizationType,
    DominantProfile as CheckpointDominantProfile,
};

// ── Split naming types ──
pub use crate::split::types::NamingConfig;

// ── Media Validation Engine — pre-merge validation & repair tracking ──
pub use crate::ffmpeg::media_validation_engine::{
    validate_input_files,
    MediaValidationReport,
    MediaValidationResult,
    ValidationStatus,
    FixType,
    DamageClassification,
    StreamIdentity,
    TimestampIssue,
    ContainerIssue,
    SubtitleIssue,
    RepairTraceEntry,
};

/// Lightweight summary of validation repairs for MergeResult.
/// Mirrors the key counts from MediaValidationReport without the full file-by-file data.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationSummary {
    pub total_files: usize,
    pub clean_count: usize,
    pub fixed_count: usize,
    pub quarantined_count: usize,
    pub timestamp_repair_count: usize,
    pub container_remux_count: usize,
    pub full_reencode_count: usize,
    pub timestamp_damage_count: usize,
    pub container_damage_count: usize,
    pub subtitle_damage_count: usize,
}

impl From<&MediaValidationReport> for ValidationSummary {
    fn from(report: &MediaValidationReport) -> Self {
        let mut timestamp_repair_count = 0usize;
        let mut container_remux_count = 0usize;
        let mut full_reencode_count = 0usize;

        for (_file_idx, _path, fix_type) in &report.repaired_files {
            match fix_type {
                FixType::TimestampRepair => timestamp_repair_count += 1,
                FixType::ContainerRemux => container_remux_count += 1,
                FixType::FullReencode => full_reencode_count += 1,
                FixType::None => {}
            }
        }

        Self {
            total_files: report.total_files,
            clean_count: report.clean_count,
            fixed_count: report.fixed_count,
            quarantined_count: report.quarantined_count,
            timestamp_repair_count,
            container_remux_count,
            full_reencode_count,
            timestamp_damage_count: report.timestamp_damage_count,
            container_damage_count: report.container_damage_count,
            subtitle_damage_count: report.subtitle_damage_count,
        }
    }
}

// ── Subtitle Timeline Audit ──
pub use crate::ffmpeg::subtitle_audit::{
    SubtitleAuditReport,
    SubtitleAuditReport as AuditReport,
    TimelineAlignment,
    FirstSpokenDialogue,
    OffsetDriftReport,
    PartBoundaryInfo,
    SplitTimestampRebaseCheck,
    AudioDelayInfo,
    FFprobeTimelineDump,
    SmartMkvPipelineTrace,
    AuditFinding,
    AuditOverallStatus,
    TimelineStatus,
    DialogueStatus,
    DriftStatus,
    OffsetType,
    generate_audit_report,
    print_audit_summary,
};
