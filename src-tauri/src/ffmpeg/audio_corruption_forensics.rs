#[cfg(test)]
mod audio_corruption_forensics {
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
    ) -> std::process::Output {
        Command::new(ffmpeg)
            .args(&[
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
            ])
            .output()
            .expect("Failed to create test file")
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

    fn get_audio_stream_info(ffprobe: &Path, file: &Path) -> Option<serde_json::Value> {
        let args = ["-v", "quiet", "-print_format", "json", "-show_streams", "-show_format", file.to_str().unwrap()];
        let output = Command::new(ffprobe).args(&args).output().ok()?;
        if output.status.success() {
            serde_json::from_slice(&output.stdout).ok()
        } else {
            None
        }
    }

    fn test_audio_decode_segment(
        ffmpeg: &Path,
        file: &Path,
        start_sec: f64,
        duration_sec: f64,
    ) -> (bool, String) {
        let args = [
            "-v", "error",
            "-ss", &start_sec.to_string(),
            "-i", file.to_str().unwrap(),
            "-vn",
            "-map", "0:a:0?",
            "-t", &duration_sec.to_string(),
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

    /// AAC CORRUPTION FORENSICS
    ///
    /// Investigate decoder error: "Number of bands (N) exceeds limit (45)"
    ///
    /// Goal: Trace AAC stream through pipeline and identify where corruption first appears.
    #[tokio::test]
    async fn test_aac_corruption_forensics() {
        let test_dir = std::env::temp_dir().join("aac_corruption_forensics");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  AAC CORRUPTION FORENSICS                                           ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();

        // ══════════════════════════════════════════════════════════════
        // SCENARIO: Create files with various AAC configurations
        // ══════════════════════════════════════════════════════════════
        println!("\n[CREATING TEST FILES WITH MIXED SAMPLE RATES]");

        let files: Vec<PathBuf> = (0..5)
            .map(|i| test_dir.join(format!("source_{}.mp4", i)))
            .collect();

        let sample_rates = [44100, 48000, 44100, 48000, 44100];

        for (i, f) in files.iter().enumerate() {
            let output = create_test_file(&ffmpeg, f, 10, "libx264", sample_rates[i]);
            if output.status.success() {
                let info = get_audio_stream_info(&ffprobe, f);
                let _codec = info.as_ref()
                    .and_then(|j| j.pointer("/streams/0/codec_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let sr = info.as_ref()
                    .and_then(|j| j.pointer("/streams/0/sample_rate"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let channels = info.as_ref()
                    .and_then(|j| j.pointer("/streams/0/channels"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                println!("  File {}: AAC {} Hz, {} channels, {} KB",
                    i, sr, channels, std::fs::metadata(f).map(|m| m.len() / 1024).unwrap_or(0));
            } else {
                println!("  File {}: FAILED to create", i);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 1: Verify source files AAC streams are clean
        // ══════════════════════════════════════════════════════════════
        println!("\n[STAGE 1: TESTING SOURCE FILES AAC STREAMS]");

        let mut source_issues: Vec<usize> = Vec::new();
        for (i, f) in files.iter().enumerate() {
            let (ok, err) = test_audio_decode_segment(&ffmpeg, f, 5.0, 5.0);
            if !ok {
                source_issues.push(i);
                println!("  File {}: CORRUPTED - {}", i, err.lines().next().unwrap_or(""));
            } else {
                println!("  File {}: CLEAN", i);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 2: Normalize files and test intermediate AAC
        // ══════════════════════════════════════════════════════════════
        println!("\n[STAGE 2: NORMALIZING FILES AND TESTING INTERMEDIATE AAC]");

        let normalized_files: Vec<PathBuf> = (0..5)
            .map(|i| test_dir.join(format!("normalized_{}.mp4", i)))
            .collect();

        let mut normalization_failed = false;
        for (i, (src, dst)) in files.iter().zip(normalized_files.iter()).enumerate() {
            let output = Command::new(&ffmpeg)
                .args(&[
                    "-y",
                    "-i", src.to_str().unwrap(),
                    "-c:v", "copy",
                    "-c:a", "aac",
                    "-ar", "48000",
                    "-ac", "2",
                    dst.to_str().unwrap()
                ])
                .output();

            if output.as_ref().map(|o| o.status.success()).unwrap_or(false) {
                let (ok, err) = test_audio_decode_segment(&ffmpeg, dst, 5.0, 5.0);
                if !ok {
                    println!("  Normalized {}: CORRUPTED after normalization - {}", i, err.lines().next().unwrap_or(""));
                    normalization_failed = true;
                } else {
                    println!("  Normalized {}: CLEAN", i);
                }
            } else {
                println!("  Normalized {}: FAILED to create", i);
                normalization_failed = true;
            }
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 3: Concat files and test output AAC
        // ══════════════════════════════════════════════════════════════
        println!("\n[STAGE 3: CONCATENATING AND TESTING FINAL OUTPUT AAC]");

        let list_path = test_dir.join("concat_list.txt");
        let list_content: String = files.iter()
            .enumerate()
            .map(|(_, p)| format!("file '{}'\nduration 10", p.to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&list_path, &list_content).unwrap();

        let output_stream_copy = test_dir.join("output_stream_copy.mp4");
        let copy_result = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy(), "-c", "copy", output_stream_copy.to_str().unwrap()])
            .output();

        if copy_result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let duration = get_duration(&ffprobe, &output_stream_copy);
            println!("  Stream copy concat: SUCCESS, duration {:.1}s", duration);

            test_audio_decode_segment(&ffmpeg, &output_stream_copy, 5.0, 5.0);
            test_audio_decode_segment(&ffmpeg, &output_stream_copy, 25.0, 5.0);
            test_audio_decode_segment(&ffmpeg, &output_stream_copy, 45.0, 5.0);

            for seek_pct in [0.25, 0.50, 0.75, 0.85, 0.90, 0.95] {
                let seek_sec = duration * seek_pct;
                let (ok, err) = test_audio_decode_segment(&ffmpeg, &output_stream_copy, seek_sec, 3.0);
                let status = if ok { "OK" } else { "CORRUPTED" };
                println!("    Seek {:.0}% ({}s): {}", seek_pct * 100.0, seek_sec, status);
                if !ok {
                    println!("      Error: {}", err.lines().take(3).collect::<Vec<_>>().join("; "));
                }
            }
        }

        // Test normalized concat
        let list_normalized = test_dir.join("concat_list_normalized.txt");
        let list_normalized_content: String = normalized_files.iter()
            .enumerate()
            .map(|(_, p)| format!("file '{}'\nduration 10", p.to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&list_normalized, &list_normalized_content).unwrap();

        let output_normalized_concat = test_dir.join("output_normalized_concat.mp4");
        let norm_concat_result = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_normalized.to_string_lossy(), "-c", "copy", output_normalized_concat.to_str().unwrap()])
            .output();

        if norm_concat_result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let duration = get_duration(&ffprobe, &output_normalized_concat);
            println!("\n  Normalized concat: SUCCESS, duration {:.1}s", duration);

            for seek_pct in [0.25, 0.50, 0.75, 0.85, 0.90, 0.95] {
                let seek_sec = duration * seek_pct;
                let (ok, err) = test_audio_decode_segment(&ffmpeg, &output_normalized_concat, seek_sec, 3.0);
                let status = if ok { "OK" } else { "CORRUPTED" };
                println!("    Seek {:.0}% ({}s): {}", seek_pct * 100.0, seek_sec, status);
                if !ok {
                    println!("      Error: {}", err.lines().take(3).collect::<Vec<_>>().join("; "));
                }
            }
        }

        // ══════════════════════════════════════════════════════════════
        // STAGE 4: Test concat with re-encode (not stream copy)
        // ══════════════════════════════════════════════════════════════
        println!("\n[STAGE 4: TESTING CONCAT WITH RE-ENCODE]");

        let output_reencode = test_dir.join("output_reencode.mp4");
        let reencode_result = Command::new(&ffmpeg)
            .args(&[
                "-y",
                "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy(),
                "-c:v", "libx264", "-preset", "ultrafast",
                "-c:a", "aac", "-ar", "48000",
                output_reencode.to_str().unwrap()
            ])
            .output();

        if reencode_result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let duration = get_duration(&ffprobe, &output_reencode);
            println!("  Re-encode concat: SUCCESS, duration {:.1}s", duration);

            for seek_pct in [0.25, 0.50, 0.75, 0.85, 0.90, 0.95] {
                let seek_sec = duration * seek_pct;
                let (ok, err) = test_audio_decode_segment(&ffmpeg, &output_reencode, seek_sec, 3.0);
                let status = if ok { "OK" } else { "CORRUPTED" };
                println!("    Seek {:.0}% ({}s): {}", seek_pct * 100.0, seek_sec, status);
                if !ok {
                    println!("      Error: {}", err.lines().take(3).collect::<Vec<_>>().join("; "));
                }
            }
        }

        // ══════════════════════════════════════════════════════════════
        // CRITICAL TEST: Does concat with SAME sample rates work?
        // ══════════════════════════════════════════════════════════════
        println!("\n[CRITICAL TEST: CONCAT WITH UNIFORM SAMPLE RATES]");

        let uniform_files: Vec<PathBuf> = (0..5)
            .map(|i| test_dir.join(format!("uniform_{}.mp4", i)))
            .collect();

        for (_i, f) in uniform_files.iter().enumerate() {
            create_test_file(&ffmpeg, f, 10, "libx264", 48000);
        }

        let list_uniform = test_dir.join("list_uniform.txt");
        let list_uniform_content: String = uniform_files.iter()
            .enumerate()
            .map(|(_i, p)| format!("file '{}'\nduration 10", p.to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&list_uniform, &list_uniform_content).unwrap();

        let output_uniform = test_dir.join("output_uniform.mp4");
        let uniform_result = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_uniform.to_string_lossy(), "-c", "copy", output_uniform.to_str().unwrap()])
            .output();

        if uniform_result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let duration = get_duration(&ffprobe, &output_uniform);
            println!("  Uniform SR concat: SUCCESS, duration {:.1}s", duration);

            for seek_pct in [0.25, 0.50, 0.75, 0.85, 0.90, 0.95] {
                let seek_sec = duration * seek_pct;
                let (ok, err) = test_audio_decode_segment(&ffmpeg, &output_uniform, seek_sec, 3.0);
                let status = if ok { "OK" } else { "CORRUPTED" };
                println!("    Seek {:.0}% ({}s): {}", seek_pct * 100.0, seek_sec, status);
                if !ok {
                    println!("      Error: {}", err.lines().take(3).collect::<Vec<_>>().join("; "));
                }
            }
        }

        // ══════════════════════════════════════════════════════════════
        // ROOT CAUSE SUMMARY
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  AAC CORRUPTION FORENSICS SUMMARY                                   ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        println!("\n  Source files with issues: {:?}", source_issues);
        println!("  Normalization produced corrupt files: {}", normalization_failed);

        if source_issues.is_empty() && !normalization_failed {
            println!("\n  All source and normalized files are clean.");
            println!("  Corruption would appear ONLY in concatenated output.");
            println!("\n  This would indicate a CONCAT-BOUNDARY issue.");
            println!("  The concat demuxer may be producing invalid AAC packet boundaries.");
        } else if !source_issues.is_empty() {
            println!("\n  ROOT CAUSE: Source files have pre-existing AAC corruption.");
            println!("  Recommendation: Investigate source file generation or acquisition.");
        } else if normalization_failed {
            println!("\n  ROOT CAUSE: Normalization is producing corrupted AAC output.");
            println!("  Recommendation: Investigate AAC encoding in normalization pipeline.");
        }

        println!("\n  Key test: Does uniform sample rate concat produce clean output?");
        println!("  If YES: The issue is sample-rate mismatch at concat boundaries.");
        println!("  If NO: The issue is more fundamental to the concat demuxer.");

        // Cleanup
        for f in &files { std::fs::remove_file(f).ok(); }
        for f in &normalized_files { std::fs::remove_file(f).ok(); }
        for f in &uniform_files { std::fs::remove_file(f).ok(); }
        std::fs::remove_file(&output_stream_copy).ok();
        std::fs::remove_file(&output_normalized_concat).ok();
        std::fs::remove_file(&output_reencode).ok();
        std::fs::remove_file(&output_uniform).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] AAC Corruption Forensics complete.");
    }
}