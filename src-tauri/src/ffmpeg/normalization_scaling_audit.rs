#[cfg(test)]
mod normalization_scaling_audit {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::Instant;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    /// Create a "needs normalization" file (mixed parameters that don't match dominant)
    /// Uses H264 1920x1080 30fps, 48000Hz stereo
    fn create_compatible_file(ffmpeg: &Path, path: &Path, duration_sec: u32) -> bool {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=1920x1080:rate=30", duration_sec),
                "-f", "lavfi",
                "-i", "sine=frequency=440:sample_rate=48000",
                "-c:v", "libx264",
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

    /// Create a "needs normalization" file (different parameters)
    /// Uses H264 1280x720 25fps, 44100Hz stereo
    fn create_outlier_file(ffmpeg: &Path, path: &Path, duration_sec: u32) -> bool {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=1280x720:rate=25", duration_sec),
                "-f", "lavfi",
                "-i", "sine=frequency=440:sample_rate=44100",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-c:a", "aac",
                "-ar", "44100",
                "-t", &duration_sec.to_string(),
                path.to_str().unwrap()
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Simulate one normalization (re-encode to dominant profile)
    /// Takes a 2s 1280x720 25fps file and normalizes to 2s 1920x1080 30fps 48kHz
    fn normalize_one(ffmpeg: &Path, src: &Path, dst: &Path) -> bool {
        Command::new(ffmpeg)
            .args(&[
                "-y",
                "-i", src.to_str().unwrap(),
                "-vf", "scale=1920:1080,fps=30",
                "-c:v", "libx264",
                "-preset", "ultrafast",
                "-ar", "48000",
                "-c:a", "aac",
                dst.to_str().unwrap()
            ])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Simulate parallel normalization (mimics the production 6-worker pool)
    fn parallel_normalize(
        ffmpeg: &Path,
        files: &[PathBuf],
        output_dir: &Path,
        max_concurrent: usize,
    ) -> (std::time::Duration, usize, u64) {
        let start = Instant::now();
        let queue = Arc::new(AtomicUsize::new(0));
        let peak_concurrent = Arc::new(AtomicUsize::new(0));
        let total_disk_written: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

        // Use scoped threads to parallelize
        let work_items: Vec<(PathBuf, PathBuf)> = files.iter().enumerate()
            .map(|(i, f)| (f.clone(), output_dir.join(format!("norm_{}.mp4", i))))
            .collect();

        let chunks: Vec<Vec<(PathBuf, PathBuf)>> = work_items
            .chunks(max_concurrent)
            .map(|c| c.to_vec())
            .collect();

        let peak_clone = peak_concurrent.clone();
        let queue_clone = queue.clone();
        let disk_clone = total_disk_written.clone();

        std::thread::scope(|s| {
            for chunk in chunks {
                for (src, dst) in chunk {
                    let peak = peak_clone.clone();
                    let queue = queue_clone.clone();
                    let disk = disk_clone.clone();

                    s.spawn(move || {
                        let before = queue.fetch_add(1, Ordering::SeqCst) + 1;
                        let mut current_peak = peak.load(Ordering::SeqCst);
                        while before > current_peak {
                            match peak.compare_exchange(current_peak, before, Ordering::SeqCst, Ordering::SeqCst) {
                                Ok(_) => break,
                                Err(actual) => current_peak = actual,
                            }
                        }

                        normalize_one(ffmpeg, &src, &dst);

                        if let Ok(meta) = std::fs::metadata(&dst) {
                            disk.fetch_add(meta.len() as usize, Ordering::SeqCst);
                        }

                        queue.fetch_sub(1, Ordering::SeqCst);
                    });
                }
            }
        });

        let elapsed = start.elapsed();
        let peak = peak_concurrent.load(Ordering::SeqCst);
        let disk = total_disk_written.load(Ordering::SeqCst) as u64;

        (elapsed, peak, disk)
    }

    fn get_process_memory_mb() -> u64 {
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

    fn get_cpu_count() -> usize {
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
    }

    fn get_temp_dir_size_mb(dir: &Path) -> f64 {
        let mut total: u64 = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        total += meta.len();
                    }
                }
            }
        }
        total as f64 / 1024.0 / 1024.0
    }

    /// Profile a single case
    #[derive(Debug)]
    struct NormalizationCase {
        name: String,
        #[allow(dead_code)]
        file_count: usize,
        normalized_count: usize,
        normalizer_pool_size: usize,
        norm_time: std::time::Duration,
        peak_concurrent: usize,
        disk_written_mb: f64,
        memory_peak_mb: u64,
    }

    fn profile_case(
        name: &str,
        n: usize,
        normalized: usize,
        pool_size: usize,
        ffmpeg: &Path,
        test_dir: &Path,
    ) -> NormalizationCase {
        println!("\n  ─── Case: {} (n={}, normalize={}, pool={}) ───", name, n, normalized, pool_size);
        let mem_start = get_process_memory_mb();
        let _temp_size_before = get_temp_dir_size_mb(test_dir);

        // Create files
        let files: Vec<PathBuf> = (0..n).map(|i| test_dir.join(format!("{}_{}.mp4", name, i))).collect();
        let file_start = Instant::now();
        for (i, f) in files.iter().enumerate() {
            if i < normalized {
                create_outlier_file(ffmpeg, f, 2);
            } else {
                create_compatible_file(ffmpeg, f, 2);
            }
        }
        let _file_create_time = file_start.elapsed();
        println!("  Files created ({}+{}) in {:.1}s", normalized, n - normalized, _file_create_time.as_secs_f64());

        // Get just the files that need normalization
        let norm_files: Vec<PathBuf> = files.iter().take(normalized).cloned().collect();

        // Run parallel normalization
        let output_dir = test_dir.join("normalized");
        std::fs::create_dir_all(&output_dir).unwrap();
        let (norm_time, peak_concurrent, _disk_bytes) = parallel_normalize(
            ffmpeg,
            &norm_files,
            &output_dir,
            pool_size,
        );
        let disk_written_mb = get_temp_dir_size_mb(&output_dir);
        let mem_end = get_process_memory_mb();
        let _temp_size_after = get_temp_dir_size_mb(test_dir);

        println!("  Normalization: {:.1}s | Peak concurrent: {}/{} | Disk written: {:.1} MB | Memory: {} → {} MB",
            norm_time.as_secs_f64(), peak_concurrent, pool_size, disk_written_mb, mem_start, mem_end);

        // Cleanup
        for f in &files { std::fs::remove_file(f).ok(); }
        std::fs::remove_dir_all(&output_dir).ok();

        NormalizationCase {
            name: name.to_string(),
            file_count: n,
            normalized_count: normalized,
            normalizer_pool_size: pool_size,
            norm_time,
            peak_concurrent,
            disk_written_mb,
            memory_peak_mb: mem_end.saturating_sub(mem_start),
        }
    }

    /// PHASE 6C: NORMALIZATION SCALING AUDIT
    ///
    /// Tests 4 cases at different normalization loads:
    /// - Case A: 100 files, 0 normalized (baseline)
    /// - Case B: 100 files, 25 normalized
    /// - Case C: 100 files, 50 normalized
    /// - Case D: 100 files, 100 normalized (worst case)
    ///
    /// Measures: CPU, memory, disk, runtime, parallel worker utilization,
    /// temp storage, peak concurrent ffmpeg processes.
    #[tokio::test]
    async fn test_normalization_scaling_audit() {
        println!("\n╔══════════════════════════════════════════════════════════════════════╗");
        println!("║  PHASE 6C: NORMALIZATION SCALING AUDIT                             ║");
        println!("╚══════════════════════════════════════════════════════════════════════╝");

        let (ffmpeg, _ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("phase6c_normalization_scaling");
        std::fs::create_dir_all(&test_dir).unwrap();

        let cpu_count = get_cpu_count();
        println!("\n  System: {} CPU cores available", cpu_count);

        // Use pool size of 6 (matches production code)
        let pool_size = 6;

        // ── Run all 4 cases ────────────────────────────────────────────
        let cases = vec![
            profile_case("A_baseline", 100, 0, pool_size, &ffmpeg, &test_dir),
            profile_case("B_25pct", 100, 25, pool_size, &ffmpeg, &test_dir),
            profile_case("C_50pct", 100, 50, pool_size, &ffmpeg, &test_dir),
            profile_case("D_100pct", 100, 100, pool_size, &ffmpeg, &test_dir),
        ];

        // ── Scaling Analysis ───────────────────────────────────────────
        println!("\n[SCALING ANALYSIS]");
        println!("\n┌──────────────┬────────┬─────────┬────────────┬─────────┬─────────┐");
        println!("│ Case         │  Norm  │  Pool   │  Time      │  Peak   │  Disk   │");
        println!("├──────────────┼────────┼─────────┼────────────┼─────────┼─────────┤");
        for c in &cases {
            println!("│ {:<12} │ {:>6} │ {:>7} │ {:>7.2}s   │ {:>5}/{:<3} │ {:>5.1}MB │",
                c.name, c.normalized_count, c.normalizer_pool_size,
                c.norm_time.as_secs_f64(), c.peak_concurrent, c.normalizer_pool_size,
                c.disk_written_mb);
        }
        println!("└──────────────┴────────┴─────────┴────────────┴─────────┴─────────┘");

        // ── Pool utilization analysis ──────────────────────────────────
        println!("\n[POOL UTILIZATION]");
        println!("  (Peak concurrent / Pool size = utilization)");
        for c in &cases {
            if c.normalized_count == 0 { continue; }
            let util = c.peak_concurrent as f64 / c.normalizer_pool_size as f64 * 100.0;
            let throughput = c.normalized_count as f64 / c.norm_time.as_secs_f64();
            println!("  {}: {:.0}% peak utilization | {:.1} normalizations/sec",
                c.name, util, throughput);
        }

        // ── Per-normalization time ─────────────────────────────────────
        println!("\n[PER-NORMALIZATION TIME]");
        for c in &cases {
            if c.normalized_count == 0 { continue; }
            let per_norm_ms = c.norm_time.as_millis() as f64 / c.normalized_count as f64;
            println!("  {}: {:.0}ms per normalization (2s clip)", c.name, per_norm_ms);
        }

        // ── Efficiency vs. pool size ───────────────────────────────────
        println!("\n[EFFICIENCY: How long does each extra normalization take?]");
        if cases.len() >= 4 {
            let case_a_time = cases[0].norm_time.as_secs_f64();  // 0 norms
            let case_b_time = cases[1].norm_time.as_secs_f64();  // 25 norms
            let case_c_time = cases[2].norm_time.as_secs_f64();  // 50 norms
            let case_d_time = cases[3].norm_time.as_secs_f64();  // 100 norms

            let time_per_norm_b = (case_b_time - case_a_time) / 25.0;
            let time_per_norm_c = (case_c_time - case_b_time) / 25.0;
            let time_per_norm_d = (case_d_time - case_c_time) / 50.0;

            println!("  Marginal cost per normalization:");
            println!("    0 → 25  norms: {:.3}s per norm", time_per_norm_b);
            println!("    25 → 50 norms: {:.3}s per norm", time_per_norm_c);
            println!("    50 → 100 norms: {:.3}s per norm", time_per_norm_d);

            // If pool is saturating, marginal cost should stay flat or grow slightly
            // If pool is starving, marginal cost grows
            let max_increase = [time_per_norm_b, time_per_norm_c, time_per_norm_d]
                .iter().cloned().fold(0.0_f64, f64::max);
            let min_increase = [time_per_norm_b, time_per_norm_c, time_per_norm_d]
                .iter().cloned().fold(f64::INFINITY, f64::min);
            let increase_ratio = max_increase / min_increase;
            println!("    Max/Min ratio: {:.2} ({})",
                increase_ratio,
                if increase_ratio < 1.5 { "stable" } else if increase_ratio < 3.0 { "moderate growth" } else { "super-linear growth" });
        }

        // ── Resource limits ────────────────────────────────────────────
        println!("\n[RESOURCE LIMITS]");
        let total_disk = cases.iter().map(|c| c.disk_written_mb).sum::<f64>();
        println!("  Total disk written across all cases: {:.1} MB", total_disk);
        println!("  Peak memory: {} MB", cases.iter().map(|c| c.memory_peak_mb).max().unwrap_or(0));

        // Project to 1000 files
        let case_d_time_per_norm = cases[3].norm_time.as_secs_f64() / 100.0;
        let projected_1000_all_norm = case_d_time_per_norm * 1000.0 / (pool_size as f64 / cases[3].peak_concurrent as f64).max(1.0);
        println!("\n  PROJECTIONS:");
        println!("    1000 files (all normalized, pool=6): ~{:.1}s", projected_1000_all_norm);
        println!("    1000 files (all normalized, pool=12): ~{:.1}s", projected_1000_all_norm / 2.0);

        // ── Verdict ─────────────────────────────────────────────────────
        println!("\n[VERDICT]");

        // Check for collapse: is the marginal cost growing rapidly?
        let scaling_ok = if cases.len() >= 4 {
            let case_d_time_per_norm = cases[3].norm_time.as_secs_f64() / 100.0;
            let case_b_time_per_norm = if cases[1].normalized_count > 0 {
                cases[1].norm_time.as_secs_f64() / cases[1].normalized_count as f64
            } else { 0.0 };
            case_d_time_per_norm < case_b_time_per_norm * 2.0
        } else { false };

        if scaling_ok {
            println!("  Scaling: STABLE - no performance collapse detected");
        } else {
            println!("  Scaling: WARNING - performance degrades with more normalizations");
        }

        // Check pool utilization
        let avg_util = cases.iter()
            .filter(|c| c.normalized_count > 0)
            .map(|c| c.peak_concurrent as f64 / c.normalizer_pool_size as f64)
            .sum::<f64>() / cases.iter().filter(|c| c.normalized_count > 0).count() as f64;
        println!("  Average pool utilization: {:.0}%", avg_util * 100.0);

        if avg_util > 0.85 {
            println!("  Pool is well-saturated. Consider increasing pool size for more parallelism.");
        } else if avg_util < 0.5 {
            println!("  Pool is under-utilized. The current pool size may be too large.");
        } else {
            println!("  Pool is appropriately utilized.");
        }

        // Check for CPU starvation
        if cpu_count < pool_size {
            println!("  Note: {} cores but pool size {} - pool may be over-subscribed",
                cpu_count, pool_size);
        } else {
            println!("  Note: {} cores available, pool size {} - OK", cpu_count, pool_size);
        }

        std::fs::remove_dir_all(&test_dir).ok();
        println!("\n[TEST] Phase 6C Normalization Scaling Audit complete.");
    }
}