use super::*;

impl MediaValidationEngine {
    pub fn repair_single(&self, mut state: FileState, _input_dir: &std::path::Path) -> FileState {
        let file_name = state.original_name.clone();
        let original_path = state.original_path.clone();
        let original_size = std::fs::metadata(&original_path).map(|m| m.len()).unwrap_or(0);

        match &state.disposition {
            FileDisposition::Healthy | FileDisposition::NeedsNormalization | FileDisposition::Unrepairable(_) => {
                state.repair_status = match &state.disposition {
                    FileDisposition::Healthy => RepairStatus::Skipped,
                    FileDisposition::NeedsNormalization => RepairStatus::Skipped,
                    FileDisposition::Unrepairable(_) => RepairStatus::Quarantined,
                    _ => RepairStatus::Skipped,
                };
                state.repair_duration_ms = 0.0;
                return state;
            }
            FileDisposition::Repairable(damage) => {
                let damage = damage.clone();
                match damage {
                    DamageClassification::SubtitleDamage => {
                        log::info!("[REPAIR_SINGLE] {} - Phase A: Subtitle repair attempt", file_name);
                        let phase_start = std::time::Instant::now();

                        match self.try_fix_subtitle_remux(state.file_index, &original_path) {
                            Some(fixed_path) => {
                                let post_fix = self.check_critical_post_repair(&fixed_path, original_size);
                                let orig_identity = self.get_stream_identity(&original_path);
                                let rep_identity = self.get_stream_identity(&fixed_path);
                                let stream_issues = self.check_stream_identity(&orig_identity, &rep_identity);

                                if post_fix.is_empty() && stream_issues.is_empty() {
                                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                                    log::info!("[REPAIR_SINGLE] {} - Subtitle repair PASSED ({:.1}ms)", file_name, phase_ms);
                                    state.repair_status = RepairStatus::Succeeded;
                                    state.fix_applied = Some(FixType::ContainerRemux);
                                    state.repaired_path = Some(fixed_path.clone());
                                    state.final_path = fixed_path.clone();
                                    state.remux_type = Some(RemuxType::Repair);
                                    state.repair_reason = Some("subtitle repair".to_string());
                                    state.repair_trace.push(RepairTraceEntry {
                                        function: "SubtitleRemux".to_string(),
                                        outcome: "succeeded".to_string(),
                                        output_path: Some(fixed_path),
                                        details: format!("Subtitle repair passed in {:.1}ms", phase_ms),
                                        effectiveness: None,
                                    });
                                    state.repair_duration_ms = phase_start.elapsed().as_millis() as f64;
                                    return state;
                                } else {
                                    let _phase_ms = phase_start.elapsed().as_millis() as f64;
                                    let mut issues = post_fix;
                                    issues.extend(stream_issues);
                                    log::warn!("[REPAIR_SINGLE] {} - Subtitle repair passed but revalidation failed: {:?}", file_name, issues);
                                    let _ = std::fs::remove_file(&fixed_path);
                                    state.repair_trace.push(RepairTraceEntry {
                                        function: "SubtitleRemux".to_string(),
                                        outcome: "failed".to_string(),
                                        output_path: None,
                                        details: format!("Revalidation failed: {}", issues.join("; ")),
                                        effectiveness: None,
                                    });
                                    log::info!("[REPAIR_SINGLE] {} - Subtitle repair failed, escalating to Phase B", file_name);
                                }
                            }
                            None => {
                                state.repair_trace.push(RepairTraceEntry {
                                    function: "SubtitleRemux".to_string(),
                                    outcome: "failed".to_string(),
                                    output_path: None,
                                    details: "try_fix_subtitle_remux returned None".to_string(),
                                    effectiveness: None,
                                });
                            }
                        }
                        let file_index = state.file_index;
                        self.repair_timestamp_or_reencode(state, file_index, &original_path, original_size)
                    }
                    DamageClassification::TimestampDamage
                    | DamageClassification::ContainerDamage
                    | DamageClassification::NeedsReencode
                    | DamageClassification::VideoDecodeFailure
                    | DamageClassification::BitstreamCorruption
                    | DamageClassification::PacketCorruption
                    | DamageClassification::VideoFrameCorruption
                    | DamageClassification::AttachmentDamage => {
                        let file_index = state.file_index;
                        self.repair_timestamp_or_reencode(state, file_index, &original_path, original_size)
                    }
                    DamageClassification::Healthy | DamageClassification::Unsupported => {
                        state.repair_status = RepairStatus::Skipped;
                        state.repair_duration_ms = 0.0;
                        state
                    }
                }
            }
        }
    }

    fn repair_timestamp_or_reencode(&self, mut state: FileState, file_index: usize, original_path: &str, original_size: u64) -> FileState {
        let file_name = state.original_name.clone();
        let phase_start = std::time::Instant::now();

        log::info!("[REPAIR_SINGLE] {} - Phase B: Timestamp/Container repair attempt", file_name);

        match self.try_fix_timestamp_repair(file_index, original_path) {
            Some(fixed_path) => {
                let post_fix = self.check_critical_post_repair(&fixed_path, original_size);
                let orig_identity = self.get_stream_identity(original_path);
                let rep_identity = self.get_stream_identity(&fixed_path);
                let stream_issues = self.check_stream_identity(&orig_identity, &rep_identity);

                if post_fix.is_empty() && stream_issues.is_empty() {
                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                    log::info!("[REPAIR_SINGLE] {} - Timestamp repair PASSED ({:.1}ms)", file_name, phase_ms);
                    state.repair_status = RepairStatus::Succeeded;
                    state.fix_applied = Some(FixType::TimestampRepair);
                    state.repaired_path = Some(fixed_path.clone());
                    state.final_path = fixed_path.clone();
                    state.remux_type = Some(RemuxType::Repair);
                    state.repair_reason = Some("timestamp repair".to_string());
                    state.repair_trace.push(RepairTraceEntry {
                        function: "TimestampRemux".to_string(),
                        outcome: "succeeded".to_string(),
                        output_path: Some(fixed_path),
                        details: format!("Timestamp repair passed in {:.1}ms", phase_ms),
                        effectiveness: None,
                    });
                    state.repair_duration_ms = phase_start.elapsed().as_millis() as f64;
                    return state;
                } else {
                    let _phase_ms = phase_start.elapsed().as_millis() as f64;
                    let mut issues = post_fix;
                    issues.extend(stream_issues);
                    log::warn!("[REPAIR_SINGLE] {} - Timestamp repair passed but revalidation failed: {:?}", file_name, issues);
                    let _ = std::fs::remove_file(&fixed_path);
                    state.repair_trace.push(RepairTraceEntry {
                        function: "TimestampRemux".to_string(),
                        outcome: "failed".to_string(),
                        output_path: None,
                        details: format!("Revalidation failed: {}", issues.join("; ")),
                        effectiveness: None,
                    });
                }
            }
            None => {
                let _phase_ms = phase_start.elapsed().as_millis() as f64;
                state.repair_trace.push(RepairTraceEntry {
                    function: "TimestampRemux".to_string(),
                    outcome: "failed".to_string(),
                    output_path: None,
                    details: "try_fix_timestamp_repair returned None".to_string(),
                    effectiveness: None,
                });
            }
        }

        log::info!("[REPAIR_SINGLE] {} - Timestamp repair failed, escalating to Phase C (re-encode)", file_name);
        self.repair_reencode_only(state, file_index, original_path, original_size)
    }

    fn repair_reencode_only(&self, mut state: FileState, file_index: usize, original_path: &str, original_size: u64) -> FileState {
        let file_name = state.original_name.clone();
        let phase_start = std::time::Instant::now();

        log::info!("[REPAIR_SINGLE] {} - Phase C: Re-encode attempt", file_name);

        match self.try_fix_reencode(file_index, original_path) {
            Some(fixed_path) => {
                let post_fix = self.check_critical_post_repair(&fixed_path, original_size);
                let orig_identity = self.get_stream_identity(original_path);
                let rep_identity = self.get_stream_identity(&fixed_path);
                let stream_issues = self.check_stream_identity(&orig_identity, &rep_identity);

                if post_fix.is_empty() && stream_issues.is_empty() {
                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                    log::info!("[REPAIR_SINGLE] {} - Re-encode PASSED ({:.1}ms)", file_name, phase_ms);
                    state.repair_status = RepairStatus::Succeeded;
                    state.fix_applied = Some(FixType::FullReencode);
                    state.repaired_path = Some(fixed_path.clone());
                    state.final_path = fixed_path.clone();
                    state.remux_type = Some(RemuxType::Repair);
                    state.repair_reason = Some("full reencode".to_string());
                    state.repair_trace.push(RepairTraceEntry {
                        function: "Reencode".to_string(),
                        outcome: "succeeded".to_string(),
                        output_path: Some(fixed_path),
                        details: format!("Re-encode passed in {:.1}ms", phase_ms),
                        effectiveness: None,
                    });
                    state.repair_duration_ms = phase_start.elapsed().as_millis() as f64;
                    return state;
                } else {
                    let _phase_ms = phase_start.elapsed().as_millis() as f64;
                    let mut issues = post_fix;
                    issues.extend(stream_issues);
                    log::warn!("[REPAIR_SINGLE] {} - Re-encode passed but revalidation failed: {:?}", file_name, issues);
                    let _ = std::fs::remove_file(&fixed_path);
                    state.repair_trace.push(RepairTraceEntry {
                        function: "Reencode".to_string(),
                        outcome: "failed".to_string(),
                        output_path: None,
                        details: format!("Revalidation failed: {}", issues.join("; ")),
                        effectiveness: None,
                    });
                }
            }
            None => {
                let _phase_ms = phase_start.elapsed().as_millis() as f64;
                state.repair_trace.push(RepairTraceEntry {
                    function: "Reencode".to_string(),
                    outcome: "failed".to_string(),
                    output_path: None,
                    details: "try_fix_reencode returned None".to_string(),
                    effectiveness: None,
                });
            }
        }

        log::warn!("[REPAIR_SINGLE] {} - All repair attempts failed, quarantining", file_name);
        state.repair_status = RepairStatus::Quarantined;
        state.disposition = FileDisposition::Unrepairable(DamageClassification::Unsupported);
        state.repair_duration_ms = phase_start.elapsed().as_millis() as f64;
        state
    }

    fn try_fix_subtitle_remux(&self, _file_index: usize, file_path: &str) -> Option<String> {
        // P0-4: Subtitle remux is specialized for subtitle stream issues:
        // negative timestamps, broken numbering, PGS/dvd subtitle problems.
        // Stream-copies all streams but focuses subtitle container repair.
        let input_path = std::path::Path::new(file_path);
        let stem = input_path.file_stem()?.to_string_lossy();
        let output_path = self.temp_dir.join(format!("{}_sub_fix.mkv", stem));
        let output_str = output_path.to_string_lossy().to_string();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd
            .args([
                "-y", "-i", file_path,
                "-c", "copy",
                "-map", "0",
                // Regenerate PTS to fix subtitle timestamp issues
                "-fflags", "+genpts",
                // Shift negative timestamps to zero
                "-avoid_negative_ts", "make_zero",
                // Preserve container-level metadata (chapters, tags, attachments)
                "-map_metadata", "0",
                // Prevent subtitle queue overflow on files with many subtitle streams
                "-max_muxing_queue_size", "4096",
                &output_str,
            ])
            .output();

        match status {
            Ok(out) if out.status.success() => Some(output_str),
            _ => None,
        }
    }

    fn try_fix_timestamp_repair(&self, _file_index: usize, file_path: &str) -> Option<String> {
        // P0-4: Timestamp repair is specialized for timestamp problems, NOT a generic remux.
        // It targets: negative PTS, missing DTS, non-monotonic timestamps, PTS < DTS.
        let input_path = std::path::Path::new(file_path);
        let stem = input_path.file_stem()?.to_string_lossy();
        let output_path = self.temp_dir.join(format!("{}_ts_fix.mkv", stem));
        let output_str = output_path.to_string_lossy().to_string();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd
            .args([
                "-y", "-i", file_path,
                "-c", "copy",
                "-map", "0",
                // Fix negative timestamps by shifting to zero
                "-avoid_negative_ts", "make_zero",
                // Regenerate PTS from DTS — fixes missing PTS and non-monotonic PTS
                "-fflags", "+genpts",
                // Normalize time_base: set video track timescale to standard NTSC (30000)
                // This differentiates timestamp repair from subtitle remux
                "-video_track_timescale", "30000",
                // Prevent muxing queue overflow on damaged files with timestamp issues
                "-max_muxing_queue_size", "4096",
                &output_str,
            ])
            .output();

        match status {
            Ok(out) if out.status.success() => Some(output_str),
            _ => None,
        }
    }

    fn try_fix_reencode(&self, _file_index: usize, file_path: &str) -> Option<String> {
        let input_path = std::path::Path::new(file_path);
        let stem = input_path.file_stem()?.to_string_lossy();
        let output_path = self.temp_dir.join(format!("{}_reencode.mkv", stem));
        let output_str = output_path.to_string_lossy().to_string();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let status = cmd
            .args([
                "-y", "-i", file_path,
                "-c:v", "libx264",
                "-preset", "fast",
                "-crf", "18",
                "-c:a", "aac",
                "-b:a", "192k",
                "-map", "0",
                "-avoid_negative_ts", "make_zero",
                "-fflags", "+genpts",
                &output_str,
            ])
            .output();

        match status {
            Ok(out) if out.status.success() => Some(output_str),
            _ => None,
        }
    }

    fn check_critical_post_repair(&self, fixed_path: &str, original_size: u64) -> Vec<String> {
        let mut issues = Vec::new();

        let path = std::path::Path::new(fixed_path);
        if !path.exists() {
            issues.push("repaired file does not exist".to_string());
            return issues;
        }

        let new_size = match std::fs::metadata(path) {
            Ok(m) => m.len(),
            Err(_) => {
                issues.push("cannot read repaired file metadata".to_string());
                return issues;
            }
        };

        if new_size == 0 {
            issues.push("repaired file is empty".to_string());
            return issues;
        }

        if original_size > 0 {
            let ratio = new_size as f64 / original_size as f64;
            if ratio < 0.01 {
                issues.push(format!("repaired file size ratio too small: {:.4}", ratio));
            } else if ratio > 10.0 {
                issues.push(format!("repaired file size ratio too large: {:.4}", ratio));
            }
        }

        let output = std::process::Command::new(&self.ffprobe_path)
            .args(["-v", "error", "-i", fixed_path])
            .output();

        match output {
            Ok(out) => {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    issues.push(format!("post-repair probe failed: {}", stderr.trim()));
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let err_lines: Vec<&str> = stderr.lines()
                        .filter(|l| !l.trim().is_empty())
                        .collect();
                    if !err_lines.is_empty() {
                        issues.push(format!("post-repair warnings: {}", err_lines.join("; ")));
                    }
                }
            }
            Err(e) => {
                issues.push(format!("cannot execute ffprobe for post-repair: {}", e));
            }
        }

        issues
    }

    fn get_stream_identity(&self, file_path: &str) -> StreamIdentity {
        let path = std::path::Path::new(file_path);
        match crate::ffmpeg::probe::probe_file(&self.ffprobe_path, path) {
            Ok(info) => {
                let video_codecs: Vec<String> = info.video_streams.iter()
                    .map(|s| s.codec_name.clone())
                    .collect();
                let audio_codecs: Vec<String> = info.audio_streams.iter()
                    .map(|s| s.codec_name.clone())
                    .collect();
                // P0-3: Extract rich metadata for post-repair identity verification
                // VideoStream has no 'language' field — only audio/subtitle streams carry language
                let video_languages: Vec<Option<String>> = Vec::new();
                let audio_languages: Vec<Option<String>> = info.audio_streams.iter()
                    .map(|s| s.language.clone())
                    .collect();
                let video_color_transfer: Vec<Option<String>> = info.video_streams.iter()
                    .map(|s| s.color_transfer.clone())
                    .collect();
                let video_color_space: Vec<Option<String>> = info.video_streams.iter()
                    .map(|s| s.color_space.clone())
                    .collect();
                let video_rotation: Vec<Option<i32>> = info.video_streams.iter()
                    .map(|s| s.rotation)
                    .collect();
                StreamIdentity {
                    video_count: info.video_streams.len(),
                    audio_count: info.audio_streams.len(),
                    subtitle_count: info.subtitle_streams.len(),
                    total_streams: info.video_streams.len() + info.audio_streams.len() + info.subtitle_streams.len(),
                    video_codecs,
                    audio_codecs,
                    duration: Some(info.duration),
                    video_languages,
                    audio_languages,
                    video_color_transfer,
                    video_color_space,
                    video_rotation,
                }
            }
            Err(_) => StreamIdentity::empty(),
        }
    }

    fn check_stream_identity(&self, orig: &StreamIdentity, rep: &StreamIdentity) -> Vec<String> {
        let mut issues = Vec::new();

        if orig.video_count != rep.video_count {
            issues.push(format!(
                "video stream count mismatch: original={}, repaired={}",
                orig.video_count, rep.video_count
            ));
        }

        if orig.audio_count != rep.audio_count {
            issues.push(format!(
                "audio stream count mismatch: original={}, repaired={}",
                orig.audio_count, rep.audio_count
            ));
        }

        if orig.subtitle_count != rep.subtitle_count {
            issues.push(format!(
                "subtitle stream count mismatch: original={}, repaired={}",
                orig.subtitle_count, rep.subtitle_count
            ));
        }

        // Video codec verification
        for (i, codec) in orig.video_codecs.iter().enumerate() {
            if i >= rep.video_codecs.len() {
                issues.push(format!("missing video codec at index {}: {}", i, codec));
            } else if &rep.video_codecs[i] != codec {
                issues.push(format!(
                    "video codec mismatch at index {}: original={}, repaired={}",
                    i, codec, rep.video_codecs[i]
                ));
            }
        }

        // Audio codec verification
        for (i, codec) in orig.audio_codecs.iter().enumerate() {
            if i >= rep.audio_codecs.len() {
                issues.push(format!("missing audio codec at index {}: {}", i, codec));
            } else if &rep.audio_codecs[i] != codec {
                issues.push(format!(
                    "audio codec mismatch at index {}: original={}, repaired={}",
                    i, codec, rep.audio_codecs[i]
                ));
            }
        }

        // P0-3: Verify language tags preserved (log-only — optional metadata)
        for (i, lang) in orig.audio_languages.iter().enumerate() {
            if let Some(rep_lang) = rep.audio_languages.get(i) {
                if lang != rep_lang {
                    log::warn!("[STREAM_IDENTITY] Audio language drift at index {}: {:?} -> {:?}", i, lang, rep_lang);
                }
            }
        }

        // P0-3: Verify HDR metadata (color_transfer) preserved — CRITICAL
        for (i, ct) in orig.video_color_transfer.iter().enumerate() {
            if let Some(rep_ct) = rep.video_color_transfer.get(i) {
                if ct != rep_ct {
                    issues.push(format!(
                        "HDR metadata loss: video stream {} color_transfer {:?} -> {:?}",
                        i, ct, rep_ct
                    ));
                }
            }
        }

        // P0-3: Verify color_space preserved — CRITICAL
        for (i, cs) in orig.video_color_space.iter().enumerate() {
            if let Some(rep_cs) = rep.video_color_space.get(i) {
                if cs != rep_cs {
                    issues.push(format!(
                        "Color space drift: video stream {} color_space {:?} -> {:?}",
                        i, cs, rep_cs
                    ));
                }
            }
        }

        // P0-3: Verify rotation preserved — CRITICAL
        for (i, rot) in orig.video_rotation.iter().enumerate() {
            if let Some(rep_rot) = rep.video_rotation.get(i) {
                if rot != rep_rot {
                    issues.push(format!(
                        "Rotation mismatch: video stream {} rotation {:?} -> {:?}",
                        i, rot, rep_rot
                    ));
                }
            }
        }

        issues
    }
}