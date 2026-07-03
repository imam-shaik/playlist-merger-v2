use std::path::{Path, PathBuf};
use std::process::Command;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum PtsCheckSeverity {
    Critical,
    Error,
    Warning,
    Info,
}

pub struct PacketTimestampReport {
    pub passed: bool,
    pub total_files: usize,
    pub files_passed: usize,
    pub files_failed: usize,
    pub layer_reports: Vec<PacketTimestampLayerReport>,
    pub total_duration_ms: u128,
}

pub struct PacketTimestampLayerReport {
    pub file_path: String,
    pub file_index: usize,
    pub passed: bool,
    pub checks: Vec<PacketTimestampCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PacketTimestampCheck {
    pub stream_index: usize,
    pub stream_type: String,
    pub check_name: String,
    pub passed: bool,
    pub severity: PtsCheckSeverity,
    pub details: String,
    pub first_occurrence_pts: Option<i64>,
    pub first_occurrence_packet: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct PacketTimestampIssue {
    pub check_name: String,
    pub stream_index: usize,
    pub stream_type: String,
    pub pts: i64,
    pub dts: Option<i64>,
    pub packet_index: usize,
    pub severity: PtsCheckSeverity,
    pub details: String,
}

pub struct PacketTimestampAnalyzer {
    ffprobe_path: PathBuf,
}

impl PacketTimestampAnalyzer {
    pub fn new(ffprobe_path: &Path) -> Self {
        PacketTimestampAnalyzer {
            ffprobe_path: ffprobe_path.to_path_buf(),
        }
    }

    pub fn analyze_file(&self, file_path: &Path, file_index: usize) -> Result<PacketTimestampLayerReport, String> {
        let packets = self.get_packets(file_path)?;
        let streams = self.get_stream_info(file_path)?;
        
        let mut checks = Vec::new();
        let mut passed = true;

        let video_stream_indices: Vec<usize> = streams.iter()
            .filter(|(s, _)| *s == "video")
            .map(|(_, idx)| *idx)
            .collect();
        let audio_stream_indices: Vec<usize> = streams.iter()
            .filter(|(s, _)| *s == "audio")
            .map(|(_, idx)| *idx)
            .collect();
        let subtitle_stream_indices: Vec<usize> = streams.iter()
            .filter(|(s, _)| *s == "subtitle")
            .map(|(_, idx)| *idx)
            .collect();

        for &stream_idx in &video_stream_indices {
            let stream_packets: Vec<(usize, i64, Option<i64>)> = packets.iter()
                .enumerate()
                .filter(|(_, (_, _, _, si, _))| *si == stream_idx as i64)
                .map(|(pidx, (_, pts, dts, _, _))| (pidx, *pts, *dts))
                .collect();
            
            if let Some(report) = self.check_missing_pts("Video", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_missing_dts("Video", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_pts_monotonic("Video", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_dts_monotonic("Video", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_pts_geq_dts("Video", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
        }

        for &stream_idx in &audio_stream_indices {
            let stream_packets: Vec<(usize, i64, Option<i64>)> = packets.iter()
                .enumerate()
                .filter(|(_, (_, _, _, si, _))| *si == stream_idx as i64)
                .map(|(pidx, (_, pts, dts, _, _))| (pidx, *pts, *dts))
                .collect();
            
            if let Some(report) = self.check_missing_pts("Audio", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_missing_dts("Audio", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_pts_monotonic("Audio", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_dts_monotonic("Audio", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_pts_geq_dts("Audio", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
        }

        // Subtitle stream validation — CRITICAL for MKV merges
        // Subtitle streams frequently have missing/zero timestamps that cause
        // "Can't write packet with unknown timestamp" errors during trailer writing.
        for &stream_idx in &subtitle_stream_indices {
            let stream_packets: Vec<(usize, i64, Option<i64>)> = packets.iter()
                .enumerate()
                .filter(|(_, (_, _, _, si, _))| *si == stream_idx as i64)
                .map(|(pidx, (_, pts, dts, _, _))| (pidx, *pts, *dts))
                .collect();

            if let Some(report) = self.check_missing_pts("Subtitle", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_missing_dts("Subtitle", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_pts_monotonic("Subtitle", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_dts_monotonic("Subtitle", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
            if let Some(report) = self.check_pts_geq_dts("Subtitle", stream_idx, &stream_packets) {
                if !report.passed { passed = false; }
                checks.push(report);
            }
        }

        Ok(PacketTimestampLayerReport {
            file_path: file_path.to_string_lossy().into_owned(),
            file_index,
            passed,
            checks,
        })
    }

    fn get_packets(&self, file_path: &Path) -> Result<Vec<(usize, i64, Option<i64>, i64, String)>, String> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-show_packets",
            "-show_entries", "packet=stream_index,pts,dts,codec_type,flags",
            file_path.to_str().unwrap(),
        ];
        
        let output = Command::new(&self.ffprobe_path)
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to run ffprobe: {}", e))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("ffprobe failed: {}", stderr));
        }
        
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Invalid JSON from ffprobe: {}", e))?;
        
        let packets = json.get("packets")
            .and_then(|p| p.as_array())
            .ok_or_else(|| "No packets in ffprobe output".to_string())?;
        
        let mut result = Vec::new();
        for (i, p) in packets.iter().enumerate() {
            let pts = p.get("pts").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
            let dts = p.get("dts").and_then(|v| v.as_i64());
            let stream_index = p.get("stream_index").and_then(|v| v.as_i64()).unwrap_or(0);
            let codec_type = p.get("codec_type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            
            result.push((i, pts, dts, stream_index, codec_type));
        }
        
        Ok(result)
    }
    
    fn get_stream_info(&self, file_path: &Path) -> Result<Vec<(String, usize)>, String> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
            "-show_entries", "stream=codec_type,index",
            file_path.to_str().unwrap(),
        ];
        
        let output = Command::new(&self.ffprobe_path)
            .args(&args)
            .output()
            .map_err(|e| format!("Failed to run ffprobe: {}", e))?;
        
        if !output.status.success() {
            return Err("ffprobe failed for stream info".to_string());
        }
        
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Invalid JSON: {}", e))?;
        
        let streams = json.get("streams")
            .and_then(|s| s.as_array())
            .ok_or_else(|| "No streams found".to_string())?;
        
        let mut result = Vec::new();
        for s in streams {
            let codec_type = s.get("codec_type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            let index = s.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            result.push((codec_type, index));
        }
        
        Ok(result)
    }

    fn check_missing_pts(&self, stream_type: &str, stream_index: usize, packets: &[(usize, i64, Option<i64>)]) -> Option<PacketTimestampCheck> {
        let missing: Vec<_> = packets.iter().filter(|(_, pts, _)| *pts == i64::MIN).collect();
        
        Some(PacketTimestampCheck {
            stream_index,
            stream_type: stream_type.to_string(),
            check_name: "No Missing PTS".to_string(),
            passed: missing.is_empty(),
            severity: if missing.is_empty() { PtsCheckSeverity::Info } else { PtsCheckSeverity::Critical },
            details: if missing.is_empty() {
                "All packets have valid PTS".to_string()
            } else {
                format!("{} packets with missing PTS (AV_NOPTS_VALUE)", missing.len())
            },
            first_occurrence_pts: None,
            first_occurrence_packet: missing.first().map(|(pidx, _, _)| *pidx),
        })
    }

    fn check_missing_dts(&self, stream_type: &str, stream_index: usize, packets: &[(usize, i64, Option<i64>)]) -> Option<PacketTimestampCheck> {
        let missing: Vec<_> = packets.iter().filter(|(_, _, dts)| dts.is_none()).collect();
        
        Some(PacketTimestampCheck {
            stream_index,
            stream_type: stream_type.to_string(),
            check_name: "No Missing DTS".to_string(),
            passed: missing.is_empty(),
            severity: if missing.is_empty() { PtsCheckSeverity::Info } else { PtsCheckSeverity::Critical },
            details: if missing.is_empty() {
                "All packets have valid DTS".to_string()
            } else {
                format!("{} packets with missing DTS", missing.len())
            },
            first_occurrence_pts: None,
            first_occurrence_packet: missing.first().map(|(pidx, _, _)| *pidx),
        })
    }

    fn check_pts_monotonic(&self, stream_type: &str, stream_index: usize, packets: &[(usize, i64, Option<i64>)]) -> Option<PacketTimestampCheck> {
        if packets.len() < 2 {
            return None;
        }

        // Sort by PTS to check monotonicity in PRESENTATION order, not decode order.
        // Video streams with B-frames have PTS that go backwards in decode order,
        // which is NORMAL. Sorting first ensures we check PTS in presentation order.
        // This matches the algorithm used by MediaValidationEngine::check_pts_monotonic
        let mut sorted_packets = packets.to_vec();
        sorted_packets.sort_by_key(|(_, pts, _)| *pts);

        let mut non_monotonic = Vec::new();
        let mut prev_pts = i64::MIN;

        for (_, pts, _) in sorted_packets {
            if pts != i64::MIN && prev_pts != i64::MIN && pts < prev_pts {
                non_monotonic.push((pts, prev_pts));
            }
            if pts != i64::MIN {
                prev_pts = pts;
            }
        }

        let first_occurrence = non_monotonic.first().map(|(curr, _)| *curr);
        
        Some(PacketTimestampCheck {
            stream_index,
            stream_type: stream_type.to_string(),
            check_name: "PTS Monotonic".to_string(),
            passed: non_monotonic.is_empty(),
            severity: if non_monotonic.is_empty() { 
                PtsCheckSeverity::Info 
            } else if non_monotonic.len() > 10 { 
                PtsCheckSeverity::Critical 
            } else { 
                PtsCheckSeverity::Error 
            },
            details: if non_monotonic.is_empty() {
                "PTS is monotonically increasing".to_string()
            } else {
                format!("{} non-monotonic PTS jumps detected", non_monotonic.len())
            },
            first_occurrence_pts: first_occurrence,
            first_occurrence_packet: None,
        })
    }

    fn check_dts_monotonic(&self, stream_type: &str, stream_index: usize, packets: &[(usize, i64, Option<i64>)]) -> Option<PacketTimestampCheck> {
        if packets.len() < 2 {
            return None;
        }
        
        let mut non_monotonic = Vec::new();
        let mut prev_dts: Option<i64> = None;
        
        for (_, _, dts) in packets {
            if let Some(dts_val) = dts {
                if let Some(prev) = prev_dts {
                    if *dts_val < prev {
                        non_monotonic.push((*dts_val, prev));
                    }
                }
                prev_dts = Some(*dts_val);
            }
        }
        
        let first_occurrence = non_monotonic.first().map(|(curr, _)| *curr);
        
        Some(PacketTimestampCheck {
            stream_index,
            stream_type: stream_type.to_string(),
            check_name: "DTS Monotonic".to_string(),
            passed: non_monotonic.is_empty(),
            severity: if non_monotonic.is_empty() { 
                PtsCheckSeverity::Info 
            } else if non_monotonic.len() > 10 { 
                PtsCheckSeverity::Critical 
            } else { 
                PtsCheckSeverity::Error 
            },
            details: if non_monotonic.is_empty() {
                "DTS is monotonically increasing".to_string()
            } else {
                format!("{} non-monotonic DTS jumps detected", non_monotonic.len())
            },
            first_occurrence_pts: first_occurrence,
            first_occurrence_packet: None,
        })
    }

    fn check_pts_geq_dts(&self, stream_type: &str, stream_index: usize, packets: &[(usize, i64, Option<i64>)]) -> Option<PacketTimestampCheck> {
        let invalid: Vec<_> = packets.iter()
            .filter(|(_, pts, dts)| {
                if let Some(dts_val) = dts {
                    *pts != i64::MIN && *pts < *dts_val
                } else {
                    false
                }
            })
            .collect();
        
        let first_occurrence = invalid.first().map(|(_, pts, _)| *pts);
        
        Some(PacketTimestampCheck {
            stream_index,
            stream_type: stream_type.to_string(),
            check_name: "PTS >= DTS".to_string(),
            passed: invalid.is_empty(),
            severity: if invalid.is_empty() { PtsCheckSeverity::Info } else { PtsCheckSeverity::Critical },
            details: if invalid.is_empty() {
                "All PTS >= DTS".to_string()
            } else {
                format!("{} packets with PTS < DTS (invalid ordering)", invalid.len())
            },
            first_occurrence_pts: first_occurrence,
            first_occurrence_packet: None,
        })
    }
}

pub fn certify_packet_timestamps(
    ffprobe_path: &Path,
    files: &[(&Path, usize)],
) -> PacketTimestampReport {
    use std::time::Instant;
    let start = Instant::now();
    
    let analyzer = PacketTimestampAnalyzer::new(ffprobe_path);
    let mut layer_reports = Vec::new();
    let mut files_passed = 0;
    let mut files_failed = 0;
    
    log::info!("[CERT:PACKET_TS] Starting packet timestamp certification for {} files", files.len());
    
    for (file_path, file_index) in files {
        match analyzer.analyze_file(file_path, *file_index) {
            Ok(report) => {
                if report.passed {
                    files_passed += 1;
                    log::info!("[CERT:PACKET_TS] File #{} ({}): PASS", file_index, file_path.display());
                } else {
                    files_failed += 1;
                    log::warn!("[CERT:PACKET_TS] File #{} ({}): FAIL", file_index, file_path.display());
                    for check in &report.checks {
                        if !check.passed {
                            log::warn!("[CERT:PACKET_TS]   - {} stream {}: {} ({})", 
                                check.stream_type, check.stream_index, check.check_name, check.details);
                        }
                    }
                }
                layer_reports.push(report);
            }
            Err(e) => {
                files_failed += 1;
                log::error!("[CERT:PACKET_TS] File #{} ({}): ERROR - {}", file_index, file_path.display(), e);
                layer_reports.push(PacketTimestampLayerReport {
                    file_path: file_path.to_string_lossy().into_owned(),
                    file_index: *file_index,
                    passed: false,
                    checks: vec![PacketTimestampCheck {
                        stream_index: 0,
                        stream_type: "unknown".to_string(),
                        check_name: "Probe Success".to_string(),
                        passed: false,
                        severity: PtsCheckSeverity::Critical,
                        details: format!("Failed to probe: {}", e),
                        first_occurrence_pts: None,
                        first_occurrence_packet: None,
                    }],
                });
            }
        }
    }
    
    let passed = files_failed == 0;
    
    log::info!("[CERT:PACKET_TS] ═══════════════════════════════════════════════════════════");
    log::info!("[CERT:PACKET_TS] PACKET TIMESTAMP CERTIFICATION COMPLETE");
    log::info!("[CERT:PACKET_TS] Total files: {} | Passed: {} | Failed: {}", 
        files.len(), files_passed, files_failed);
    if passed {
        log::info!("[CERT:PACKET_TS] ✅ ALL FILES PASSED TIMESTAMP CERTIFICATION");
    } else {
        log::info!("[CERT:PACKET_TS] ❌ TIMESTAMP CERTIFICATION FAILED");
    }
    log::info!("[CERT:PACKET_TS] Duration: {}ms", start.elapsed().as_millis());
    log::info!("[CERT:PACKET_TS] ═══════════════════════════════════════════════════════════");
    
    PacketTimestampReport {
        passed,
        total_files: files.len(),
        files_passed,
        files_failed,
        layer_reports,
        total_duration_ms: start.elapsed().as_millis(),
    }
}

impl std::fmt::Display for PacketTimestampReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║           PACKET TIMESTAMP CERTIFICATION REPORT                    ║")?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  Files: {:3} total | {:3} passed | {:3} failed                        ║", 
            self.total_files, self.files_passed, self.files_failed)?;
        writeln!(f, "║  Duration: {:50}ms ║", self.total_duration_ms)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        
        for layer in &self.layer_reports {
            if !layer.passed {
                writeln!(f, "║  FILE #{}: {} ", layer.file_index, if layer.passed { "✅ PASS" } else { "❌ FAIL" })?;
                for check in &layer.checks {
                    if !check.passed {
                        let sev = match check.severity {
                            PtsCheckSeverity::Critical => "CRITICAL",
                            PtsCheckSeverity::Error => "ERROR",
                            PtsCheckSeverity::Warning => "WARNING",
                            PtsCheckSeverity::Info => "INFO",
                        };
                        writeln!(f, "║    [{}] {} stream {}: {} — {}           ║",
                            sev, check.stream_type, check.stream_index, check.check_name, check.details)?;
                    }
                }
            }
        }
        
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        let overall = if self.passed { "✅ PRODUCTION CERTIFIED" } else { "❌ CERTIFICATION FAILED" };
        writeln!(f, "║  OVERALL: {:57} ║", overall)?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════╝")?;
        Ok(())
    }
}