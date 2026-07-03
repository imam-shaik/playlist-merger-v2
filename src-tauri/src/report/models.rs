use serde::Serialize;

// ═════════════════════════════════════════════════════════════════════════════
// MergeReport — Single source of truth for all report renderers
// ═════════════════════════════════════════════════════════════════════════════

/// Complete report data for a merge operation.
/// Built once by `build_report_data()`, consumed by any renderer.
#[derive(Debug, Clone, Serialize)]
pub struct MergeReport {
    pub header: ReportHeader,
    /// Ordered timeline entries (videos, cards, folder headers) in playback order.
    /// This is the primary report view — a timeline, not a table.
    pub timeline: Vec<TimelineEntry>,
    pub stats: ReportStats,
    /// Per-folder breakdown (future: rendered in dedicated section).
    pub folder_breakdown: Vec<FolderBreakdown>,
    /// Split summary, if the merge was split into parts.
    pub split_summary: Option<SplitSummary>,
    /// Repeat summary (reserved for upcoming repeat feature).
    pub repeat_summary: Option<RepeatSummary>,
    /// Recovery summary (reserved for upcoming Resume/Certification feature).
    pub recovery_summary: Option<RecoverySummary>,
}

// ═════════════════════════════════════════════════════════════════════════════
// ReportHeader
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct ReportHeader {
    /// Full absolute path to the output video file.
    pub output_path: String,
    /// Display name (file name without path).
    pub output_name: String,
    /// ISO-8601 timestamp of report generation.
    pub generated_at: String,
    /// Formatted for display (e.g. "2026-06-11 14:30:00").
    pub generated_at_formatted: String,
    /// Merge mode as a display string: "Lossless", "Custom", "Fast MKV", "Smart MKV".
    pub mode: String,
    /// Total duration of the merged output in seconds.
    pub total_duration: f64,
    /// Human-readable duration (e.g. "01:53:21").
    pub total_duration_formatted: String,
    /// Output file size in bytes.
    pub total_size_bytes: u64,
    /// Human-readable size (e.g. "124.5 MB").
    pub total_size_formatted: String,
    /// Total file count (videos only, excluding cards).
    pub file_count: usize,
    /// Total card count.
    pub card_count: usize,
    /// Job identifier, if available.
    pub job_id: Option<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// CardType — Distinguishes card insertion strategies
// ═════════════════════════════════════════════════════════════════════════════

/// Strategy used to insert a card into the timeline.
/// Future: Repeat cards will add a variant here without schema migration.
#[derive(Debug, Clone, Serialize)]
pub enum CardType {
    PerVideo,
    PerFolder,
    /// Reserved for the upcoming repeat feature.
    Repeat,
}

// ═════════════════════════════════════════════════════════════════════════════
// TimelineEntry — Timeline entries in playback order
// ═════════════════════════════════════════════════════════════════════════════

/// A single entry in the merge timeline.
/// Using an enum instead of boolean flags (is_card, is_folder) for
/// future-proofing — new entry types (Repeat, etc.)
/// can be added without breaking existing match arms.
#[derive(Debug, Clone, Serialize)]
pub enum TimelineEntry {
    /// A standard video segment.
    Video {
        /// 1-based display index.
        index: usize,
        /// Display name of the video.
        name: String,
        /// Duration in seconds.
        duration: f64,
        /// Formatted duration string (e.g. "00:09:27").
        duration_formatted: String,
        /// Absolute start time within the merged output.
        start_time: f64,
        /// Formatted start time (e.g. "00:00:00").
        start_time_formatted: String,
        /// Absolute end time within the merged output.
        end_time: f64,
        /// Formatted end time (e.g. "00:09:27").
        end_time_formatted: String,
        /// Parent folder name for folder-grouped playlists.
        parent_folder: Option<String>,
    },
    /// A canvas card inserted between videos.
    Card {
        /// 1-based display index (shared counter with Video, not separate).
        entry_index: usize,
        /// Display name (e.g. "▶ Section: grammar").
        name: String,
        /// Card duration in seconds.
        duration: f64,
        /// Formatted duration string.
        duration_formatted: String,
        /// Absolute start time within the merged output.
        start_time: f64,
        /// Formatted start time.
        start_time_formatted: String,
        /// Absolute end time within the merged output.
        end_time: f64,
        /// Formatted end time.
        end_time_formatted: String,
        /// Card background color in hex.
        color: Option<String>,
        /// Card insertion strategy.
        card_type: CardType,
    },
    /// A folder header inserted when the parent folder changes.
    /// This is a visual grouping element, not a playable segment.
    FolderHeader {
        /// Display name of the folder.
        name: String,
    },
    /// A split boundary section within the timeline.
    /// Used when the merge output is split into multiple files.
    SplitSection {
        /// 1-based part index.
        part_index: u32,
        /// Display label (e.g. "Part 1").
        label: String,
        /// Duration of this part in seconds.
        duration: f64,
        /// Formatted duration.
        duration_formatted: String,
        /// Number of source files in this part.
        file_count: u32,
    },
}

impl TimelineEntry {
    pub fn duration(&self) -> f64 {
        match self {
            TimelineEntry::Video { duration, .. } => *duration,
            TimelineEntry::Card { duration, .. } => *duration,
            TimelineEntry::FolderHeader { .. } => 0.0,
            TimelineEntry::SplitSection { duration, .. } => *duration,
        }
    }

    pub fn is_card(&self) -> bool {
        matches!(self, TimelineEntry::Card { .. })
    }

    pub fn is_video(&self) -> bool {
        matches!(self, TimelineEntry::Video { .. })
    }

    pub fn is_folder_header(&self) -> bool {
        matches!(self, TimelineEntry::FolderHeader { .. })
    }

    pub fn is_split_section(&self) -> bool {
        matches!(self, TimelineEntry::SplitSection { .. })
    }

    pub fn display_index(&self) -> Option<usize> {
        match self {
            TimelineEntry::Video { index, .. } => Some(*index),
            TimelineEntry::Card { entry_index, .. } => Some(*entry_index),
            TimelineEntry::FolderHeader { .. } => None,
            TimelineEntry::SplitSection { .. } => None,
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// ReportStats
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct ReportStats {
    /// Number of video segments.
    pub video_count: usize,
    /// Number of card segments.
    pub card_count: usize,
    /// Total duration of original source content (before cards).
    pub original_duration: f64,
    /// Formatted original duration.
    pub original_duration_formatted: String,
    /// Final merged duration (original + cards).
    pub final_duration: f64,
    /// Formatted final duration.
    pub final_duration_formatted: String,
    /// Additional time added by cards (final - original).
    pub added_time: f64,
    /// Formatted added time.
    pub added_time_formatted: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// FolderBreakdown
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct FolderBreakdown {
    /// Folder name.
    pub name: String,
    /// Number of video files in this folder.
    pub video_count: usize,
    /// Number of card entries associated with this folder.
    pub card_count: usize,
    /// Total duration of content from this folder.
    pub total_duration: f64,
    /// Formatted total duration.
    pub total_duration_formatted: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// SplitSummary
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct SplitSummary {
    /// Whether splitting is enabled.
    pub enabled: bool,
    /// Split mode display string (e.g. "Count", "Duration", "Folder").
    pub mode: String,
    /// Total number of parts.
    pub part_count: u32,
    /// Individual part information.
    pub parts: Vec<SplitPartInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitPartInfo {
    /// 1-based part index.
    pub index: u32,
    /// Output file path for this part.
    pub output_path: String,
    /// Duration of this part in seconds.
    pub duration: f64,
    /// Number of source files in this part.
    pub file_count: u32,
}

// ═════════════════════════════════════════════════════════════════════════════
// RepeatSummary — Reserved for the upcoming repeat feature
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct RepeatSummary {
    /// Whether repeat is enabled.
    pub enabled: bool,
    /// Repeat mode (e.g. "PerVideo", "PerFolder").
    pub mode: Option<String>,
    /// Number of times content is repeated.
    pub repeat_count: Option<u32>,
    /// Duration before repeat.
    pub original_duration: Option<f64>,
    /// Duration after repeat.
    pub final_duration: Option<f64>,
}

// ═════════════════════════════════════════════════════════════════════════════
// RecoverySummary — Reserved for the upcoming Resume/Certification feature
// ═════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct RecoverySummary {
    /// Whether the merge was resumed from a checkpoint.
    pub resumed: bool,
    /// Age of the checkpoint at resume time.
    pub checkpoint_age_secs: Option<f64>,
    /// Number of files skipped during resume (already completed).
    pub skipped_files: usize,
    /// Number of previously normalized files reused.
    pub reused_files: usize,
    /// Files remaining to normalize after resume.
    pub normalized_remaining: usize,
}

// ═════════════════════════════════════════════════════════════════════════════
// Builder input helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Aggregated folder data computed during report building.
#[derive(Debug, Clone, Default)]
pub(crate) struct FolderAccumulator {
    #[allow(dead_code)]
    pub name: String,
    pub video_count: usize,
    pub card_count: usize,
    pub total_duration: f64,
}
