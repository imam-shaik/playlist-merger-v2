use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryTransition {
    pub file_a_index: usize,
    pub file_a_path: String,
    pub file_b_index: usize,
    pub file_b_path: String,
    pub stream_type: String,
    pub stream_index: usize,
    pub last_pts_a: i64,
    pub last_dts_a: Option<i64>,
    pub first_pts_b: i64,
    pub first_dts_b: Option<i64>,
    pub gap: i64,
    pub is_monotonic: bool,
    pub is_compatible: bool,
    pub incompatibility_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryReport {
    pub boundary_index: usize,
    pub transitions: Vec<BoundaryTransition>,
    pub all_compatible: bool,
    pub problematic_boundaries: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcatListIntegrity {
    pub concat_list_entries: Vec<ConcatListEntry>,
    pub expected_files: Vec<String>,
    pub order_matches: bool,
    pub missing_files: Vec<String>,
    pub extra_files: Vec<String>,
    pub duplicate_entries: Vec<String>,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcatListEntry {
    pub index: usize,
    pub path: String,
    pub line_number: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostConcatDemuxerProbe {
    pub file_path: String,
    pub total_packets: usize,
    pub packets_with_nopts: usize,
    pub first_invalid_packet_index: Option<usize>,
    pub stream_with_invalid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryCertification {
    pub file_boundaries: Vec<BoundaryReport>,
    pub concat_list_integrity: Option<ConcatListIntegrity>,
    pub post_concat_probe: Option<PostConcatDemuxerProbe>,
    pub overall_passed: bool,
    pub confidence_score: f32,
    pub findings: Vec<String>,
}

pub fn certify_boundaries(
    ffprobe_path: &Path,
    files: &[String],
) -> Result<BoundaryCertification, String> {
    let mut findings = Vec::new();
    let mut all_boundaries = Vec::new();
    let mut all_compatible = true;
    let mut problematic_boundaries = Vec::new();

    for i in 0..files.len() - 1 {
        let file_a = &files[i];
        let file_b = &files[i + 1];

        let boundary = analyze_boundary(ffprobe_path, i, file_a, i + 1, file_b)?;

        if !boundary.all_compatible {
            all_compatible = false;
            problematic_boundaries.push(i);
            findings.push(format!(
                "BOUNDARY {} (File {} → File {}): INCOMPATIBLE — {}",
                i,
                i,
                i + 1,
                boundary.transitions.iter()
                    .filter(|t| !t.is_compatible)
                    .map(|t| t.incompatibility_reason.clone().unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        } else {
            findings.push(format!(
                "BOUNDARY {} (File {} → File {}): Compatible",
                i,
                i,
                i + 1
            ));
        }

        all_boundaries.push(boundary);
    }

    let confidence = if all_compatible {
        findings.push("All boundaries compatible — concat transition issues unlikely".to_string());
        0.95
    } else {
        findings.push(format!(
            "{} boundary incompatibilities detected — likely source of concat failure",
            problematic_boundaries.len()
        ));
        0.85
    };

    Ok(BoundaryCertification {
        file_boundaries: all_boundaries,
        concat_list_integrity: None,
        post_concat_probe: None,
        overall_passed: all_compatible,
        confidence_score: confidence,
        findings,
    })
}

fn analyze_boundary(
    ffprobe_path: &Path,
    idx_a: usize,
    path_a: &str,
    idx_b: usize,
    path_b: &str,
) -> Result<BoundaryReport, String> {
    let file_a_packets = get_packet_timestamps(ffprobe_path, path_a)?;
    let file_b_packets = get_packet_timestamps(ffprobe_path, path_b)?;

    let mut transitions = Vec::new();
    let mut compatible = true;

    let stream_types = ["video", "audio", "subtitle"];

    for stream_type in &stream_types {
        let last_a = get_last_packet_of_type(&file_a_packets, stream_type);
        let first_b = get_first_packet_of_type(&file_b_packets, stream_type);

        if let (Some(last), Some(first)) = (last_a, first_b) {
            let gap = first.pts - last.pts;
            let is_monotonic = first.pts >= last.pts;
            let is_compatible = is_monotonic && gap < 3_600_000_000i64;
            let mut reason = None;

            if first.pts < last.pts {
                reason = Some(format!(
                    "REGRESSION: {} PTS goes backwards at boundary (last={}, first={}, gap={})",
                    stream_type, last.pts, first.pts, gap
                ));
                compatible = false;
            } else if gap > 3_600_000_000i64 {
                reason = Some(format!(
                    "GAP: {} has {}ms gap at boundary (last={}, first={})",
                    stream_type, gap / 90_000, last.pts, first.pts
                ));
            }

            transitions.push(BoundaryTransition {
                file_a_index: idx_a,
                file_a_path: path_a.to_string(),
                file_b_index: idx_b,
                file_b_path: path_b.to_string(),
                stream_type: stream_type.to_string(),
                stream_index: 0,
                last_pts_a: last.pts,
                last_dts_a: last.dts,
                first_pts_b: first.pts,
                first_dts_b: first.dts,
                gap,
                is_monotonic,
                is_compatible,
                incompatibility_reason: reason,
            });
        }
    }

    Ok(BoundaryReport {
        boundary_index: idx_a,
        transitions,
        all_compatible: compatible,
        problematic_boundaries: if compatible { vec![] } else { vec![idx_a] },
    })
}

#[derive(Debug, Clone)]
struct PacketTimestamp {
    pts: i64,
    dts: Option<i64>,
    #[allow(dead_code)]
    stream_index: i64,
    codec_type: String,
}

fn get_packet_timestamps(ffprobe_path: &Path, file_path: &str) -> Result<Vec<PacketTimestamp>, String> {
    let args = [
        "-v", "quiet",
        "-print_format", "json",
        "-show_packets",
        "-show_entries", "packet=stream_index,pts,dts,codec_type",
        file_path,
    ];

    let output = Command::new(ffprobe_path)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    let packets = json.get("packets")
        .and_then(|p| p.as_array())
        .ok_or_else(|| "No packets in ffprobe output".to_string())?;

    let mut result = Vec::new();
    for p in packets {
        let pts = p.get("pts").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
        let dts = p.get("dts").and_then(|v| v.as_i64());
        let stream_index = p.get("stream_index").and_then(|v| v.as_i64()).unwrap_or(0);
        let codec_type = p.get("codec_type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();

        result.push(PacketTimestamp {
            pts,
            dts,
            stream_index,
            codec_type,
        });
    }

    Ok(result)
}

fn get_last_packet_of_type(packets: &[PacketTimestamp], codec_type: &str) -> Option<PacketTimestamp> {
    packets.iter()
        .filter(|p| p.codec_type == codec_type && p.pts != i64::MIN)
        .last()
        .cloned()
}

fn get_first_packet_of_type(packets: &[PacketTimestamp], codec_type: &str) -> Option<PacketTimestamp> {
    packets.iter()
        .filter(|p| p.codec_type == codec_type && p.pts != i64::MIN)
        .next()
        .cloned()
}

pub fn check_concat_list_integrity(
    concat_list_content: &str,
    expected_files: &[String],
) -> ConcatListIntegrity {
    let lines: Vec<&str> = concat_list_content.lines().collect();
    let mut entries = Vec::new();
    let mut actual_paths = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("file '") || trimmed.starts_with("file \"") {
            let path = if trimmed.starts_with("file '") {
                trimmed.trim_start_matches("file '").trim_end_matches('\'')
            } else {
                trimmed.trim_start_matches("file \"").trim_end_matches('"')
            };
            entries.push(ConcatListEntry {
                index: actual_paths.len(),
                path: path.to_string(),
                line_number: i + 1,
            });
            actual_paths.push(path.to_string());
        }
    }

    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let mut duplicates = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for expected in expected_files {
        if !actual_paths.contains(expected) {
            missing.push(expected.clone());
        }
        *seen.entry(expected.clone()).or_insert(0) += 1;
    }

    for (path, count) in &seen {
        if *count > 1 {
            duplicates.push(path.clone());
        }
    }

    for actual in &actual_paths {
        if !expected_files.contains(actual) {
            extra.push(actual.clone());
        }
    }

    let order_matches = actual_paths == expected_files;

    let is_valid = missing.is_empty() && extra.is_empty() && duplicates.is_empty() && order_matches;

    ConcatListIntegrity {
        concat_list_entries: entries,
        expected_files: expected_files.to_vec(),
        order_matches,
        missing_files: missing,
        extra_files: extra,
        duplicate_entries: duplicates,
        is_valid,
    }
}

pub fn probe_post_concat_demuxer(
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
    concat_list_path: &str,
) -> Result<PostConcatDemuxerProbe, String> {
    let temp_output = std::env::temp_dir().join(format!(
        "boundary_cert_probe_{}.mkv",
        std::process::id()
    ));

    let args = [
        "-y",
        "-f", "concat",
        "-safe", "0",
        "-i", concat_list_path,
        "-t", "5",
        "-c", "copy",
        temp_output.to_str().unwrap(),
    ];

    let output = Command::new(ffmpeg_path)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run ffmpeg: {}", e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut nopts_count = 0;

    let full_output = format!("{}\n{}", stderr, stdout);

    if full_output.contains("NOPTS") {
        nopts_count = full_output.matches("NOPTS").count();
    }

    let probe_result = if temp_output.exists() {
        let probe_result = probe_mkv_packets(ffprobe_path, &temp_output)?;
        let _ = std::fs::remove_file(&temp_output);
        probe_result
    } else {
        PostConcatDemuxerProbe {
            file_path: concat_list_path.to_string(),
            total_packets: 0,
            packets_with_nopts: 0,
            first_invalid_packet_index: None,
            stream_with_invalid: None,
        }
    };

    let total_nopts = nopts_count + probe_result.packets_with_nopts;

    Ok(PostConcatDemuxerProbe {
        file_path: concat_list_path.to_string(),
        total_packets: probe_result.total_packets,
        packets_with_nopts: total_nopts,
        first_invalid_packet_index: if total_nopts > 0 { Some(0) } else { None },
        stream_with_invalid: if total_nopts > 0 {
            Some(format!("{} NOPTS issues detected ({} from probe, {} from mkv probe)",
                total_nopts, nopts_count, probe_result.packets_with_nopts))
        } else { None },
    })
}

fn probe_mkv_packets(ffprobe_path: &Path, mkv_path: &std::path::Path) -> Result<PostConcatDemuxerProbe, String> {
    let args = [
        "-v", "quiet",
        "-print_format", "json",
        "-show_packets",
        "-show_entries", "packet=stream_index,pts,dts,codec_type",
        mkv_path.to_str().unwrap(),
    ];

    let output = Command::new(ffprobe_path)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Ok(PostConcatDemuxerProbe {
            file_path: mkv_path.to_string_lossy().into_owned(),
            total_packets: 0,
            packets_with_nopts: 0,
            first_invalid_packet_index: None,
            stream_with_invalid: None,
        });
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    let empty_vec: Vec<serde_json::Value> = vec![];
    let packets = json.get("packets")
        .and_then(|p| p.as_array())
        .unwrap_or(&empty_vec);

    let total = packets.len();
    let with_nopts = packets.iter().filter(|p| {
        p.get("pts").and_then(|v| v.as_i64()).map(|v| v == i64::MIN).unwrap_or(false)
    }).count();

    Ok(PostConcatDemuxerProbe {
        file_path: mkv_path.to_string_lossy().into_owned(),
        total_packets: total,
        packets_with_nopts: with_nopts,
        first_invalid_packet_index: if with_nopts > 0 { Some(0) } else { None },
        stream_with_invalid: if with_nopts > 0 {
            Some(format!("{} packets with PTS = i64::MIN (NOPTS)", with_nopts))
        } else { None },
    })
}

impl std::fmt::Display for BoundaryCertification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║            BOUNDARY TRANSITION CERTIFICATION                              ║")?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════════════╝")?;
        writeln!(f)?;

        writeln!(f, "OVERALL: {}", if self.overall_passed { "✅ PASSED" } else { "❌ FAILED" })?;
        writeln!(f, "Confidence: {:.0}%", self.confidence_score * 100.0)?;
        writeln!(f)?;

        writeln!(f, "FINDINGS:")?;
        for finding in &self.findings {
            writeln!(f, "  • {}", finding)?;
        }
        writeln!(f)?;

        writeln!(f, "BOUNDARY DETAILS:")?;
        for boundary in &self.file_boundaries {
            let (file_a_idx, file_b_idx) = boundary.transitions.first()
                .map(|t| (t.file_a_index, t.file_b_index))
                .unwrap_or((boundary.boundary_index, boundary.boundary_index + 1));
            writeln!(f, "  ── Boundary {} (File {} → File {}) ──",
                boundary.boundary_index,
                file_a_idx,
                file_b_idx
            )?;
            for t in &boundary.transitions {
                let status = if t.is_compatible { "✅" } else { "❌" };
                writeln!(f, "    {} {} stream: last_pts={}, first_pts={}, gap={} ({:.1}s), monotonic={}",
                    status,
                    t.stream_type,
                    t.last_pts_a,
                    t.first_pts_b,
                    t.gap,
                    t.gap as f64 / 90_000.0,
                    t.is_monotonic
                )?;
                if let Some(ref reason) = t.incompatibility_reason {
                    writeln!(f, "       ⚠ {}", reason)?;
                }
            }
        }

        Ok(())
    }
}

impl std::fmt::Display for ConcatListIntegrity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║              CONCAT LIST INTEGRITY CHECK                                  ║")?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════════════╝")?;
        writeln!(f)?;
        writeln!(f, "Valid: {}", if self.is_valid { "✅ YES" } else { "❌ NO" })?;
        writeln!(f, "Order matches expected: {}", if self.order_matches { "✅ YES" } else { "❌ NO" })?;
        writeln!(f)?;
        if !self.missing_files.is_empty() {
            writeln!(f, "MISSING files:")?;
            for m in &self.missing_files {
                writeln!(f, "  • {}", m)?;
            }
            writeln!(f)?;
        }
        if !self.extra_files.is_empty() {
            writeln!(f, "EXTRA files (not in expected):")?;
            for e in &self.extra_files {
                writeln!(f, "  • {}", e)?;
            }
            writeln!(f)?;
        }
        if !self.duplicate_entries.is_empty() {
            writeln!(f, "DUPLICATE entries:")?;
            for d in &self.duplicate_entries {
                writeln!(f, "  • {}", d)?;
            }
            writeln!(f)?;
        }
        if self.is_valid {
            writeln!(f, "Concat list is VALID — {} entries, order correct, no duplicates",
                self.concat_list_entries.len())?;
        }
        Ok(())
    }
}