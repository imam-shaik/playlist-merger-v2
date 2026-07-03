#!/usr/bin/env python3
"""Step 2: Insert analyze_single method into media_validation_engine.rs."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# Find the insertion point: just before validate_single_impl
marker = '    fn validate_single_impl(&self, file_index: usize, file_path: &str) -> MediaValidationResult {'
if marker not in content:
    print("ERROR: Could not find validate_single_impl")
    exit(1)

idx = content.index(marker)

analyze_single = """    /// Phase 1: Analyze a single file (READ-ONLY, side-effect free).
    /// Runs all analysis checks without any repair attempts.
    /// Returns a FileState with disposition and classification populated.
    fn analyze_single(&self, file_index: usize, file_path: &str) -> FileState {
        let file_start = std::time::Instant::now();
        let file_name = std::path::Path::new(file_path)
            .file_name().map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string());
        let original_duration = self.get_duration_secs(file_path);

        // -- Phase 1: Quick structural check (ffprobe header) --
        let p1_start = std::time::Instant::now();
        let quick_result = self.quick_container_check(file_path);
        let phase1_ms = p1_start.elapsed().as_millis() as f64;

        // Collect container issues from quick check
        let mut has_container_critical = false;
        let mut container_issues_from_quick: Vec<ContainerIssue> = Vec::new();
        if let Err(ref issues) = quick_result {
            container_issues_from_quick = issues.clone();
            has_container_critical = issues.iter().any(|i| i.severity == crate::ffmpeg::media_validation_engine::IssueSeverity::Critical);
        }

        // -- Phase 2: Deep packet-level analysis --
        let p2_start = std::time::Instant::now();

        // Identify stream types
        let (video_audio_streams, subtitle_streams) = self.get_stream_codec_types(file_path);

        // Timing & structural checks
        let pts_issues = self.check_pts_monotonic(file_path, &video_audio_streams);
        let dts_issues = self.check_dts_validity(file_path, &video_audio_streams);
        let timebase_issues = self.check_timebase_consistency(file_path);
        let vfr_instability = self.check_vfr_instability(file_path, &video_audio_streams);
        let deep_container_issues = self.check_container_corruption(file_path);
        let subtitle_issues = self.check_subtitle_validity(file_path, &subtitle_streams);

        let phase2_ms = p2_start.elapsed().as_millis() as f64;

        // -- Phase 9: Additional integrity checks --
        let p9_start = std::time::Instant::now();

        let video_decode_issues = self.check_video_decode(file_path);
        let bitstream_issues = self.check_bitstream_integrity(file_path);
        let packet_issues = self.check_packet_integrity(file_path);
        let frame_issues = self.check_video_frame_integrity(file_path);
        let attachment_issues = self.check_attachment_integrity(file_path);

        let phase9_ms = p9_start.elapsed().as_millis() as f64;

        // -- Classification --
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

        // -- Determine disposition --
        let disposition = match &damage {
            DamageClassification::Healthy => {
                if vfr_instability.is_empty() {
                    FileDisposition::Healthy
                } else {
                    // VFR is non-critical for MKV
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

        // -- Build validation_reason --
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

        // -- Per-file timing log --
        if total_ms > 1000.0 {
            log::warn!("[STAGE_TIMING] SLOW_ANALYSIS file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms classify={:.0}ms disposition={:?}",
                file_name, file_index, total_ms, phase1_ms, phase2_ms, phase9_ms, classify_ms, disposition);
        } else {
            log::trace!("[STAGE_TIMING] ANALYSIS file={} index={} total={:.0}ms | p1={:.0}ms p2={:.0}ms p9={:.0}ms classify={:.0}ms",
                file_name, file_index, total_ms, phase1_ms, phase2_ms, phase9_ms, classify_ms);
        }

        // -- Construct FileState --
        FileState {
            file_index,
            original_path: file_path.to_string(),
            original_name: file_name,
            original_duration_secs: original_duration,
            disposition,
            damage_classification: damage,
            confidence: 1.0,
            analysis_reason,
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
            // Phase 2 & 3 defaults (not yet processed)
            repair_status: RepairStatus::Skipped,
            fix_applied: None,
            repaired_path: None,
            repair_trace: Vec::new(),
            repair_duration_ms: 0.0,
            revalidation_status: RevalidationStatus::NotNeeded,
            revalidation_duration_ms: 0.0,
            // Default final_path to original (will be updated in Phase 2/3)
            final_path: file_path.to_string(),
        }
    }

    /// Helper: Get duration in seconds for a file via ffprobe.
    fn get_duration_secs(&self, file_path: &str) -> f64 {
        let output = Command::new(&self.ffprobe_path)
            .args(["-v", "quiet", "-print_format", "json", "-show_format", file_path])
            .output();
        match output {
            Ok(out) => {
                if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                    json["format"]["duration"]
                        .as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0)
                } else {
                    0.0
                }
            }
            Err(_) => 0.0,
        }
    }

"""

content = content[:idx] + analyze_single + content[idx:]

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print("Inserted analyze_single method")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    verify = f.read()

checks = [
    'fn analyze_single',
    'fn get_duration_secs',
    'FileDisposition::Healthy',
    'FileDisposition::Repairable',
    'FileDisposition::Unrepairable',
    'quick_container_check',
    'check_pts_monotonic',
    'classify_damage_extended',
    'FileState {',
    'repair_status: RepairStatus::Skipped',
    'final_path: file_path.to_string()',
]
for c in checks:
    print(f"  [{'PASS' if c in verify else 'FAIL'}] {c}")
