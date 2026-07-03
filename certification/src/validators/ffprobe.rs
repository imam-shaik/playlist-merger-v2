use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FfprobeStream {
    pub index: u32,
    pub codec_type: String,
    pub codec_name: String,
    pub codec_long_name: Option<String>,
    pub profile: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub channels: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_rate: Option<u64>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub channel_layout: Option<String>,
    pub bits_per_raw_sample: Option<u32>,
    pub r_frame_rate: Option<String>,
    pub avg_frame_rate: Option<String>,
    pub duration: Option<f64>,
    pub start_time: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FfprobeFormat {
    pub filename: String,
    pub format_name: String,
    pub format_long_name: Option<String>,
    pub duration: Option<f64>,
    pub size: Option<u64>,
    pub bit_rate: Option<u64>,
    pub probe_score: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FfprobeOutput {
    pub streams: Vec<FfprobeStream>,
    pub format: FfprobeFormat,
    pub raw_json: String,
}

impl FfprobeOutput {
    pub fn from_json(json: &str) -> Result<Self> {
        let parsed: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| anyhow!("Failed to parse FFprobe JSON: {}", e))?;

        let streams: Vec<FfprobeStream> = parsed["streams"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|s| FfprobeStream {
                index: s["index"].as_u64().unwrap_or(0) as u32,
                codec_type: s["codec_type"].as_str().unwrap_or("").to_string(),
                codec_name: s["codec_name"].as_str().unwrap_or("").to_string(),
                codec_long_name: s["codec_long_name"].as_str().map(String::from),
                profile: s["profile"].as_str().map(String::from),
                width: s["width"].as_u64().map(|v| v as u32),
                height: s["height"].as_u64().map(|v| v as u32),
                channels: s["channels"].as_u64().map(|v| v as u32),
                sample_rate: s["sample_rate"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
                    .or_else(|| s["sample_rate"].as_u64().map(|v| v as u32)),
                bit_rate: s["bit_rate"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
                    .or_else(|| s["bit_rate"].as_u64()),
                language: s["tags"]["language"]
                    .as_str()
                    .or_else(|| s["tags"]["LANGUAGE"].as_str())
                    .map(String::from),
                title: s["tags"]["title"].as_str().map(String::from),
                channel_layout: s["channel_layout"].as_str().map(String::from),
                bits_per_raw_sample: s["bits_per_raw_sample"]
                    .as_u64()
                    .map(|v| v as u32),
                r_frame_rate: s["r_frame_rate"].as_str().map(String::from),
                avg_frame_rate: s["avg_frame_rate"].as_str().map(String::from),
                duration: s["duration"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
                    .or_else(|| s["duration"].as_f64()),
                start_time: s["start_time"]
                    .as_str()
                    .and_then(|v| v.parse().ok())
                    .or_else(|| s["start_time"].as_f64()),
            })
            .collect();

        let format_val = parsed.get("format").cloned().unwrap_or(serde_json::Value::Null);
        let format_obj = FfprobeFormat {
            filename: format_val["filename"].as_str().unwrap_or("").to_string(),
            format_name: format_val["format_name"].as_str().unwrap_or("").to_string(),
            format_long_name: format_val["format_long_name"].as_str().map(String::from),
            duration: format_val["duration"]
                .as_str()
                .and_then(|v| v.parse().ok())
                .or_else(|| format_val["duration"].as_f64()),
            size: format_val["size"].as_u64(),
            bit_rate: format_val["bit_rate"]
                .as_str()
                .and_then(|v| v.parse().ok())
                .or_else(|| format_val["bit_rate"].as_u64()),
            probe_score: format_val["probe_score"].as_u64().map(|v| v as u32),
        };

        Ok(Self {
            streams,
            format: format_obj,
            raw_json: json.to_string(),
        })
    }

    pub fn video_streams(&self) -> Vec<&FfprobeStream> {
        self.streams.iter().filter(|s| s.codec_type == "video").collect()
    }

    pub fn audio_streams(&self) -> Vec<&FfprobeStream> {
        self.streams.iter().filter(|s| s.codec_type == "audio").collect()
    }

    pub fn subtitle_streams(&self) -> Vec<&FfprobeStream> {
        self.streams.iter().filter(|s| s.codec_type == "subtitle").collect()
    }

    pub fn video_count(&self) -> usize {
        self.video_streams().len()
    }

    pub fn audio_count(&self) -> usize {
        self.audio_streams().len()
    }

    pub fn subtitle_count(&self) -> usize {
        self.subtitle_streams().len()
    }

    pub fn primary_video(&self) -> Option<&FfprobeStream> {
        self.video_streams().first().copied()
    }

    pub fn primary_audio(&self) -> Option<&FfprobeStream> {
        self.audio_streams().first().copied()
    }

    pub fn is_vfr(&self) -> bool {
        for v in self.video_streams() {
            if let (Some(rfr), Some(afr)) = (&v.r_frame_rate, &v.avg_frame_rate) {
                let rfr_parts: Vec<&str> = rfr.split('/').collect();
                let afr_parts: Vec<&str> = afr.split('/').collect();
                if rfr_parts.len() == 2 && afr_parts.len() == 2 {
                    if let (Ok(r_num), Ok(r_den), Ok(a_num), Ok(a_den)) = (
                        rfr_parts[0].parse::<f64>(),
                        rfr_parts[1].parse::<f64>(),
                        afr_parts[0].parse::<f64>(),
                        afr_parts[1].parse::<f64>(),
                    ) {
                        if (r_num / r_den - a_num / a_den).abs() > 0.1 {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    pub fn total_duration(&self) -> Option<f64> {
        self.format.duration
    }

    pub fn total_size(&self) -> Option<u64> {
        self.format.size
    }

    pub fn container_format(&self) -> &str {
        &self.format.format_name
    }

    pub fn has_audio_codec(&self, codec: &str) -> bool {
        self.audio_streams()
            .into_iter()
            .any(|s| s.codec_name.to_lowercase() == codec.to_lowercase())
    }

    pub fn has_video_codec(&self, codec: &str) -> bool {
        self.video_streams()
            .into_iter()
            .any(|s| s.codec_name.to_lowercase() == codec.to_lowercase())
    }

    pub fn unique_audio_channels(&self) -> Vec<u32> {
        let mut channels: Vec<u32> = self
            .audio_streams()
            .into_iter()
            .filter_map(|s| s.channels)
            .collect();
        channels.sort();
        channels.dedup();
        channels
    }

    pub fn unique_audio_sample_rates(&self) -> Vec<u32> {
        let mut rates: Vec<_> = self
            .audio_streams()
            .into_iter()
            .filter_map(|s| s.sample_rate)
            .collect();
        rates.sort();
        rates.dedup();
        rates
    }
}

pub struct FfprobeValidator {
    ffprobe_path: PathBuf,
}

impl FfprobeValidator {
    pub fn new() -> Self {
        let ffprobe_path = which_ffprobe();
        Self { ffprobe_path }
    }

    pub fn probe(&self, path: &Path) -> Result<FfprobeOutput> {
        if !path.exists() {
            return Err(anyhow!("File does not exist: {:?}", path));
        }

        let output = Command::new(&self.ffprobe_path)
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                "-show_streams",
                path.to_str().unwrap_or(""),
            ])
            .output()?;

        if !output.status.success() {
            return Err(anyhow!(
                "ffprobe failed with exit code {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let json = String::from_utf8_lossy(&output.stdout);
        FfprobeOutput::from_json(&json)
    }

    pub fn verify_video_exists(&self, path: &Path) -> Result<bool> {
        let probe = self.probe(path)?;
        Ok(probe.video_count() > 0)
    }

    pub fn verify_audio_exists(&self, path: &Path) -> Result<bool> {
        let probe = self.probe(path)?;
        Ok(probe.audio_count() > 0)
    }

    pub fn verify_duration(&self, path: &Path, expected_secs: f64, tolerance_pct: f64) -> Result<bool> {
        let probe = self.probe(path)?;
        let actual = probe.total_duration().unwrap_or(0.0);
        let diff = (expected_secs - actual).abs();
        let threshold = expected_secs * tolerance_pct / 100.0;
        Ok(diff <= threshold)
    }

    pub fn verify_container(&self, path: &Path, expected: &[&str]) -> Result<bool> {
        let probe = self.probe(path)?;
        let format = probe.container_format();
        Ok(expected.iter().any(|e| format.contains(e)))
    }

    pub fn verify_video_codec(&self, path: &Path, expected: &[&str]) -> Result<bool> {
        let probe = self.probe(path)?;
        Ok(expected.iter().any(|e| probe.has_video_codec(e)))
    }

    pub fn verify_audio_codec(&self, path: &Path, expected: &[&str]) -> Result<bool> {
        let probe = self.probe(path)?;
        Ok(expected.iter().any(|e| probe.has_audio_codec(e)))
    }

    pub fn verify_audio_stream_count(&self, path: &Path, min_count: usize) -> Result<bool> {
        let probe = self.probe(path)?;
        Ok(probe.audio_count() >= min_count)
    }

    pub fn verify_subtitle_stream_count(&self, path: &Path, min_count: usize) -> Result<bool> {
        let probe = self.probe(path)?;
        Ok(probe.subtitle_count() >= min_count)
    }

    pub fn verify_no_corruption(&self, path: &Path) -> Result<ValidationResult> {
        if !path.exists() {
            return Ok(ValidationResult {
                passed: false,
                error: Some("File does not exist".to_string()),
                details: None,
            });
        }

        let probe = self.probe(path)?;

        // Check if file has valid duration
        if probe.total_duration().unwrap_or(0.0) <= 0.0 {
            return Ok(ValidationResult {
                passed: false,
                error: Some("Invalid duration (0 or negative)".to_string()),
                details: Some(vec![format!("Duration: {:?}", probe.total_duration())]),
            });
        }

        // Check if file has video
        if probe.video_count() == 0 {
            return Ok(ValidationResult {
                passed: false,
                error: Some("No video streams found".to_string()),
                details: None,
            });
        }

        Ok(ValidationResult {
            passed: true,
            error: None,
            details: None,
        })
    }

    pub fn full_validation(&self, path: &Path) -> Result<FfprobeValidationReport> {
        let probe = self.probe(path)?;
        let mut checks = Vec::new();

        // Duration check
        checks.push(ValidationCheck {
            name: "duration_positive".to_string(),
            passed: probe.total_duration().unwrap_or(0.0) > 0.0,
            detail: format!("Duration: {:?}s", probe.total_duration()),
        });

        // Video check
        checks.push(ValidationCheck {
            name: "has_video".to_string(),
            passed: probe.video_count() > 0,
            detail: format!("Video streams: {}", probe.video_count()),
        });

        // Audio check
        checks.push(ValidationCheck {
            name: "has_audio".to_string(),
            passed: probe.audio_count() > 0,
            detail: format!("Audio streams: {}", probe.audio_count()),
        });

        // Container format
        checks.push(ValidationCheck {
            name: "valid_container".to_string(),
            passed: !probe.container_format().is_empty(),
            detail: format!("Container: {}", probe.container_format()),
        });

        // Codec checks
        for v in probe.video_streams() {
            checks.push(ValidationCheck {
                name: format!("video_codec_{}", v.codec_name),
                passed: !v.codec_name.is_empty(),
                detail: format!("Video codec: {}", v.codec_name),
            });
        }

        // Audio channel diversity
        let channels = probe.unique_audio_channels();
        if channels.len() > 1 {
            checks.push(ValidationCheck {
                name: "audio_channel_mismatch".to_string(),
                passed: false,
                detail: format!(
                    "Multiple channel configs: {:?}",
                    channels
                ),
            });
        }

        // Audio sample rate diversity
        let rates = probe.unique_audio_sample_rates();
        if rates.len() > 1 {
            checks.push(ValidationCheck {
                name: "audio_sample_rate_mismatch".to_string(),
                passed: false,
                detail: format!("Multiple sample rates: {:?}", rates),
            });
        }

        let passed_count = checks.iter().filter(|c| c.passed).count();
        let total_count = checks.len();

        Ok(FfprobeValidationReport {
            file: path.to_path_buf(),
            total_checks: total_count,
            passed_checks: passed_count,
            checks,
            ffprobe_output: probe,
        })
    }
}

impl Default for FfprobeValidator {
    fn default() -> Self {
        Self::new()
    }
}

fn which_ffprobe() -> PathBuf {
    // 1. Try production find_ffprobe (searches bundled binaries + common system paths)
    let prod_path = playlist_merger_lib::certification_api::find_ffprobe(None::<&str>);
    if let Ok(path) = prod_path {
        // Verify it actually works
        if Command::new(&path).arg("-version").output().map(|o| o.status.success()).unwrap_or(false) {
            return path;
        }
    }

    // 2. Try bundled binaries relative to certification project dir
    let bundled_candidates = [
        "../src-tauri/binaries/ffprobe.exe",
        "../../src-tauri/binaries/ffprobe.exe",
    ];
    for candidate in &bundled_candidates {
        let candidate_path = std::env::current_dir()
            .unwrap_or_default()
            .join(candidate);
        if candidate_path.exists() {
            if Command::new(&candidate_path).arg("-version").output().map(|o| o.status.success()).unwrap_or(false) {
                return candidate_path;
            }
        }
    }

    // 3. Hardcoded Windows paths
    let candidates = [
        "ffprobe",
        "ffprobe.exe",
        "C:\\ffmpeg\\bin\\ffprobe.exe",
        "C:\\Program Files\\ffmpeg\\bin\\ffprobe.exe",
    ];

    for candidate in &candidates {
        if let Ok(output) = Command::new(candidate).arg("-version").output() {
            if output.status.success() {
                return PathBuf::from(candidate);
            }
        }
    }

    // Fallback to ffprobe in PATH
    PathBuf::from("ffprobe")
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub passed: bool,
    pub error: Option<String>,
    pub details: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ValidationCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug)]
pub struct FfprobeValidationReport {
    pub file: PathBuf,
    pub total_checks: usize,
    pub passed_checks: usize,
    pub checks: Vec<ValidationCheck>,
    pub ffprobe_output: FfprobeOutput,
}

impl FfprobeValidationReport {
    pub fn pass_rate(&self) -> f64 {
        if self.total_checks == 0 {
            return 100.0;
        }
        self.passed_checks as f64 / self.total_checks as f64 * 100.0
    }

    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }
}