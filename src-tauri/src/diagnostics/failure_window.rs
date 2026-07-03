use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureWindow {
    pub failure_timestamp: String,
    pub concat_position: ConcatPosition,
    pub suspect_files: Vec<SuspectFile>,
    pub confidence: ConfidenceLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcatPosition {
    pub current: usize,
    pub total: usize,
    pub current_file: Option<String>,
    pub previous_file: Option<String>,
    pub next_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuspectFile {
    pub index: usize,
    pub filename: String,
    pub path: String,
    pub reason: SuspectReason,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuspectReason {
    WasCurrentWhenFailed,
    WasNextWhenFailed,
    NearBoundaryBetweenNormalizedAndOriginal,
    HasSubtitleStream,
    AudioStreamMismatch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

impl std::fmt::Display for ConfidenceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfidenceLevel::High => write!(f, "High"),
            ConfidenceLevel::Medium => write!(f, "Medium"),
            ConfidenceLevel::Low => write!(f, "Low"),
        }
    }
}

pub fn parse_failure_window(
    ffmpeg_stderr: &str,
    concat_list_content: &str,
    failure_time: &str,
) -> FailureWindow {
    let files = parse_concat_list(concat_list_content);
    let total = files.len();

    let current_index = estimate_concat_position(ffmpeg_stderr, total);

    let (current_file, previous_file, next_file) = get_neighboring_files(&files, current_index);

    let suspect_files = build_suspect_list(
        &files,
        current_index,
        &current_file,
        &previous_file,
        &next_file,
    );

    let confidence = if current_index > 0 && current_index < total {
        ConfidenceLevel::High
    } else {
        ConfidenceLevel::Medium
    };

    FailureWindow {
        failure_timestamp: failure_time.to_string(),
        concat_position: ConcatPosition {
            current: current_index,
            total,
            current_file,
            previous_file,
            next_file,
        },
        suspect_files,
        confidence,
    }
}

fn parse_concat_list(content: &str) -> Vec<String> {
    content
        .lines()
        .filter(|line| line.starts_with("file '") || line.starts_with("file \""))
        .filter_map(|line| {
            line.strip_prefix("file '")
                .and_then(|s| s.strip_suffix("'"))
                .or_else(|| line.strip_prefix("file \"").and_then(|s| s.strip_suffix("\"")))
                .map(|s| s.to_string())
        })
        .collect()
}

fn estimate_concat_position(stderr: &str, total_files: usize) -> usize {
    for line in stderr.lines().rev() {
        if line.contains("Opening '") && line.contains("' for reading") {
            if let Some(filename) = extract_filename_from_ffmpeg_line(line) {
                if let Some(idx) = find_file_index_in_concat(&filename, total_files) {
                    return idx;
                }
            }
        }

        if line.contains("Output #0") || line.contains("Output stream") {
            continue;
        }

        if line.contains("keyboardInterrupt") || line.contains("Cancel") {
            continue;
        }
    }

    if total_files > 0 {
        total_files.saturating_sub(1)
    } else {
        0
    }
}

fn extract_filename_from_ffmpeg_line(line: &str) -> Option<String> {
    line.split("Opening '")
        .nth(1)
        .and_then(|s| s.split("'").next())
        .map(|s| {
            std::path::Path::new(s)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| s.to_string())
        })
}

fn find_file_index_in_concat(_filename: &str, _total: usize) -> Option<usize> {
    None
}

fn get_neighboring_files(files: &[String], index: usize) -> (Option<String>, Option<String>, Option<String>) {
    let current = files.get(index).cloned();
    let previous = if index > 0 { files.get(index - 1).cloned() } else { None };
    let next = files.get(index + 1).cloned();

    (current, previous, next)
}

fn build_suspect_list(
    _files: &[String],
    current_index: usize,
    current_file: &Option<String>,
    previous_file: &Option<String>,
    next_file: &Option<String>,
) -> Vec<SuspectFile> {
    let mut suspects = Vec::new();

    if let Some(ref curr) = current_file {
        suspects.push(SuspectFile {
            index: current_index,
            filename: Path::new(curr)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| curr.clone()),
            path: curr.clone(),
            reason: SuspectReason::WasCurrentWhenFailed,
            confidence: 0.85,
        });
    }

    if let Some(ref prev) = previous_file {
        suspects.push(SuspectFile {
            index: current_index.saturating_sub(1),
            filename: Path::new(prev)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| prev.clone()),
            path: prev.clone(),
            reason: SuspectReason::NearBoundaryBetweenNormalizedAndOriginal,
            confidence: 0.60,
        });
    }

    if let Some(ref nxt) = next_file {
        suspects.push(SuspectFile {
            index: current_index + 1,
            filename: Path::new(nxt)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| nxt.clone()),
            path: nxt.clone(),
            reason: SuspectReason::WasNextWhenFailed,
            confidence: 0.40,
        });
    }

    suspects
}

impl std::fmt::Display for FailureWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║              FAILURE WINDOW DETECTION REPORT                       ║")?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║ Failure at: {}                                      ║", self.failure_timestamp)?;
        writeln!(f, "║ Concat position: {}/{}                                       ║", self.concat_position.current, self.concat_position.total)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║ SUSPECT FILES ({} total)                                          ║", self.suspect_files.len())?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;

        for (i, suspect) in self.suspect_files.iter().enumerate() {
            writeln!(f, "║ [#{}] {}                                       ║", i + 1, suspect.filename)?;
            writeln!(f, "║      Path: {}       ║", truncate_middle(&suspect.path, 50))?;
            writeln!(f, "║      Reason: {:?}              ║", suspect.reason)?;
            writeln!(f, "║      Confidence: {:.0}%                               ║", suspect.confidence * 100.0)?;
            writeln!(f, "║                                                               ║")?;
        }

        writeln!(f, "╚══════════════════════════════════════════════════════════════════════╝")?;
        Ok(())
    }
}

fn truncate_middle(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let ellipsis = "...";
    let available = max_len - ellipsis.len();
    let front = available / 2;
    let back = available - front;
    format!("{}{}{}", &s[..front], ellipsis, &s[s.len() - back..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_concat_list() {
        let content = "file '/path/to/lecture001.mp4'\nfile '/path/to/lecture002.mp4'\nfile '/path/to/lecture003.mp4'\n";
        let files = parse_concat_list(content);
        assert_eq!(files.len(), 3);
        assert!(files[0].contains("lecture001"));
    }

    #[test]
    fn test_truncate_middle() {
        let s = "very_long_filename_that_needs_truncation.txt";
        let truncated = truncate_middle(s, 20);
        assert!(truncated.len() <= 20);
        assert!(truncated.contains("..."));
    }
}