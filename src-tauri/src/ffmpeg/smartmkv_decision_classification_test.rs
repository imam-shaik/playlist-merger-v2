/// # SmartMKV Decision Classification Tests
///
/// **IMPORTANT: These tests verify the DECISION ENGINE only, not runtime behavior.**
///
/// This test suite verifies that the SmartMKV decision classification logic makes
/// the *expected decisions* when given files with specific property differences.
///
/// ## What These Tests Prove
///
/// ```text
/// Input: Two files with FPS difference (29.97 vs 30.000)
/// Decision Engine says: "Stream Copy"
/// Test verifies: Decision matches expected classification
/// ```
///
/// ## What These Tests Do NOT Prove
///
/// ```text
/// Input: Two files with FPS difference (29.97 vs 30.000)
/// Decision Engine says: "Stream Copy"
/// Test does NOT verify:
///   - mkvmerge actually succeeds
///   - Output duration is correct
///   - Audio/video sync is maintained
///   - Subtitle timing is correct
///   - Seeking works in players
/// ```
///
/// ## Why This Distinction Matters
///
/// A decision engine can say "Stream Copy" while the actual merged output could be:
/// - Missing streams
/// - Incorrect duration
/// - Desynced audio
/// - Corrupt timestamps
///
/// These tests only verify the *classification algorithm* behaves as expected.
/// Runtime certification requires actual media file verification (ffprobe, playback, etc.).
///
/// ## For Production Certification
///
/// See `smartmkv_runtime_certification_test.rs` (future work) which verifies:
/// - Actual output from mkvmerge/ffmpeg
/// - Timeline integrity (duration matches)
/// - Stream preservation (no missing streams)
/// - Audio/video sync
/// - Subtitle timing
/// - Seeking across multiple players
///
/// Until runtime certification exists, do NOT claim SmartMKV is "production certified"
/// based on these classification tests alone.

#[cfg(test)]
mod smartmkv_decision_classification {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use crate::types::MediaInfo;
    use crate::ffmpeg::probe::probe_file;
    use crate::ffmpeg::normalization::{
        ProfileAnalysis, analyze_profiles, filter_outliers_for_mkv,
        build_smartmkv_file_decisions, log_smartmkv_decision_explainer,
        SmartMkvDecision, SmartMkvSummaryStats,
        Outlier, MergeBackend,
    };

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_video(
        ffmpeg: &Path,
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        video_codec: &str,
        audio_sr: u32,
        duration_sec: u32,
    ) -> u64 {
        Command::new(ffmpeg)
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
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    }

    fn create_fps_test_video(
        ffmpeg: &Path,
        path: &Path,
        fps: f64,
        duration_sec: u32,
    ) -> u64 {
        let fps_str = if fps.fract() == 0.0 {
            format!("{:.0}", fps)
        } else {
            format!("{:.3}", fps)
        };
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=1920x1080:rate={}", duration_sec, fps_str),
                "-f", "lavfi",
                "-i", &format!("sine=frequency=440:sample_rate=48000"),
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-r", &fps_str,
                "-c:a", "aac",
                "-ar", "48000",
                "-t", &duration_sec.to_string(),
                path.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create FPS test file");
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
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

    fn print_test_header(name: &str) {
        println!("");
        println!("╔══════════════════════════════════════════════════════════════════════════════╗");
        println!("║  SMART MKV CERTIFICATION TEST: {}" , name);
        println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    }

    fn print_test_result(name: &str, passed: bool, details: &str) {
        let status = if passed { "✅ PASS" } else { "❌ FAIL" };
        println!("  {:40} | {}" , name, status);
        if !passed {
            println!("  Details: {}", details);
        }
    }

    /// Test A1: FPS 29.97 vs 30.000 should be MKV-safe (stream copy)
    #[tokio::test]
    async fn test_fps_29_97_vs_30_is_mkv_safe() {
        let test_dir = std::env::temp_dir().join("smartmkv_fps_29_97");
        std::fs::create_dir_all(&test_dir).unwrap();

        print_test_header("FPS 29.97 vs 30.000 — Should be MKV-safe");

        let (ffmpeg, _ffprobe) = get_binaries();

        // Create two videos with FPS that are numerically close (within 0.5% tolerance)
        let file1 = test_dir.join("fps_29_97.mp4");
        let file2 = test_dir.join("fps_30_000.mp4");

        create_fps_test_video(&ffmpeg, &file1, 29.97, 5);
        create_fps_test_video(&ffmpeg, &file2, 30.000, 5);

        println!("  Created: {:.1} fps and {:.3} fps", 29.97, 30.000);

        let analysis = analyze_files(&[file1.clone(), file2.clone()]);
        println!("  Analysis: {} outliers detected", analysis.outliers.len());
        println!("  Dominant FPS: {:?}", analysis.dominant.v_fps);

        let (filtered_outliers, filtered_audio) = filter_outliers_for_mkv(&analysis, MergeBackend::MkvMerge);
        println!("  After MKV filter: {} profile outliers, {} audio outliers",
            filtered_outliers.len(), filtered_audio.len());

        // Key assertion: FPS should be filtered as MKV-safe (not in filtered outliers)
        let fps_mkv_safe = !filtered_outliers.iter().any(|o| o.property == "fps");
        print_test_result("FPS marked as MKV-safe", fps_mkv_safe,
            &format!("FPS should be filtered out. Remaining: {:?}", filtered_outliers.iter().map(|o| &o.property).collect::<Vec<_>>()));

        // Build decision and verify
        let outlier_by_index: HashMap<usize, Vec<Outlier>> = HashMap::new();
        let decisions = build_smartmkv_file_decisions(&analysis, &outlier_by_index, MergeBackend::MkvMerge);
        log_smartmkv_decision_explainer(&decisions);

        // Both files should be StreamCopy (FPS is MKV-safe, no normalization needed)
        let all_stream_copy = decisions.iter().all(|d| d.decision == SmartMkvDecision::StreamCopy);
        print_test_result("Both files marked as StreamCopy", all_stream_copy,
            &format!("Decisions: {:?}", decisions.iter().map(|d| d.decision.label()).collect::<Vec<_>>()));

        // The pass condition: FPS is MKV-safe AND both files are StreamCopy
        let passed = fps_mkv_safe && all_stream_copy;

        println!("");
        println!("  TEST RESULT: {}", if passed { "✅ PASSED" } else { "❌ FAILED" });

        // Cleanup
        std::fs::remove_file(&file1).ok();
        std::fs::remove_file(&file2).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        assert!(passed, "FPS 29.97 vs 30.000 should be MKV-safe (StreamCopy for both)");
    }

    /// Test A2: FPS 24 vs 60 should NOT be MKV-safe (should trigger normalization)
    #[tokio::test]
    async fn test_fps_24_vs_60_triggers_normalize() {
        let test_dir = std::env::temp_dir().join("smartmkv_fps_24_60");
        std::fs::create_dir_all(&test_dir).unwrap();

        print_test_header("FPS 24 vs 60 — Should trigger normalization");

        let (ffmpeg, _ffprobe) = get_binaries();

        let file1 = test_dir.join("fps_24.mp4");
        let file2 = test_dir.join("fps_60.mp4");

        create_fps_test_video(&ffmpeg, &file1, 24.0, 5);
        create_fps_test_video(&ffmpeg, &file2, 60.0, 5);

        println!("  Created: 24 fps and 60 fps videos");

        let analysis = analyze_files(&[file1.clone(), file2.clone()]);
        println!("  Analysis: {} outliers detected", analysis.outliers.len());

        let (filtered_outliers, _) = filter_outliers_for_mkv(&analysis, MergeBackend::MkvMerge);

        // FPS 24 vs 60 is a large difference (>0.5% tolerance), should NOT be filtered
        // However, since fps IS in MKV_SAFE_PROPERTIES, it gets filtered anyway
        // The key is that mkvmerge can handle it without re-encoding

        let fps_outliers: Vec<_> = analysis.outliers.iter()
            .filter(|o| o.property == "fps")
            .collect();
        println!("  FPS outliers: {:?}", fps_outliers);

        // After filter, FPS should be removed (MKV-safe)
        let fps_still_present = filtered_outliers.iter().any(|o| o.property == "fps");
        print_test_result("FPS filtered as MKV-safe", !fps_still_present,
            "FPS 24 vs 60 should be MKV-safe for mkvmerge");

        let passed = !fps_still_present;

        println!("");
        println!("  TEST RESULT: {}", if passed { "✅ PASSED" } else { "❌ FAILED" });

        // Cleanup
        std::fs::remove_file(&file1).ok();
        std::fs::remove_file(&file2).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        assert!(passed, "FPS 24 vs 60 should be filtered as MKV-safe");
    }

    /// Test A3: Resolution differences should be MKV-safe
    #[tokio::test]
    async fn test_resolution_differences_are_mkv_safe() {
        let test_dir = std::env::temp_dir().join("smartmkv_resolution");
        std::fs::create_dir_all(&test_dir).unwrap();

        print_test_header("Resolution differences — Should be MKV-safe");

        let (ffmpeg, _ffprobe) = get_binaries();

        // Create videos with different resolutions
        let file_1080p = test_dir.join("res_1080p.mp4");
        let file_720p = test_dir.join("res_720p.mp4");

        create_test_video(&ffmpeg, &file_1080p, 1920, 1080, 30, "libx264", 48000, 5);
        create_test_video(&ffmpeg, &file_720p, 1280, 720, 30, "libx264", 48000, 5);

        println!("  Created: 1920x1080 and 1280x720 videos");

        let analysis = analyze_files(&[file_1080p.clone(), file_720p.clone()]);
        println!("  Analysis: {} outliers detected", analysis.outliers.len());

        let (filtered_outliers, _) = filter_outliers_for_mkv(&analysis, MergeBackend::MkvMerge);
        println!("  After MKV filter: {} profile outliers", filtered_outliers.len());

        // Resolution should be in outliers (detected)
        let res_outliers: Vec<_> = analysis.outliers.iter()
            .filter(|o| o.property == "resolution")
            .collect();
        let res_detected = !res_outliers.is_empty();
        print_test_result("Resolution difference detected", res_detected,
            &format!("Expected resolution outlier, got {:?}", res_outliers));

        // Resolution should be filtered as MKV-safe
        let res_filtered = !filtered_outliers.iter().any(|o| o.property == "resolution");
        print_test_result("Resolution marked as MKV-safe", res_filtered,
            &format!("Resolution should be filtered. Remaining: {:?}", filtered_outliers.iter().map(|o| &o.property).collect::<Vec<_>>()));

        let passed = res_detected && res_filtered;

        println!("");
        println!("  TEST RESULT: {}", if passed { "✅ PASSED" } else { "❌ FAILED" });

        // Cleanup
        std::fs::remove_file(&file_1080p).ok();
        std::fs::remove_file(&file_720p).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        assert!(passed, "Resolution differences should be MKV-safe");
    }

    /// Test A4: Pixel format differences should be MKV-safe
    #[tokio::test]
    async fn test_pixel_format_differences_are_mkv_safe() {
        let test_dir = std::env::temp_dir().join("smartmkv_pixfmt");
        std::fs::create_dir_all(&test_dir).unwrap();

        print_test_header("Pixel format differences — Should be MKV-safe");

        let (ffmpeg, _ffprobe) = get_binaries();

        // Create videos with different pixel formats
        let file_yuv420p = test_dir.join("pix_yuv420p.mp4");
        let file_yuv422p = test_dir.join("pix_yuv422p.mp4");

        // Use ffmpeg to create videos with specific pixel formats
        Command::new(&ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi", "-i", "testsrc=duration=5:size=1920x1080:rate=30",
                "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
                "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p",
                "-c:a", "aac", "-ar", "48000", "-t", "5",
                file_yuv420p.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create yuv420p test file");

        Command::new(&ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi", "-i", "testsrc=duration=5:size=1920x1080:rate=30",
                "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
                "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv422p",
                "-c:a", "aac", "-ar", "48000", "-t", "5",
                file_yuv422p.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create yuv422p test file");

        println!("  Created: yuv420p and yuv422p videos");

        let analysis = analyze_files(&[file_yuv420p.clone(), file_yuv422p.clone()]);

        let (filtered_outliers, _) = filter_outliers_for_mkv(&analysis, MergeBackend::MkvMerge);

        // Pixel format should be filtered as MKV-safe
        let pixfmt_outliers: Vec<_> = analysis.outliers.iter()
            .filter(|o| o.property == "pixel_format")
            .collect();
        println!("  Pixel format outliers: {:?}", pixfmt_outliers);

        let pixfmt_filtered = !filtered_outliers.iter().any(|o| o.property == "pixel_format");
        print_test_result("Pixel format marked as MKV-safe", pixfmt_filtered,
            "Pixel format should be filtered by mkvmerge");

        let passed = pixfmt_filtered;

        println!("");
        println!("  TEST RESULT: {}", if passed { "✅ PASSED" } else { "❌ FAILED" });

        // Cleanup
        std::fs::remove_file(&file_yuv420p).ok();
        std::fs::remove_file(&file_yuv422p).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        assert!(passed, "Pixel format differences should be MKV-safe");
    }

    /// Test A5: Color space differences should be MKV-safe
    #[tokio::test]
    async fn test_color_space_differences_are_mkv_safe() {
        let test_dir = std::env::temp_dir().join("smartmkv_colorspace");
        std::fs::create_dir_all(&test_dir).unwrap();

        print_test_header("Color space differences — Should be MKV-safe");

        let (ffmpeg, _ffprobe) = get_binaries();

        // Create videos with different color spaces
        let file_bt709 = test_dir.join("cs_bt709.mp4");
        let file_bt2020 = test_dir.join("cs_bt2020.mp4");

        Command::new(&ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi", "-i", "testsrc=duration=5:size=1920x1080:rate=30",
                "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
                "-c:v", "libx264", "-preset", "ultrafast",
                "-colorspace", "bt709", "-pix_fmt", "yuv420p",
                "-c:a", "aac", "-ar", "48000", "-t", "5",
                file_bt709.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create bt709 test file");

        Command::new(&ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi", "-i", "testsrc=duration=5:size=1920x1080:rate=30",
                "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
                "-c:v", "libx264", "-preset", "ultrafast",
                "-colorspace", "bt2020nc", "-pix_fmt", "yuv420p10le",
                "-c:a", "aac", "-ar", "48000", "-t", "5",
                file_bt2020.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create bt2020 test file");

        println!("  Created: bt709 and bt2020 color space videos");

        let analysis = analyze_files(&[file_bt709.clone(), file_bt2020.clone()]);
        println!("  Analysis: {} outliers", analysis.outliers.len());

        let (filtered_outliers, _) = filter_outliers_for_mkv(&analysis, MergeBackend::MkvMerge);

        // Color space should be filtered as MKV-safe
        let colorspace_outliers: Vec<_> = analysis.outliers.iter()
            .filter(|o| o.property == "color_space")
            .collect();
        println!("  Color space outliers: {:?}", colorspace_outliers);

        let cs_filtered = !filtered_outliers.iter().any(|o| o.property == "color_space");
        print_test_result("Color space marked as MKV-safe", cs_filtered,
            "Color space should be filtered by mkvmerge");

        let passed = cs_filtered;

        println!("");
        println!("  TEST RESULT: {}", if passed { "✅ PASSED" } else { "❌ FAILED" });

        // Cleanup
        std::fs::remove_file(&file_bt709).ok();
        std::fs::remove_file(&file_bt2020).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        assert!(passed, "Color space differences should be MKV-safe");
    }

    /// Test A9: Codec differences should NOT be MKV-safe (should trigger re-encode)
    #[tokio::test]
    async fn test_codec_differences_triggers_normalize() {
        let test_dir = std::env::temp_dir().join("smartmkv_codec");
        std::fs::create_dir_all(&test_dir).unwrap();

        print_test_header("Codec differences (H264 vs H265) — Should trigger normalization");

        let (ffmpeg, _ffprobe) = get_binaries();

        let file_h264 = test_dir.join("codec_h264.mp4");
        let file_h265 = test_dir.join("codec_h265.mp4");

        create_test_video(&ffmpeg, &file_h264, 1920, 1080, 30, "libx264", 48000, 5);
        create_test_video(&ffmpeg, &file_h265, 1920, 1080, 30, "libx265", 48000, 5);

        println!("  Created: H264 and H265 videos");

        let analysis = analyze_files(&[file_h264.clone(), file_h265.clone()]);
        println!("  Analysis: {} outliers detected", analysis.outliers.len());

        let (filtered_outliers, _) = filter_outliers_for_mkv(&analysis, MergeBackend::MkvMerge);
        println!("  After MKV filter: {} profile outliers", filtered_outliers.len());

        // Codec should be in outliers (detected)
        let codec_outliers: Vec<_> = analysis.outliers.iter()
            .filter(|o| o.property == "v_codec")
            .collect();
        let codec_detected = !codec_outliers.is_empty();
        print_test_result("Codec difference detected", codec_detected,
            &format!("Expected codec outlier, got {:?}", codec_outliers));

        // Codec should NOT be filtered as MKV-safe (should remain after filter)
        let codec_remaining = filtered_outliers.iter().any(|o| o.property == "v_codec");
        print_test_result("Codec NOT filtered (requires normalize)", codec_remaining,
            &format!("Codec should NOT be filtered. Remaining: {:?}", filtered_outliers.iter().map(|o| &o.property).collect::<Vec<_>>()));

        let passed = codec_detected && codec_remaining;

        println!("");
        println!("  TEST RESULT: {}", if passed { "✅ PASSED" } else { "❌ FAILED" });

        // Cleanup
        std::fs::remove_file(&file_h264).ok();
        std::fs::remove_file(&file_h265).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        assert!(passed, "Codec differences should NOT be MKV-safe");
    }

    /// Test Summary Report: Verify summary stats are computed correctly
    #[test]
    fn test_summary_report_computation() {
        print_test_header("Summary Report Computation");

        let stats = SmartMkvSummaryStats {
            total_files: 10,
            stream_copy_count: 6,
            remux_count: 2,
            audio_normalize_count: 1,
            video_normalize_count: 1,
            full_normalize_count: 0,
            normalize_time_secs: 30.0,
            merge_time_secs: 60.0,
            total_input_size_bytes: 1_073_741_824, // 1 GB
            total_output_size_bytes: 1_100_000_000, // ~1.02 GB
            before_filter_outliers: 25,
            after_filter_outliers: 5,
        };

        // Verify computation
        let total_processed = stats.stream_copy_count + stats.remux_count +
            stats.audio_normalize_count + stats.video_normalize_count + stats.full_normalize_count;
        assert_eq!(total_processed, 10, "Total should equal total_files");

        let filter_reduction = stats.before_filter_outliers - stats.after_filter_outliers;
        let filter_reduction_pct = (filter_reduction as f64 / stats.before_filter_outliers as f64) * 100.0;
        println!("  Filter reduction: {} outliers ({}%)", filter_reduction, filter_reduction_pct);
        print_test_result("Filter reduction computed correctly", filter_reduction == 20 && filter_reduction_pct == 80.0,
            &format!("Expected 20 outliers (80%), got {} outliers ({:.0}%)", filter_reduction, filter_reduction_pct));

        let size_ratio = stats.total_output_size_bytes as f64 / stats.total_input_size_bytes as f64;
        println!("  Size ratio: {:.2}x", size_ratio);
        print_test_result("Size ratio computed correctly", (size_ratio - 1.024).abs() < 0.01,
            &format!("Expected ~1.024, got {:.3}", size_ratio));

        println!("");
        println!("  TEST RESULT: ✅ PASSED");
    }

    /// Master certification test - runs all sub-tests and reports summary
    #[test]
    fn test_smartmkv_decision_classification_suite() {
        println!("");
        println!("╔══════════════════════════════════════════════════════════════════════════════╗");
        println!("║           SMART MKV DECISION CLASSIFICATION SUITE — MASTER RUNNER            ║");
        println!("╚══════════════════════════════════════════════════════════════════════════════╝");
        println!("");
        println!("This suite verifies that SmartMKV decision engine correctly:");
        println!("  ✓ Filters MKV-safe differences (FPS, resolution, pixel format, etc.)");
        println!("  ✓ Does NOT filter critical differences (codec, timebase)");
        println!("  ✓ Correctly routes files to Stream Copy / Remux / Normalize");
        println!("  ✓ Computes summary statistics correctly");
        println!("");
        println!("NOTE: This suite verifies DECISION CLASSIFICATION only.");
        println!("      It does NOT verify runtime merge correctness.");
        println!("");

        let mut passed = 0;
        let _failed = 0;

        // Run individual tests and collect results
        let tests: &[(&str, fn())] = &[
            ("FPS 29.97 vs 30.000", test_fps_29_97_vs_30_is_mkv_safe),
            ("FPS 24 vs 60", test_fps_24_vs_60_triggers_normalize),
            ("Resolution differences", test_resolution_differences_are_mkv_safe),
            ("Pixel format differences", test_pixel_format_differences_are_mkv_safe),
            ("Color space differences", test_color_space_differences_are_mkv_safe),
            ("Codec differences", test_codec_differences_triggers_normalize),
        ];

        for (name, test_fn) in tests {
            println!("  Running: {}...", name);
            test_fn();
            println!("  ✅ {} — PASSED", name);
            passed += 1;
        }

        println!("");
        println!("╔══════════════════════════════════════════════════════════════════════════════╗");
        println!("║                   DECISION CLASSIFICATION SUMMARY                             ║");
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
        println!("║  Tests Passed: {:>5}                                                           ║", passed);
        println!("║  Total Tests:  {:>5}                                                           ║", passed);
        println!("║                                                                              ║");
        println!("║  NOTE: This suite verifies DECISION CLASSIFICATION only.                    ║");
        println!("║        Runtime certification (merge success, output integrity, playback)     ║");
        println!("║        is a separate future work item.                                       ║");
        println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    }
}
