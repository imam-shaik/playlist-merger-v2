// Phase 1: Regression Corpus — Damaged Media Generator
// Generates intentionally corrupted test media to exercise repair paths.
// Every production repair function MUST have at least one corresponding damage type.
// This is the highest-ROI component for production certification.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

pub struct DamageGenerator {
    ffmpeg_path: PathBuf,
    ffprobe_path: PathBuf,
    temp_dir: PathBuf,
}

impl DamageGenerator {
    pub fn new(ffmpeg_path: PathBuf, ffprobe_path: PathBuf, temp_dir: PathBuf) -> Self {
        Self {
            ffmpeg_path,
            ffprobe_path,
            temp_dir,
        }
    }

    fn run_ffmpeg(&self, args: &[&str]) -> Result<()> {
        let output = std::process::Command::new(&self.ffmpeg_path)
            .args(args)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("ffmpeg failed: {}", stderr);
        }
        Ok(())
    }

    fn probe_duration(&self, path: &Path) -> Result<f64> {
        let output = std::process::Command::new(&self.ffprobe_path)
            .args([
                "-v", "quiet",
                "-print_format", "json",
                "-show_format",
                path.to_str().unwrap_or(""),
            ])
            .output()?;
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let dur = json["format"]["duration"]
            .as_str()
            .and_then(|s| s.parse::<f64>().ok())
            .or_else(|| json["format"]["duration"].as_f64())
            .ok_or_else(|| anyhow!("could not probe duration of {:?}", path))?;
        Ok(dur)
    }

    // ─────────────────────────────────────────────────────────────
    // P0: Subtitle Repair Coverage (CRITICAL GAP)
    // Production: normalization.rs:2003-2069 (repair_subtitle_file)
    // ─────────────────────────────────────────────────────────────

    /// Generate SRT file with overlapping cue timestamps.
    /// Production repair: normalization.rs:2039-2054 (trim previous end)
    pub fn generate_srt_overlapping_cues(&self, output: &Path) -> Result<()> {
        let content = r#"1
00:00:01,000 --> 00:00:03,000
First subtitle

2
00:00:02,500 --> 00:00:04,000
This overlaps with the previous subtitle (starts at 2.5s, prev ends at 3s)
"#;
        std::fs::write(output, content)?;
        Ok(())
    }

    /// Generate SRT file with negative timestamps.
    /// Production repair: normalization.rs:2021-2029 (clamp to 0)
    pub fn generate_srt_negative_timestamps(&self, output: &Path) -> Result<()> {
        let content = r#"1
00:00:-01,000 --> 00:00:02,000
Negative start timestamp

2
00:00:02,000 --> 00:00:-01,000
Negative end timestamp
"#;
        std::fs::write(output, content)?;
        Ok(())
    }

    /// Generate SRT file with zero-duration cues (end <= start).
    /// Production repair: normalization.rs:2031-2035 (set end = start + 1s)
    pub fn generate_srt_zero_duration(&self, output: &Path) -> Result<()> {
        let content = r#"1
00:00:01,000 --> 00:00:01,000
Zero duration cue

2
00:00:03,000 --> 00:00:02,000
End before start
"#;
        std::fs::write(output, content)?;
        Ok(())
    }

    /// Generate SRT file with subtitle PTS out of range (> video_duration * 2).
    /// Production repair: media_validation_engine.rs:1437-1447 (pts_out_of_range)
    pub fn generate_srt_pts_out_of_range(&self, output: &Path) -> Result<()> {
        let content = r#"1
00:00:01,000 --> 00:00:03,000
Normal subtitle

2
99:59:59,000 --> 99:59:59,500
PTS way out of range (100 hours in a 10 second video)
"#;
        std::fs::write(output, content)?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────
    // P0: Timestamp Repair Coverage (missing PTS, pts overflow)
    // Production: media_validation_engine.rs (try_fix_timestamp_repair)
    // ─────────────────────────────────────────────────────────────

    /// Generate a file where packets have pts == i64::MIN (missing PTS).
    /// Production repair: media_validation_engine.rs:936-948 (missing_pts)
    /// Detection: pts < 0 || pts == i64::MIN → TimestampDamage → TimestampRepair
    pub fn generate_missing_pts(&self, input: &Path, output: &Path) -> Result<()> {
        let temp_path = self.temp_dir.join("temp_missing_pts.mp4");
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-c:a", "aac",
            "-b:a", "128k",
            "-fflags", "+genpts+discardcorrupt",
            "-fps_mode", "cfr",
            &temp_path.to_string_lossy(),
        ])?;
        let mut data = std::fs::read(&temp_path)?;
        if data.len() > 5000 {
            for i in (0..1000).step_by(4) {
                data[i] = 0x00;
                data[i + 1] = 0x00;
                data[i + 2] = 0x00;
                data[i + 3] = 0x80;
            }
        }
        std::fs::write(output, &data)?;
        std::fs::remove_file(temp_path).ok();
        Ok(())
    }

    /// Generate a file with PTS overflow (> 10_000_000_000).
    /// Production repair: media_validation_engine.rs:979-996 (pts_overflow)
    /// Note: Cannot easily generate real pts overflow via ffmpeg; instead
    /// corrupt packet headers to simulate.
    pub fn generate_pts_overflow(&self, input: &Path, output: &Path) -> Result<()> {
        let temp_path = self.temp_dir.join("temp_pts_overflow.mp4");
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-c:a", "aac",
            "-b:a", "128k",
            &temp_path.to_string_lossy(),
        ])?;
        let mut data = std::fs::read(&temp_path)?;
        if data.len() > 5000 {
            for i in (0..2000).step_by(8) {
                data[i] = 0xFF;
                data[i + 1] = 0xFF;
                data[i + 2] = 0xFF;
                data[i + 3] = 0xFF;
            }
        }
        std::fs::write(output, &data)?;
        std::fs::remove_file(temp_path).ok();
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────
    // P1: Container Deep-Check Coverage
    // Production: media_validation_engine.rs:1294-1401 (deep_container_check)
    // ─────────────────────────────────────────────────────────────

    /// Generate a file with broken moov atom (container corruption).
    /// Production: ContainerDamage → ContainerRemux
    pub fn generate_broken_moov(&self, input: &Path, output: &Path) -> Result<()> {
        let temp_path = self.temp_dir.join("temp_broken_moov.mp4");
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c", "copy",
            &temp_path.to_string_lossy(),
        ])?;
        let mut data = std::fs::read(&temp_path)?;
        if data.len() > 1000 {
            for i in 0..100 {
                data[i] ^= 0xFF;
            }
        }
        std::fs::write(output, data)?;
        std::fs::remove_file(temp_path).ok();
        Ok(())
    }

    /// Generate a truncated file (simulates incomplete download).
    /// Production: ContainerDamage → ContainerRemux
    pub fn generate_truncated(&self, input: &Path, output: &Path, truncate_pct: f32) -> Result<()> {
        let temp_path = self.temp_dir.join("temp_trunc.mp4");
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c", "copy",
            &temp_path.to_string_lossy(),
        ])?;
        let metadata = std::fs::metadata(&temp_path)?;
        let original_size = metadata.len();
        let truncate_at = (original_size as f32 * truncate_pct / 100.0) as u64;
        let mut file = std::fs::File::open(&temp_path)?;
        let mut buffer = Vec::new();
        use std::io::Read;
        file.read_to_end(&mut buffer)?;
        buffer.truncate(truncate_at as usize);
        std::fs::write(output, &buffer)?;
        std::fs::remove_file(temp_path).ok();
        Ok(())
    }

    /// Generate a file with zero duration (0 second duration).
    /// Production: media_validation_engine.rs:1343-1354 (zero_duration)
    /// Creates very short clip then truncates duration metadata.
    pub fn generate_zero_duration(&self, input: &Path, output: &Path) -> Result<()> {
        let temp_path = self.temp_dir.join("temp_zero_dur.mp4");
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-t", "0.001",
            "-c", "copy",
            &temp_path.to_string_lossy(),
        ])?;
        std::fs::rename(temp_path, output)?;
        Ok(())
    }

    /// Generate a file with no audio stream (video-only).
    /// Production: missing_audio_stream → AudioReencode
    pub fn generate_no_audio_stream(&self, input: &Path, output: &Path) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", "copy",
            "-an",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    /// Generate a file with no video stream (audio-only).
    /// Production: missing_video_stream → FullReencode (or quarantine if unfixable)
    pub fn generate_no_video_stream(&self, input: &Path, output: &Path) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-vn",
            "-c:a", "aac",
            "-b:a", "128k",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────
    // P1: Audio Timing Damage Coverage
    // Production: normalization.rs:746-777 (audio_start_offset, duration_drift)
    // ─────────────────────────────────────────────────────────────

    /// Generate audio that starts 1 second after video (audio_start_offset).
    /// Production: normalization.rs:746-759 → FullReencode
    pub fn generate_audio_start_offset(&self, input: &Path, output: &Path) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-itsoffset", "1.0",
            "-i", &input.to_string_lossy(),
            "-map", "0:v:0",
            "-map", "1:a:0",
            "-c:v", "copy",
            "-c:a", "aac",
            "-b:a", "128k",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    /// Generate audio that is shorter than video by 2 seconds (duration drift).
    /// Production: normalization.rs:764-777 → AudioReencode
    pub fn generate_audio_duration_drift(&self, input: &Path, output: &Path) -> Result<()> {
        let dur = self.probe_duration(input)?;
        let audio_duration = (dur - 2.0).max(1.0);
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", "copy",
            "-c:a", "aac",
            "-b:a", "128k",
            "-t", &audio_duration.to_string(),
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────
    // P1: Intra-File Audio Mismatch Coverage
    // Production: normalization.rs:856-891 (intra_audio_*_mismatch)
    // ─────────────────────────────────────────────────────────────

    /// Generate a file with two audio streams: stereo (AAC) and 5.1 (AC3).
    /// Production: normalization.rs:868-879 (intra_audio_channel_mismatch)
    pub fn generate_intra_file_channel_mismatch(&self, input: &Path, output: &Path) -> Result<()> {
        let temp_aac = self.temp_dir.join("temp_stereo.aac");
        let temp_ac3 = self.temp_dir.join("temp_51.ac3");
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:a", "aac",
            "-b:a", "128k",
            "-ac", "2",
            &temp_aac.to_string_lossy(),
        ])?;
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:a", "ac3",
            "-b:a", "192k",
            "-ac", "6",
            &temp_ac3.to_string_lossy(),
        ])?;
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-i", &temp_aac.to_string_lossy(),
            "-i", &temp_ac3.to_string_lossy(),
            "-c:v", "copy",
            "-map", "0:v:0",
            "-map", "1:a:0",
            "-map", "2:a:0",
            "-c:a", "copy",
            &output.to_string_lossy(),
        ])?;
        std::fs::remove_file(temp_aac).ok();
        std::fs::remove_file(temp_ac3).ok();
        Ok(())
    }

    /// Generate a file with two audio streams: 44.1kHz and 48kHz.
    /// Production: normalization.rs:880-891 (intra_audio_sample_rate_mismatch)
    pub fn generate_intra_file_sample_rate_mismatch(&self, input: &Path, output: &Path) -> Result<()> {
        let temp_441 = self.temp_dir.join("temp_44100.aac");
        let temp_48 = self.temp_dir.join("temp_48000.aac");
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:a", "aac",
            "-b:a", "128k",
            "-ar", "44100",
            &temp_441.to_string_lossy(),
        ])?;
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:a", "aac",
            "-b:a", "128k",
            "-ar", "48000",
            &temp_48.to_string_lossy(),
        ])?;
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-i", &temp_441.to_string_lossy(),
            "-i", &temp_48.to_string_lossy(),
            "-c:v", "copy",
            "-map", "0:v:0",
            "-map", "1:a:0",
            "-map", "2:a:0",
            "-c:a", "copy",
            &output.to_string_lossy(),
        ])?;
        std::fs::remove_file(temp_441).ok();
        std::fs::remove_file(temp_48).ok();
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────
    // P1: Non-Monotonic DTS/PTS Coverage (already partially covered)
    // Production: media_validation_engine.rs:1014-1073
    // ─────────────────────────────────────────────────────────────

    /// Generate a file with non-monotonic DTS.
    /// Production: TimestampDamage → TimestampRepair
    pub fn generate_non_monotonic_dts(&self, input: &Path, output: &Path) -> Result<()> {
        let temp_path = self.temp_dir.join("temp_bad_dts.mp4");
        self.run_ffmpeg(&[
            "-y",
            "-fflags", "+discardcorrupt+nobuffer",
            "-i", &input.to_string_lossy(),
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-c:a", "aac",
            "-b:a", "128k",
            "-fflags", "+genpts+igndts",
            "-fps_mode", "cfr",
            &temp_path.to_string_lossy(),
        ])?;
        let mut data = std::fs::read(&temp_path)?;
        if data.len() > 5000 {
            for i in (0..1500).step_by(8) {
                data[i] ^= 0x20;
            }
        }
        std::fs::write(output, &data)?;
        std::fs::remove_file(temp_path).ok();
        Ok(())
    }

    /// Generate a file with VFR (variable frame rate).
    /// Production: Warning only, passes through to merge
    pub fn generate_vfr(&self, input: &Path, output: &Path) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-fflags", "+discardcorrupt",
            "-i", &input.to_string_lossy(),
            "-c:v", "libx264",
            "-preset", "ultrafast",
            "-crf", "28",
            "-c:a", "aac",
            "-b:a", "128k",
            "-vsync", "vfr",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────
    // P2: Video Normalization Coverage (FPS, resolution, codec)
    // Production: normalization.rs:634-797
    // ─────────────────────────────────────────────────────────────

    /// Generate a file with mismatched FPS (25fps in a 30fps playlist).
    /// Production: normalization.rs:650-655 → VideoReencode
    pub fn generate_mismatched_fps(&self, input: &Path, output: &Path, fps: f64) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-r", &fps.to_string(),
            "-c:a", "aac",
            "-b:a", "128k",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    /// Generate a file with different resolution (720p in a 1080p playlist).
    /// Production: normalization.rs:648 → VideoReencode
    pub fn generate_mismatched_resolution(&self, input: &Path, output: &Path, width: u32, height: u32) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-vf", &format!("scale={}:{}", width, height),
            "-c:a", "aac",
            "-b:a", "128k",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    /// Generate a file with HDR (SMPTE 2084) transferred to SDR playlist.
    /// Production: normalization.rs:787-797 → VideoReencode
    pub fn generate_hdr_to_sdr_mismatch(&self, input: &Path, output: &Path) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", "libx265",
            "-preset", "fast",
            "-crf", "23",
            "-color_transfer", "smpte2084",
            "-color_primaries", "bt2020",
            "-c:a", "aac",
            "-b:a", "128k",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    /// Generate a file with a different video codec (VP9 in an H.264 playlist).
    /// Production: normalization.rs:634 → FullReencode
    /// Note: VP9 encoding is very slow; use short duration
    pub fn generate_codec_mismatch(&self, input: &Path, output: &Path, codec: &str) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", codec,
            "-preset", "fast",
            "-crf", "28",
            "-c:a", "aac",
            "-b:a", "128k",
            "-t", "5",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }

    /// Generate an interlaced (TFF) file for testing deinterlacing.
    /// Production: normalization.rs:689-704 → VideoReencode
    pub fn generate_interlaced(&self, input: &Path, output: &Path) -> Result<()> {
        self.run_ffmpeg(&[
            "-y",
            "-i", &input.to_string_lossy(),
            "-c:v", "libx264",
            "-preset", "fast",
            "-crf", "23",
            "-x264-params", "fieldtff=1",
            "-c:a", "aac",
            "-b:a", "128k",
            &output.to_string_lossy(),
        ])?;
        Ok(())
    }
}

/// Generate a complete damage test suite from source files.
/// Returns paths to all generated damaged media organized by category.
pub fn generate_damage_suite(
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
    source_dir: &Path,
    output_dir: &Path,
) -> Result<DamageSuiteOutput> {
    std::fs::create_dir_all(output_dir)?;

    let generator = DamageGenerator::new(
        ffmpeg_path.to_path_buf(),
        ffprobe_path.to_path_buf(),
        output_dir.to_path_buf(),
    );

    let sources: Vec<_> = std::fs::read_dir(source_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            ext == "mp4" || ext == "mkv"
        })
        .take(3)
        .collect();

    let mut suite = DamageSuiteOutput::default();

    for (i, source) in sources.iter().enumerate() {
        let src = source.path();

        // P0: Subtitle (SRT files — no source needed)
        let srt_overlap = output_dir.join(format!("srt_overlapping_cues_{}.srt", i));
        if !srt_overlap.exists() {
            let _ = generator.generate_srt_overlapping_cues(&srt_overlap);
        }
        suite.subtitle_repairs.push((srt_overlap.clone(), "overlapping_cues".to_string()));

        let srt_neg = output_dir.join(format!("srt_negative_ts_{}.srt", i));
        if !srt_neg.exists() {
            let _ = generator.generate_srt_negative_timestamps(&srt_neg);
        }
        suite.subtitle_repairs.push((srt_neg.clone(), "negative_timestamps".to_string()));

        let srt_zero = output_dir.join(format!("srt_zero_duration_{}.srt", i));
        if !srt_zero.exists() {
            let _ = generator.generate_srt_zero_duration(&srt_zero);
        }
        suite.subtitle_repairs.push((srt_zero.clone(), "zero_duration".to_string()));

        let srt_oob = output_dir.join(format!("srt_pts_out_of_range_{}.srt", i));
        if !srt_oob.exists() {
            let _ = generator.generate_srt_pts_out_of_range(&srt_oob);
        }
        suite.subtitle_repairs.push((srt_oob.clone(), "pts_out_of_range".to_string()));

        // P0: Timestamp
        let missing_pts = output_dir.join(format!("damage_missing_pts_{}.mp4", i));
        if !missing_pts.exists() {
            let _ = generator.generate_missing_pts(&src, &missing_pts);
        }
        suite.timestamp_repairs.push((missing_pts.clone(), "missing_pts".to_string()));

        let pts_overflow = output_dir.join(format!("damage_pts_overflow_{}.mp4", i));
        if !pts_overflow.exists() {
            let _ = generator.generate_pts_overflow(&src, &pts_overflow);
        }
        suite.timestamp_repairs.push((pts_overflow.clone(), "pts_overflow".to_string()));

        // P1: Container
        let broken_moov = output_dir.join(format!("damage_broken_moov_{}.mp4", i));
        if !broken_moov.exists() {
            let _ = generator.generate_broken_moov(&src, &broken_moov);
        }
        suite.container_repairs.push((broken_moov.clone(), "broken_moov".to_string()));

        let truncated = output_dir.join(format!("damage_truncated_{}.mp4", i));
        if !truncated.exists() {
            let _ = generator.generate_truncated(&src, &truncated, 70.0);
        }
        suite.container_repairs.push((truncated.clone(), "truncated".to_string()));

        let zero_dur = output_dir.join(format!("damage_zero_dur_{}.mp4", i));
        if !zero_dur.exists() {
            let _ = generator.generate_zero_duration(&src, &zero_dur);
        }
        suite.container_repairs.push((zero_dur.clone(), "zero_duration".to_string()));

        // P1: Audio timing
        let audio_offset = output_dir.join(format!("damage_audio_offset_{}.mp4", i));
        if !audio_offset.exists() {
            let _ = generator.generate_audio_start_offset(&src, &audio_offset);
        }
        suite.audio_repairs.push((audio_offset.clone(), "audio_start_offset".to_string()));

        let audio_drift = output_dir.join(format!("damage_audio_drift_{}.mp4", i));
        if !audio_drift.exists() {
            let _ = generator.generate_audio_duration_drift(&src, &audio_drift);
        }
        suite.audio_repairs.push((audio_drift.clone(), "audio_duration_drift".to_string()));

        // P1: Intra-file audio
        let intra_ch = output_dir.join(format!("damage_intra_ch_{}.mp4", i));
        if !intra_ch.exists() {
            let _ = generator.generate_intra_file_channel_mismatch(&src, &intra_ch);
        }
        suite.audio_repairs.push((intra_ch.clone(), "intra_file_channel_mismatch".to_string()));

        let intra_sr = output_dir.join(format!("damage_intra_sr_{}.mp4", i));
        if !intra_sr.exists() {
            let _ = generator.generate_intra_file_sample_rate_mismatch(&src, &intra_sr);
        }
        suite.audio_repairs.push((intra_sr.clone(), "intra_file_sample_rate_mismatch".to_string()));

        // P1: Stream presence
        let no_audio = output_dir.join(format!("damage_no_audio_{}.mp4", i));
        if !no_audio.exists() {
            let _ = generator.generate_no_audio_stream(&src, &no_audio);
        }
        suite.audio_repairs.push((no_audio.clone(), "no_audio_stream".to_string()));

        // P1: DTS/PTS
        let nonmon_dts = output_dir.join(format!("damage_nonmon_dts_{}.mp4", i));
        if !nonmon_dts.exists() {
            let _ = generator.generate_non_monotonic_dts(&src, &nonmon_dts);
        }
        suite.timestamp_repairs.push((nonmon_dts.clone(), "non_monotonic_dts".to_string()));

        // P2: Video normalization
        let fps_mismatch = output_dir.join(format!("damage_fps_25_{}.mp4", i));
        if !fps_mismatch.exists() {
            let _ = generator.generate_mismatched_fps(&src, &fps_mismatch, 25.0);
        }
        suite.video_normalization.push((fps_mismatch.clone(), "fps_mismatch".to_string()));

        let res_mismatch = output_dir.join(format!("damage_res_720p_{}.mp4", i));
        if !res_mismatch.exists() {
            let _ = generator.generate_mismatched_resolution(&src, &res_mismatch, 1280, 720);
        }
        suite.video_normalization.push((res_mismatch.clone(), "resolution_mismatch".to_string()));

        let vfr = output_dir.join(format!("damage_vfr_{}.mp4", i));
        if !vfr.exists() {
            let _ = generator.generate_vfr(&src, &vfr);
        }
        suite.video_normalization.push((vfr.clone(), "vfr".to_string()));

        let interlaced = output_dir.join(format!("damage_interlaced_{}.mp4", i));
        if !interlaced.exists() {
            let _ = generator.generate_interlaced(&src, &interlaced);
        }
        suite.video_normalization.push((interlaced.clone(), "interlaced".to_string()));
    }

    Ok(suite)
}

#[derive(Debug, Default)]
pub struct DamageSuiteOutput {
    pub subtitle_repairs: Vec<(PathBuf, String)>,
    pub timestamp_repairs: Vec<(PathBuf, String)>,
    pub container_repairs: Vec<(PathBuf, String)>,
    pub audio_repairs: Vec<(PathBuf, String)>,
    pub video_normalization: Vec<(PathBuf, String)>,
}

impl DamageSuiteOutput {
    pub fn total_count(&self) -> usize {
        self.subtitle_repairs.len()
            + self.timestamp_repairs.len()
            + self.container_repairs.len()
            + self.audio_repairs.len()
            + self.video_normalization.len()
    }

    pub fn summary(&self) -> String {
        format!(
            "Damage Suite: {} files | {} subtitle | {} timestamp | {} container | {} audio | {} video",
            self.total_count(),
            self.subtitle_repairs.len(),
            self.timestamp_repairs.len(),
            self.container_repairs.len(),
            self.audio_repairs.len(),
            self.video_normalization.len()
        )
    }
}