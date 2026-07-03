#[cfg(test)]
mod outlier_certification {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use crate::types::MediaInfo;
    use crate::ffmpeg::probe::probe_file;
    use crate::ffmpeg::normalization::{ProfileAnalysis, analyze_profiles};

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_file(
        ffmpeg: &Path,
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        video_codec: &str,
        audio_sr: u32,
        duration_sec: u32,
        _prefix: &str,
    ) {
        let _output = Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size={}x{}:rate={}", duration_sec, width, height, fps),
                "-f", "lavfi",
                "-i", &format!("sine=frequency=440:sample_rate={}", audio_sr),
                "-c:v", video_codec,
                "-preset", "ultrafast",
                "-c:a", "aac",
                "-ar", &audio_sr.to_string(),
                "-t", &duration_sec.to_string(),
                path.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create test file");
    }

    fn analyze_files(files: &[PathBuf]) -> ProfileAnalysis {
        let ffprobe = get_binaries().1;
        let infos: Vec<(usize, String, MediaInfo)> = files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                probe_file(&ffprobe, f)
                    .ok()
                    .map(|info| (i, f.to_string_lossy().into_owned(), info))
            })
            .collect();
        analyze_profiles(&infos)
    }

    /// OUTLIER NORMALIZATION CERTIFICATION TEST
    ///
    /// Creates 10 files:
    /// - Files 0-5: H264, 1080p, 30fps, AAC 48000 (compatible)
    /// - File 6: H265 (different video codec)
    /// - File 7: H265 (different video codec)
    /// - File 8: 60fps (different frame rate)
    /// - File 9: AAC 44100 (different sample rate)
    ///
    /// Expected: Files 0-5 should be SKIPPED, Files 6-9 should be NORMALIZED
    #[tokio::test]
    async fn test_outlier_only_normalization() {
        let test_dir = std::env::temp_dir().join("outlier_cert");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n═══════════════════════════════════════════════════════════════════════");
        println!("OUTLIER NORMALIZATION CERTIFICATION TEST");
        println!("═══════════════════════════════════════════════════════════════════════");

        let (ffmpeg, _ffprobe) = get_binaries();

        // Create 10 files
        let files: Vec<PathBuf> = (0..10).map(|i| test_dir.join(format!("file_{}.mp4", i))).collect();

        println!("\n[CREATING FILES 0-5: compatible (H264, 1080p, 30fps, 48000Hz AAC)]");
        for i in 0..6 {
            create_test_file(&ffmpeg, &files[i], 1920, 1080, 30, "libx264", 48000, 3, &format!("File-{}", i));
            println!("  Created File-{}", i);
        }

        println!("\n[CREATING OUTLIER FILES 6-9]");
        create_test_file(&ffmpeg, &files[6], 1920, 1080, 30, "libx265", 48000, 3, "File-6 (H265)");
        println!("  Created File-6 (H265 outlier)");
        create_test_file(&ffmpeg, &files[7], 1920, 1080, 30, "libx265", 48000, 3, "File-7 (H265)");
        println!("  Created File-7 (H265 outlier)");
        create_test_file(&ffmpeg, &files[8], 1920, 1080, 60, "libx264", 48000, 3, "File-8 (60fps)");
        println!("  Created File-8 (60fps outlier)");
        create_test_file(&ffmpeg, &files[9], 1920, 1080, 30, "libx264", 44100, 3, "File-9 (44100Hz)");
        println!("  Created File-9 (44100Hz outlier)");

        // Analyze
        println!("\n[ANALYZING FILES...]");
        let analysis = analyze_files(&files);

        // Print dominant profile
        println!("\n[DOMINANT PROFILE]");
        println!("  v_codec: {:?}", analysis.dominant.v_codec);
        println!("  v_width: {:?}", analysis.dominant.v_width);
        println!("  v_height: {:?}", analysis.dominant.v_height);
        println!("  v_fps: {:?}", analysis.dominant.v_fps);
        println!("  a_sample_rate: {:?}", analysis.dominant.a_sample_rate);

        // Print all outliers
        println!("\n[OUTLIERS DETECTED]");
        for outlier in &analysis.outliers {
            println!("  File-{}: norm_type={:?} reason={}", outlier.index, outlier.normalization_type, outlier.reason);
        }
        if analysis.outliers.is_empty() {
            println!("  (none)");
        }

        // Print audio outliers
        println!("\n[AUDIO OUTLIERS]");
        for ao in &analysis.audio_outliers {
            println!("  File-{}: sr={} dominant={}", ao.index, ao.actual_value, ao.dominant_value);
        }
        if analysis.audio_outliers.is_empty() {
            println!("  (none)");
        }

        // Build results
        let outlier_indices: std::collections::HashSet<usize> = analysis.outliers.iter()
            .map(|o| o.index)
            .chain(analysis.audio_outliers.iter().map(|a| a.index))
            .collect();

        println!("\n[CERTIFICATION RESULTS]");
        println!("");
        println!("  {:<10} | {:<20} | {}", "File", "Status", "Reason");
        println!("  {:─<10} | {:─<20} | {:}", "", "", "");

        let mut passed = true;
        for (i, _f) in files.iter().enumerate() {
            let (status, reason) = if outlier_indices.contains(&i) {
                let mut reasons: Vec<String> = analysis.outliers.iter()
                    .filter(|o| o.index == i)
                    .map(|o| o.reason.clone())
                    .collect();
                reasons.extend(analysis.audio_outliers.iter()
                    .filter(|a| a.index == i)
                    .map(|a| format!("a_sample_rate: {} != {}", a.actual_value, a.dominant_value)));
                let is_compatible = i < 6;
                if is_compatible {
                    passed = false;
                    ("NORMALIZED", format!("{} (SHOULD BE SKIPPED!)", reasons.join(", ")))
                } else {
                    ("NORMALIZED", reasons.join(", "))
                }
            } else {
                let is_outlier = i >= 6;
                if is_outlier {
                    passed = false;
                    ("SKIPPED", "should be NORMALIZED!".to_string())
                } else {
                    ("SKIPPED", "matches dominant".to_string())
                }
            };

            println!("  File-{:<5} | {:<20} | {}", i, status, reason);
        }

        println!("");
        println!("  ===============================================================");
        println!("  SUMMARY:");
        let skipped_compatible = (0..6).filter(|i| !outlier_indices.contains(i)).count();
        let normalized_outliers = (6..10).filter(|i| outlier_indices.contains(i)).count();
        let false_positives = (0..6).filter(|i| outlier_indices.contains(i)).count();
        let false_negatives = (6..10).filter(|i| !outlier_indices.contains(i)).count();

        println!("  Compatible files (0-5) skipped: {}/6", skipped_compatible);
        println!("  Outlier files (6-9) normalized: {}/4", normalized_outliers);
        println!("  False positives (compatible normalized): {}", false_positives);
        println!("  False negatives (outlier skipped): {}", false_negatives);

        if passed && skipped_compatible == 6 && normalized_outliers == 4 {
            println!("");
            println!("  CERTIFICATION PASSED!");
            println!("  Only outliers are normalized, compatible files are correctly skipped.");
        } else {
            println!("");
            println!("  CERTIFICATION FAILED!");
            println!("  Optimization is NOT working correctly.");
        }
        println!("  ===============================================================");

        // Cleanup
        for f in &files { std::fs::remove_file(f).ok(); }
        std::fs::remove_dir_all(&test_dir).ok();

        assert!(passed, "Outlier normalization certification FAILED");
    }

    /// Simple 3-file test: 2 compatible + 1 outlier
    #[tokio::test]
    async fn test_three_file_outlier_detection() {
        let test_dir = std::env::temp_dir().join("three_file_outlier");
        std::fs::create_dir_all(&test_dir).unwrap();

        let (ffmpeg, _ffprobe) = get_binaries();

        let f0 = test_dir.join("file_0.mp4");
        let f1 = test_dir.join("file_1.mp4");
        let f2 = test_dir.join("file_2.mp4");

        println!("\n═══════════════════════════════════════════════════════════════════════");
        println!("THREE-FILE OUTLIER TEST (2 compatible + 1 outlier)");
        println!("═══════════════════════════════════════════════════════════════════════");

        create_test_file(&ffmpeg, &f0, 1920, 1080, 25, "libx264", 44100, 3, "File-0 (dominant)");
        create_test_file(&ffmpeg, &f1, 1920, 1080, 25, "libx264", 44100, 3, "File-1 (dominant)");
        create_test_file(&ffmpeg, &f2, 1920, 1080, 25, "libx264", 48000, 3, "File-2 (48000Hz outlier)");

        let analysis = analyze_files(&[f0.clone(), f1.clone(), f2.clone()]);

        let outlier_indices: std::collections::HashSet<usize> = analysis.outliers.iter()
            .map(|o| o.index)
            .chain(analysis.audio_outliers.iter().map(|a| a.index))
            .collect();

        println!("\n[RESULTS]");
        println!("  File-0: {}", if outlier_indices.contains(&0) { "NORMALIZED" } else { "SKIPPED (expected)" });
        println!("  File-1: {}", if outlier_indices.contains(&1) { "NORMALIZED" } else { "SKIPPED (expected)" });
        println!("  File-2: {}", if outlier_indices.contains(&2) { "NORMALIZED (expected)" } else { "SKIPPED" });

        let passed = !outlier_indices.contains(&0) && !outlier_indices.contains(&1) && outlier_indices.contains(&2);

        if passed {
            println!("\n  PASS: 2 compatible skipped, 1 outlier normalized");
        } else {
            println!("\n  FAIL: Outlier detection not working correctly");
        }

        // Cleanup
        std::fs::remove_file(&f0).ok();
        std::fs::remove_file(&f1).ok();
        std::fs::remove_file(&f2).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        assert!(passed, "Three-file outlier detection FAILED");
    }
}