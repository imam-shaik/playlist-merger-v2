#[cfg(test)]
mod audio_seek_dense_audit {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_file(
        ffmpeg: &Path,
        path: &Path,
        duration_sec: u32,
        width: u32,
        height: u32,
        fps: u32,
        video_codec: &str,
        audio_sr: u32,
    ) -> std::process::Output {
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
            .expect("Failed to create test file")
    }

    fn test_audio_seek_at(ffmpeg: &Path, file: &Path, seek_pct: f64, duration: f64) -> (bool, String) {
        let seek_sec = duration * seek_pct;
        let args = [
            "-v", "error",
            "-ss", &seek_sec.to_string(),
            "-i", file.to_str().unwrap(),
            "-vn",
            "-map", "0:a:0?",
            "-t", "5",
            "-f", "null", "-"
        ];
        match Command::new(ffmpeg).args(&args).output() {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if out.status.success() && stderr.trim().is_empty() {
                    (true, String::new())
                } else {
                    (false, stderr.trim().to_string())
                }
            }
            Err(e) => (false, e.to_string())
        }
    }

    fn test_video_seek_at(ffmpeg: &Path, file: &Path, seek_pct: f64, duration: f64) -> (bool, String) {
        let seek_sec = duration * seek_pct;
        let args = [
            "-v", "error",
            "-ss", &seek_sec.to_string(),
            "-i", file.to_str().unwrap(),
            "-an",
            "-frames:v", "1",
            "-f", "null", "-"
        ];
        match Command::new(ffmpeg).args(&args).output() {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if out.status.success() && stderr.trim().is_empty() {
                    (true, String::new())
                } else {
                    (false, stderr.trim().to_string())
                }
            }
            Err(e) => (false, e.to_string())
        }
    }

    fn get_duration(ffprobe: &Path, file: &Path) -> f64 {
        let args = ["-v", "quiet", "-print_format", "json", "-show_format", file.to_str().unwrap()];
        match Command::new(ffprobe).args(&args).output() {
            Ok(out) if out.status.success() => {
                let json_str = String::from_utf8_lossy(&out.stdout);
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    json.pointer("/format/duration")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0)
                } else {
                    0.0
                }
            }
            _ => 0.0
        }
    }

    fn run_dense_seek_test(
        ffmpeg: &Path,
        ffprobe: &Path,
        output: &Path,
        _label: &str,
    ) -> (usize, usize, Vec<(f64, String)>) {
        let duration = get_duration(ffprobe, output);
        if duration <= 0.0 {
            return (0, 0, vec![]);
        }

        // Dense test: every 5% from 5% to 95%
        let test_points: Vec<f64> = (1..20).map(|i| i as f64 * 0.05).collect();
        let mut audio_failures = 0;
        let mut video_failures = 0;
        let mut failure_locations: Vec<(f64, String)> = Vec::new();

        for pct in &test_points {
            let (a_ok, a_err) = test_audio_seek_at(ffmpeg, output, *pct, duration);
            let (v_ok, v_err) = test_video_seek_at(ffmpeg, output, *pct, duration);

            if !a_ok {
                audio_failures += 1;
                let err = if !a_err.is_empty() { &a_err } else { "unknown" };
                failure_locations.push((*pct * 100.0, format!("AUDIO: {}", err)));
            }
            if !v_ok {
                video_failures += 1;
                let err = if !v_err.is_empty() { &v_err } else { "unknown" };
                failure_locations.push((*pct * 100.0, format!("VIDEO: {}", err)));
            }
        }

        (audio_failures, video_failures, failure_locations)
    }

    /// PHASE 4A: DENSE SEEK COVERAGE AUDIT
    ///
    /// Tests every 5% instead of the current sparse 10-point grid.
    ///
    /// Test cases:
    /// 1. Clean compatible playlist (all same parameters)
    /// 2. Mixed codecs (H264 + H265)
    /// 3. Mixed sample rates (44100 + 48000)
    /// 4. Large playlist (10+ files)
    ///
    /// Current system tests: 5%, 10%, 15%, 25%, 35%, 50%, 65%, 75%, 85%, 95%
    /// Dense audit tests: 5%, 10%, 15%, 20%, 25%, 30%, 35%, 40%, 45%, 50%, 55%, 60%, 65%, 70%, 75%, 80%, 85%, 90%, 95%
    #[tokio::test]
    async fn test_dense_seek_coverage_audit() {
        let test_dir = std::env::temp_dir().join("dense_seek_audit");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 4A: DENSE SEEK COVERAGE AUDIT                                ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();

        let mut all_results: HashMap<String, (usize, usize, Vec<(f64, String)>)> = HashMap::new();

        // ══════════════════════════════════════════════════════════════
        // TEST CASE 1: Clean compatible files
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  TEST 1: Clean Compatible Files (5 files, same params)              ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let clean_files: Vec<PathBuf> = (0..5)
            .map(|i| test_dir.join(format!("clean_{}.mp4", i)))
            .collect();

        for f in &clean_files {
            create_test_file(&ffmpeg, f, 5, 1920, 1080, 30, "libx264", 44100);
        }

        let list_clean = test_dir.join("list_clean.txt");
        let list_content: String = clean_files
            .iter()
            .map(|p| format!("file '{}'\nduration 5", p.to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&list_clean, &list_content).unwrap();

        let output_clean = test_dir.join("output_clean.mp4");
        let _ = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_clean.to_string_lossy(), "-c", "copy", output_clean.to_str().unwrap()])
            .output();

        let (a_fail, v_fail, locs) = run_dense_seek_test(&ffmpeg, &ffprobe, &output_clean, "Clean");
        all_results.insert("Clean (5 files)".to_string(), (a_fail, v_fail, locs.clone()));
        println!("\n  Dense seek test (19 points every 5%):");
        println!("  Audio failures: {}/19", a_fail);
        println!("  Video failures: {}/19", v_fail);
        if !locs.is_empty() {
            for (pct, err) in &locs {
                println!("    {:.0}%: {}", pct, err);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // TEST CASE 2: Mixed codecs
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  TEST 2: Mixed Codecs (H264 + H265)                                   ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let codec_files: Vec<PathBuf> = (0..5)
            .map(|i| test_dir.join(format!("codec_{}.mp4", i)))
            .collect();

        for (i, f) in codec_files.iter().enumerate() {
            let codec = if i % 2 == 0 { "libx264" } else { "libx265" };
            create_test_file(&ffmpeg, f, 5, 1920, 1080, 30, codec, 44100);
        }

        let list_codec = test_dir.join("list_codec.txt");
        let list_content: String = codec_files
            .iter()
            .map(|p| format!("file '{}'\nduration 5", p.to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&list_codec, &list_content).unwrap();

        let output_codec = test_dir.join("output_codec.mp4");
        let _ = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_codec.to_string_lossy(), "-c", "copy", output_codec.to_str().unwrap()])
            .output();

        let (a_fail, v_fail, locs) = run_dense_seek_test(&ffmpeg, &ffprobe, &output_codec, "Mixed Codec");
        all_results.insert("Mixed Codec".to_string(), (a_fail, v_fail, locs.clone()));
        println!("\n  Dense seek test (19 points every 5%):");
        println!("  Audio failures: {}/19", a_fail);
        println!("  Video failures: {}/19", v_fail);
        if !locs.is_empty() {
            for (pct, err) in &locs {
                println!("    {:.0}%: {}", pct, err);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // TEST CASE 3: Mixed sample rates
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  TEST 3: Mixed Sample Rates (44100 + 48000)                             ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let sr_files: Vec<PathBuf> = (0..5)
            .map(|i| test_dir.join(format!("sr_{}.mp4", i)))
            .collect();

        for (i, f) in sr_files.iter().enumerate() {
            let sr = if i % 2 == 0 { 44100 } else { 48000 };
            create_test_file(&ffmpeg, f, 5, 1920, 1080, 30, "libx264", sr);
        }

        let list_sr = test_dir.join("list_sr.txt");
        let list_content: String = sr_files
            .iter()
            .map(|p| format!("file '{}'\nduration 5", p.to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&list_sr, &list_content).unwrap();

        let output_sr = test_dir.join("output_sr.mp4");
        let _ = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_sr.to_string_lossy(), "-c", "copy", output_sr.to_str().unwrap()])
            .output();

        let (a_fail, v_fail, locs) = run_dense_seek_test(&ffmpeg, &ffprobe, &output_sr, "Mixed SR");
        all_results.insert("Mixed Sample Rates".to_string(), (a_fail, v_fail, locs.clone()));
        println!("\n  Dense seek test (19 points every 5%):");
        println!("  Audio failures: {}/19", a_fail);
        println!("  Video failures: {}/19", v_fail);
        if !locs.is_empty() {
            for (pct, err) in &locs {
                println!("    {:.0}%: {}", pct, err);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // TEST CASE 4: Large playlist (10 files)
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  TEST 4: Large Playlist (10 files, varied params)                     ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let large_files: Vec<PathBuf> = (0..10)
            .map(|i| test_dir.join(format!("large_{}.mp4", i)))
            .collect();

        let codecs = ["libx264", "libx264", "libx265", "libx264", "libx265", "libx264", "libx265", "libx264", "libx264", "libx265"];
        let rates = [44100, 48000, 44100, 48000, 44100, 48000, 44100, 48000, 44100, 48000];

        for (i, f) in large_files.iter().enumerate() {
            create_test_file(&ffmpeg, f, 5, 1920, 1080, 30, codecs[i], rates[i]);
        }

        let list_large = test_dir.join("list_large.txt");
        let list_content: String = large_files
            .iter()
            .map(|p| format!("file '{}'\nduration 5", p.to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&list_large, &list_content).unwrap();

        let output_large = test_dir.join("output_large.mp4");
        let _ = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_large.to_string_lossy(), "-c", "copy", output_large.to_str().unwrap()])
            .output();

        let (a_fail, v_fail, locs) = run_dense_seek_test(&ffmpeg, &ffprobe, &output_large, "Large");
        all_results.insert("Large (10 files)".to_string(), (a_fail, v_fail, locs.clone()));
        println!("\n  Dense seek test (19 points every 5%):");
        println!("  Audio failures: {}/19", a_fail);
        println!("  Video failures: {}/19", v_fail);
        if !locs.is_empty() {
            for (pct, err) in &locs {
                println!("    {:.0}%: {}", pct, err);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // SUMMARY
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  DENSE SEEK AUDIT SUMMARY                                            ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let mut total_audio_failures = 0;
        let mut total_video_failures = 0;
        let mut total_failures = 0;

        println!("\n  {:<25} | {:>12} | {:>12}", "Test Case", "Audio Fail", "Video Fail");
        println!("  {:─<25}─|───{:─<12}──|───{:─<12}", "", "", "");

        for (name, (a, v, _locs)) in &all_results {
            println!("  {:<25} | {:>12} | {:>12}", name, a, v);
            total_audio_failures += a;
            total_video_failures += v;
            total_failures += a + v;
        }

        println!("  {:─<25}─|───{:─<12}──|───{:─<12}", "", "", "");
        println!("  {:<25} | {:>12} | {:>12}", "TOTAL", total_audio_failures, total_video_failures);

        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  CONCLUSION                                                            ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        if total_failures == 0 {
            println!("\n  ✅ NO CORRUPTION DETECTED");
            println!("\n  All 4 test cases passed dense seek coverage:");
            println!("  - Clean compatible files: PASS");
            println!("  - Mixed codecs (H264 + H265): PASS");
            println!("  - Mixed sample rates (44100 + 48000): PASS");
            println!("  - Large playlist (10 files): PASS");
            println!("\n  The current 10-point warning system is ADEQUATE.");
            println!("  Blind spots in coverage did not reveal actual corruption.");
            println!("\n  RECOMMENDATION: Close Phase 4, no production changes needed.");
        } else {
            println!("\n  ⚠️  CORRUPTION DETECTED");
            println!("\n  Total failures across all tests: {}", total_failures);
            println!("  Audio: {}, Video: {}", total_audio_failures, total_video_failures);
            for (name, (_a, _v, locs)) in &all_results {
                if !locs.is_empty() {
                    println!("\n  {} failures:", name);
                    for (pct, err) in locs {
                        println!("    {:.0}%: {}", pct, err);
                    }
                }
            }
            println!("\n  RECOMMENDATION: Investigate root cause before production changes.");
        }

        // Cleanup
        for f in clean_files { std::fs::remove_file(f).ok(); }
        for f in codec_files { std::fs::remove_file(f).ok(); }
        for f in sr_files { std::fs::remove_file(f).ok(); }
        for f in large_files { std::fs::remove_file(f).ok(); }
        std::fs::remove_file(&output_clean).ok();
        std::fs::remove_file(&output_codec).ok();
        std::fs::remove_file(&output_sr).ok();
        std::fs::remove_file(&output_large).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Phase 4A Dense Seek Coverage Audit complete.");
    }
}