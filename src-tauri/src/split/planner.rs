use crate::split::types::*;

/// Generate a split plan based on user parameters.
pub fn generate_plan(request: &SplitPlanRequest) -> Result<SplitPlan, String> {
    let total_duration = request.input_duration;
    if total_duration <= 0.0 {
        return Err("Input video has zero or unknown duration".to_string());
    }

    let mode = &request.mode;
    let params = &request.params;
    let output_format = params
        .output_format
        .clone()
        .unwrap_or_else(|| detect_format(&request.input_file));

    let segments = match mode {
        SplitMode::ByParts => plan_by_parts(total_duration, params),
        SplitMode::ByDuration => plan_by_duration(total_duration, params),
        SplitMode::ByChapters => {
            // Chapters require probing the file — handled at command level
            return Err("ByChapters mode requires chapter metadata probing first. Use the dedicated command.".to_string());
        }
        SplitMode::CustomRanges => plan_custom_ranges(total_duration, params),
        SplitMode::ByPlaylistItems => {
            return Err("ByPlaylistItems mode requires playlist item info. Use the dedicated command.".to_string());
        }
        SplitMode::ByOutputSize => plan_by_output_size(total_duration, request.input_size_bytes, params),
        SplitMode::SmartCourse => plan_smart_course(total_duration, params),
    }?;

    if segments.is_empty() {
        return Err("No segments generated for the given parameters".to_string());
    }

    let label_prefix = params
        .label_prefix
        .clone()
        .unwrap_or_else(|| match mode {
            SplitMode::ByParts => "Part".to_string(),
            SplitMode::ByDuration => "Chunk".to_string(),
            SplitMode::ByChapters => "Chapter".to_string(),
            SplitMode::CustomRanges => "Segment".to_string(),
            SplitMode::ByPlaylistItems => "Batch".to_string(),
            SplitMode::ByOutputSize => "Part".to_string(),
            SplitMode::SmartCourse => {
                match params.course_mode.as_deref() {
                    Some("daily") => "Day".to_string(),
                    Some("weekly") => "Week".to_string(),
                    _ => "Lesson".to_string(),
                }
            }
        });

    let segments: Vec<SplitSegment> = segments
        .into_iter()
        .enumerate()
        .map(|(i, (start, end, label_override))| {
            let duration = end - start;
            // Use label_override (set by planner functions) or auto-generate with correct index
            let label = label_override.unwrap_or_else(|| {
                format!("{} {}", label_prefix, i + 1)
            });
            SplitSegment {
                index: i + 1,
                label,
                start_time: start,
                end_time: end,
                duration,
                estimated_size_bytes: None,
            }
        })
        .map(|mut seg| {
            // Estimate size proportionally
            if request.input_size_bytes > 0 && total_duration > 0.0 {
                seg.estimated_size_bytes = Some(
                    ((seg.duration / total_duration) * request.input_size_bytes as f64) as u64,
                );
            }
            seg
        })
        .collect();

    // Validate total duration matches
    let total_segment_duration: f64 = segments.iter().map(|s| s.duration).sum();
    let diff = (total_segment_duration - total_duration).abs();
    if diff > 1.0 {
        log::warn!(
            "[SplitPlanner] Segment total duration ({:.2}s) differs from input ({:.2}s) by {:.2}s",
            total_segment_duration,
            total_duration,
            diff
        );
    }

    let output_dir = if request.output_dir.is_empty() {
        std::path::Path::new(&request.input_file)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        request.output_dir.clone()
    };

    Ok(SplitPlan {
        job_id: request.job_id.clone(),
        input_file: request.input_file.clone(),
        input_duration: total_duration,
        input_size_bytes: request.input_size_bytes,
        mode: mode.clone(),
        segments,
        output_dir,
        output_format,
        label_suffix: params.label_suffix.clone(),
        naming_template: params.naming_template.clone(),
        naming_config: params.naming_config.clone(),
        include_timestamp: params.include_timestamp,
    })
}

/// Plan: split into N equal-duration parts.
fn plan_by_parts(
    total_duration: f64,
    params: &SplitParams,
) -> Result<Vec<(f64, f64, Option<String>)>, String> {
    let count = params.part_count.unwrap_or(2) as usize;
    if count < 1 {
        return Err("Part count must be at least 1".to_string());
    }
    if count as f64 > total_duration {
        return Err(format!(
            "Cannot split {:.0}s video into {} parts (more parts than seconds)",
            total_duration, count
        ));
    }

    let part_duration = total_duration / count as f64;
    let mut segments = Vec::with_capacity(count);

    for i in 0..count {
        let start = i as f64 * part_duration;
        let end = if i == count - 1 {
            total_duration
        } else {
            (i + 1) as f64 * part_duration
        };
        let label = Some(format!("Part {}", i + 1));
        segments.push((start, end, label));
    }

    Ok(segments)
}

/// Plan: split into fixed-duration chunks.
fn plan_by_duration(
    total_duration: f64,
    params: &SplitParams,
) -> Result<Vec<(f64, f64, Option<String>)>, String> {
    let chunk_duration = params.part_duration.unwrap_or(3600.0); // default 1 hour
    if chunk_duration <= 0.0 {
        return Err("Chunk duration must be positive".to_string());
    }

    let count = (total_duration / chunk_duration).ceil() as usize;
    let mut segments = Vec::with_capacity(count);

    for i in 0..count {
        let start = i as f64 * chunk_duration;
        let end = ((i + 1) as f64 * chunk_duration).min(total_duration);
        let label = Some(format!("Chunk {}", i + 1));
        segments.push((start, end, label));
    }

    Ok(segments)
}

/// Plan: use user-defined time ranges.
fn plan_custom_ranges(
    total_duration: f64,
    params: &SplitParams,
) -> Result<Vec<(f64, f64, Option<String>)>, String> {
    let ranges = params
        .custom_ranges
        .as_ref()
        .ok_or_else(|| "Custom ranges not provided".to_string())?;

    if ranges.len() < 2 || ranges.len() % 2 != 0 {
        return Err(format!(
            "Custom ranges must be pairs of [start, end] values. Got {} values.",
            ranges.len()
        ));
    }

    let mut segments = Vec::new();
    for chunk in ranges.chunks(2) {
        let start = chunk[0].max(0.0);
        let end = chunk[1].min(total_duration);
        if end <= start {
            return Err(format!(
                "Invalid range: start ({:.2}s) >= end ({:.2}s)",
                start, end
            ));
        }
        let label = Some(format!("Segment {}", segments.len() + 1));
        segments.push((start, end, label));
    }

    Ok(segments)
}

/// Plan: split by max output file size.
/// Estimates bitrate from total size/duration, then calculates segment count.
fn plan_by_output_size(
    total_duration: f64,
    total_size_bytes: u64,
    params: &SplitParams,
) -> Result<Vec<(f64, f64, Option<String>)>, String> {
    let max_size = params.max_size_bytes.unwrap_or(4_000_000_000) as f64; // default 4GB
    if max_size <= 0.0 {
        return Err("Max output size must be positive".to_string());
    }

    // If we don't know the input size, fall back to duration-based estimate
    if total_size_bytes == 0 {
        // Assume ~10MB per minute as rough estimate
        let estimated_bitrate = 10_000_000.0 * 60.0; // 10MB per minute
        let max_duration = max_size / estimated_bitrate;
        return plan_by_duration(total_duration, &SplitParams {
            part_duration: Some(max_duration),
            ..Default::default()
        });
    }

    let avg_bitrate = total_size_bytes as f64 / total_duration; // bytes per second
    let max_duration_per_segment = max_size / avg_bitrate;
    let count = (total_duration / max_duration_per_segment).ceil() as usize;
    let count = count.max(1);

    let part_duration = max_duration_per_segment;
    let mut segments = Vec::with_capacity(count);

    for i in 0..count {
        let start = i as f64 * part_duration;
        let end = ((i + 1) as f64 * part_duration).min(total_duration);
        let label = Some(format!("Part {}", i + 1));
        segments.push((start, end, label));
    }

    Ok(segments)
}

/// Plan: smart course split (daily or weekly learning chunks).
fn plan_smart_course(
    total_duration: f64,
    params: &SplitParams,
) -> Result<Vec<(f64, f64, Option<String>)>, String> {
    let unit_label = match params.course_mode.as_deref() {
        Some("daily") => "Day",
        Some("weekly") => "Week",
        _ => "Lesson",
    };
    let hours_per = params.hours_per_unit.unwrap_or(2.0); // default 2h
    let seconds_per_unit = hours_per * 3600.0;

    let count = (total_duration / seconds_per_unit).ceil() as usize;
    if count == 0 {
        return Err("Duration too short for any course unit".to_string());
    }

    let mut segments = Vec::with_capacity(count);
    for i in 0..count {
        let start = i as f64 * seconds_per_unit;
        let end = ((i + 1) as f64 * seconds_per_unit).min(total_duration);
        let label = Some(format!("{} {}", unit_label, i + 1));
        segments.push((start, end, label));
    }

    Ok(segments)
}

/// Detect output format from file extension.
fn detect_format(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp4")
        .to_lowercase()
}
