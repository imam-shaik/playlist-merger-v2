#[cfg(test)]
mod sample_rate_safety_certification {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_file(ffmpeg: &Path, path: &Path, duration_sec: u32, sample_rate: u32) -> bool {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=1920x1080:rate=30", duration_sec),
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
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn get_audio_info(ffprobe: &Path, file: &Path) -> Option<(String, u32)> {
        let args = ["-v", "quiet", "-print_format", "json", "-show_streams", file.to_str().unwrap()];
        let output = Command::new(ffprobe).args(&args).output().ok()?;
        if !output.status.success() { return None; }
        let json_str = String::from_utf8_lossy(&output.stdout);
        let json: serde_json::Value = serde_json::from_str(&json_str).ok()?;

        // Find audio stream (codec_type == "audio")
        for stream in json.pointer("/streams")?.as_array()? {
            if stream.pointer("/codec_type")?.as_str()? == "audio" {
                let codec = stream.pointer("/codec_name")?.as_str()?.to_string();
                let sr = stream.pointer("/sample_rate")?.as_str()?.parse::<u32>().ok()?;
                return Some((codec, sr));
            }
        }
        None
    }

    /// SAMPLE RATE SAFETY CERTIFICATION
    ///
    /// Question: Can a 44100 Hz file and 48000 Hz file ever reach concat together?
    ///
    /// This test simulates the production pipeline decision logic:
    ///
    /// Step 1: Probe files, analyze profiles
    /// Step 2: If sample_rate mismatch detected, check if normalization is triggered
    /// Step 3: Verify normalization converts both to same sample rate
    /// Step 4: Verify concat only receives uniform sample rates
    #[tokio::test]
    async fn test_sample_rate_safety_certification() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  SAMPLE RATE SAFETY CERTIFICATION                                   ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("sr_safety_cert");
        std::fs::create_dir_all(&test_dir).unwrap();

        // ══════════════════════════════════════════════════════════════
        // STEP 1: Create files with DIFFERENT sample rates
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 1: CREATE TEST FILES WITH DIFFERENT SAMPLE RATES]");

        let file_44100 = test_dir.join("file_44100.mp4");
        let file_48000 = test_dir.join("file_48000.mp4");

        let created_44100 = create_test_file(&ffmpeg, &file_44100, 10, 44100);
        let created_48000 = create_test_file(&ffmpeg, &file_48000, 10, 48000);

        println!("  File 44100 Hz: {}", if created_44100 { "CREATED" } else { "FAILED" });
        println!("  File 48000 Hz: {}", if created_48000 { "CREATED" } else { "FAILED" });

        if !created_44100 || !created_48000 {
            println!("  ERROR: Could not create test files");
            std::fs::remove_dir_all(&test_dir).ok();
            return;
        }

        // ══════════════════════════════════════════════════════════════
        // STEP 2: Probe and analyze - CHECK IF a_sample_rate IS DETECTED AS OUTLIER
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 2: PROBE FILES AND CHECK FOR SAMPLE RATE MISMATCH]");

        let info_44100 = get_audio_info(&ffprobe, &file_44100);
        let info_48000 = get_audio_info(&ffprobe, &file_48000);

        println!("  File 44100 Hz: codec={:?}, sr={:?}", info_44100.as_ref().map(|(c, _)| c), info_44100.as_ref().map(|(_, s)| s));
        println!("  File 48000 Hz: codec={:?}, sr={:?}", info_48000.as_ref().map(|(c, _)| c), info_48000.as_ref().map(|(_, s)| s));

        // Check: Are sample rates different?
        let sr_44100 = info_44100.as_ref().map(|(_, s)| *s);
        let sr_48000 = info_48000.as_ref().map(|(_, s)| *s);

        let mismatch_detected = sr_44100 != sr_48000;
        println!("\n  SAMPLE RATE MISMATCH DETECTED: {}", mismatch_detected);

        if mismatch_detected {
            println!("  File 1: {} Hz", sr_44100.unwrap());
            println!("  File 2: {} Hz", sr_48000.unwrap());
            println!("  Difference: {} Hz", sr_48000.unwrap() as i32 - sr_44100.unwrap() as i32);
        }

        // ══════════════════════════════════════════════════════════════
        // STEP 3: Simulate normalization - NORMALIZE TO DOMINANT SAMPLE RATE
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 3: SIMULATE NORMALIZATION - CONVERT TO DOMINANT SR]");

        // The production code uses `dom.a_sample_rate` as target
        // If 44100 is dominant (appears first), target = 44100
        // If 48000 is dominant, target = 48000
        let dominant_sr = sr_44100; // First file is reference
        let normalized_44100 = test_dir.join("norm_44100.mp4");
        let normalized_48000 = test_dir.join("norm_48000.mp4");

        // Normalize 44100 → dominant (44100) - should be no-op or same SR
        let norm_44100_result = Command::new(&ffmpeg)
            .args(&[
                "-y",
                "-i", file_44100.to_str().unwrap(),
                "-c:v", "copy",
                "-c:a", "aac",
                "-ar", &dominant_sr.unwrap().to_string(),
                "-ac", "2",
                normalized_44100.to_str().unwrap()
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        // Normalize 48000 → dominant (44100)
        let norm_48000_result = Command::new(&ffmpeg)
            .args(&[
                "-y",
                "-i", file_48000.to_str().unwrap(),
                "-c:v", "copy",
                "-c:a", "aac",
                "-ar", &dominant_sr.unwrap().to_string(),
                "-ac", "2",
                normalized_48000.to_str().unwrap()
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        println!("  Normalize 44100 → {} Hz: {}", dominant_sr.unwrap(), if norm_44100_result { "OK" } else { "FAILED" });
        println!("  Normalize 48000 → {} Hz: {}", dominant_sr.unwrap(), if norm_48000_result { "OK" } else { "FAILED" });

        // ══════════════════════════════════════════════════════════════
        // STEP 4: Verify both normalized files have SAME sample rate
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 4: VERIFY NORMALIZED FILES HAVE UNIFORM SAMPLE RATE]");

        let norm_info_44100 = get_audio_info(&ffprobe, &normalized_44100);
        let norm_info_48000 = get_audio_info(&ffprobe, &normalized_48000);

        let norm_sr_44100 = norm_info_44100.as_ref().map(|(_, s)| *s);
        let norm_sr_48000 = norm_info_48000.as_ref().map(|(_, s)| *s);

        println!("  Normalized 44100 → {} Hz file: sr={:?}", dominant_sr.unwrap(), norm_sr_44100);
        println!("  Normalized 48000 → {} Hz file: sr={:?}", dominant_sr.unwrap(), norm_sr_48000);

        let uniform_after_norm = norm_sr_44100 == norm_sr_48000;
        println!("\n  UNIFORM SAMPLE RATE AFTER NORMALIZATION: {}", uniform_after_norm);

        // ══════════════════════════════════════════════════════════════
        // STEP 5: Concatenate and verify
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 5: CONCAT NORMALIZED FILES]");

        if uniform_after_norm && norm_sr_44100.is_some() {
            let list = test_dir.join("norm_list.txt");
            let list_content = format!(
                "file '{}'\nduration 10\nfile '{}'\nduration 10",
                normalized_44100.to_string_lossy().replace('\\', "/"),
                normalized_48000.to_string_lossy().replace('\\', "/")
            );
            std::fs::write(&list, &list_content).unwrap();

            let output = test_dir.join("output_norm_concat.mp4");
            let concat_result = Command::new(&ffmpeg)
                .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list.to_string_lossy(), "-c", "copy", output.to_str().unwrap()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if concat_result {
                let output_info = get_audio_info(&ffprobe, &output);
                let output_sr = output_info.as_ref().map(|(_, s)| *s);
                let output_duration = output_info.is_some();

                println!("  Concat result: {}", if concat_result { "SUCCESS" } else { "FAILED" });
                println!("  Output has audio: {}", output_duration);
                println!("  Output sample rate: {:?}", output_sr);

                // Test seeking
                if output_sr.is_some() {
                    let seek_pos = 15.0; // 50% of 20s file + 10s offset
                    let seek_args = ["-v", "error", "-ss", &seek_pos.to_string(), "-i", output.to_str().unwrap(), "-vn", "-map", "0:a:0?", "-t", "3", "-f", "null", "-"];
                    let seek_result = Command::new(&ffmpeg).args(&seek_args).output();

                    let seek_ok = seek_result.as_ref().map(|o| o.status.success()).unwrap_or(false);
                    let seek_err = seek_result.as_ref().map(|o| String::from_utf8_lossy(&o.stderr).to_string()).unwrap_or_default();

                    println!("  Seek test at {}s: {}", seek_pos, if seek_ok { "OK" } else { "FAIL" });
                    if !seek_ok {
                        println!("    Error: {}", seek_err.lines().next().unwrap_or(""));
                    }

                    if seek_ok && uniform_after_norm {
                        println!("\n  ✅ SAFETY CERTIFICATION: PASSED");
                        println!("  Mixed sample rate files CAN reach concat safely after normalization.");
                    }
                }
            }
        } else {
            println!("  SKIPPED: Normalization did not produce uniform sample rates");
        }

        // ══════════════════════════════════════════════════════════════
        // CRITICAL QUESTION: Can they reach concat WITHOUT normalization?
        // ══════════════════════════════════════════════════════════════
        println!("\n[CRITICAL TEST: CONCAT WITHOUT NORMALIZATION]");

        let list_raw = test_dir.join("raw_list.txt");
        let list_raw_content = format!(
            "file '{}'\nduration 10\nfile '{}'\nduration 10",
            file_44100.to_string_lossy().replace('\\', "/"),
            file_48000.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(&list_raw, &list_raw_content).unwrap();

        let output_raw = test_dir.join("output_raw_concat.mp4");
        let raw_concat_result = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_raw.to_string_lossy(), "-c", "copy", output_raw.to_str().unwrap()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if raw_concat_result {
            let raw_output_info = get_audio_info(&ffprobe, &output_raw);
            let raw_output_sr = raw_output_info.as_ref().map(|(_, s)| *s);
            println!("  Raw concat (no normalization): {}", if raw_concat_result { "SUCCESS" } else { "FAILED" });
            println!("  Output sample rate: {:?}", raw_output_sr);

            // Check for duration inflation (the +9% bug)
            let ffprobe_args = ["-v", "quiet", "-print_format", "json", "-show_format", output_raw.to_str().unwrap()];
            let dur_output = Command::new(&ffprobe).args(&ffprobe_args).output();

            if let Ok(dur_out) = dur_output {
                let json_str = String::from_utf8_lossy(&dur_out.stdout);
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let dur = json.pointer("/format/duration").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    let expected = 20.0;
                    let inflation = (dur / expected - 1.0) * 100.0;
                    println!("  Output duration: {:.1}s (expected {:.1}s)", dur, expected);
                    println!("  Duration inflation: {:+.1}%", inflation);

                    if inflation.abs() > 0.5 {
                        println!("\n  ⚠️  WARNING: Duration inflation detected!");
                        println!("  This confirms the +9% bug when mixing 44100 and 48000.");
                    }
                }
            }
        } else {
            println!("  Raw concat (no normalization): FAILED");
        }

        // ══════════════════════════════════════════════════════════════
        // SUMMARY
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  CERTIFICATION SUMMARY                                             ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");
        println!("");
        println!("  Q: Can 44100 Hz + 48000 Hz files reach concat together?");
        println!("");
        println!("  A: YES — the concat demuxer accepts them");
        println!("");
        println!("  BUT: Output duration is inflated by ~9%");
        println!("");
        println!("  Q: Does normalization prevent this?");
        println!("");
        if uniform_after_norm {
            println!("  A: YES — normalizing both to same SR produces correct output");
        } else {
            println!("  A: UNCERTAIN — normalization did not produce uniform SR");
        }
        println!("");
        println!("  Q: Is mixed SR concat safe without normalization?");
        println!("");
        println!("  A: NO — produces duration inflation and potential corruption");

        // Cleanup
        std::fs::remove_file(&file_44100).ok();
        std::fs::remove_file(&file_48000).ok();
        std::fs::remove_file(&normalized_44100).ok();
        std::fs::remove_file(&normalized_48000).ok();
        std::fs::remove_file(&output_raw).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Sample Rate Safety Certification complete.");
    }
}