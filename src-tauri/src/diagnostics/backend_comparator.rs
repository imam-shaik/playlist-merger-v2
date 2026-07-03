use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendComparisonResult {
    pub job_id: String,
    pub playlist: Vec<String>,
    pub ffmpeg_result: BackendRunResult,
    pub mkvmerge_result: BackendRunResult,
    pub comparison: BackendComparison,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendRunResult {
    pub backend: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub output_path: Option<String>,
    pub output_size_bytes: Option<u64>,
    pub error_message: Option<String>,
    pub logs: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendComparison {
    pub both_failed: bool,
    pub both_succeeded: bool,
    pub ffmpeg_failed_mkvmerge_succeeded: bool,
    pub mkvmerge_failed_ffmpeg_succeeded: bool,
    pub output_size_match: bool,
    pub execution_time_ratio: Option<f64>,
    pub key_differences: Vec<String>,
}

pub struct BackendComparator {
    ffmpeg_path: PathBuf,
    mkvmerge_path: PathBuf,
    #[allow(dead_code)]
    ffprobe_path: PathBuf,
}

impl BackendComparator {
    pub fn new(ffmpeg_path: &Path, mkvmerge_path: &Path, ffprobe_path: &Path) -> Self {
        Self {
            ffmpeg_path: ffmpeg_path.to_path_buf(),
            mkvmerge_path: mkvmerge_path.to_path_buf(),
            ffprobe_path: ffprobe_path.to_path_buf(),
        }
    }

    pub fn compare(&self, job_id: &str, concat_list: &Path, output_dir: &Path) -> BackendComparisonResult {
        let playlist = self.read_concat_list(concat_list);

        let ffmpeg_result = self.run_ffmpeg_concat(job_id, concat_list, output_dir);
        let mkvmerge_result = self.run_mkvmerge_concat(job_id, &playlist, output_dir);

        let comparison = self.compare_results(&ffmpeg_result, &mkvmerge_result);

        BackendComparisonResult {
            job_id: job_id.to_string(),
            playlist,
            ffmpeg_result,
            mkvmerge_result,
            comparison,
        }
    }

    fn read_concat_list(&self, path: &Path) -> Vec<String> {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        content.lines()
            .filter(|l| l.starts_with("file '") || l.starts_with("file \""))
            .filter_map(|l| {
                let stripped = l.trim()
                    .trim_start_matches("file '")
                    .trim_start_matches("file \"")
                    .trim_end_matches('\'')
                    .trim_end_matches('"');
                Some(stripped.to_string())
            })
            .collect()
    }

    fn run_ffmpeg_concat(&self, job_id: &str, concat_list: &Path, output_dir: &Path) -> BackendRunResult {
        let output_path = output_dir.join(format!("ffmpeg_output_{}.mkv", job_id));
        let start = std::time::Instant::now();

        let args = [
            "-y",
            "-f", "concat",
            "-safe", "0",
            "-i", concat_list.to_str().unwrap(),
            "-c", "copy",
            output_path.to_str().unwrap(),
        ];

        let output = Command::new(&self.ffmpeg_path)
            .args(&args)
            .output();

        let duration_ms = start.elapsed().as_millis() as u64;

        match output {
            Ok(result) => {
                let success = result.status.success();
                let output_size = if output_path.exists() {
                    std::fs::metadata(&output_path).ok().map(|m| m.len())
                } else {
                    None
                };

                BackendRunResult {
                    backend: "ffmpeg".to_string(),
                    success,
                    exit_code: result.status.code(),
                    duration_ms,
                    output_path: if output_path.exists() { Some(output_path.to_string_lossy().into_owned()) } else { None },
                    output_size_bytes: output_size,
                    error_message: if !success { Some(String::from_utf8_lossy(&result.stderr).into_owned()) } else { None },
                    logs: String::from_utf8_lossy(&result.stderr).into_owned(),
                }
            }
            Err(e) => BackendRunResult {
                backend: "ffmpeg".to_string(),
                success: false,
                exit_code: None,
                duration_ms,
                output_path: None,
                output_size_bytes: None,
                error_message: Some(e.to_string()),
                logs: String::new(),
            },
        }
    }

    fn run_mkvmerge_concat(&self, job_id: &str, files: &[String], output_dir: &Path) -> BackendRunResult {
        let output_path = output_dir.join(format!("mkvmerge_output_{}.mkv", job_id));
        let start = std::time::Instant::now();

        let mut args = vec!["-o", output_path.to_str().unwrap()];
        for f in files {
            args.push(f);
        }

        let output = Command::new(&self.mkvmerge_path)
            .args(&args)
            .output();

        let duration_ms = start.elapsed().as_millis() as u64;

        match output {
            Ok(result) => {
                let success = result.status.success();
                let output_size = if output_path.exists() {
                    std::fs::metadata(&output_path).ok().map(|m| m.len())
                } else {
                    None
                };

                BackendRunResult {
                    backend: "mkvmerge".to_string(),
                    success,
                    exit_code: result.status.code(),
                    duration_ms,
                    output_path: if output_path.exists() { Some(output_path.to_string_lossy().into_owned()) } else { None },
                    output_size_bytes: output_size,
                    error_message: if !success { Some(String::from_utf8_lossy(&result.stderr).into_owned()) } else { None },
                    logs: String::from_utf8_lossy(&result.stderr).into_owned(),
                }
            }
            Err(e) => BackendRunResult {
                backend: "mkvmerge".to_string(),
                success: false,
                exit_code: None,
                duration_ms,
                output_path: None,
                output_size_bytes: None,
                error_message: Some(e.to_string()),
                logs: String::new(),
            },
        }
    }

    fn compare_results(&self, ffmpeg: &BackendRunResult, mkvmerge: &BackendRunResult) -> BackendComparison {
        let both_failed = !ffmpeg.success && !mkvmerge.success;
        let both_succeeded = ffmpeg.success && mkvmerge.success;
        let ffmpeg_failed_mkvmerge_succeeded = !ffmpeg.success && mkvmerge.success;
        let mkvmerge_failed_ffmpeg_succeeded = ffmpeg.success && !mkvmerge.success;

        let output_size_match = match (ffmpeg.output_size_bytes, mkvmerge.output_size_bytes) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };

        let execution_time_ratio = if ffmpeg.duration_ms > 0 && mkvmerge.duration_ms > 0 {
            Some(mkvmerge.duration_ms as f64 / ffmpeg.duration_ms as f64)
        } else {
            None
        };

        let mut key_differences = Vec::new();

        if ffmpeg_failed_mkvmerge_succeeded {
            key_differences.push("FFmpeg failed but mkvmerge succeeded — indicates FFmpeg-specific runtime issue".to_string());
        }
        if mkvmerge_failed_ffmpeg_succeeded {
            key_differences.push("mkvmerge failed but FFmpeg succeeded — indicates media incompatibility with mkvmerge".to_string());
        }
        if !output_size_match && both_succeeded {
            key_differences.push(format!("Output size mismatch: ffmpeg={:?} vs mkvmerge={:?}",
                ffmpeg.output_size_bytes, mkvmerge.output_size_bytes));
        }

        BackendComparison {
            both_failed,
            both_succeeded,
            ffmpeg_failed_mkvmerge_succeeded,
            mkvmerge_failed_ffmpeg_succeeded,
            output_size_match,
            execution_time_ratio,
            key_differences,
        }
    }
}

impl std::fmt::Display for BackendComparisonResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║                  BACKEND COMPARISON REPORT                               ║")?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════════════╝")?;
        writeln!(f)?;
        writeln!(f, "Job ID: {}", self.job_id)?;
        writeln!(f, "Files in playlist: {}", self.playlist.len())?;
        writeln!(f)?;

        writeln!(f, "FFMPEG:")?;
        writeln!(f, "  Status: {}", if self.ffmpeg_result.success { "✅ SUCCESS" } else { "❌ FAILED" })?;
        writeln!(f, "  Duration: {}ms ({:.1}s)", self.ffmpeg_result.duration_ms, self.ffmpeg_result.duration_ms as f64 / 1000.0)?;
        if let Some(code) = self.ffmpeg_result.exit_code {
            writeln!(f, "  Exit Code: {}", code)?;
        }
        if let Some(ref err) = self.ffmpeg_result.error_message {
            let preview = if err.len() > 200 { format!("{}...", &err[..200]) } else { err.clone() };
            writeln!(f, "  Error: {}", preview)?;
        }
        writeln!(f)?;

        writeln!(f, "MKVMERGE:")?;
        writeln!(f, "  Status: {}", if self.mkvmerge_result.success { "✅ SUCCESS" } else { "❌ FAILED" })?;
        writeln!(f, "  Duration: {}ms ({:.1}s)", self.mkvmerge_result.duration_ms, self.mkvmerge_result.duration_ms as f64 / 1000.0)?;
        if let Some(code) = self.mkvmerge_result.exit_code {
            writeln!(f, "  Exit Code: {}", code)?;
        }
        if let Some(ref err) = self.mkvmerge_result.error_message {
            let preview = if err.len() > 200 { format!("{}...", &err[..200]) } else { err.clone() };
            writeln!(f, "  Error: {}", preview)?;
        }
        writeln!(f)?;

        writeln!(f, "COMPARISON:")?;
        let c = &self.comparison;
        if c.both_succeeded {
            writeln!(f, "  ✅ Both succeeded")?;
        } else if c.both_failed {
            writeln!(f, "  ❌ Both failed")?;
        } else if c.ffmpeg_failed_mkvmerge_succeeded {
            writeln!(f, "  🔴 FFmpeg FAILED, mkvmerge SUCCEEDED")?;
            writeln!(f, "     → This is the observed production behavior")?;
            writeln!(f, "     → Problem is FFmpeg-specific, not media")?;
        } else if c.mkvmerge_failed_ffmpeg_succeeded {
            writeln!(f, "  🟡 mkvmerge FAILED, FFmpeg succeeded")?;
            writeln!(f, "     → Problem is mkvmerge-specific, not FFmpeg")?;
        }

        if !c.key_differences.is_empty() {
            writeln!(f)?;
            writeln!(f, "KEY DIFFERENCES:")?;
            for diff in &c.key_differences {
                writeln!(f, "  • {}", diff)?;
            }
        }

        Ok(())
    }
}