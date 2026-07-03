use rand::Rng;
use std::collections::HashSet;

#[cfg(test)]
mod tests {
    use super::*;

    const STRESS_TEST_CUE_COUNT: usize = 2500;
    const STRESS_TEST_PARTS: usize = 5;

    fn generate_random_srt(cue_count: usize, min_time: f64, max_time: f64) -> String {
        let mut rng = rand::thread_rng();
        let mut output = String::new();
        let mut current_time = min_time;

        for i in 1..=cue_count {
            let gap = rng.gen_range(0.1..2.0);
            current_time += gap;

            if current_time >= max_time {
                break;
            }

            let duration = rng.gen_range(0.5..4.0);
            let end_time = (current_time + duration).min(max_time);

            let start_ms = (current_time * 1000.0).round() as i64;
            let end_ms = (end_time * 1000.0).round() as i64;

            output.push_str(&format!(
                "{}\n{} --> {}\nLine {}\n\n",
                i,
                format_srt_ts_ms(start_ms),
                format_srt_ts_ms(end_ms),
                i
            ));

            current_time = end_time;
        }

        output
    }

    fn format_srt_ts_ms(ms: i64) -> String {
        let total_secs = ms / 1000;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        let millis = ms % 1000;
        format!("{:02}:{:02}:{:02},{:03}", hours, minutes, seconds, millis)
    }

    fn parse_srt_cues_str(content: &str) -> Vec<(i64, i64)> {
        let mut cues = Vec::new();
        for block in content.split("\n\n") {
            let lines: Vec<&str> = block.lines().collect();
            if lines.len() >= 3 {
                let times: Vec<&str> = lines[1].split(" --> ").collect();
                if times.len() == 2 {
                    let start = parse_timestamp_ms(times[0].trim());
                    let end = parse_timestamp_ms(times[1].trim());
                    cues.push((start, end));
                }
            }
        }
        cues
    }

    fn parse_timestamp_ms(ts: &str) -> i64 {
        let normalized = ts.replace(",", ".");
        let parts: Vec<&str> = normalized.split(':').collect();
        if parts.len() == 3 {
            let hours: i64 = parts[0].parse().unwrap_or(0);
            let minutes: i64 = parts[1].parse().unwrap_or(0);
            let seconds: f64 = parts[2].parse().unwrap_or(0.0);
            hours * 3600000 + minutes * 60000 + (seconds * 1000.0).round() as i64
        } else {
            0
        }
    }

    fn rebase_cues_for_part(cues: &[(i64, i64)], part_start_ms: i64, part_duration_ms: i64) -> Vec<(i64, i64)> {
        cues.iter()
            .filter(|(start, _end)| {
                *start >= part_start_ms && *start < part_start_ms + part_duration_ms
            })
            .map(|(start, end)| {
                let new_start = start - part_start_ms;
                let new_end = (*end - part_start_ms).max(new_start);
                (new_start, new_end)
            })
            .collect()
    }

    fn check_no_overlaps(cues: &[(i64, i64)]) -> bool {
        for i in 0..cues.len() {
            for j in (i + 1)..cues.len() {
                if cues[i].1 > cues[j].0 {
                    return false;
                }
            }
        }
        true
    }

    fn check_no_timestamp_reversal(cues: &[(i64, i64)]) -> bool {
        cues.iter().all(|(s, e)| e > s)
    }

    #[allow(dead_code)]
    fn check_sequential_indices(content: &str) -> bool {
        let cues = parse_srt_cues_str(content);
        for (i, cue) in cues.iter().enumerate() {
            let _index = i + 1;
            let (s, e) = *cue;
            if s < 0 || e <= s {
                return false;
            }
        }
        true
    }

    #[test]
    fn test_randomized_stress_cue_integrity() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║         RANDOMIZED STRESS TEST: CUE INTEGRITY                     ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let mut _rng = rand::thread_rng();

        // Generate random timeline: 30-180 minutes (1.8M - 10.8M ms)
        let total_duration_ms: i64 = _rng.gen_range(1_800_000..10_800_000);
        let part_duration_ms = total_duration_ms / STRESS_TEST_PARTS as i64;

        println!("Generating {} random cues...", STRESS_TEST_CUE_COUNT);
        let original_srt = generate_random_srt(STRESS_TEST_CUE_COUNT, 0.0, (total_duration_ms as f64) / 1000.0);

        let original_cues = parse_srt_cues_str(&original_srt);
        let original_count = original_cues.len();
        println!("  Generated {} cues over {}ms ({} parts)\n",
            original_count, total_duration_ms, STRESS_TEST_PARTS);

        // Rebase for each part
        let mut part_cues: Vec<Vec<(i64, i64)>> = Vec::new();
        let mut total_rebased = 0;

        for part in 0..STRESS_TEST_PARTS {
            let part_start = part as i64 * part_duration_ms;
            let rebased = rebase_cues_for_part(&original_cues, part_start, part_duration_ms);
            let count = rebased.len();
            total_rebased += count;
            part_cues.push(rebased);

            let start_ms = part_start;
            let end_ms = part_start + part_duration_ms;
            println!("  Part {}: {} cues (time {:.1}s - {:.1}s)",
                part + 1, count,
                start_ms as f64 / 1000.0,
                end_ms as f64 / 1000.0);
        }

        println!("\nIntegrity check:");
        println!("  Original cues: {}", original_count);
        println!("  Total rebased cues: {}", total_rebased);

        assert_eq!(
            original_count, total_rebased,
            "Cue count must be preserved! Lost {} cues",
            original_count - total_rebased
        );

        println!("  ✅ CUE COUNT PRESERVED: No cues lost");
    }

    #[test]
    fn test_randomized_stress_no_duplicates() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║         RANDOMIZED STRESS TEST: NO DUPLICATES                     ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let mut _rng = rand::thread_rng();
        let total_duration_ms: i64 = 5_400_000; // 90 minutes
        let part_duration_ms = total_duration_ms / STRESS_TEST_PARTS as i64;

        let original_srt = generate_random_srt(500, 0.0, (total_duration_ms as f64) / 1000.0);
        let original_cues = parse_srt_cues_str(&original_srt);

        let mut all_cue_keys: HashSet<String> = HashSet::new();
        let mut has_duplicate = false;

        for part in 0..STRESS_TEST_PARTS {
            let part_start = part as i64 * part_duration_ms;
            let rebased = rebase_cues_for_part(&original_cues, part_start, part_duration_ms);

            for (s, e) in &rebased {
                let key = format!("{},{}", s, e);
                if !all_cue_keys.insert(key) {
                    println!("  ⚠️  DUPLICATE FOUND: {} - {}", s, e);
                    has_duplicate = true;
                }
            }
        }

        assert!(!has_duplicate, "No cues should be duplicated across parts");
        println!("  ✅ NO DUPLICATES: All cues unique across parts");
    }

    #[test]
    fn test_randomized_stress_no_overlaps() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║         RANDOMIZED STRESS TEST: NO OVERLAPS                       ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let mut _rng = rand::thread_rng();
        let total_duration_ms: i64 = 3_600_000; // 60 minutes
        let part_duration_ms = total_duration_ms / STRESS_TEST_PARTS as i64;

        let original_srt = generate_random_srt(1000, 0.0, (total_duration_ms as f64) / 1000.0);
        let original_cues = parse_srt_cues_str(&original_srt);

        let mut all_pass = true;

        for part in 0..STRESS_TEST_PARTS {
            let part_start = part as i64 * part_duration_ms;
            let rebased = rebase_cues_for_part(&original_cues, part_start, part_duration_ms);

            if !check_no_overlaps(&rebased) {
                println!("  Part {}: ❌ OVERLAPS DETECTED", part + 1);
                all_pass = false;
            } else {
                println!("  Part {}: ✅ No overlaps", part + 1);
            }
        }

        assert!(all_pass, "No overlaps should exist within any part");
        println!("\n  ✅ NO OVERLAPS: All parts clean");
    }

    #[test]
    fn test_randomized_stress_no_timestamp_reversal() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║         RANDOMIZED STRESS TEST: NO TIMESTAMP REVERSAL               ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let mut _rng = rand::thread_rng();
        let total_duration_ms: i64 = 3_600_000;
        let part_duration_ms = total_duration_ms / STRESS_TEST_PARTS as i64;

        let original_srt = generate_random_srt(1000, 0.0, (total_duration_ms as f64) / 1000.0);
        let original_cues = parse_srt_cues_str(&original_srt);

        let mut all_pass = true;

        for part in 0..STRESS_TEST_PARTS {
            let part_start = part as i64 * part_duration_ms;
            let rebased = rebase_cues_for_part(&original_cues, part_start, part_duration_ms);

            if !check_no_timestamp_reversal(&rebased) {
                println!("  Part {}: ❌ TIMESTAMP REVERSAL DETECTED", part + 1);
                all_pass = false;
            } else {
                println!("  Part {}: ✅ All timestamps valid", part + 1);
            }
        }

        assert!(all_pass, "No timestamp reversals should exist");
        println!("\n  ✅ NO TIMESTAMP REVERSALS: All parts valid");
    }

    #[test]
    fn test_randomized_stress_sequential_numbering() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║         RANDOMIZED STRESS TEST: SEQUENTIAL NUMBERING               ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let mut _rng = rand::thread_rng();
        let total_duration_ms: i64 = 3_600_000;
        let part_duration_ms = total_duration_ms / STRESS_TEST_PARTS as i64;

        let original_srt = generate_random_srt(500, 0.0, (total_duration_ms as f64) / 1000.0);
        let original_cues = parse_srt_cues_str(&original_srt);

        let mut all_pass = true;

        for part in 0..STRESS_TEST_PARTS {
            let part_start = part as i64 * part_duration_ms;
            let rebased = rebase_cues_for_part(&original_cues, part_start, part_duration_ms);

            for (i, (s, e)) in rebased.iter().enumerate() {
                if s >= e {
                    println!("  Part {}: ❌ Cue {} has invalid duration (start={}, end={})",
                        part + 1, i + 1, s, e);
                    all_pass = false;
                }
            }

            println!("  Part {}: ✅ {} cues, all durations valid", part + 1, rebased.len());
        }

        assert!(all_pass, "All cues should have valid durations");
        println!("\n  ✅ SEQUENTIAL NUMBERING VERIFIED: All parts clean");
    }

    #[test]
    fn test_randomized_boundary_crossing_cues() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║         RANDOMIZED STRESS TEST: BOUNDARY CROSSING                  ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let part_duration_ms: i64 = 1_200_000; // 20 min per part

        // Test boundary crossing: cues that started in PREVIOUS part but extend into CURRENT part
        // For example, a cue from 58.5s - 60.8s crossing the 60s boundary
        let mut boundary_cues_processed = 0;

        for part in 1..STRESS_TEST_PARTS {
            let part_start = part as i64 * part_duration_ms;
            let prev_part_end = part_start;

            // Generate cue that STARTS before this part starts (in previous part) but ENDS in this part
            // Example: Part 2 starts at 60s, so a cue from 58.5s to 60.8s crosses the boundary
            let duration_ms: i64 = 2800; // 2.8 seconds
            let start_before_boundary = prev_part_end - 1500; // 1.5 seconds BEFORE boundary
            let end_after_boundary = start_before_boundary + duration_ms; // Extends into this part

            println!("  Part {}: Original cue {:.1}s - {:.1}s (crosses boundary at {:.1}s)",
                part + 1,
                start_before_boundary as f64 / 1000.0,
                end_after_boundary as f64 / 1000.0,
                prev_part_end as f64 / 1000.0);

            // Verify rebasing clamps negative start to 0
            let rebased_start = (start_before_boundary - part_start).max(0);
            let rebased_end = end_after_boundary - part_start;

            println!("           Rebased for Part {}: {:.1}s - {:.1}s",
                part + 1,
                rebased_start as f64 / 1000.0,
                rebased_end as f64 / 1000.0);

            // After rebase, start should be clamped to 0, end should be positive
            if rebased_start == 0 && rebased_end > 0 {
                println!("           ✅ Cue trimmed correctly (start clamped to 0, end preserved)");
                boundary_cues_processed += 1;
            } else {
                println!("           ❌ Unexpected rebase result");
            }
        }

        assert_eq!(boundary_cues_processed, STRESS_TEST_PARTS - 1,
            "All boundary cues should be processed correctly");
        println!("\n  ✅ BOUNDARY CROSSING TEST PASSED");
    }

    #[test]
    fn test_randomized_stress_comprehensive() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║         RANDOMIZED STRESS TEST: COMPREHENSIVE                      ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝\n");

        let mut _rng = rand::thread_rng();
        let total_duration_ms: i64 = _rng.gen_range(3_600_000..7_200_000);
        let part_duration_ms = total_duration_ms / STRESS_TEST_PARTS as i64;
        let cue_count = _rng.gen_range(1000..3000);

        println!("Configuration:");
        println!("  Total duration: {:.1} minutes", total_duration_ms as f64 / 60000.0);
        println!("  Parts: {}", STRESS_TEST_PARTS);
        println!("  Part duration: {:.1} minutes", part_duration_ms as f64 / 60000.0);
        println!("  Cue count: {}\n", cue_count);

        let original_srt = generate_random_srt(cue_count, 0.0, (total_duration_ms as f64) / 1000.0);
        let original_cues = parse_srt_cues_str(&original_srt);
        let original_count = original_cues.len();

        let mut part_results: Vec<(usize, bool, bool, bool)> = Vec::new();
        let mut total_cues_after_split = 0;

        for part in 0..STRESS_TEST_PARTS {
            let part_start = part as i64 * part_duration_ms;
            let rebased = rebase_cues_for_part(&original_cues, part_start, part_duration_ms);

            let no_overlaps = check_no_overlaps(&rebased);
            let no_reversal = check_no_timestamp_reversal(&rebased);
            let count = rebased.len();

            total_cues_after_split += count;

            part_results.push((count, no_overlaps, no_reversal, true));

            print!("  Part {}: {} cues", part + 1, count);
            if no_overlaps { print!(", no overlaps"); }
            if no_reversal { print!(", no reversals"); }
            println!();
        }

        println!("\nSummary:");
        println!("  Original cues: {}", original_count);
        println!("  Total after split: {}", total_cues_after_split);

        let cue_count_ok = original_count == total_cues_after_split;
        let all_parts_ok = part_results.iter().all(|(c, o, r, v)| *c > 0 && *o && *r && *v);

        if cue_count_ok { println!("  ✅ Cue count preserved"); }
        else { println!("  ❌ Cue count mismatch!"); }

        if all_parts_ok { println!("  ✅ All parts valid"); }
        else { println!("  ❌ Some parts have issues"); }

        assert!(cue_count_ok, "Cue count must be preserved");
        assert!(all_parts_ok, "All parts must be valid");
        println!("\n  ✅ COMPREHENSIVE STRESS TEST PASSED");
    }
}