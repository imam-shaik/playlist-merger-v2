//! Post-Merge Output Verification
//!
//! Verifies that the merged output matches expected properties.
//! This does NOT change merge behavior - only detects problems.

use std::path::Path;
use std::process::Command;
use serde::{Serialize, Deserialize};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct MergeVerificationConfig {
    pub expected_video_streams: Option<usize>,
    pub expected_audio_streams: Option<usize>,
    pub expected_subtitle_streams: Option<usize>,
    pub expected_chapters: Option<usize>,
    pub expected_attachment_count: Option<usize>,
    pub preserve_subtitles: bool,
    pub preserve_chapters: bool,
    pub preserve_attachments: bool,
    pub ffprobe_path: Option<std::path::PathBuf>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeVerificationReport {
    pub output_path: String,
    pub video_streams: usize,
    pub audio_streams: usize,
    pub subtitle_streams: usize,
    pub chapter_count: usize,
    pub attachment_count: usize,
    pub duration_secs: Option<f64>,
    pub has_drift: bool,
    pub drift_percent: Option<f64>,
    pub errors: Vec<VerificationError>,
    pub warnings: Vec<VerificationWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationError {
    pub property: String,
    pub expected: String,
    pub actual: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationWarning {
    pub property: String,
    pub message: String,
}

impl MergeVerificationReport {
    pub fn is_healthy(&self) -> bool {
        self.errors.is_empty()
    }

    #[allow(dead_code)]
    pub fn has_critical_errors(&self) -> bool {
        self.errors.iter().any(|e| e.severity == "critical")
    }

    #[allow(dead_code)]
    pub fn summary(&self) -> String {
        if self.is_healthy() {
            format!(
                "✅ Verified: {} video, {} audio, {} subtitle, {} chapters",
                self.video_streams,
                self.audio_streams,
                self.subtitle_streams,
                self.chapter_count
            )
        } else {
            format!(
                "❌ {} errors: {} video, {} audio, {} subtitle",
                self.errors.len(),
                self.video_streams,
                self.audio_streams,
                self.subtitle_streams
            )
        }
    }
}

/// Raw probe result for a merged output file.
#[allow(dead_code)]
struct ProbeOutput {
    video_streams: usize,
    audio_streams: usize,
    subtitle_streams: usize,
    chapter_count: usize,
    attachment_count: usize,
    duration: Option<f64>,
}

/// Probe a merged output file using ffprobe.
/// Returns stream counts and metadata needed for verification.
#[allow(dead_code)]
fn probe_output_file(ffprobe_path: &Path, output_path: &Path) -> Result<ProbeOutput, String> {
    use crate::ffmpeg::probe::probe_file;

    let media_info = probe_file(ffprobe_path, output_path)
        .map_err(|e| format!("Failed to probe output file '{}': {}", output_path.display(), e))?;

    // Get chapter count via separate ffprobe call (not included in MediaInfo)
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = Command::new(ffprobe_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let chapters_output = cmd
        .args(["-v", "quiet", "-print_format", "json", "-show_chapters"])
        .arg(output_path)
        .output()
        .map_err(|e| format!("Failed to run ffprobe for chapters: {}", e))?;

    let chapters_json: serde_json::Value = serde_json::from_slice(&chapters_output.stdout)
        .map_err(|e| format!("Failed to parse ffprobe JSON for chapters: {}", e))?;

    let chapter_count = chapters_json
        .get("chapters")
        .and_then(|c| c.as_array())
        .map(|c| c.len())
        .unwrap_or(0);

    // Count attachment streams via ffprobe -show_streams
    let mut attachment_count = 0usize;
    let mut stream_cmd = Command::new(ffprobe_path);
    #[cfg(windows)]
    stream_cmd.creation_flags(CREATE_NO_WINDOW);
    if let Ok(stream_output) = stream_cmd
        .args(["-v", "quiet", "-print_format", "json", "-show_streams", "-show_entries", "stream=codec_type"])
        .arg(output_path)
        .output()
    {
        if let Ok(stream_json) = serde_json::from_slice::<serde_json::Value>(&stream_output.stdout) {
            if let Some(streams) = stream_json.get("streams").and_then(|s| s.as_array()) {
                attachment_count = streams.iter()
                    .filter(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("attachment"))
                    .count();
            }
        }
    }

    Ok(ProbeOutput {
        video_streams: media_info.video_streams.len(),
        audio_streams: media_info.audio_streams.len(),
        subtitle_streams: media_info.subtitle_streams.len(),
        chapter_count,
        attachment_count,
        duration: if media_info.duration > 0.0 {
            Some(media_info.duration)
        } else {
            None
        },
    })
}

#[allow(dead_code)]
pub fn verify_merge_output(
    output_path: &Path,
    config: &MergeVerificationConfig,
) -> Result<MergeVerificationReport, String> {
    let ffprobe_path = config
        .ffprobe_path
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("ffprobe"));

    let probe_data = probe_output_file(&ffprobe_path, output_path)?;

    let mut report = MergeVerificationReport {
        output_path: output_path.to_string_lossy().into_owned(),
        video_streams: probe_data.video_streams,
        audio_streams: probe_data.audio_streams,
        subtitle_streams: probe_data.subtitle_streams,
        chapter_count: probe_data.chapter_count,
        attachment_count: probe_data.attachment_count,
        duration_secs: probe_data.duration,
        has_drift: false,
        drift_percent: None,
        errors: Vec::new(),
        warnings: Vec::new(),
    };

    if let Some(expected) = config.expected_video_streams {
        if probe_data.video_streams != expected {
            report.errors.push(VerificationError {
                property: "video_streams".into(),
                expected: expected.to_string(),
                actual: probe_data.video_streams.to_string(),
                severity: if expected > 0 && probe_data.video_streams == 0 {
                    "critical".into()
                } else {
                    "warning".into()
                },
            });
        }
    }

    if let Some(expected) = config.expected_audio_streams {
        if probe_data.audio_streams != expected {
            report.errors.push(VerificationError {
                property: "audio_streams".into(),
                expected: expected.to_string(),
                actual: probe_data.audio_streams.to_string(),
                severity: if expected > 0 && probe_data.audio_streams == 0 {
                    "critical".into()
                } else {
                    "warning".into()
                },
            });
        }
    }

    if config.preserve_subtitles {
        if let Some(expected) = config.expected_subtitle_streams {
            if probe_data.subtitle_streams != expected {
                report.errors.push(VerificationError {
                    property: "subtitle_streams".into(),
                    expected: expected.to_string(),
                    actual: probe_data.subtitle_streams.to_string(),
                    severity: if expected > 0 && probe_data.subtitle_streams == 0 {
                        "critical".into()
                    } else {
                        "warning".into()
                    },
                });
            }
        }
    }

    if config.preserve_chapters {
        if let Some(expected) = config.expected_chapters {
            if probe_data.chapter_count != expected {
                report.errors.push(VerificationError {
                    property: "chapters".into(),
                    expected: expected.to_string(),
                    actual: probe_data.chapter_count.to_string(),
                    severity: "warning".into(),
                });
            }
        }
    }

    if report.video_streams == 0 {
        report.errors.push(VerificationError {
            property: "video_streams".into(),
            expected: ">= 1".into(),
            actual: "0".into(),
            severity: "critical".into(),
        });
    }

    if report.audio_streams == 0 {
        report.warnings.push(VerificationWarning {
            property: "audio_streams".into(),
            message: "No audio streams found".into(),
        });
    }

    if config.preserve_subtitles && config.expected_subtitle_streams.unwrap_or(0) > 0 {
        if report.subtitle_streams == 0 {
            report.warnings.push(VerificationWarning {
                property: "subtitle_streams".into(),
                message: "Subtitles expected but none found".into(),
            });
        }
    }

    if config.preserve_attachments {
        if let Some(expected) = config.expected_attachment_count {
            if probe_data.attachment_count != expected {
                report.errors.push(VerificationError {
                    property: "attachments".into(),
                    expected: expected.to_string(),
                    actual: probe_data.attachment_count.to_string(),
                    severity: "warning".into(),
                });
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_report_healthy() {
        let report = MergeVerificationReport {
            output_path: "test.mkv".into(),
            video_streams: 1,
            audio_streams: 1,
            subtitle_streams: 2,
            chapter_count: 0,
            attachment_count: 0,
            duration_secs: Some(120.0),
            has_drift: false,
            drift_percent: None,
            errors: vec![],
            warnings: vec![],
        };
        assert!(report.is_healthy());
        assert!(!report.has_critical_errors());
    }

    #[test]
    fn test_verification_report_critical_error() {
        let report = MergeVerificationReport {
            output_path: "test.mkv".into(),
            video_streams: 0,
            audio_streams: 0,
            subtitle_streams: 0,
            chapter_count: 0,
            attachment_count: 0,
            duration_secs: None,
            has_drift: false,
            drift_percent: None,
            errors: vec![VerificationError {
                property: "video_streams".into(),
                expected: "1".into(),
                actual: "0".into(),
                severity: "critical".into(),
            }],
            warnings: vec![],
        };
        assert!(!report.is_healthy());
        assert!(report.has_critical_errors());
    }
}