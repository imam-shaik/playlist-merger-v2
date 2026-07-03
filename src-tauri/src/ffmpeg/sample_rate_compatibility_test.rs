#[cfg(test)]
mod sample_rate_tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_file(ffmpeg: &Path, path: &Path, duration_sec: u32, sample_rate: u32, prefix: &str) {
        let output = Command::new(ffmpeg)
            .args([
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
            .expect("Failed to create test file");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("[CREATE] {} @ {}Hz FAILED: {}", prefix, sample_rate, stderr);
        } else {
            println!("[CREATE] {} @ {}Hz OK: {} bytes",
                prefix,
                sample_rate,
                std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
            );
        }
    }

    fn probe_file(ffprobe: &Path, path: &Path) -> Option<crate::types::MediaInfo> {
        crate::ffmpeg::probe::probe_file(ffprobe, path).ok()
    }

    fn try_concat(ffmpeg: &Path, list_path: &Path, output: &Path, label: &str) -> Result<(), String> {
        let _output_local = output.to_string_lossy().into_owned();
        let list_str = list_path.to_string_lossy().into_owned();

        let result = Command::new(ffmpeg)
            .args([
                "-y",
                "-f", "concat",
                "-safe", "0",
                "-i", &list_str,
                "-c", "copy",
                output.to_str().unwrap()
            ])
            .output();

        match result {
            Ok(out) if out.status.success() => {
                let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
                println!("[CONCAT] {}: SUCCESS ({} bytes)", label, size);
                Ok(())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                println!("[CONCAT] {}: FAILED — {}", label, stderr.lines().take(3).collect::<Vec<_>>().join("; "));
                Err(stderr.into_owned())
            }
            Err(e) => {
                println!("[CONCAT] {}: ERROR — {}", label, e);
                Err(e.to_string())
            }
        }
    }

    fn try_concat_with_genpts(ffmpeg: &Path, list_path: &Path, output: &Path, label: &str) -> Result<(), String> {
        let result = Command::new(ffmpeg)
            .args([
                "-y",
                "-fflags", "+genpts",
                "-f", "concat",
                "-safe", "0",
                "-i", &list_path.to_string_lossy(),
                "-c", "copy",
                output.to_str().unwrap()
            ])
            .output();

        match result {
            Ok(out) if out.status.success() => {
                let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
                println!("[CONCAT] {} (+genpts): SUCCESS ({} bytes)", label, size);
                Ok(())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                println!("[CONCAT] {} (+genpts): FAILED — {}", label, stderr.lines().take(3).collect::<Vec<_>>().join("; "));
                Err(stderr.into_owned())
            }
            Err(e) => {
                println!("[CONCAT] {} (+genpts): ERROR — {}", label, e);
                Err(e.to_string())
            }
        }
    }

    fn try_concat_with_filter(ffmpeg: &Path, list_path: &Path, output: &Path, label: &str) -> Result<(), String> {
        let result = Command::new(ffmpeg)
            .args([
                "-y",
                "-f", "concat",
                "-safe", "0",
                "-i", &list_path.to_string_lossy(),
                "-c:v", "copy",
                "-af", "aresample=async=44100",
                output.to_str().unwrap()
            ])
            .output();

        match result {
            Ok(out) if out.status.success() => {
                let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
                println!("[CONCAT] {} (aresample): SUCCESS ({} bytes)", label, size);
                Ok(())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                println!("[CONCAT] {} (aresample): FAILED — {}", label, stderr.lines().take(3).collect::<Vec<_>>().join("; "));
                Err(stderr.into_owned())
            }
            Err(e) => {
                println!("[CONCAT] {} (aresample): ERROR — {}", label, e);
                Err(e.to_string())
            }
        }
    }

    fn validate_output(ffprobe: &Path, ffmpeg: &Path, output: &Path, label: &str) -> (bool, String) {
        let info = match probe_file(ffprobe, output) {
            Some(i) => i,
            None => return (false, "Cannot probe output".to_string())
        };

        let v_dur = info.video_streams.first().and_then(|s| s.duration).unwrap_or(0.0);
        let a_dur = info.audio_streams.first().and_then(|s| s.duration).unwrap_or(0.0);
        let total_dur = info.duration;
        let diff = (v_dur - a_dur).abs();

        // Test seekability at 25%, 50%, 75%
        let mut seek_ok = true;
        for &pct in &[0.25, 0.50, 0.75] {
            let seek_sec = total_dur * pct;
            let result = Command::new(ffmpeg)
                .args([
                    "-v", "error",
                    "-ss", &seek_sec.to_string(),
                    "-i", output.to_str().unwrap(),
                    "-t", "1",
                    "-f", "null", "-"
                ])
                .output();

            match result {
                Ok(out) if out.status.success() => {
                    println!("[VALIDATE] {} seek {:.0}% ({:.1}s): OK", label, pct * 100.0, seek_sec);
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if !stderr.trim().is_empty() {
                        println!("[VALIDATE] {} seek {:.0}% ({:.1}s): WARN — {}", label, pct * 100.0, seek_sec, stderr.lines().next().unwrap_or("unknown"));
                        seek_ok = false;
                    }
                }
                Err(e) => {
                    println!("[VALIDATE] {} seek {:.0}% ({:.1}s): ERROR — {}", label, pct * 100.0, seek_sec, e);
                    seek_ok = false;
                }
            }
        }

        let sync_ok = diff < 0.5;
        println!("[VALIDATE] {} duration: video={:.3}s audio={:.3}s diff={:.3}s — {}{}",
            label, v_dur, a_dur, diff,
            if sync_ok { "SYNC_OK" } else { "SYNC_FAIL" },
            if seek_ok { " SEEK_OK" } else { " SEEK_WARN" }
        );

        let _status = if sync_ok && seek_ok { "PASS" } else { "PARTIAL" };
        let detail = format!("v={:.3}s a={:.3}s diff={:.3}s seek={}",
            v_dur, a_dur, diff,
            if seek_ok { "ok" } else { "issues" }
        );
        (sync_ok && seek_ok, detail)
    }

    fn write_concat_list(files: &[&Path], durations: &[f64], list_path: &Path) {
        use std::fmt::Write as _;
        let mut content = String::new();
        for (i, file) in files.iter().enumerate() {
            let raw = file.to_string_lossy().replace('\\', "/");
            let escaped = raw.replace('\'', "'\\''");
            writeln!(content, "file '{}'", escaped).unwrap();
            if durations.len() > i {
                writeln!(content, "duration {}", durations[i]).unwrap();
            }
        }
        std::fs::write(list_path, &content).unwrap();
    }

    /// PHASE 2B-A: SAMPLE RATE COMPATIBILITY INVESTIGATION
    ///
    /// Tests whether FFmpeg concat can handle mismatched audio sample rates
    /// (44100 Hz vs 48000 Hz) without audio re-encoding.
    ///
    /// Test cases:
    /// Case 1: 44100 AAC + 48000 AAC (same codec, different SR)
    /// Case 2: 44100 AAC + 48000 AAC + genpts (force PTS generation)
    /// Case 3: 44100 AAC + 48000 AAC + aresample (output-stage resampling)
    /// Case 4: Mixed duration test
    ///
    /// Expected outcomes:
    /// - Outcome A: Concat FAILS → normalization required (current behavior)
    /// - Outcome B: Concat SUCCEEDS → normalization may be unnecessary
    /// - Outcome C: Concat succeeds but with subtle issues → smart fallback needed
    #[tokio::test]
    async fn test_sample_rate_concat_compatibility() {
        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("sample_rate_test");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 2B-A: SAMPLE RATE COMPATIBILITY INVESTIGATION        ║");
        println!("╚══════════════════════════════════════════════════════════════╝");

        // ══════════════════════════════════════════════════════════════
        // CASE 1: 44100 AAC + 48000 AAC — Basic concat test
        // ══════════════════════════════════════════════════════════════
        println!("\n─── CASE 1: 44100 AAC + 48000 AAC ───");

        let f1 = test_dir.join("file_44100_aac.mp4");
        let f2 = test_dir.join("file_48000_aac.mp4");

        create_test_file(&ffmpeg, &f1, 3, 44100, "File-A (44100)");
        create_test_file(&ffmpeg, &f2, 3, 48000, "File-B (48000)");

        // Probe to verify sample rates
        let info1 = probe_file(&ffprobe, &f1);
        let info2 = probe_file(&ffprobe, &f2);
        println!("[PROBE] File-A: SR={:?} Codec={:?}",
            info1.as_ref().and_then(|i| i.audio_streams.first().and_then(|s| s.sample_rate)),
            info1.as_ref().and_then(|i| i.audio_streams.first().map(|s| s.codec_name.clone()))
        );
        println!("[PROBE] File-B: SR={:?} Codec={:?}",
            info2.as_ref().and_then(|i| i.audio_streams.first().and_then(|s| s.sample_rate)),
            info2.as_ref().and_then(|i| i.audio_streams.first().map(|s| s.codec_name.clone()))
        );

        let list1 = test_dir.join("concat_44100_48000.txt");
        write_concat_list(&[&f1, &f2], &[3.0, 3.0], &list1);

        // Test 1a: Direct concat (no flags)
        let out1a = test_dir.join("out_1a_direct.mp4");
        let r1a = try_concat(&ffmpeg, &list1, &out1a, "1a-direct");

        // Test 1b: Concat with +genpts
        let out1b = test_dir.join("out_1b_genpts.mp4");
        let r1b = try_concat_with_genpts(&ffmpeg, &list1, &out1b, "1b-genpts");

        // Test 1c: Concat with aresample filter
        let out1c = test_dir.join("out_1c_aresample.mp4");
        let r1c = try_concat_with_filter(&ffmpeg, &list1, &out1c, "1c-aresample");

        if out1a.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out1a, "1a-direct"); }
        if out1b.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out1b, "1b-genpts"); }
        if out1c.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out1c, "1c-aresample"); }

        // ══════════════════════════════════════════════════════════════
        // CASE 2: 44100 AAC + 48000 AAC — Different durations (3s + 5s)
        // ══════════════════════════════════════════════════════════════
        println!("\n─── CASE 2: 44100 AAC (3s) + 48000 AAC (5s) ───");

        let f3 = test_dir.join("file_44100_3s.mp4");
        let f4 = test_dir.join("file_48000_5s.mp4");

        create_test_file(&ffmpeg, &f3, 3, 44100, "File-C (44100, 3s)");
        create_test_file(&ffmpeg, &f4, 5, 48000, "File-D (48000, 5s)");

        let list2 = test_dir.join("concat_44100_48000_different_dur.txt");
        write_concat_list(&[&f3, &f4], &[3.0, 5.0], &list2);

        let out2a = test_dir.join("out_2a_direct.mp4");
        let r2a = try_concat(&ffmpeg, &list2, &out2a, "2a-direct");

        let out2b = test_dir.join("out_2b_genpts.mp4");
        let r2b = try_concat_with_genpts(&ffmpeg, &list2, &out2b, "2b-genpts");

        if out2a.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out2a, "2a-direct"); }
        if out2b.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out2b, "2b-genpts"); }

        // ══════════════════════════════════════════════════════════════
        // CASE 3: Three files — 44100 + 48000 + 44100
        // ══════════════════════════════════════════════════════════════
        println!("\n─── CASE 3: 44100 + 48000 + 44100 (alternating) ───");

        let f5 = test_dir.join("file_44100_2s.mp4");
        let f6 = test_dir.join("file_48000_2s.mp4");
        let f7 = test_dir.join("file_44100_2s_b.mp4");

        create_test_file(&ffmpeg, &f5, 2, 44100, "File-E (44100, 2s)");
        create_test_file(&ffmpeg, &f6, 2, 48000, "File-F (48000, 2s)");
        create_test_file(&ffmpeg, &f7, 2, 44100, "File-G (44100, 2s)");

        let list3 = test_dir.join("concat_three_way.txt");
        write_concat_list(&[&f5, &f6, &f7], &[2.0, 2.0, 2.0], &list3);

        let out3a = test_dir.join("out_3a_direct.mp4");
        let r3a = try_concat(&ffmpeg, &list3, &out3a, "3a-direct");

        let out3b = test_dir.join("out_3b_genpts.mp4");
        let r3b = try_concat_with_genpts(&ffmpeg, &list3, &out3b, "3b-genpts");

        if out3a.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out3a, "3a-direct"); }
        if out3b.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out3b, "3b-genpts"); }

        // ══════════════════════════════════════════════════════════════
        // CASE 4: Long duration (stress test)
        // ══════════════════════════════════════════════════════════════
        println!("\n─── CASE 4: 44100 (10s) + 48000 (10s) — stress test ───");

        let f8 = test_dir.join("file_44100_10s.mp4");
        let f9 = test_dir.join("file_48000_10s.mp4");

        create_test_file(&ffmpeg, &f8, 10, 44100, "File-H (44100, 10s)");
        create_test_file(&ffmpeg, &f9, 10, 48000, "File-I (48000, 10s)");

        let list4 = test_dir.join("concat_long.txt");
        write_concat_list(&[&f8, &f9], &[10.0, 10.0], &list4);

        let out4a = test_dir.join("out_4a_direct.mp4");
        let r4a = try_concat(&ffmpeg, &list4, &out4a, "4a-direct");

        let out4b = test_dir.join("out_4b_genpts.mp4");
        let r4b = try_concat_with_genpts(&ffmpeg, &list4, &out4b, "4b-genpts");

        if out4a.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out4a, "4a-direct"); }
        if out4b.exists() { let _ = validate_output(&ffprobe, &ffmpeg, &out4b, "4b-genpts"); }

        // ══════════════════════════════════════════════════════════════
        // SUMMARY
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 2B-A SUMMARY                                         ║");
        println!("╚══════════════════════════════════════════════════════════════╝");

        let results = [
            ("Case 1 (3s+3s, direct)", r1a.is_ok()),
            ("Case 1 (3s+3s, +genpts)", r1b.is_ok()),
            ("Case 1 (3s+3s, aresample)", r1c.is_ok()),
            ("Case 2 (3s+5s, direct)", r2a.is_ok()),
            ("Case 2 (3s+5s, +genpts)", r2b.is_ok()),
            ("Case 3 (2s×3, direct)", r3a.is_ok()),
            ("Case 3 (2s×3, +genpts)", r3b.is_ok()),
            ("Case 4 (10s×2, direct)", r4a.is_ok()),
            ("Case 4 (10s×2, +genpts)", r4b.is_ok()),
        ];

        for (label, ok) in &results {
            println!("  {:30} | {}", label, if *ok { "✅ CONCAT SUCCEEDED" } else { "❌ CONCAT FAILED" });
        }

        let pass_count = results.iter().filter(|r| r.1).count();
        let total = results.len();
        println!("\n  Result: {}/{} concat attempts succeeded", pass_count, total);

        if pass_count == total {
            println!("\n  🚨 CONCLUSION: Direct concat WORKS for mismatched sample rates.");
            println!("     Current AudioReencode may be UNNECESSARY for AAC sample_rate mismatches.");
        } else if pass_count > 0 {
            println!("\n  ⚠️  CONCLUSION: Partial success — smart fallback architecture needed.");
        } else {
            println!("\n  ✅ CONCLUSION: Direct concat FAILS — normalization required (keep current logic).");
        }

        // Cleanup
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Phase 2B-A investigation complete.");
    }

    /// Test whether aresample filter produces correct output sample rate
    #[tokio::test]
    async fn test_aresample_output_sample_rate() {
        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("aresample_sr_test");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n─── ARESAMPLE OUTPUT SAMPLE RATE TEST ───");

        let f1 = test_dir.join("file_44100.mp4");
        let f2 = test_dir.join("file_48000.mp4");

        create_test_file(&ffmpeg, &f1, 3, 44100, "A-44100");
        create_test_file(&ffmpeg, &f2, 3, 48000, "B-48000");

        let list = test_dir.join("concat_ar.txt");
        write_concat_list(&[&f1, &f2], &[3.0, 3.0], &list);

        let out = test_dir.join("out_aresample.mp4");
        let result = Command::new(&ffmpeg)
            .args([
                "-y",
                "-f", "concat",
                "-safe", "0",
                "-i", &list.to_string_lossy(),
                "-af", "aresample=44100",
                "-c:v", "copy",
                out.to_str().unwrap()
            ])
            .output();

        if result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let info = probe_file(&ffprobe, &out);
            let actual_sr = info.as_ref()
                .and_then(|i| i.audio_streams.first().and_then(|s| s.sample_rate));
            println!("[RESULT] Output sample rate: {:?}", actual_sr);
            if actual_sr == Some(44100) {
                println!("  ✅ aresample correctly outputs 44100 Hz (target dominant rate)");
            } else {
                println!("  ⚠️  aresample output {} Hz (expected 44100)", actual_sr.unwrap_or(0));
            }
        } else {
            let stderr = result.as_ref().map(|o| String::from_utf8_lossy(&o.stderr)).unwrap_or_default();
            println!("  ❌ aresample concat FAILED: {}", stderr.lines().take(3).collect::<Vec<_>>().join("; "));
        }

        std::fs::remove_dir_all(&test_dir).ok();
    }
}