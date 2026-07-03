#[cfg(test)]
mod keyframe_boundary_audit {
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

    /// Frame info: (pts, dts, frame_type, key_frame, pkt_size)
    fn get_frames(ffprobe: &Path, file: &Path) -> Vec<(i64, i64, String, i64, Option<i64>)> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-select_streams", "v:0",
            "-show_frames",
            "-show_entries", "frame=pts,dts,pict_type,key_frame,pkt_size,best_effort_timestamp_time",
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
                let sz = f.pointer("/pkt_size").and_then(|v| v.as_i64());
                (pts, dts, pt, kf, sz)
            })
            .collect()
    }

    /// Concat using concat demuxer with `-c copy`
    fn concat_files(ffmpeg: &Path, list_path: &Path, output: &Path) -> bool {
        Command::new(ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy(), "-c", "copy", output.to_str().unwrap()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Concat using concat demuxer with re-encode (forces keyframes at boundaries)
    fn concat_files_reencode(ffmpeg: &Path, list_path: &Path, output: &Path) -> bool {
        Command::new(ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy(),
                    "-c:v", "libx264", "-preset", "ultrafast",
                    "-c:a", "aac", "-ar", "48000",
                    output.to_str().unwrap()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Find frames around a given time boundary in the concat output
    /// Returns (last_frame_before, first_frame_after)
    fn find_boundary_frames(
        frames: &[(i64, i64, String, i64, Option<i64>)],
        boundary_seconds: f64,
        video_timebase: f64,
    ) -> (Option<(i64, i64, String, i64)>, Option<(i64, i64, String, i64)>) {
        let boundary_pts = (boundary_seconds / video_timebase) as i64;

        let mut last_before: Option<(i64, i64, String, i64)> = None;
        let mut first_after: Option<(i64, i64, String, i64)> = None;

        for f in frames {
            let pts = f.0;
            let dts = f.1;
            let pt = &f.2;
            let kf = f.3;
            if pts < boundary_pts {
                last_before = Some((pts, dts, pt.clone(), kf));
            } else if first_after.is_none() && pts >= boundary_pts {
                first_after = Some((pts, dts, pt.clone(), kf));
            }
        }

        (last_before, first_after)
    }

    /// PHASE 5B: KEYFRAME BOUNDARY FORENSICS
    ///
    /// For every boundary where PTS goes backwards:
    /// 1. Identify last keyframe of previous segment
    /// 2. Identify first keyframe of next segment
    /// 3. Extract: PTS, DTS, Frame Type
    /// 4. Determine: Is the first frame after boundary a keyframe?
    #[tokio::test]
    async fn test_keyframe_boundary_audit() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 5B: KEYFRAME BOUNDARY FORENSICS                             ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("keyframe_boundary_audit");
        std::fs::create_dir_all(&test_dir).unwrap();

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 1: Uniform codec (H264) - baseline
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 1: UNIFORM H264 (BASELINE)]");
        let n = 5;
        let uniform_files: Vec<PathBuf> = (0..n)
            .map(|i| test_dir.join(format!("u_{}.mp4", i)))
            .collect();
        for (i, f) in uniform_files.iter().enumerate() {
            create_test_file(&ffmpeg, f, 5, "libx264");
            println!("  File {}: H264 5s", i);
        }
        let uniform_list = test_dir.join("u_list.txt");
        let uniform_content: String = (0..n)
            .map(|i| format!("file '{}'\nduration 5", uniform_files[i].to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&uniform_list, &uniform_content).unwrap();
        let uniform_output = test_dir.join("u_output.mp4");
        concat_files(&ffmpeg, &uniform_list, &uniform_output);

        let uniform_frames = get_frames(&ffprobe, &uniform_output);
        println!("  Concat output: {} frames", uniform_frames.len());

        let mut uniform_keyframe_issues = 0;
        for b in 1..n {
            let boundary_s = (b * 5) as f64;
            let (last_before, first_after) = find_boundary_frames(&uniform_frames, boundary_s, 1.0/12800.0);
            if let (Some(lb), Some(fa)) = (last_before, first_after) {
                let is_kf = fa.3 == 1;
                let pts_delta = fa.0 - lb.0;
                let dts_delta = fa.1 - lb.1;
                let status = if !is_kf { "❌ NOT KEYFRAME" } else { "✅ KEYFRAME" };
                println!("    B{}@{:.0}s: last=[{} pts={} dts={}], first=[{} pts={} dts={}] Δpts={} Δdts={} | {}",
                    b, boundary_s, lb.2, lb.0, lb.1, fa.2, fa.0, fa.1, pts_delta, dts_delta, status);
                if !is_kf { uniform_keyframe_issues += 1; }
            }
        }
        println!("  Uniform: {} keyframe issues", uniform_keyframe_issues);

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 2: Mixed codec (H264/H265) - the suspect
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 2: MIXED CODECS H264↔H265 (-c copy)]");
        let m = 4;
        let mixed_files: Vec<PathBuf> = (0..m)
            .map(|i| test_dir.join(format!("m_{}.mp4", i)))
            .collect();
        let codecs = ["libx264", "libx265", "libx264", "libx265"];
        for (i, f) in mixed_files.iter().enumerate() {
            create_test_file(&ffmpeg, f, 5, codecs[i]);
            println!("  File {}: {} 5s", i, codecs[i]);
        }
        let mixed_list = test_dir.join("m_list.txt");
        let mixed_content: String = (0..m)
            .map(|i| format!("file '{}'\nduration 5", mixed_files[i].to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&mixed_list, &mixed_content).unwrap();
        let mixed_output = test_dir.join("m_output.mp4");
        concat_files(&ffmpeg, &mixed_list, &mixed_output);

        let mixed_frames = get_frames(&ffprobe, &mixed_output);
        println!("  Concat output: {} frames", mixed_frames.len());

        let mut mixed_keyframe_issues = 0;
        let mut mixed_pts_backwards = 0;
        for b in 1..m {
            let boundary_s = (b * 5) as f64;
            let (last_before, first_after) = find_boundary_frames(&mixed_frames, boundary_s, 1.0/12800.0);
            if let (Some(lb), Some(fa)) = (last_before, first_after) {
                let is_kf = fa.3 == 1;
                let pts_delta = fa.0 - lb.0;
                let dts_delta = fa.1 - lb.1;
                let pts_backwards = pts_delta < 0;
                let dts_backwards = dts_delta < 0;
                let mut status = String::new();
                if !is_kf { status.push_str("❌ NOT_KEYFRAME "); mixed_keyframe_issues += 1; }
                else { status.push_str("✅ KEYFRAME "); }
                if pts_backwards { status.push_str("❌ PTS_BACKWARDS "); mixed_pts_backwards += 1; }
                if dts_backwards { status.push_str("❌ DTS_BACKWARDS "); }
                println!("    B{}@{:.0}s: last=[{} pts={} dts={}], first=[{} pts={} dts={}] Δpts={} Δdts={} | {}",
                    b, boundary_s, lb.2, lb.0, lb.1, fa.2, fa.0, fa.1, pts_delta, dts_delta, status.trim());
            }
        }
        println!("  Mixed: {} keyframe issues, {} PTS backwards", mixed_keyframe_issues, mixed_pts_backwards);

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 3: Mixed codec with RE-ENCODE
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 3: MIXED CODECS WITH RE-ENCODE (forces keyframes)]");
        let mixed_re_output = test_dir.join("m_re_output.mp4");
        concat_files_reencode(&ffmpeg, &mixed_list, &mixed_re_output);

        let mixed_re_frames = get_frames(&ffprobe, &mixed_re_output);
        println!("  Concat output: {} frames", mixed_re_frames.len());

        let mut mixed_re_keyframe_issues = 0;
        let mut mixed_re_pts_backwards = 0;
        for b in 1..m {
            let boundary_s = (b * 5) as f64;
            let (last_before, first_after) = find_boundary_frames(&mixed_re_frames, boundary_s, 1.0/12800.0);
            if let (Some(lb), Some(fa)) = (last_before, first_after) {
                let is_kf = fa.3 == 1;
                let pts_delta = fa.0 - lb.0;
                let pts_backwards = pts_delta < 0;
                let mut status = String::new();
                if !is_kf { status.push_str("❌ NOT_KEYFRAME "); mixed_re_keyframe_issues += 1; }
                else { status.push_str("✅ KEYFRAME "); }
                if pts_backwards { status.push_str("❌ PTS_BACKWARDS "); mixed_re_pts_backwards += 1; }
                println!("    B{}@{:.0}s: last=[{} pts={}], first=[{} pts={}] Δpts={} | {}",
                    b, boundary_s, lb.2, lb.0, fa.2, fa.0, pts_delta, status.trim());
            }
        }
        println!("  Mixed (re-encoded): {} keyframe issues, {} PTS backwards", mixed_re_keyframe_issues, mixed_re_pts_backwards);

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 4: Verify first frame of H265 file is a keyframe
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 4: VERIFY FIRST FRAME IS KEYFRAME IN SOURCE FILES]");
        for (i, f) in mixed_files.iter().enumerate() {
            let frames = get_frames(&ffprobe, f);
            if let Some(first) = frames.first() {
                let is_kf = first.3 == 1;
                let status = if is_kf { "✅" } else { "❌" };
                println!("  File {} ({}): first frame type={} key_frame={} pts={} | {}",
                    i, codecs[i], first.2, first.3, first.0, status);
            }
        }

        // ══════════════════════════════════════════════════════════════
        // SCENARIO 5: Long file with many boundaries (simulate real corruption)
        // ══════════════════════════════════════════════════════════════
        println!("\n[SCENARIO 5: LONG FILE WITH 10 BOUNDARIES (simulate production)]");
        let n_long = 10;
        let long_files: Vec<PathBuf> = (0..n_long)
            .map(|i| test_dir.join(format!("long_{}.mp4", i)))
            .collect();
        // Alternate codecs to maximize boundary discontinuities
        for (i, f) in long_files.iter().enumerate() {
            let codec = if i % 2 == 0 { "libx264" } else { "libx265" };
            create_test_file(&ffmpeg, f, 5, codec);
        }
        let long_list = test_dir.join("long_list.txt");
        let long_content: String = (0..n_long)
            .map(|i| format!("file '{}'\nduration 5", long_files[i].to_string_lossy().replace('\\', "/")))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&long_list, &long_content).unwrap();
        let long_output = test_dir.join("long_output.mp4");
        concat_files(&ffmpeg, &long_list, &long_output);

        let long_frames = get_frames(&ffprobe, &long_output);
        println!("  Concat output: {} frames", long_frames.len());

        let mut long_keyframe_issues = 0;
        let mut long_pts_backwards = 0;
        let mut first_problem_boundary: Option<usize> = None;
        for b in 1..n_long {
            let boundary_s = (b * 5) as f64;
            let (last_before, first_after) = find_boundary_frames(&long_frames, boundary_s, 1.0/12800.0);
            if let (Some(lb), Some(fa)) = (last_before, first_after) {
                let is_kf = fa.3 == 1;
                let pts_delta = fa.0 - lb.0;
                let pts_backwards = pts_delta < 0;
                if !is_kf { long_keyframe_issues += 1; if first_problem_boundary.is_none() { first_problem_boundary = Some(b); } }
                if pts_backwards { long_pts_backwards += 1; if first_problem_boundary.is_none() { first_problem_boundary = Some(b); } }
                let status = if !is_kf || pts_backwards { "❌ CORRUPT" } else { "✅ OK" };
                println!("    B{}@{:.0}s: last type={} first type={} kf={} Δpts={} | {}",
                    b, boundary_s, lb.2, fa.2, fa.3, pts_delta, status);
            }
        }
        println!("  Long: {} keyframe issues, {} PTS backwards", long_keyframe_issues, long_pts_backwards);
        if let Some(b) = first_problem_boundary {
            println!("  ⚠️ First problem boundary: B{}@{:.0}s", b, (b*5) as f64);
        } else {
            println!("  ✅ No problem boundaries");
        }

        // Cleanup
        for f in &uniform_files { std::fs::remove_file(f).ok(); }
        for f in &mixed_files { std::fs::remove_file(f).ok(); }
        for f in &long_files { std::fs::remove_file(f).ok(); }
        std::fs::remove_dir_all(&test_dir).ok();

        println!("\n[TEST] Phase 5B Keyframe Boundary Forensics complete.");
    }
}