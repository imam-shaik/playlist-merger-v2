use std::fmt::Write as FmtWrite;
use std::path::Path;

use crate::report::models::*;

/// Render a MergeReport to a Markdown (.md) file alongside the output video.
///
/// Produces a structured Markdown document with sections for:
/// - Header / Overview
/// - Statistics
/// - Timeline (videos, cards, folder headers, split sections)
/// - Folder Breakdown
/// - Repeat Summary (if present)
/// - Split Summary (if present)
/// - Recovery Summary (if present)
pub fn render_markdown_report(
    report: &MergeReport,
    output_path: &str,
) -> Option<String> {
    let report_path = {
        let p = Path::new(output_path);
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("merged");
        let parent = p.parent().unwrap_or(Path::new(""));
        parent.join(format!("{}_report.md", stem))
    };

    let mut md = String::new();
    let _ = writeln!(md, "# Merge Report\n");

    // ── Header / Overview ─────────────────────────────────────────────────
    let _ = writeln!(md, "## Overview\n");
    let _ = writeln!(md, "| Field | Value |");
    let _ = writeln!(md, "|-------|-------|");
    let _ = writeln!(md, "| **Output** | `{}` |", report.header.output_name);
    let _ = writeln!(md, "| **Path** | `{}` |", report.header.output_path);
    let _ = writeln!(md, "| **Generated** | {} |", report.header.generated_at_formatted);
    let _ = writeln!(md, "| **Mode** | {} |", report.header.mode);
    let _ = writeln!(md, "| **Total Duration** | {} |", report.header.total_duration_formatted);
    let _ = writeln!(md, "| **File Size** | {} |", report.header.total_size_formatted);
    let _ = writeln!(md, "| **Files** | {} |", report.header.file_count);
    if report.header.card_count > 0 {
        let _ = writeln!(md, "| **Cards** | {} |", report.header.card_count);
    }
    if let Some(ref jid) = report.header.job_id {
        let _ = writeln!(md, "| **Job ID** | `{}` |", jid);
    }
    let _ = writeln!(md);

    // ── Statistics ────────────────────────────────────────────────────────
    let _ = writeln!(md, "---\n");
    let _ = writeln!(md, "## Statistics\n");
    let _ = writeln!(md, "| Metric | Value |");
    let _ = writeln!(md, "|--------|-------|");
    let _ = writeln!(md, "| Videos | {} |", report.stats.video_count);
    let _ = writeln!(md, "| Cards | {} |", report.stats.card_count);
    let _ = writeln!(md, "| Original Duration | {} |", report.stats.original_duration_formatted);
    let _ = writeln!(md, "| Final Duration | {} |", report.stats.final_duration_formatted);
    let _ = writeln!(md, "| Added Time | {} |", report.stats.added_time_formatted);
    let _ = writeln!(md);

    // ── Timeline ──────────────────────────────────────────────────────────
    let _ = writeln!(md, "---\n");
    let _ = writeln!(md, "## Timeline\n");
    let _ = writeln!(md, "| # | Name | Duration | Time Range |");
    let _ = writeln!(md, "|---|------|----------|------------|");

    for entry in &report.timeline {
        match entry {
            TimelineEntry::FolderHeader { name } => {
                let _ = writeln!(md, "| | **{}** | | |", escape_markdown(name));
            }
            TimelineEntry::Video { index, name, duration_formatted, start_time_formatted, end_time_formatted, .. } => {
                let _ = writeln!(
                    md,
                    "| {} | {} | {} | {} → {} |",
                    index, escape_markdown(name), duration_formatted,
                    start_time_formatted, end_time_formatted,
                );
            }
            TimelineEntry::Card { entry_index, name, duration_formatted, start_time_formatted, end_time_formatted, card_type, .. } => {
                let card_icon = match card_type {
                    CardType::PerVideo => "🃏",
                    CardType::PerFolder => "🃏",
                    CardType::Repeat => "🔁",
                };
                let _ = writeln!(
                    md,
                    "| {} | {} {} | {} | {} → {} |",
                    entry_index, card_icon, escape_markdown(name), duration_formatted,
                    start_time_formatted, end_time_formatted,
                );
            }
            TimelineEntry::SplitSection { label, duration_formatted, file_count, .. } => {
                let _ = writeln!(
                    md,
                    "| | 📦 **{}** ({} files) | {} | |",
                    label, file_count, duration_formatted,
                );
            }
        }
    }
    let _ = writeln!(md);

    // ── Folder Breakdown ──────────────────────────────────────────────────
    if !report.folder_breakdown.is_empty() {
        let _ = writeln!(md, "---\n");
        let _ = writeln!(md, "## Folder Breakdown\n");
        let _ = writeln!(md, "| Folder | Videos | Cards | Duration |");
        let _ = writeln!(md, "|--------|--------|-------|----------|");
        for fb in &report.folder_breakdown {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} |",
                escape_markdown(&fb.name), fb.video_count, fb.card_count,
                fb.total_duration_formatted,
            );
        }
        let _ = writeln!(md);
    }

    // ── Repeat Summary ────────────────────────────────────────────────────
    if let Some(ref rs) = report.repeat_summary {
        let _ = writeln!(md, "---\n");
        let _ = writeln!(md, "## Repeat Summary\n");
        let _ = writeln!(md, "| Setting | Value |");
        let _ = writeln!(md, "|---------|-------|");
        let _ = writeln!(md, "| **Enabled** | {} |", if rs.enabled { "Yes" } else { "No" });
        if let Some(ref mode) = rs.mode {
            let _ = writeln!(md, "| **Mode** | {} |", mode);
        }
        if let Some(count) = rs.repeat_count {
            let _ = writeln!(md, "| **Repeat Count** | {} |", count);
        }
        if let Some(orig_dur) = rs.original_duration {
            let _ = writeln!(md, "| **Original Duration** | {} |", format_duration(orig_dur));
        }
        if let Some(final_dur) = rs.final_duration {
            let _ = writeln!(md, "| **Final Duration** | {} |", format_duration(final_dur));
        }
        let _ = writeln!(md);
    }

    // ── Split Summary ─────────────────────────────────────────────────────
    if let Some(ref ss) = report.split_summary {
        let _ = writeln!(md, "---\n");
        let _ = writeln!(md, "## Split Summary\n");
        let _ = writeln!(md, "| Setting | Value |");
        let _ = writeln!(md, "|---------|-------|");
        let _ = writeln!(md, "| **Mode** | {} |", ss.mode);
        let _ = writeln!(md, "| **Parts** | {} |", ss.part_count);
        let _ = writeln!(md);
        if !ss.parts.is_empty() {
            let _ = writeln!(md, "### Parts\n");
            let _ = writeln!(md, "| # | Path | Duration | Files |");
            let _ = writeln!(md, "|---|------|----------|-------|");
            for part in &ss.parts {
                let _ = writeln!(
                    md,
                    "| {} | `{}` | {} | {} |",
                    part.index, escape_markdown(&part.output_path),
                    format_duration(part.duration), part.file_count,
                );
            }
            let _ = writeln!(md);
        }
    }

    // ── Recovery Summary ──────────────────────────────────────────────────
    if let Some(ref rcv) = report.recovery_summary {
        let _ = writeln!(md, "---\n");
        let _ = writeln!(md, "## Recovery Summary\n");
        let _ = writeln!(md, "| Setting | Value |");
        let _ = writeln!(md, "|---------|-------|");
        let _ = writeln!(md, "| **Resumed** | {} |", if rcv.resumed { "Yes" } else { "No" });
        if let Some(age) = rcv.checkpoint_age_secs {
            let _ = writeln!(md, "| **Checkpoint Age** | {} |", format_duration(age));
        }
        let _ = writeln!(md, "| **Skipped Files** | {} |", rcv.skipped_files);
        let _ = writeln!(md, "| **Reused Files** | {} |", rcv.reused_files);
        let _ = writeln!(md, "| **Normalized Remaining** | {} |", rcv.normalized_remaining);
        let _ = writeln!(md);
    }

    // ── Footer ────────────────────────────────────────────────────────────
    let _ = writeln!(md, "---\n");
    let _ = writeln!(md, "*Report generated by Playlist Merger v2 — {0}*", 
        report.header.generated_at_formatted);

    // ── Write file ────────────────────────────────────────────────────────
    if std::fs::write(&report_path, &md).is_ok() {
        Some(report_path.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Escape pipe characters in Markdown table cells to prevent layout breakage.
fn escape_markdown(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Format seconds as HH:MM:SS.
fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::merge::MergeSegment;

    fn video(name: &str, dur: f64, start: f64, folder: Option<&str>) -> MergeSegment {
        MergeSegment {
            name: name.to_string(),
            duration: dur,
            start_time: start,
            end_time: start + dur,
            is_card: Some(false),
            card_color: None,
            parent_folder: folder.map(|s| s.to_string()),
        }
    }

    fn card(name: &str, dur: f64, start: f64, color: Option<&str>, _card_type: CardType) -> MergeSegment {
        MergeSegment {
            name: name.to_string(),
            duration: dur,
            start_time: start,
            end_time: start + dur,
            is_card: Some(true),
            card_color: color.map(|s| s.to_string()),
            parent_folder: None,
        }
    }

    fn build_minimal_report(
        segments: &[MergeSegment],
        card_config: Option<&crate::types::CardConfig>,
        repeat_config: Option<&crate::types::RepeatConfig>,
        split_config: Option<&crate::types::SplitConfig>,
    ) -> MergeReport {
        let total_dur: f64 = segments.iter().map(|s| s.duration).sum();
        let original_dur: f64 = segments.iter()
            .filter(|s| s.is_card != Some(true))
            .map(|s| s.duration)
            .sum();
        crate::report::builder::build_report_data(
            segments,
            None,
            "/output/test.mp4",
            50_000_000,
            total_dur,
            Some("Smart MKV"),
            card_config,
            split_config,
            repeat_config,
            Some(original_dur),
        )
    }

    #[test]
    fn test_markdown_renderer_creates_content() {
        let segments = vec![video("Test Video", 120.0, 0.0, None)];
        let report = build_minimal_report(&segments, None, None, None);

        let tmp_dir = std::env::temp_dir().join(format!("md_test_basic_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let test_output = tmp_dir.join("basic.mp4").to_string_lossy().to_string();
        let result = render_markdown_report(&report, &test_output);

        assert!(result.is_some(), "Markdown file should be created");
        let content = std::fs::read_to_string(result.unwrap()).expect("Should read MD file");

        assert!(content.contains("# Merge Report"), "Should have title");
        assert!(content.contains("## Overview"), "Should have overview");
        assert!(content.contains("## Statistics"), "Should have statistics");
        assert!(content.contains("## Timeline"), "Should have timeline");
        assert!(content.contains("Test Video"), "Should contain video name");
        assert!(content.contains("Smart MKV"), "Should contain mode");
        assert!(content.contains("00:02:00"), "Should contain formatted duration");

        // These sections should be absent when no extra data provided
        assert!(!content.contains("Folder Breakdown"), "No folder breakdown");
        assert!(!content.contains("Repeat Summary"), "No repeat summary");
        assert!(!content.contains("Split Summary"), "No split summary");
        assert!(!content.contains("Recovery Summary"), "No recovery summary");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_markdown_includes_all_sections_when_present() {
        use crate::types::*;

        let card_cfg = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 6.0,
            show_in_report: true,
            frequency: CardFrequency::PerVideo,
        };
        let repeat_cfg = RepeatConfig {
            enabled: true,
            by_count: true,
            repeat_count: 3,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: false,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        };
        let split_cfg = SplitConfig {
            mode: SplitMode::Count,
            folder_split_mode: None,
            part_count: Some(2),
            max_duration_per_part: None,
            subtitle_mode: None,
        };

        let segments = vec![
            video("A.mp4", 60.0, 0.0, Some("folder_x")),
            card("Section Card", 6.0, 60.0, Some("#3366FF"), CardType::PerVideo),
            video("B.mp4", 60.0, 66.0, Some("folder_y")),
        ];
        let report = build_minimal_report(&segments, Some(&card_cfg), Some(&repeat_cfg), Some(&split_cfg));

        // Render to a temp file
        let tmp_dir = std::env::temp_dir().join(format!("md_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let test_output = tmp_dir.join("test_output.mp4").to_string_lossy().to_string();
        let result = render_markdown_report(&report, &test_output);

        assert!(result.is_some(), "Markdown file should be created");
        let path = result.unwrap();
        let content = std::fs::read_to_string(&path).expect("Should read MD file");

        // Check all sections are present
        assert!(content.contains("# Merge Report"), "Should have title");
        assert!(content.contains("## Overview"), "Should have overview");
        assert!(content.contains("## Statistics"), "Should have statistics");
        assert!(content.contains("## Timeline"), "Should have timeline");
        assert!(content.contains("## Folder Breakdown"), "Should have folder breakdown");
        assert!(content.contains("## Repeat Summary"), "Should have repeat summary");
        assert!(content.contains("## Split Summary"), "Should have split summary");

        // Check specific data appears
        assert!(content.contains("Smart MKV"), "Should contain mode");
        assert!(content.contains("A.mp4"), "Should contain video name");
        assert!(content.contains("🃏"), "Should contain card emoji");
        assert!(content.contains("folder_x"), "Should contain folder name");
        assert!(content.contains("3"), "Should contain repeat count");

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_markdown_skip_sections_when_empty() {
        let segments = vec![video("Only Video", 60.0, 0.0, None)];
        let report = build_minimal_report(&segments, None, None, None);

        let tmp_dir = std::env::temp_dir().join(format!("md_test_skip_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let test_output = tmp_dir.join("skip.mp4").to_string_lossy().to_string();
        let result = render_markdown_report(&report, &test_output);
        assert!(result.is_some());
        let content = std::fs::read_to_string(result.unwrap()).expect("Should read MD file");

        // These sections should NOT appear when the data is absent
        assert!(!content.contains("Folder Breakdown"), "No folder breakdown without folders");
        assert!(!content.contains("Repeat Summary"), "No repeat summary without repeat");
        assert!(!content.contains("Split Summary"), "No split summary without split");
        assert!(!content.contains("Recovery Summary"), "No recovery summary without recovery");

        // Core sections should always be present
        assert!(content.contains("## Overview"));
        assert!(content.contains("## Statistics"));
        assert!(content.contains("## Timeline"));
        assert!(content.contains("Only Video"));

        let _ = std::fs::write(&test_output, "");
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_markdown_escape_pipes_in_names() {
        let segments = vec![video("File | A & B", 60.0, 0.0, Some("Folder | X"))];
        let report = build_minimal_report(&segments, None, None, None);

        let tmp_dir = std::env::temp_dir().join(format!("md_test_escape_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let test_output = tmp_dir.join("escape.mp4").to_string_lossy().to_string();
        let result = render_markdown_report(&report, &test_output);
        assert!(result.is_some());
        let content = std::fs::read_to_string(result.unwrap()).expect("Should read MD file");

        // Pipes in names should be escaped
        assert!(content.contains("File \\| A & B"), "Pipe in video name should be escaped");
        assert!(content.contains("Folder \\| X"), "Pipe in folder name should be escaped");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_markdown_recovery_summary() {
        // Build a report with recovery data (manually, since builder doesn't set it yet)
        let segments = vec![video("Recovered", 60.0, 0.0, None)];
        let report = build_minimal_report(&segments, None, None, None);
        let mut report = report;
        report.recovery_summary = Some(RecoverySummary {
            resumed: true,
            checkpoint_age_secs: Some(154.0),
            skipped_files: 5,
            reused_files: 3,
            normalized_remaining: 2,
        });

        let tmp_dir = std::env::temp_dir().join(format!("md_test_recov_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp_dir);
        let test_output = tmp_dir.join("recovered.mp4").to_string_lossy().to_string();
        let result = render_markdown_report(&report, &test_output);
        assert!(result.is_some());
        let content = std::fs::read_to_string(result.unwrap()).expect("Should read MD file");

        assert!(content.contains("## Recovery Summary"), "Should show recovery section");
        assert!(content.contains("Yes"), "Should show resumed = Yes");
        assert!(content.contains("00:02:34"), "Should show checkpoint age formatted");
        assert!(content.contains("5"), "Should show skipped files");
        assert!(content.contains("3"), "Should show reused files");
        assert!(content.contains("2"), "Should show normalized remaining");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
