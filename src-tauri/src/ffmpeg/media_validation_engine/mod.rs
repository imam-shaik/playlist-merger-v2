use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub mod types;
pub use types::enums::*;
pub use types::structs::*;

pub mod pipeline;
pub mod analyze;
pub mod repair;
pub mod revalidate;

#[cfg(test)]
mod tests;

pub struct MediaValidationEngine {
    pub ffprobe_path: std::path::PathBuf,
    pub ffmpeg_path: std::path::PathBuf,
    pub temp_dir: std::path::PathBuf,
    #[allow(dead_code)]
    pub quarantine_dir: std::path::PathBuf,
    pub cancel_flag: Option<Arc<AtomicBool>>,
}

impl MediaValidationEngine {
    pub fn new(
        ffprobe_path: &Path,
        ffmpeg_path: &Path,
        temp_dir: &Path,
        quarantine_dir: &Path,
    ) -> Self {
        Self {
            ffprobe_path: ffprobe_path.to_path_buf(),
            ffmpeg_path: ffmpeg_path.to_path_buf(),
            temp_dir: temp_dir.to_path_buf(),
            quarantine_dir: quarantine_dir.to_path_buf(),
            cancel_flag: None,
        }
    }

    pub fn with_cancel_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.cancel_flag = Some(flag);
        self
    }

    pub fn get_duration_secs(&self, file_path: &str) -> f64 {
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
}

pub fn validate_input_files(
    ffprobe_path: &Path,
    ffmpeg_path: &Path,
    input_files: &[String],
    quarantine_dir: &Path,
    cancel_flag: Option<Arc<AtomicBool>>,
) -> MediaValidationReport {
    let temp_dir = match crate::ffmpeg::get_temp_dir() {
        Ok(d) => d,
        Err(_) => std::env::temp_dir(),
    };

    let engine = MediaValidationEngine::new(ffprobe_path, ffmpeg_path, &temp_dir, quarantine_dir);
    let cancel_flag_ref: Option<&Arc<AtomicBool>> = cancel_flag.as_ref();
    let engine = if let Some(flag) = cancel_flag_ref {
        engine.with_cancel_flag(flag.clone())
    } else {
        engine
    };

    let pipeline_report = engine.run_pipeline(input_files, &temp_dir, cancel_flag_ref);
    pipeline_report.to_media_validation_report(pipeline_report.total_duration_ms as u128)
}

pub fn apply_validation_results(
    original_files: &[String],
    report: &MediaValidationReport,
) -> (Vec<String>, Vec<usize>) {
    let mut updated = Vec::new();
    let mut removed = Vec::new();

    for result in &report.file_results {
        if matches!(result.status, ValidationStatus::Quarantined) {
            removed.push(result.file_index);
        } else if let Some(ref path) = result.repaired_path {
            updated.push(path.clone());
        } else {
            updated.push(original_files[result.file_index].clone());
        }
    }

    (updated, removed)
}