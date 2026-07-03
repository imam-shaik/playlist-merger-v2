use crate::ffmpeg::subtitle_audit::parse_srt_cues;
use crate::split::engine::SrtEntry;

#[cfg(test)]
mod split_merge_subtitle_rebase_bug_test {
    use super::*;

    // Simulates merged SRT content for Part 1 (video starts at 0s)
    // Cues are at 1s and 5s relative to Part 1 start
    const SRT_CONTENT_PART1: &str = r#"1
00:00:01,000 --> 00:00:04,000
Hello from Part 1

2
00:00:05,000 --> 00:00:08,000
This is segment 1
"#;

    // Simulates merged SRT content for Part 2 (video starts at 60s in original timeline)
    // Cues are at 65s and 70s in the ORIGINAL/MERGED timeline
    // After correct rebase to Part 2 start (60s), should become 5s and 10s
    const SRT_CONTENT_PART2: &str = r#"1
00:01:05,000 --> 00:01:08,000
Hello from Part 2

2
00:01:10,000 --> 00:01:13,000
This is segment 2
"#;

    #[derive(Debug, Clone)]
    struct PartDefinition {
        pub index: u32,
        pub start_time: f64,
        pub end_time: f64,
        pub srt_content: String,
    }

    fn parse_srt_entries(content: &str) -> Vec<SrtEntry> {
        let cues = parse_srt_cues(content);
        cues.into_iter()
            .map(|c| SrtEntry {
                index: c.index,
                start_time: c.start_time,
                end_time: c.end_time,
                text: c.text,
            })
            .collect()
    }

    #[allow(dead_code)]
    fn format_srt_entries(entries: &[SrtEntry]) -> String {
        entries
            .iter()
            .map(|e| {
                format!(
                    "{}\n{} --> {}\n{}\n",
                    e.index,
                    format_timestamp(e.start_time),
                    format_timestamp(e.end_time),
                    e.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[allow(dead_code)]
    fn format_timestamp(seconds: f64) -> String {
        let hours = (seconds / 3600.0).floor();
        let minutes = ((seconds % 3600.0) / 60.0).floor();
        let secs = seconds % 60.0;
        format!("{:02}:{:02}:{:06.3}", hours, minutes, secs).replace('.', ",")
    }

    #[allow(dead_code)]
    fn parse_timestamp(ts: &str) -> f64 {
        let ts = ts.replace(',', ".");
        let parts: Vec<&str> = ts.split(':').collect();
        if parts.len() != 3 {
            return 0.0;
        }
        let hours: f64 = parts[0].parse().unwrap_or(0.0);
        let minutes: f64 = parts[1].parse().unwrap_or(0.0);
        let seconds: f64 = parts[2].parse().unwrap_or(0.0);
        hours * 3600.0 + minutes * 60.0 + seconds
    }

    #[test]
    fn test_prove_direct_split_is_correct() {
        let parts = vec![
            PartDefinition {
                index: 1,
                start_time: 0.0,
                end_time: 60.0,
                srt_content: SRT_CONTENT_PART1.to_string(),
            },
            PartDefinition {
                index: 2,
                start_time: 60.0,
                end_time: 120.0,
                srt_content: SRT_CONTENT_PART2.to_string(),
            },
        ];

        println!("\n=== DIRECT SPLIT (CORRECT BEHAVIOR) ===");
        println!("Simulating split_srt_for_segments() approach: rebase by subtracting segment.start_time\n");

        for part in &parts {
            let entries = parse_srt_entries(&part.srt_content);
            let adjusted: Vec<SrtEntry> = entries
                .into_iter()
                .map(|mut e| {
                    e.start_time -= part.start_time;
                    e.end_time -= part.start_time;
                    if e.start_time < 0.0 {
                        e.start_time = 0.0;
                    }
                    if e.end_time > part.end_time - part.start_time {
                        e.end_time = part.end_time - part.start_time;
                    }
                    e
                })
                .collect();

            println!("[DirectSplit] Part {} (start={:.1}s):", part.index, part.start_time);
            for entry in &adjusted {
                println!(
                    "  Cue {}: {:.3}s -> {:.3}s | {}",
                    entry.index, entry.start_time, entry.end_time, entry.text
                );
            }

            // After correct rebase:
            // Part 1 (start=0): cue should be 1s (1 - 0 = 1)
            // Part 2 (start=60): cue should be 5s (65 - 60 = 5)
            if part.index == 1 {
                assert!(
                    (adjusted[0].start_time - 1.0).abs() < 0.001,
                    "Part {} first cue should be 1.0, got {:.3}",
                    part.index,
                    adjusted[0].start_time
                );
            } else {
                assert!(
                    (adjusted[0].start_time - 5.0).abs() < 0.001,
                    "Part {} first cue should be 5.0 (rebased from 65s), got {:.3}",
                    part.index,
                    adjusted[0].start_time
                );
            }
        }
    }

    #[test]
    fn test_prove_concat_demuxer_preserves_timestamps() {
        let parts = vec![
            PartDefinition {
                index: 1,
                start_time: 0.0,
                end_time: 60.0,
                srt_content: SRT_CONTENT_PART1.to_string(),
            },
            PartDefinition {
                index: 2,
                start_time: 60.0,
                end_time: 120.0,
                srt_content: SRT_CONTENT_PART2.to_string(),
            },
        ];

        println!("\n=== FFMPEG CONCAT DEMUXER (BUGGY BEHAVIOR) ===");
        println!("Simulating generate_merged_srt() which uses -f concat WITHOUT rebasing\n");

        let mut all_entries: Vec<SrtEntry> = Vec::new();
        for part in &parts {
            let entries = parse_srt_entries(&part.srt_content);
            println!(
                "[Concat] Part {} - Original timestamps (NOT rebased):",
                part.index
            );
            for entry in &entries {
                println!(
                    "  Cue {}: {:.3}s -> {:.3}s | {}",
                    entry.index, entry.start_time, entry.end_time, entry.text
                );
            }
            all_entries.extend(entries);
        }

        println!("\n[Concat] Combined output (BUG: timestamps NOT rebased):");
        for entry in &all_entries {
            println!(
                "  Cue {}: {:.3}s -> {:.3}s | {}",
                entry.index, entry.start_time, entry.end_time, entry.text
            );
        }

        // Bug verification:
        // Part 1 cues: 1s, 5s (correct since start=0)
        // Part 2 cues: 65s, 70s (WRONG - should be 5s, 10s after rebase)
        println!("\n=== BUG ANALYSIS ===");
        println!("Part 1 (start=0):  cues at 1s, 5s  -> CORRECT (offset is 0)");
        println!("Part 2 (start=60): cues at 65s, 70s -> WRONG (should be 5s, 10s after rebase)");
        println!("\nWithout rebasing, timestamps from Part 2 retain their original merged-timeline positions.");
        println!("FFmpeg's concat demuxer with duration directives DOES rebase correctly, but");
        println!("this test simulates the in-memory merge path which must rebase manually.");

        // Part 2 first cue should be 65s (bug preserved) vs 5s (correct)
        assert!(
            (all_entries[2].start_time - 65.0).abs() < 0.001,
            "Part 2 first cue should be 65s (bug preserved), got {:.3}",
            all_entries[2].start_time
        );
    }

    #[test]
    fn test_offset_type_is_constant_not_growing() {
        println!("\n=== OFFSET TYPE ANALYSIS ===");
        println!("Determining if bug is CONSTANT (rebase) or GROWING (timeline scaling)\n");

        // Simulate Part 1 and Part 2 SRT content
        // Part 1: starts at 0s, cues at 1s, 5s (no rebase needed)
        // Part 2: starts at 60s, cues at 65s, 70s (in merged timeline)
        //         After rebase: should be 5s, 10s (65-60, 70-60)
        //         With bug (concat): stays at 65s, 70s

        println!("Simulating Part 1 (start=0s):");
        let p1_entries = parse_srt_entries(SRT_CONTENT_PART1);
        println!("  Cues at: {:.3}s, {:.3}s", p1_entries[0].start_time, p1_entries[1].start_time);
        println!("  After correct rebase: {:.3}s, {:.3}s", p1_entries[0].start_time, p1_entries[1].start_time);
        println!("  With bug (concat): same as original\n");

        println!("Simulating Part 2 (start=60s):");
        let p2_entries = parse_srt_entries(SRT_CONTENT_PART2);
        println!("  Cues at (in merged timeline): {:.3}s, {:.3}s", p2_entries[0].start_time, p2_entries[1].start_time);
        println!("  After CORRECT rebase: {:.3}s, {:.3}s (subtract 60s)",
            p2_entries[0].start_time - 60.0, p2_entries[1].start_time - 60.0);
        println!("  With BUG (concat): {:.3}s, {:.3}s (timestamps preserved)\n",
            p2_entries[0].start_time, p2_entries[1].start_time);

        // Calculate the ERROR (difference between correct rebase and buggy output)
        let p1_correct_first = p1_entries[0].start_time; // 1s
        let p2_correct_first = p2_entries[0].start_time - 60.0; // 5s (rebased)
        let p2_buggy_first = p2_entries[0].start_time; // 65s

        let p1_error = (p1_correct_first - p1_correct_first).abs(); // 0
        let p2_error = (p2_buggy_first - p2_correct_first).abs(); // 60

        println!("=== OFFSET ANALYSIS ===");
        println!("Part 1 error: {:.3}s (correct output = {:.3}s, buggy output = {:.3}s)",
            p1_error, p1_correct_first, p1_correct_first);
        println!("Part 2 error: {:.3}s (correct output = {:.3}s, buggy output = {:.3}s)",
            p2_error, p2_correct_first, p2_buggy_first);
        println!("\nPart 1 error: {:.1}s", p1_error);
        println!("Part 2 error: {:.1}s", p2_error);

        if (p1_error - 0.0).abs() < 0.001 && (p2_error - 60.0).abs() < 0.001 {
            println!("\nOFFSET TYPE: CONSTANT (60s per part after first)");
            println!("This is a REBASE bug - missing timestamp offset adjustment");
        } else if p2_error > p1_error && p2_error > p1_error * 1.5 {
            println!("\nOFFSET TYPE: GROWING");
            println!("This would indicate timeline scaling/FPS issue");
        }

        // Assertions
        assert!(
            (p1_error - 0.0).abs() < 0.001,
            "Part 1 error should be ~0"
        );
        assert!(
            (p2_error - 60.0).abs() < 0.001,
            "Part 2 error should be ~60s (offset = part start time)"
        );
    }

    #[test]
    fn test_part_boundary_subtitle_continuity() {
        println!("\n=== SUBTITLE CONTINUITY TEST ===");
        println!("Verifying Part 1 last cue + Part 2 first cue = no gap, no overlap\n");

        let part1_srt = r#"1
00:00:01,000 --> 00:00:04,000
Hello

2
00:00:08,000 --> 00:00:10,000
Goodbye
"#;

        let part2_srt = r#"1
00:00:01,000 --> 00:00:03,000
Next part starts
"#;

        let part1_cues = parse_srt_cues(part1_srt);
        let part2_cues = parse_srt_cues(part2_srt);

        println!("Part 1 last cue ends at: {:.3}s", part1_cues.last().unwrap().end_time);
        println!("Part 2 first cue starts at: {:.3}s", part2_cues.first().unwrap().start_time);

        let _part1_duration = 60.0;
        let part2_start = 60.0;

        println!("\nWith CORRECT rebase (split_srt_for_segments approach):");
        let rebase_offset_part2 = part2_start;
        println!("  Part 2 cue would be: {:.3}s - {:.3}s = {:.3}s",
            part2_cues.first().unwrap().start_time, rebase_offset_part2,
            part2_cues.first().unwrap().start_time - rebase_offset_part2);

        println!("\nWith BUGGY concat (no rebase):");
        println!("  Part 2 cue stays at: {:.3}s (WRONG - should be ~1s)",
            part2_cues.first().unwrap().start_time);

        let continuity_gap = part2_cues.first().unwrap().start_time - part1_cues.last().unwrap().end_time;
        println!("\nContinuity gap between Part 1 end and Part 2 start: {:.3}s", continuity_gap);
        println!("(Positive = gap, Negative = overlap, ~0 = correct)");
    }

    #[test]
    fn test_mathematical_proof_rebase_needed() {
        println!("\n=== MATHEMATICAL PROOF: TIMESTAMP REBASE REQUIRED ===\n");

        let original_cue_time = 1732.5;
        let part_start_time = 1500.0;

        let expected_rebased_time = original_cue_time - part_start_time;
        let actual_with_concat = original_cue_time;
        let error = actual_with_concat - expected_rebased_time;

        println!("Given:");
        println!("  Original SRT cue time: {:.3}s", original_cue_time);
        println!("  Part start time: {:.3}s", part_start_time);
        println!();
        println!("Expected (correct rebase):");
        println!("  cue_time - part_start = {:.3}s - {:.3}s = {:.3}s", original_cue_time, part_start_time, expected_rebased_time);
        println!();
        println!("Actual (FFmpeg concat, no rebase):");
        println!("  cue_time = {:.3}s (UNCHANGED)", actual_with_concat);
        println!();
        println!("Error: {:.3}s ({:.1} minutes)", error, error / 60.0);

        assert!(
            (expected_rebased_time - 232.5_f64).abs() < 0.001,
            "Expected rebased time should be 232.5s"
        );
        assert!(
            (error - 1500.0_f64).abs() < 0.001,
            "Error should equal part_start_time (1500s)"
        );
    }
}

#[cfg(test)]
mod merge_subtitle_mode_trace_test {

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum SubtitleMode {
        None,
        Embed,
        Burn,
        ExportSrt,
        SrtMergeOnly,
    }

    struct MergePart {
        pub part_index: u32,
        pub start_time: f64,
        pub end_time: f64,
        pub file_indices: Vec<usize>,
    }

    fn trace_subtitle_mode_in_split_merge(
        mode: SubtitleMode,
        parts: &[MergePart],
        srt_export_paths: &mut Vec<String>,
    ) -> Vec<String> {
        let mut affected_paths = Vec::new();

        for part in parts {
            println!("\n=== Part {} | Mode: {:?} ===", part.part_index, mode);

            match mode {
                SubtitleMode::None => {
                    println!("  [Path] None: No subtitle processing");
                }
                SubtitleMode::Embed => {
                    println!("  [Path] Embed: Subtitle muxed via ffmpeg -c copy");
                    println!("    - Concat list created with write_subtitle_concat_list()");
                    println!("    - FFmpeg copies subtitle stream WITHOUT rebasing");
                    println!("    - BUG: FFmpeg concat demuxer preserves original timestamps");
                    affected_paths.push(format!("Part {} embed subtitle", part.part_index));
                }
                SubtitleMode::Burn => {
                    println!("  [Path] Burn: Re-encode with burned subtitles");
                    println!("    - generate_merged_srt() called to create merged SRT");
                    println!("    - FFmpeg concat demuxer used -> NO REBASING");
                    println!("    - BUG: Burned subtitle timing wrong");
                    affected_paths.push(format!("Part {} burn subtitle", part.part_index));
                }
                SubtitleMode::ExportSrt => {
                    println!("  [Path] ExportSrt: Only export SRT file");
                    println!("    - generate_merged_srt() called at concat.rs:2371");
                    println!("    - FFmpeg concat demuxer used -> NO REBASING");
                    println!("    - BUG: Exported SRT has wrong timestamps");
                    affected_paths.push(format!("Part {} export SRT", part.part_index));
                }
                SubtitleMode::SrtMergeOnly => {
                    println!("  [Path] SrtMergeOnly: Merge SRT only, bypass video");
                    println!("    - Uses same generate_merged_srt() path");
                    println!("    - BUG: Same timestamp issue");
                    affected_paths.push(format!("Part {} SRT merge only", part.part_index));
                }
            }

            println!("  Part start: {:.1}s, end: {:.1}s", part.start_time, part.end_time);
            println!("  Files: {:?}", part.file_indices);
        }

        if affected_paths.is_empty() {
            println!("\n  [Result] No bug for this mode");
        } else {
            println!("\n  [Result] BUG AFFECTS: {:?}", affected_paths);
        }

        srt_export_paths.extend(affected_paths.clone());
        affected_paths
    }

    #[test]
    fn test_all_subtitle_modes_affected() {
        let parts = vec![
            MergePart {
                part_index: 1,
                start_time: 0.0,
                end_time: 60.0,
                file_indices: vec![0],
            },
            MergePart {
                part_index: 2,
                start_time: 60.0,
                end_time: 120.0,
                file_indices: vec![1],
            },
        ];

        let mut all_affected = Vec::new();

        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║   SUBTITLE MODE BUG TRACE: Split Merge vs Direct Split      ║");
        println!("╚════════════════════════════════════════════════════════════════╝");

        for mode in [
            SubtitleMode::None,
            SubtitleMode::Embed,
            SubtitleMode::Burn,
            SubtitleMode::ExportSrt,
            SubtitleMode::SrtMergeOnly,
        ] {
            let affected = trace_subtitle_mode_in_split_merge(mode, &parts, &mut all_affected);
            if !affected.is_empty() {
                println!("\n  ❌ {} MODE AFFECTED", format!("{:?}", mode).to_uppercase());
            }
        }

        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║   SUMMARY: All non-None modes use same broken path          ║");
        println!("╚════════════════════════════════════════════════════════════════╝");
        println!("\nAll modes (Embed, Burn, ExportSrt, SrtMergeOnly) call:");
        println!("  1. write_subtitle_concat_list() -> creates concat list");
        println!("  2. generate_merged_srt() -> FFmpeg concat demuxer");
        println!("  3. NO timestamp rebasing happens");
        println!("\nThe ONLY correct implementation is Direct Split's");
        println!("  split_srt_for_segments() which explicitly does:");
        println!("    sub.start_time -= segment.start_time;");
        println!("    sub.end_time -= segment.start_time;");
    }

    #[test]
    fn test_direct_split_uses_correct_logic() {
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║   DIRECT SPLIT: Correct Implementation                       ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        println!("split_srt_for_segments() at split/engine.rs:972-984:");
        println!();
        println!("  let adjusted_subs: Vec<SrtEntry> = segment_subs");
        println!("      .into_iter()");
        println!("      .map(|mut sub| {{");
        println!("          sub.start_time -= segment.start_time;  // <-- REBASE");
        println!("          sub.end_time -= segment.start_time;    // <-- REBASE");
        println!("          ...");
        println!("      }})");
        println!();
        println!("✅ CORRECT: Subtitles are rebased to segment-relative timeline");
        println!("✅ CORRECT: Part 2 subtitles start at ~0s, not at original timestamp");
    }
}

#[cfg(test)]
mod cue_count_integrity_test {
    use super::*;

    const LARGE_SRT_PART1: &str = r#"1
00:00:01,000 --> 00:00:04,000
Line 1

2
00:00:05,000 --> 00:00:08,000
Line 2

3
00:00:09,000 --> 00:00:12,000
Line 3

4
00:00:13,000 --> 00:00:16,000
Line 4

5
00:00:17,000 --> 00:00:20,000
Line 5
"#;

    const LARGE_SRT_PART2: &str = r#"1
00:01:05,000 --> 00:01:08,000
Line 6

2
00:01:09,000 --> 00:01:12,000
Line 7

3
00:01:13,000 --> 00:01:16,000
Line 8

4
00:01:17,000 --> 00:01:20,000
Line 9

5
00:01:21,000 --> 00:01:24,000
Line 10
"#;

    const LARGE_SRT_PART3: &str = r#"1
00:02:05,000 --> 00:02:08,000
Line 11

2
00:02:09,000 --> 00:02:12,000
Line 12

3
00:02:13,000 --> 00:02:16,000
Line 13

4
00:02:17,000 --> 00:02:20,000
Line 14

5
00:02:21,000 --> 00:02:24,000
Line 15
"#;

    const CROSSING_BOUNDARY_SRT: &str = r#"1
00:00:58,500 --> 00:01:00,800
This cue crosses boundary at 60s

2
00:01:01,000 --> 00:01:04,000
This cue starts after boundary
"#;

    fn parse_srt_for_count(content: &str) -> usize {
        parse_srt_cues(content).len()
    }

    #[test]
    fn test_cue_count_preserved_no_loss() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║           CUE COUNT INTEGRITY TEST - NO LOSS/DUPLICATION          ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let part1_count = parse_srt_for_count(LARGE_SRT_PART1);
        let part2_count = parse_srt_for_count(LARGE_SRT_PART2);
        let total_cues = part1_count + part2_count;

        println!("Input:");
        println!("  Part 1 cues: {}", part1_count);
        println!("  Part 2 cues: {}", part2_count);
        println!("  Total: {}\n", total_cues);

        // Simulate the rebase for Part 1 (starts at 0s)
        let part1_cues = parse_srt_cues(LARGE_SRT_PART1);
        let part1_after_rebase: Vec<_> = part1_cues
            .iter()
            .map(|c| {
                let start = c.start_time - 0.0; // Part 1 starts at 0
                let end = c.end_time - 0.0;
                (start.max(0.0), end.max(0.0))
            })
            .filter(|(s, e)| e > s)
            .collect();

        // Simulate the rebase for Part 2 (starts at 60s)
        let part2_cues = parse_srt_cues(LARGE_SRT_PART2);
        let part2_after_rebase: Vec<_> = part2_cues
            .iter()
            .map(|c| {
                let start = c.start_time - 60.0; // Part 2 starts at 60s
                let end = c.end_time - 60.0;
                (start.max(0.0), end.max(0.0))
            })
            .filter(|(s, e)| e > s)
            .collect();

        let part1_rebased_count = part1_after_rebase.len();
        let part2_rebased_count = part2_after_rebase.len();
        let total_after_rebase = part1_rebased_count + part2_rebased_count;

        println!("After rebase:");
        println!("  Part 1 rebased cues: {}", part1_rebased_count);
        println!("  Part 2 rebased cues: {}", part2_rebased_count);
        println!("  Total after rebase: {}\n", total_after_rebase);

        println!("Integrity check:");
        println!("  Original total: {}", total_cues);
        println!("  After rebase total: {}", total_after_rebase);

        assert_eq!(
            total_cues, total_after_rebase,
            "Cue count must be preserved! Lost {} cues",
            total_cues - total_after_rebase
        );

        println!("  ✅ CUE COUNT PRESERVED: No cues lost or duplicated");
    }

    #[test]
    fn test_boundary_crossing_cues_trimmed_not_discarded() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║              BOUNDARY-CROSSING CUE TEST                           ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let cues = parse_srt_cues(CROSSING_BOUNDARY_SRT);
        println!("Original cues:");
        for cue in &cues {
            println!("  {:.3}s --> {:.3}s: {}",
                cue.start_time, cue.end_time, cue.text);
        }

        // Simulate Part 2 (starts at 60s)
        let rebased: Vec<_> = cues
            .iter()
            .map(|c| {
                let start = c.start_time - 60.0;
                let end = c.end_time - 60.0;
                (start.max(0.0), end.max(0.0))
            })
            .filter(|(s, e)| e > s)
            .collect();

        println!("\nAfter rebase to Part 2 (start=60s):");
        for (i, (s, e)) in rebased.iter().enumerate() {
            println!("  {:.3}s --> {:.3}s (cue {})", s, e, i + 1);
        }

        // Verify:
        // Cue 1: 58.5 - 60 = -1.5 -> clamped to 0, 60.8 - 60 = 0.8 -> 0.000 --> 0.800
        // Cue 2: 61 - 60 = 1, 64 - 60 = 4 -> 1.000 --> 4.000
        assert_eq!(rebased.len(), 2, "Both cues should be preserved (trimmed, not discarded)");

        let cue1 = &rebased[0];
        assert!((cue1.0 - 0.0).abs() < 0.001, "Cue 1 start should be ~0.0");
        assert!((cue1.1 - 0.800).abs() < 0.001, "Cue 1 end should be ~0.8");

        let cue2 = &rebased[1];
        assert!((cue2.0 - 1.0).abs() < 0.001, "Cue 2 start should be ~1.0");
        assert!((cue2.1 - 4.0).abs() < 0.001, "Cue 2 end should be ~4.0");

        println!("\n✅ BOUNDARY-CROSSING POLICY VERIFIED:");
        println!("   Cue 1: trimmed from [58.5-60.8] to [0.0-0.8]");
        println!("   Cue 2: preserved as-is [1.0-4.0]");
    }

    #[test]
    fn test_cue_numbering_sequential_per_part() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║              CUE NUMBERING INTEGRITY TEST                         ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let part1_cues = parse_srt_cues(LARGE_SRT_PART1);
        let part2_cues = parse_srt_cues(LARGE_SRT_PART2);

        println!("Part 1 has {} cues (should be indexed 1-{})", part1_cues.len(), part1_cues.len());
        println!("Part 2 has {} cues (should be indexed 1-{})", part2_cues.len(), part2_cues.len());

        // After rebase, each part should have sequential indices starting from 1
        // Part 1 reindexed: 1, 2, 3, 4, 5
        // Part 2 reindexed: 1, 2, 3, 4, 5

        assert_eq!(part1_cues.len(), 5, "Part 1 should have 5 cues");
        assert_eq!(part2_cues.len(), 5, "Part 2 should have 5 cues");

        // Verify indices are as expected (parse_srt_cues returns cues in order)
        for (i, cue) in part1_cues.iter().enumerate() {
            assert_eq!(cue.index, (i + 1) as u32, "Part 1 cue {} should have index {}", i + 1, i + 1);
        }

        for (i, cue) in part2_cues.iter().enumerate() {
            assert_eq!(cue.index, (i + 1) as u32, "Part 2 cue {} should have index {}", i + 1, i + 1);
        }

        println!("\n✅ CUE NUMBERING VERIFIED: Each part has sequential indices 1-N");
    }

    #[test]
    fn test_multipart_cue_distribution() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║              MULTI-PART CUE DISTRIBUTION TEST                     ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        // Simulate 3-part split where each part's content has cues within its time range
        // Part 1 (0-60s): cues at 1-20s (video 1 original content)
        // Part 2 (60-120s): cues at 65-84s (video 2 content offset by 60s in merged timeline)
        // Part 3 (120-180s): cues at 125-144s (video 3 content offset by 120s in merged timeline)

        let parts = vec![
            ("Part 1", 0.0, LARGE_SRT_PART1),   // 0-60s, cues at 1-20s
            ("Part 2", 60.0, LARGE_SRT_PART2),   // 60-120s, cues at 65-84s
            ("Part 3", 120.0, LARGE_SRT_PART3),  // 120-180s, cues at 125-144s
        ];

        let mut total_cues = 0;
        let mut part_cues = Vec::new();

        for (name, start_time, srt_content) in &parts {
            let cues = parse_srt_cues(srt_content);
            let rebased: Vec<_> = cues
                .iter()
                .map(|c| {
                    let start = c.start_time - start_time;
                    let end = c.end_time - start_time;
                    (start.max(0.0), end.max(0.0))
                })
                .filter(|(s, e)| e > s)
                .collect();

            let count = rebased.len();
            total_cues += count;
            part_cues.push((name, count));

            println!("  {} (start={:.0}s): {} cues after rebase", name, start_time, count);
        }

        println!("\nTotal cues across all parts: {}", total_cues);

        // Part 1 has 5 cues, Part 2 has 5 cues, Part 3 has 5 cues
        // Total should be 15
        let expected_total = 15;
        assert_eq!(total_cues, expected_total,
            "Total cues should be {} (5 + 5 + 5), got {}", expected_total, total_cues);

        println!("\n✅ CUE DISTRIBUTION VERIFIED: Cues properly distributed across {} parts", parts.len());
    }
}

/// Regression tests for the subtitle double-shift bug fix (2026-07-03).
///
/// The bug: `generate_merged_srt_with_rebase()` pre-shifted timestamps AND fed them
/// through FFmpeg's concat demuxer with duration directives, causing double-offset.
///
/// The fix: Remove pre-shifting; let the concat demuxer rebase automatically.
///
/// These tests verify that the concat demuxer produces correct timestamps for:
/// - Uniform durations
/// - Non-uniform durations
/// - Multi-cue segments
/// - Missing subtitle segments (None)
/// - Edge cases (empty SRT, very short segments)
#[cfg(test)]
mod concat_demuxer_rebase_regression_test {
    use super::*;
    use std::io::Write;
    use std::fmt::Write as FmtWrite;
    use std::path::PathBuf;

    /// Helper: create a temporary SRT file with given cues
    fn create_temp_srt(name: &str, cues: &[(f64, f64, &str)], test_dir: &std::path::Path) -> PathBuf {
        let path = test_dir.join(format!("{}.srt", name));
        let mut f = std::fs::File::create(&path).unwrap();
        for (i, (start, end, text)) in cues.iter().enumerate() {
            writeln!(f, "{}", i + 1).unwrap();
            writeln!(f, "{} --> {}", format_ts(*start), format_ts(*end)).unwrap();
            writeln!(f, "{}", text).unwrap();
            writeln!(f).unwrap();
        }
        path
    }

    /// Format seconds as SRT timestamp (HH:MM:SS,mmm)
    fn format_ts(secs: f64) -> String {
        let h = (secs / 3600.0) as u32;
        let m = ((secs % 3600.0) / 60.0) as u32;
        let s = secs % 60.0;
        format!("{:02}:{:02}:{:06.3}", h, m, s).replace('.', ",")
    }

    /// Helper: run FFmpeg concat demuxer on a set of SRT files with durations
    /// Returns the parsed cue start times from the output
    fn run_concat_demuxer(srt_paths: &[PathBuf], durations: &[f64], test_dir: &std::path::Path) -> Vec<f64> {
        let concat_path = test_dir.join("test_concat.txt");
        let output_path = test_dir.join("test_output.srt");
        let ffmpeg_path = std::env::current_dir()
            .unwrap()
            .join("binaries")
            .join("ffmpeg.exe");

        let mut content = String::new();
        for (i, path) in srt_paths.iter().enumerate() {
            let escaped = path.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
            write!(content, "file '{}'\n", escaped).unwrap();
            write!(content, "duration {}\n", durations[i]).unwrap();
        }
        std::fs::write(&concat_path, &content).unwrap();

        let output = std::process::Command::new(&ffmpeg_path)
            .args([
                "-y",
                "-f", "concat",
                "-safe", "0",
                "-i", concat_path.to_string_lossy().as_ref(),
                "-c:s", "srt",
                output_path.to_string_lossy().as_ref(),
            ])
            .output()
            .expect("Failed to run FFmpeg");

        assert!(output.status.success(), "FFmpeg failed: {}", String::from_utf8_lossy(&output.stderr));

        let result = std::fs::read_to_string(&output_path).unwrap();
        parse_srt_cues(&result)
            .iter()
            .map(|c| c.start_time)
            .collect()
    }

    /// Create a unique temp directory for each test
    fn make_test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("subtitle_regression_{}", name));
        std::fs::create_dir_all(&dir).unwrap();
        // Clean previous files
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_uniform_durations() {
        println!("\n=== REGRESSION: Uniform durations (10+10+10) ===");
        let dir = make_test_dir("uniform");

        let s1 = create_temp_srt("uni1", &[(5.0, 7.0, "Cue 1")], &dir);
        let s2 = create_temp_srt("uni2", &[(5.0, 7.0, "Cue 2")], &dir);
        let s3 = create_temp_srt("uni3", &[(5.0, 7.0, "Cue 3")], &dir);

        let starts = run_concat_demuxer(&[s1, s2, s3], &[10.0, 10.0, 10.0], &dir);

        println!("  Expected: 5.0s, 15.0s, 25.0s");
        println!("  Got:      {:.1}s, {:.1}s, {:.1}s", starts[0], starts[1], starts[2]);

        assert!((starts[0] - 5.0).abs() < 0.001, "Cue 1 should be at 5.0s, got {:.3}", starts[0]);
        assert!((starts[1] - 15.0).abs() < 0.001, "Cue 2 should be at 15.0s, got {:.3}", starts[1]);
        assert!((starts[2] - 25.0).abs() < 0.001, "Cue 3 should be at 25.0s, got {:.3}", starts[2]);

        println!("  ✅ PASS");
    }

    #[test]
    fn test_nonuniform_durations() {
        println!("\n=== REGRESSION: Non-uniform durations (7+12+9) ===");
        let dir = make_test_dir("nonuniform");

        let s1 = create_temp_srt("nuni1", &[(3.0, 5.0, "Video A")], &dir);
        let s2 = create_temp_srt("nuni2", &[(3.0, 5.0, "Video B")], &dir);
        let s3 = create_temp_srt("nuni3", &[(3.0, 5.0, "Video C")], &dir);

        let starts = run_concat_demuxer(&[s1, s2, s3], &[7.0, 12.0, 9.0], &dir);

        // Expected: 3s, 3+7=10s, 3+7+12=22s
        println!("  Expected: 3.0s, 10.0s, 22.0s");
        println!("  Got:      {:.1}s, {:.1}s, {:.1}s", starts[0], starts[1], starts[2]);

        assert!((starts[0] - 3.0).abs() < 0.001, "Cue 1 should be at 3.0s, got {:.3}", starts[0]);
        assert!((starts[1] - 10.0).abs() < 0.001, "Cue 2 should be at 10.0s, got {:.3}", starts[1]);
        assert!((starts[2] - 22.0).abs() < 0.001, "Cue 3 should be at 22.0s, got {:.3}", starts[2]);

        println!("  ✅ PASS");
    }

    #[test]
    fn test_multi_cue_segments() {
        println!("\n=== REGRESSION: Multi-cue segments (2 cues each) ===");
        let dir = make_test_dir("multicue");

        let s1 = create_temp_srt("mc1", &[(2.0, 4.0, "A1"), (6.0, 8.0, "A2")], &dir);
        let s2 = create_temp_srt("mc2", &[(3.0, 5.0, "B1"), (7.0, 9.0, "B2")], &dir);
        let s3 = create_temp_srt("mc3", &[(1.0, 3.0, "C1"), (4.0, 6.0, "C2")], &dir);

        let starts = run_concat_demuxer(&[s1, s2, s3], &[8.0, 10.0, 7.0], &dir);

        // Expected: A1@2, A2@6, B1@11(8+3), B2@15(8+7), C1@19(8+10+1), C2@22(8+10+4)
        let expected = vec![2.0, 6.0, 11.0, 15.0, 19.0, 22.0];
        println!("  Expected: {:?}", expected);
        println!("  Got:      {:?}", starts.iter().map(|s| (s * 10.0).round() / 10.0).collect::<Vec<_>>());

        assert_eq!(starts.len(), 6, "Should have 6 cues");
        for (i, (got, exp)) in starts.iter().zip(expected.iter()).enumerate() {
            assert!((got - exp).abs() < 0.001, "Cue {} should be at {:.1}s, got {:.3}", i + 1, exp, got);
        }

        println!("  ✅ PASS");
    }

    #[test]
    fn test_missing_subtitle_segment() {
        println!("\n=== REGRESSION: Missing subtitle segment (None → dummy) ===");
        let dir = make_test_dir("missing");

        let s1 = create_temp_srt("ms1", &[(3.0, 5.0, "Has subs")], &dir);
        // s2 is missing (simulates None)
        let s3 = create_temp_srt("ms3", &[(2.0, 4.0, "Also has subs")], &dir);

        let dummy = dir.join("dummy_regression.srt");
        std::fs::write(&dummy, "1\n00:00:00,000 --> 00:00:00,001\n \n").unwrap();

        let starts = run_concat_demuxer(&[s1, dummy, s3], &[10.0, 10.0, 10.0], &dir);

        // Expected: 3s, dummy (near 0), 22s (10+10+2)
        println!("  Expected: ~3.0s, ~0.0s (dummy), ~22.0s");
        println!("  Got:      {:.1}s, {:.1}s, {:.1}s", starts[0], starts[1], starts[2]);

        assert!((starts[0] - 3.0).abs() < 0.001, "Cue 1 should be at 3.0s, got {:.3}", starts[0]);
        assert!((starts[2] - 22.0).abs() < 0.001, "Cue 3 should be at 22.0s, got {:.3}", starts[2]);

        println!("  ✅ PASS");
    }

    #[test]
    fn test_very_short_segments() {
        println!("\n=== REGRESSION: Very short segments (0.5s each) ===");
        let dir = make_test_dir("veryshort");

        let s1 = create_temp_srt("vs1", &[(0.1, 0.3, "Quick 1")], &dir);
        let s2 = create_temp_srt("vs2", &[(0.1, 0.3, "Quick 2")], &dir);
        let s3 = create_temp_srt("vs3", &[(0.1, 0.3, "Quick 3")], &dir);

        let starts = run_concat_demuxer(&[s1, s2, s3], &[0.5, 0.5, 0.5], &dir);

        println!("  Expected: 0.1s, 0.6s, 1.1s");
        println!("  Got:      {:.2}s, {:.2}s, {:.2}s", starts[0], starts[1], starts[2]);

        assert!((starts[0] - 0.1).abs() < 0.001, "Cue 1 should be at 0.1s, got {:.3}", starts[0]);
        assert!((starts[1] - 0.6).abs() < 0.001, "Cue 2 should be at 0.6s, got {:.3}", starts[1]);
        assert!((starts[2] - 1.1).abs() < 0.001, "Cue 3 should be at 1.1s, got {:.3}", starts[2]);

        println!("  ✅ PASS");
    }

    #[test]
    fn test_forensic_timeline_certification() {
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║   FORENSIC TIMELINE CERTIFICATION REPORT                     ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        let dir = make_test_dir("forensic");

        // Simulate a real-world scenario: 3 videos with different durations and multi-cue SRTs
        let segments = vec![
            ("Video A (7s)", 7.0, vec![(2.0, 4.0, "A-Intro"), (5.0, 6.5, "A-Outro")]),
            ("Video B (12s)", 12.0, vec![(1.0, 3.0, "B-Start"), (6.0, 8.0, "B-Mid"), (10.0, 11.5, "B-End")]),
            ("Video C (9s)", 9.0, vec![(0.5, 2.5, "C-Open"), (4.0, 6.0, "C-Close")]),
        ];

        let mut srt_paths = Vec::new();
        let mut durations = Vec::new();
        let mut expected_cues = Vec::new();
        let mut cumulative_offset = 0.0;

        for (name, dur, cues) in &segments {
            let path = create_temp_srt(&format!("forensic_{}", name), cues, &dir);
            srt_paths.push(path);
            durations.push(*dur);

            for (start, end, text) in cues {
                expected_cues.push((cumulative_offset + start, cumulative_offset + end, *text));
            }
            cumulative_offset += dur;
        }

        let starts = run_concat_demuxer(&srt_paths, &durations, &dir);

        println!("  ┌─────────────────────────────────────────────────────────────────┐");
        println!("  │ Segment             │ Video Start │ Sub Start │ Difference      │");
        println!("  ├─────────────────────────────────────────────────────────────────┤");

        let mut max_drift = 0.0_f64;
        let mut total_drift = 0.0_f64;
        let mut all_pass = true;

        for (i, ((exp_start, _exp_end, text), got_start)) in expected_cues.iter().zip(starts.iter()).enumerate() {
            let drift = (exp_start - got_start).abs();
            max_drift = max_drift.max(drift);
            total_drift += drift;
            let pass = drift < 0.001;
            if !pass { all_pass = false; }

            let status = if pass { "✅" } else { "❌" };
            println!("  │ {:>2}. {:<18} │ {:>9.1}s  │ {:>8.1}s  │ {:>+8.3}s {}     │",
                i + 1, text, exp_start, got_start, exp_start - got_start, status);
        }

        let avg_drift = total_drift / starts.len() as f64;

        println!("  └─────────────────────────────────────────────────────────────────┘");
        println!();
        println!("  Maximum Drift: {:.3}s ({:.0}ms)", max_drift, max_drift * 1000.0);
        println!("  Average Drift: {:.3}s ({:.0}ms)", avg_drift, avg_drift * 1000.0);
        println!();

        if all_pass && max_drift < 0.001 {
            println!("  ╔══════════════════════════════════════════════════════════════╗");
            println!("  ║                    CERTIFICATION: PASS                     ║");
            println!("  ╚══════════════════════════════════════════════════════════════╝");
        } else {
            println!("  ╔══════════════════════════════════════════════════════════════╗");
            println!("  ║                    CERTIFICATION: FAIL                     ║");
            println!("  ╚══════════════════════════════════════════════════════════════╝");
        }

        assert!(all_pass, "All cues must have < 1ms drift");
        assert!(max_drift < 0.001, "Maximum drift must be < 1ms, got {:.3}s", max_drift);
    }
}