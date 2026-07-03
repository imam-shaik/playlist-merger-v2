#[cfg(test)]
mod pts_dts_timeline_audit {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_file(ffmpeg: &Path, path: &Path, duration_sec: u32, sample_rate: u32, video_codec: &str) -> bool {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=1920x1080:rate=30", duration_sec),
                "-f", "lavfi",
                "-i", &format!("sine=frequency=440:sample_rate={}", sample_rate),
                "-c:v", video_codec,
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

    #[allow(dead_code)]
    fn get_packets(ffprobe: &Path, file: &Path) -> Vec<(i64, i64, String, Option<i64>)> {
        // (pts, dts, codec_type, size)
        let args = ["-v", "quiet", "-print_format", "json", "-show_packets", file.to_str().unwrap()];
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
                (pts, dts, ct, p.pointer("/size").and_then(|v| v.as_i64()))
            })
            .collect()
    }

    /// Get the concat demuxer's packet structure (with metadata)
    /// Uses `-show_entries packet=stream_index` to map packets to streams
    fn get_output_packets_with_stream(ffprobe: &Path, file: &Path) -> Vec<(i64, i64, String, i64)> {
        // (pts, dts, codec_type, stream_index)
        let args = ["-v", "quiet", "-print_format", "json", "-show_packets", "-show_entries", "packet=stream_index,pts,dts,codec_type", file.to_str().unwrap()];
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
                (pts, dts, ct, si)
            })
            .collect()
    }

    /// Check if there are duplicate / overlapping PTS values at a boundary
    /// A "boundary" is the expected end of file N (e.g., 5s, 10s)
    fn check_boundary_continuity(
        name: &str,
        packets: &[(i64, i64, String, i64)],
        boundary_seconds: f64,
        video_timebase: f64,
        audio_timebase: f64,
    ) {
        // Find video and audio packets near the boundary
        let boundary_v = (boundary_seconds / video_timebase) as i64;
        let boundary_a = (boundary_seconds / audio_timebase) as i64;

        println!("\n  Boundary at {:.1}s (V t={}, A t={})", boundary_seconds, boundary_v, boundary_a);

        // Find last video packet BEFORE boundary and first video packet AT/AFTER
        let mut last_v_before: Option<&(i64, i64, String, i64)> = None;
        let mut first_v_at_after: Option<&(i64, i64, String, i64)> = None;
        let mut last_a_before: Option<&(i64, i64, String, i64)> = None;
        let mut first_a_at_after: Option<&(i64, i64, String, i64)> = None;

        for p in packets {
            match p.2.as_str() {
                "video" => {
                    if p.0 < boundary_v {
                        last_v_before = Some(p);
                    } else if first_v_at_after.is_none() && p.0 >= boundary_v {
                        first_v_at_after = Some(p);
                    }
                }
                "audio" => {
                    if p.0 < boundary_a {
                        last_a_before = Some(p);
                    } else if first_a_at_after.is_none() && p.0 >= boundary_a {
                        first_a_at_after = Some(p);
                    }
                }
                _ => {}
            }
        }

        let mut issues = Vec::new();

        if let (Some(lv), Some(fv)) = (last_v_before, first_v_at_after) {
            let v_gap = fv.0 - lv.0;
            let v_gap_s = v_gap as f64 * video_timebase;
            // A normal 30fps file has 1 frame gap of 1/30s = ~0.033s
            // Anything negative = overlap. Anything > 1 frame = jump
            let status = if v_gap < 0 {
                format!("❌ OVERLAP (delta={} ticks, {:.4}s)", v_gap, v_gap_s)
            } else if v_gap == 0 {
                format!("❌ DUPLICATE PTS (delta=0)")
            } else if v_gap_s > 1.0 {
                format!("⚠️ LARGE GAP (delta={} ticks, {:.4}s)", v_gap, v_gap_s)
            } else {
                format!("✅ OK (delta={} ticks, {:.4}s)", v_gap, v_gap_s)
            };
            println!("    Video: last_before pts={}, first_after pts={} | {}", lv.0, fv.0, status);
            if status.contains("❌") || status.contains("⚠️") {
                issues.push(format!("video {}", status));
            }
        } else {
            println!("    Video: missing packet near boundary");
        }

        if let (Some(la), Some(fa)) = (last_a_before, first_a_at_after) {
            let a_gap = fa.0 - la.0;
            let a_gap_s = a_gap as f64 * audio_timebase;
            // 1024 samples / 48000 = 21.3ms normal AAC frame gap
            let status = if a_gap < 0 {
                format!("❌ OVERLAP (delta={} ticks, {:.4}s)", a_gap, a_gap_s)
            } else if a_gap == 0 {
                format!("❌ DUPLICATE PTS (delta=0)")
            } else if a_gap_s > 0.5 {
                format!("⚠️ LARGE GAP (delta={} ticks, {:.4}s)", a_gap, a_gap_s)
            } else {
                format!("✅ OK (delta={} ticks, {:.4}s)", a_gap, a_gap_s)
            };
            println!("    Audio: last_before pts={}, first_after pts={} | {}", la.0, fa.0, status);
            if status.contains("❌") || status.contains("⚠️") {
                issues.push(format!("audio {}", status));
            }
        } else {
            println!("    Audio: missing packet near boundary");
        }

        if issues.is_empty() {
            println!("    [{}] boundary OK", name);
        } else {
            println!("    [{}] boundary ISSUES: {:?}", name, issues);
        }
    }

    /// PHASE 5A: PTS/DTS TIMELINE AUDIT (V2 - Output Analysis)
    ///
    /// Inspect the CONCAT OUTPUT's actual timeline (not per-file).
    /// Look for discontinuities at file boundaries (every N seconds).
    #[tokio::test]
    async fn test_pts_dts_timeline_audit() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 5A: PTS/DTS TIMELINE AUDIT (V2 — Concat Output)           ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("pts_dts_audit_v2");
        std::fs::create_dir_all(&test_dir).unwrap();

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 1: Uniform parameters (baseline)
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 1: UNIFORM PARAMETERS (BASELINE)]");
        let n = 5;
        let uniform_files: Vec<PathBuf> = (0..n)
            .map(|i| test_dir.join(format!("uniform_{}.mp4", i)))
            .collect();
        for (i, f) in uniform_files.iter().enumerate() {
            create_test_file(&ffmpeg, f, 5, 48000, "libx264");
            println!("  File {}: CREATED (libx264 48000Hz 5s)", i);
        }
        let uniform_list = test_dir.join("uniform_list.txt");
        let uniform_content: String = (0..n)
            .map(|i| format!("file '{}'\nduration 5", uniform_files[i].to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&uniform_list, &uniform_content).unwrap();
        let uniform_output = test_dir.join("uniform_output.mp4");
        let _ = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &uniform_list.to_string_lossy(), "-c", "copy", uniform_output.to_str().unwrap()])
            .output();

        let uniform_packets = get_output_packets_with_stream(&ffprobe, &uniform_output);
        println!("  Concat output: {} total packets", uniform_packets.len());
        // Boundaries at 5s, 10s, 15s, 20s
        for b in 1..n {
            check_boundary_continuity("uniform", &uniform_packets, (b * 5) as f64, 1.0/12800.0, 1.0/48000.0);
        }

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 2: Mixed sample rates
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 2: MIXED SAMPLE RATES (44100/48000)]");
        let m = 4;
        let mixed_files: Vec<PathBuf> = (0..m)
            .map(|i| test_dir.join(format!("mixed_{}.mp4", i)))
            .collect();
        let sample_rates = [44100, 48000, 44100, 48000];
        for (i, f) in mixed_files.iter().enumerate() {
            create_test_file(&ffmpeg, f, 5, sample_rates[i], "libx264");
            println!("  File {}: CREATED ({}Hz 5s)", i, sample_rates[i]);
        }
        let mixed_list = test_dir.join("mixed_list.txt");
        let mixed_content: String = (0..m)
            .map(|i| format!("file '{}'\nduration 5", mixed_files[i].to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&mixed_list, &mixed_content).unwrap();
        let mixed_output = test_dir.join("mixed_output.mp4");
        let _ = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &mixed_list.to_string_lossy(), "-c", "copy", mixed_output.to_str().unwrap()])
            .output();

        let mixed_packets = get_output_packets_with_stream(&ffprobe, &mixed_output);
        println!("  Concat output: {} total packets", mixed_packets.len());
        // Boundaries at 5s, 10s, 15s
        for b in 1..m {
            // Audio packets have mixed timebases. Use ffprobe to detect
            // For 44100Hz audio, timebase is 1/44100. For 48000Hz, 1/48000
            // We'll use a generous timebase here and check manually
            check_boundary_continuity("mixed_sr", &mixed_packets, (b * 5) as f64, 1.0/12800.0, 1.0/48000.0);
        }

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 3: Mixed codecs
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 3: MIXED CODECS (H264/H265)]");
        let c = 4;
        let codec_files: Vec<PathBuf> = (0..c)
            .map(|i| test_dir.join(format!("codec_{}.mp4", i)))
            .collect();
        let codecs = ["libx264", "libx265", "libx264", "libx265"];
        for (i, f) in codec_files.iter().enumerate() {
            create_test_file(&ffmpeg, f, 5, 48000, codecs[i]);
            println!("  File {}: CREATED ({} 5s)", i, codecs[i]);
        }
        let codec_list = test_dir.join("codec_list.txt");
        let codec_content: String = (0..c)
            .map(|i| format!("file '{}'\nduration 5", codec_files[i].to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&codec_list, &codec_content).unwrap();
        let codec_output = test_dir.join("codec_output.mp4");
        let _ = Command::new(&ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &codec_list.to_string_lossy(), "-c", "copy", codec_output.to_str().unwrap()])
            .output();

        let codec_packets = get_output_packets_with_stream(&ffprobe, &codec_output);
        println!("  Concat output: {} total packets", codec_packets.len());
        for b in 1..c {
            check_boundary_continuity("mixed_codec", &codec_packets, (b * 5) as f64, 1.0/12800.0, 1.0/48000.0);
        }

        // ══════════════════════════════════════════════════════════════
        // MONOTONICITY CHECK: For each scenario, verify PTS is monotonic
        // ══════════════════════════════════════════════════════════════
        println!("\n[CHECKING PTS MONOTONICITY IN CONCAT OUTPUTS]");

        for (name, packets) in &[
            ("Uniform", &uniform_packets),
            ("Mixed SR", &mixed_packets),
            ("Mixed Codec", &codec_packets),
        ] {
            let mut v_prev: Option<i64> = None;
            let mut a_prev: Option<i64> = None;
            let mut v_backwards: Vec<(usize, i64, i64)> = Vec::new();
            let mut a_backwards: Vec<(usize, i64, i64)> = Vec::new();
            let mut v_duplicates: Vec<(usize, i64)> = Vec::new();
            let mut a_duplicates: Vec<(usize, i64)> = Vec::new();

            for (i, p) in packets.iter().enumerate() {
                if p.2 == "video" {
                    if let Some(prev) = v_prev {
                        if p.0 < prev { v_backwards.push((i, prev, p.0)); }
                        else if p.0 == prev { v_duplicates.push((i, p.0)); }
                    }
                    v_prev = Some(p.0);
                } else if p.2 == "audio" {
                    if let Some(prev) = a_prev {
                        if p.0 < prev { a_backwards.push((i, prev, p.0)); }
                        else if p.0 == prev { a_duplicates.push((i, p.0)); }
                    }
                    a_prev = Some(p.0);
                }
            }

            println!("\n  {}:", name);
            println!("    Video backwards jumps: {}", v_backwards.len());
            if !v_backwards.is_empty() {
                for (idx, prev, curr) in v_backwards.iter().take(5) {
                    println!("      packet {}: {} → {} (delta={})", idx, prev, curr, curr - prev);
                }
            }
            println!("    Video duplicate PTS: {}", v_duplicates.len());
            if !v_duplicates.is_empty() {
                for (idx, pts) in v_duplicates.iter().take(5) {
                    println!("      packet {}: pts={}", idx, pts);
                }
            }
            println!("    Audio backwards jumps: {}", a_backwards.len());
            if !a_backwards.is_empty() {
                for (idx, prev, curr) in a_backwards.iter().take(5) {
                    println!("      packet {}: {} → {} (delta={})", idx, prev, curr, curr - prev);
                }
            }
            println!("    Audio duplicate PTS: {}", a_duplicates.len());
            if !a_duplicates.is_empty() {
                for (idx, pts) in a_duplicates.iter().take(5) {
                    println!("      packet {}: pts={}", idx, pts);
                }
            }
        }

        // ══════════════════════════════════════════════════════════════
        // DURATION ANALYSIS: Output duration vs expected
        // ══════════════════════════════════════════════════════════════
        println!("\n[CHECK: OUTPUT DURATIONS]");

        for (name, output) in &[
            ("Uniform (5 files × 5s = 25s)", &uniform_output),
            ("Mixed SR (4 files × 5s = 20s)", &mixed_output),
            ("Mixed Codec (4 files × 5s = 20s)", &codec_output),
        ] {
            let dur_args = ["-v", "quiet", "-print_format", "json", "-show_format", output.to_str().unwrap()];
            let dur_output = Command::new(&ffprobe).args(&dur_args).output().ok();
            let dur = match dur_output {
                Some(o) => {
                    let parsed: Option<serde_json::Value> = serde_json::from_slice(&o.stdout).ok();
                    parsed
                        .as_ref()
                        .and_then(|j| j.pointer("/format/duration"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0)
                }
                None => 0.0,
            };
            println!("  {}: actual={:.3}s", name, dur);
        }

        // ══════════════════════════════════════════════════════════════
        // TIMEBASE DETECTION: For mixed SR, audio packets may have different timebase
        // ══════════════════════════════════════════════════════════════
        println!("\n[CHECK: AUDIO TIMEBASE IN MIXED SR OUTPUT]");

        // Probe streams in mixed output
        let stream_args = ["-v", "quiet", "-print_format", "json", "-show_streams", mixed_output.to_str().unwrap()];
        let stream_output = Command::new(&ffprobe).args(&stream_args).output().ok();
        if let Some(o) = stream_output {
            if let Ok(info) = serde_json::from_slice::<serde_json::Value>(&o.stdout) {
                if let Some(streams) = info.pointer("/streams").and_then(|v| v.as_array()) {
                    for s in streams {
                        let idx = s.pointer("/index").and_then(|v| v.as_i64()).unwrap_or(0);
                        let ct = s.pointer("/codec_type").and_then(|v| v.as_str()).unwrap_or("?");
                        let sr = s.pointer("/sample_rate").and_then(|v| v.as_str()).unwrap_or("?");
                        let tbn = s.pointer("/time_base").and_then(|v| v.as_str()).unwrap_or("?");
                        println!("  Stream {}: type={} sr={} tbn={}", idx, ct, sr, tbn);
                    }
                }
            }
        }

        // Cleanup
        for f in &uniform_files { std::fs::remove_file(f).ok(); }
        for f in &mixed_files { std::fs::remove_file(f).ok(); }
        for f in &codec_files { std::fs::remove_file(f).ok(); }
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Phase 5A PTS/DTS Timeline Audit (V2) complete.");
    }
}