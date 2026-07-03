use crate::commands::merge::{MergeSegment, MergePartResult};
use crate::report::builder::build_report_data;
use crate::report::models::*;
use crate::types::{CardConfig, CardFrequency, RepeatConfig, SplitConfig, SplitMode};

// ═════════════════════════════════════════════════════════════════════════════
// Helper: construct a basic video segment
// ═════════════════════════════════════════════════════════════════════════════

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

fn card(name: &str, dur: f64, start: f64, color: Option<&str>) -> MergeSegment {
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

fn card_in_folder(name: &str, dur: f64, start: f64, folder: Option<&str>, color: Option<&str>) -> MergeSegment {
    MergeSegment {
        name: name.to_string(),
        duration: dur,
        start_time: start,
        end_time: start + dur,
        is_card: Some(true),
        card_color: color.map(|s| s.to_string()),
        parent_folder: folder.map(|s| s.to_string()),
    }
}

fn make_split_parts(count: u32) -> Vec<MergePartResult> {
    (0..count)
        .map(|i| MergePartResult {
            part_index: i + 1,
            output_path: format!("/output/part_{}.mp4", i + 1),
            output_size_bytes: 100_000_000,
            file_count: 5,
            total_duration: 600.0,
        })
        .collect()
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 1: Empty segments
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_empty_segments() {
    let segments: Vec<MergeSegment> = vec![];
    let report = build_report_data(
        &segments,
        None,
        "/output/video.mp4",
        0,
        0.0,
        None,
        None,
        None,
        None,
        None,
    );

    assert!(report.timeline.is_empty(), "Empty segments → empty timeline");
    assert_eq!(report.stats.video_count, 0, "No videos");
    assert_eq!(report.stats.card_count, 0, "No cards");
    assert_eq!(report.header.file_count, 0, "Zero file count");
    assert_eq!(report.header.card_count, 0, "Zero card count");
    assert_eq!(report.folder_breakdown.len(), 0, "No folder breakdown");
    assert!(report.split_summary.is_none(), "No split summary");
    assert!(report.repeat_summary.is_none(), "No repeat summary");
    assert!(report.recovery_summary.is_none(), "No recovery summary");
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 2: Single video (no cards, no folders)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_single_video() {
    let segments = vec![video("My Video", 120.0, 0.0, None)];
    let report = build_report_data(&segments, None, "/output/video.mp4", 50_000_000, 120.0, None, None, None, None, None);

    assert_eq!(report.timeline.len(), 1, "One timeline entry");
    assert_eq!(report.stats.video_count, 1);
    assert_eq!(report.stats.card_count, 0);
    assert_eq!(report.header.file_count, 1);
    assert_eq!(report.header.card_count, 0);
    assert_eq!(report.header.total_duration, 120.0);
    assert_eq!(report.stats.original_duration, 120.0);
    assert_eq!(report.stats.final_duration, 120.0);
    assert_eq!(report.stats.added_time, 0.0);

    match &report.timeline[0] {            TimelineEntry::Video { index, duration, .. } => {
                assert_eq!(*index, 1);
                assert_eq!(*duration, 120.0);
        }
        other => panic!("Expected Video entry, got {:?}", other),
    }

    assert_eq!(report.folder_breakdown.len(), 0, "No folders → no breakdown when parent_folder is None");
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 3: Multiple videos (no cards, no folders)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_multiple_videos() {
    let segments = vec![
        video("Video A", 60.0, 0.0, None),
        video("Video B", 90.0, 60.0, None),
        video("Video C", 30.0, 150.0, None),
    ];
    let total_dur: f64 = segments.iter().map(|s| s.duration).sum();
    let report = build_report_data(&segments, None, "/output/video.mp4", 100_000_000, total_dur, None, None, None, None, None);

    assert_eq!(report.timeline.len(), 3, "Three timeline entries");
    assert_eq!(report.stats.video_count, 3);
    assert_eq!(report.stats.card_count, 0);
    assert_eq!(report.header.file_count, 3);
    assert_eq!(report.header.card_count, 0);
    assert_eq!(report.stats.original_duration, 180.0);
    assert_eq!(report.stats.final_duration, 180.0);

    // Check indices are 1-based and sequential
    for (i, entry) in report.timeline.iter().enumerate() {
        match entry {
            TimelineEntry::Video { index, name: _, duration, .. } => {
                assert_eq!(*index, i + 1, "Index {} should be {}", index, i + 1);
                assert_eq!(*duration, segments[i].duration);
            }
            other => panic!("Entry {} expected Video, got {:?}", i, other),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 4: Cards ON — PerVideo mode
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_cards_per_video() {
    let card_cfg = CardConfig {
        color: "#3366FF".to_string(),
        font_color: "#FFFFFF".to_string(),
        duration: 6.0,
        show_in_report: true,
        frequency: CardFrequency::PerVideo,
    };

    // 2 videos + 1 card between them = 3 segments
    let segments = vec![
        video("Lesson 1", 300.0, 0.0, Some("module_a")),
        card_in_folder("▶ Section: Module A", 6.0, 300.0, Some("module_a"), Some("#3366FF")),
        video("Lesson 2", 400.0, 306.0, Some("module_a")),
    ];
    let total_dur: f64 = segments.iter().map(|s| s.duration).sum();

    let report = build_report_data(&segments, None, "/output/video.mp4", 200_000_000, total_dur, None, Some(&card_cfg), None, None, None);

    // Timeline: [0] FolderHeader, [1] Video, [2] Card, [3] Video
    assert_eq!(report.timeline.len(), 4, "4 entries: folder header + video + card + video");
    assert_eq!(report.stats.video_count, 2, "2 videos");
    assert_eq!(report.stats.card_count, 1, "1 card");

    assert_eq!(report.header.file_count, 3, "file_count = total segments (3)");
    assert_eq!(report.header.card_count, 1, "card_count = 1");

    // Verify order
    assert!(report.timeline[0].is_folder_header(), "Entry 0: FolderHeader");
    assert!(report.timeline[1].is_video(), "Entry 1: Video");
    assert!(report.timeline[2].is_card(), "Entry 2: Card");
    assert!(report.timeline[3].is_video(), "Entry 3: Video");

    // Verify card type
    match &report.timeline[2] {
        TimelineEntry::Card { card_type, color, .. } => {
            assert!(matches!(card_type, CardType::PerVideo), "Card type should be PerVideo");
            assert_eq!(color.as_ref().unwrap(), "#3366FF");
        }
        other => panic!("Entry 2 expected Card, got {:?}", other),
    }

    // Statistics: original = videos only (700s), final = total (706s)
    assert_eq!(report.stats.original_duration, 700.0);
    assert_eq!(report.stats.final_duration, total_dur);
    assert!((report.stats.added_time - 6.0).abs() < 0.001, "Added time should be card duration (6s)");

    // Folder breakdown should have 1 entry for module_a
    assert_eq!(report.folder_breakdown.len(), 1, "One folder in breakdown");
    assert_eq!(report.folder_breakdown[0].video_count, 2);
    assert_eq!(report.folder_breakdown[0].card_count, 1);
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 5: Cards ON — PerFolder mode (no card should appear within same folder)
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_cards_per_folder_same_folder() {
    let card_cfg = CardConfig {
        color: "#3366FF".to_string(),
        font_color: "#FFFFFF".to_string(),
        duration: 6.0,
        show_in_report: true,
        frequency: CardFrequency::PerFolder,
    };

    // 3 segments, all in same folder, no card segments
    let segments = vec![
        video("Lesson 1", 300.0, 0.0, Some("grammar")),
        video("Lesson 2", 400.0, 300.0, Some("grammar")),
        video("Lesson 3", 200.0, 700.0, Some("grammar")),
    ];
    let total_dur: f64 = segments.iter().map(|s| s.duration).sum();

    let report = build_report_data(&segments, None, "/output/video.mp4", 200_000_000, total_dur, None, Some(&card_cfg), None, None, None);

    // All in same folder → one folder header at start, then 3 videos (no cards)
    assert_eq!(report.timeline.len(), 4, "FolderHeader + 3 videos");
    assert!(report.timeline[0].is_folder_header());
    assert!(report.timeline[1].is_video());
    assert!(report.timeline[2].is_video());
    assert!(report.timeline[3].is_video());
    assert_eq!(report.stats.card_count, 0, "No card segments → card_count=0");
    assert_eq!(report.stats.video_count, 3);
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 6: Cards ON — PerFolder mode crossing folder boundaries
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_cards_per_folder_cross_boundary() {
    // With PerFolder mode + actual card segments at folder boundaries
    let card_cfg = CardConfig {
        color: "#FF6633".to_string(),
        font_color: "#FFFFFF".to_string(),
        duration: 6.0,
        show_in_report: true,
        frequency: CardFrequency::PerFolder,
    };

    let segments = vec![
        video("Intro", 120.0, 0.0, Some("intro")),
        card_in_folder("▶ Section: Intro", 6.0, 120.0, Some("intro"), Some("#FF6633")),
        video("Main Topic", 300.0, 126.0, Some("main")),
        card_in_folder("▶ Section: Main", 6.0, 426.0, Some("main"), Some("#FF6633")),
        video("Conclusion", 60.0, 432.0, Some("outro")),
    ];
    let total_dur: f64 = segments.iter().map(|s| s.duration).sum();

    let report = build_report_data(&segments, None, "/output/video.mp4", 200_000_000, total_dur, None, Some(&card_cfg), None, None, None);

    // Timeline: FolderHeader(intro) + Video + FolderHeader(main) + Video + FolderHeader(outro) + Video + ...cards?
    // Actually, cards are segments themselves in this test, so they appear after their respective folders
    
    // Let me just check counts are correct
    let video_entries: Vec<&TimelineEntry> = report.timeline.iter().filter(|e| e.is_video()).collect();
    let card_entries: Vec<&TimelineEntry> = report.timeline.iter().filter(|e| e.is_card()).collect();
    let folder_entries: Vec<&TimelineEntry> = report.timeline.iter().filter(|e| e.is_folder_header()).collect();

    assert_eq!(video_entries.len(), 3, "3 videos");
    assert_eq!(card_entries.len(), 2, "2 cards (one per folder boundary)");
    assert_eq!(folder_entries.len(), 3, "3 folder headers (intro, main, outro)");

    // Verify card types
    for card_entry in &card_entries {
        match card_entry {
            TimelineEntry::Card { card_type, .. } => {
                assert!(matches!(card_type, CardType::PerFolder), "Card type should be PerFolder");
            }
            _ => unreachable!(),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 7: Folder grouping — multiple folders, no cards
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_folder_grouping() {
    let segments = vec![
        video("A1", 60.0, 0.0, Some("folder_a")),
        video("A2", 60.0, 60.0, Some("folder_a")),
        video("B1", 60.0, 120.0, Some("folder_b")),
        video("C1", 60.0, 180.0, Some("folder_c")),
        video("C2", 60.0, 240.0, Some("folder_c")),
        video("C3", 60.0, 300.0, Some("folder_c")),
    ];
    let total_dur: f64 = segments.iter().map(|s| s.duration).sum();
    let report = build_report_data(&segments, None, "/output/video.mp4", 300_000_000, total_dur, None, None, None, None, None);

    // Timeline: FHA + A1 + A2 + FHB + B1 + FHC + C1 + C2 + C3
    assert_eq!(report.timeline.len(), 9, "3 folder headers + 6 videos");

    let folder_headers: Vec<&str> = report.timeline.iter().filter_map(|e| {
        if let TimelineEntry::FolderHeader { name } = e { Some(name.as_str()) } else { None }
    }).collect();
    assert_eq!(folder_headers, vec!["📁 folder_a", "📁 folder_b", "📁 folder_c"]);

    // Folder breakdown
    assert_eq!(report.folder_breakdown.len(), 3, "3 folders in breakdown");
    let fb_a = report.folder_breakdown.iter().find(|f| f.name == "folder_a").unwrap();
    assert_eq!(fb_a.video_count, 2);
    assert_eq!(fb_a.card_count, 0);
    assert_eq!(fb_a.total_duration, 120.0);

    let fb_c = report.folder_breakdown.iter().find(|f| f.name == "folder_c").unwrap();
    assert_eq!(fb_c.video_count, 3);
    assert_eq!(fb_c.total_duration, 180.0);
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 8: Split summary
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_split_summary() {
    let segments = vec![video("Video", 600.0, 0.0, None)];
    let parts = make_split_parts(3);
    let split_cfg = SplitConfig {
        mode: SplitMode::Count,
        folder_split_mode: None,
        part_count: Some(3),
        max_duration_per_part: None,
        subtitle_mode: None,
    };

    let report = build_report_data(
        &segments,
        Some(parts.as_slice()),
        "/output/video.mp4",
        300_000_000,
        600.0,
        None,
        None,
        Some(&split_cfg),
        None,
        None,
    );

    assert!(report.split_summary.is_some(), "Split summary should exist");
    let ss = report.split_summary.as_ref().unwrap();
    assert!(ss.enabled);
    assert!(ss.part_count >= 3, "Part count >= 3");
    assert_eq!(ss.parts.len(), 3, "3 parts in summary");
    assert_eq!(ss.parts[0].index, 1);
    assert_eq!(ss.parts[0].output_path, "/output/part_1.mp4");
    assert_eq!(ss.parts[0].file_count, 5);
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 9: Split disabled — no summary
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_split_disabled() {
    let segments = vec![video("Video", 600.0, 0.0, None)];
    let split_cfg = SplitConfig {
        mode: SplitMode::None,
        folder_split_mode: None,
        part_count: None,
        max_duration_per_part: None,
        subtitle_mode: None,
    };

    let report = build_report_data(
        &segments,
        None,
        "/output/video.mp4",
        300_000_000,
        600.0,
        None,
        None,
        Some(&split_cfg),
        None,
        None,
    );

    assert!(report.split_summary.is_none(), "Split disabled → no summary");
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 10: Statistics — correct original vs final duration with cards
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_statistics_with_cards() {
    let segments = vec![
        video("Part 1", 300.0, 0.0, None),
        card("Canvas", 10.0, 300.0, Some("#000000")),
        video("Part 2", 200.0, 310.0, None),
        card("Canvas 2", 5.0, 510.0, Some("#000000")),
        video("Part 3", 100.0, 515.0, None),
    ];
    let total_dur: f64 = segments.iter().map(|s| s.duration).sum();
    let original_dur: f64 = segments.iter().filter(|s| s.is_card != Some(true)).map(|s| s.duration).sum();
    let added_time = total_dur - original_dur;

    let report = build_report_data(&segments, None, "/output/video.mp4", 300_000_000, total_dur, None, None, None, None, None);

    assert_eq!(report.stats.video_count, 3);
    assert_eq!(report.stats.card_count, 2);
    assert!((report.stats.original_duration - original_dur).abs() < 0.001,
        "original_duration should be {} got {}", original_dur, report.stats.original_duration);
    assert!((report.stats.final_duration - total_dur).abs() < 0.001);
    assert!((report.stats.added_time - added_time).abs() < 0.001,
        "added_time should be {} got {}", added_time, report.stats.added_time);
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 11: Mode display string passed through
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_mode_string() {
    let segments = vec![video("Test", 60.0, 0.0, None)];
    let report = build_report_data(&segments, None, "/output/video.mp4", 10_000_000, 60.0, Some("Smart MKV"), None, None, None, None);

    assert_eq!(report.header.mode, "Smart MKV");
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 12: Output file name extraction from path
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_output_name() {
    let segments = vec![video("Test", 60.0, 0.0, None)];
    let report = build_report_data(&segments, None, "/output/Course.mkv", 10_000_000, 60.0, None, None, None, None, None);

    assert_eq!(report.header.output_name, "Course.mkv");
    assert_eq!(report.header.total_size_formatted, "9.5 MB");
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 13: Mixed — videos + cards + folders + splits
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_mixed_workflow() {
    let card_cfg = CardConfig {
        color: "#3366FF".to_string(),
        font_color: "#FFFFFF".to_string(),
        duration: 6.0,
        show_in_report: true,
        frequency: CardFrequency::PerVideo,
    };

    // Simulate a real course: 2 folders, cards between videos
    let segments = vec![
        // Folder A: 2 videos + 1 card
        video("Intro", 120.0, 0.0, Some("module_a")),
        card_in_folder("▶ Section A", 6.0, 120.0, Some("module_a"), Some("#3366FF")),
        video("Core Concepts", 600.0, 126.0, Some("module_a")),
        // Folder B: 1 video + 1 card
        card_in_folder("▶ Section B", 6.0, 726.0, Some("module_b"), Some("#3366FF")),
        video("Advanced Topics", 500.0, 732.0, Some("module_b")),
    ];
    let total_dur: f64 = segments.iter().map(|s| s.duration).sum();
    let parts = make_split_parts(2);
    let split_cfg = SplitConfig {
        mode: SplitMode::Duration,
        folder_split_mode: None,
        part_count: Some(2),
        max_duration_per_part: Some(700.0),
        subtitle_mode: None,
    };

    let report = build_report_data(
        &segments,
        Some(parts.as_slice()),
        "/output/Course.mp4",
        500_000_000,
        total_dur,
        Some("Smart MKV"),
        Some(&card_cfg),
        Some(&split_cfg),
        None,
        None,
    );

    // Verify timeline structure
    assert_eq!(report.timeline.len(), 7, "7 entries: 2 folder headers + 2 videos + 2 cards + 1 video (last no card after)");

    let video_count = report.timeline.iter().filter(|e| e.is_video()).count();
    let card_count = report.timeline.iter().filter(|e| e.is_card()).count();
    let folder_count = report.timeline.iter().filter(|e| e.is_folder_header()).count();

    assert_eq!(video_count, 3, "3 videos total");
    assert_eq!(card_count, 2, "2 cards");
    assert_eq!(folder_count, 2, "2 folder headers");

    // Header
    assert_eq!(report.header.mode, "Smart MKV");
    assert_eq!(report.header.file_count, 5, "file_count = total segments (5)");

    // Stats
    assert_eq!(report.stats.video_count, 3);
    assert_eq!(report.stats.card_count, 2);
    assert!((report.stats.original_duration - 1220.0).abs() < 0.001,
        "Original duration should be 1220s (videos only)");
    assert!((report.stats.final_duration - total_dur).abs() < 0.001);

    // Split summary
    assert!(report.split_summary.is_some());
    let ss = report.split_summary.as_ref().unwrap();
    assert_eq!(ss.parts.len(), 2);
    assert_eq!(ss.mode, "Duration");

    // Folder breakdown
    assert_eq!(report.folder_breakdown.len(), 2);
    let fb_a = report.folder_breakdown.iter().find(|f| f.name == "module_a").unwrap();
    assert_eq!(fb_a.video_count, 2);
    assert_eq!(fb_a.card_count, 1);
    let fb_b = report.folder_breakdown.iter().find(|f| f.name == "module_b").unwrap();
    assert_eq!(fb_b.video_count, 1);
    assert_eq!(fb_b.card_count, 1);
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 14: Header formatting
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_header_formatting() {
    let segments = vec![video("Test", 3600.0, 0.0, None)];
    let report = build_report_data(&segments, None, "/output/Course.mp4", 1_048_576_000, 3600.0, None, None, None, None, None);

    assert_eq!(report.header.total_duration_formatted, "01:00:00",
        "3600s should be 01:00:00");
    assert_eq!(report.header.total_size_formatted, "1000.0 MB",
        "1 GB should be 1000.0 MB");
    assert!(!report.header.generated_at_formatted.is_empty(),
        "Timestamp should be non-empty");
    assert_eq!(report.header.output_path, "/output/Course.mp4");
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 15: No folder header for segments without parent_folder
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_no_folder_header_when_no_parent() {
    let segments = vec![
        video("A", 60.0, 0.0, None),
        video("B", 60.0, 60.0, None),
    ];
    let total_dur: f64 = segments.iter().map(|s| s.duration).sum();
    let report = build_report_data(&segments, None, "/output/video.mp4", 10_000_000, total_dur, None, None, None, None, None);

    // No folder headers when all parent_folder = None
    let folder_count = report.timeline.iter().filter(|e| e.is_folder_header()).count();
    assert_eq!(folder_count, 0, "No folder headers when no parent_folder set");
    assert_eq!(report.timeline.len(), 2, "2 videos, no folder headers");
}



// ═════════════════════════════════════════════════════════════════════════════
// Test 16: Repeat summary — populated with By Count mode
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_repeat_summary_populated() {
    let segments = vec![
        video("A", 60.0, 0.0, None),
        video("B", 60.0, 60.0, None),
        video("C", 60.0, 120.0, None),
    ];
    let original_dur: f64 = segments.iter().map(|s| s.duration).sum();
    let repeat_cfg = RepeatConfig {
        enabled: true,
        by_count: true,
        repeat_count: 5,
        until_duration: false,
        target_duration_seconds: 0.0,
        insert_boundary_cards: false,
        boundary_card_template: "🔁 Repeat {n}".to_string(),
    };
    // 3 videos x 5 repeats = 15 segments total = 900s
    let total_dur = original_dur * 5.0;

    let report = build_report_data(
        &segments,
        None,
        "/output/video.mp4",
        100_000_000,
        total_dur,
        None,
        None,
        None,
        Some(&repeat_cfg),
        Some(original_dur),
    );

    let rs = report.repeat_summary.expect("RepeatSummary should be Some");
    assert!(rs.enabled, "Repeat should be enabled");
    assert_eq!(rs.mode.as_deref(), Some("By Count"), "Mode should be 'By Count'");
    assert_eq!(rs.repeat_count, Some(5), "Repeat count should be 5");
    assert!((rs.original_duration.unwrap() - 180.0).abs() < 0.001,
        "Original duration should be 180s (videos only)");
    assert!((rs.final_duration.unwrap() - total_dur).abs() < 0.001,
        "Final duration should be {}s (5x)", total_dur);
}

// ═════════════════════════════════════════════════════════════════════════════
// Test 17: Repeat summary — Until Duration and Both modes
// ═════════════════════════════════════════════════════════════════════════════

#[test]
fn test_repeat_summary_modes() {
    let segments = vec![video("X", 120.0, 0.0, None)];

    // — Until Duration mode —
    let cfg_until = RepeatConfig {
        enabled: true,
        by_count: false,
        repeat_count: 0,
        until_duration: true,
        target_duration_seconds: 3600.0,
        insert_boundary_cards: false,
        boundary_card_template: "🔁 Repeat {n}".to_string(),
    };
    let report_until = build_report_data(
        &segments, None, "/output/video.mp4", 10_000_000, 3600.0,
        None, None, None, Some(&cfg_until), Some(120.0),
    );
    let rs_until = report_until.repeat_summary.expect("RepeatSummary should be Some");
    assert_eq!(rs_until.mode.as_deref(), Some("Until Duration"),
        "Until Duration mode string");

    // — Both modes active —
    let cfg_both = RepeatConfig {
        enabled: true,
        by_count: true,
        repeat_count: 5,
        until_duration: true,
        target_duration_seconds: 3600.0,
        insert_boundary_cards: false,
        boundary_card_template: "🔁 Repeat {n}".to_string(),
    };
    let report_both = build_report_data(
        &segments, None, "/output/video.mp4", 10_000_000, 600.0,
        None, None, None, Some(&cfg_both), Some(120.0),
    );
    let rs_both = report_both.repeat_summary.expect("RepeatSummary should be Some");
    assert_eq!(rs_both.mode.as_deref(), Some("Both"),
        "Both mode string");
}
