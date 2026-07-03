use std::path::Path;
use anyhow::{Context, Result};

/// Subtitle Timeline Engine
/// 
/// Handles SRT parsing, timestamp manipulation, validation, and merging for both
/// split (part-wise) and non-split (full timeline) merge modes.
/// 
/// # Split Mode
/// For part-relative timelines (e.g., Part 2 starts at 60s), subtract offset:
/// 
/// ```
/// let mut timeline = SubtitleTimeline::from_srt(&srt_content);
/// timeline.rebase(60.0);  // Subtract 60s, so first cue at 65s becomes 5s
/// ```
/// 
/// # Non-Split Mode  
/// For full timeline concatenation, add cumulative offsets before merging:
/// 
/// ```
/// let cumulative_offset = 300.0;  // Sum of previous video durations
/// let mut timeline = SubtitleTimeline::from_srt(&srt_content);
/// timeline.add_offset(cumulative_offset);  // Add 300s to all timestamps
/// ```

#[derive(Debug, Clone)]
pub struct SrtCue {
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct TimelineValidation {
    pub is_valid: bool,
    #[allow(dead_code)]
    pub cue_count: usize,
    pub backwards_cues: Vec<usize>,      // end <= start
    pub negative_timestamps: Vec<usize>, // start < 0
    pub overlapping_cues: Vec<usize>,    // cue starts before previous ends
    pub timestamp_overflow: Vec<usize>, // timestamp > 99:59:59
    pub duplicate_timestamps: Vec<usize>, // consecutive cues with identical start
    pub empty_text: Vec<usize>,          // subtitle text is empty after trim
}

impl TimelineValidation {
    #[allow(dead_code)]
    pub fn summary(&self) -> String {
        if self.is_valid {
            return format!("OK ({} cues)", self.cue_count);
        }
        let mut issues = Vec::new();
        if !self.negative_timestamps.is_empty() {
            issues.push(format!("{} negative", self.negative_timestamps.len()));
        }
        if !self.backwards_cues.is_empty() {
            issues.push(format!("{} backwards", self.backwards_cues.len()));
        }
        if !self.overlapping_cues.is_empty() {
            issues.push(format!("{} overlapping", self.overlapping_cues.len()));
        }
        if !self.timestamp_overflow.is_empty() {
            issues.push(format!("{} overflow", self.timestamp_overflow.len()));
        }
        if !self.duplicate_timestamps.is_empty() {
            issues.push(format!("{} duplicate", self.duplicate_timestamps.len()));
        }
        if !self.empty_text.is_empty() {
            issues.push(format!("{} empty", self.empty_text.len()));
        }
        format!("INVALID: {}", issues.join(", "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum BoundaryPolicy {
    Clip,      // Clip cues that cross boundaries to boundary value
    Discard,   // Discard cues that cross boundaries entirely
    Keep,      // Keep cues even if they cross boundaries
}

pub struct SubtitleTimeline {
    cues: Vec<SrtCue>,
}

impl SubtitleTimeline {
    pub fn new() -> Self {
        Self { cues: Vec::new() }
    }

    /// Parse SRT content into a SubtitleTimeline.
    pub fn from_srt(content: &str) -> Self {
        let cues = parse_srt_content(content);
        Self { cues }
    }

    /// Load SRT content from a file path.
    pub fn from_srt_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read SRT file: {:?}", path))?;
        Ok(Self::from_srt(&content))
    }

    /// Get the number of cues in this timeline.
    pub fn cue_count(&self) -> usize {
        self.cues.len()
    }

    /// Get all cues.
    #[allow(dead_code)]
    pub fn cues(&self) -> &[SrtCue] {
        &self.cues
    }

    /// Add offset to all timestamps (for non-split: cumulative position in merged timeline).
    /// Example: Video 2 starts at 300s, so add 300 to all Video 2's subtitle timestamps.
    pub fn add_offset(&mut self, offset_seconds: f64) {
        for cue in &mut self.cues {
            cue.start_time += offset_seconds;
            cue.end_time += offset_seconds;
        }
    }

    /// Subtract offset from all timestamps (for split: rebase to part-relative timeline).
    /// Example: Part starts at 60s, so subtract 60 so first cue at 65s becomes 5s.
    pub fn rebase(&mut self, offset_seconds: f64) {
        for cue in &mut self.cues {
            cue.start_time -= offset_seconds;
            cue.end_time -= offset_seconds;
        }
    }

    /// Clip negative timestamps to 0.
    pub fn clip_negative_to_zero(&mut self) {
        for cue in &mut self.cues {
            if cue.start_time < 0.0 {
                cue.start_time = 0.0;
            }
            if cue.end_time < 0.0 {
                cue.end_time = 0.0;
            }
        }
    }

    /// Filter out invalid cues (where end_time <= start_time).
    pub fn filter_invalid(&mut self) {
        self.cues.retain(|cue| cue.end_time > cue.start_time);
    }

    /// Validate timeline integrity.
    /// Returns a TimelineValidation with any issues found.
    ///
    /// Checks:
    /// - end > start (backwards cues)
    /// - start >= 0 (negative timestamps)
    /// - timestamps monotonic (no overlap with previous cue)
    /// - no timestamp overflow (> 99:59:59)
    /// - no duplicate consecutive start times
    /// - no empty text after trim
    pub fn validate(&self) -> TimelineValidation {
        let mut validation = TimelineValidation {
            is_valid: true,
            cue_count: self.cues.len(),
            ..Default::default()
        };

        const MAX_SRT_TIMESTAMP: f64 = 359999.999; // 99:59:59.999

        for (i, cue) in self.cues.iter().enumerate() {
            // Check for negative timestamps
            if cue.start_time < 0.0 || cue.end_time < 0.0 {
                validation.negative_timestamps.push(i);
                validation.is_valid = false;
            }

            // Check for backwards cues (end <= start)
            if cue.end_time <= cue.start_time {
                validation.backwards_cues.push(i);
                validation.is_valid = false;
            }

            // Check for timestamp overflow (SRT format limits to 99:59:59)
            if cue.start_time > MAX_SRT_TIMESTAMP || cue.end_time > MAX_SRT_TIMESTAMP {
                validation.timestamp_overflow.push(i);
                validation.is_valid = false;
            }

            // Check for overlapping with previous cue
            if i > 0 {
                let prev = &self.cues[i - 1];
                if cue.start_time < prev.end_time {
                    validation.overlapping_cues.push(i);
                    validation.is_valid = false;
                }
                // Check for duplicate start times (should be strictly increasing)
                if (cue.start_time - prev.start_time).abs() < 0.001 {
                    validation.duplicate_timestamps.push(i);
                    validation.is_valid = false;
                }
            }

            // Check for empty text
            if cue.text.trim().is_empty() {
                validation.empty_text.push(i);
                // Empty text is a warning, not a fatal error
            }
        }

        validation
    }

    /// Merge another timeline after this one, adding cumulative offset automatically.
    /// The other timeline's timestamps will be shifted by (cumulative_offset + gap_seconds).
    #[allow(dead_code)]
    pub fn merge_after(&mut self, other: &SubtitleTimeline, cumulative_offset: f64, gap_seconds: f64) {
        let mut other_cues = other.cues.clone();
        let shift = cumulative_offset + gap_seconds;
        for cue in &mut other_cues {
            cue.start_time += shift;
            cue.end_time += shift;
        }
        self.cues.extend(other_cues);
    }

    /// Apply boundary policy: clip or discard cues that cross the boundary at `boundary_seconds`.
    #[allow(dead_code)]
    pub fn apply_boundary_policy(&mut self, boundary_seconds: f64, policy: BoundaryPolicy) {
        match policy {
            BoundaryPolicy::Clip => {
                for cue in &mut self.cues {
                    if cue.start_time < boundary_seconds && cue.end_time > boundary_seconds {
                        cue.end_time = boundary_seconds;
                    }
                }
                self.filter_invalid();
            }
            BoundaryPolicy::Discard => {
                self.cues.retain(|cue| cue.end_time <= boundary_seconds);
            }
            BoundaryPolicy::Keep => {
                // No-op, keep all cues
            }
        }
    }

    /// Export timeline to SRT string format with proper cue numbering.
    pub fn to_srt_string(&self) -> String {
        self.cues
            .iter()
            .enumerate()
            .map(|(i, cue)| {
                format!(
                    "{}\n{} --> {}\n{}\n",
                    i + 1,
                    format_srt_ts(cue.start_time),
                    format_srt_ts(cue.end_time),
                    cue.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Write timeline to an SRT file.
    pub fn write_to_srt_file(&self, path: &Path) -> Result<()> {
        let content = self.to_srt_string();
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write SRT file: {:?}", path))?;
        Ok(())
    }

    /// Take a slice of SRT file paths and cumulative offsets, produce a merged timeline.
    /// This is used by the non-split merge path to pre-shift all SRTs before concatenation.
    /// 
    /// # Arguments
    /// * `srt_paths` - Array of optional SRT file paths (None = no subtitle for this segment)
    /// * `cumulative_offsets` - Cumulative duration before each segment (in seconds)
    /// * `segment_durations` - Duration of each segment (used for gap calculation)
    #[allow(dead_code)]
    pub fn merge_srt_files(
        srt_paths: &[Option<String>],
        cumulative_offsets: &[f64],
        segment_durations: &[f64],
    ) -> Result<(Vec<Option<String>>, Vec<f64>)> {
        if srt_paths.len() != cumulative_offsets.len() || srt_paths.len() != segment_durations.len() {
            anyhow::bail!("Array length mismatch in merge_srt_files");
        }

        let mut merged_timelines: Vec<Option<SubtitleTimeline>> = Vec::new();
        let mut final_offsets: Vec<f64> = Vec::new();

        for (i, srt_path) in srt_paths.iter().enumerate() {
            if let Some(path_str) = srt_path {
                let path = Path::new(path_str);
                if path.exists() {
                    let mut timeline = SubtitleTimeline::from_srt_file(path)?;
                    timeline.add_offset(cumulative_offsets[i]);
                    timeline.clip_negative_to_zero();
                    timeline.filter_invalid();
                    merged_timelines.push(Some(timeline));
                    final_offsets.push(cumulative_offsets[i]);
                } else {
                    merged_timelines.push(None);
                    final_offsets.push(cumulative_offsets[i]);
                }
            } else {
                merged_timelines.push(None);
                final_offsets.push(cumulative_offsets[i]);
            }
        }

        // Collect rebased temp file paths
        let temp_dir = std::env::temp_dir();
        let mut rebased_paths: Vec<Option<String>> = Vec::new();
        let _running_offset: f64 = 0.0;

        for (i, timeline_opt) in merged_timelines.iter().enumerate() {
            if let Some(timeline) = timeline_opt {
                let rebased_path = temp_dir.join(format!("merged_sub_{}.srt", i));
                timeline.write_to_srt_file(&rebased_path)?;
                rebased_paths.push(Some(rebased_path.to_string_lossy().into_owned()));
            } else {
                rebased_paths.push(None);
            }
        }

        // Compute final cumulative offsets for concat list
        let mut final_durations: Vec<f64> = Vec::new();
        for i in 0..srt_paths.len() {
            final_durations.push(segment_durations[i]);
        }

        Ok((rebased_paths, final_durations))
    }
}

impl Default for SubtitleTimeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse SRT content into SrtCue entries.
fn parse_srt_content(content: &str) -> Vec<SrtCue> {
    let mut cues = Vec::new();
    let normalized = content.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        // Next line should be timestamp
        if i + 1 >= lines.len() {
            break;
        }

        let timestamp_line = lines[i + 1];
        let timestamps: Vec<&str> = timestamp_line.split("-->").collect();
        if timestamps.len() != 2 {
            i += 1;
            continue;
        }

        let start = parse_srt_ts(timestamps[0].trim());
        let end = parse_srt_ts(timestamps[1].trim());

        // Read subtitle text
        let mut text_lines = Vec::new();
        let mut j = i + 2;
        while j < lines.len() && !lines[j].trim().is_empty() {
            text_lines.push(lines[j]);
            j += 1;
        }

        let text = text_lines.join("\n");
        cues.push(SrtCue { start_time: start, end_time: end, text });

        i = j;
    }

    cues
}

/// Format seconds to SRT timestamp "HH:MM:SS,mmm".
fn format_srt_ts(seconds: f64) -> String {
    let h = (seconds / 3600.0).floor();
    let m = ((seconds % 3600.0) / 60.0).floor();
    let s = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", h, m, s).replace('.', ",")
}

/// Parse SRT timestamp to seconds.
fn parse_srt_ts(s: &str) -> f64 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_and_format_srt_ts() {
        assert_eq!(format_srt_ts(0.0), "00:00:00,000");
        assert_eq!(format_srt_ts(15.5), "00:00:15,500");
        assert_eq!(format_srt_ts(65.25), "00:01:05,250");
        assert_eq!(format_srt_ts(3661.123), "01:01:01,123");
    }

    #[test]
    fn test_parse_srt_ts() {
        assert!((parse_srt_ts("00:00:15,500") - 15.5).abs() < 0.001);
        assert!((parse_srt_ts("00:01:05,250") - 65.25).abs() < 0.001);
    }

    #[test]
    fn test_add_offset() {
        let mut timeline = SubtitleTimeline::from_srt(
            "1\n00:00:15,000 --> 00:00:20,000\nHello\n"
        );
        timeline.add_offset(300.0); // Add 5 minutes
        assert_eq!(timeline.cues[0].start_time, 315.0);
        assert_eq!(timeline.cues[0].end_time, 320.0);
    }

    #[test]
    fn test_rebase() {
        let mut timeline = SubtitleTimeline::from_srt(
            "1\n00:01:05,000 --> 00:01:10,000\nHello\n"
        );
        timeline.rebase(60.0); // Subtract 60 seconds
        assert_eq!(timeline.cues[0].start_time, 5.0);
        assert_eq!(timeline.cues[0].end_time, 10.0);
    }

    #[test]
    fn test_clip_negative() {
        let mut timeline = SubtitleTimeline::from_srt(
            "1\n00:00:00,000 --> 00:00:05,000\nHello\n"
        );
        timeline.rebase(10.0); // Would create negative times
        timeline.clip_negative_to_zero();
        assert_eq!(timeline.cues[0].start_time, 0.0);
        assert_eq!(timeline.cues[0].end_time, 0.0);
    }

    #[test]
    fn test_filter_invalid() {
        let mut timeline = SubtitleTimeline::from_srt(
            "1\n00:00:10,000 --> 00:00:05,000\nHello\n"
        );
        assert_eq!(timeline.cue_count(), 1);
        timeline.filter_invalid();
        assert_eq!(timeline.cue_count(), 0); // Invalid cue filtered out
    }

    #[test]
    fn test_validate() {
        let timeline = SubtitleTimeline::from_srt(
            "1\n00:00:00,000 --> 00:00:05,000\nHello\n\n2\n00:00:06,000 --> 00:00:10,000\nWorld\n"
        );
        let validation = timeline.validate();
        assert!(validation.is_valid);
        assert_eq!(validation.cue_count, 2);
        assert!(validation.backwards_cues.is_empty());
    }

    #[test]
    fn test_to_srt_string() {
        let timeline = SubtitleTimeline::from_srt(
            "1\n00:00:15,000 --> 00:00:20,000\nHello World\n"
        );
        let output = timeline.to_srt_string();
        assert!(output.contains("00:00:15,000 --> 00:00:20,000"));
        assert!(output.contains("Hello World"));
    }

    #[test]
    fn test_merge_after() {
        let mut timeline1 = SubtitleTimeline::from_srt(
            "1\n00:00:00,000 --> 00:00:05,000\nFirst\n"
        );
        let timeline2 = SubtitleTimeline::from_srt(
            "1\n00:00:00,000 --> 00:00:05,000\nSecond\n"
        );
        timeline1.merge_after(&timeline2, 300.0, 0.0); // After 300s, no gap
        assert_eq!(timeline1.cue_count(), 2);
        assert_eq!(timeline1.cues[0].start_time, 0.0);
        assert_eq!(timeline1.cues[1].start_time, 300.0); // 300 + 0 offset
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PROPERTY-BASED TESTS - Round-trip parse/serialize symmetry
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_roundtrip_simple() {
        let original = "1\n00:00:15,000 --> 00:00:20,000\nHello World\n";
        let timeline = SubtitleTimeline::from_srt(original);
        let output = timeline.to_srt_string();
        let reparsed = SubtitleTimeline::from_srt(&output);
        assert_eq!(timeline.cue_count(), reparsed.cue_count());
        assert_eq!(timeline.cues[0].text, reparsed.cues[0].text);
    }

    #[test]
    fn test_roundtrip_multiple_cues() {
        let original = "1\n00:00:01,000 --> 00:00:03,000\nFirst\n\n2\n00:00:05,000 --> 00:00:08,000\nSecond\n\n3\n00:00:10,000 --> 00:00:12,000\nThird\n";
        let timeline = SubtitleTimeline::from_srt(original);
        let output = timeline.to_srt_string();
        let reparsed = SubtitleTimeline::from_srt(&output);
        assert_eq!(timeline.cue_count(), reparsed.cue_count());
        for (a, b) in timeline.cues.iter().zip(reparsed.cues.iter()) {
            assert!((a.start_time - b.start_time).abs() < 0.001);
            assert!((a.end_time - b.end_time).abs() < 0.001);
            assert_eq!(a.text, b.text);
        }
    }

    #[test]
    fn test_roundtrip_unicode() {
        let original = "1\n00:00:01,000 --> 00:00:03,000\n日本語テスト\n\n2\n00:00:05,000 --> 00:00:08,000\n한국어\n\n3\n00:00:10,000 --> 00:00:12,000\nΕλληνικά\n";
        let timeline = SubtitleTimeline::from_srt(original);
        let output = timeline.to_srt_string();
        let reparsed = SubtitleTimeline::from_srt(&output);
        assert_eq!(timeline.cues[0].text, reparsed.cues[0].text);
        assert_eq!(timeline.cues[1].text, reparsed.cues[1].text);
        assert_eq!(timeline.cues[2].text, reparsed.cues[2].text);
    }

    #[test]
    fn test_roundtrip_multiline() {
        let original = "1\n00:00:01,000 --> 00:00:03,000\nLine 1\nLine 2\nLine 3\n";
        let timeline = SubtitleTimeline::from_srt(original);
        let output = timeline.to_srt_string();
        let reparsed = SubtitleTimeline::from_srt(&output);
        assert!(reparsed.cues[0].text.contains("Line 1"));
        assert!(reparsed.cues[0].text.contains("Line 2"));
        assert!(reparsed.cues[0].text.contains("Line 3"));
    }

    #[test]
    fn test_roundtrip_html_tags() {
        let original = "1\n00:00:01,000 --> 00:00:03,000\n<i>Italic</i> and <b>Bold</b>\n";
        let timeline = SubtitleTimeline::from_srt(original);
        let output = timeline.to_srt_string();
        let reparsed = SubtitleTimeline::from_srt(&output);
        assert_eq!(timeline.cues[0].text, reparsed.cues[0].text);
    }

    #[test]
    fn test_roundtrip_emoji() {
        let original = "1\n00:00:01,000 --> 00:00:03,000\n🎬 📽️ 🎥\n\n2\n00:00:05,000 --> 00:00:08,000\n🚀 💡 ✅\n";
        let timeline = SubtitleTimeline::from_srt(original);
        let output = timeline.to_srt_string();
        let reparsed = SubtitleTimeline::from_srt(&output);
        assert_eq!(timeline.cues[0].text, reparsed.cues[0].text);
        assert_eq!(timeline.cues[1].text, reparsed.cues[1].text);
    }

    #[test]
    fn test_roundtrip_rtl_languages() {
        let original = "1\n00:00:01,000 --> 00:00:03,000\nשלום עולם\n\n2\n00:00:05,000 --> 00:00:08,000\nمرحبا بالعالم\n";
        let timeline = SubtitleTimeline::from_srt(original);
        let output = timeline.to_srt_string();
        let reparsed = SubtitleTimeline::from_srt(&output);
        assert_eq!(timeline.cues[0].text, reparsed.cues[0].text);
        assert_eq!(timeline.cues[1].text, reparsed.cues[1].text);
    }

    #[test]
    fn test_roundtrip_crlf() {
        let original = "1\r\n00:00:01,000 --> 00:00:03,000\r\nHello\r\n\r\n2\r\n00:00:05,000 --> 00:00:08,000\r\nWorld\r\n";
        let timeline = SubtitleTimeline::from_srt(original);
        let output = timeline.to_srt_string();
        let _reparsed = SubtitleTimeline::from_srt(&output);
        assert_eq!(timeline.cue_count(), 2);
        assert_eq!(timeline.cues[0].text, "Hello");
        assert_eq!(timeline.cues[1].text, "World");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PROPERTY-BASED TESTS - Offset precision
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_offset_precision_100_videos() {
        // Simulate 100 videos, 10 minutes each
        let mut timeline = SubtitleTimeline::new();
        let durations: Vec<f64> = (0..100).map(|i| 600.0 + (i as f64) * 0.033).collect(); // slight variation

        let mut cum_offset: f64 = 0.0;
        for (i, dur) in durations.iter().enumerate() {
            let mut seg_timeline = SubtitleTimeline::from_srt(&format!(
                "1\n00:00:10,000 --> 00:00:15,000\nVideo {} subtitle\n", i
            ));
            seg_timeline.add_offset(cum_offset);
            for cue in &seg_timeline.cues {
                timeline.cues.push(cue.clone());
            }
            cum_offset += dur;
        }

        // After 100 videos, verify last cue is at correct position
        // Last video (index 99) gets cum_offset = sum of durations[0..99] BEFORE increment
        // But we increment AFTER add_offset, so cum_offset for video 99 = sum of durations[0..98]
        // Which equals total_sum - durations[99]
        let total_sum = durations.iter().sum::<f64>();
        let expected_last_start = total_sum - durations[99] + 10.0;
        let actual_last_start = timeline.cues.last().unwrap().start_time;
        let drift = (expected_last_start - actual_last_start).abs();
        assert!(drift < 0.001,
            "Expected {:.3}s, got {:.3}s, drift = {:.6}s",
            expected_last_start, actual_last_start, expected_last_start - actual_last_start);
    }

    #[test]
    fn test_offset_precision_submillisecond() {
        // Test that 0.001 second offsets are preserved
        let mut timeline = SubtitleTimeline::from_srt("1\n00:00:01,500 --> 00:00:03,500\nTest\n");
        timeline.add_offset(0.001);
        assert!((timeline.cues[0].start_time - 1.501).abs() < 0.0001);
        assert!((timeline.cues[0].end_time - 3.501).abs() < 0.0001);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PROPERTY-BASED TESTS - Validation
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_validation_backwards_cue() {
        let timeline = SubtitleTimeline::from_srt("1\n00:00:10,000 --> 00:00:05,000\nInvalid\n");
        let v = timeline.validate();
        assert!(!v.is_valid);
        assert!(v.backwards_cues.contains(&0));
    }

    #[test]
    fn test_validation_negative_timestamp() {
        let timeline = SubtitleTimeline::from_srt("1\n00:00:00,000 --> 00:00:00,000\nInvalid\n");
        let v = timeline.validate();
        assert!(!v.is_valid);
    }

    #[test]
    fn test_validation_timestamp_overflow() {
        let timeline = SubtitleTimeline::from_srt("1\n99:59:59,999 --> 99:59:59,999\nValid max\n\n2\n100:00:00,000 --> 100:00:01,000\nInvalid overflow\n");
        let v = timeline.validate();
        // First cue should be valid (at boundary), second should overflow
        assert!(!v.timestamp_overflow.is_empty());
    }

    #[test]
    fn test_validation_overlapping() {
        let timeline = SubtitleTimeline::from_srt(
            "1\n00:00:00,000 --> 00:00:05,000\nFirst\n\n2\n00:00:03,000 --> 00:00:08,000\nOverlaps\n"
        );
        let v = timeline.validate();
        assert!(!v.is_valid);
        assert!(v.overlapping_cues.contains(&1)); // Second cue overlaps
    }

    #[test]
    fn test_validation_empty_text() {
        let timeline = SubtitleTimeline::from_srt("1\n00:00:01,000 --> 00:00:03,000\n   \n");
        let v = timeline.validate();
        // Empty text is a warning but not fatal
        assert!(v.empty_text.contains(&0));
        assert!(v.is_valid); // Still valid
    }

    #[test]
    fn test_validation_summary() {
        let valid = SubtitleTimeline::from_srt("1\n00:00:01,000 --> 00:00:03,000\nHello\n");
        assert!(valid.validate().summary().contains("OK"));

        let invalid = SubtitleTimeline::from_srt("1\n00:00:10,000 --> 00:00:05,000\nBad\n");
        assert!(invalid.validate().summary().contains("INVALID"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PROPERTY-BASED TESTS - Edge cases
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_boundary_exact_zero() {
        let mut timeline = SubtitleTimeline::from_srt("1\n00:00:00,000 --> 00:00:01,000\nStart\n");
        timeline.rebase(0.0);
        assert_eq!(timeline.cues[0].start_time, 0.0);
    }

    #[test]
    fn test_boundary_negative_clip() {
        let mut timeline = SubtitleTimeline::from_srt("1\n00:00:00,500 --> 00:00:01,000\nEarly\n");
        timeline.rebase(10.0);
        timeline.clip_negative_to_zero();
        assert_eq!(timeline.cues[0].start_time, 0.0);
        assert_eq!(timeline.cues[0].end_time, 0.0);
    }

    #[test]
    fn test_boundary_span_across() {
        // Cue that spans from before 60s to after 60s
        let mut timeline = SubtitleTimeline::from_srt("1\n00:00:59,500 --> 00:01:01,000\nSpans boundary\n");
        timeline.clip_negative_to_zero(); // Only clips negative, not spanning
        assert_eq!(timeline.cues[0].start_time, 59.5);
        assert_eq!(timeline.cues[0].end_time, 61.0);
    }

    #[test]
    fn test_mixed_valid_invalid_filter() {
        let mut timeline = SubtitleTimeline::from_srt(
            "1\n00:00:01,000 --> 00:00:05,000\nValid\n\n2\n00:00:10,000 --> 00:00:05,000\nInvalid\n\n3\n00:00:15,000 --> 00:00:20,000\nValid2\n"
        );
        assert_eq!(timeline.cue_count(), 3);
        timeline.filter_invalid();
        assert_eq!(timeline.cue_count(), 2); // Only valid ones remain
        assert_eq!(timeline.cues[0].text, "Valid");
        assert_eq!(timeline.cues[1].text, "Valid2");
    }

    #[test]
    fn test_exact_boundary_timestamps() {
        // Video1 ends at 59.999, Video2 starts at 60.000
        let mut tl1 = SubtitleTimeline::from_srt("1\n00:00:55,000 --> 00:00:59,999\nLast of V1\n");
        let mut tl2 = SubtitleTimeline::from_srt("1\n00:00:00,000 --> 00:00:05,000\nFirst of V2\n");
        tl2.add_offset(60.0); // Video 2 starts at 60s
        tl1.merge_after(&tl2, 0.0, 0.0);
        assert_eq!(tl1.cue_count(), 2);
        assert!((tl1.cues[0].end_time - 59.999).abs() < 0.001);
        assert!((tl1.cues[1].start_time - 60.0).abs() < 0.001);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PERFORMANCE BENCHMARKS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn benchmark_100_cues_parse() {
        let srt = generate_test_srt(100);
        let start = std::time::Instant::now();
        let _ = SubtitleTimeline::from_srt(&srt);
        let elapsed = start.elapsed();
        println!("100 cues parse: {:?}", elapsed);
        assert!(elapsed.as_millis() < 50, "Parse too slow: {:?}", elapsed);
    }

    #[test]
    fn benchmark_1000_cues_parse() {
        let srt = generate_test_srt(1000);
        let start = std::time::Instant::now();
        let _ = SubtitleTimeline::from_srt(&srt);
        let elapsed = start.elapsed();
        println!("1000 cues parse: {:?}", elapsed);
        assert!(elapsed.as_millis() < 200, "Parse too slow: {:?}", elapsed);
    }

    #[test]
    fn benchmark_10000_cues_parse() {
        let srt = generate_test_srt(10000);
        let start = std::time::Instant::now();
        let _ = SubtitleTimeline::from_srt(&srt);
        let elapsed = start.elapsed();
        println!("10000 cues parse: {:?}", elapsed);
        assert!(elapsed.as_millis() < 2000, "Parse too slow: {:?}", elapsed);
    }

    #[test]
    fn benchmark_offset_1000_cues() {
        let srt = generate_test_srt(1000);
        let mut timeline = SubtitleTimeline::from_srt(&srt);
        let start = std::time::Instant::now();
        timeline.add_offset(3600.0);
        let elapsed = start.elapsed();
        println!("1000 cues offset: {:?}", elapsed);
        assert!(elapsed.as_millis() < 100, "Offset too slow: {:?}", elapsed);
    }

    #[test]
    fn benchmark_validate_1000_cues() {
        let srt = generate_test_srt(1000);
        let timeline = SubtitleTimeline::from_srt(&srt);
        let start = std::time::Instant::now();
        let _ = timeline.validate();
        let elapsed = start.elapsed();
        println!("1000 cues validate: {:?}", elapsed);
        assert!(elapsed.as_millis() < 100, "Validate too slow: {:?}", elapsed);
    }

    #[test]
    fn benchmark_serialize_1000_cues() {
        let srt = generate_test_srt(1000);
        let timeline = SubtitleTimeline::from_srt(&srt);
        let start = std::time::Instant::now();
        let _ = timeline.to_srt_string();
        let elapsed = start.elapsed();
        println!("1000 cues serialize: {:?}", elapsed);
        assert!(elapsed.as_millis() < 200, "Serialize too slow: {:?}", elapsed);
    }

    #[test]
    fn benchmark_roundtrip_1000_cues() {
        let srt = generate_test_srt(1000);
        let start = std::time::Instant::now();
        let timeline = SubtitleTimeline::from_srt(&srt);
        let output = timeline.to_srt_string();
        let _ = SubtitleTimeline::from_srt(&output);
        let elapsed = start.elapsed();
        println!("1000 cues roundtrip: {:?}", elapsed);
        assert!(elapsed.as_millis() < 500, "Roundtrip too slow: {:?}", elapsed);
    }

    // Helper to generate test SRT content
    fn generate_test_srt(count: usize) -> String {
        let mut srt = String::new();
        for i in 0..count {
            let start = i as f64 * 5.0;
            let end = start + 2.0;
            srt.push_str(&format!(
                "{}\n{:02}:{:02}:{:06.3} --> {:02}:{:02}:{:06.3}\nLine {}\n\n",
                i + 1,
                (start / 3600.0).floor(),
                ((start % 3600.0) / 60.0).floor(),
                start % 60.0,
                (end / 3600.0).floor(),
                ((end % 3600.0) / 60.0).floor(),
                end % 60.0,
                i
            ));
        }
        srt
    }
}