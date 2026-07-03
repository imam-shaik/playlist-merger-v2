#[cfg(test)]
mod audio_seek_root_cause {
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
        _file_idx: usize,
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

    #[allow(dead_code)]
    fn get_stream_info(ffprobe: &Path, file: &Path) -> Option<serde_json::Value> {
        let args = ["-v", "quiet", "-print_format", "json", "-show_streams", "-show_packets", file.to_str().unwrap()];
        let output = Command::new(ffprobe).args(&args).output().ok()?;
        if output.status.success() {
            serde_json::from_slice(&output.stdout).ok()
        } else {
            None
        }
    }

    fn test_audio_seek_at(ffmpeg: &Path, file: &Path, seek_pct: f64, duration: f64) -> (bool, String, String) {
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
                let stdout = String::from_utf8_lossy(&out.stdout);
                if out.status.success() && stderr.trim().is_empty() {
                    (true, String::new(), stdout.to_string())
                } else {
                    (false, stderr.trim().to_string(), stdout.to_string())
                }
            }
            Err(e) => (false, e.to_string(), String::new())
        }
    }

    fn test_video_seek_at(ffmpeg: &Path, file: &Path, seek_pct: f64, duration: f64) -> (bool, String, String) {
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
                let stdout = String::from_utf8_lossy(&out.stdout);
                if out.status.success() && stderr.trim().is_empty() {
                    (true, String::new(), stdout.to_string())
                } else {
                    (false, stderr.trim().to_string(), stdout.to_string())
                }
            }
            Err(e) => (false, e.to_string(), String::new())
        }
    }

    /// PHASE 4B: ROOT CAUSE AUDIT
    ///
    /// Goal: Find exact corruption source in failed Large playlist.
    ///
    /// Steps:
    /// 1. Create 10-file playlist (same as Phase 4A that failed)
    /// 2. Trace timeline to identify which source file is at 85%, 90%, 95%
    /// 3. Test each source file individually
    /// 4. Identify if corruption is in source, normalized output, or concat result
    /// 5. Report exact source file responsible
    #[tokio::test]
    async fn test_root_cause_audit() {
        let test_dir = std::env::temp_dir().join("root_cause_audit");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 4B: ROOT CAUSE AUDIT                                          ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();

        // ══════════════════════════════════════════════════════════════
        // RECREATE LARGE PLAYLIST (10 files with mixed params)
        // ══════════════════════════════════════════════════════════════
        println!("\n[CREATING TEST FILES]");

        let large_files: Vec<PathBuf> = (0..10)
            .map(|i| test_dir.join(format!("large_{}.mp4", i)))
            .collect();

        let codecs = ["libx264", "libx264", "libx265", "libx264", "libx265", "libx264", "libx265", "libx264", "libx264", "libx265"];
        let rates = [44100, 48000, 44100, 48000, 44100, 48000, 44100, 48000, 44100, 48000];
        let durations = [5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0, 5.0];

        for (i, f) in large_files.iter().enumerate() {
            let output = create_test_file(&ffmpeg, f, durations[i] as u32, 1920, 1080, 30, codecs[i], rates[i], i);
            if output.status.success() {
                let size = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
                println!("  File {}: {} ({} Hz, {}) - {} KB", i, codecs[i], rates[i],
                    if i % 2 == 0 { "H264" } else { "H265" }, size / 1024);
            } else {
                println!("  File {}: FAILED to create", i);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // BUILD FILE TIMELINE
        // ══════════════════════════════════════════════════════════════
        println!("\n[BUILDING FILE TIMELINE]");
        let mut cumulative: Vec<(usize, f64, f64)> = Vec::new(); // (file_idx, start, end)
        let mut current_start = 0.0;
        for (i, dur) in durations.iter().enumerate() {
            cumulative.push((i, current_start, current_start + dur));
            println!("  File {}: {:.1}s - {:.1}s", i, current_start, current_start + dur);
            current_start += dur;
        }
        let total_duration: f64 = durations.iter().sum();

        // ══════════════════════════════════════════════════════════════
        // MERGE WITH STREAM COPY (direct concat - no normalization)
        // ══════════════════════════════════════════════════════════════
        println!("\n[CREATING CONCAT OUTPUT (stream copy)]");
        let list_large = test_dir.join("list_large.txt");
        let list_content: String = large_files
            .iter()
            .enumerate()
            .map(|(i, p)| format!("file '{}'\nduration {}", p.to_string_lossy().replace('\\', "/"), durations[i]))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&list_large, &list_content).unwrap();

        let output_large = test_dir.join("output_large.mp4");
        let merge_result = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_large.to_string_lossy(), "-c", "copy", output_large.to_str().unwrap()])
            .output();

        if !merge_result.as_ref().map(|o| o.status.success()).unwrap_or(false) {
            let stderr = if let Ok(ref o) = merge_result {
                String::from_utf8_lossy(&o.stderr).to_string()
            } else {
                String::new()
            };
            println!("  Merge FAILED: {}", stderr.lines().take(5).collect::<Vec<_>>().join("; "));
        } else {
            println!("  Merge: SUCCESS");
            let output_duration = get_duration(&ffprobe, &output_large);
            println!("  Output duration: {:.1}s", output_duration);

            // ══════════════════════════════════════════════════════════════
            // IDENTIFY WHICH FILE IS AT EACH FAILING SEEK POINT
            // ══════════════════════════════════════════════════════════════
            println!("\n[IDENTIFYING SOURCE FILES AT FAILING SEEK POINTS]");
            let fail_points = [0.85, 0.90, 0.95];

            for &pct in &fail_points {
                let seek_sec = total_duration * pct;
                let (file_idx, file_start, file_end) = cumulative.iter()
                    .find(|(_, start, end)| seek_sec >= *start && seek_sec < *end)
                    .map(|(i, s, e)| (*i, *s, *e))
                    .unwrap_or((usize::MAX, 0.0, 0.0));

                let file_duration = if file_idx < large_files.len() {
                    durations[file_idx]
                } else {
                    0.0
                };

                let relative_seek = seek_sec - file_start;
                println!("\n  {:.0}% seek ({:.1}s):", pct * 100.0, seek_sec);
                println!("    → File {} (codec={}, sr={})", file_idx, codecs[file_idx], rates[file_idx]);
                println!("    → File timeline: {:.1}s - {:.1}s", file_start, file_end);
                println!("    → Relative seek within file: {:.1}s / {:.1}s", relative_seek, file_duration);
            }

            // ══════════════════════════════════════════════════════════════
            // TEST EACH SOURCE FILE INDIVIDUALLY FOR CORRUPTION
            // ══════════════════════════════════════════════════════════════
            println!("\n[TESTING EACH SOURCE FILE FOR CORRUPTION]");

            for (i, f) in large_files.iter().enumerate() {
                let duration = get_duration(&ffprobe, f);
                if duration <= 0.0 {
                    println!("  File {}: duration=0, skipping", i);
                    continue;
                }

                // Test at the same relative position as the failing points
                let relative_positions = [0.85, 0.90, 0.95];
                let mut file_has_issue = false;

                for &rel_pct in &relative_positions {
                    let seek_sec = duration * rel_pct;
                    let (a_ok, a_err, _) = test_audio_seek_at(&ffmpeg, f, rel_pct, duration);
                    let (v_ok, v_err, _) = test_video_seek_at(&ffmpeg, f, rel_pct, duration);

                    if !a_ok || !v_ok {
                        file_has_issue = true;
                        println!("  File {}: ISSUE at {:.0}% ({:.1}s)", i, rel_pct * 100.0, seek_sec);
                        if !a_ok { println!("    Audio: {}", a_err.lines().next().unwrap_or("")); }
                        if !v_ok { println!("    Video: {}", v_err.lines().next().unwrap_or("")); }
                    }
                }

                if !file_has_issue {
                    println!("  File {}: ✅ CLEAN (no corruption at 85%, 90%, 95%)", i);
                }
            }

            // ══════════════════════════════════════════════════════════════
            // TEST OUTPUT AT FAILING POINTS + SURROUNDING AREA
            // ══════════════════════════════════════════════════════════════
            println!("\n[TESTING OUTPUT AT FAILING POINTS + SURROUNDING]");

            let dense_points: Vec<f64> = vec![
                0.70, 0.75, 0.80, 0.82, 0.84, 0.85, 0.87, 0.89, 0.90, 0.92, 0.94, 0.95, 0.97, 0.99
            ];

            let mut corruption_onset: Option<f64> = None;
            for &pct in &dense_points {
                let (a_ok, a_err, _) = test_audio_seek_at(&ffmpeg, &output_large, pct, output_duration);
                let (v_ok, v_err, _) = test_video_seek_at(&ffmpeg, &output_large, pct, output_duration);

                let seek_sec = output_duration * pct;
                let status = if !a_ok || !v_ok { "❌ FAIL" } else { "✅ OK" };

                if !a_ok || !v_ok {
                    corruption_onset.get_or_insert(pct);
                    println!("  {:.0}% ({:.1}s): {} - First corruption detected here!", pct * 100.0, seek_sec, status);
                    if !a_err.is_empty() { println!("    Audio: {}", a_err.lines().next().unwrap_or("")); }
                    if !v_err.is_empty() { println!("    Video: {}", v_err.lines().next().unwrap_or("")); }
                } else {
                    println!("  {:.0}% ({:.1}s): {}", pct * 100.0, seek_sec, status);
                }
            }

            // ══════════════════════════════════════════════════════════════
            // ROOT CAUSE ANALYSIS
            // ══════════════════════════════════════════════════════════════
            println!("\n╔══════════════════════════════════════════════════════════════════════╗");
            println!("║  ROOT CAUSE ANALYSIS                                                ║");
            println!("╚══════════════════════════════════════════════════════════════════════╝");

            let mut responsible_files: Vec<usize> = Vec::new();

            // Find which file(s) cover the corruption onset region
            if let Some(onset_pct) = corruption_onset {
                let onset_sec = output_duration * onset_pct;
                println!("\n  Corruption onset: {:.0}% ({:.1}s)", onset_pct * 100.0, onset_sec);

                for (i, (_, start, end)) in cumulative.iter().enumerate() {
                    if onset_sec >= *start && onset_sec < *end {
                        responsible_files.push(i);
                    }
                }

                println!("\n  Files covering corruption onset region:");
                for i in &responsible_files {
                    println!("    File {}: codec={}, sr={}", i, codecs[*i], rates[*i]);
                    println!("      Timeline: {:.1}s - {:.1}s", cumulative[*i].1, cumulative[*i].2);
                }

                // Check if corruption onset is at a concat boundary
                for (_i, (_, start, end)) in cumulative.iter().enumerate() {
                    let boundary_tolerance = 0.5;
                    if (onset_sec - end).abs() < boundary_tolerance || (onset_sec - start).abs() < boundary_tolerance {
                        println!("\n  Corruption onset is NEAR A CONCAT BOUNDARY (within {:.1}s)", boundary_tolerance);
                        println!("     This suggests a timestamp/PTS discontinuity at the concat boundary.");
                    }
                }
            }

            // Check if ALL H265 files have issues
            let h265_indices: Vec<usize> = cumulative.iter()
                .enumerate()
                .filter(|(i, _)| codecs[*i] == "libx265")
                .map(|(i, _)| i)
                .collect();

            if !h265_indices.is_empty() {
                println!("\n  H265 files in playlist: {:?}", h265_indices);
                println!("  H265 coverage: {:.1}s - {:.1}s",
                    cumulative[h265_indices[0]].1, cumulative[h265_indices[h265_indices.len()-1]].2);
            }

            // ══════════════════════════════════════════════════════════════
            // CONCLUSION
            // ══════════════════════════════════════════════════════════════
            println!("\n╔══════════════════════════════════════════════════════════════════════╗");
            println!("║  ROOT CAUSE SUMMARY                                                  ║");
            println!("╚══════════════════════════════════════════════════════════════════════╝");

            // Determine if issue is in source or concat
            let source_issues: Vec<usize> = large_files.iter().enumerate()
                .filter(|(_i, f)| {
                    let duration = get_duration(&ffprobe, f);
                    if duration <= 0.0 { return false; }
                    let (_, a_err, _) = test_audio_seek_at(&ffmpeg, f, 0.85, duration);
                    let (_, v_err, _) = test_video_seek_at(&ffmpeg, f, 0.85, duration);
                    !a_err.is_empty() || !v_err.is_empty()
                })
                .map(|(i, _)| i)
                .collect();

            if !source_issues.is_empty() {
                println!("\n  🔍 ROOT CAUSE: Source files have pre-existing issues");
                println!("  Affected files: {:?}", source_issues);
                println!("  Recommendation: Fix or reject these source files before merge");
            } else {
                println!("\n  🔍 ROOT CAUSE: Corruption introduced by CONCATENATION");
                println!("  All source files pass individual seek tests.");
                println!("  Issue appears only in the concatenated output.");
                println!("  Recommendation: Investigate concat boundary handling");
            }

            println!("\n  Files at corrupted output region: {:?}", responsible_files);
            println!("  Corruption onset: {:.0}%", corruption_onset.unwrap_or(0.0) * 100.0);
        }

        // Cleanup
        for f in &large_files { std::fs::remove_file(f).ok(); }
        std::fs::remove_file(&output_large).ok();
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Phase 4B Root Cause Audit complete.");
    }
}