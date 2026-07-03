// Validators output module
use crate::validators::ffprobe::FfprobeOutput;
use anyhow::Result;
use std::path::Path;

pub struct OutputValidator;

impl OutputValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate_output_exists(&self, path: &Path) -> Result<bool> {
        Ok(path.exists())
    }

    pub fn validate_ffprobe(&self, path: &Path) -> Result<Option<FfprobeOutput>> {
        if !path.exists() {
            return Ok(None);
        }

        let validator = crate::validators::FfprobeValidator::new();
        Ok(Some(validator.probe(path)?))
    }

    pub fn validate_duration(&self, path: &Path, expected_secs: f64, tolerance_pct: f64) -> Result<bool> {
        let validator = crate::validators::FfprobeValidator::new();
        validator.verify_duration(path, expected_secs, tolerance_pct)
    }

    pub fn validate_stream_counts(&self, path: &Path, min_video: usize, min_audio: usize, min_subtitle: usize) -> Result<bool> {
        let probe = crate::validators::FfprobeValidator::new().probe(path)?;
        Ok(probe.video_count() >= min_video
            && probe.audio_count() >= min_audio
            && probe.subtitle_count() >= min_subtitle)
    }
}

impl Default for OutputValidator {
    fn default() -> Self {
        Self::new()
    }
}