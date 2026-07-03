#[cfg(test)]
mod boundary_isolation_test {
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
        video_codec: &str,
        audio_sr: u32,
    ) -> (bool, String) {
        let args = &[
            "-y",
            "-f", "lavfi",
            "-i", &format!("testsrc=duration={}:size=1920x1080:rate=30", duration_sec),
            "-f", "lavfi",
            "-i", &format!("sine=frequency=440:sample_rate={}", audio_sr),
            "-c:v", video_codec,
            "-preset", "ultrafast",
            "-c:a", "aac",
            "-ar", &audio_sr.to_string(),
            "-t", &duration_sec.to_string(),
            path.to_str().unwrap()
        ];
        match Command::new(ffmpeg).args(args).output() {
            Ok(out) => {
                if out.status.success() {
                    (true, String::new())
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    (false, stderr.to_string())
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
                } else { 0.0 }
            }
            _ => 0.0
        }
    }

    fn test_audio_at_seek(
        ffmpeg: &Path,
        file: &Path,
        seek_pct: f64,
        duration: f64,
    ) -> (bool, String) {
        let seek_sec = duration * seek_pct;
        let args = [
            "-v", "error",
            "-ss", &seek_sec.to_string(),
            "-i", file.to_str().unwrap(),
            "-vn",
            "-map", "0:a:0?",
            "-t", "3",
            "-f", "null", "-"
        ];
        match Command::new(ffmpeg).args(&args).output() {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if out.status.success() && stderr.trim().is_empty() {
                    (true, String::new())
                } else {
                    (false, stderr.to_string())
                }
            }
            Err(e) => (false, e.to_string())
        }
    }

    fn test_video_at_seek(
        ffmpeg: &Path,
        file: &Path,
        seek_pct: f64,
        duration: f64,
    ) -> (bool, String) {
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
                    (false, stderr.to_string())
                }
            }
            Err(e) => (false, e.to_string())
        }
    }

    fn run_concat_test(
        ffmpeg: &Path,
        ffprobe: &Path,
        file1: &Path,
        file2: &Path,
        label: &str,
    ) -> (bool, String, f64) {
        let test_dir = std::env::temp_dir().join("boundary_isolation");
        std::fs::create_dir_all(&test_dir).ok();

        let list_path = test_dir.join("list.txt");
        let list_content = format!(
            "file '{}'\nduration 5\nfile '{}'\nduration 5",
            file1.to_string_lossy().replace('\\', "/"),
            file2.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(&list_path, &list_content).ok();

        let output = test_dir.join(format!("output_{}.mp4", label));
        let result = Command::new(ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy(), "-c", "copy", output.to_str().unwrap()])
            .output();

        std::fs::remove_file(&list_path).ok();

        if !result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let stderr = result.as_ref().map(|o| String::from_utf8_lossy(&o.stderr).to_string()).unwrap_or_default();
            return (false, format!("Merge failed: {}", stderr.lines().take(3).collect::<Vec<_>>().join("; ")), 0.0);
        }

        let duration = get_duration(ffprobe, &output);
        let mut all_ok = true;
        let mut errors = Vec::new();

        for pct in [0.50, 0.75, 0.85, 0.90, 0.95] {
            let (a_ok, a_err) = test_audio_at_seek(ffmpeg, &output, pct, duration);
            let (v_ok, v_err) = test_video_at_seek(ffmpeg, &output, pct, duration);

            if !a_ok || !v_ok {
                all_ok = false;
                if !a_err.is_empty() {
                    errors.push(format!("A@{:0.0}%: {}", pct * 100.0, a_err.lines().next().unwrap_or("")));
                }
                if !v_err.is_empty() {
                    errors.push(format!("V@{:0.0}%: {}", pct * 100.0, v_err.lines().next().unwrap_or("")));
                }
            }
        }

        std::fs::remove_file(&output).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        (all_ok, errors.join("; "), duration)
    }

    /// PHASE 4C: BOUNDARY ISOLATION MATRIX
    ///
    /// Goal: Isolate whether corruption is caused by:
    /// - Codec change (H264 → H265)
    /// - Sample rate change (44100 ↔ 48000)
    /// - Both
    ///
    /// Test cases:
    ///   A: H264/44100 → H264/44100  (neither changes)
    ///   B: H264/44100 → H264/48000  (sample rate change only)
    ///   C: H264/44100 → H265/44100  (codec change only)
    ///   D: H264/44100 → H265/48000  (both changes)
    #[tokio::test]
    async fn test_boundary_isolation() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 4C: BOUNDARY ISOLATION MATRIX                                 ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("boundary_isolation");
        std::fs::create_dir_all(&test_dir).unwrap();

        // File templates: codec, sample_rate
        // Case A: H264/44100 → H264/44100
        // Case B: H264/44100 → H264/48000
        // Case C: H264/44100 → H265/44100
        // Case D: H264/44100 → H265/48000

        let configs = vec![
            ("libx264", 44100, "libx264", 44100, "A"),
            ("libx264", 44100, "libx264", 48000, "B"),
            ("libx264", 44100, "libx265", 44100, "C"),
            ("libx264", 44100, "libx265", 48000, "D"),
        ];

        let mut results: Vec<(String, bool, String, f64, f64)> = Vec::new();

        for (codec1, sr1, codec2, sr2, label) in &configs {
            println!("\n[TEST CASE {}: {} {}Hz → {} {}Hz]", label, codec1, sr1, codec2, sr2);

            let file1 = test_dir.join(format!("f1_{}.mp4", label));
            let file2 = test_dir.join(format!("f2_{}.mp4", label));

            let (created1, err1) = create_test_file(&ffmpeg, &file1, 5, codec1, *sr1);
            let (created2, err2) = create_test_file(&ffmpeg, &file2, 5, codec2, *sr2);

            if !created1 || !created2 {
                println!("  FAILED: Could not create test files");
                if !created1 { println!("  File1 error: {}", err1.lines().take(3).collect::<Vec<_>>().join("; ")); }
                if !created2 { println!("  File2 error: {}", err2.lines().take(3).collect::<Vec<_>>().join("; ")); }
                results.push((label.to_string(), false, format!("File1: {} File2: {}", err1.lines().next().unwrap_or(""), err2.lines().next().unwrap_or("")), 0.0, 0.0));
                continue;
            }

            let dur1 = get_duration(&ffprobe, &file1);
            let dur2 = get_duration(&ffprobe, &file2);
            println!("  File1: {:.1}s ({} {} Hz)", dur1, codec1, sr1);
            println!("  File2: {:.1}s ({} {} Hz)", dur2, codec2, sr2);

            let (passed, errors, output_dur) = run_concat_test(&ffmpeg, &ffprobe, &file1, &file2, label);
            let status = if passed { "PASS" } else { "FAIL" };
            println!("  Result: {} (output duration: {:.1}s)", status, output_dur);

            if !passed {
                println!("  Errors: {}", errors);
            }

            results.push((label.to_string(), passed, errors, output_dur, dur1 + dur2));

            std::fs::remove_file(&file1).ok();
            std::fs::remove_file(&file2).ok();
        }

        // ══════════════════════════════════════════════════════════════
        // ANALYSIS
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  BOUNDARY ISOLATION ANALYSIS                                        ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        println!("\n  Results Matrix:");
        println!("  ┌───────┬────────┬───────────┬────────────┬──────────────┐");
        println!("  │ Case  │ Change │ Pass/Fail │ Output Dur │ Duration Delta │");
        println!("  ├───────┼────────┼───────────┼────────────┼──────────────┤");

        for (label, passed, _, output_dur, expected) in &results {
            let status = if *passed { "PASS" } else { "FAIL" };
            let delta = if *expected > 0.0 { ((output_dur / expected - 1.0) * 100.0) as i32 } else { 0 };
            println!("  │ {}     │ {} │ {}      │ {:.1}s      │ {:+}%          │", label, label, status, output_dur, delta);
        }
        println!("  └───────┴────────┴───────────┴────────────┴──────────────┘");

        // Determine root cause
        let case_a_pass = results.iter().find(|(l, ..)| l == "A").map(|(_, p, ..)| *p).unwrap_or(false);
        let case_b_pass = results.iter().find(|(l, ..)| l == "B").map(|(_, p, ..)| *p).unwrap_or(false);
        let case_c_pass = results.iter().find(|(l, ..)| l == "C").map(|(_, p, ..)| *p).unwrap_or(false);
        let case_d_pass = results.iter().find(|(l, ..)| l == "D").map(|(_, p, ..)| *p).unwrap_or(false);

        println!("\n  ┌─────────────────────────────────────────────────────────────┐");
        println!("  │ ROOT CAUSE DETERMINATION                                      │");
        println!("  ├─────────────────────────────────────────────────────────────┤");

        if case_a_pass && case_b_pass && case_c_pass && case_d_pass {
            println!("  │ All cases PASS                                                 │");
            println!("  │ → No corruption with 2-file concat                            │");
            println!("  │ → Issue requires larger playlist or specific conditions       │");
        } else if case_a_pass && !case_b_pass {
            println!("  │ Case A: PASS  (H264/44100 → H264/44100)                        │");
            println!("  │ Case B: FAIL  (H264/44100 → H264/48000)                        │");
            println!("  │                                                               │");
            println!("  │ → SAMPLE RATE CHANGE is the root cause                        │");
            println!("  │ → Codec transition is NOT the cause                           │");
        } else if case_a_pass && !case_c_pass {
            println!("  │ Case A: PASS  (H264/44100 → H264/44100)                        │");
            println!("  │ Case C: FAIL  (H264/44100 → H265/44100)                        │");
            println!("  │                                                               │");
            println!("  │ → CODEC CHANGE is the root cause                             │");
            println!("  │ → Sample rate is NOT the cause                               │");
        } else if case_a_pass && case_b_pass && !case_d_pass {
            println!("  │ Case A: PASS                                                   │");
            println!("  │ Case B: PASS  (SR change works)                               │");
            println!("  │ Case C: PASS  (Codec change works)                            │");
            println!("  │ Case D: FAIL  (Both changes together fail)                   │");
            println!("  │                                                               │");
            println!("  │ → COMBINATION of SR + Codec change causes corruption          │");
            println!("  │ → Neither alone is sufficient to trigger the bug               │");
        } else if !case_a_pass {
            println!("  │ Case A: FAIL  (even identical files corrupt)                   │");
            println!("  │ → Something else is wrong - investigate test methodology      │");
        } else {
            println!("  │ Pattern does not match simple expectations                     │");
            println!("  │ → Investigate specific error messages for each failed case    │");
        }

        println!("  └─────────────────────────────────────────────────────────────┘");

        // Duration inflation analysis
        println!("\n  Duration Inflation Analysis:");
        for (label, _, _, output_dur, expected) in &results {
            if *expected > 0.0 {
                let ratio = output_dur / expected;
                let pct_diff = (ratio - 1.0) * 100.0;
                if pct_diff.abs() > 0.5 {
                    println!("  Case {}: {:.1}s / {:.1}s = {:.4} ({:+.2}%)", label, output_dur, expected, ratio, pct_diff);
                } else {
                    println!("  Case {}: {:.1}s / {:.1}s = {:.4} (no inflation)", label, output_dur, expected, ratio);
                }
            }
        }

        std::fs::remove_dir_all(&test_dir).ok();
        println!("\n[TEST] Phase 4C Boundary Isolation complete.");
    }
}