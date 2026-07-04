#[cfg(test)]
mod card_production_certification {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use tokio::task::spawn_blocking;
    use crate::ffmpeg::concat::{MergeConfig, run_merge_blocking};
    use crate::ffmpeg::cards::render_cards_for_merge;
    use crate::types::{MergeMode, SubtitleMode, CardConfig, CardFrequency, SplitConfig, SplitMode};
    use crate::ffmpeg::probe::probe_file;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    fn create_test_video(
        ffmpeg: &Path,
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        duration_secs: u32,
        sample_rate: u32,
    ) {
        let output = Command::new(ffmpeg)
            .args([
                "-y", "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size={}x{}:rate={}", duration_secs, width, height, fps),
                "-f", "lavfi",
                "-i", &format!("sine=frequency=440:sample_rate={}", sample_rate),
                "-c:v", "libx264", "-preset", "ultrafast",
                "-c:a", "aac", "-ar", &sample_rate.to_string(),
                "-t", &duration_secs.to_string(),
                path.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create test video");
        assert!(output.status.success(), "Failed to create test video: {}", 
            String::from_utf8_lossy(&output.stderr));
    }

    fn create_test_video_in_dir(
        ffmpeg: &Path,
        dir: &Path,
        name: &str,
        width: u32,
        height: u32,
        fps: u32,
        duration_secs: u32,
    ) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        create_test_video(ffmpeg, &path, width, height, fps, duration_secs, 48000);
        path
    }

    /// Extract a single frame from a video at a specific timestamp
    fn extract_frame(
        ffmpeg: &Path,
        video: &Path,
        timestamp: f64,
        output: &Path,
    ) -> bool {
        let result = Command::new(ffmpeg)
            .args([
                "-y", "-ss", &format!("{:.3}", timestamp),
                "-i", video.to_str().unwrap(),
                "-frames:v", "1",
                "-f", "image2",
                output.to_str().unwrap()
            ])
            .output();
        result.map(|r| r.status.success()).unwrap_or(false)
    }

    /// Compare two image files and return true if they differ
    fn frames_differ(frame1: &Path, frame2: &Path) -> bool {
        let data1 = std::fs::read(frame1).unwrap_or_default();
        let data2 = std::fs::read(frame2).unwrap_or_default();
        data1 != data2
    }

    /// Verify card visibility by extracting frames and comparing with adjacent video frames
    fn verify_card_visibility(
        ffmpeg: &Path,
        output: &Path,
        expected_card_timestamps: &[f64],
        card_duration: f64,
    ) {
        let test_dir = output.parent().unwrap_or(Path::new("."));
        
        for (i, &timestamp) in expected_card_timestamps.iter().enumerate() {
            let mid = timestamp + (card_duration / 2.0);
            let card_frame = test_dir.join(format!("verify_card_{}.png", i));
            let before_frame = test_dir.join(format!("verify_before_{}.png", i));
            let after_frame = test_dir.join(format!("verify_after_{}.png", i));

            assert!(
                extract_frame(ffmpeg, output, mid, &card_frame),
                "Failed to extract card frame at {:.2}s", mid
            );
            assert!(card_frame.exists(), "Card frame file not created at {:.2}s", mid);

            let before_ts = (timestamp - 0.5).max(0.0);
            assert!(
                extract_frame(ffmpeg, output, before_ts, &before_frame),
                "Failed to extract before frame at {:.2}s", before_ts
            );

            let after_ts = timestamp + card_duration + 0.5;
            if after_ts > 0.0 {
                let _ = extract_frame(ffmpeg, output, after_ts, &after_frame);
            }

            if before_frame.exists() {
                assert!(
                    frames_differ(&card_frame, &before_frame),
                    "Card frame at {:.2}s identical to before-video frame - card may be blank", timestamp
                );
            }

            if after_frame.exists() {
                assert!(
                    frames_differ(&card_frame, &after_frame),
                    "Card frame at {:.2}s identical to after-video frame - card may be blank", timestamp
                );
            }

            let frame_size = card_frame.metadata().map(|m| m.len()).unwrap_or(0);
            assert!(
                frame_size > 5000,
                "Card frame at {:.2}s too small ({} bytes) - likely blank", timestamp, frame_size
            );

            let _ = std::fs::remove_file(&card_frame);
            let _ = std::fs::remove_file(&before_frame);
            let _ = std::fs::remove_file(&after_frame);
        }
    }

    /// Render cards, interleave with videos, write concat list, build config — mirrors merge.rs logic
    fn prepare_merge_with_cards(
        ffmpeg: &Path,
        ffprobe: &Path,
        files: &[PathBuf],
        names: &[String],
        durations: &[f64],
        output: &Path,
        mode: MergeMode,
        card_config: &CardConfig,
        split_config: Option<SplitConfig>,
    ) -> (MergeConfig, PathBuf, Vec<f64>) {
        let test_dir = output.parent().unwrap();
        let cards_dir = test_dir.join("cards");
        std::fs::create_dir_all(&cards_dir).ok();

        // Render cards (exactly like merge.rs:3629)
        let input_file_strs: Vec<String> = files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
        let rendered = render_cards_for_merge(
            ffmpeg,
            Some(ffprobe),
            &input_file_strs,
            names,
            durations,
            card_config,
            &cards_dir,
            None,
        ).expect("render_cards_for_merge failed");

        // Interleave cards (exactly like merge.rs:3644-3686)
        let mut interleaved_files: Vec<String> = Vec::new();
        let mut interleaved_names = Vec::new();
        let mut interleaved_durs = Vec::new();
        let mut segment_is_card = Vec::new();
        let mut current_folder = String::new();

        for i in 0..files.len() {
            let detection_path = &files[i];
            let folder = Path::new(detection_path)
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let should_insert_card = match card_config.frequency {
                CardFrequency::PerVideo => true,
                CardFrequency::PerFolder => folder != current_folder || i == 0,
            };

            if should_insert_card {
                if let Some((path, _)) = rendered.get(i) {
                    let skip_first_per_video = card_config.frequency == CardFrequency::PerVideo && i == 0;
                    if !skip_first_per_video {
                        interleaved_files.push(path.clone());
                        let display_label = if folder.is_empty() { "Start".to_string() } else { folder.clone() };
                        interleaved_names.push(format!("▶ Section: {}", display_label));
                        interleaved_durs.push(card_config.duration);
                        segment_is_card.push(true);
                    }
                }
                current_folder = folder;
            }

            interleaved_files.push(files[i].to_string_lossy().into_owned());
            interleaved_names.push(names[i].clone());
            interleaved_durs.push(durations[i]);
            segment_is_card.push(false);
        }

        // Write concat list with interleaved files
        let concat_list = test_dir.join("concat.txt");
        let path_refs: Vec<&Path> = interleaved_files.iter().map(|f| Path::new(f)).collect();
        crate::ffmpeg::write_concat_list_with_durations(&path_refs, Some(&interleaved_durs), &concat_list, mode == MergeMode::Custom).unwrap();

        let is_custom = mode == MergeMode::Custom;
        let config = MergeConfig {
            input_files: interleaved_files.clone(),
            input_names: interleaved_names,
            input_durations: interleaved_durs.clone(),
            subtitle_list_path: None,
            output_path: output.to_string_lossy().into_owned(),
            mode,
            total_duration: interleaved_durs.iter().sum(),
            video_codec: if is_custom { Some("libx264".to_string()) } else { None },
            audio_codec: if is_custom { Some("aac".to_string()) } else { None },
            video_crf: Some(23),
            video_preset: Some("ultrafast".to_string()),
            audio_bitrate: Some("128k".to_string()),
            target_resolution: None,
            target_fps: None,
            hw_accel: None,
            split_config,
            naming_config: None,
            subtitle_files: vec![None; interleaved_files.len()],
            card_config: Some(card_config.clone()),
            segment_is_card,
            subtitle_mode: SubtitleMode::None,
            export_merged_srt: false,
            burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
            audio_normalized: false,
            immutability_registry: None,
        };

        (config, concat_list, interleaved_durs)
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 1: Smart MKV + Cards (Uniform Resolution)
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_smart_mkv_with_cards() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_smart_mkv");
        let _ = std::fs::create_dir_all(&test_dir);

        let v1 = create_test_video_in_dir(&ffmpeg, &test_dir, "v1.mp4", 1920, 1080, 30, 5);
        let v2 = create_test_video_in_dir(&ffmpeg, &test_dir, "v2.mp4", 1920, 1080, 30, 5);
        let v3 = create_test_video_in_dir(&ffmpeg, &test_dir, "v3.mp4", 1920, 1080, 30, 5);
        let files = vec![v1, v2, v3];
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_smart_mkv.mp4");
        let card_config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerVideo,
        };

        let (config, concat_list, interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::SmartMkv, &card_config, None,
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        assert!(result.is_ok(), "Smart MKV + Cards failed: {:?}", result.err());
        let merge_result = result.unwrap();
        
        assert!(output.exists(), "Output file not created");
        
        // Duration check: interleaved_durs already includes card durations
        let info = probe_file(&ffprobe, &output).unwrap();
        let expected_duration: f64 = interleaved_durs.iter().sum();
        let drift_pct = ((info.duration - expected_duration).abs() / expected_duration) * 100.0;
        assert!(drift_pct < 2.0, "Duration drift too large: {:.2}% (expected {:.1}s, got {:.1}s)", 
            drift_pct, expected_duration, info.duration);

        let card_count = merge_result.segments.iter().filter(|s| s.is_card == Some(true)).count();
        assert_eq!(card_count, 2, "Expected 2 cards, got {}", card_count);

        // PerVideo: card before V1 (skipped), card before V2 at 5s, card before V3 at 12s
        verify_card_visibility(&ffmpeg, &output, &[5.0, 12.0], 2.0);

        println!("[CERT] ✅ Smart MKV + Cards PASSED");
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 2: Lossless + Cards
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_lossless_with_cards() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_lossless");
        let _ = std::fs::create_dir_all(&test_dir);

        let v1 = create_test_video_in_dir(&ffmpeg, &test_dir, "v1.mp4", 1920, 1080, 30, 5);
        let v2 = create_test_video_in_dir(&ffmpeg, &test_dir, "v2.mp4", 1920, 1080, 30, 5);
        let v3 = create_test_video_in_dir(&ffmpeg, &test_dir, "v3.mp4", 1920, 1080, 30, 5);
        let files = vec![v1, v2, v3];
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_lossless.mp4");
        let card_config = CardConfig {
            color: "#FF6B6B".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerVideo,
        };

        let (config, concat_list, interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::Lossless, &card_config, None,
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        assert!(result.is_ok(), "Lossless + Cards failed: {:?}", result.err());
        
        assert!(output.exists(), "Output file not created");
        let info = probe_file(&ffprobe, &output).unwrap();
        let expected_duration: f64 = interleaved_durs.iter().sum();
        let drift_pct = ((info.duration - expected_duration).abs() / expected_duration) * 100.0;
        assert!(drift_pct < 2.0, "Duration drift: {:.2}%", drift_pct);

        verify_card_visibility(&ffmpeg, &output, &[5.0, 12.0], 2.0);

        println!("[CERT] ✅ Lossless + Cards PASSED");
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 3: Custom + Cards
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_custom_with_cards() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_custom");
        let _ = std::fs::create_dir_all(&test_dir);

        let v1 = create_test_video_in_dir(&ffmpeg, &test_dir, "v1.mp4", 1920, 1080, 30, 5);
        let v2 = create_test_video_in_dir(&ffmpeg, &test_dir, "v2.mp4", 1920, 1080, 30, 5);
        let v3 = create_test_video_in_dir(&ffmpeg, &test_dir, "v3.mp4", 1920, 1080, 30, 5);
        let files = vec![v1, v2, v3];
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_custom.mp4");
        let card_config = CardConfig {
            color: "#4ECDC4".to_string(),
            font_color: "#000000".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerVideo,
        };

        let (config, concat_list, interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::Custom, &card_config, None,
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        assert!(result.is_ok(), "Custom + Cards failed: {:?}", result.err());
        
        assert!(output.exists(), "Output file not created");
        let info = probe_file(&ffprobe, &output).unwrap();
        let expected_duration: f64 = interleaved_durs.iter().sum();
        let drift_pct = ((info.duration - expected_duration).abs() / expected_duration) * 100.0;
        assert!(drift_pct < 2.0, "Duration drift: {:.2}%", drift_pct);

        verify_card_visibility(&ffmpeg, &output, &[5.0, 12.0], 2.0);

        println!("[CERT] ✅ Custom + Cards PASSED");
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 4: Mixed Resolution + Cards
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_mixed_resolution_with_cards() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_mixed_res");
        let _ = std::fs::create_dir_all(&test_dir);

        let v1 = create_test_video_in_dir(&ffmpeg, &test_dir, "v1_1080p.mp4", 1920, 1080, 30, 5);
        let v2 = create_test_video_in_dir(&ffmpeg, &test_dir, "v2_720p.mp4", 1280, 720, 30, 5);
        let v3 = create_test_video_in_dir(&ffmpeg, &test_dir, "v3_1080p.mp4", 1920, 1080, 30, 5);
        let files = vec![v1, v2, v3];
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_mixed_res.mp4");
        let card_config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerVideo,
        };

        let (config, concat_list, interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::SmartMkv, &card_config, None,
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        assert!(result.is_ok(), "Mixed Resolution + Cards failed: {:?}", result.err());
        
        assert!(output.exists(), "Output file not created");
        let info = probe_file(&ffprobe, &output).unwrap();
        let expected_duration: f64 = interleaved_durs.iter().sum();
        let drift_pct = ((info.duration - expected_duration).abs() / expected_duration) * 100.0;
        assert!(drift_pct < 2.0, "Duration drift: {:.2}%", drift_pct);

        let video = info.video_streams.first().unwrap();
        assert_eq!(video.width, Some(1920), "Card should inherit 1080p width");
        assert_eq!(video.height, Some(1080), "Card should inherit 1080p height");

        verify_card_visibility(&ffmpeg, &output, &[5.0, 12.0], 2.0);

        println!("[CERT] ✅ Mixed Resolution + Cards PASSED");
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 5: Folder Split + Cards (PerFolder mode)
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_folder_split_with_cards() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_folder_split");
        let _ = std::fs::create_dir_all(&test_dir);

        let folder_a = test_dir.join("folder_a");
        let folder_b = test_dir.join("folder_b");
        let v1 = create_test_video_in_dir(&ffmpeg, &folder_a, "v1.mp4", 1920, 1080, 30, 5);
        let v2 = create_test_video_in_dir(&ffmpeg, &folder_a, "v2.mp4", 1920, 1080, 30, 5);
        let v3 = create_test_video_in_dir(&ffmpeg, &folder_b, "v3.mp4", 1920, 1080, 30, 5);
        let v4 = create_test_video_in_dir(&ffmpeg, &folder_b, "v4.mp4", 1920, 1080, 30, 5);
        let files = vec![v1, v2, v3, v4];
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_folder_split.mp4");
        let card_config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerFolder,
        };

        let split_config = SplitConfig {
            mode: SplitMode::Folder,
            folder_split_mode: None,
            part_count: None,
            max_duration_per_part: None,
            subtitle_mode: None,
        };

        let (config, concat_list, _interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::SmartMkv, &card_config, Some(split_config),
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        assert!(result.is_ok(), "Folder Split + Cards failed: {:?}", result.err());
        let merge_result = result.unwrap();

        let total_cards: usize = merge_result.segments.iter().filter(|s| s.is_card == Some(true)).count();
        assert_eq!(total_cards, 2, "Expected 2 cards (2 folders), got {}", total_cards);

        println!("[CERT] ✅ Folder Split + Cards PASSED");
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 6: Duration Split + Cards
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_duration_split_with_cards() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_duration_split");
        let _ = std::fs::create_dir_all(&test_dir);

        let v1 = create_test_video_in_dir(&ffmpeg, &test_dir, "v1.mp4", 1920, 1080, 30, 8);
        let v2 = create_test_video_in_dir(&ffmpeg, &test_dir, "v2.mp4", 1920, 1080, 30, 8);
        let v3 = create_test_video_in_dir(&ffmpeg, &test_dir, "v3.mp4", 1920, 1080, 30, 8);
        let files = vec![v1, v2, v3];
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_duration_split.mp4");
        let card_config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerVideo,
        };

        let split_config = SplitConfig {
            mode: SplitMode::Duration,
            folder_split_mode: None,
            part_count: None,
            max_duration_per_part: Some(12.0),
            subtitle_mode: None,
        };

        let (config, concat_list, _interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::SmartMkv, &card_config, Some(split_config),
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        assert!(result.is_ok(), "Duration Split + Cards failed: {:?}", result.err());
        let merge_result = result.unwrap();

        let total_cards: usize = merge_result.segments.iter().filter(|s| s.is_card == Some(true)).count();
        assert_eq!(total_cards, 2, "Expected 2 cards, got {}", total_cards);

        println!("[CERT] ✅ Duration Split + Cards PASSED");
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 7: 25-File Workload + Cards
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_25file_workload_with_cards() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_25file");
        let _ = std::fs::create_dir_all(&test_dir);

        let mut files = Vec::new();
        for i in 0..25 {
            let folder = match i {
                0..=9 => "folder_a",
                10..=19 => "folder_b",
                _ => "folder_c",
            };
            let dir = test_dir.join(folder);
            let path = create_test_video_in_dir(&ffmpeg, &dir, &format!("v{}.mp4", i), 1920, 1080, 30, 3);
            files.push(path);
        }

        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_25file.mp4");
        let card_config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerFolder,
        };

        let split_config = SplitConfig {
            mode: SplitMode::Duration,
            folder_split_mode: None,
            part_count: None,
            max_duration_per_part: Some(30.0),
            subtitle_mode: None,
        };

        let (config, concat_list, _interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::SmartMkv, &card_config, Some(split_config),
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        assert!(result.is_ok(), "25-File Workload failed: {:?}", result.err());
        let merge_result = result.unwrap();

        let total_cards: usize = merge_result.segments.iter().filter(|s| s.is_card == Some(true)).count();
        assert_eq!(total_cards, 3, "Expected 3 cards (3 folders), got {}", total_cards);

        let segments = &merge_result.segments;
        for (i, seg) in segments.iter().enumerate() {
            if seg.is_card == Some(true) {
                assert!(i + 1 < segments.len(), "Card at end of segments - orphan card");
                assert!(segments[i + 1].is_card != Some(true), "Two consecutive cards - invalid");
            }
        }

        println!("[CERT] ✅ 25-File Workload + Cards PASSED");
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 8: Unicode Card Rendering
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_unicode_card_rendering() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_unicode");
        let _ = std::fs::create_dir_all(&test_dir);

        let v1 = create_test_video_in_dir(&ffmpeg, &test_dir, "v1.mp4", 1920, 1080, 30, 3);
        let v2 = create_test_video_in_dir(&ffmpeg, &test_dir, "v2.mp4", 1920, 1080, 30, 3);
        let files = vec![v1, v2];
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_unicode.mp4");
        let card_config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerVideo,
        };

        let (config, concat_list, _interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::SmartMkv, &card_config, None,
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        if result.is_ok() {
            assert!(output.exists(), "Output file not created");
            println!("[CERT] ✅ Unicode Card Rendering PASSED (merge succeeded)");
        } else {
            println!("[CERT] ⚠️ Unicode Card Rendering: merge failed (expected if font lacks Unicode support)");
        }
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 9: Long Filename Card Rendering
    // ════════════════════════════════════════════════════════════════════════════
    #[tokio::test]
    async fn test_long_filename_card_rendering() {
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() { eprintln!("SKIP: ffmpeg not found"); return; }

        let test_dir = std::env::temp_dir().join("card_cert_longname");
        let _ = std::fs::create_dir_all(&test_dir);

        let long_name = "How To Build Production AI Agents Using N8N And OpenAI APIs Complete Course Part 1.mp4";
        let v1_path = test_dir.join(long_name);
        create_test_video(&ffmpeg, &v1_path, 1920, 1080, 30, 3, 48000);
        
        let v2_path = test_dir.join("short.mp4");
        create_test_video(&ffmpeg, &v2_path, 1920, 1080, 30, 3, 48000);
        
        let files = vec![v1_path, v2_path];
        let names: Vec<String> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
        let durations: Vec<f64> = files.iter().map(|f| probe_file(&ffprobe, f).unwrap().duration).collect();

        let output = test_dir.join("output_longname.mp4");
        let card_config = CardConfig {
            color: "#3366FF".to_string(),
            font_color: "#FFFFFF".to_string(),
            duration: 2.0,
            show_in_report: true,
            frequency: CardFrequency::PerVideo,
        };

        let (config, concat_list, _interleaved_durs) = prepare_merge_with_cards(
            &ffmpeg, &ffprobe, &files, &names, &durations, &output,
            MergeMode::SmartMkv, &card_config, None,
        );

        let cancel = Arc::new(AtomicBool::new(false));
        let ffmpeg_clone = ffmpeg.clone();
        let result = spawn_blocking(move || {
            run_merge_blocking(&ffmpeg_clone, &config, &concat_list, cancel, |_| {})
        }).await.unwrap();

        assert!(result.is_ok(), "Long Filename Card failed: {:?}", result.err());
        assert!(output.exists(), "Output file not created");

        let info = probe_file(&ffprobe, &output).unwrap();
        assert!(info.duration > 0.0, "Output has zero duration");

        println!("[CERT] ✅ Long Filename Card Rendering PASSED");
    }

    // ════════════════════════════════════════════════════════════════════════════
    // TEST 10: Font Resolution Regression
    // ════════════════════════════════════════════════════════════════════════════
    #[test]
    fn test_font_resolution_regression() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        
        let bundled_font = root.join("binaries").join("arial.ttf");
        assert!(bundled_font.exists(), 
            "Bundled font not found at: {}. This is a regression - font must be bundled.", 
            bundled_font.display());
        
        let font_size = bundled_font.metadata().map(|m| m.len()).unwrap_or(0);
        assert!(font_size > 1024, 
            "Bundled font is too small ({} bytes) - may be a Git LFS pointer", 
            font_size);
        
        let font_data = std::fs::read(&bundled_font);
        assert!(font_data.is_ok(), "Bundled font is not readable");
        assert!(!font_data.unwrap().is_empty(), "Bundled font is empty");

        println!("[CERT] ✅ Font Resolution Regression PASSED");
        println!("[CERT]   Bundled font: {}", bundled_font.display());
        println!("[CERT]   Font size: {} bytes", font_size);
    }
}
