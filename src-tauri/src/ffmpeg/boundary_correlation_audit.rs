#[cfg(test)]
mod boundary_correlation_audit {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_file(ffmpeg: &Path, path: &Path, duration_sec: u32, video_codec: &str) -> bool {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=1920x1080:rate=30", duration_sec),
                "-f", "lavfi",
                "-i", "sine=frequency=440:sample_rate=48000",
                "-c:v", video_codec,
                "-preset", "ultrafast",
                "-c:a", "aac",
                "-ar", "48000",
                "-t", &duration_sec.to_string(),
                path.to_str().unwrap()
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn concat_files(ffmpeg: &Path, list_path: &Path, output: &Path, use_reencode: bool) -> bool {
        let list_str = list_path.to_string_lossy().into_owned();
        let output_str = output.to_str().unwrap().to_string();
        let mut args: Vec<String> = vec!["-y".into(), "-f".into(), "concat".into(), "-safe".into(), "0".into(), "-i".into(), list_str];
        if use_reencode {
            args.extend_from_slice(&["-c:v".into(), "libx264".into(), "-preset".into(), "ultrafast".into(), "-c:a".into(), "aac".into(), "-ar".into(), "48000".into()]);
        } else {
            args.push("-c".into());
            args.push("copy".into());
        }
        args.push(output_str);
        Command::new(ffmpeg).args(&args).output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// (pts, dts, frame_type, key_frame)
    fn get_frames(ffprobe: &Path, file: &Path) -> Vec<(i64, i64, String, i64)> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-select_streams", "v:0",
            "-show_frames",
            "-show_entries", "frame=pts,dts,pict_type,key_frame",
            file.to_str().unwrap()
        ];
        let output = match Command::new(ffprobe).args(&args).output() {
            Ok(o) if o.status.success() => o.stdout,
            _ => return vec![],
        };
        let info: serde_json::Value = match serde_json::from_slice(&output) {
            Ok(v) => v,
            _ => return vec![],
        };
        let arr = match info.pointer("/frames").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => return vec![],
        };
        arr.iter()
            .map(|f| {
                let pts = f.pointer("/pts").and_then(|v| v.as_i64()).unwrap_or(0);
                let dts = f.pointer("/dts").and_then(|v| v.as_i64()).unwrap_or(0);
                let pt = f.pointer("/pict_type").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                let kf = f.pointer("/key_frame").and_then(|v| v.as_i64()).unwrap_or(0);
                (pts, dts, pt, kf)
            })
            .collect()
    }

    /// Get packet info (better for PTS/DTS analysis)
    fn get_packets_with_index(ffprobe: &Path, file: &Path) -> Vec<(i64, i64, String, i64, i64)> {
        // (pts, dts, codec_type, stream_index, size)
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-show_packets",
            "-show_entries", "packet=stream_index,pts,dts,codec_type,size",
            file.to_str().unwrap()
        ];
        let output = match Command::new(ffprobe).args(&args).output() {
            Ok(o) if o.status.success() => o.stdout,
            _ => return vec![],
        };
        let info: serde_json::Value = match serde_json::from_slice(&output) {
            Ok(v) => v,
            _ => return vec![],
        };
        let arr = match info.pointer("/packets").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => return vec![],
        };
        arr.iter()
            .map(|p| {
                let pts = p.pointer("/pts").and_then(|v| v.as_i64()).unwrap_or(0);
                let dts = p.pointer("/dts").and_then(|v| v.as_i64()).unwrap_or(0);
                let ct = p.pointer("/codec_type").and_then(|v| v.as_str()).unwrap_or("?").to_string();
                let si = p.pointer("/stream_index").and_then(|v| v.as_i64()).unwrap_or(0);
                let sz = p.pointer("/size").and_then(|v| v.as_i64()).unwrap_or(0);
                (pts, dts, ct, si, sz)
            })
            .collect()
    }

    /// Perform a real SEEK to the boundary and check for decoder errors
    /// Returns: (succeeded, error_count, error_messages)
    fn seek_test(ffmpeg: &Path, file: &Path, seek_seconds: f64) -> (bool, usize, Vec<String>) {
        // Use ffmpeg to seek to the specified timestamp and decode a few frames
        // Capture stderr for errors
        let output = Command::new(ffmpeg)
            .args(&[
                "-v", "error",
                "-ss", &seek_seconds.to_string(),
                "-i", file.to_str().unwrap(),
                "-frames:v", "5",
                "-f", "null",
                "-"
            ])
            .output();

        let output = match output {
            Ok(o) => o,
            Err(_) => return (false, 0, vec!["FFMPEG_FAILED_TO_RUN".to_string()]),
        };

        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let error_count = stderr.lines()
            .filter(|l| l.contains("error") || l.contains("Error") || l.contains("missing") || l.contains("Invalid"))
            .count();
        let error_messages: Vec<String> = stderr.lines()
            .filter(|l| l.contains("error") || l.contains("Error") || l.contains("missing") || l.contains("Invalid"))
            .map(|s| s.to_string())
            .collect();

        (output.status.success() && error_count == 0, error_count, error_messages)
    }

    /// PHASE 5C: BOUNDARY CORRELATION AUDIT
    ///
    /// Determine which finding actually predicts corruption:
    /// A. Non-keyframe start (P-frame at boundary)
    /// B. Large PTS jump (>1000 ticks)
    /// C. Codec transition
    ///
    /// Uses REAL SEEK TESTS to measure decoder pass/fail.
    #[tokio::test]
    async fn test_boundary_correlation_audit() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 5C: BOUNDARY CORRELATION AUDIT                              ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("boundary_correlation_audit");
        std::fs::create_dir_all(&test_dir).unwrap();

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 1: Uniform H264 (CONTROL)
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 1: UNIFORM H264 (CONTROL - no codec transitions)]");
        let n1 = 5;
        let uniform_files: Vec<PathBuf> = (0..n1)
            .map(|i| test_dir.join(format!("uni_{}.mp4", i)))
            .collect();
        for f in &uniform_files {
            create_test_file(&ffmpeg, f, 5, "libx264");
        }
        let uniform_list = test_dir.join("uni_list.txt");
        let uniform_content: String = (0..n1)
            .map(|i| format!("file '{}'\nduration 5", uniform_files[i].to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&uniform_list, &uniform_content).unwrap();
        let uniform_output = test_dir.join("uni_output.mp4");
        concat_files(&ffmpeg, &uniform_list, &uniform_output, false);

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 2: Mixed H264↔H265 (SUSPECT)
        // ══════════════════════════════════════════════════════════════
        println!("[SCENARIO 2: MIXED H264↔H265 (SUSPECT - has codec transitions)]");
        let n2 = 4;
        let mixed_files: Vec<PathBuf> = (0..n2)
            .map(|i| test_dir.join(format!("mix_{}.mp4", i)))
            .collect();
        let codecs = ["libx264", "libx265", "libx264", "libx265"];
        for (i, f) in mixed_files.iter().enumerate() {
            create_test_file(&ffmpeg, f, 5, codecs[i]);
        }
        let mixed_list = test_dir.join("mix_list.txt");
        let mixed_content: String = (0..n2)
            .map(|i| format!("file '{}'\nduration 5", mixed_files[i].to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&mixed_list, &mixed_content).unwrap();
        let mixed_output = test_dir.join("mix_output.mp4");
        concat_files(&ffmpeg, &mixed_list, &mixed_output, false);

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 3: Mixed with re-encode
        // ══════════════════════════════════════════════════════════════
        println!("[SCENARIO 3: MIXED H264↔H265 (RE-ENCODE)]");
        let mixed_re_output = test_dir.join("mix_re_output.mp4");
        concat_files(&ffmpeg, &mixed_list, &mixed_re_output, true);

        // ══════════════════════════════════════════════════════════════
        // COLLECT BOUNDARY DATA
        // ══════════════════════════════════════════════════════════════
        #[derive(Debug)]
        struct BoundaryRecord {
            #[allow(dead_code)]
            scenario: String,
            boundary_idx: usize,
            boundary_time: f64,
            codec_from: String,
            codec_to: String,
            first_frame_type: String,
            is_keyframe: bool,
            pts_delta: i64,
            pts_delta_seconds: f64,
            seek_test_pass: bool,
            error_count: usize,
            error_sample: String,
        }

        let mut all_records: Vec<BoundaryRecord> = Vec::new();

        // Process each scenario
        for (scenario_name, files, output, codec_labels) in &[
            ("Uniform H264", &uniform_files, &uniform_output, &vec!["libx264".to_string(); 5]),
            ("Mixed H264↔H265", &mixed_files, &mixed_output, &codecs.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
        ] {
            let frames = get_frames(&ffprobe, output);
            let packets = get_packets_with_index(&ffprobe, output);
            let n = files.len();
            println!("\n[ANALYZING: {}]", scenario_name);
            println!("  Frames: {}, Packets: {}", frames.len(), packets.len());

            for b in 0..(n - 1) {
                let boundary_time = ((b + 1) * 5) as f64;
                let boundary_pts_video = (boundary_time * 12800.0) as i64;

                // Find first video frame at/after boundary
                let first_frame_after = frames.iter().find(|f| f.0 >= boundary_pts_video);
                let last_frame_before = frames.iter().rev().find(|f| f.0 < boundary_pts_video);

                // Find codec transition
                let codec_from = codec_labels[b].clone();
                let codec_to = codec_labels[b+1].clone();
                let _codec_match = codec_from == codec_to;

                // Calculate PTS delta
                let pts_delta = if let (Some(lf), Some(ff)) = (last_frame_before, first_frame_after) {
                    ff.0 - lf.0
                } else { 0 };

                // Get first frame type
                let (first_type, is_kf) = if let Some(ff) = first_frame_after {
                    (ff.2.clone(), ff.3 == 1)
                } else { ("?".to_string(), false) };

                // Perform SEEK TEST to the boundary
                let (pass, err_count, err_msgs) = seek_test(&ffmpeg, output, boundary_time - 0.5);
                let err_sample = err_msgs.first().cloned().unwrap_or_default();

                let record = BoundaryRecord {
                    scenario: scenario_name.to_string(),
                    boundary_idx: b + 1,
                    boundary_time,
                    codec_from,
                    codec_to,
                    first_frame_type: first_type,
                    is_keyframe: is_kf,
                    pts_delta,
                    pts_delta_seconds: pts_delta as f64 / 12800.0,
                    seek_test_pass: pass,
                    error_count: err_count,
                    error_sample: err_sample.clone(),
                };

                println!("  B{}@{:.0}s [{}→{}]: type={} kf={} Δpts={} ({:.3}s) | seek={} errors={}",
                    b+1, boundary_time, record.codec_from, record.codec_to,
                    record.first_frame_type, record.is_keyframe,
                    record.pts_delta, record.pts_delta_seconds,
                    if record.seek_test_pass { "✅" } else { "❌" },
                    record.error_count);

                if !record.seek_test_pass && !record.error_sample.is_empty() {
                    println!("     └─ error: {}", record.error_sample);
                }

                all_records.push(record);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // CORRELATION ANALYSIS
        // ══════════════════════════════════════════════════════════════
        println!("\n[CORRELATION ANALYSIS]");

        let total = all_records.len();
        let failures: Vec<&BoundaryRecord> = all_records.iter().filter(|r| !r.seek_test_pass).collect();
        let passes: Vec<&BoundaryRecord> = all_records.iter().filter(|r| r.seek_test_pass).collect();

        println!("\n  Total boundaries tested: {}", total);
        println!("  Seek pass: {}, Seek fail: {}", passes.len(), failures.len());

        // ══════════════════════════════════════════════════════════════
        // HYPOTHESIS A: P-frame at boundary predicts failure
        // ══════════════════════════════════════════════════════════════
        println!("\n  [HYPOTHESIS A: P-frame at boundary predicts failure]");
        let p_frame_count = all_records.iter().filter(|r| !r.is_keyframe).count();
        let p_frame_failures = all_records.iter().filter(|r| !r.is_keyframe && !r.seek_test_pass).count();
        let p_frame_passes = all_records.iter().filter(|r| !r.is_keyframe && r.seek_test_pass).count();
        let kf_count = all_records.iter().filter(|r| r.is_keyframe).count();
        let kf_failures = all_records.iter().filter(|r| r.is_keyframe && !r.seek_test_pass).count();
        let kf_passes = all_records.iter().filter(|r| r.is_keyframe && r.seek_test_pass).count();

        println!("    P-frame boundaries: {} ({} failed, {} passed)", p_frame_count, p_frame_failures, p_frame_passes);
        println!("    Keyframe boundaries: {} ({} failed, {} passed)", kf_count, kf_failures, kf_passes);
        if p_frame_count > 0 {
            println!("    P-frame failure rate: {:.1}%", 100.0 * p_frame_failures as f64 / p_frame_count as f64);
        }
        if kf_count > 0 {
            println!("    Keyframe failure rate: {:.1}%", 100.0 * kf_failures as f64 / kf_count as f64);
        }

        // ══════════════════════════════════════════════════════════════
        // HYPOTHESIS B: Large PTS jump (>1000 ticks) predicts failure
        // ══════════════════════════════════════════════════════════════
        println!("\n  [HYPOTHESIS B: Large PTS jump (>1000 ticks) predicts failure]");
        let large_jump_threshold = 1000i64;
        let large_count = all_records.iter().filter(|r| r.pts_delta.abs() > large_jump_threshold).count();
        let large_failures = all_records.iter().filter(|r| r.pts_delta.abs() > large_jump_threshold && !r.seek_test_pass).count();
        let large_passes = all_records.iter().filter(|r| r.pts_delta.abs() > large_jump_threshold && r.seek_test_pass).count();
        let small_count = all_records.iter().filter(|r| r.pts_delta.abs() <= large_jump_threshold).count();
        let small_failures = all_records.iter().filter(|r| r.pts_delta.abs() <= large_jump_threshold && !r.seek_test_pass).count();
        let small_passes = all_records.iter().filter(|r| r.pts_delta.abs() <= large_jump_threshold && r.seek_test_pass).count();

        println!("    Large jump boundaries: {} ({} failed, {} passed)", large_count, large_failures, large_passes);
        println!("    Small jump boundaries: {} ({} failed, {} passed)", small_count, small_failures, small_passes);
        if large_count > 0 {
            println!("    Large jump failure rate: {:.1}%", 100.0 * large_failures as f64 / large_count as f64);
        }
        if small_count > 0 {
            println!("    Small jump failure rate: {:.1}%", 100.0 * small_failures as f64 / small_count as f64);
        }

        // ══════════════════════════════════════════════════════════════
        // HYPOTHESIS C: Codec transition predicts failure
        // ══════════════════════════════════════════════════════════════
        println!("\n  [HYPOTHESIS C: Codec transition predicts failure]");
        let transition_count = all_records.iter().filter(|r| r.codec_from != r.codec_to).count();
        let transition_failures = all_records.iter().filter(|r| r.codec_from != r.codec_to && !r.seek_test_pass).count();
        let transition_passes = all_records.iter().filter(|r| r.codec_from != r.codec_to && r.seek_test_pass).count();
        let same_count = all_records.iter().filter(|r| r.codec_from == r.codec_to).count();
        let same_failures = all_records.iter().filter(|r| r.codec_from == r.codec_to && !r.seek_test_pass).count();
        let same_passes = all_records.iter().filter(|r| r.codec_from == r.codec_to && r.seek_test_pass).count();

        println!("    Codec transition boundaries: {} ({} failed, {} passed)", transition_count, transition_failures, transition_passes);
        println!("    Same codec boundaries: {} ({} failed, {} passed)", same_count, same_failures, same_passes);
        if transition_count > 0 {
            println!("    Transition failure rate: {:.1}%", 100.0 * transition_failures as f64 / transition_count as f64);
        }
        if same_count > 0 {
            println!("    Same codec failure rate: {:.1}%", 100.0 * same_failures as f64 / same_count as f64);
        }

        // ══════════════════════════════════════════════════════════════
        // FINAL VERDICT TABLE
        // ══════════════════════════════════════════════════════════════
        println!("\n[FINAL VERDICT: PREDICTOR vs ACTUAL CORRUPTION]");
        println!("\n  Boundary | B-Type   | FType | KF? | PTS Δ    | Decoder");
        println!("  ---------|----------|-------|-----|----------|--------");
        for r in &all_records {
            let btype = if r.codec_from == r.codec_to { "SAME" } else { "DIFF" };
            let result = if r.seek_test_pass { "✅ OK" } else { "❌ FAIL" };
            let kf_marker = if r.is_keyframe { "Y" } else { "N" };
            println!("  B{}@{:.0}s    | {}({})  | {}     | {}   | {:>8} | {}",
                r.boundary_idx, r.boundary_time, btype,
                format!("{}→{}", r.codec_from.replace("libx", ""), r.codec_to.replace("libx", "")),
                r.first_frame_type, kf_marker, r.pts_delta, result);
        }

        // ══════════════════════════════════════════════════════════════
        // STRONGEST PREDICTOR
        // ══════════════════════════════════════════════════════════════
        println!("\n[STRONGEST PREDICTOR]");

        // Calculate failure rates for each hypothesis
        let a_fail_rate = if p_frame_count > 0 { p_frame_failures as f64 / p_frame_count as f64 } else { 0.0 };
        let b_fail_rate = if large_count > 0 { large_failures as f64 / large_count as f64 } else { 0.0 };
        let c_fail_rate = if transition_count > 0 { transition_failures as f64 / transition_count as f64 } else { 0.0 };

        let a_kf_fail_rate = if kf_count > 0 { kf_failures as f64 / kf_count as f64 } else { 0.0 };
        let b_small_fail_rate = if small_count > 0 { small_failures as f64 / small_count as f64 } else { 0.0 };
        let c_same_fail_rate = if same_count > 0 { same_failures as f64 / same_count as f64 } else { 0.0 };

        // The strongest predictor has highest differential failure rate
        println!("\n  Differential failure rates (high = better predictor):");
        println!("    A (P-frame vs KF):      {:.2} - {:.2} = {:.2}", a_fail_rate, a_kf_fail_rate, a_fail_rate - a_kf_fail_rate);
        println!("    B (Large vs Small PTS): {:.2} - {:.2} = {:.2}", b_fail_rate, b_small_fail_rate, b_fail_rate - b_small_fail_rate);
        println!("    C (Diff vs Same codec): {:.2} - {:.2} = {:.2}", c_fail_rate, c_same_fail_rate, c_fail_rate - c_same_fail_rate);

        let diffs = [
            ("P-frame at boundary", a_fail_rate - a_kf_fail_rate),
            ("Large PTS jump", b_fail_rate - b_small_fail_rate),
            ("Codec transition", c_fail_rate - c_same_fail_rate),
        ];
        let strongest = diffs.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap();
        println!("\n  >>> STRONGEST PREDICTOR: {} (differential = {:.2}) <<<", strongest.0, strongest.1);

        // Cleanup
        for f in &uniform_files { std::fs::remove_file(f).ok(); }
        for f in &mixed_files { std::fs::remove_file(f).ok(); }
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Phase 5C Boundary Correlation Audit complete.");
    }
}