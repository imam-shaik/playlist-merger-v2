#[cfg(test)]
mod sample_rate_benchmark {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{Duration, Instant};

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn probe_file(ffprobe: &Path, path: &Path) -> Option<crate::types::MediaInfo> {
        crate::ffmpeg::probe::probe_file(ffprobe, path).ok()
    }

    fn create_test_file(ffmpeg: &Path, path: &Path, duration_sec: u32, sample_rate: u32, prefix: &str) {
        let _output = Command::new(ffmpeg)
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

        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        println!("[CREATE] {} @ {}Hz: {} bytes", prefix, sample_rate, size);
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

    /// Get audio bitrate from probe info
    #[allow(dead_code)]
    fn get_audio_bitrate(info: &crate::types::MediaInfo) -> Option<u64> {
        info.audio_streams.first().and_then(|s| s.bit_rate)
    }

    /// Validate output: check A/V sync and seekability
    #[allow(dead_code)]
    fn validate_output(ffprobe: &Path, ffmpeg: &Path, output: &Path) -> (bool, f64, f64) {
        let info = match probe_file(ffprobe, output) {
            Some(i) => i,
            None => return (false, 0.0, 0.0)
        };

        let v_dur = info.video_streams.first().and_then(|s| s.duration).unwrap_or(0.0);
        let a_dur = info.audio_streams.first().and_then(|s| s.duration).unwrap_or(0.0);
        let diff = (v_dur - a_dur).abs();
        let sync_ok = diff < 0.1; // 100ms tolerance

        // Seek test
        let total_dur = info.duration;
        let seek_ok = [0.25, 0.50, 0.75].iter().all(|pct| {
            let seek_sec = total_dur * pct;
            Command::new(ffmpeg)
                .args(["-v", "error", "-ss", &seek_sec.to_string(), "-i", output.to_str().unwrap(), "-t", "1", "-f", "null", "-"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        });

        (sync_ok && seek_ok, v_dur, a_dur)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // METHOD A: Current approach (pre-normalize then concat)
    // ═══════════════════════════════════════════════════════════════════════
    fn method_a_pre_normalize_and_concat(
        ffmpeg: &Path,
        ffprobe: &Path,
        files: &[&Path],
        durations: &[f64],
        target_sr: u32,
        test_dir: &Path,
        case_label: &str,
    ) -> (PathBuf, Duration, u64, f64, f64) {
        println!("\n  ┌─ METHOD A: Pre-normalize + Concat ─");
        let start = Instant::now();
        let temp_files: Vec<PathBuf> = files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let out = test_dir.join(format!("method_a_norm_{}_{}.mp4", case_label, i));
                let args = [
                    "-y", "-i", f.to_str().unwrap(),
                    "-c:v", "copy",
                    "-c:a", "aac",
                    "-ar", &target_sr.to_string(),
                    "-af", "aresample=async=1:first_pts=0",
                    out.to_str().unwrap()
                ];
                Command::new(ffmpeg).args(&args).output().expect("Method A normalize failed");
                out
            })
            .collect();

        let list = test_dir.join(format!("method_a_list_{}.txt", case_label));
        let temp_refs: Vec<&Path> = temp_files.iter().map(|p| p.as_path()).collect();
        write_concat_list(&temp_refs, durations, &list);

        let output = test_dir.join(format!("method_a_out_{}.mp4", case_label));
        let concat_args = ["-y", "-f", "concat", "-safe", "0", "-i", &list.to_string_lossy(), "-c", "copy", output.to_str().unwrap()];
        Command::new(ffmpeg).args(&concat_args).output().expect("Method A concat failed");

        let elapsed = start.elapsed();
        let output_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

        // Sum of temp files + output
        let disk_used: u64 = temp_files.iter().map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)).sum::<u64>() + output_size;

        // Cleanup temp files
        for f in &temp_files { std::fs::remove_file(f).ok(); }

        let info = probe_file(ffprobe, &output);
        let v_dur = info.as_ref().and_then(|i| i.video_streams.first().and_then(|s| s.duration)).unwrap_or(0.0);
        let a_dur = info.as_ref().and_then(|i| i.audio_streams.first().and_then(|s| s.duration)).unwrap_or(0.0);

        println!("  │  Runtime: {:.3}s", elapsed.as_secs_f64());
        println!("  │  Disk used: {} bytes", disk_used);
        println!("  │  Output: {} bytes", output_size);
        println!("  │  Duration: video={:.3}s audio={:.3}s diff={:.3}s", v_dur, a_dur, (v_dur - a_dur).abs());
        println!("  └─");

        (output, elapsed, disk_used, v_dur, a_dur)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // METHOD D: Video Copy + Audio Resample Pipeline (corrected)
    // Properly preserves video while resampling audio at merge stage
    // ═══════════════════════════════════════════════════════════════════════
    fn method_d_video_copy_audio_resample(
        ffmpeg: &Path,
        ffprobe: &Path,
        files: &[&Path],
        _durations: &[f64],
        target_sr: u32,
        test_dir: &Path,
        case_label: &str,
    ) -> (PathBuf, Duration, u64, f64, f64) {
        println!("\n  ┌─ METHOD D: Video Copy + Audio Resample (corrected) ─");
        let start = Instant::now();
        let n = files.len();

        let output = test_dir.join(format!("method_d_out_{}.mp4", case_label));

        // Build proper filter_complex that preserves video:
        // [0:v]copy[0v];[1:v]copy[1v];...[N-1:v]copy[N-1v];
        // [0:a]aresample=SR[0a];[1:a]aresample=SR[1a];...[N-1:a]aresample=SR[N-1a];
        // [0v][0a][1v][1a]...concat=n=N:v=1:a=1[outv][outa]
        let mut filter_parts: Vec<String> = Vec::new();

        for i in 0..n {
            filter_parts.push(format!("[{}:v]copy[{}v]", i, i));
            filter_parts.push(format!(
                "[{}:a]aresample={}:osr={}:first_pts=0[{}a]",
                i, target_sr, target_sr, i
            ));
        }

        let mut concat_inputs: Vec<String> = Vec::new();
        for i in 0..n {
            concat_inputs.push(format!("[{}v]", i));
            concat_inputs.push(format!("[{}a]", i));
        }
        filter_parts.push(format!(
            "{}{}concat=n={}:v=1:a=1[outv][outa]",
            concat_inputs.join(""),
            "",
            n
        ));

        let filter_str = filter_parts.join(";");

        let mut cmd_args: Vec<String> = vec!["-y".to_string()];
        for f in files { cmd_args.push("-i".to_string()); cmd_args.push(f.to_string_lossy().into_owned()); }
        cmd_args.push("-filter_complex".to_string());
        cmd_args.push(filter_str.clone());
        cmd_args.push("-map".to_string());
        cmd_args.push("[outv]".to_string());
        cmd_args.push("-map".to_string());
        cmd_args.push("[outa]".to_string());
        cmd_args.push("-c:v".to_string());
        cmd_args.push("copy".to_string());
        cmd_args.push("-c:a".to_string());
        cmd_args.push("aac".to_string());
        cmd_args.push("-shortest".to_string());
        cmd_args.push(output.to_string_lossy().into_owned());
        let cmd_args_ref: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();

        let cmd_str = format!("ffmpeg {}", cmd_args_ref[1..].join(" "));
        let result = Command::new(ffmpeg).args(&cmd_args_ref).output();
        if let Ok(out) = &result {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let lines: Vec<&str> = stderr.lines().collect();
                println!("  │  Method D FFMPEG FAILED");
                println!("  │  Filter: {}", filter_str);
                println!("  │  Command: {}", cmd_str);
                if lines.len() > 10 {
                    println!("  │  Stderr (last 15 lines):");
                    lines.iter().rev().take(15).for_each(|l| println!("  │    {}", l));
                } else {
                    println!("  │  Stderr:");
                    lines.iter().for_each(|l| println!("  │    {}", l));
                }
            } else {
                println!("  │  Method D FFMPEG: SUCCESS");
            }
        } else {
            println!("  │  Method D: Command execution failed: {:?}", result.err());
        }

        let elapsed = start.elapsed();
        let output_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

        let info = probe_file(ffprobe, &output);
        let v_dur = info.as_ref().and_then(|i| i.video_streams.first().and_then(|s| s.duration)).unwrap_or(0.0);
        let a_dur = info.as_ref().and_then(|i| i.audio_streams.first().and_then(|s| s.duration)).unwrap_or(0.0);

        println!("  │  Runtime: {:.3}s", elapsed.as_secs_f64());
        println!("  │  Output: {} bytes", output_size);
        println!("  │  Duration: video={:.3}s audio={:.3}s diff={:.3}s", v_dur, a_dur, (v_dur - a_dur).abs());
        println!("  └─");

        (output, elapsed, output_size, v_dur, a_dur)
    }

    // ═══════════════════════════════════════════════════════════════════════
    // METHOD C: Direct concat with -c copy (baseline - no processing)
    // ═══════════════════════════════════════════════════════════════════════
    fn method_c_direct_concat(
        ffmpeg: &Path,
        ffprobe: &Path,
        files: &[&Path],
        durations: &[f64],
        test_dir: &Path,
        case_label: &str,
    ) -> (PathBuf, Duration, u64, f64, f64) {
        println!("\n  ┌─ METHOD C: Direct Concat (baseline) ─");
        let start = Instant::now();

        let list = test_dir.join(format!("method_c_list_{}.txt", case_label));
        write_concat_list(files, durations, &list);

        let output = test_dir.join(format!("method_c_out_{}.mp4", case_label));
        let args = ["-y", "-f", "concat", "-safe", "0", "-i", &list.to_string_lossy(), "-c", "copy", output.to_str().unwrap()];
        Command::new(ffmpeg).args(&args).output().expect("Method C failed");

        let elapsed = start.elapsed();
        let output_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

        let info = probe_file(ffprobe, &output);
        let v_dur = info.as_ref().and_then(|i| i.video_streams.first().and_then(|s| s.duration)).unwrap_or(0.0);
        let a_dur = info.as_ref().and_then(|i| i.audio_streams.first().and_then(|s| s.duration)).unwrap_or(0.0);

        println!("  │  Runtime: {:.3}s", elapsed.as_secs_f64());
        println!("  │  Output: {} bytes", output_size);
        println!("  │  Duration: video={:.3}s audio={:.3}s diff={:.3}s", v_dur, a_dur, (v_dur - a_dur).abs());
        println!("  └─");

        (output, elapsed, output_size, v_dur, a_dur)
    }

    /// PHASE 2B-B BENCHMARK
    ///
    /// Compare three approaches for handling mismatched sample rates:
    ///
    /// Method A (Current): Pre-normalize each file → concat
    ///   - Full audio decode/re-encode per file
    ///   - Then stream copy concat
    ///
    /// Method B (Proposed): Concat filter + aresample
    ///   - No pre-normalization
    ///   - Resample at merge stage using FFmpeg filter
    ///
    /// Method C (Baseline): Direct concat
    ///   - No processing, just concat streams
    ///   - Known to produce ~9% audio drift
    ///
    /// Measures: Runtime, Disk usage, Output size, A/V sync
    #[tokio::test]
    async fn test_sample_rate_benchmark() {
        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("sr_benchmark");
        std::fs::create_dir_all(&test_dir).unwrap();

        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 2B-B: SAMPLE RATE OPTIMIZATION BENCHMARK                      ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        // ══════════════════════════════════════════════════════════════
        // BENCHMARK CASE 1: 44100 AAC + 48000 AAC (3s + 3s)
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  CASE 1: 44100 AAC (3s) + 48000 AAC (3s)                             ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let f1 = test_dir.join("bench1_44100.mp4");
        let f2 = test_dir.join("bench1_48000.mp4");
        create_test_file(&ffmpeg, &f1, 3, 44100, "File-44100");
        create_test_file(&ffmpeg, &f2, 3, 48000, "File-48000");

        let (out_c, t_c, s_c, v_c, a_c) = method_c_direct_concat(&ffmpeg, &ffprobe, &[&f1, &f2], &[3.0, 3.0], &test_dir, "case1");
        let (out_a, t_a, s_a, v_a, a_a) = method_a_pre_normalize_and_concat(&ffmpeg, &ffprobe, &[&f1, &f2], &[3.0, 3.0], 44100, &test_dir, "case1");
        let (out_d, t_d, s_d, v_d, a_d) = method_d_video_copy_audio_resample(&ffmpeg, &ffprobe, &[&f1, &f2], &[3.0, 3.0], 44100, &test_dir, "case1");

        println!("\n  ── CASE 1 RESULTS ──");
        println!("  {:12} | {:>10} | {:>12} | {:>8} | {:>8} | {}", "Method", "Runtime(s)", "Disk(bytes)", "Video(s)", "Audio(s)", "A/V");
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "Direct(C)", t_c.as_secs_f64(), s_c, v_c, a_c, if (v_c-a_c).abs()<0.1 { "SYNC" } else { "DRIFT" });
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "PreNorm+Concat(A)", t_a.as_secs_f64(), s_a, v_a, a_a, if (v_a-a_a).abs()<0.1 { "SYNC" } else { "DRIFT" });
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "Video+Audio(D)", t_d.as_secs_f64(), s_d, v_d, a_d, if (v_d-a_d).abs()<0.1 { "SYNC" } else { "DRIFT" });

        // Cleanup
        std::fs::remove_file(&out_c).ok();
        std::fs::remove_file(&out_a).ok();
        std::fs::remove_file(&out_d).ok();

        // ══════════════════════════════════════════════════════════════
        // BENCHMARK CASE 2: 44100 + 48000 + 44100 (2s × 3)
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  CASE 2: 44100 + 48000 + 44100 (2s × 3, alternating)                  ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let f3 = test_dir.join("bench2_44100_a.mp4");
        let f4 = test_dir.join("bench2_48000.mp4");
        let f5 = test_dir.join("bench2_44100_b.mp4");
        create_test_file(&ffmpeg, &f3, 2, 44100, "File-44100-A");
        create_test_file(&ffmpeg, &f4, 2, 48000, "File-48000");
        create_test_file(&ffmpeg, &f5, 2, 44100, "File-44100-B");

        let (out_c, t_c, s_c, v_c, a_c) = method_c_direct_concat(&ffmpeg, &ffprobe, &[&f3, &f4, &f5], &[2.0, 2.0, 2.0], &test_dir, "case2");
        let (out_a, t_a, s_a, v_a, a_a) = method_a_pre_normalize_and_concat(&ffmpeg, &ffprobe, &[&f3, &f4, &f5], &[2.0, 2.0, 2.0], 44100, &test_dir, "case2");
        let (out_d, t_d, s_d, v_d, a_d) = method_d_video_copy_audio_resample(&ffmpeg, &ffprobe, &[&f3, &f4, &f5], &[2.0, 2.0, 2.0], 44100, &test_dir, "case2");

        println!("\n  ── CASE 2 RESULTS ──");
        println!("  {:12} | {:>10} | {:>12} | {:>8} | {:>8} | {}", "Method", "Runtime(s)", "Disk(bytes)", "Video(s)", "Audio(s)", "A/V");
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "Direct(C)", t_c.as_secs_f64(), s_c, v_c, a_c, if (v_c-a_c).abs()<0.1 { "SYNC" } else { "DRIFT" });
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "PreNorm+Concat(A)", t_a.as_secs_f64(), s_a, v_a, a_a, if (v_a-a_a).abs()<0.1 { "SYNC" } else { "DRIFT" });
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "Video+Audio(D)", t_d.as_secs_f64(), s_d, v_d, a_d, if (v_d-a_d).abs()<0.1 { "SYNC" } else { "DRIFT" });

        // Cleanup
        std::fs::remove_file(&out_c).ok();
        std::fs::remove_file(&out_a).ok();
        std::fs::remove_file(&out_d).ok();

        // ══════════════════════════════════════════════════════════════
        // BENCHMARK CASE 3: 44100 + 48000 (10s + 10s) — longer stress test
        // ══════════════════════════════════════════════════════════════
        println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
        println!("║  CASE 3: 44100 (10s) + 48000 (10s) — stress test                     ║");
        println!("╚═══════════════════════════════════════════════════════════════════════╝");

        let f6 = test_dir.join("bench3_44100_10s.mp4");
        let f7 = test_dir.join("bench3_48000_10s.mp4");
        create_test_file(&ffmpeg, &f6, 10, 44100, "File-44100-10s");
        create_test_file(&ffmpeg, &f7, 10, 48000, "File-48000-10s");

        let (out_c, t_c, s_c, v_c, a_c) = method_c_direct_concat(&ffmpeg, &ffprobe, &[&f6, &f7], &[10.0, 10.0], &test_dir, "case3");
        let (out_a, t_a, s_a, v_a, a_a) = method_a_pre_normalize_and_concat(&ffmpeg, &ffprobe, &[&f6, &f7], &[10.0, 10.0], 44100, &test_dir, "case3");
        let (out_d, t_d, s_d, v_d, a_d) = method_d_video_copy_audio_resample(&ffmpeg, &ffprobe, &[&f6, &f7], &[10.0, 10.0], 44100, &test_dir, "case3");

        println!("\n  ── CASE 3 RESULTS ──");
        println!("  {:12} | {:>10} | {:>12} | {:>8} | {:>8} | {}", "Method", "Runtime(s)", "Disk(bytes)", "Video(s)", "Audio(s)", "A/V");
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "Direct(C)", t_c.as_secs_f64(), s_c, v_c, a_c, if (v_c-a_c).abs()<0.1 { "SYNC" } else { "DRIFT" });
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "PreNorm+Concat(A)", t_a.as_secs_f64(), s_a, v_a, a_a, if (v_a-a_a).abs()<0.1 { "SYNC" } else { "DRIFT" });
        println!("  {:12} | {:>10.3} | {:>12} | {:>8.3} | {:>8.3} | {}", "Video+Audio(D)", t_d.as_secs_f64(), s_d, v_d, a_d, if (v_d-a_d).abs()<0.1 { "SYNC" } else { "DRIFT" });

        // Cleanup
        std::fs::remove_file(&out_c).ok();
        std::fs::remove_file(&out_a).ok();
        std::fs::remove_file(&out_d).ok();

        // ══════════════════════════════════════════════════════════════
        // SUMMARY
        // ══════════════════════════════════════════════════════════════
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  BENCHMARK SUMMARY                                                  ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");
        println!("\n  Method A (Pre-norm + concat): Full audio transcode per file");
        println!("  Method D (Video Copy + Audio Resample): Resample at merge stage (no pre-norm)");
        println!("  Method C (Direct concat): No processing — baseline");

        // Cleanup test files
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Phase 2B-B benchmark complete.");
    }
}