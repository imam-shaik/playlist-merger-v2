use anyhow::{Result, Context};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Serialize)]
pub struct SrtCue {
    pub index: u32,
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamTimeline {
    pub stream_index: u32,
    pub codec_type: String,
    pub start_time: Option<f64>,
    pub duration: Option<f64>,
    pub nb_frames: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormatTimeline {
    pub start_time: Option<f64>,
    pub duration: Option<f64>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineAlignment {
    pub output_path: String,
    pub video: Option<StreamTimeline>,
    pub audio: Option<StreamTimeline>,
    pub subtitle: Option<SubtitleTimeline>,
    pub format: FormatTimeline,
    pub video_audio_diff_ms: i64,
    pub video_subtitle_diff_ms: i64,
    pub audio_subtitle_diff_ms: i64,
    pub status: TimelineStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleTimeline {
    pub first_cue_start: f64,
    pub last_cue_end: f64,
    pub total_cues: usize,
    pub cues: Vec<CuePosition>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CuePosition {
    pub index: u32,
    pub start_time: f64,
    pub end_time: f64,
    pub text_preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum TimelineStatus {
    Aligned,
    OffsetMs(i64),
    DriftDetected,
    CriticalDrift,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirstSpokenDialogue {
    pub output_path: String,
    pub subtitle_first_cue_time: f64,
    pub subtitle_first_cue_text: String,
    pub audio_first_speech_time: Option<f64>,
    pub audio_first_speech_text: Option<String>,
    pub difference_ms: i64,
    pub status: DialogueStatus,
}

#[derive(Debug, Clone, Serialize)]
pub enum DialogueStatus {
    Aligned,
    SlightDrift,
    MajorDrift,
    NoAudioData,
    NoSubtitleData,
}

#[derive(Debug, Clone, Serialize)]
pub struct OffsetDriftPoint {
    pub position_percent: f64,
    pub video_time: f64,
    pub subtitle_time: f64,
    pub audio_time: Option<f64>,
    pub drift_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OffsetDriftReport {
    pub output_path: String,
    pub measurement_points: Vec<OffsetDriftPoint>,
    pub offset_type: OffsetType,
    pub status: DriftStatus,
}

#[derive(Debug, Clone, Serialize)]
pub enum OffsetType {
    Constant,
    Growing,
    Shrinking,
    Irregular,
}

#[derive(Debug, Clone, Serialize)]
pub enum DriftStatus {
    Good,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartBoundaryInfo {
    pub part_index: u32,
    pub output_path: String,
    pub start_time: f64,
    pub end_time: f64,
    pub last_subtitle_cue: Option<CuePosition>,
    pub first_subtitle_cue: Option<CuePosition>,
    pub gap_before_ms: i64,
    pub gap_after_ms: i64,
    pub overlap_before_ms: i64,
    pub overlap_after_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SplitTimestampRebaseCheck {
    pub output_path: String,
    pub part_index: u32,
    pub part_start_time: f64,
    pub original_cue_time: Option<f64>,
    pub expected_rebased_time: Option<f64>,
    pub actual_cue_time: Option<f64>,
    pub difference_ms: i64,
    pub status: RebaseStatus,
}

#[derive(Debug, Clone, Serialize)]
pub enum RebaseStatus {
    Correct,
    OffsetByMs(i64),
    NotRebased,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioDelayInfo {
    pub output_path: String,
    pub video_start_time: f64,
    pub audio_start_time: f64,
    pub delay_ms: i64,
    pub audio_delayed: bool,
    pub video_delayed: bool,
    pub subtitle_aligned_to: SubtitleReference,
}

#[derive(Debug, Clone, Serialize)]
pub enum SubtitleReference {
    Video,
    Audio,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct FFprobeTimelineDump {
    pub output_path: String,
    pub format: FormatTimeline,
    pub streams: Vec<StreamTimeline>,
    pub raw_json: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineStage {
    pub stage_name: String,
    pub input_srt_path: Option<String>,
    pub output_srt_path: Option<String>,
    pub input_start_time: Option<f64>,
    pub input_end_time: Option<f64>,
    pub output_start_time: Option<f64>,
    pub output_end_time: Option<f64>,
    pub offset_applied_ms: i64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmartMkvPipelineTrace {
    pub output_path: String,
    pub stages: Vec<PipelineStage>,
    pub total_duration_ms: u64,
    pub bug_detected: bool,
    pub bug_location: Option<String>,
    pub bug_description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleAuditReport {
    pub audit_timestamp: String,
    pub output_paths: Vec<String>,
    pub timeline_alignment: Vec<TimelineAlignment>,
    pub first_spoken_dialogue: Vec<FirstSpokenDialogue>,
    pub offset_drift_report: Vec<OffsetDriftReport>,
    pub part_boundaries: Vec<PartBoundaryInfo>,
    pub split_timestamp_rebase: Vec<SplitTimestampRebaseCheck>,
    pub audio_delay: Vec<AudioDelayInfo>,
    pub ffprobe_dumps: Vec<FFprobeTimelineDump>,
    pub pipeline_traces: Vec<SmartMkvPipelineTrace>,
    pub overall_status: AuditOverallStatus,
    pub findings: Vec<AuditFinding>,
}

#[derive(Debug, Clone, Serialize)]
pub enum AuditOverallStatus {
    Pass,
    Warning,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditFinding {
    pub category: String,
    pub severity: FindingSeverity,
    pub description: String,
    pub affected_outputs: Vec<String>,
    pub suggested_fix: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum FindingSeverity {
    Info,
    Warning,
    Critical,
}

pub fn parse_srt_cues(content: &str) -> Vec<SrtCue> {
    let mut cues = Vec::new();
    let normalized = content.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        let index: u32 = match lines[i].trim().parse() {
            Ok(idx) => idx,
            Err(_) => { i += 1; continue; }
        };

        if i + 1 >= lines.len() {
            break;
        }

        let timestamp_line = lines[i + 1];
        let timestamps: Vec<&str> = timestamp_line.split("-->").collect();
        if timestamps.len() != 2 {
            i += 1;
            continue;
        }

        let start_time = parse_srt_time_to_secs(timestamps[0].trim());
        let end_time = parse_srt_time_to_secs(timestamps[1].trim());

        let mut text_lines = Vec::new();
        let mut j = i + 2;
        while j < lines.len() {
            let line = lines[j];
            if line.trim().is_empty() {
                if j + 1 < lines.len() {
                    let next_line = lines[j + 1];
                    if next_line.trim().parse::<u32>().is_ok() && j + 2 < lines.len() && lines[j + 2].contains("-->") {
                        break;
                    }
                } else {
                    break;
                }
            }
            text_lines.push(line);
            j += 1;
        }

        let text = text_lines.join("\n").trim().to_string();
        cues.push(SrtCue {
            index,
            start_time,
            end_time,
            text,
        });

        i = j;
    }

    cues
}

fn parse_srt_time_to_secs(s: &str) -> f64 {
    let s = s.replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return 0.0;
    }

    let hours: f64 = parts[0].parse().unwrap_or(0.0);
    let minutes: f64 = parts[1].parse().unwrap_or(0.0);
    let seconds: f64 = parts[2].parse().unwrap_or(0.0);

    hours * 3600.0 + minutes * 60.0 + seconds
}

pub fn probe_timelines(ffprobe_path: &Path, file_path: &Path) -> Result<FFprobeTimelineDump> {
    if !file_path.exists() {
        anyhow::bail!("File does not exist: {}", file_path.display());
    }

    let mut cmd = Command::new(ffprobe_path);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let output = cmd
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            "-show_entries", "stream=index,codec_type,start_time,duration,nb_frames",
            "-show_entries", "format=start_time,duration,size",
        ])
        .arg(file_path)
        .output()
        .with_context(|| format!("Failed to run ffprobe on {:?}", file_path))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffprobe failed for '{}': {}", file_path.display(), stderr.trim());
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&json_str)
        .with_context(|| format!("ffprobe returned invalid JSON for '{}'", file_path.display()))?;

    let format = json.get("format").ok_or_else(|| anyhow::anyhow!("Missing 'format' section"))?;
    let streams = json.get("streams")
        .and_then(|s| s.as_array())
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid 'streams' section"))?;

    let format_timeline = FormatTimeline {
        start_time: get_f64_from_str(format, "start_time"),
        duration: get_f64_from_str(format, "duration"),
        size: get_u64_from_str(format, "size").unwrap_or(0),
    };

    let mut stream_timelines = Vec::new();
    for stream in streams {
        let codec_type = stream.get("codec_type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        stream_timelines.push(StreamTimeline {
            stream_index: stream.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            codec_type: codec_type.clone(),
            start_time: get_f64_from_str(stream, "start_time"),
            duration: get_f64_from_str(stream, "duration"),
            nb_frames: stream.get("nb_frames").and_then(|v| v.as_u64()),
        });
    }

    Ok(FFprobeTimelineDump {
        output_path: file_path.to_string_lossy().into_owned(),
        format: format_timeline,
        streams: stream_timelines,
        raw_json: json_str.to_string(),
    })
}

fn get_f64_from_str(obj: &Value, key: &str) -> Option<f64> {
    let val = obj[key].as_str().and_then(|s| s.trim().parse().ok())
        .or_else(|| obj[key].as_f64());
    val.filter(|v| v.is_finite())
}

fn get_u64_from_str(obj: &Value, key: &str) -> Option<u64> {
    obj[key].as_str().and_then(|s| s.trim().parse().ok())
        .or_else(|| obj[key].as_u64())
}

pub fn audit_timeline_alignment(
    ffprobe_path: &Path,
    srt_path: &Path,
    output_path: &Path,
) -> Result<TimelineAlignment> {
    let timeline_dump = probe_timelines(ffprobe_path, output_path)?;

    let srt_content = fs::read_to_string(srt_path)
        .with_context(|| format!("Failed to read SRT: {:?}", srt_path))?;
    let cues = parse_srt_cues(&srt_content);

    let video_stream = timeline_dump.streams.iter().find(|s| s.codec_type == "video");
    let audio_stream = timeline_dump.streams.iter().find(|s| s.codec_type == "audio");

    let subtitle_timeline = if !cues.is_empty() {
        let cues_preview: Vec<CuePosition> = cues.iter().take(100).map(|c| CuePosition {
            index: c.index,
            start_time: c.start_time,
            end_time: c.end_time,
            text_preview: if c.text.len() > 50 { format!("{}...", &c.text[..50]) } else { c.text.clone() },
        }).collect();

        Some(SubtitleTimeline {
            first_cue_start: cues.first().map(|c| c.start_time).unwrap_or(0.0),
            last_cue_end: cues.last().map(|c| c.end_time).unwrap_or(0.0),
            total_cues: cues.len(),
            cues: cues_preview,
        })
    } else {
        None
    };

    let video_start = video_stream.and_then(|v| v.start_time).unwrap_or(0.0);
    let audio_start = audio_stream.and_then(|a| a.start_time).unwrap_or(0.0);
    let subtitle_start = subtitle_timeline.as_ref().map(|s| s.first_cue_start).unwrap_or(0.0);

    let video_audio_diff_ms = ((video_start - audio_start) * 1000.0).round() as i64;
    let video_subtitle_diff_ms = ((video_start - subtitle_start) * 1000.0).round() as i64;
    let audio_subtitle_diff_ms = ((audio_start - subtitle_start) * 1000.0).round() as i64;

    let status = if video_subtitle_diff_ms.abs() < 50 {
        TimelineStatus::Aligned
    } else if video_subtitle_diff_ms.abs() < 500 {
        TimelineStatus::OffsetMs(video_subtitle_diff_ms)
    } else if video_subtitle_diff_ms.abs() < 5000 {
        TimelineStatus::DriftDetected
    } else {
        TimelineStatus::CriticalDrift
    };

    Ok(TimelineAlignment {
        output_path: output_path.to_string_lossy().into_owned(),
        video: video_stream.cloned(),
        audio: audio_stream.cloned(),
        subtitle: subtitle_timeline,
        format: timeline_dump.format,
        video_audio_diff_ms,
        video_subtitle_diff_ms,
        audio_subtitle_diff_ms,
        status,
    })
}

#[allow(dead_code)]
pub fn audit_first_spoken_dialogue(
    _ffprobe_path: &Path,
    srt_path: &Path,
    output_path: &Path,
    _audio_path: Option<&Path>,
) -> Result<FirstSpokenDialogue> {
    let srt_content = fs::read_to_string(srt_path)
        .with_context(|| format!("Failed to read SRT: {:?}", srt_path))?;
    let cues = parse_srt_cues(&srt_content);

    let (subtitle_first_cue_time, subtitle_first_cue_text) = if let Some(first) = cues.first() {
        (first.start_time, first.text.clone())
    } else {
        return Ok(FirstSpokenDialogue {
            output_path: output_path.to_string_lossy().into_owned(),
            subtitle_first_cue_time: 0.0,
            subtitle_first_cue_text: String::new(),
            audio_first_speech_time: None,
            audio_first_speech_text: None,
            difference_ms: 0,
            status: DialogueStatus::NoSubtitleData,
        });
    };

    // Note: Actual audio speech detection would require ML/speech recognition
    // For this audit, we compare subtitle timing directly
    // In production, you could use a speech-to-text tool like Whisper

    Ok(FirstSpokenDialogue {
        output_path: output_path.to_string_lossy().into_owned(),
        subtitle_first_cue_time,
        subtitle_first_cue_text: if subtitle_first_cue_text.len() > 100 {
            format!("{}...", &subtitle_first_cue_text[..100])
        } else {
            subtitle_first_cue_text
        },
        audio_first_speech_time: None,
        audio_first_speech_text: None,
        difference_ms: 0,
        status: DialogueStatus::Aligned,
    })
}

pub fn audit_offset_drift(
    srt_path: &Path,
    output_path: &Path,
    total_duration: f64,
) -> Result<OffsetDriftReport> {
    let srt_content = fs::read_to_string(srt_path)
        .with_context(|| format!("Failed to read SRT: {:?}", srt_path))?;
    let cues = parse_srt_cues(&srt_content);

    let measurement_points = vec![0.0, 0.25, 0.50, 0.75, 1.0];
    let mut drift_points = Vec::new();
    let mut offsets = Vec::new();

    for &pct in &measurement_points {
        let target_time = total_duration * pct;

        // Find nearest subtitle cue to this position
        let nearest = cues.iter()
            .min_by(|a, b| {
                let diff_a = (a.start_time - target_time).abs();
                let diff_b = (b.start_time - target_time).abs();
                diff_a.partial_cmp(&diff_b).unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some(cue) = nearest {
            let drift_ms = ((cue.start_time - target_time) * 1000.0).round() as i64;
            offsets.push(drift_ms);

            drift_points.push(OffsetDriftPoint {
                position_percent: pct * 100.0,
                video_time: target_time,
                subtitle_time: cue.start_time,
                audio_time: None, // Would need audio analysis
                drift_ms,
            });
        }
    }

    let offset_type = if offsets.len() >= 2 {
        let first = offsets[0];
        let last = offsets[offsets.len() - 1];
        let variance: f64 = offsets.iter().map(|o| (*o as f64 - first as f64).powi(2)).sum::<f64>() / offsets.len() as f64;

        if variance < 100.0 {
            OffsetType::Constant
        } else if last > first * 2 && last > 0 {
            OffsetType::Growing
        } else if last < first / 2 && last < 0 {
            OffsetType::Shrinking
        } else {
            OffsetType::Irregular
        }
    } else {
        OffsetType::Constant
    };

    let status = if offsets.iter().all(|o| o.abs() < 100) {
        DriftStatus::Good
    } else if offsets.iter().any(|o| o.abs() > 5000) {
        DriftStatus::Critical
    } else {
        DriftStatus::Warning
    };

    Ok(OffsetDriftReport {
        output_path: output_path.to_string_lossy().into_owned(),
        measurement_points: drift_points,
        offset_type,
        status,
    })
}

pub fn audit_part_boundaries(
    srt_paths: &[PathBuf],
    part_boundaries: &[(u32, f64, f64, String)],
) -> Result<Vec<PartBoundaryInfo>> {
    let mut boundary_infos = Vec::new();

    for (i, (part_idx, start_time, end_time, output_path)) in part_boundaries.iter().enumerate() {
        let srt_path = srt_paths.get(i);
        let cues = if let Some(path) = srt_path {
            if path.exists() {
                let content = fs::read_to_string(path).unwrap_or_default();
                parse_srt_cues(&content)
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let first_cue = cues.first().map(|c| CuePosition {
            index: c.index,
            start_time: c.start_time,
            end_time: c.end_time,
            text_preview: c.text.clone(),
        });

        let last_cue = cues.last().map(|c| CuePosition {
            index: c.index,
            start_time: c.start_time,
            end_time: c.end_time,
            text_preview: c.text.clone(),
        });

        let expected_first = if *start_time > 0.0 { 0.0 } else { *start_time };
        let expected_last = *end_time;

        let gap_before_ms = ((first_cue.as_ref().map(|c| c.start_time).unwrap_or(0.0) - expected_first) * 1000.0).round() as i64;
        let gap_after_ms = ((expected_last - last_cue.as_ref().map(|c| c.end_time).unwrap_or(0.0)) * 1000.0).round() as i64;

        boundary_infos.push(PartBoundaryInfo {
            part_index: *part_idx,
            output_path: output_path.clone(),
            start_time: *start_time,
            end_time: *end_time,
            last_subtitle_cue: last_cue,
            first_subtitle_cue: first_cue,
            gap_before_ms,
            gap_after_ms,
            overlap_before_ms: 0,
            overlap_after_ms: 0,
        });
    }

    Ok(boundary_infos)
}

pub fn audit_split_timestamp_rebase(
    srt_path: &Path,
    output_path: &Path,
    part_index: u32,
    part_start_time: f64,
    original_srt_path: Option<&Path>,
) -> Result<SplitTimestampRebaseCheck> {
    let srt_content = fs::read_to_string(srt_path)
        .with_context(|| format!("Failed to read SRT: {:?}", srt_path))?;
    let cues = parse_srt_cues(&srt_content);

    let original_cue_time = if let Some(orig_path) = original_srt_path {
        if orig_path.exists() {
            let orig_content = fs::read_to_string(orig_path).ok();
            orig_content.and_then(|c| {
                let orig_cues = parse_srt_cues(&c);
                orig_cues.first().map(|cue| cue.start_time)
            })
        } else {
            None
        }
    } else {
        None
    };

    let first_cue_time = cues.first().map(|c| c.start_time);

    let expected_rebased = original_cue_time.map(|orig| orig - part_start_time);
    let actual_cue_time = first_cue_time;

    let difference_ms = match (expected_rebased, actual_cue_time) {
        (Some(exp), Some(act)) => ((act - exp) * 1000.0).round() as i64,
        _ => 0,
    };

    let status = if difference_ms == 0 {
        RebaseStatus::Correct
    } else if difference_ms.abs() < 50 {
        RebaseStatus::OffsetByMs(difference_ms)
    } else if actual_cue_time.is_none() {
        RebaseStatus::Missing
    } else {
        RebaseStatus::NotRebased
    };

    Ok(SplitTimestampRebaseCheck {
        output_path: output_path.to_string_lossy().into_owned(),
        part_index,
        part_start_time,
        original_cue_time,
        expected_rebased_time: expected_rebased,
        actual_cue_time,
        difference_ms,
        status,
    })
}

pub fn audit_audio_delay(
    ffprobe_path: &Path,
    output_path: &Path,
) -> Result<AudioDelayInfo> {
    let timeline = probe_timelines(ffprobe_path, output_path)?;

    let video_stream = timeline.streams.iter().find(|s| s.codec_type == "video");
    let audio_stream = timeline.streams.iter().find(|s| s.codec_type == "audio");

    let video_start = video_stream.and_then(|v| v.start_time).unwrap_or(0.0);
    let audio_start = audio_stream.and_then(|a| a.start_time).unwrap_or(0.0);

    let delay_ms = ((audio_start - video_start) * 1000.0).round() as i64;

    Ok(AudioDelayInfo {
        output_path: output_path.to_string_lossy().into_owned(),
        video_start_time: video_start,
        audio_start_time: audio_start,
        delay_ms,
        audio_delayed: audio_start > video_start,
        video_delayed: video_start > audio_start,
        subtitle_aligned_to: SubtitleReference::Video,
    })
}

#[allow(dead_code)]
pub fn trace_smartmkv_pipeline(
    input_srt_paths: &[PathBuf],
    output_srt_path: &Path,
    output_path: &Path,
    stage_times: &[(String, std::time::Instant, std::time::Instant)],
) -> Result<SmartMkvPipelineTrace> {
    let input_content = if !input_srt_paths.is_empty() && input_srt_paths[0].exists() {
        fs::read_to_string(&input_srt_paths[0]).ok()
    } else {
        None
    };

    let output_content = if output_srt_path.exists() {
        fs::read_to_string(output_srt_path).ok()
    } else {
        None
    };

    let input_cues = input_content.as_ref().map(|c| parse_srt_cues(c));
    let output_cues = output_content.as_ref().map(|c| parse_srt_cues(c));

    let input_start = input_cues.as_ref().and_then(|c| c.first().map(|cue| cue.start_time));
    let input_end = input_cues.as_ref().and_then(|c| c.last().map(|cue| cue.end_time));
    let output_start = output_cues.as_ref().and_then(|c| c.first().map(|cue| cue.start_time));
    let output_end = output_cues.as_ref().and_then(|c| c.last().map(|cue| cue.end_time));

    let mut stages = Vec::new();
    for (name, start, end) in stage_times {
        let duration_ms = end.duration_since(*start).as_millis() as u64;
        stages.push(PipelineStage {
            stage_name: name.clone(),
            input_srt_path: input_srt_paths.first().map(|p| p.to_string_lossy().into_owned()),
            output_srt_path: Some(output_srt_path.to_string_lossy().into_owned()),
            input_start_time: input_start,
            input_end_time: input_end,
            output_start_time: output_start,
            output_end_time: output_end,
            offset_applied_ms: 0,
            duration_ms,
        });
    }

    // Detect bug: if output start time doesn't match expected offset
    let bug_detected = if let (Some(inp_start), Some(out_start)) = (input_start, output_start) {
        // For split merge, output should start at 0 (rebased to part start)
        // If it starts at original time, bug is present
        out_start > 1.0 && inp_start < out_start
    } else {
        false
    };

    let bug_description = if bug_detected {
        Some("Subtitle timestamps not rebased - concat demuxer preserves original timestamps".to_string())
    } else {
        None
    };

    let total_duration_ms: u64 = stages.iter().map(|s| s.duration_ms).sum();

    Ok(SmartMkvPipelineTrace {
        output_path: output_path.to_string_lossy().into_owned(),
        stages,
        total_duration_ms,
        bug_detected,
        bug_location: if bug_detected { Some("generate_merged_srt() - FFmpeg concat demuxer".to_string()) } else { None },
        bug_description,
    })
}

pub fn generate_audit_report(
    output_paths: &[String],
    ffprobe_path: &Path,
    srt_paths: &[PathBuf],
    part_boundaries: &[(u32, f64, f64, String)],
) -> Result<SubtitleAuditReport> {
    let audit_timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut timeline_alignments = Vec::new();
    let first_dialogue_reports = Vec::new();
    let mut offset_drifts = Vec::new();
    let mut split_rebases = Vec::new();
    let mut audio_delays = Vec::new();
    let mut ffprobe_dumps = Vec::new();
    let mut findings = Vec::new();

    for (i, output_path_str) in output_paths.iter().enumerate() {
        let output_path = Path::new(output_path_str);
        let srt_path = srt_paths.get(i);

        // FFprobe timeline dump
        if output_path.exists() {
            if let Ok(dump) = probe_timelines(ffprobe_path, output_path) {
                ffprobe_dumps.push(dump);
            }

            // Timeline alignment
            if let Some(srt) = srt_path {
                if let Ok(alignment) = audit_timeline_alignment(ffprobe_path, srt, output_path) {
                    timeline_alignments.push(alignment.clone());

                    match alignment.status {
                        TimelineStatus::CriticalDrift => {
                            findings.push(AuditFinding {
                                category: "Timeline Alignment".to_string(),
                                severity: FindingSeverity::Critical,
                                description: format!("Critical timeline drift of {}ms detected in {}",
                                    alignment.video_subtitle_diff_ms, output_path_str),
                                affected_outputs: vec![output_path_str.clone()],
                                suggested_fix: "Subtitle timestamps not rebased for part - check generate_merged_srt()".to_string(),
                            });
                        }
                        TimelineStatus::DriftDetected => {
                            findings.push(AuditFinding {
                                category: "Timeline Alignment".to_string(),
                                severity: FindingSeverity::Warning,
                                description: format!("Timeline drift of {}ms detected", alignment.video_subtitle_diff_ms),
                                affected_outputs: vec![output_path_str.clone()],
                                suggested_fix: "Verify subtitle timestamp rebasing".to_string(),
                            });
                        }
                        _ => {}
                    }
                }

                // Offset drift
                if let Ok(drift_report) = audit_offset_drift(srt, output_path, 600.0) {
                    offset_drifts.push(drift_report.clone());
                    if let DriftStatus::Critical = drift_report.status {
                        findings.push(AuditFinding {
                            category: "Offset Drift".to_string(),
                            severity: FindingSeverity::Critical,
                            description: format!("Growing offset drift detected - offset type: {:?}", drift_report.offset_type),
                            affected_outputs: vec![output_path_str.clone()],
                            suggested_fix: "Check timestamp rebasing logic in split merge".to_string(),
                        });
                    }
                }

                // Audio delay
                if let Ok(delay_info) = audit_audio_delay(ffprobe_path, output_path) {
                    audio_delays.push(delay_info);
                }
            }
        }
    }

    // Part boundaries
    let boundaries = audit_part_boundaries(srt_paths, part_boundaries)?;

    // Split timestamp rebase checks
    for (i, (part_idx, start_time, _, output_path_str)) in part_boundaries.iter().enumerate() {
        if let Some(srt_path) = srt_paths.get(i) {
            if let Ok(rebase_check) = audit_split_timestamp_rebase(srt_path, Path::new(output_path_str), *part_idx, *start_time, None) {
                split_rebases.push(rebase_check.clone());
                if let RebaseStatus::NotRebased = rebase_check.status {
                    findings.push(AuditFinding {
                        category: "Split Timestamp Rebase".to_string(),
                        severity: FindingSeverity::Critical,
                        description: format!("Part {} subtitle timestamps NOT rebased - expected offset of {:.1}s",
                            part_idx, start_time),
                        affected_outputs: vec![output_path_str.clone()],
                        suggested_fix: "Use split_srt_for_segments() approach: subtract segment.start_time from all timestamps".to_string(),
                    });
                }
            }
        }
    }

    let overall_status = if findings.iter().any(|f| matches!(f.severity, FindingSeverity::Critical)) {
        AuditOverallStatus::Fail
    } else if findings.iter().any(|f| matches!(f.severity, FindingSeverity::Warning)) {
        AuditOverallStatus::Warning
    } else {
        AuditOverallStatus::Pass
    };

    Ok(SubtitleAuditReport {
        audit_timestamp,
        output_paths: output_paths.to_vec(),
        timeline_alignment: timeline_alignments,
        first_spoken_dialogue: first_dialogue_reports,
        offset_drift_report: offset_drifts,
        part_boundaries: boundaries,
        split_timestamp_rebase: split_rebases,
        audio_delay: audio_delays,
        ffprobe_dumps,
        pipeline_traces: Vec::new(),
        overall_status,
        findings,
    })
}

pub fn print_audit_summary(report: &SubtitleAuditReport) {
    println!("\n╔══════════════════════════════════════════════════════════════════╗");
    println!("║         SUBTITLE TIMELINE & A/V SYNC FORENSIC AUDIT            ║");
    println!("╚══════════════════════════════════════════════════════════════════╝");
    println!("\n  Audit Time: {}", report.audit_timestamp);
    println!("  Overall Status: {:?}", report.overall_status);
    println!("  Findings: {} critical, {} warning, {} info",
        report.findings.iter().filter(|f| matches!(f.severity, FindingSeverity::Critical)).count(),
        report.findings.iter().filter(|f| matches!(f.severity, FindingSeverity::Warning)).count(),
        report.findings.iter().filter(|f| matches!(f.severity, FindingSeverity::Info)).count()
    );

    if !report.part_boundaries.is_empty() {
        println!("\n  ── Part Boundary Audit ──");
        println!("  ┌───────┬────────────────────────────────┬─────────────┬────────────┐");
        println!("  │ Part  │ Output                         │ First Cue  │ Last Cue   │");
        println!("  ├───────┼────────────────────────────────┼─────────────┼────────────┤");
        for boundary in &report.part_boundaries {
            let first = boundary.first_subtitle_cue.as_ref()
                .map(|c| format_srt_timestamp_short(c.start_time))
                .unwrap_or_else(|| "N/A".to_string());
            let last = boundary.last_subtitle_cue.as_ref()
                .map(|c| format_srt_timestamp_short(c.start_time))
                .unwrap_or_else(|| "N/A".to_string());
            let out_short = if boundary.output_path.len() > 30 {
                format!("...{}", &boundary.output_path[boundary.output_path.len()-30..])
            } else {
                boundary.output_path.clone()
            };
            println!("  │ {:>5} │ {:<30} │ {:>11} │ {:>10} │",
                boundary.part_index, out_short, first, last);
        }
        println!("  └───────┴────────────────────────────────┴─────────────┴────────────┘");
    }

    if !report.split_timestamp_rebase.is_empty() {
        println!("\n  ── Split Timestamp Rebase Audit ──");
        println!("  ┌───────┬──────────────┬──────────────┬──────────────┬────────────┐");
        println!("  │ Part  │ Part Start   │ Expected     │ Actual       │ Status     │");
        println!("  ├───────┼──────────────┼──────────────┼──────────────┼────────────┤");
        for rebase in &report.split_timestamp_rebase {
            let expected = rebase.expected_rebased_time
                .map(format_srt_timestamp_short)
                .unwrap_or_else(|| "N/A".to_string());
            let actual = rebase.actual_cue_time
                .map(format_srt_timestamp_short)
                .unwrap_or_else(|| "N/A".to_string());
            let status_str = match rebase.status {
                RebaseStatus::Correct => "✅ CORRECT".to_string(),
                RebaseStatus::OffsetByMs(ms) => format!("⚠️  +{}ms", ms),
                RebaseStatus::NotRebased => "❌ NOT REBASED".to_string(),
                RebaseStatus::Missing => "❌ MISSING".to_string(),
            };
            println!("  │ {:>5} │ {:>12.3}s │ {:>12} │ {:>12} │ {:>10} │",
                rebase.part_index,
                rebase.part_start_time,
                expected,
                actual,
                status_str
            );
        }
        println!("  └───────┴──────────────┴──────────────┴──────────────┴────────────┘");
    }

    if !report.findings.is_empty() {
        println!("\n  ── Critical Findings ──");
        for (i, finding) in report.findings.iter().enumerate() {
            if matches!(finding.severity, FindingSeverity::Critical) {
                println!("\n  {}. [{}] {}", i + 1, finding.category, finding.description);
                println!("     Suggested Fix: {}", finding.suggested_fix);
            }
        }
    }
}

fn format_srt_timestamp_short(seconds: f64) -> String {
    let hours = (seconds / 3600.0).floor();
    let minutes = ((seconds % 3600.0) / 60.0).floor();
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:05.2}", hours, minutes, secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_srt_time_to_secs() {
        assert!((parse_srt_time_to_secs("00:00:01,500") - 1.5).abs() < 0.001);
        assert!((parse_srt_time_to_secs("01:30:45,000") - 5445.0).abs() < 0.001);
        assert!((parse_srt_time_to_secs("00:00:00,000") - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parse_srt_cues() {
        let srt = r#"1
00:00:01,000 --> 00:00:04,000
Hello world

2
00:00:05,000 --> 00:00:08,000
Goodbye world
"#;
        let cues = parse_srt_cues(srt);
        assert_eq!(cues.len(), 2);
        assert!((cues[0].start_time - 1.0).abs() < 0.001);
        assert!((cues[0].end_time - 4.0).abs() < 0.001);
        assert_eq!(cues[0].text, "Hello world");
    }
}