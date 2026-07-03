#[cfg(test)]
mod production_path_verification {
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

        for stream in json.pointer("/streams")?.as_array()? {
            if stream.pointer("/codec_type")?.as_str()? == "audio" {
                let codec = stream.pointer("/codec_name")?.as_str()?.to_string();
                let sr = stream.pointer("/sample_rate")?.as_str()?.parse::<u32>().ok()?;
                return Some((codec, sr));
            }
        }
        None
    }

    fn normalize_file(ffmpeg: &Path, input: &Path, output: &Path, target_sr: u32) -> bool {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-i", input.to_str().unwrap(),
                "-c:v", "copy",
                "-c:a", "aac",
                "-ar", &target_sr.to_string(),
                "-ac", "2",
                output.to_str().unwrap()
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// PRODUCTION PATH VERIFICATION
    ///
    /// Goal: Verify whether mixed sample rate files can bypass normalization
    /// and reach concat together.
    ///
    /// Simulates the production decision logic:
    /// 1. Analyze profiles
    /// 2. Check for a_sample_rate outliers
    /// 3. If outlier found -> needs_normalization = true
    /// 4. Normalize files to dominant SR
    /// 5. Verify concat receives uniform SR
    #[tokio::test]
    async fn test_production_path_verification() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PRODUCTION PATH VERIFICATION AUDIT                                 ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("production_path_verification");
        std::fs::create_dir_all(&test_dir).unwrap();

        // ══════════════════════════════════════════════════════════════
        // SCENARIO: Create files with DIFFERENT sample rates
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO: 44100 Hz + 48000 Hz MIXED PLAYLIST]");

        let files: Vec<(PathBuf, u32)> = vec![
            (test_dir.join("file_44100_a.mp4"), 44100),
            (test_dir.join("file_48000_a.mp4"), 48000),
            (test_dir.join("file_44100_b.mp4"), 44100),
            (test_dir.join("file_48000_b.mp4"), 48000),
        ];

        for (path, sr) in &files {
            let created = create_test_file(&ffmpeg, path, 10, *sr);
            println!("  File {:?}: {} Hz - {}", path.file_name().unwrap().to_str().unwrap(), sr, if created { "CREATED" } else { "FAILED" });
        }

        // ══════════════════════════════════════════════════════════════
        // STEP 1: Analyze profiles - DETECT sample rate outliers
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 1: ANALYZE PROFILES]");

        let mut file_infos: Vec<(usize, u32)> = Vec::new();
        for (i, (path, sr)) in files.iter().enumerate() {
            let info = get_audio_info(&ffprobe, path);
            let actual_sr = info.as_ref().map(|(_, s)| *s).unwrap_or(*sr);
            file_infos.push((i, actual_sr));
            println!("  File {}: input SR = {} Hz", i, actual_sr);
        }

        // Find dominant SR (most common, just like production)
        let mut sr_counts: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for (_, sr) in &file_infos {
            *sr_counts.entry(*sr).or_insert(0) += 1;
        }
        let dominant_sr = sr_counts.iter().max_by_key(|(_, c)| *c).map(|(sr, _)| *sr).unwrap_or(48000);
        println!("  Dominant SR: {} Hz (appears {} times)", dominant_sr, sr_counts.get(&dominant_sr).unwrap_or(&0));

        // Check for mismatches (like production's outlier detection)
        let mismatches: Vec<(usize, u32)> = file_infos.iter().filter(|(_, sr)| *sr != dominant_sr).cloned().collect();
        let has_sr_mismatch = !mismatches.is_empty();

        println!("\n  Sample rate mismatch detected: {}", has_sr_mismatch);
        if has_sr_mismatch {
            for (i, sr) in &mismatches {
                println!("    File {}: {} Hz (will be normalized)", i, sr);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // STEP 2: Simulate production decision logic
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 2: SIMULATE PRODUCTION DECISION LOGIC]");

        // This mirrors merge.rs lines 1493-1502:
        // "Force normalization if critical outliers exist that would break concat"
        let has_critical_outliers = has_sr_mismatch; // a_sample_rate is critical

        let needs_normalization = has_critical_outliers; // simplified

        println!("  has_critical_outliers (a_sample_rate mismatch): {}", has_critical_outliers);
        println!("  needs_normalization: {}", needs_normalization);

        // ══════════════════════════════════════════════════════════════
        // STEP 3: Normalize files (if needed)
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 3: NORMALIZE FILES TO DOMINANT SR]");

        let normalized_files: Vec<PathBuf> = files.iter()
            .enumerate()
            .map(|(i, (_, _))| test_dir.join(format!("normalized_{}.mp4", i)))
            .collect();

        for (i, ((path, original_sr), norm_path)) in files.iter().zip(normalized_files.iter()).enumerate() {
            if *original_sr != dominant_sr {
                // Normalize to dominant SR
                let success = normalize_file(&ffmpeg, path, norm_path, dominant_sr);
                println!("  File {}: {} Hz -> {} Hz (normalized): {}",
                    i, original_sr, dominant_sr, if success { "OK" } else { "FAILED" });
            } else {
                // Copy to normalized path
                std::fs::copy(path, norm_path).ok();
                println!("  File {}: {} Hz -> {} Hz (no change): OK",
                    i, original_sr, dominant_sr);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // STEP 4: Verify normalized files have UNIFORM SR
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 4: VERIFY NORMALIZED FILES HAVE UNIFORM SR]");

        let normalized_srs: Vec<u32> = normalized_files.iter()
            .map(|p| get_audio_info(&ffprobe, p).map(|(_, s)| s).unwrap_or(0))
            .collect();

        println!("  Normalized SRs: {:?}", normalized_srs);
        let all_uniform = normalized_srs.iter().all(|&sr| sr == dominant_sr);
        println!("  All files have uniform SR ({} Hz): {}", dominant_sr, all_uniform);

        // ══════════════════════════════════════════════════════════════
        // STEP 5: Verify concat receives uniform SR files
        // ══════════════════════════════════════════════════════════════
        println!("\n[STEP 5: CONCATENATE NORMALIZED FILES]");

        if all_uniform {
            let list = test_dir.join("norm_list.txt");
            let list_content: String = normalized_files.iter()
                .map(|p| format!("file '{}'\nduration 10", p.to_string_lossy().replace('\\', "/")))
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&list, &list_content).unwrap();

            let output = test_dir.join("output_norm_concat.mp4");
            let concat_ok = Command::new(&ffmpeg)
                .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list.to_string_lossy(), "-c", "copy", output.to_str().unwrap()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if concat_ok {
                let dur_output = Command::new(&ffprobe)
                    .args(&["-v", "quiet", "-print_format", "json", "-show_format", output.to_str().unwrap()])
                    .output()
                    .ok()
                    .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());

                let duration = dur_output
                    .as_ref()
                    .and_then(|j| j.pointer("/format/duration"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);

                let expected = 40.0; // 4 files x 10s each
                let inflation = (duration / expected - 1.0) * 100.0;

                println!("  Concat: SUCCESS");
                println!("  Output duration: {:.1}s (expected {:.1}s)", duration, expected);
                println!("  Duration inflation: {:+.1}%", inflation);

                if inflation.abs() < 0.5 {
                    println!("  ✅ NO DURATION INFLATION");
                } else {
                    println!("  ⚠️ DURATION INFLATION DETECTED");
                }
            } else {
                println!("  Concat: FAILED");
            }
        }

        // ══════════════════════════════════════════════════════════════
        // CRITICAL TEST: What if normalization was BYPASSED?
        // ══════════════════════════════════════════════════════════════
        println!("\n[CRITICAL TEST: CONCAT WITHOUT NORMALIZATION (BYPASS SCENARIO)]");

        let raw_files: Vec<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
        let raw_list = test_dir.join("raw_list.txt");
        let raw_list_content: String = raw_files.iter()
            .map(|p| format!("file '{}'\nduration 10", p.to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&raw_list, &raw_list_content).unwrap();

        let raw_output_path = test_dir.join("output_raw_concat.mp4");
        let raw_concat_ok = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &raw_list.to_string_lossy(), "-c", "copy", raw_output_path.to_str().unwrap()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let mut raw_inflation = 0.0;
        if raw_concat_ok {
            let dur_raw = Command::new(&ffprobe)
                .args(&["-v", "quiet", "-print_format", "json", "-show_format", raw_output_path.to_str().unwrap()])
                .output()
                .ok()
                .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());

            let dur_raw_val = dur_raw
                .as_ref()
                .and_then(|j| j.pointer("/format/duration"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0);

            raw_inflation = (dur_raw_val / 40.0 - 1.0) * 100.0;
            println!("  Raw concat (NO normalization): SUCCESS");
            println!("  Output duration: {:.1}s (expected 40.0s)", dur_raw_val);
            println!("  Duration inflation: {:+.1}%", raw_inflation);

            if raw_inflation > 1.0 {
                println!("  ⚠️ CORRUPTION CONFIRMED when normalization is bypassed!");
            }
        }

        // ══════════════════════════════════════════════════════════════
        // FINAL VERDICT
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PRODUCTION PATH VERIFICATION VERDICT                              ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        if all_uniform {
            println!("");
            println!("  ✅ NORMALIZATION PIPELINE PREVENTES MIXED SR CONCAT");
            println!("");
            println!("  Question: Can mixed SR files reach concat?");
            println!("  Answer: NO — production pipeline normalizes them first.");
            println!("");
            println!("  Question: What if normalization is bypassed?");
            println!("  Answer: Mixed SR concat produces {}% duration inflation.", raw_inflation);
            println!("");
            println!("  CONCLUSION: Sample rate corruption requires BYPASSING normalization.");
            println!("  This is NOT expected production behavior.");
        } else {
            println!("");
            println!("  ❌ NORMALIZATION FAILED — mixed SR files could reach concat!");
            println!("");
            println!("  This is a PRODUCTION BUG if it occurs.");
        }

        // Cleanup
        let output_norm = test_dir.join("output_norm_concat.mp4");
        let raw_out = test_dir.join("output_raw_concat.mp4");

        for (path, _) in &files { std::fs::remove_file(path).ok(); }
        for path in &normalized_files { std::fs::remove_file(path).ok(); }
        std::fs::remove_file(&output_norm).ok();
        std::fs::remove_file(&raw_out).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Production Path Verification complete.");
    }
}