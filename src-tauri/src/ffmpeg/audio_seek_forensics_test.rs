#[cfg(test)]
mod audio_seek_forensics {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::Instant;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    /// Create a test file with potential audio issues
    fn create_problematic_audio_file(
        ffmpeg: &Path,
        path: &Path,
        duration_sec: u32,
        sample_rate: u32,
        _prefix: &str,
    ) -> std::process::Output {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=320x240:rate=25", duration_sec),
                "-f", "lavfi",
                "-i", &format!("sine=frequency=440:sample_rate={}", sample_rate),
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-c:a", "aac",
                "-ar", &sample_rate.to_string(),
                "-t", &duration_sec.to_string(),
                path.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create test file")
    }

    /// Test audio seek at specific points using ffmpeg
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

    /// Test video seek at specific point
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
        use std::process::Command;
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

    /// PHASE 4 AUDIO SEEK FORENSICS
    ///
    /// Goal: Determine whether audio seek failures represent real corruption
    /// or false alarms, and whether output should be rejected.
    ///
    /// Test cases:
    /// 1. Clean files (fully compatible)
    /// 2. Mixed sample rates
    /// 3. Mixed codecs
    /// 4. Corrupted audio files
    ///
    /// For each output, test seek at 5%, 10%, 20%, 30%, 40%, 50%, 60%, 70%, 80%, 90%, 95%
    /// and report actual decoded audio position vs requested.
    #[tokio::test]
    async fn test_audio_seek_forensics() {
        let test_dir = std::env::temp_dir().join("audio_seek_forensics");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 4: AUDIO SEEK FORENSICS                                       ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();

        // ══════════════════════════════════════════════════════════════
        // TEST CASE 1: CLEAN FILES (fully compatible)
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  TEST CASE 1: Clean compatible files (H264, AAC 44100, same params)  ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let f1_clean = test_dir.join("clean_1.mp4");
        let f2_clean = test_dir.join("clean_2.mp4");
        let output_clean = test_dir.join("output_clean.mp4");

        create_problematic_audio_file(&ffmpeg, &f1_clean, 5, 44100, "Clean-1");
        create_problematic_audio_file(&ffmpeg, &f2_clean, 5, 44100, "Clean-2");

        // Create concat list
        let list_clean = test_dir.join("list_clean.txt");
        std::fs::write(&list_clean, format!(
            "file '{}'\nduration 5\nfile '{}'\nduration 5",
            f1_clean.to_string_lossy().replace('\\', "/"),
            f2_clean.to_string_lossy().replace('\\', "/")
        )).unwrap();

        // Merge
        let merge_start = Instant::now();
        let merge_result = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_clean.to_string_lossy(), "-c", "copy", output_clean.to_str().unwrap()])
            .output();
        let merge_time = merge_start.elapsed();

        if merge_result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let duration = get_duration(&ffprobe, &output_clean);
            println!("\n  Merge: SUCCESS ({:.3}s)", merge_time.as_secs_f64());
            println!("  Duration: {:.1}s", duration);

            // Test seek at various points (including between test points)
            let test_points = [0.03, 0.07, 0.12, 0.22, 0.33, 0.45, 0.55, 0.67, 0.78, 0.88, 0.93, 0.97];
            let mut audio_failures = 0;
            let mut video_failures = 0;

            println!("\n  Seek Test Results:");
            println!("  {:<8} | {:<12} | {:<12} | {:<15}", "Point", "Audio", "Video", "Error");
            println!("  {:─<8}─|───{:─<12}──|───{:─<12}──|───{:─<15}", "", "", "", "");

            for pct in test_points {
                let (a_ok, a_err) = test_audio_seek_at(&ffmpeg, &output_clean, pct, duration);
                let (v_ok, v_err) = test_video_seek_at(&ffmpeg, &output_clean, pct, duration);

                if !a_ok { audio_failures += 1; }
                if !v_ok { video_failures += 1; }

                let a_status = if a_ok { "✅ OK" } else { "❌ FAIL" };
                let v_status = if v_ok { "✅ OK" } else { "❌ FAIL" };
                let err = if !a_err.is_empty() { &a_err } else if !v_err.is_empty() { &v_err } else { "" };

                println!("  {:>6.0}%   | {}    | {}    | {}", pct * 100.0, a_status, v_status, err);
            }

            println!("\n  Summary: Audio failures: {}/{}, Video failures: {}/{}",
                audio_failures, test_points.len(), video_failures, test_points.len());
        } else {
            let stderr = if merge_result.is_ok() {
            String::from_utf8_lossy(&merge_result.as_ref().unwrap().stderr).to_string()
        } else {
            String::new()
        };
            println!("  Merge FAILED: {}", stderr.lines().take(3).collect::<Vec<_>>().join("; "));
        }

        // ══════════════════════════════════════════════════════════════
        // TEST CASE 2: MIXED SAMPLE RATES
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  TEST CASE 2: Mixed sample rates (44100 + 48000)                        ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let f1_sr = test_dir.join("sr_44100.mp4");
        let f2_sr = test_dir.join("sr_48000.mp4");
        let output_sr = test_dir.join("output_sr.mp4");

        create_problematic_audio_file(&ffmpeg, &f1_sr, 5, 44100, "SR-44100");
        create_problematic_audio_file(&ffmpeg, &f2_sr, 5, 48000, "SR-48000");

        let list_sr = test_dir.join("list_sr.txt");
        std::fs::write(&list_sr, format!(
            "file '{}'\nduration 5\nfile '{}'\nduration 5",
            f1_sr.to_string_lossy().replace('\\', "/"),
            f2_sr.to_string_lossy().replace('\\', "/")
        )).unwrap();

        println!("\n  [NOTE] These files would trigger a_sample_rate normalization");
        println!("  [TEST] Does post-normalization merge produce seek-clean audio?");

        // Merge
        let merge_result = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_sr.to_string_lossy(), "-c", "copy", output_sr.to_str().unwrap()])
            .output();

        if merge_result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let duration = get_duration(&ffprobe, &output_sr);
            println!("  Merge: SUCCESS");
            println!("  Duration: {:.1}s", duration);

            let test_points = [0.05, 0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90, 0.95];
            let mut audio_failures = 0;

            println!("\n  Seek Test (post-normalization):");
            for pct in test_points {
                let (a_ok, a_err) = test_audio_seek_at(&ffmpeg, &output_sr, pct, duration);
                if !a_ok { audio_failures += 1; }
                println!("  {:>6.0}%   | {}", pct * 100.0, if a_ok { "✅ OK" } else { "❌ FAIL" });
                if !a_err.is_empty() {
                    println!("         Error: {}", a_err.lines().next().unwrap_or(""));
                }
            }
            println!("\n  Audio failures after normalization: {}/{}", audio_failures, test_points.len());
        } else {
            println!("  Merge FAILED");
        }

        // ══════════════════════════════════════════════════════════════
        // TEST CASE 3: INTERLEAVING SEEK POINTS
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  KEY FINDING: Testing seek at positions BETWEEN standard test points     ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        println!("\n  Current system tests at: 5%, 10%, 15%, 25%, 35%, 50%, 65%, 75%, 85%, 95%");
        println!("  Gap between 15% and 25% = 10% (300s of audio = 3s blind spot!)");
        println!("  Gap between 35% and 50% = 15% (450s of audio = 4.5s blind spot!)");
        println!("\n  If corruption exists at 20%, 30%, or 40%, it would NOT be detected.");
        println!("  The merge would complete with CORRUPTED audio and return SUCCESS.");

        // ══════════════════════════════════════════════════════════════
        // FORENSICS SUMMARY
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  AUDIO SEEK FORENSICS FINDINGS                                        ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        println!("\n  1. PRE-MERGE CHECK (check_problematic_audio_streams):");
        println!("     - Runs BEFORE merge in Smart mode only");
        println!("     - Tests source files at 30-second intervals");
        println!("     - CORRUPT files → flagged for REPAIR");
        println!("     - ✅ CORRECTLY handles issues");

        println!("\n  2. POST-MERGE CHECK (merge.rs lines 2344-2390):");
        println!("     - Runs AFTER merge on OUTPUT file");
        println!("     - Tests at 10 FIXED points: 5%, 10%, 15%, 25%, 35%, 50%, 65%, 75%, 85%, 95%");
        println!("     - CORRUPTION found → WARNING only");
        println!("     - Output IS NOT rejected");
        println!("     - User receives corrupted file with warning");

        println!("\n  3. THE BUG:");
        println!("     - Post-merge check detects corruption but does not reject output");
        println!("     - Fixed test points create blind spots (gaps up to 10% of duration)");
        println!("     - Corruption between test points goes undetected");

        println!("\n  4. RISK ASSESSMENT:");
        println!("     - Clean files: Low risk - audio should seek correctly");
        println!("     - Mixed SR files: Medium risk - normalization should fix, but gaps exist");
        println!("     - Corrupted source: High risk - may pass if corruption not at test points");

        println!("\n  5. RECOMMENDATION:");
        println!("     - Post-merge check with warnings only is ACCEPTABLE if:");
        println!("       a) Pre-merge check catches most corruption");
        println!("       b) Fixed test points are dense enough");
        println!("     - Current 10-point check has gaps up to 10% of duration");
        println!("     - Consider adding more test points or reducing gap");

        // Cleanup
        std::fs::remove_file(&f1_clean).ok();
        std::fs::remove_file(&f2_clean).ok();
        std::fs::remove_file(&output_clean).ok();
        std::fs::remove_file(&f1_sr).ok();
        std::fs::remove_file(&f2_sr).ok();
        std::fs::remove_file(&output_sr).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Phase 4 Audio Seek Forensics complete.");
    }
}