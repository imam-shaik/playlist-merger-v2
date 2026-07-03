#[cfg(test)]
mod audio_codec_certification_tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn probe_file(ffprobe: &Path, path: &Path) -> Option<crate::types::MediaInfo> {
        crate::ffmpeg::probe::probe_file(ffprobe, path).ok()
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

    fn create_test_file(
        ffmpeg: &Path,
        path: &Path,
        duration_sec: u32,
        acodec: &str,
        label: &str,
    ) {
        let output = Command::new(ffmpeg)
            .args([
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=320x240:rate=25", duration_sec),
                "-f", "lavfi",
                "-i", "sine=frequency=440:sample_rate=48000",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-c:a", acodec,
                "-ar", "48000",
                "-t", &duration_sec.to_string(),
                "-shortest",
                path.to_str().unwrap()
            ])
            .output()
            .expect(&format!("Failed to run ffmpeg for {} test file", label));

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Check if encoder is available — skip is handled by caller
            if stderr.contains("Unknown encoder") || stderr.contains("codec not supported") {
                println!("[SKIP] {} encoder not available — skipping test", label);
            } else {
                println!("[CREATE] {} FAILED: {}", label, stderr.lines().take(3).collect::<Vec<_>>().join("; "));
            }
        } else {
            println!("[CREATE] {} OK: {} bytes", label,
                std::fs::metadata(path).map(|m| m.len()).unwrap_or(0));
        }
    }

    fn try_concat_mkv(ffmpeg: &Path, list_path: &Path, output: &Path, label: &str) -> Result<(), String> {
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
                println!("[CONCAT] {}: SUCCESS ({} bytes)", label, size);
                Ok(())
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let first_errors: Vec<&str> = stderr.lines()
                    .filter(|l| l.contains("error") || l.contains("Error") || l.contains("Invalid"))
                    .take(3)
                    .collect();
                let err_summary = if first_errors.is_empty() {
                    stderr.lines().take(3).collect::<Vec<_>>().join("; ")
                } else {
                    first_errors.join("; ")
                };
                println!("[CONCAT] {}: FAILED — {}", label, err_summary);
                Err(stderr.into_owned())
            }
            Err(e) => {
                println!("[CONCAT] {}: ERROR — {}", label, e);
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

        let v_codec = info.video_streams.first().map(|s| s.codec_name.clone()).unwrap_or_default();
        let a_codec = info.audio_streams.first().map(|s| s.codec_name.clone()).unwrap_or_default();

        println!("[PROBE] {}: vcodec={}, acodec={}, vdur={:.1}s, adur={:.1}s, dur={:.1}s",
            label, v_codec, a_codec, v_dur, a_dur, total_dur);

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
                        println!("[VALIDATE] {} seek {:.0}% ({:.1}s): WARN — {}",
                            label, pct * 100.0, seek_sec,
                            stderr.lines().next().unwrap_or("unknown"));
                        seek_ok = false;
                    }
                }
                Err(e) => {
                    println!("[VALIDATE] {} seek {:.0}% ({:.1}s): ERROR — {}",
                        label, pct * 100.0, seek_sec, e);
                    seek_ok = false;
                }
            }
        }

        // Check for decoder warnings in output
        let mut decode_ok = true;
        let decode_result = Command::new(ffmpeg)
            .args([
                "-v", "error",
                "-i", output.to_str().unwrap(),
                "-f", "null", "-"
            ])
            .output();

        if let Ok(out) = decode_result {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.trim().is_empty() && out.status.success() {
                println!("[VALIDATE] {} decode: WARN — {}", label,
                    stderr.lines().take(2).collect::<Vec<_>>().join("; "));
                // Non-fatal warnings are OK — only fail on actual errors
                if stderr.contains("error") || stderr.contains("Error") {
                    decode_ok = false;
                }
            } else if !out.status.success() {
                println!("[VALIDATE] {} decode: FAILED — {}", label,
                    stderr.lines().take(3).collect::<Vec<_>>().join("; "));
                decode_ok = false;
            } else {
                println!("[VALIDATE] {} decode: OK", label);
            }
        }

        let sync_ok = diff < 1.0; // Allow 1s tolerance for concat boundary rounding
        println!("[VALIDATE] {} sync: vdur={:.3}s adur={:.3}s diff={:.3}s — {}",
            label, v_dur, a_dur, diff,
            if sync_ok { "SYNC_OK" } else { "SYNC_FAIL" });

        let all_pass = sync_ok && seek_ok && decode_ok;
        let detail = format!("acodec={} vdur={:.1}s adur={:.1}s diff={:.3}s seek={} decode={}",
            a_codec, v_dur, a_dur, diff,
            if seek_ok { "ok" } else { "issues" },
            if decode_ok { "ok" } else { "issues" });

        (all_pass, detail)
    }

    /// Test whether files with different audio codecs can be concatenated
    /// with -c copy into an MKV container.
    ///
    /// Test combinations:
    ///   Case A: AAC + Opus
    ///   Case B: AAC + MP3
    ///   Case C: AAC + FLAC
    ///   Case D: Opus + MP3
    ///   Case E: AAC + Opus + MP3 (three-way)
    #[tokio::test]
    async fn test_audio_codec_concat_compatibility() {
        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("audio_codec_certification");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 3: AUDIO CODEC CERTIFICATION TEST (MKV)             ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
        println!("  Testing whether different audio codecs can be concatenated");
        println!("  with -c copy into an MKV container without playback issues.");
        println!();

        // ── Create test files with different audio codecs ────────────────
        let f_aac = test_dir.join("aac_3s.mp4");
        let f_opus = test_dir.join("opus_3s.mp4");
        let f_mp3 = test_dir.join("mp3_3s.mp4");
        let f_flac = test_dir.join("flac_3s.mp4");
        let f_aac_5s = test_dir.join("aac_5s.mp4");

        println!("─── Creating test files ───");
        create_test_file(&ffmpeg, &f_aac, 3, "aac", "AAC-3s");
        create_test_file(&ffmpeg, &f_opus, 3, "libopus", "Opus-3s");
        create_test_file(&ffmpeg, &f_mp3, 3, "libmp3lame", "MP3-3s");
        create_test_file(&ffmpeg, &f_flac, 3, "flac", "FLAC-3s");
        create_test_file(&ffmpeg, &f_aac_5s, 5, "aac", "AAC-5s");

        // Only test files that were created successfully
        let mut test_files: Vec<(PathBuf, &str)> = Vec::new();
        if f_aac.exists() { test_files.push((f_aac.clone(), "AAC")); }
        if f_opus.exists() { test_files.push((f_opus.clone(), "Opus")); }
        if f_mp3.exists() { test_files.push((f_mp3.clone(), "MP3")); }
        if f_flac.exists() { test_files.push((f_flac.clone(), "FLAC")); }
        if f_aac_5s.exists() { test_files.push((f_aac_5s.clone(), "AAC-5s")); }

        if test_files.len() < 2 {
            println!("\n  ⚠️  Not enough test files could be created. Skipping. (Need at least 2 audio codecs)");
            println!("     This is expected if FFmpeg does not have the required encoders.");
            std::fs::remove_dir_all(&test_dir).ok();
            return;
        }

        // ── CASE A: AAC + Opus ───────────────────────────────────────────
        println!("\n─── CASE A: AAC + Opus ───");
        let mut results: Vec<(String, bool, String)> = Vec::new();
        let mut run_case = |label: &str, files: &[&Path], durations: &[f64]| {
            // Verify all files exist
            if files.iter().any(|f| !f.exists()) {
                println!("  ⚠️  Skipping {} — one or more files not available", label);
                return;
            }
            let list = test_dir.join(format!("concat_{}.txt", label.replace(' ', "_")));
            write_concat_list(files, durations, &list);
            let out = test_dir.join(format!("out_{}.mkv", label.replace(' ', "_")));
            let concat_ok = try_concat_mkv(&ffmpeg, &list, &out, label).is_ok();
            if concat_ok && out.exists() {
                let (pass, detail) = validate_output(&ffprobe, &ffmpeg, &out, label);
                results.push((label.to_string(), pass, detail));
            } else {
                results.push((label.to_string(), false, "concat_failed".to_string()));
            }
        };

        // Only test codec pairs that are available
        let aac_file = test_files.iter().find(|(_, name)| *name == "AAC");
        let opus_file = test_files.iter().find(|(_, name)| *name == "Opus");
        let mp3_file = test_files.iter().find(|(_, name)| *name == "MP3");
        let flac_file = test_files.iter().find(|(_, name)| *name == "FLAC");

        if let (Some((aac, _)), Some((opus, _))) = (aac_file, opus_file) {
            run_case("AAC_Opus_1", &[aac, opus], &[3.0, 3.0]);
            run_case("AAC_Opus_2", &[opus, aac], &[3.0, 3.0]);
        }

        // ── CASE B: AAC + MP3 ───────────────────────────────────────────
        println!("\n─── CASE B: AAC + MP3 ───");
        if let (Some((aac, _)), Some((mp3, _))) = (aac_file, mp3_file) {
            run_case("AAC_MP3", &[aac, mp3], &[3.0, 3.0]);
        }

        // ── CASE C: AAC + FLAC ──────────────────────────────────────────
        println!("\n─── CASE C: AAC + FLAC ───");
        if let (Some((aac, _)), Some((flac, _))) = (aac_file, flac_file) {
            run_case("AAC_FLAC", &[aac, flac], &[3.0, 3.0]);
        }

        // ── CASE D: Opus + MP3 ──────────────────────────────────────────
        println!("\n─── CASE D: Opus + MP3 ───");
        if let (Some((opus, _)), Some((mp3, _))) = (opus_file, mp3_file) {
            run_case("Opus_MP3", &[opus, mp3], &[3.0, 3.0]);
        }

        // ── CASE E: AAC + Opus + MP3 (three-way) ────────────────────────
        println!("\n─── CASE E: AAC + Opus + MP3 (three-way) ───");
        let three_files: Vec<&Path> = [aac_file, opus_file, mp3_file].iter()
            .filter_map(|f| *f).map(|(p, _)| p.as_path()).collect();
        if three_files.len() == 3 {
            run_case("AAC_Opus_MP3_3way", &three_files, &[3.0, 3.0, 3.0]);
        }

        // ── CASE F: AAC (different durations, 3s + 5s) ─────────────────
        println!("\n─── CASE F: AAC (3s) + AAC (5s) — same codec control ───");
        if let (Some((_aac, _)), _) = (aac_file, aac_file.as_ref()) {
            if aac_file.is_some() && f_aac.exists() && f_aac_5s.exists() {
                run_case("AAC_same_codec_control", &[&f_aac, &f_aac_5s], &[3.0, 5.0]);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // SUMMARY
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║  AUDIO CODEC CERTIFICATION SUMMARY                           ║");
        println!("╚══════════════════════════════════════════════════════════════╝");
        println!();
        for (label, pass, detail) in &results {
            let icon = if *pass { "✅ PASS" } else { "❌ FAIL" };
            println!("  {:25} | {} | {}", label, icon, detail);
        }

        let pass_count = results.iter().filter(|r| r.1).count();
        let total_cases = results.len();
        let all_pass = pass_count == total_cases;

        println!();
        println!("  Result: {}/{} test cases passed", pass_count, total_cases);

        if all_pass && total_cases > 0 {
            println!("\n  🚨 CONCLUSION: Direct concat (-c copy) WORKS for MKV");
            println!("     with mixed audio codecs.");
            println!("     'a_codec' CAN be added to Smart MKV's MKV_SAFE_PROPERTIES.");
            println!("     Expected impact: significant normalization reduction");
            println!("     for playlists with mixed AAC/Opus/MP3 files.");
        } else if pass_count > 0 {
            println!("\n  ⚠️  CONCLUSION: Partial success.");
            println!("     Some audio codec pairs work, some don't.");
            println!("     'a_codec' should remain in normalization for now.");
            println!("     Investigate failing pairs before relaxing.");
        } else {
            println!("\n  ✅ CONCLUSION: Direct concat FAILS for mixed audio codecs.");
            println!("     Current behavior (normalize) is correct.");
            println!("     'a_codec' must remain in MKV_SAFE_PROPERTIES exclusion.");
        }

        // Report encoder availability for documentation
        println!("\n  Encoder Availability:");
        println!("    AAC:  {}", if f_aac.exists() { "✅" } else { "❌" });
        println!("    Opus: {}", if f_opus.exists() { "✅" } else { "❌" });
        println!("    MP3:  {}", if f_mp3.exists() { "✅" } else { "❌" });
        println!("    FLAC: {}", if f_flac.exists() { "✅" } else { "❌" });

        // Cleanup
        std::fs::remove_dir_all(&test_dir).ok();
        println!("\n[TEST] Audio codec certification complete.");
    }
}
