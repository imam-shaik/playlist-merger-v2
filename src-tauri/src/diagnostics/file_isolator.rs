use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitTestResult {
    pub range_start: usize,
    pub range_end: usize,
    pub files: Vec<String>,
    pub success: bool,
    pub error_message: Option<String>,
    pub files_tested: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryIsolationReport {
    pub original_file_count: usize,
    pub iterations: Vec<IsolationIteration>,
    pub bad_file_index: Option<usize>,
    pub bad_file_path: Option<String>,
    pub total_tests: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolationIteration {
    pub iteration: usize,
    pub range_start: usize,
    pub range_end: usize,
    pub files_tested: usize,
    pub result: String,
}

pub struct BinaryFileIsolator {
    ffmpeg_path: String,
    #[allow(dead_code)]
    ffprobe_path: String,
}

impl BinaryFileIsolator {
    pub fn new(ffmpeg_path: &Path, ffprobe_path: &Path) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.to_string_lossy().into_owned(),
            ffprobe_path: ffprobe_path.to_string_lossy().into_owned(),
        }
    }

    pub fn test_range(&self, files: &[String], start: usize, end: usize) -> SplitTestResult {
        let range_files: Vec<String> = files[start..end].iter().cloned().collect();

        let temp_list = std::env::temp_dir().join(format!(
            "isolation_test_{}.txt",
            std::process::id()
        ));

        let list_content: String = range_files.iter()
            .map(|f| format!("file '{}'", f))
            .collect::<Vec<_>>()
            .join("\n");

        if let Err(e) = std::fs::write(&temp_list, &list_content) {
            return SplitTestResult {
                range_start: start,
                range_end: end,
                files: range_files,
                success: false,
                error_message: Some(format!("Failed to write concat list: {}", e)),
                files_tested: end - start,
            };
        }

        let temp_output = std::env::temp_dir().join(format!(
            "isolation_out_{}.mkv",
            std::process::id()
        ));

        let args = [
            "-y",
            "-f", "concat",
            "-safe", "0",
            "-i", temp_list.to_str().unwrap(),
            "-c", "copy",
            "-t", "5",
            temp_output.to_str().unwrap(),
        ];

        let output = Command::new(&self.ffmpeg_path)
            .args(&args)
            .output();

        let _ = std::fs::remove_file(&temp_list);
        let _ = std::fs::remove_file(&temp_output);

        match output {
            Ok(result) => SplitTestResult {
                range_start: start,
                range_end: end,
                files: range_files,
                success: result.status.success(),
                error_message: if result.status.success() {
                    None
                } else {
                    Some(String::from_utf8_lossy(&result.stderr).into_owned())
                },
                files_tested: end - start,
            },
            Err(e) => SplitTestResult {
                range_start: start,
                range_end: end,
                files: range_files,
                success: false,
                error_message: Some(format!("Failed to run ffmpeg: {}", e)),
                files_tested: end - start,
            },
        }
    }

    pub fn binary_isolate(&self, files: &[String]) -> BinaryIsolationReport {
        let mut iterations = Vec::new();
        let mut low = 0;
        let mut high = files.len();
        let mut bad_file_index = None;
        let mut bad_file_path = None;
        let mut iteration = 0;
        let total_files = files.len();

        while low < high {
            let mid = (low + high) / 2;
            if mid == low {
                break;
            }

            let test_range = if mid - low >= 2 { mid } else { low + 2.min(high - low) };

            let result = self.test_range(files, low, test_range);

            let result_str = if result.success {
                format!("PASS ({} files)", result.files_tested)
            } else {
                let preview = result.error_message.as_ref()
                    .map(|s| if s.len() > 100 { format!("{}...", &s[..100]) } else { s.clone() })
                    .unwrap_or_default();
                format!("FAIL: {}", preview)
            };

            iterations.push(IsolationIteration {
                iteration,
                range_start: low,
                range_end: test_range,
                files_tested: test_range - low,
                result: result_str.clone(),
            });

            if result.success {
                low = test_range;
            } else {
                high = test_range;
                if test_range - low <= 1 {
                    bad_file_index = Some(low);
                    bad_file_path = files.get(low).cloned();
                    break;
                }
            }

            iteration += 1;

            if iteration > 20 {
                iterations.push(IsolationIteration {
                    iteration,
                    range_start: low,
                    range_end: high,
                    files_tested: high - low,
                    result: "MAX_ITERATIONS_REACHED".to_string(),
                });
                break;
            }
        }

        if bad_file_index.is_none() && low < files.len() {
            let remaining = files.len() - low;
            let result = self.test_range(files, low, files.len());

            if !result.success {
                if remaining <= 3 {
                    bad_file_index = Some(low);
                    bad_file_path = files.get(low).cloned();
                } else {
                    for i in low..files.len() {
                        let single = self.test_range(files, i, i + 1);
                        if !single.success {
                            bad_file_index = Some(i);
                            bad_file_path = files.get(i).cloned();
                            break;
                        }
                    }
                }
            }
        }

        let total_tests: usize = iterations.iter().map(|i| i.files_tested).sum();

        BinaryIsolationReport {
            original_file_count: total_files,
            iterations,
            bad_file_index,
            bad_file_path,
            total_tests,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIntegrityProbe {
    pub file_path: String,
    pub file_index: usize,
    pub probe_result: ProbeResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub success: bool,
    pub duration_secs: Option<f64>,
    pub video_streams: usize,
    pub audio_streams: usize,
    pub subtitle_streams: usize,
    pub first_video_pts: Option<i64>,
    pub first_audio_pts: Option<i64>,
    pub first_subtitle_pts: Option<i64>,
    pub timebase: Option<String>,
    pub bitrate: Option<String>,
    pub has_warnings: bool,
    pub warnings: Vec<String>,
}

pub fn probe_file_integrity(ffprobe_path: &Path, file_path: &str, file_index: usize) -> FileIntegrityProbe {
    let result = probe_file_internal(ffprobe_path, file_path);

    FileIntegrityProbe {
        file_path: file_path.to_string(),
        file_index,
        probe_result: result,
    }
}

fn probe_file_internal(ffprobe_path: &Path, file_path: &str) -> ProbeResult {
    let args = [
        "-v", "quiet",
        "-print_format", "json",
        "-show_streams",
        "-show_format",
        file_path,
    ];

    let output = match Command::new(ffprobe_path).args(&args).output() {
        Ok(o) => o,
        Err(e) => {
            return ProbeResult {
                success: false,
                duration_secs: None,
                video_streams: 0,
                audio_streams: 0,
                subtitle_streams: 0,
                first_video_pts: None,
                first_audio_pts: None,
                first_subtitle_pts: None,
                timebase: None,
                bitrate: None,
                has_warnings: true,
                warnings: vec![format!("ffprobe failed: {}", e)],
            };
        }
    };

    if !output.status.success() {
        return ProbeResult {
            success: false,
            duration_secs: None,
            video_streams: 0,
            audio_streams: 0,
            subtitle_streams: 0,
            first_video_pts: None,
            first_audio_pts: None,
            first_subtitle_pts: None,
            timebase: None,
            bitrate: None,
            has_warnings: true,
            warnings: vec![String::from_utf8_lossy(&output.stderr).into_owned()],
        };
    }

    let json: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(j) => j,
        Err(e) => {
            return ProbeResult {
                success: false,
                duration_secs: None,
                video_streams: 0,
                audio_streams: 0,
                subtitle_streams: 0,
                first_video_pts: None,
                first_audio_pts: None,
                first_subtitle_pts: None,
                timebase: None,
                bitrate: None,
                has_warnings: true,
                warnings: vec![format!("Invalid JSON: {}", e)],
            };
        }
    };

    let streams = json.get("streams").and_then(|s| s.as_array());
    let video_streams = streams.as_ref().map(|s| s.iter().filter(|x| x.get("codec_type") == Some(&serde_json::json!("video"))).count()).unwrap_or(0);
    let audio_streams = streams.as_ref().map(|s| s.iter().filter(|x| x.get("codec_type") == Some(&serde_json::json!("audio"))).count()).unwrap_or(0);
    let subtitle_streams = streams.as_ref().map(|s| s.iter().filter(|x| x.get("codec_type") == Some(&serde_json::json!("subtitle"))).count()).unwrap_or(0);

    let duration_secs = json.get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse::<f64>().ok());

    let bitrate = json.get("format")
        .and_then(|f| f.get("bit_rate"))
        .and_then(|b| b.as_str())
        .map(|s| s.to_string());

    let timebase = streams.and_then(|s| s.iter()
        .find(|x| x.get("codec_type") == Some(&serde_json::json!("video"))))
        .and_then(|v| v.get("time_base"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    let first_pts_by_type = |codec_type: &str| -> Option<i64> {
        let args = [
            "-v", "quiet",
            "-print_format", "json",
            "-show_packets",
            "-select_streams", codec_type,
            "-show_entries", "packet=pts",
            "-of", "json",
            file_path,
        ];
        let output = Command::new(ffprobe_path).args(&args).output().ok()?;
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
        let packets = json.get("packets")?.as_array()?;
        let first = packets.first()?;
        first.get("pts")?.as_i64()
    };

    let first_video_pts = first_pts_by_type("video");
    let first_audio_pts = first_pts_by_type("audio");
    let first_subtitle_pts = first_pts_by_type("subtitle");

    let mut warnings = Vec::new();

    if duration_secs.map(|d| d <= 0.0).unwrap_or(false) {
        warnings.push("Zero or negative duration".to_string());
    }
    if first_video_pts.map(|p| p == i64::MIN).unwrap_or(false) {
        warnings.push("First video PTS is NOPTS".to_string());
    }
    if timebase.as_ref().map(|t| t == "0/1" || t == "1/0").unwrap_or(false) {
        warnings.push("Invalid timebase".to_string());
    }

    ProbeResult {
        success: true,
        duration_secs,
        video_streams,
        audio_streams,
        subtitle_streams,
        first_video_pts,
        first_audio_pts,
        first_subtitle_pts,
        timebase,
        bitrate,
        has_warnings: !warnings.is_empty(),
        warnings,
    }
}

pub fn scan_playlist_for_anomalies(
    ffprobe_path: &Path,
    files: &[String],
) -> Vec<FileIntegrityProbe> {
    files.iter()
        .enumerate()
        .map(|(i, f)| probe_file_integrity(ffprobe_path, f, i))
        .collect()
}

impl std::fmt::Display for BinaryIsolationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║              BINARY ISOLATION REPORT                                     ║")?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════════════╝")?;
        writeln!(f)?;
        writeln!(f, "Original playlist: {} files", self.original_file_count)?;
        writeln!(f, "Total tests performed: {}", self.total_tests)?;
        writeln!(f)?;

        writeln!(f, "ITERATIONS:")?;
        writeln!(f, "  {:>3} | {:>5} → {:>5} | {:>5} files | {:<20}",
            "Iter", "Start", "End", "Count", "Result")?;
        writeln!(f, "  {}", "-".repeat(65))?;
        for iter in &self.iterations {
            writeln!(f, "  {:>3} | {:>5} → {:>5} | {:>5} files | {}",
                iter.iteration,
                iter.range_start,
                iter.range_end,
                iter.files_tested,
                iter.result)?;
        }
        writeln!(f)?;

        if let Some(idx) = self.bad_file_index {
            writeln!(f, "BAD FILE FOUND:")?;
            writeln!(f, "  Index: {}", idx)?;
            if let Some(ref path) = self.bad_file_path {
                writeln!(f, "  Path: {}", path)?;
            }
        } else {
            writeln!(f, "No bad file identified")?;
        }

        Ok(())
    }
}

impl std::fmt::Display for FileIntegrityProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "File {:>3} | {} | {} streams | dur={:.1}s | pts_v={:?} | pts_a={:?}",
            self.file_index,
            self.probe_result.timebase.as_ref().unwrap_or(&"??/??".to_string()),
            self.probe_result.video_streams,
            self.probe_result.duration_secs.unwrap_or(0.0),
            self.probe_result.first_video_pts,
            self.probe_result.first_audio_pts)?;

        if self.probe_result.has_warnings {
            for w in &self.probe_result.warnings {
                writeln!(f, "  ⚠️  {}", w)?;
            }
        }

        Ok(())
    }
}