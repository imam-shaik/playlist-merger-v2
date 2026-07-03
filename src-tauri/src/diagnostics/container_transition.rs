use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerTransitionAudit {
    pub file_path: String,
    pub transitions: Vec<TransitionStage>,
    pub first_invalid_timestamp_stage: Option<usize>,
    pub timestamp_evolution: TimestampEvolution,
    pub verdict: TransitionVerdict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionStage {
    pub stage_index: usize,
    pub stage_name: String,
    pub container_type: String,
    pub stream_count: StreamCounts,
    pub timestamp_info: TimestampInfo,
    pub has_invalid_timestamps: bool,
    pub issues: Vec<TimestampIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamCounts {
    pub video: usize,
    pub audio: usize,
    pub subtitle: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampInfo {
    pub has_pts: bool,
    pub has_dts: bool,
    pub has_start_time: bool,
    pub start_time: Option<f64>,
    pub timebase: Option<String>,
    pub duration: Option<f64>,
    pub first_pts: Option<f64>,
    pub last_pts: Option<f64>,
    pub pts_gaps: Vec<PtsGap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtsGap {
    pub position: usize,
    pub gap_seconds: f64,
    pub severity: GapSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GapSeverity {
    Small,
    Medium,
    Large,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampIssue {
    pub issue_type: IssueType,
    pub description: String,
    pub severity: IssueSeverity,
    pub affected_stream: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueType {
    MissingPts,
    MissingDts,
    NegativePts,
    NegativeDts,
    PtsDtsMismatch,
    NonMonotonicPts,
    NonMonotonicDts,
    TimebaseChange,
    LargeGap,
    InvalidTimestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampEvolution {
    pub start_times: Vec<Option<f64>>,
    pub first_pts_values: Vec<Option<f64>>,
    pub last_pts_values: Vec<Option<f64>>,
    pub duration_values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransitionVerdict {
    Clean,
    HasIssues,
    FailedAtStage(usize),
}

impl std::fmt::Display for TransitionVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionVerdict::Clean => write!(f, "CLEAN (no timestamp issues detected)"),
            TransitionVerdict::HasIssues => write!(f, "HAS ISSUES (manual verification recommended)"),
            TransitionVerdict::FailedAtStage(stage) => write!(f, "FAILED AT STAGE {} (timestamp invalid)", stage),
        }
    }
}

pub fn audit_container_transition(
    ffprobe_path: &Path,
    file_path: &Path,
) -> Result<ContainerTransitionAudit, String> {
    let mut transitions = Vec::new();

    let stage1 = analyze_stage(
        ffprobe_path,
        file_path,
        "Original",
        0,
    )?;
    transitions.push(stage1);

    let has_invalid = transitions.iter().any(|t| t.has_invalid_timestamps);
    let first_invalid = if has_invalid {
        transitions.iter().position(|t| t.has_invalid_timestamps)
    } else {
        None
    };

    let timestamp_evolution = build_timestamp_evolution(&transitions);

    let verdict = if !has_invalid {
        TransitionVerdict::Clean
    } else if let Some(idx) = first_invalid {
        TransitionVerdict::FailedAtStage(idx)
    } else {
        TransitionVerdict::HasIssues
    };

    Ok(ContainerTransitionAudit {
        file_path: file_path.to_string_lossy().into_owned(),
        transitions,
        first_invalid_timestamp_stage: first_invalid,
        timestamp_evolution,
        verdict,
    })
}

fn analyze_stage(
    ffprobe_path: &Path,
    file_path: &Path,
    stage_name: &str,
    stage_index: usize,
) -> Result<TransitionStage, String> {
    let probe_output = run_detailed_probe(ffprobe_path, file_path)?;

    let has_invalid = check_for_invalid_timestamps(&probe_output);

    let issues = detect_timestamp_issues(&probe_output);

    let container_type = detect_container_type(file_path);

    let stream_counts = StreamCounts {
        video: probe_output.streams.iter().filter(|s| s.codec_type == "video").count(),
        audio: probe_output.streams.iter().filter(|s| s.codec_type == "audio").count(),
        subtitle: probe_output.streams.iter().filter(|s| s.codec_type == "subtitle").count(),
    };

    let timestamp_info = TimestampInfo {
        has_pts: probe_output.streams.iter().all(|s| s.start_pts.is_some()),
        has_dts: probe_output.streams.iter().all(|s| s.start_dts.is_some()),
        has_start_time: probe_output.format.start_time.is_some(),
        start_time: probe_output.format.start_time,
        timebase: probe_output.streams.first().and_then(|s| s.time_base.clone()),
        duration: probe_output.format.duration,
        first_pts: {
            let mut min_val: Option<f64> = None;
            for stream in &probe_output.streams {
                if let Some(pts) = stream.first_pts {
                    min_val = Some(match min_val {
                        None => pts,
                        Some(curr) if pts < curr => pts,
                        Some(curr) => curr,
                    });
                }
            }
            min_val
        },
        last_pts: {
            let mut max_val: Option<f64> = None;
            for stream in &probe_output.streams {
                if let Some(pts) = stream.last_pts {
                    max_val = Some(match max_val {
                        None => pts,
                        Some(curr) if pts > curr => pts,
                        Some(curr) => curr,
                    });
                }
            }
            max_val
        },
        pts_gaps: detect_pts_gaps(&probe_output),
    };

    Ok(TransitionStage {
        stage_index,
        stage_name: stage_name.to_string(),
        container_type,
        stream_count: stream_counts,
        timestamp_info,
        has_invalid_timestamps: has_invalid,
        issues,
    })
}

fn run_detailed_probe(ffprobe_path: &Path, file_path: &Path) -> Result<DetailedProbeOutput, String> {
    let output = Command::new(ffprobe_path)
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            "-show_packets",
            file_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

    parse_detailed_probe(&json)
}

fn parse_detailed_probe(json: &serde_json::Value) -> Result<DetailedProbeOutput, String> {
    let format = &json["format"];
    let streams = json["streams"].as_array()
        .ok_or("Missing streams")?;

    let mut parsed_streams = Vec::new();
    for stream in streams {
        let codec_type = stream["codec_type"].as_str().unwrap_or("");
        if codec_type != "video" && codec_type != "audio" && codec_type != "subtitle" {
            continue;
        }

        let start_pts = stream["start_pts"].as_f64()
            .or_else(|| stream["start_time"].as_str().and_then(|s| s.parse().ok()));
        let start_dts = stream["start_dts"].as_f64()
            .or_else(|| stream["start_time"].as_str().and_then(|s| s.parse().ok()));

        let first_pts = stream["first_pts"].as_f64()
            .or_else(|| stream["start_time"].as_str().and_then(|s| s.parse().ok()));
        let last_pts = stream["last_pts"].as_f64()
            .or_else(|| stream["duration"].as_str().and_then(|s| {
                let dur: f64 = s.parse().ok()?;
                let start: f64 = stream["start_time"].as_str()?.parse().ok()?;
                Some(start + dur)
            }));

        parsed_streams.push(ParsedStream {
            codec_type: codec_type.to_string(),
            index: stream["index"].as_i64().unwrap_or(0) as usize,
            codec_name: stream["codec_name"].as_str().map(|s| s.to_string()),
            time_base: stream["time_base"].as_str().map(|s| s.to_string()),
            start_pts,
            start_dts,
            first_pts,
            last_pts,
            duration: stream["duration"].as_str().and_then(|s| s.parse().ok()),
        });
    }

    Ok(DetailedProbeOutput {
        format: ParsedFormat {
            filename: format["filename"].as_str().map(|s| s.to_string()),
            format_name: format["format_name"].as_str().map(|s| s.to_string()),
            start_time: format["start_time"].as_str().and_then(|s| s.parse().ok()),
            duration: format["duration"].as_str().and_then(|s| s.parse().ok()),
        },
        streams: parsed_streams,
    })
}

#[derive(Debug, Clone)]
pub struct DetailedProbeOutput {
    pub format: ParsedFormat,
    pub streams: Vec<ParsedStream>,
}

#[derive(Debug, Clone)]
pub struct ParsedFormat {
    pub filename: Option<String>,
    pub format_name: Option<String>,
    pub start_time: Option<f64>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ParsedStream {
    pub codec_type: String,
    pub index: usize,
    pub codec_name: Option<String>,
    pub time_base: Option<String>,
    pub start_pts: Option<f64>,
    pub start_dts: Option<f64>,
    pub first_pts: Option<f64>,
    pub last_pts: Option<f64>,
    pub duration: Option<f64>,
}

fn detect_container_type(file_path: &Path) -> String {
    file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "unknown".to_string())
}

fn check_for_invalid_timestamps(probe: &DetailedProbeOutput) -> bool {
    for stream in &probe.streams {
        if stream.start_pts.map(|p| p < 0.0).unwrap_or(false) {
            return true;
        }
        if stream.first_pts.map(|p| p < 0.0).unwrap_or(false) {
            return true;
        }
    }
    false
}

fn detect_timestamp_issues(probe: &DetailedProbeOutput) -> Vec<TimestampIssue> {
    let mut issues = Vec::new();

    for stream in &probe.streams {
        if stream.start_pts.is_none() {
            issues.push(TimestampIssue {
                issue_type: IssueType::MissingPts,
                description: format!("Stream {} missing start_pts", stream.index),
                severity: IssueSeverity::Error,
                affected_stream: Some(stream.index),
            });
        }

        if stream.start_dts.is_none() {
            issues.push(TimestampIssue {
                issue_type: IssueType::MissingDts,
                description: format!("Stream {} missing start_dts", stream.index),
                severity: IssueSeverity::Error,
                affected_stream: Some(stream.index),
            });
        }

        if let Some(pts) = stream.start_pts {
            if pts < 0.0 {
                issues.push(TimestampIssue {
                    issue_type: IssueType::NegativePts,
                    description: format!("Stream {} has negative PTS: {}", stream.index, pts),
                    severity: IssueSeverity::Error,
                    affected_stream: Some(stream.index),
                });
            }
        }

        if let (Some(pts), Some(dts)) = (stream.start_pts, stream.start_dts) {
            if pts < dts {
                issues.push(TimestampIssue {
                    issue_type: IssueType::PtsDtsMismatch,
                    description: format!("Stream {} PTS ({}) < DTS ({})", stream.index, pts, dts),
                    severity: IssueSeverity::Error,
                    affected_stream: Some(stream.index),
                });
            }
        }
    }

    issues
}

fn detect_pts_gaps(_probe: &DetailedProbeOutput) -> Vec<PtsGap> {
    Vec::new()
}

fn build_timestamp_evolution(transitions: &[TransitionStage]) -> TimestampEvolution {
    let start_times: Vec<Option<f64>> = transitions.iter().map(|t| t.timestamp_info.start_time).collect();
    let first_pts_values: Vec<Option<f64>> = transitions.iter().map(|t| t.timestamp_info.first_pts).collect();
    let last_pts_values: Vec<Option<f64>> = transitions.iter().map(|t| t.timestamp_info.last_pts).collect();
    let duration_values: Vec<Option<f64>> = transitions.iter().map(|t| t.timestamp_info.duration).collect();

    TimestampEvolution {
        start_times,
        first_pts_values,
        last_pts_values,
        duration_values,
    }
}

impl std::fmt::Display for ContainerTransitionAudit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║         CONTAINER TRANSITION AUDIT REPORT                         ║")?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║ File: {}     ║", truncate_middle(&self.file_path, 50))?;
        writeln!(f, "║ Verdict: {}                                           ║", self.verdict)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;

        for stage in &self.transitions {
            writeln!(f, "║ STAGE {}: {}                                            ║", stage.stage_index, stage.stage_name)?;
            writeln!(f, "║   Container: {}                                               ║", stage.container_type)?;
            writeln!(f, "║   Streams: V={}, A={}, S={}                                          ║",
                stage.stream_count.video, stage.stream_count.audio, stage.stream_count.subtitle)?;
            writeln!(f, "║   Start time: {:?}                                          ║", stage.timestamp_info.start_time)?;
            writeln!(f, "║   First PTS: {:?}                                           ║", stage.timestamp_info.first_pts)?;
            writeln!(f, "║   Last PTS: {:?}                                            ║", stage.timestamp_info.last_pts)?;
            writeln!(f, "║   Timebase: {:?}                                            ║", stage.timestamp_info.timebase)?;
            writeln!(f, "║   Has invalid timestamps: {}                                   ║", stage.has_invalid_timestamps)?;
            if !stage.issues.is_empty() {
                writeln!(f, "║   Issues:                                                       ║")?;
                for issue in &stage.issues {
                    writeln!(f, "║     - {:?} ({:?}): {}       ║",
                        issue.issue_type, issue.severity,
                        truncate_middle(&issue.description, 40))?;
                }
            }
            writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        }

        writeln!(f, "╚══════════════════════════════════════════════════════════════════════╝")?;

        Ok(())
    }
}

fn truncate_middle(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let ellipsis = "...";
    let available = max_len - ellipsis.len();
    let front = available / 2;
    let back = available - front;
    format!("{}{}{}", &s[..front], ellipsis, &s[s.len() - back..])
}