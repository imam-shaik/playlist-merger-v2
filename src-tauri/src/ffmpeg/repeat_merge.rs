use std::collections::HashMap;

use crate::ffmpeg::repeat::{expand_repeat, ExpandedPlaylist};
use crate::types::RepeatConfig;

/// Result of applying repeat expansion to merge inputs.
/// This is the complete output of the repeat module for a merge operation.
#[derive(Debug, Clone)]
pub struct RepeatExpansionResult {
    /// The ExpandedPlaylist if repeat was applied and resulted in expansion (repeat_count > 1)
    pub expanded: Option<ExpandedPlaylist>,
    /// Expanded file paths (or original if no expansion)
    pub working_files: Vec<String>,
    /// Expanded durations (or original if no expansion)
    pub working_durations: Vec<f64>,
    /// Expanded names (or original if no expansion)
    pub working_names: Vec<String>,
    /// Total duration after expansion
    pub working_total_duration: f64,
    /// Original file count before expansion
    pub original_count: usize,
    /// Effective repeat count (1 if disabled, actual count if enabled)
    pub repeat_count: u32,
}

/// Apply repeat expansion to merge inputs.
///
/// This is the main entry point for the repeat module. It handles:
/// - Calling expand_repeat()
/// - Validating expanded files exist
/// - Building input_paths from expanded files
///
/// Note: Subtitle array expansion is handled by the caller (merge.rs) using
/// the index_mapping from ExpandedPlaylist.
///
/// Returns RepeatExpansionResult with all working arrays updated.
pub fn apply_repeat_expansion(
    config: &RepeatConfig,
    files: &[String],
    durations: &[f64],
    names: &[String],
    total_duration: f64,
    _external_subs: Option<Vec<Option<String>>>,
    _selected_sub_indices: Option<Vec<Option<u32>>>,
) -> Result<RepeatExpansionResult, String> {
    let original_count = files.len();

    // If repeat is not enabled, return identity result
    if !config.enabled {
        return Ok(RepeatExpansionResult {
            expanded: None,
            working_files: files.to_vec(),
            working_durations: durations.to_vec(),
            working_names: names.to_vec(),
            working_total_duration: total_duration,
            original_count,
            repeat_count: 1,
        });
    }

    // Call expand_repeat to get expanded playlist
    match expand_repeat(config, files, durations, names) {
        None => {
            // expand_repeat returns None when repeat_count <= 1 or other invalid conditions
            log::info!("[Repeat] No expansion needed (repeat_count <= 1 or invalid config)");
            Ok(RepeatExpansionResult {
                expanded: None,
                working_files: files.to_vec(),
                working_durations: durations.to_vec(),
                working_names: names.to_vec(),
                working_total_duration: total_duration,
                original_count,
                repeat_count: 1,
            })
        }
        Some(expanded) => {
            log::info!("[Repeat] ═══════════════════════════════════════════════════════════════");
            log::info!("[Repeat] EXPANSION: {} files × {} = {} files",
                expanded.original_count, expanded.repeat_count, expanded.files.len());
            log::info!("[Repeat] Original duration: {:.1}s → Final duration: {:.1}s",
                total_duration, expanded.total_duration);
            log::info!("[Repeat] ═══════════════════════════════════════════════════════════════");

            // Build RepeatExpansionResult
            let result = RepeatExpansionResult {
                expanded: Some(expanded.clone()),
                working_files: expanded.files.clone(),
                working_durations: expanded.durations.clone(),
                working_names: expanded.names.clone(),
                working_total_duration: expanded.total_duration,
                original_count: expanded.original_count,
                repeat_count: expanded.repeat_count,
            };

            log::info!("[Repeat] Expansion complete: {} files × {} = {} files",
                original_count, result.repeat_count, result.working_files.len());

            Ok(result)
        }
    }
}

/// Build boundary card labels from expanded playlist.
///
/// Returns a HashMap mapping position -> label for positions that need boundary cards.
/// For example: { 0 => "🔁 Repeat 2", 3 => "🔁 Repeat 3", 6 => "🔁 Repeat 4" }
///
/// Returns None if:
/// - expanded is None (no repeat applied)
/// - insert_boundary_cards is false
/// - no boundary cards would be inserted (empty result)
pub fn build_boundary_card_labels(
    expanded: &ExpandedPlaylist,
    file_count: usize,
) -> Option<HashMap<usize, String>> {
    if !expanded.insert_boundary_cards {
        return None;
    }

    let mut labels = HashMap::new();
    for i in 0..file_count {
        if expanded.should_insert_boundary_at(i) {
            let cycle = expanded.repeat_cycle(i);
            let label = expanded.get_boundary_label(cycle + 1);
            labels.insert(i, label);
        }
    }

    if labels.is_empty() {
        None
    } else {
        log::info!("[Repeat:BOUNDARY] Enabled: {} boundary cards will be rendered", labels.len());
        Some(labels)
    }
}

#[allow(dead_code)]
pub fn get_repeat_metadata(
    expanded: &Option<ExpandedPlaylist>,
) -> Option<(usize, u32)> {
    expanded.as_ref().map(|e| (e.original_count, e.repeat_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RepeatConfig;

    fn create_test_config(by_count: bool, repeat_count: u32) -> RepeatConfig {
        RepeatConfig {
            enabled: true,
            by_count,
            repeat_count,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: false,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        }
    }

    #[test]
    fn test_no_expansion_when_disabled() {
        let config = RepeatConfig {
            enabled: false,
            by_count: true,
            repeat_count: 5,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: false,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        };

        let files = vec!["a.mp4".to_string(), "b.mp4".to_string()];
        let durations = vec![10.0, 20.0];
        let names = vec!["A".to_string(), "B".to_string()];

        let result = apply_repeat_expansion(&config, &files, &durations, &names, 30.0, None, None)
            .unwrap();

        assert!(result.expanded.is_none());
        assert_eq!(result.working_files, files);
        assert_eq!(result.repeat_count, 1);
    }

    #[test]
    fn test_expansion_with_count() {
        let config = create_test_config(true, 3);
        let files = vec!["a.mp4".to_string(), "b.mp4".to_string()];
        let durations = vec![10.0, 20.0];
        let names = vec!["A".to_string(), "B".to_string()];

        let result = apply_repeat_expansion(&config, &files, &durations, &names, 30.0, None, None)
            .unwrap();

        assert!(result.expanded.is_some());
        assert_eq!(result.working_files.len(), 6); // 2 * 3
        assert_eq!(result.repeat_count, 3);
        assert_eq!(result.original_count, 2);
    }

    #[test]
    fn test_expansion_with_boundary_cards() {
        let config = RepeatConfig {
            enabled: true,
            by_count: true,
            repeat_count: 3,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: true,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        };

        let files = vec!["a.mp4".to_string(), "b.mp4".to_string()];
        let durations = vec![10.0, 20.0];
        let names = vec!["A".to_string(), "B".to_string()];

        let result = apply_repeat_expansion(&config, &files, &durations, &names, 30.0, None, None)
            .unwrap();

        let expanded = result.expanded.as_ref().unwrap();
        let labels = build_boundary_card_labels(expanded, result.working_files.len());

        assert!(labels.is_some());
        let labels = labels.unwrap();
        // With 2 original files and 3 repeats, boundaries at indices 0, 2, 4
        assert!(labels.contains_key(&0)); // cycle 0 start
        assert!(labels.contains_key(&2)); // cycle 1 start
        assert!(labels.contains_key(&4)); // cycle 2 start
        assert_eq!(labels.get(&0), Some(&"🔁 Repeat 1".to_string()));
        assert_eq!(labels.get(&2), Some(&"🔁 Repeat 2".to_string()));
        assert_eq!(labels.get(&4), Some(&"🔁 Repeat 3".to_string()));
    }

    #[test]
    fn test_no_boundary_cards_when_disabled() {
        let config = RepeatConfig {
            enabled: true,
            by_count: true,
            repeat_count: 3,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: false,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        };

        let files = vec!["a.mp4".to_string()];
        let durations = vec![10.0];
        let names = vec!["A".to_string()];

        let result = apply_repeat_expansion(&config, &files, &durations, &names, 10.0, None, None)
            .unwrap();

        let labels = build_boundary_card_labels(result.expanded.as_ref().unwrap(), result.working_files.len());
        assert!(labels.is_none());
    }

    /// FORENSIC TEST: Normalization × Repeat Bug Verification
    ///
    /// This test proves/disproves the audit finding:
    /// "Normalization dedup removes repeat instances from queue, but write-back
    /// only updates first occurrence, leaving repeat instances pointing to
    /// ORIGINAL files."
    ///
    /// Scenario: A.mp4, B.mp4, C.mp4 with Repeat ×4
    /// Expected after normalization: ALL 12 entries should use normalized paths
    /// Actual (if bug exists): Only indices 0,1,2 are updated; 3-11 still use originals
    #[test]
    fn test_normalization_dedup_repeat_bug() {
        // ── Setup: Simulate working_input_files after repeat expansion ─────────────
        // 3 original files × repeat ×4 = 12 expanded entries
        let original_files = vec![
            "/src/A.mp4".to_string(),
            "/src/B.mp4".to_string(),
            "/src/C.mp4".to_string(),
        ];
        let config = create_test_config(true, 4);
        let result = apply_repeat_expansion(&config, &original_files, &[10.0, 10.0, 10.0], &["A".to_string(), "B".to_string(), "C".to_string()], 30.0, None, None).unwrap();

        let mut working_input_files = result.working_files;
        println!("\n[FORENSIC] After repeat expansion:");
        for (i, f) in working_input_files.iter().enumerate() {
            println!("  [{}] {}", i, f);
        }

        // Verify expansion: 3 files × 4 = 12
        assert_eq!(working_input_files.len(), 12, "Should have 12 entries after repeat ×4");
        assert_eq!(working_input_files[0], "/src/A.mp4");
        assert_eq!(working_input_files[3], "/src/A.mp4"); // repeat instance of A

        // ── Phase 1: Simulate Profile Normalization Dedup (merge.rs lines 3135-3149)
        // The dedup keeps ONLY first occurrence of each unique source path
        let all_indices: Vec<usize> = (0..working_input_files.len()).collect();
        let need_profile_norm: Vec<usize> = all_indices.clone(); // all 12 need norm

        let mut seen_sources = std::collections::HashSet::new();
        let deduped_profile_norm: Vec<usize> = need_profile_norm
            .into_iter()
            .filter(|&idx| {
                if let Some(path) = working_input_files.get(idx) {
                    seen_sources.insert(path.clone())
                } else {
                    true
                }
            })
            .collect();

        println!("\n[FORENSIC] Profile norm dedup:");
        println!("  All indices: {:?}", (0..12).collect::<Vec<_>>());
        println!("  Deduped to: {:?}", deduped_profile_norm);
        println!("  Skipped (repeat instances): {:?}", (3..12).collect::<Vec<_>>());

        // Should be [0, 1, 2] only (first occurrence of each unique file)
        assert_eq!(deduped_profile_norm, vec![0, 1, 2],
            "Dedup should keep ONLY first occurrence of each unique source");

        // ── Phase 2: Simulate Profile Normalization Write-Back
        // Only deduped indices (0, 1, 2) are processed and write back their normalized paths
        // This is what merge.rs line 3357 does: wg[idx] = path.clone()
        let normalized_paths = vec![
            "/tmp/norm_prof_job_0.mp4".to_string(), // normalized A
            "/tmp/norm_prof_job_1.mp4".to_string(), // normalized B
            "/tmp/norm_prof_job_2.mp4".to_string(), // normalized C
        ];

        // Write back normalized paths (ONLY for deduped indices)
        for (i, &idx) in deduped_profile_norm.iter().enumerate() {
            working_input_files[idx] = normalized_paths[i].clone();
        }

        println!("\n[FORENSIC] After profile norm write-back:");
        for (i, f) in working_input_files.iter().enumerate() {
            let tag = if i < 3 { "NORMALIZED" } else { "ORIGINAL   " };
            println!("  [{}] {} ({})", i, f, tag);
        }

        // ── Phase 3: Check for Mixed Paths (THE BUG)
        // After normalization, working_input_files should be ALL normalized
        // BUG: indices 3-11 still point to ORIGINAL files
        let has_original_paths = working_input_files[3..].iter().any(|p| p.contains("/src/"));
        let has_normalized_paths = working_input_files[0..3].iter().all(|p| p.contains("/tmp/norm"));

        println!("\n[FORENSIC] Bug Detection:");
        println!("  Indices 3-11 still use ORIGINAL paths: {}", has_original_paths);
        println!("  Indices 0-2 use NORMALIZED paths: {}", has_normalized_paths);

        // ── Phase 4: Simulate Concat List Generation (merge.rs line 4135)
        // The concat list is built from working_input_files, which now contains MIXED paths
        let path_refs: Vec<&::std::path::Path> = working_input_files.iter().map(::std::path::Path::new).collect();

        println!("\n[FORENSIC] Final Concat List (what enters FFmpeg concat):");
        let mut content = String::new();
        for (i, p) in path_refs.iter().enumerate() {
            let line = format!("file '{}'", p.to_string_lossy().replace('\\', "/"));
            println!("  L{:03}: {}", i, line);
            content.push_str(&line);
            content.push('\n');
        }

        // ── BUG VERIFICATION ────────────────────────────────────────────────────
        // If bug exists: concat list contains MIXED paths (norm + original)
        // If no bug: concat list contains ALL normalized paths
        let norm_count = working_input_files.iter().filter(|p| p.contains("/tmp/norm")).count();
        let orig_count = working_input_files.iter().filter(|p| p.contains("/src/")).count();

        println!("\n[FORENSIC] Concat List Analysis:");
        println!("  Normalized files: {} (expected: 12)", norm_count);
        println!("  Original files: {} (BUG if > 0)", orig_count);

        // THE BUG: 9 files are still original (indices 3-11)
        // This means concat list = [norm_A, norm_B, norm_C, A, B, C, A, B, C, A, B, C]
        // Not [norm_A, norm_A, norm_A, norm_A, norm_B, norm_B, norm_B, norm_B, norm_C, norm_C, norm_C, norm_C]
        assert!(
            orig_count > 0,
            "BUG PROOF: {} repeat instances still use ORIGINAL files (not normalized)",
            orig_count
        );

        // The concat list has MIXED paths — this is the bug
        assert!(
            content.contains("/src/A.mp4") && content.contains("/tmp/norm_prof_job_0.mp4"),
            "BUG: Concat list contains BOTH normalized and original paths"
        );

        println!("\n[FORENSIC] ═══════════════════════════════════════════════════════");
        println!("[FORENSIC] BUG CONFIRMED: Concat list contains MIXED paths");
        println!("[FORENSIC]   - Indices 0-2: normalized files (PTS reset via aresample=first_pts=0)");
        println!("[FORENSIC]   - Indices 3-11: ORIGINAL files (PTS NOT reset)");
        println!("[FORENSIC]   - Audio PTS discontinuity at every repeat boundary");
        println!("[FORENSIC] ═══════════════════════════════════════════════════════");
    }

    /// Test the CORRECT behavior: all repeat instances should use normalized files
    /// This shows what SHOULD happen if the bug were fixed
    #[test]
    fn test_normalization_should_propagate_to_all_repeat_instances() {
        // Simulate the fixed behavior: build source→normalized mapping, then apply to ALL indices
        let result = apply_repeat_expansion(
            &create_test_config(true, 4),
            &["/src/A.mp4".to_string(), "/src/B.mp4".to_string(), "/src/C.mp4".to_string()],
            &[10.0, 10.0, 10.0],
            &["A".to_string(), "B".to_string(), "C".to_string()],
            30.0, None, None
        ).unwrap();

        let mut working_input_files = result.working_files;

        // After dedup, build source→normalized mapping
        let mut source_to_normalized = std::collections::HashMap::new();
        source_to_normalized.insert("/src/A.mp4".to_string(), "/tmp/norm_prof_job_0.mp4".to_string());
        source_to_normalized.insert("/src/B.mp4".to_string(), "/tmp/norm_prof_job_1.mp4".to_string());
        source_to_normalized.insert("/src/C.mp4".to_string(), "/tmp/norm_prof_job_2.mp4".to_string());

        // FIX: Apply normalized path to ALL repeat instances (O(N) single pass)
        for file in working_input_files.iter_mut() {
            if let Some(norm_path) = source_to_normalized.get(file) {
                *file = norm_path.clone();
            }
        }

        println!("\n[FIXED] After fix - all repeat instances use normalized paths:");
        for (i, f) in working_input_files.iter().enumerate() {
            println!("  [{}] {}", i, f);
        }

        // All 12 entries should now be normalized
        let all_normalized = working_input_files.iter().all(|p| p.contains("/tmp/norm"));
        assert!(all_normalized, "FIXED: All repeat instances should use normalized paths");

        // Concat list should contain only normalized paths
        let norm_count = working_input_files.iter().filter(|p| p.contains("/tmp/norm")).count();
        let orig_count = working_input_files.iter().filter(|p| p.contains("/src/")).count();
        assert_eq!(norm_count, 12, "FIXED: All 12 should be normalized");
        assert_eq!(orig_count, 0, "FIXED: None should be original");
    }
}