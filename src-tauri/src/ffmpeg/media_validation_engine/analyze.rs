use super::*;

impl MediaValidationEngine {
    pub fn analyze_single(&self, file_index: usize, file_path: &str) -> FileState {
        let file_start = std::time::Instant::now();
        let file_name = std::path::Path::new(file_path)
            .file_name().map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());
        let original_duration = self.get_duration_secs(file_path);
        let original_size = std::fs::metadata(file_path).map(|m| m.len()).unwrap_or(0);

        let p1_start = std::time::Instant::now();
        let quick_result = self.quick_container_check(file_path);
        let phase1_ms = p1_start.elapsed().as_millis() as f64;

        let mut has_container_critical = false;
        let mut container_issues_from_quick: Vec<ContainerIssue> = Vec::new();
        if let Err(ref issues) = quick_result {
            container_issues_from_quick = issues.clone();
            has_container_critical = issues.iter().any(|i| i.severity == IssueSeverity::Critical);
        }

        let p2_start = std::time::Instant::now();

        let stream_types = self.get_stream_codec_types(file_path);
        let video_audio_streams: std::collections::HashSet<usize> = stream_types.iter()
            .filter(|(_, ct)| *ct == "video" || *ct == "audio")
            .map(|(si, _)| *si)
            .collect();
        let subtitle_streams: std::collections::HashSet<usize> = stream_types.iter()
            .filter(|(_, ct)| *ct == "subtitle")
            .map(|(si, _)| *si)
            .collect();

        let pts_issues = self.check_pts_monotonic(file_path, &video_audio_streams);
        let dts_issues = self.check_dts_validity(file_path, &video_audio_streams);
        let timebase_issues = self.check_timebase_consistency(file_path);
        let vfr_instability = self.check_vfr_instability(file_path, &video_audio_streams);
        let deep_container_issues = self.check_container_corruption(file_path);
        let subtitle_issues = self.check_subtitle_validity(file_path, &subtitle_streams);

        let phase2_ms = p2_start.elapsed().as_millis() as f64;

        let phase9_start = std::time::Instant::now();
        let phase2_clean = pts_issues.is_empty() && dts_issues.is_empty() && subtitle_issues.is_empty()
            && timebase_issues.is_empty() && container_issues_from_quick.is_empty() && deep_container_issues.is_empty();
        let (video_decode_issues, bitstream_issues, packet_issues, frame_issues, attachment_issues) =
            if phase2_clean {
                let bitstream = self.check_bitstream_integrity(file_path);
                (Vec::new(), bitstream, Vec::new(), Vec::new(), Vec::new())
            } else {
                (
                    self.check_video_decode(file_path),
                    self.check_bitstream_integrity(file_path),
                    self.check_packet_integrity(file_path),
                    self.check_video_frame_integrity(file_path),
                    self.check_attachment_integrity(file_path),
                )
            };

        let phase9_ms = phase9_start.elapsed().as_millis() as f64;

        let phase9_mode = if phase2_clean { "FAST" } else { "FULL" };
        let phase9_reason = if phase2_clean {
            if !bitstream_issues.is_empty() {
                "bitstream_issues_detected"
            } else {
                "healthy_after_phase2"
            }
        } else {
            "phase2_issues_detected"
        };

        let cl_start = std::time::Instant::now();

        let has_pts_issues = !pts_issues.is_empty();
        let has_dts_issues = !dts_issues.is_empty();
        let has_subtitle_issues = !subtitle_issues.is_empty();
        let has_timebase_issues = !timebase_issues.is_empty();
        let has_video_decode_issues = !video_decode_issues.is_empty();
        let has_bitstream_issues = !bitstream_issues.is_empty();
        let has_packet_issues = !packet_issues.is_empty();
        let has_frame_issues = !frame_issues.is_empty();
        let has_attachment_issues = !attachment_issues.is_empty();

        let damage = self.classify_damage_extended(
            has_container_critical,
            has_pts_issues, has_dts_issues,
            has_subtitle_issues, has_timebase_issues,
            has_video_decode_issues,
            has_bitstream_issues,
            has_packet_issues,
            has_frame_issues,
            has_attachment_issues,
        );

        let classify_ms = cl_start.elapsed().as_millis() as f64;

        let disposition = match &damage {
            DamageClassification::Healthy => {
                if vfr_instability.is_empty() {
                    FileDisposition::Healthy
                } else {
                    FileDisposition::Healthy
                }
            }
            DamageClassification::TimestampDamage
            | DamageClassification::ContainerDamage
            | DamageClassification::SubtitleDamage
            | DamageClassification::NeedsReencode
            | DamageClassification::VideoDecodeFailure
            | DamageClassification::BitstreamCorruption
            | DamageClassification::PacketCorruption
            | DamageClassification::VideoFrameCorruption
            | DamageClassification::AttachmentDamage => FileDisposition::Repairable(damage.clone()),
            DamageClassification::Unsupported => FileDisposition::Unrepairable(damage.clone()),
        };

        let mut reasons = Vec::new();
        if has_container_critical { reasons.push("container_critical".to_string()); }
        if has_pts_issues { reasons.push("pts".to_string()); }
        if has_dts_issues { reasons.push("dts".to_string()); }
        if has_subtitle_issues { reasons.push("subtitle".to_string()); }
        if has_timebase_issues { reasons.push("timebase".to_string()); }
        if has_video_decode_issues { reasons.push("video_decode".to_string()); }
        if has_bitstream_issues { reasons.push("bitstream".to_string()); }
        if has_packet_issues { reasons.push("packet".to_string()); }
        if has_frame_issues { reasons.push("frame".to_string()); }
        if has_attachment_issues { reasons.push("attachment".to_string()); }
        let analysis_reason = if reasons.is_empty() {
            "healthy".to_string()
        } else {
            reasons.join(", ")
        };

        let total_ms = file_start.elapsed().as_millis() as f64;

        if total_ms > 1000.0 {
            log::warn!("[STAGE_TIMING] SLOW_ANALYSIS file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms({}:{}) classify={:.0}ms disposition={:?}",
                file_name, file_index, total_ms, phase1_ms, phase2_ms, phase9_ms, phase9_mode, phase9_reason, classify_ms, disposition);
        } else {
            log::trace!("[STAGE_TIMING] ANALYSIS file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms({}:{}) classify={:.0}ms",
                file_name, file_index, total_ms, phase1_ms, phase2_ms, phase9_ms, phase9_mode, phase9_reason, classify_ms);
        }

        FileState {
            file_index,
            original_path: file_path.to_string(),
            original_name: file_name,
            original_size,
            original_duration_secs: original_duration,
            disposition,
            damage_classification: damage,
            confidence: 1.0,
            analysis_reason,
            analysis_timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            has_pts_issues,
            has_dts_issues,
            has_subtitle_issues,
            has_container_issues: !container_issues_from_quick.is_empty() || !deep_container_issues.is_empty(),
            has_video_decode_issues,
            has_bitstream_issues,
            has_packet_issues,
            has_frame_issues,
            has_attachment_issues,
            has_timebase_issues,
            has_vfr_instability: !vfr_instability.is_empty(),
            analysis_duration_ms: total_ms,
            repair_status: RepairStatus::Skipped,
            fix_applied: None,
            remux_type: None,
            repair_reason: None,
            repaired_path: None,
            repair_trace: Vec::new(),
            repair_duration_ms: 0.0,
            revalidation_status: RevalidationStatus::NotNeeded,
            revalidation_duration_ms: 0.0,
            current_size: original_size,
            current_duration_secs: original_duration,
            current_path: file_path.to_string(),
            final_path: file_path.to_string(),
        }
    }

    pub(super) fn quick_container_check(&self, file_path: &str) -> Result<(), Vec<ContainerIssue>> {
        let output = std::process::Command::new(&self.ffprobe_path)
            .args(["-v", "error", "-i", file_path])
            .output();

        match output {
            Ok(out) => {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let issue_type = if stderr.contains("No such file") || stderr.contains("does not exist") {
                        "file_not_found"
                    } else if stderr.contains("Permission denied") {
                        "permission_denied"
                    } else if stderr.contains("Invalid data") || stderr.contains("moov atom not found") {
                        "corrupt_container"
                    } else {
                        "probe_failed"
                    };
                    return Err(vec![ContainerIssue {
                        severity: IssueSeverity::Critical,
                        stream_index: None,
                        issue_type: issue_type.to_string(),
                        message: stderr.trim().to_string(),
                    }]);
                }
                let stderr = String::from_utf8_lossy(&out.stderr);
                if !stderr.trim().is_empty() {
                    return Err(vec![ContainerIssue {
                        severity: IssueSeverity::Warning,
                        stream_index: None,
                        issue_type: "container_warning".to_string(),
                        message: stderr.trim().to_string(),
                    }]);
                }
                Ok(())
            }
            Err(e) => {
                let issue_type = if e.kind() == std::io::ErrorKind::NotFound {
                    "ffprobe_not_found"
                } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                    "permission_denied"
                } else {
                    "cannot_execute_ffprobe"
                };
                Err(vec![ContainerIssue {
                    severity: IssueSeverity::Critical,
                    stream_index: None,
                    issue_type: issue_type.to_string(),
                    message: format!("{}", e),
                }])
            }
        }
    }

    fn get_stream_codec_types(&self, file_path: &str) -> Vec<(usize, String)> {
        let path = std::path::Path::new(file_path);
        match crate::ffmpeg::probe::probe_file(&self.ffprobe_path, path) {
            Ok(info) => {
                let mut result = Vec::new();
                for stream in &info.video_streams {
                    result.push((stream.stream_index as usize, "video".to_string()));
                }
                for stream in &info.audio_streams {
                    result.push((stream.stream_index as usize, "audio".to_string()));
                }
                for stream in &info.subtitle_streams {
                    result.push((stream.stream_index as usize, "subtitle".to_string()));
                }
                result.sort_by_key(|(idx, _)| *idx);
                result
            }
            Err(_) => Vec::new(),
        }
    }

    fn check_pts_monotonic(&self, file_path: &str, streams: &std::collections::HashSet<usize>) -> Vec<TimestampIssue> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-show_packets",
            "-show_entries", "packet=stream_index,pts,dts,codec_type",
            file_path,
        ];

        let output = match std::process::Command::new(&self.ffprobe_path).args(&args).output() {
            Ok(out) => out,
            Err(_) => return Vec::new(),
        };

        if !output.status.success() {
            return Vec::new();
        }

        let json = match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        let packets = match json.get("packets").and_then(|p| p.as_array()) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut issues = Vec::new();
        let mut stream_packets: std::collections::HashMap<usize, Vec<(usize, i64)>> = std::collections::HashMap::new();

        for (i, p) in packets.iter().enumerate() {
            let stream_index = p.get("stream_index").and_then(|v| v.as_i64()).unwrap_or(-1) as usize;
            let pts = p.get("pts").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
            let codec_type = p.get("codec_type").and_then(|v| v.as_str()).unwrap_or("");

            if streams.contains(&stream_index) || (codec_type == "video" && streams.is_empty()) {
                stream_packets.entry(stream_index).or_default().push((i, pts));
            }
        }

        for (stream_index, mut packets) in stream_packets {
            if packets.len() < 2 {
                continue;
            }
            packets.sort_by_key(|(_, pts)| *pts);

            let mut prev_pts = i64::MIN;
            for (packet_index, pts) in packets {
                if pts != i64::MIN && prev_pts != i64::MIN && pts < prev_pts {
                    issues.push(TimestampIssue {
                        stream_index,
                        stream_type: "video".to_string(),
                        packet_index,
                        issue_type: "pts_non_monotonic".to_string(),
                        pts: Some(pts),
                        dts: None,
                    });
                }
                if pts != i64::MIN {
                    prev_pts = pts;
                }
            }
        }

        issues
    }

    fn check_dts_validity(&self, file_path: &str, streams: &std::collections::HashSet<usize>) -> Vec<TimestampIssue> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-show_packets",
            "-show_entries", "packet=stream_index,pts,dts,codec_type",
            file_path,
        ];

        let output = match std::process::Command::new(&self.ffprobe_path).args(&args).output() {
            Ok(out) => out,
            Err(_) => return Vec::new(),
        };

        if !output.status.success() {
            return Vec::new();
        }

        let json = match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        let packets = match json.get("packets").and_then(|p| p.as_array()) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut issues = Vec::new();
        let mut stream_packets: std::collections::HashMap<usize, Vec<(usize, i64)>> = std::collections::HashMap::new();

        for (i, p) in packets.iter().enumerate() {
            let stream_index = p.get("stream_index").and_then(|v| v.as_i64()).unwrap_or(-1) as usize;
            let dts = p.get("dts").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
            let codec_type = p.get("codec_type").and_then(|v| v.as_str()).unwrap_or("");

            if streams.contains(&stream_index) || (codec_type == "video" && streams.is_empty()) {
                stream_packets.entry(stream_index).or_default().push((i, dts));
            }
        }

        for (stream_index, mut packets) in stream_packets {
            if packets.len() < 2 {
                continue;
            }
            packets.sort_by_key(|(_, dts)| *dts);

            let mut prev_dts = i64::MIN;
            for (packet_index, dts) in packets {
                if dts != i64::MIN && prev_dts != i64::MIN && dts < prev_dts {
                    issues.push(TimestampIssue {
                        stream_index,
                        stream_type: "video".to_string(),
                        packet_index,
                        issue_type: "dts_non_monotonic".to_string(),
                        pts: None,
                        dts: Some(dts),
                    });
                }
                if dts != i64::MIN {
                    prev_dts = dts;
                }
            }
        }

        issues
    }

    fn check_timebase_consistency(&self, file_path: &str) -> Vec<TimestampIssue> {
        let path = std::path::Path::new(file_path);
        let info = match crate::ffmpeg::probe::probe_file(&self.ffprobe_path, path) {
            Ok(i) => i,
            Err(_) => return Vec::new(),
        };

        let mut issues = Vec::new();
        let video_streams: Vec<_> = info.video_streams.iter().collect();

        if video_streams.len() < 2 {
            return Vec::new();
        }

        let timebases: Vec<(usize, String)> = video_streams.iter()
            .filter_map(|s| s.time_base.as_ref().map(|tb| (s.stream_index as usize, tb.clone())))
            .collect();

        if timebases.is_empty() {
            return Vec::new();
        }

        let mut freq_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (_, tb) in &timebases {
            *freq_counts.entry(tb.clone()).or_default() += 1;
        }

        let dominant_tb = freq_counts.iter().max_by_key(|(_, c)| *c).map(|(tb, _)| tb.clone());

        if let Some(dominant) = dominant_tb {
            for (stream_index, tb) in &timebases {
                if tb != &dominant {
                    issues.push(TimestampIssue {
                        stream_index: *stream_index,
                        stream_type: "video".to_string(),
                        packet_index: 0,
                        issue_type: "timebase_inconsistent".to_string(),
                        pts: None,
                        dts: None,
                    });
                }
            }
        }

        issues
    }

    fn check_vfr_instability(&self, file_path: &str, _streams: &std::collections::HashSet<usize>) -> Vec<TimestampIssue> {
        let path = std::path::Path::new(file_path);
        let info = match crate::ffmpeg::probe::probe_file(&self.ffprobe_path, path) {
            Ok(i) => i,
            Err(_) => return Vec::new(),
        };

        let mut issues = Vec::new();

        for stream in &info.video_streams {
            let rfr = stream.r_frame_rate.as_deref();
            let afr = stream.avg_frame_rate.as_deref();

            if let (Some(rfr_str), Some(afr_str)) = (rfr, afr) {
                let rfr_val = Self::parse_frame_rate(rfr_str);
                let afr_val = Self::parse_frame_rate(afr_str);

                if let (Some(r), Some(a)) = (rfr_val, afr_val) {
                    if (r - a).abs() > 0.1 {
                        issues.push(TimestampIssue {
                            stream_index: stream.stream_index as usize,
                            stream_type: "video".to_string(),
                            packet_index: 0,
                            issue_type: "vfr_detected".to_string(),
                            pts: None,
                            dts: None,
                        });
                    }
                }
            }
        }

        issues
    }

    fn parse_frame_rate(s: &str) -> Option<f64> {
        let parts: Vec<&str> = s.split('/').collect();
        match parts.len() {
            2 => {
                let num: f64 = parts[0].parse().ok()?;
                let den: f64 = parts[1].parse().ok()?;
                if den == 0.0 { return None; }
                Some(num / den)
            }
            1 => parts[0].parse().ok(),
            _ => None,
        }
    }

    fn check_container_corruption(&self, file_path: &str) -> Vec<ContainerIssue> {
        let path = std::path::Path::new(file_path);
        let mut issues = Vec::new();

        let output = std::process::Command::new(&self.ffprobe_path)
            .args(["-v", "error", "-show_entries", "packet=stream_index,pts,dts", file_path])
            .output();

        if let Ok(out) = output {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("Invalid data") || stderr.contains("moov atom not found") {
                    issues.push(ContainerIssue {
                        severity: IssueSeverity::Critical,
                        stream_index: None,
                        issue_type: "corrupt_container".to_string(),
                        message: stderr.trim().to_string(),
                    });
                }
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            for line in stderr.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with("[in") { continue; }
                if line.contains("error") || line.contains("Invalid") || line.contains("corrupt") {
                    issues.push(ContainerIssue {
                        severity: IssueSeverity::Warning,
                        stream_index: None,
                        issue_type: "container_error".to_string(),
                        message: line.to_string(),
                    });
                }
            }
        }

        let header_valid = (|| {
            use std::io::Read;
            let mut file = match std::fs::File::open(path) {
                Ok(f) => f,
                Err(_) => return false,
            };
            let mut header = [0u8; 12];
            let bytes_read = match file.read(&mut header) {
                Ok(n) => n,
                Err(_) => return false,
            };
            if bytes_read < 4 { return false; }
            if bytes_read >= 8 && &header[4..8] == b"ftyp" { return true; }
            if bytes_read >= 4 && header[0] == 0x1A && header[1] == 0x45 && header[2] == 0xDF && header[3] == 0xA3 { return true; }
            if bytes_read >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"AVI " { return true; }
            if header[0] == 0x47 { return true; }
            if bytes_read >= 4 && &header[0..4] == b"fLaC" { return true; }
            if bytes_read >= 4 && &header[0..4] == b"OggS" { return true; }
            false
        })();

        if !header_valid {
            issues.push(ContainerIssue {
                severity: IssueSeverity::Critical,
                stream_index: None,
                issue_type: "invalid_header".to_string(),
                message: "Unknown or invalid container header".to_string(),
            });
        }

        issues
    }

    fn check_subtitle_validity(&self, file_path: &str, _streams: &std::collections::HashSet<usize>) -> Vec<SubtitleIssue> {
        let path = std::path::Path::new(file_path);
        let info = match crate::ffmpeg::probe::probe_file(&self.ffprobe_path, path) {
            Ok(i) => i,
            Err(_) => return Vec::new(),
        };

        let mut issues = Vec::new();
        for stream in &info.subtitle_streams {
            let mut stream_issues = Vec::new();
            let codec = stream.codec_name.as_str();
            if codec == "hdmv_pgs_subtitle" || codec == "dvd_subtitle" || codec == "dvb_subtitle" || codec == "xsub" {
                stream_issues.push("bitmap_subtitle".to_string());
            }
            if codec == "mov_text" {
                stream_issues.push("mov_text_subtitle".to_string());
            }
            if !stream_issues.is_empty() {
                issues.push(SubtitleIssue {
                    stream_index: stream.stream_index as usize,
                    stream_type: "subtitle".to_string(),
                    issue_type: stream_issues.join(","),
                    message: format!("Subtitle codec: {}, issues: {}", codec, stream_issues.join(", ")),
                });
            }
        }
        issues
    }

    fn check_video_decode(&self, file_path: &str) -> Vec<TimestampIssue> {
        let mut issues = Vec::new();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let output = cmd
            .args([
                "-v", "error",
                "-i", file_path,
                "-f", "null",
                "-",
            ])
            .output();

        match output {
            Ok(out) => {
                if !out.status.success() {
                    issues.push(TimestampIssue {
                        stream_index: 0,
                        stream_type: "video".to_string(),
                        packet_index: 0,
                        issue_type: "decode_failure".to_string(),
                        pts: None,
                        dts: None,
                    });
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    for line in stderr.lines() {
                        let line = line.trim();
                        if line.is_empty() { continue; }
                        if line.contains("error") || line.contains("Invalid") || line.contains("corrupt") {
                            issues.push(TimestampIssue {
                                stream_index: 0,
                                stream_type: "video".to_string(),
                                packet_index: 0,
                                issue_type: "decode_error".to_string(),
                                pts: None,
                                dts: None,
                            });
                        }
                    }
                }
            }
            Err(_) => {}
        }

        issues
    }

    fn check_bitstream_integrity(&self, file_path: &str) -> Vec<TimestampIssue> {
        let mut issues = Vec::new();

        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = std::process::Command::new(&self.ffmpeg_path);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let output = cmd
            .args([
                "-v", "error",
                "-i", file_path,
                "-c", "copy",
                "-f", "null",
                "-",
            ])
            .output();

        match output {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let error_str = stderr.trim();
                if !error_str.is_empty() {
                    if error_str.contains("Invalid NAL") || error_str.contains("invalid") {
                        issues.push(TimestampIssue {
                            stream_index: 0,
                            stream_type: "video".to_string(),
                            packet_index: 0,
                            issue_type: "bitstream_invalid_nal".to_string(),
                            pts: None,
                            dts: None,
                        });
                    }
                    if error_str.contains("missing reference") || error_str.contains("reference") {
                        issues.push(TimestampIssue {
                            stream_index: 0,
                            stream_type: "video".to_string(),
                            packet_index: 0,
                            issue_type: "bitstream_missing_ref".to_string(),
                            pts: None,
                            dts: None,
                        });
                    }
                    if error_str.contains("corrupt") || error_str.contains("corrupted") {
                        issues.push(TimestampIssue {
                            stream_index: 0,
                            stream_type: "video".to_string(),
                            packet_index: 0,
                            issue_type: "bitstream_corrupt".to_string(),
                            pts: None,
                            dts: None,
                        });
                    }
                    if error_str.contains("error") || error_str.contains("Error") {
                        issues.push(TimestampIssue {
                            stream_index: 0,
                            stream_type: "video".to_string(),
                            packet_index: 0,
                            issue_type: "bitstream_error".to_string(),
                            pts: None,
                            dts: None,
                        });
                    }
                }
            }
            Err(_) => {}
        }

        issues
    }

    fn check_packet_integrity(&self, file_path: &str) -> Vec<TimestampIssue> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-show_packets",
            "-show_entries", "packet=stream_index,pts,dts,codec_type",
            file_path,
        ];

        let output = match std::process::Command::new(&self.ffprobe_path).args(&args).output() {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        if !output.status.success() {
            return Vec::new();
        }

        let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        let packets = match json.get("packets").and_then(|p| p.as_array()) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let mut issues = Vec::new();
        for (i, p) in packets.iter().enumerate() {
            let pts = p.get("pts").and_then(|v| v.as_i64());
            let dts = p.get("dts").and_then(|v| v.as_i64());
            let stream_index = p.get("stream_index").and_then(|v| v.as_i64()).unwrap_or(-1) as usize;
            let codec_type = p.get("codec_type").and_then(|v| v.as_str()).unwrap_or("unknown");

            if pts == Some(i64::MIN) && dts == Some(i64::MIN) {
                issues.push(TimestampIssue {
                    stream_index,
                    stream_type: codec_type.to_string(),
                    packet_index: i,
                    issue_type: "missing_timestamps".to_string(),
                    pts: None,
                    dts: None,
                });
            }

            if let (Some(pts_val), Some(dts_val)) = (pts, dts) {
                if pts_val != i64::MIN && dts_val != i64::MIN && pts_val < dts_val {
                    issues.push(TimestampIssue {
                        stream_index,
                        stream_type: codec_type.to_string(),
                        packet_index: i,
                        issue_type: "pts_less_than_dts".to_string(),
                        pts: Some(pts_val),
                        dts: Some(dts_val),
                    });
                }
            }
        }

        issues
    }

    fn check_video_frame_integrity(&self, file_path: &str) -> Vec<TimestampIssue> {
        let mut issues = Vec::new();

        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-select_streams", "v:0",
            "-show_frames",
            "-show_entries", "frame=pict_type,key_frame",
            file_path,
        ];

        let output = match std::process::Command::new(&self.ffprobe_path).args(&args).output() {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        if !output.status.success() {
            return Vec::new();
        }

        let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        let frames = match json.get("frames").and_then(|f| f.as_array()) {
            Some(f) => f,
            None => return Vec::new(),
        };

        if frames.is_empty() {
            return Vec::new();
        }

        let mut has_keyframe = false;
        let mut i_frame_count = 0;
        let mut p_frame_count = 0;
        let mut b_frame_count = 0;
        let mut unknown_frame_count = 0;

        for f in frames {
            let pict_type = f.get("pict_type").and_then(|v| v.as_str()).unwrap_or("");
            let is_key = f.get("key_frame").and_then(|v| v.as_i64()).unwrap_or(0);

            match pict_type {
                "I" => {
                    i_frame_count += 1;
                    if is_key == 1 {
                        has_keyframe = true;
                    }
                }
                "P" => p_frame_count += 1,
                "B" => b_frame_count += 1,
                _ => unknown_frame_count += 1,
            }
        }

        if !has_keyframe && i_frame_count == 0 {
            issues.push(TimestampIssue {
                stream_index: 0,
                stream_type: "video".to_string(),
                packet_index: 0,
                issue_type: "no_keyframes".to_string(),
                pts: None,
                dts: None,
            });
        }

        if unknown_frame_count > frames.len() / 2 {
            issues.push(TimestampIssue {
                stream_index: 0,
                stream_type: "video".to_string(),
                packet_index: 0,
                issue_type: "invalid_frame_types".to_string(),
                pts: None,
                dts: None,
            });
        }

        if i_frame_count > 0 && p_frame_count == 0 && b_frame_count == 0 {
            if frames.len() > 30 {
                issues.push(TimestampIssue {
                    stream_index: 0,
                    stream_type: "video".to_string(),
                    packet_index: 0,
                    issue_type: "all_intra_frames".to_string(),
                    pts: None,
                    dts: None,
                });
            }
        }

        if i_frame_count == 0 && p_frame_count > 0 {
            issues.push(TimestampIssue {
                stream_index: 0,
                stream_type: "video".to_string(),
                packet_index: 0,
                issue_type: "missing_i_frames".to_string(),
                pts: None,
                dts: None,
            });
        }

        issues
    }

    fn check_attachment_integrity(&self, file_path: &str) -> Vec<TimestampIssue> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-show_entries", "stream=index,codec_type,codec_name,tags",
            file_path,
        ];

        let output = match std::process::Command::new(&self.ffprobe_path).args(&args).output() {
            Ok(o) => o,
            Err(_) => return Vec::new(),
        };

        if !output.status.success() {
            return Vec::new();
        }

        let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
            Ok(j) => j,
            Err(_) => return Vec::new(),
        };

        let mut issues = Vec::new();
        if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
            for (i, stream) in streams.iter().enumerate() {
                let codec_type = stream.get("codec_type").and_then(|v| v.as_str()).unwrap_or("");
                if codec_type == "attachment" {
                    let stream_index = stream.get("index").and_then(|v| v.as_i64()).unwrap_or(i as i64) as usize;
                    issues.push(TimestampIssue {
                        stream_index,
                        stream_type: "attachment".to_string(),
                        packet_index: 0,
                        issue_type: "attachment_stream_present".to_string(),
                        pts: None,
                        dts: None,
                    });
                }
            }
        }

        issues
    }

    fn classify_damage_extended(
        &self,
        has_container_critical: bool,
        has_pts_issues: bool,
        has_dts_issues: bool,
        has_subtitle_issues: bool,
        has_timebase_issues: bool,
        has_video_decode_issues: bool,
        has_bitstream_issues: bool,
        has_packet_issues: bool,
        has_frame_issues: bool,
        _has_attachment_issues: bool,
    ) -> DamageClassification {
        if has_container_critical {
            return DamageClassification::ContainerDamage;
        }
        if has_subtitle_issues {
            return DamageClassification::SubtitleDamage;
        }
        if has_pts_issues || has_dts_issues || has_timebase_issues {
            return DamageClassification::TimestampDamage;
        }
        if has_video_decode_issues {
            return DamageClassification::VideoDecodeFailure;
        }
        if has_bitstream_issues {
            return DamageClassification::BitstreamCorruption;
        }
        if has_packet_issues {
            return DamageClassification::PacketCorruption;
        }
        if has_frame_issues {
            return DamageClassification::VideoFrameCorruption;
        }
        DamageClassification::Healthy
    }
}