use std::collections::HashMap;

/// Parsed progress entry from `ffmpeg -progress pipe:2`
///
/// FFmpeg with `-progress pipe:2` writes one key=value per line, ending with
/// `progress=continue` (mid-run) or `progress=end` (finished).
/// This is the RELIABLE modern progress protocol — not the old stats line.
#[derive(Debug, Clone, Default)]
pub struct FfmpegProgress {
    #[allow(dead_code)]
    pub frame: Option<u64>,
    pub fps: Option<f32>,
    #[allow(dead_code)]
    pub bitrate_kbps: Option<f32>,
    pub total_size_bytes: Option<u64>,
    pub out_time_seconds: Option<f64>,
    pub speed: Option<f32>,
    pub is_end: bool,
}

/// Parse a complete progress block (all lines between two `progress=` markers).
///
/// Returns `None` if the block contains no `out_time_us` or `out_time` key.
pub fn parse_progress_block(block: &HashMap<String, String>) -> Option<FfmpegProgress> {
    let out_time_us = block.get("out_time_us")
        .and_then(|v| v.trim().parse::<i64>().ok())
        .map(|us| us as f64 / 1_000_000.0);

    let out_time_str = block.get("out_time")
        .and_then(|v| parse_time_string(v.trim()));

    let out_time_seconds = out_time_us.or(out_time_str);
    out_time_seconds?;

    let fps = block.get("fps")
        .and_then(|v| v.trim().parse::<f32>().ok());

    let frame = block.get("frame")
        .and_then(|v| v.trim().parse::<u64>().ok());

    let total_size_bytes = block.get("total_size")
        .and_then(|v| v.trim().parse::<u64>().ok());

    // bitrate comes as "5041.2kbits/s" or "N/A"
    let bitrate_kbps = block.get("bitrate")
        .and_then(|v| {
            let s = v.trim();
            if s == "N/A" { return None; }
            s.trim_end_matches("kbits/s").trim().parse::<f32>().ok()
        });

    // speed comes as "1.5x" or "N/A"
    let speed = block.get("speed")
        .and_then(|v| {
            let s = v.trim();
            if s == "N/A" { return None; }
            s.trim_end_matches('x').trim().parse::<f32>().ok()
        });

    let is_end = block.get("progress")
        .map(|v| v.trim() == "end")
        .unwrap_or(false);

    Some(FfmpegProgress {
        frame,
        fps,
        bitrate_kbps,
        total_size_bytes,
        out_time_seconds,
        speed,
        is_end,
    })
}

/// Parse HH:MM:SS.mmm or SS.mmm time string to seconds
pub fn parse_time_string(s: &str) -> Option<f64> {
    if s == "N/A" || s.is_empty() {
        return None;
    }

    // Handle negative times (can happen with concat demuxer)
    let (negative, s) = if let Some(stripped) = s.strip_prefix('-') { (true, stripped) } else { (false, s) };

    let parts: Vec<&str> = s.splitn(3, ':').collect();
    let result = match parts.as_slice() {
        [h, m, sec] => {
            let hours: f64 = h.parse().ok()?;
            let minutes: f64 = m.parse().ok()?;
            let seconds: f64 = sec.parse().ok()?;
            Some(hours * 3600.0 + minutes * 60.0 + seconds)
        }
        [m, sec] => {
            let minutes: f64 = m.parse().ok()?;
            let seconds: f64 = sec.parse().ok()?;
            Some(minutes * 60.0 + seconds)
        }
        [sec] => sec.parse::<f64>().ok(),
        _ => None,
    };

    result.map(|v| if negative { -v } else { v })
}

/// Build progress percentage from elapsed time and total duration.
/// Clamps to [0, 99] during merge — only reaches 100 on explicit completion.
pub fn calc_progress_percent(elapsed: f64, total: f64) -> f32 {
    if total <= 0.0 || elapsed <= 0.0 {
        return 0.0;
    }
    ((elapsed / total) * 100.0).clamp(0.0, 99.0) as f32
}

/// A streaming line-based parser that accumulates key=value pairs into blocks.
/// Each block ends when a `progress=continue` or `progress=end` line arrives.
pub struct ProgressBlockReader {
    current: HashMap<String, String>,
}

/// Maximum number of key=value entries to accumulate before flushing.
/// FFmpeg outputs ~15 keys per progress block, so 256 is generous.
const MAX_BLOCK_ENTRIES: usize = 256;

impl ProgressBlockReader {
    pub fn new() -> Self {
        Self { current: HashMap::new() }
    }

    /// Feed a single line. Returns Some(block) when a complete block is ready.
    pub fn feed_line(&mut self, line: &str) -> Option<HashMap<String, String>> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }

        if let Some((key, value)) = line.split_once('=') {
            self.current.insert(key.trim().to_string(), value.trim().to_string());
        }

        // Safety cap: if we've accumulated too many entries without seeing
        // `progress=update/end`, flush whatever we have to prevent unbounded memory.
        if self.current.len() >= MAX_BLOCK_ENTRIES {
            let block = std::mem::take(&mut self.current);
            log::warn!("[Progress] Flushing block with {} entries (missing progress=continue/end)", block.len());
            return Some(block);
        }

        // A block is complete when we see `progress=continue` or `progress=end`
        if self.current.get("progress").map(|v| v.as_str() == "continue" || v.as_str() == "end").unwrap_or(false) {
            let block = std::mem::take(&mut self.current);
            return Some(block);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time() {
        assert!((parse_time_string("00:01:30.500").unwrap() - 90.5).abs() < 0.001);
        assert!((parse_time_string("01:00:00.000").unwrap() - 3600.0).abs() < 0.001);
        assert_eq!(parse_time_string("N/A"), None);
        assert_eq!(parse_time_string(""), None);
    }

    #[test]
    fn test_block_reader() {
        let mut reader = ProgressBlockReader::new();
        reader.feed_line("frame=100");
        reader.feed_line("fps=30.0");
        reader.feed_line("out_time_us=3330000");
        reader.feed_line("speed=1.5x");
        let block = reader.feed_line("progress=continue");
        assert!(block.is_some());
        let b = block.unwrap();
        assert_eq!(b.get("frame"), Some(&"100".to_string()));
    }
}
