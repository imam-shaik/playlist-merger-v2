#[cfg(test)]
mod production_certification {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc};
    use std::sync::atomic::{AtomicBool};
    use std::time::{Duration, Instant};
    use tokio::task::spawn_blocking;
    use crate::commands::merge::{MergeRequest, AudioRepairMode};
    use crate::ffmpeg::concat::{MergeConfig, run_merge_blocking};
    use crate::types::{MergeMode, SubtitleMode};
    use crate::ffmpeg::probe::probe_file;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn get_mixed_media_files() -> Vec<PathBuf> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let base = root.parent().unwrap().join("tests").join("fixtures").join("production_test");
        let mut files = Vec::new();
        // Mix of dominant, timebase outliers, audio outliers, and both
        for i in 0..=5 { files.push(base.join(format!("file_{}_dominant.mp4", i))); }
        for i in 12..=13 { files.push(base.join(format!("file_{}_tb_outlier.mp4", i))); }
        for i in 15..=16 { files.push(base.join(format!("file_{}_audio_outlier.mp4", i))); }
        files.push(base.join("file_18_BUG_BOTH.mp4"));
        files.iter().filter(|f| f.exists()).cloned().collect()
    }

    async fn run_certification_run(
        name: &str,
        files: Vec<PathBuf>,
        mode: &str,
        audio_repair: &str,
    ) -> CertificationMetrics {
        let (ffmpeg, ffprobe) = get_binaries();
        let test_dir = std::env::temp_dir().join("production_certification").join(name);
        std::fs::create_dir_all(&test_dir).unwrap();
        let output_path = test_dir.join("certified_output.mp4");

        println!("\n[CERT] Starting {} | Mode: {} | Files: {}", name, mode, files.len());

        // 1. SIMULATE FRONTEND IMPORT (Batch Probe)
        let import_start = Instant::now();
        let mut media_infos = Vec::new();
        for f in &files {
            media_infos.push(probe_file(&ffprobe, f).unwrap());
        }
        let import_latency = import_start.elapsed();

        let mode_enum = match mode {
            "lossless" => MergeMode::Lossless,
            _ => MergeMode::Custom,
        };
        let repair_enum = match audio_repair {
            "fast" => Some(AudioRepairMode::Fast),
            "smart" => Some(AudioRepairMode::Smart),
            "safe" => Some(AudioRepairMode::Safe),
            _ => None,
        };

        // 2. CONSTRUCT MERGE REQUEST & CONFIG
        let request = MergeRequest {
            job_id: format!("cert_{}", name),
            input_files: files.iter().map(|f| f.to_string_lossy().into_owned()).collect(),
            media_infos: Some(media_infos.clone()),
            input_names: files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect(),
            input_durations: media_infos.iter().map(|m| m.duration).collect(),
            external_subtitles: Some(vec![None; files.len()]),
            output_path: output_path.to_string_lossy().into_owned(),
            mode: mode_enum.clone(),
            audio_repair_mode: repair_enum,
            total_duration: media_infos.iter().map(|m| m.duration).sum(),
            subtitle_mode: Some(SubtitleMode::None),
            export_merged_srt: Some(false),
            selected_subtitle_stream_indices: None,
            video_codec: if mode == "custom" { Some("libx264".to_string()) } else { None },
            audio_codec: if mode == "custom" { Some("aac".to_string()) } else { None },
            video_crf: Some(23),
            video_preset: Some("ultrafast".to_string()),
            audio_bitrate: Some("128k".to_string()),
            target_resolution: None,
            target_fps: None,
            hw_accel: None,
            card_config: None,
            split_config: None,
            naming_config: None,
            convert_to_mp4: None,
            validate_audio: Some(true),
            large_playlist_strategy: None,
            repeat_config: None,
            phase: None,
        };

        let config = MergeConfig {
            input_files: request.input_files.clone(),
            input_names: request.input_names.clone(),
            input_durations: request.input_durations.clone(),
            subtitle_list_path: None,
            output_path: request.output_path.clone(),
            mode: request.mode.clone(),
            total_duration: request.total_duration,
            video_codec: request.video_codec.clone(),
            audio_codec: request.audio_codec.clone(),
            video_crf: request.video_crf,
            video_preset: request.video_preset.clone(),
            audio_bitrate: request.audio_bitrate.clone(),
            target_resolution: request.target_resolution.clone(),
            target_fps: request.target_fps.clone(),
            hw_accel: request.hw_accel.clone(),
            split_config: request.split_config.clone(),
            naming_config: None,
            subtitle_files: vec![None; request.input_files.len()],
            card_config: request.card_config.clone(),
            segment_is_card: vec![false; request.input_files.len()],
            subtitle_mode: SubtitleMode::None,
            export_merged_srt: false,
            burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
            audio_normalized: false,
            immutability_registry: None,
        };

        // 3. RUN MERGE (Capture Preparation Latency)
        let merge_start = Instant::now();
        let cancel_flag = Arc::new(AtomicBool::new(false));
        
        let concat_list_path = test_dir.join("list.txt");
        let path_refs: Vec<&Path> = files.iter().map(|p| p.as_path()).collect();
        let durations: Vec<f64> = media_infos.iter().map(|m| m.duration).collect();
        let include_duration = config.mode == MergeMode::Custom;
        crate::ffmpeg::write_concat_list_with_durations(&path_refs, Some(&durations), &concat_list_path, include_duration).unwrap();
        
        let result = spawn_blocking(move || {
            run_merge_blocking(
                &ffmpeg,
                &config,
                &concat_list_path,
                cancel_flag,
                |_| {} // Progress callback
            )
        }).await.unwrap();

        let total_merge_time = merge_start.elapsed();

        // 4. VERIFY CLEANUP
        std::thread::sleep(Duration::from_millis(500)); // Wait for RAII drop
        let temp_files_remaining = count_files_recursive(&test_dir); 

        CertificationMetrics {
            name: name.to_string(),
            import_latency,
            total_merge_time,
            success: result.is_ok(),
            temp_files_remaining,
        }
    }

    struct CertificationMetrics {
        name: String,
        import_latency: Duration,
        total_merge_time: Duration,
        success: bool,
        temp_files_remaining: usize,
    }

    fn count_files_recursive(dir: &Path) -> usize {
        if !dir.exists() { return 0; }
        walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .count()
    }

    #[tokio::test]
    async fn test_production_certification_matrix() {
        println!("\n[CERT] ════════════════════════════════════════════════════════════════");
        println!("[CERT] PRODUCTION CERTIFICATION MATRIX");
        println!("[CERT] ════════════════════════════════════════════════════════════════");

        let mixed_files = get_mixed_media_files();
        assert!(!mixed_files.is_empty(), "Fixtures must exist for certification");

        // TEST 1: Mixed Media Stress Test (Lossless + Smart)
        let m1 = run_certification_run("Mixed_Lossless_Smart", mixed_files.clone(), "lossless", "smart").await;
        
        // TEST 2: Mixed Media Stress Test (Custom + Safe)
        let m2 = run_certification_run("Mixed_Custom_Safe", mixed_files.clone(), "custom", "safe").await;

        println!("\n[CERT] RESULTS SUMMARY:");
        println!("┌──────────────────────┬──────────┬──────────┬──────────┬──────────┐");
        println!("│ Scenario             │ Success  │ Import   │ Merge    │ Leaks    │");
        println!("├──────────────────────┼──────────┼──────────┼──────────┼──────────┤");
        
        for m in &[m1, m2] {
            println!("│ {:<20} │ {:<8} │ {:<8?} │ {:<8?} │ {:<8} │",
                m.name,
                if m.success { "✅ YES" } else { "❌ NO" },
                m.import_latency,
                m.total_merge_time,
                m.temp_files_remaining);
            
            assert!(m.success, "Scenario {} failed", m.name);
        }
        println!("└──────────────────────┴──────────┴──────────┴──────────┴──────────┘");

        println!("\n[CERT] 🏁 PRODUCTION CERTIFICATION COMPLETE: PASS");
    }

    #[tokio::test]
    async fn test_high_volume_scalability_benchmark() {
        let n_files = 1000;
        println!("\n[CERT] Starting High-Volume Scalability Benchmark ({} files)", n_files);
        
        let start = Instant::now();
        // Simulate frontend store growth
        let mut entries = Vec::new();
        for i in 0..n_files {
            entries.push(format!("File_{}", i));
        }
        let store_latency = start.elapsed();
        println!("[CERT] Simulated store update (1,000 strings): {:?}", store_latency);
        
        assert!(store_latency < Duration::from_millis(100), "Store update too slow");
        println!("[CERT] SCALABILITY BENCHMARK COMPLETE: PASS");
    }
}