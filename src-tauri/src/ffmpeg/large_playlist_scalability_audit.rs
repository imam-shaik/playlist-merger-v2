#[cfg(test)]
mod large_playlist_scalability_audit {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::Instant;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_file(ffmpeg: &Path, path: &Path, duration_sec: u32) -> bool {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=320x240:rate=15", duration_sec),
                "-f", "lavfi",
                "-i", "sine=frequency=440:sample_rate=22050",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-c:a", "aac",
                "-ar", "22050",
                "-t", &duration_sec.to_string(),
                path.to_str().unwrap()
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn get_process_memory_mb() -> u64 {
        // Windows: use wmic to get working set memory
        let output = Command::new("powershell")
            .args(&["-NoProfile", "-Command",
                "(Get-Process -Id $PID).WorkingSet64 / 1MB"])
            .output();
        if let Ok(o) = output {
            let s = String::from_utf8_lossy(&o.stdout);
            if let Ok(val) = s.trim().parse::<f64>() {
                return val as u64;
            }
        }
        0
    }

    fn time_operation<F, R>(name: &str, op: F) -> (R, std::time::Duration)
    where F: FnOnce() -> R
    {
        let start = Instant::now();
        let result = op();
        let dur = start.elapsed();
        println!("    {}: {:.2}s", name, dur.as_secs_f64());
        (result, dur)
    }

    fn probe_file(ffprobe: &Path, file: &Path) -> std::time::Duration {
        let start = Instant::now();
        let _ = Command::new(ffprobe)
            .args(&["-v", "quiet", "-print_format", "json", "-show_streams", file.to_str().unwrap()])
            .output();
        start.elapsed()
    }

    fn concat_files(ffmpeg: &Path, list_path: &Path, output: &Path) -> std::time::Duration {
        let start = Instant::now();
        let _ = Command::new(ffmpeg)
            .args(&["-y", "-f", "concat", "-safe", "0", "-i", &list_path.to_string_lossy(),
                    "-c", "copy", output.to_str().unwrap()])
            .output();
        start.elapsed()
    }

    /// Profile at a given playlist size
    #[derive(Debug, Clone)]
    struct ScalabilityProfile {
        file_count: usize,
        create_time: std::time::Duration,
        probe_total_time: std::time::Duration,
        probe_avg_time: std::time::Duration,
        concat_list_gen_time: std::time::Duration,
        concat_time: std::time::Duration,
        #[allow(dead_code)]
        total_time: std::time::Duration,
        output_size_mb: f64,
        #[allow(dead_code)]
        peak_memory_mb: u64,
    }

    fn profile_size(n: usize, ffmpeg: &Path, ffprobe: &Path, test_dir: &Path) -> ScalabilityProfile {
        println!("\n  ┌── Profiling {}-file playlist ──", n);
        let mem_start = get_process_memory_mb();

        let _mem_after_create: u64;
        let create_time: std::time::Duration;
        let probe_total_time: std::time::Duration;
        let probe_avg_time: std::time::Duration;
        let concat_list_gen_time: std::time::Duration;
        let concat_time: std::time::Duration;
        let output_size_mb: f64;
        let mem_end: u64;

        // ── 1. Create test files ──────────────────────────────────────
        {
            let start = Instant::now();
            let files: Vec<PathBuf> = (0..n)
                .map(|i| test_dir.join(format!("p{}_{}.mp4", n, i)))
                .collect();
            for f in &files {
                create_test_file(ffmpeg, f, 1);
            }
            create_time = start.elapsed();
            println!("  │ ✓ Created {} files in {:.2}s", n, create_time.as_secs_f64());
            _mem_after_create = get_process_memory_mb();

            // ── 2. Probe files sequentially (simulating real cost) ─────
            let probe_start = Instant::now();
            let mut total_probe = std::time::Duration::ZERO;
            for f in &files {
                total_probe += probe_file(ffprobe, f);
            }
            probe_total_time = probe_start.elapsed();
            probe_avg_time = if n > 0 { total_probe / n as u32 } else { std::time::Duration::ZERO };
            println!("  │ ✓ Probed {} files in {:.2}s (avg {:.0}ms/file)",
                n, probe_total_time.as_secs_f64(), probe_avg_time.as_millis());

            // ── 3. Generate concat list ─────────────────────────────────
            let list_path = test_dir.join(format!("p{}_list.txt", n));
            let list_start = Instant::now();
            let list_content: String = (0..n)
                .map(|i| format!("file '{}'\nduration 1", files[i].to_string_lossy().replace('\\', "/")))
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&list_path, &list_content).unwrap();
            concat_list_gen_time = list_start.elapsed();
            println!("  │ ✓ Generated concat list in {:.4}s ({} bytes)", concat_list_gen_time.as_secs_f64(), list_content.len());

            // ── 4. Concat ───────────────────────────────────────────────
            let output = test_dir.join(format!("p{}_output.mp4", n));
            concat_time = concat_files(ffmpeg, &list_path, &output);
            println!("  │ ✓ Concat in {:.2}s", concat_time.as_secs_f64());

            output_size_mb = if output.exists() {
                std::fs::metadata(&output).map(|m| m.len() as f64 / 1024.0 / 1024.0).unwrap_or(0.0)
            } else { 0.0 };
            mem_end = get_process_memory_mb();

            // Cleanup files
            for f in &files { std::fs::remove_file(f).ok(); }
            std::fs::remove_file(&list_path).ok();
            std::fs::remove_file(&output).ok();
        }

        let total_time = create_time + probe_total_time + concat_list_gen_time + concat_time;

        println!("  │ 📊 Output: {:.2} MB | Memory: {} → {} MB | Total: {:.2}s",
            output_size_mb, mem_start, mem_end, total_time.as_secs_f64());
        println!("  └─────────────────────────────────────────");

        ScalabilityProfile {
            file_count: n,
            create_time,
            probe_total_time,
            probe_avg_time,
            concat_list_gen_time,
            concat_time,
            total_time,
            output_size_mb,
            peak_memory_mb: mem_end.saturating_sub(mem_start),
        }
    }

    /// PHASE 6A: LARGE PLAYLIST SCALABILITY AUDIT
    ///
    /// Profiles memory, timing, and resource usage at 100, 500, 1000+ file scales.
    /// Identifies scaling bottlenecks and infers theoretical limits.
    #[tokio::test]
    async fn test_large_playlist_scalability_audit() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 6A: LARGE PLAYLIST SCALABILITY AUDIT                        ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("phase6_scalability");
        std::fs::create_dir_all(&test_dir).unwrap();

        // Profile multiple sizes
        let sizes = vec![100, 500, 1000];
        let mut profiles: Vec<ScalabilityProfile> = Vec::new();

        for &n in &sizes {
            let profile = profile_size(n, &ffmpeg, &ffprobe, &test_dir);
            profiles.push(profile);
        }

        // ── Analysis: scaling efficiency ────────────────────────────────
        println!("\n[SCALING ANALYSIS]");
        println!("\n┌─────────┬────────────┬────────────┬────────────┬────────────┬──────────┐");
        println!("│ Files   │ Create     │ Probe      │ List Gen   │ Concat     │ Output   │");
        println!("├─────────┼────────────┼────────────┼────────────┼────────────┼──────────┤");
        for p in &profiles {
            println!("│ {:>7} │ {:>7.1}s   │ {:>7.1}s   │ {:>7.3}s   │ {:>7.1}s   │ {:>5.1}MB │",
                p.file_count,
                p.create_time.as_secs_f64(),
                p.probe_total_time.as_secs_f64(),
                p.concat_list_gen_time.as_secs_f64(),
                p.concat_time.as_secs_f64(),
                p.output_size_mb);
        }
        println!("└─────────┴────────────┴────────────┴────────────┴────────────┴──────────┘");

        // ── Identify bottlenecks ────────────────────────────────────────
        println!("\n[BOTTLENECK IDENTIFICATION]");

        if profiles.len() >= 2 {
            let p1 = &profiles[0];
            let p2 = &profiles[profiles.len() - 1];
            let size_ratio = p2.file_count as f64 / p1.file_count as f64;

            let create_ratio = if p1.create_time.as_secs_f64() > 0.0 {
                p2.create_time.as_secs_f64() / p1.create_time.as_secs_f64()
            } else { 0.0 };
            let probe_ratio = if p1.probe_total_time.as_secs_f64() > 0.0 {
                p2.probe_total_time.as_secs_f64() / p1.probe_total_time.as_secs_f64()
            } else { 0.0 };
            let concat_ratio = if p1.concat_time.as_secs_f64() > 0.0 {
                p2.concat_time.as_secs_f64() / p1.concat_time.as_secs_f64()
            } else { 0.0 };

            println!("  Size scaling: {:.1}x ({} to {})", size_ratio, p1.file_count, p2.file_count);
            println!("  Create time scaling: {:.2}x ({:.1}s to {:.1}s)", create_ratio, p1.create_time.as_secs_f64(), p2.create_time.as_secs_f64());
            println!("  Probe time scaling:  {:.2}x ({:.1}s to {:.1}s)", probe_ratio, p1.probe_total_time.as_secs_f64(), p2.probe_total_time.as_secs_f64());
            println!("  Concat time scaling: {:.2}x ({:.1}s to {:.1}s)", concat_ratio, p1.concat_time.as_secs_f64(), p2.concat_time.as_secs_f64());

            // Linear scaling: 10x more files = 10x more time
            // Sub-linear: parallelization helping
            // Super-linear: bottleneck getting worse
            let linear = size_ratio;
            println!("\n  EFFICIENCY (linear = 1.0):");
            println!("    Create: {:.2} ({})", create_ratio / linear, if (create_ratio / linear) < 1.2 { "linear" } else { "super-linear" });
            println!("    Probe:  {:.2} ({})", probe_ratio / linear, if (probe_ratio / linear) < 1.2 { "linear" } else { "super-linear" });
            println!("    Concat: {:.2} ({})", concat_ratio / linear, if (concat_ratio / linear) < 1.2 { "linear" } else { "super-linear" });
        }

        // ── Theoretical limits ──────────────────────────────────────────
        println!("\n[THEORETICAL LIMITS]");

        if let Some(p1000) = profiles.iter().find(|p| p.file_count == 1000) {
            let probe_rate = p1000.file_count as f64 / p1000.probe_total_time.as_secs_f64();
            let probe_per_file_ms = p1000.probe_avg_time.as_millis();
            println!("  Probe rate: {:.0} files/sec ({}ms per file)", probe_rate, probe_per_file_ms);

            // At 6 parallel (existing code), theoretical max throughput
            let parallel_speedup = 6.0;
            let sequential_for_1000 = p1000.probe_total_time.as_secs_f64();
            let parallel_for_1000_estimate = sequential_for_1000 / parallel_speedup;
            println!("  With 6 parallel probes, 1000 files ~ {:.1}s (vs {:.1}s sequential)", parallel_for_1000_estimate, sequential_for_1000);

            // For 5000 files (production scale)
            let est_5000_sequential = sequential_for_1000 * 5.0;
            let est_5000_parallel = est_5000_sequential / parallel_speedup;
            println!("  Estimated 5000 files (sequential): {:.1}s", est_5000_sequential);
            println!("  Estimated 5000 files (6 parallel): {:.1}s", est_5000_parallel);
            println!("  Estimated 10000 files (6 parallel): {:.1}s", est_5000_sequential * 2.0 / parallel_speedup);

            if est_5000_parallel > 300.0 {
                println!("  WARNING: 5000+ files: probe alone takes >5min — needs optimization");
            } else {
                println!("  OK: 5000+ files: probe alone completes <5min");
            }
        }

        // ── FFmpeg startup overhead ────────────────────────────────────
        println!("\n[FFMPEG STARTUP OVERHEAD]");
        let (_, ffmpeg_startup) = time_operation("ffmpeg -version", || {
            let _ = Command::new(&ffmpeg).arg("-version").output();
        });
        let (_, ffprobe_startup) = time_operation("ffprobe -version", || {
            let _ = Command::new(&ffprobe).arg("-version").output();
        });
        let ffmpeg_ms = ffmpeg_startup.as_millis();
        let ffprobe_ms = ffprobe_startup.as_millis();
        println!("  Each ffmpeg invocation costs: {}ms", ffmpeg_ms);
        println!("  Each ffprobe invocation costs: {}ms", ffprobe_ms);

        // For 1000 files with sequential probes
        if let Some(p1000) = profiles.iter().find(|p| p.file_count == 1000) {
            let probe_overhead_per_file = ffprobe_ms as f64 / p1000.probe_total_time.as_millis() as f64 * 100.0;
            println!("  Probe overhead = {:.0}% of total probe time ({}ms startup / {}ms total)",
                probe_overhead_per_file, ffprobe_ms, p1000.probe_total_time.as_millis());
        }

        // ── Concat list size scaling ────────────────────────────────────
        println!("\n[CONCAT LIST SIZE SCALING]");
        println!("  Each entry: ~80 bytes (path + duration directive)");
        for n in [100, 500, 1000, 5000, 10000] {
            let est_size = n * 80;
            println!("    {} files -> ~{} KB concat list", n, est_size / 1024);
        }
        println!("  OK: Concat list file is negligible at any reasonable size");

        std::fs::remove_dir_all(&test_dir).ok();
        println!("\n[TEST] Phase 6A Scalability Audit complete.");
    }
}