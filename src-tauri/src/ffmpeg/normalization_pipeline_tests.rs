#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use crate::ffmpeg::probe::probe_file;
    use crate::ffmpeg::normalization::analyze_profiles;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn get_production_files() -> Vec<PathBuf> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let base = root.parent().unwrap().join("tests").join("fixtures").join("production_test");
        let mut files = Vec::new();

        // file_0 to file_11: dominant (11 files)
        for i in 0..=11 {
            files.push(base.join(format!("file_{}_dominant.mp4", i)));
        }
        // file_12 to file_14: timebase outlier (3 files)
        for i in 12..=14 {
            files.push(base.join(format!("file_{}_tb_outlier.mp4", i)));
        }
        // file_15 to file_17: audio sample_rate outlier (3 files)
        for i in 15..=17 {
            files.push(base.join(format!("file_{}_audio_outlier.mp4", i)));
        }
        // file_18 to file_19: both outliers (2 files)
        for i in 18..=19 {
            files.push(base.join(format!("file_{}_BUG_BOTH.mp4", i)));
        }

        files
    }

    /// NORMALIZATION FORENSICS TEST — Phase 2A Validation
    ///
    /// Uses 19 production_test files with KNOWN mismatches:
    ///   - 11 dominant files (file_0 to file_11) — match dominant, NO normalization needed
    ///   - 3 timebase outliers (file_12 to file_14) — RemuxOnly normalization
    ///   - 3 audio sample_rate outliers (file_15 to file_17) — AudioReencode normalization
    ///   - 2 both outliers (file_18 to file_19) — FullReencode normalization
    ///
    /// Expected forensic report:
    ///   Normalized: 8 files (3 tb + 3 audio_sr + 2 both)
    ///   NOT Normalized: 11 files
    ///   Top triggers: time_base, a_sample_rate
    #[tokio::test]
    async fn test_normalization_forensics_report() {
        let (_ffmpeg, ffprobe) = get_binaries();
        let files = get_production_files();

        // Filter to only files that exist
        let files: Vec<_> = files.into_iter().filter(|f| f.exists()).collect();
        println!("\n[NORM_FORENSICS] Found {} production_test files", files.len());

        // Probe all files
        let mut profile_infos = Vec::new();
        for (i, file) in files.iter().enumerate() {
            match probe_file(&ffprobe, file) {
                Ok(info) => {
                    let filename = file.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("file_{}", i));
                    println!("[PROBE] #{:<3} | {} | TB: {:?} | SR: {:?} | FPS: {:?}",
                        i,
                        filename,
                        info.video_streams.first().and_then(|s| s.time_base.clone()),
                        info.audio_streams.first().and_then(|s| s.sample_rate),
                        info.video_streams.first().and_then(|s| s.fps));
                    profile_infos.push((i, file.to_string_lossy().into_owned(), info));
                }
                Err(e) => {
                    println!("[PROBE] #{:<3} | {} | ERROR: {}", i, file.display(), e);
                }
            }
        }

        // Analyze profiles
        let analysis = analyze_profiles(&profile_infos);

        // ═══════════════════════════════════════════════════════════════
        // NORMALIZATION FORENSICS REPORT
        // ═══════════════════════════════════════════════════════════════
        println!("\n[NORM_FORENSICS] ════════════════════════════════════════════════════════");
        println!("[NORM_FORENSICS] NORMALIZATION FORENSICS REPORT (production_test)");
        println!("[NORM_FORENSICS] ════════════════════════════════════════════════════════");
        println!("[NORM_FORENSICS] Total Files: {}", files.len());

        // Count normalized vs not
        let mut norm_indices = std::collections::HashSet::new();
        let mut prop_counts = std::collections::HashMap::<String, usize>::new();
        let mut norm_type_counts = std::collections::HashMap::<String, usize>::new();

        for o in &analysis.outliers {
            norm_indices.insert(o.index);
            *prop_counts.entry(o.property.clone()).or_insert(0) += 1;
            let nt = format!("{:?}", o.normalization_type);
            *norm_type_counts.entry(nt).or_insert(0) += 1;
        }

        let total_normalized = norm_indices.len();
        let total_not_normalized = files.len() - total_normalized;

        println!("[NORM_FORENSICS] Normalized: {}", total_normalized);
        println!("[NORM_FORENSICS] NOT Normalized: {}", total_not_normalized);
        println!("[NORM_FORENSICS] ────────────────────────────────────────────────────");

        // Per-file breakdown
        println!("[NORM_FORENSICS] PER-FILE BREAKDOWN:");
        let mut file_list: Vec<_> = norm_indices.iter().collect();
        file_list.sort();
        for &idx in &file_list {
            let file = &files[*idx];
            let filename = file.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("file_{}", idx));
            let outliers_for_file: Vec<_> = analysis.outliers.iter()
                .filter(|o| o.index == *idx)
                .map(|o| format!("{}:{}→{}", o.property, o.actual_value, o.dominant_value))
                .collect();
            println!("[NORM_FORENSICS]   File #{:<4} | {:<30} | {}",
                idx, format!("\"{}\"", filename), outliers_for_file.join(", "));
        }
        println!("[NORM_FORENSICS] ────────────────────────────────────────────────────");

        // Property frequency
        println!("[NORM_FORENSICS] TRIGGER FREQUENCY (ranked by count):");
        let mut prop_sorted: Vec<_> = prop_counts.iter().collect();
        prop_sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (prop, count) in prop_sorted {
            let pct = if total_normalized > 0 { (*count as f64 / total_normalized as f64) * 100.0 } else { 0.0 };
            println!("[NORM_FORENSICS]   {:>6.1}% | {:>5} files | {}", pct, count, prop);
        }
        println!("[NORM_FORENSICS] ────────────────────────────────────────────────────");

        // Normalization type frequency
        println!("[NORM_FORENSICS] NORMALIZATION TYPE BREAKDOWN:");
        let mut type_sorted: Vec<_> = norm_type_counts.iter().collect();
        type_sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (t, count) in type_sorted {
            let pct = if total_normalized > 0 { (*count as f64 / total_normalized as f64) * 100.0 } else { 0.0 };
            println!("[NORM_FORENSICS]   {:>6.1}% | {:>5} files | {}", pct, count, t);
        }
        println!("[NORM_FORENSICS] ────────────────────────────────────────────────────");

        // Dominant profile
        println!("[NORM_FORENSICS] DOMINANT PROFILE:");
        println!("[NORM_FORENSICS]   v_time_base:  {:?}", analysis.dominant.v_time_base);
        println!("[NORM_FORENSICS]   v_fps:        {:?}", analysis.dominant.v_fps);
        println!("[NORM_FORENSICS]   a_sample_rate: {:?}", analysis.dominant.a_sample_rate);
        println!("[NORM_FORENSICS]   a_channels:   {:?}", analysis.dominant.a_channels);
        println!("[NORM_FORENSICS]   v_codec:      {:?}", analysis.dominant.v_codec);
        println!("[NORM_FORENSICS]   a_codec:      {:?}", analysis.dominant.a_codec);

        // Audio outlier breakdown
        println!("[NORM_FORENSICS] ────────────────────────────────────────────────────");
        println!("[NORM_FORENSICS] AUDIO OUTLIER TYPES:");
        let mut audio_type_counts = std::collections::HashMap::<String, usize>::new();
        for ao in &analysis.audio_outliers {
            *audio_type_counts.entry(ao.audio_type.code().to_string()).or_insert(0) += 1;
        }
        if audio_type_counts.is_empty() {
            println!("[NORM_FORENSICS]   (none detected)");
        } else {
            for (t, count) in audio_type_counts {
                println!("[NORM_FORENSICS]   {}: {} files", t, count);
            }
        }

        // ALL outliers list
        println!("[NORM_FORENSICS] ────────────────────────────────────────────────────");
        println!("[NORM_FORENSICS] ALL OUTLIERS DETECTED:");
        for o in &analysis.outliers {
            let _filename = files.get(o.index).and_then(|f| f.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("file_{}", o.index));
            println!("[NORM_FORENSICS]   [#{:<4}] {:20} | {} → {} | {:?}",
                o.index, o.property, o.actual_value, o.dominant_value, o.normalization_type);
        }

        println!("[NORM_FORENSICS] ════════════════════════════════════════════════════════");

        // Assertions to validate the test makes sense
        assert!(total_normalized > 0, "Expected at least some files to be normalized");
        assert!(total_not_normalized > 0, "Expected at least some files to NOT be normalized");

        // time_base and a_sample_rate should be the top triggers
        assert!(prop_counts.contains_key("time_base"), "Expected time_base to be a trigger");
        assert!(prop_counts.contains_key("a_sample_rate"), "Expected a_sample_rate to be a trigger");

        println!("\n[NORM_FORENSICS] ✅ VALIDATION COMPLETE");
    }

    #[tokio::test]
    async fn test_p0_fix_runtime_execution() {
        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("merge_p0_test");
        std::fs::create_dir_all(&test_dir).unwrap();

        let file_a = test_dir.join("file_30000_1.mp4");
        let file_b = test_dir.join("file_30000_2.mp4");
        let file_c = test_dir.join("file_15360.mp4");

        for f in [&file_a, &file_b] {
            Command::new(&ffmpeg)
                .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=160x120:rate=30", "-video_track_timescale", "30000", f.to_str().unwrap()])
                .output().unwrap();
        }

        Command::new(&ffmpeg)
            .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=1:size=160x120:rate=30", "-video_track_timescale", "15360", file_c.to_str().unwrap()])
            .output().unwrap();

        println!("\n[FORENSIC:RUNTIME_START] Testing File C (1/15360 Outlier)");

        let info_a = probe_file(&ffprobe, &file_a).unwrap();
        let info_b = probe_file(&ffprobe, &file_b).unwrap();
        let info_c = probe_file(&ffprobe, &file_c).unwrap();

        let mut working_input_files = vec![
            file_a.to_string_lossy().into_owned(),
            file_b.to_string_lossy().into_owned(),
            file_c.to_string_lossy().into_owned(),
        ];

        let profile_infos = vec![
            (0, working_input_files[0].clone(), info_a.clone()),
            (1, working_input_files[1].clone(), info_b.clone()),
            (2, working_input_files[2].clone(), info_c.clone()),
        ];
        let analysis = analyze_profiles(&profile_infos);

        println!("[FORENSIC:BEFORE] File #2 | TB: {:?}", info_c.video_streams[0].time_base);
        assert!(analysis.outliers.iter().any(|o| o.index == 2 && o.property == "time_base"));

        let target_timescale = analysis.dominant.timescale_den.unwrap_or(30000);
        let norm_output = test_dir.join("norm_fixed_2.mp4");

        println!("[FORENSIC:NORMALIZE] Executing timescale fix for File #2...");

        let res = Command::new(&ffmpeg)
            .args(["-y", "-i", &working_input_files[2], "-c", "copy", "-video_track_timescale", &target_timescale.to_string(), norm_output.to_str().unwrap()])
            .output().unwrap();

        if res.status.success() {
            let path = norm_output.to_string_lossy().into_owned();

            working_input_files[2] = path.clone();

            let new_info = probe_file(&ffprobe, Path::new(&path)).unwrap();
            println!("[FORENSIC:CACHE] Refreshed File #2 | NEW TB: {:?}", new_info.video_streams[0].time_base);

            println!("[FORENSIC:FINAL_VALIDATION] Checking: {}", working_input_files[2]);
            let final_profile_infos = vec![
                (0, working_input_files[0].clone(), info_a),
                (1, working_input_files[1].clone(), info_b),
                (2, working_input_files[2].clone(), new_info),
            ];
            let final_analysis = analyze_profiles(&final_profile_infos);

            println!("[FORENSIC:CONCAT_INPUT] [2] {}", working_input_files[2]);

            assert_eq!(final_analysis.outliers.len(), 0, "Final validation should have ZERO outliers after fix");
            println!("\n[FORENSIC:VERDICT] P0 FIX VERIFIED: PASS");
        } else {
            panic!("Normalization failed during test");
        }

        std::fs::remove_dir_all(&test_dir).unwrap();
    }
}