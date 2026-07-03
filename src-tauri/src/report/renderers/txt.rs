use std::path::Path;
use std::fmt::Write as FmtWrite;

use crate::report::models::*;

/// Render a MergeReport to a TXT file at the standard report path.
///
/// Produces output identical to the legacy `write_report_file()` format
/// so that existing consumers (split parser, user expectations) are not broken.
pub fn render_txt_report(
    report: &MergeReport,
    output_path: &str,
) -> Option<String> {
    let report_path = {
        let p = Path::new(output_path);
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("merged");
        let parent = p.parent().unwrap_or(Path::new(""));
        parent.join(format!("{}_report.txt", stem))
    };

    let mut content = String::new();
    let _ = writeln!(content, "╔══════════════════════════════════════════════════════════════╗");
    let _ = writeln!(content, "║                       MERGE REPORT                          ║");
    let _ = writeln!(content, "╚══════════════════════════════════════════════════════════════╝");
    let _ = writeln!(content);
    let _ = writeln!(content, "  Report Version : 2");
    let _ = writeln!(content, "  Generated  : {}", report.header.generated_at_formatted);
    let _ = writeln!(content, "  Output     : {}", report.header.output_path);
    let _ = writeln!(content, "  Total Time : {}", report.header.total_duration_formatted);
    let _ = writeln!(content, "  File Size  : {}", report.header.total_size_formatted);
    let _ = writeln!(content, "  Files      : {}", report.header.file_count);
    let _ = writeln!(content);

    // ── Table header ─────────────────────────────────────────────────────
    let _ = writeln!(content, "  ┌─────┬──────────────────────────────────────────┬──────────┬──────────────────────┬──────────┐");
    let _ = writeln!(content, "  │  #  │ File Name                                │ Duration │ In Merged (Start-End)│ Remaining│");
    let _ = writeln!(content, "  ├─────┼──────────────────────────────────────────┼──────────┼──────────────────────┼──────────┤");

    // ── Table body ───────────────────────────────────────────────────────
    let mut remaining = report.header.total_duration;
    for entry in &report.timeline {
        match entry {
            TimelineEntry::FolderHeader { name } => {
                let folder_trunc = truncate_for_table(name, 40);
                let _ = writeln!(
                    content,
                    "  │     │ {:<40} │          │                      │          │",
                    folder_trunc
                );
                let _ = writeln!(
                    content,
                    "  ├─────┼──────────────────────────────────────────┼──────────┼──────────────────────┼──────────┤"
                );
            }
            TimelineEntry::Video { index, name, duration_formatted, start_time_formatted, end_time_formatted, .. }
            | TimelineEntry::Card { entry_index: index, name, duration_formatted, start_time_formatted, end_time_formatted, .. } => {
                remaining -= entry.duration();
                let name_trunc = truncate_for_table(name, 40);
                let _ = writeln!(
                    content,
                    "  │ {:>3} │ {:<40} │ {:>8} │ {} → {} │ {:>8} │",
                    index,
                    name_trunc,
                    duration_formatted,
                    start_time_formatted,
                    end_time_formatted,
                    format_duration(remaining.max(0.0)),
                );
            }
            // SplitSection entries are not rendered in the legacy TXT format
            TimelineEntry::SplitSection { .. } => {}
        }
    }

    let _ = writeln!(content, "  └─────┴──────────────────────────────────────────┴──────────┴──────────────────────┴──────────┘");

    // ── Write file ───────────────────────────────────────────────────────
    if std::fs::write(&report_path, &content).is_ok() {
        Some(report_path.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Format seconds as HH:MM:SS (identical to legacy `format_duration`).
fn format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

/// Truncate a string to fit in a table column, appending "…" if truncated.
/// Matches the legacy behaviour: if len > max, take (max-1) chars + "…".
fn truncate_for_table(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}…", &s[..max.saturating_sub(1)])
    } else {
        s.to_string()
    }
}
