use std::collections::HashMap;

use crate::commands::merge::{MergeSegment, MergePartResult};
use crate::types::{CardConfig, SplitConfig, CardFrequency, RepeatConfig};
use crate::report::models::*;

/// Build a MergeReport from raw merge data.
///
/// This is the **only** place where business logic transforms segments
/// into the report data model. All renderers consume `MergeReport` directly.
///
/// # Parameters
/// - `segments`: Ordered merge segments (videos + cards).
/// - `parts`: Optional split part info for multi-part merges.
/// - `output_path`: Absolute path to the merged output file.
/// - `output_size_bytes`: Size of the output file.
/// - `total_duration`: Total duration of the merged output.
/// - `mode`: Merge mode display string (e.g. "Smart MKV").
/// - `card_config`: Card configuration, if cards were enabled.
/// - `split_config`: Split configuration, if split was enabled.
/// - `repeat_config`: Repeat configuration, if repeat was enabled.
/// - `original_duration`: Original duration before repeat expansion.
#[allow(clippy::too_many_arguments)]
pub fn build_report_data(
    segments: &[MergeSegment],
    parts: Option<&[MergePartResult]>,
    output_path: &str,
    output_size_bytes: u64,
    total_duration: f64,
    mode: Option<&str>,
    card_config: Option<&CardConfig>,
    split_config: Option<&SplitConfig>,
    repeat_config: Option<&RepeatConfig>,
    original_duration: Option<f64>,
) -> MergeReport {
    let now = chrono::Local::now();
    let generated_at = now.to_rfc3339();
    let generated_at_formatted = now.format("%Y-%m-%d %H:%M:%S").to_string();

    let output_name = std::path::Path::new(output_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(output_path)
        .to_string();

    let size_mb = output_size_bytes as f64 / 1_048_576.0;
    let total_size_formatted = format!("{:.1} MB", size_mb);

    // ── Count cards vs videos ─────────────────────────────────────────────
    let video_count = segments.iter().filter(|s| s.is_card != Some(true)).count();
    let card_count = segments.iter().filter(|s| s.is_card == Some(true)).count();
    // Use total segment count for backward-compatible file_count
    let total_entry_count = segments.len();

    // ── Build ordered timeline (videos + cards + folder headers) ────────────
    let mut timeline: Vec<TimelineEntry> = Vec::with_capacity(segments.len() + video_count);
    let mut current_folder: Option<String> = None;
    let mut display_index: usize = 0;

    for seg in segments {
        // Detect folder change → insert FolderHeader
        let folder_changed = match (&seg.parent_folder, &current_folder) {
            (Some(pf), Some(cf)) => pf != cf,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        };

        if folder_changed {
            current_folder = seg.parent_folder.clone();
            let folder_name = current_folder
                .as_ref()
                .map(|pf| format!("📁 {}", pf))
                .unwrap_or_else(|| "📁 Root / Other".to_string());
            timeline.push(TimelineEntry::FolderHeader { name: folder_name });
        }

        // Determine entry type
        let is_card = seg.is_card == Some(true);
        let dur_str = format_duration(seg.duration);
        let start_str = format_duration(seg.start_time);
        let end_str = format_duration(seg.end_time);

        if is_card {
            display_index += 1;
            // Determine card type from card_config frequency
            let card_type = card_config
                .map(|cc| match cc.frequency {
                    CardFrequency::PerVideo => CardType::PerVideo,
                    CardFrequency::PerFolder => CardType::PerFolder,
                })
                .unwrap_or(CardType::PerVideo);
            timeline.push(TimelineEntry::Card {
                entry_index: display_index,
                name: seg.name.clone(),
                duration: seg.duration,
                duration_formatted: dur_str,
                start_time: seg.start_time,
                start_time_formatted: start_str,
                end_time: seg.end_time,
                end_time_formatted: end_str,
                color: seg.card_color.clone(),
                card_type,
            });
        } else {
            display_index += 1;
            timeline.push(TimelineEntry::Video {
                index: display_index,
                name: seg.name.clone(),
                duration: seg.duration,
                duration_formatted: dur_str,
                start_time: seg.start_time,
                start_time_formatted: start_str,
                end_time: seg.end_time,
                end_time_formatted: end_str,
                parent_folder: seg.parent_folder.clone(),
            });
        }
    }

    // ── Compute stats ────────────────────────────────────────────────────
    let original_duration_param = original_duration;
    let original_duration: f64 = segments
        .iter()
        .filter(|s| s.is_card != Some(true))
        .map(|s| s.duration)
        .sum();
    let added_time = total_duration - original_duration;
    let final_duration = total_duration;

    let stats = ReportStats {
        video_count,
        card_count,
        original_duration,
        original_duration_formatted: format_duration(original_duration),
        final_duration,
        final_duration_formatted: format_duration(final_duration),
        added_time: added_time.max(0.0),
        added_time_formatted: format_duration(added_time.max(0.0)),
    };

    // ── Compute folder breakdown ──────────────────────────────────────────
    let folder_breakdown = build_folder_breakdown(segments);

    // ── Build split summary ──────────────────────────────────────────────
    let split_summary = build_split_summary(parts, split_config);

    // ── Build repeat summary ──────────────────────────────────────────────
    let repeat_summary = repeat_config.and_then(|rc| {
        if !rc.enabled {
            return None;
        }
        let mode = if rc.by_count && rc.until_duration {
            Some("Both".to_string())
        } else if rc.by_count {
            Some("By Count".to_string())
        } else if rc.until_duration {
            Some("Until Duration".to_string())
        } else {
            None
        };
        Some(RepeatSummary {
            enabled: true,
            mode,
            repeat_count: Some(rc.repeat_count),
            original_duration: original_duration_param,
            final_duration: Some(total_duration),
        })
    });

    // ── Header ───────────────────────────────────────────────────────────
    let mode_str = mode.unwrap_or("").to_string();
    let header = ReportHeader {
        output_path: output_path.to_string(),
        output_name,
        generated_at,
        generated_at_formatted,
        mode: mode_str,
        total_duration,
        total_duration_formatted: format_duration(total_duration),
        total_size_bytes: output_size_bytes,
        total_size_formatted,
        file_count: total_entry_count,
        card_count,
        job_id: None,
    };

    MergeReport {
        header,
        timeline,
        stats,
        folder_breakdown,
        split_summary,
        repeat_summary,
        recovery_summary: None,
    }
}

/// Compute per-folder breakdown from segments.
fn build_folder_breakdown(segments: &[MergeSegment]) -> Vec<FolderBreakdown> {
    let mut folders: HashMap<String, FolderAccumulator> = HashMap::new();

    for seg in segments {
        let is_card = seg.is_card == Some(true);
        let folder_name = match &seg.parent_folder {
            Some(name) => name.clone(),
            None => continue, // Skip segments without a parent folder
        };

        let entry = folders.entry(folder_name).or_default();
        if is_card {
            entry.card_count += 1;
        } else {
            entry.video_count += 1;
            entry.total_duration += seg.duration;
        }
    }

    let mut breakdown: Vec<FolderBreakdown> = folders
        .into_iter()
        .map(|(name, acc)| FolderBreakdown {
            name,
            video_count: acc.video_count,
            card_count: acc.card_count,
            total_duration: acc.total_duration,
            total_duration_formatted: format_duration(acc.total_duration),
        })
        .collect();

    // Stable order: sort by name for deterministic output
    breakdown.sort_by(|a, b| a.name.cmp(&b.name));
    breakdown
}

/// Build split summary from part data and config.
fn build_split_summary(
    parts: Option<&[MergePartResult]>,
    config: Option<&SplitConfig>,
) -> Option<SplitSummary> {
    let config = config?;
    if !matches!(config.mode, crate::types::SplitMode::Count | crate::types::SplitMode::Duration | crate::types::SplitMode::Folder) {
        return None;
    }

    let part_count = config.part_count.unwrap_or(0);
    if part_count == 0 && parts.is_none() {
        return None;
    }

    let mode_str = format!("{:?}", config.mode);
    let actual_parts = match parts {
        Some(parts) => parts
            .iter()
            .map(|p| SplitPartInfo {
                index: p.part_index,
                output_path: p.output_path.clone(),
                duration: p.total_duration,
                file_count: p.file_count,
            })
            .collect(),
        None => Vec::new(),
    };

    Some(SplitSummary {
        enabled: true,
        mode: mode_str,
        part_count: part_count.max(actual_parts.len() as u32),
        parts: actual_parts,
    })
}

/// Format seconds as HH:MM:SS.
fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}
