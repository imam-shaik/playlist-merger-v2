use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamProperties {
    pub file_index: usize,
    pub file_path: String,
    pub codec: String,
    pub codec_type: String,
    pub time_base: String,
    pub start_time: Option<f64>,
    pub duration: Option<f64>,
    pub bit_rate: Option<String>,
    pub profile: Option<String>,
    pub level: Option<i64>,
    pub field_order: Option<String>,
    pub color_space: Option<String>,
    pub color_range: Option<String>,
    pub extradata_size: Option<i64>,
    pub extradata_hash: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub sample_rate: Option<String>,
    pub channels: Option<i64>,
    pub channel_layout: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistEquivalenceReport {
    pub files: Vec<StreamProperties>,
    pub stream_groups: HashMap<String, Vec<usize>>,
    pub mismatches: Vec<EquivalenceMismatch>,
    pub overall_equivalent: bool,
    pub confidence_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquivalenceMismatch {
    pub property: String,
    pub file_a_index: usize,
    pub file_b_index: usize,
    pub file_a_value: String,
    pub file_b_value: String,
    pub severity: String,
}

pub fn certify_playlist_equivalence(
    ffprobe_path: &Path,
    files: &[String],
) -> Result<PlaylistEquivalenceReport, String> {
    let mut all_properties = Vec::new();
    let mut mismatches = Vec::new();

    for (i, file) in files.iter().enumerate() {
        match get_stream_properties(ffprobe_path, file, i) {
            Ok(props) => all_properties.push(props),
            Err(e) => {
                return Err(format!("Failed to probe file {}: {}", file, e));
            }
        }
    }

    let mut stream_groups: HashMap<String, Vec<usize>> = HashMap::new();
    for props in &all_properties {
        let key = format!("{}:{}:{}",
            props.codec_type,
            props.codec,
            props.time_base
        );
        stream_groups.entry(key).or_insert_with(Vec::new).push(props.file_index);
    }

    for i in 0..all_properties.len() {
        for j in (i + 1)..all_properties.len() {
            let file_a = &all_properties[i];
            let file_b = &all_properties[j];

            let critical_mismatches = check_equivalence(file_a, file_b);
            mismatches.extend(critical_mismatches);
        }
    }

    let overall_equivalent = mismatches.is_empty();

    let confidence_score = if overall_equivalent {
        0.97
    } else {
        let critical_count = mismatches.iter()
            .filter(|m| m.severity == "CRITICAL")
            .count();
        if critical_count > 0 {
            0.70
        } else {
            0.85
        }
    };

    Ok(PlaylistEquivalenceReport {
        files: all_properties,
        stream_groups,
        mismatches,
        overall_equivalent,
        confidence_score,
    })
}

fn check_equivalence(a: &StreamProperties, b: &StreamProperties) -> Vec<EquivalenceMismatch> {
    let mut mismatches = Vec::new();

    let critical_checks: Vec<(&str, String, String)> = vec![
        ("time_base", a.time_base.clone(), b.time_base.clone()),
        ("codec", a.codec.clone(), b.codec.clone()),
        ("extradata_hash",
            a.extradata_hash.clone().unwrap_or_default(),
            b.extradata_hash.clone().unwrap_or_default()),
    ];

    for (prop, val_a, val_b) in critical_checks {
        if val_a != val_b {
            mismatches.push(EquivalenceMismatch {
                property: prop.to_string(),
                file_a_index: a.file_index,
                file_b_index: b.file_index,
                file_a_value: val_a,
                file_b_value: val_b,
                severity: "CRITICAL".to_string(),
            });
        }
    }

    let warning_checks: Vec<(&str, Option<String>, Option<String>)> = vec![
        ("start_time", a.start_time.map(|s| s.to_string()), b.start_time.map(|s| s.to_string())),
        ("duration", a.duration.map(|s| s.to_string()), b.duration.map(|s| s.to_string())),
        ("profile", a.profile.clone(), b.profile.clone()),
        ("level", a.level.map(|s| s.to_string()), b.level.map(|s| s.to_string())),
        ("field_order", a.field_order.clone(), b.field_order.clone()),
        ("color_space", a.color_space.clone(), b.color_space.clone()),
        ("color_range", a.color_range.clone(), b.color_range.clone()),
    ];

    for (prop, val_a, val_b) in warning_checks {
        let val_a_str = val_a.unwrap_or_default();
        let val_b_str = val_b.unwrap_or_default();
        if !val_a_str.is_empty() && !val_b_str.is_empty() && val_a_str != val_b_str {
            mismatches.push(EquivalenceMismatch {
                property: prop.to_string(),
                file_a_index: a.file_index,
                file_b_index: b.file_index,
                file_a_value: val_a_str,
                file_b_value: val_b_str,
                severity: "WARNING".to_string(),
            });
        }
    }

    mismatches
}

fn get_stream_properties(
    ffprobe_path: &Path,
    file_path: &str,
    file_index: usize,
) -> Result<StreamProperties, String> {
    let args = [
        "-v", "quiet",
        "-print_format", "json",
        "-show_streams",
        "-show_format",
        file_path,
    ];

    let output = Command::new(ffprobe_path)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Err(format!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    let streams = json.get("streams")
        .and_then(|s| s.as_array())
        .ok_or_else(|| "No streams in ffprobe output".to_string())?;

    let video_stream = streams.iter().find(|s| {
        s.get("codec_type").and_then(|c| c.as_str()) == Some("video")
    });

    let audio_stream = streams.iter().find(|s| {
        s.get("codec_type").and_then(|c| c.as_str()) == Some("audio")
    });

    let stream = video_stream.or(audio_stream).ok_or_else(|| "No video or audio stream".to_string())?;

    let codec = stream.get("codec_name").and_then(|c| c.as_str()).unwrap_or("unknown").to_string();
    let codec_type = stream.get("codec_type").and_then(|c| c.as_str()).unwrap_or("unknown").to_string();
    let time_base = stream.get("time_base").and_then(|t| t.as_str()).unwrap_or("0/1").to_string();
    let start_time = json.get("format")
        .and_then(|f| f.get("start_time"))
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<f64>().ok());
    let duration = json.get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<f64>().ok());
    let bit_rate = json.get("format")
        .and_then(|f| f.get("bit_rate"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    let profile = stream.get("profile").and_then(|p| p.as_str()).map(|s| s.to_string());
    let level = stream.get("level").and_then(|l| l.as_i64());

    let field_order = stream.get("field_order").and_then(|f| f.as_str()).map(|s| s.to_string());
    let color_space = stream.get("color_space").and_then(|c| c.as_str()).map(|s| s.to_string());
    let color_range = stream.get("color_range").and_then(|c| c.as_str()).map(|s| s.to_string());

    let extradata_size = stream.get("extradata_size").and_then(|e| e.as_i64());
    let extradata_hash = stream.get("extradata_hash").and_then(|e| e.as_str()).map(|s| s.to_string());

    let width = stream.get("width").and_then(|w| w.as_i64());
    let height = stream.get("height").and_then(|h| h.as_i64());

    let sample_rate = stream.get("sample_rate").and_then(|s| s.as_str()).map(|s| s.to_string());
    let channels = stream.get("channels").and_then(|c| c.as_i64());
    let channel_layout = stream.get("channel_layout").and_then(|c| c.as_str()).map(|s| s.to_string());

    Ok(StreamProperties {
        file_index,
        file_path: file_path.to_string(),
        codec,
        codec_type,
        time_base,
        start_time,
        duration,
        bit_rate,
        profile,
        level,
        field_order,
        color_space,
        color_range,
        extradata_size,
        extradata_hash,
        width,
        height,
        sample_rate,
        channels,
        channel_layout,
    })
}

impl std::fmt::Display for PlaylistEquivalenceReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║           PLAYLIST EQUIVALENCE CERTIFICATION                             ║")?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════════════╝")?;
        writeln!(f)?;

        writeln!(f, "OVERALL: {}", if self.overall_equivalent { "✅ EQUIVALENT" } else { "❌ MISMATCHES FOUND" })?;
        writeln!(f, "Confidence: {:.0}%", self.confidence_score * 100.0)?;
        writeln!(f, "Files analyzed: {}", self.files.len())?;
        writeln!(f)?;

        writeln!(f, "STREAM GROUPS (files sharing same codec/time_base):")?;
        for (key, indices) in &self.stream_groups {
            writeln!(f, "  {} → Files: {:?}", key, indices)?;
        }
        writeln!(f)?;

        if self.mismatches.is_empty() {
            writeln!(f, "MISMATCHES: None")?;
        } else {
            writeln!(f, "MISMATCHES:")?;
            let critical: Vec<_> = self.mismatches.iter().filter(|m| m.severity == "CRITICAL").collect();
            let warnings: Vec<_> = self.mismatches.iter().filter(|m| m.severity == "WARNING").collect();

            if !critical.is_empty() {
                writeln!(f, "  CRITICAL ({}):", critical.len())?;
                for m in critical {
                    writeln!(f, "    • {}: File {} vs File {} → '{}' vs '{}'",
                        m.property, m.file_a_index, m.file_b_index, m.file_a_value, m.file_b_value)?;
                }
            }

            if !warnings.is_empty() {
                writeln!(f, "  WARNINGS ({}):", warnings.len())?;
                for m in warnings {
                    writeln!(f, "    • {}: File {} vs File {} → '{}' vs '{}'",
                        m.property, m.file_a_index, m.file_b_index, m.file_a_value, m.file_b_value)?;
                }
            }
        }

        Ok(())
    }
}

pub fn compare_extradata_hash(
    ffprobe_path: &Path,
    original_path: &str,
    normalized_path: &str,
) -> Result<(Option<String>, Option<String>, bool), String> {
    let original_hash = get_extradata_hash(ffprobe_path, original_path)?;
    let normalized_hash = get_extradata_hash(ffprobe_path, normalized_path)?;

    let changed = original_hash.is_some() && normalized_hash.is_some()
        && original_hash != normalized_hash;

    Ok((original_hash, normalized_hash, changed))
}

fn get_extradata_hash(ffprobe_path: &Path, file_path: &str) -> Result<Option<String>, String> {
    let args = [
        "-v", "quiet",
        "-print_format", "json",
        "-show_streams",
        file_path,
    ];

    let output = Command::new(ffprobe_path)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Ok(None);
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    let streams = json.get("streams")
        .and_then(|s| s.as_array())
        .ok_or_else(|| "No streams".to_string())?;

    for stream in streams {
        if let Some(hash) = stream.get("extradata_hash").and_then(|h| h.as_str()) {
            return Ok(Some(hash.to_string()));
        }
        if let Some(size) = stream.get("extradata_size").and_then(|s| s.as_i64()) {
            if size > 0 {
                return Ok(Some(format!("size={}", size)));
            }
        }
    }

    Ok(None)
}