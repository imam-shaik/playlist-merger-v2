use crate::split::types::*;
use crate::split::{generate_plan, extract_chapters};
use crate::AppState;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{command, State, Emitter};

/// Generate a split plan (preview) without executing it.
/// Returns the plan with segment boundaries so the UI can display a preview.
#[command]
pub async fn generate_split_plan(
    request: SplitPlanRequest,
) -> Result<SplitPlan, String> {
    log::info!("[SplitCmd] generate_split_plan: mode={:?}, input={}", request.mode, request.input_file);

    let plan = generate_plan(&request)?;

    log::info!("[SplitCmd] Plan generated: {} segments, total={:.2}s", plan.segments.len(), plan.input_duration);

    Ok(plan)
}

/// Generate a plan based on chapter metadata or fallback merge report.
/// Probes the file for chapters first, then falls back to merge report if not found.
#[command]
pub async fn generate_chapter_split_plan(
    request: SplitPlanRequest,
) -> Result<SplitPlan, String> {
    log::info!("[SplitCmd] generate_chapter_split_plan: mode={:?}, input={}", request.mode, request.input_file);

    let settings = crate::services::settings::load_settings_internal();
    let ffprobe_path = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref())
        .map_err(|e| format!("ffprobe not found: {}", e))?;

    let chapters_result = extract_chapters(&ffprobe_path, Path::new(&request.input_file));
    let mut chapters = match chapters_result {
        Ok(c) => c,
        Err(e) => {
            log::info!("[SplitCmd] Probing chapters failed: {}. Falling back to merge report.", e);
            let video_path = Path::new(&request.input_file);
            let stem = video_path.file_stem().and_then(|s| s.to_str()).unwrap_or("merged");
            let parent = video_path.parent().unwrap_or(Path::new(""));
            let report_path = parent.join(format!("{}_report.txt", stem));
            
            if report_path.exists() {
                parse_report_file(&report_path)?
            } else {
                return Err(format!(
                    "No chapters found in video file, and no merge report found at {:?}",
                    report_path
                ));
            }
        }
    };

    if chapters.is_empty() {
        return Err("No chapters or merge report entries available to split".to_string());
    }

    // Chronologically sort and correct any overlapping boundaries
    let overlap_warnings = correct_overlapping_chapters(&mut chapters);
    for warn in overlap_warnings {
        log::warn!("[SplitCmd] {}", warn);
    }

    let is_playlist_mode = request.mode == SplitMode::ByPlaylistItems;
    let custom_ranges: Vec<f64>;
    let mut segment_labels = Vec::new();

    if is_playlist_mode {
        let items_per_seg = request.params.items_per_segment.unwrap_or(10) as usize;
        if items_per_seg == 0 {
            return Err("Items per segment must be greater than 0".to_string());
        }

        let mut ranges = Vec::new();
        let chunks = chapters.chunks(items_per_seg);
        for (idx, chunk) in chunks.enumerate() {
            let first_chapter = &chunk[0];
            let last_chapter = &chunk[chunk.len() - 1];
            ranges.push(first_chapter.1); // start time of first chapter in chunk
            ranges.push(last_chapter.2);  // end time of last chapter in chunk
            segment_labels.push(format!("Batch {}", idx + 1));
        }
        custom_ranges = ranges;
    } else {
        // ByChapters mode
        custom_ranges = chapters
            .iter()
            .flat_map(|(_, start, end)| vec![*start, *end])
            .collect();
        
        for ch in &chapters {
            segment_labels.push(ch.0.clone());
        }
    }

    let mut chapter_params = request.params.clone();
    chapter_params.custom_ranges = Some(custom_ranges);
    chapter_params.label_prefix = Some(if is_playlist_mode { "Batch".to_string() } else { "Chapter".to_string() });

    let chapter_request = SplitPlanRequest {
        mode: SplitMode::CustomRanges,
        params: chapter_params,
        ..request
    };

    let plan = generate_plan(&chapter_request)?;

    // Apply resolved titles as segment labels
    let segments: Vec<SplitSegment> = plan
        .segments
        .into_iter()
        .enumerate()
        .map(|(i, seg)| {
            let title = segment_labels
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("Part {}", i + 1));
            SplitSegment {
                label: title,
                ..seg
            }
        })
        .collect();

    Ok(SplitPlan {
        segments,
        mode: request.mode.clone(),
        ..plan
    })
}

/// Parse a duration in format "HH:MM:SS" into seconds.
fn parse_duration_hhmmss(s: &str) -> Result<f64, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("Invalid duration format: '{}'", s));
    }
    let h: f64 = parts[0].parse().map_err(|_| format!("Invalid hours: '{}'", parts[0]))?;
    let m: f64 = parts[1].parse().map_err(|_| format!("Invalid minutes: '{}'", parts[1]))?;
    let s_val: f64 = parts[2].parse().map_err(|_| format!("Invalid seconds: '{}'", parts[2]))?;
    Ok(h * 3600.0 + m * 60.0 + s_val)
}

/// Parse a merge report file to reconstruct the segment boundaries.
pub fn parse_report_file(report_path: &Path) -> Result<Vec<(String, f64, f64)>, String> {
    let content = std::fs::read_to_string(report_path)
        .map_err(|e| format!("Failed to read report file: {}", e))?;

    // Detect version
    let mut version = 1;
    for line in content.lines() {
        if line.contains("Report Version") {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 2 {
                if let Ok(v) = parts[1].trim().parse::<u32>() {
                    version = v;
                    log::info!("[SplitCmd] Detected report version: {}", version);
                    break;
                }
            }
        }
    }

    if version > 2 {
        log::warn!("[SplitCmd] Warning: Report version {} is newer than supported version 2. Attempting to parse.", version);
    }

    let mut entries = Vec::new();
    for line in content.lines() {
        if !line.contains('│') {
            continue;
        }

        let parts: Vec<&str> = line.split('│').collect();
        if parts.len() < 5 {
            continue;
        }

        let index_str = parts[1].trim();
        let name_str = parts[2].trim();
        let times_str = parts[4].trim(); // e.g. "00:00:00 → 00:05:00"

        // Skip headers and other metadata lines
        let Ok(_) = index_str.parse::<u32>() else {
            continue;
        };

        let time_separator = if times_str.contains('→') {
            '→'
        } else if times_str.contains("->") {
            '-'
        } else {
            continue;
        };

        let time_parts: Vec<&str> = if time_separator == '-' {
            times_str.split("->").collect()
        } else {
            times_str.split('→').collect()
        };

        if time_parts.len() != 2 {
            log::warn!("[SplitCmd] Skipping malformed time range line: {}", line);
            continue;
        }

        let start_res = parse_duration_hhmmss(time_parts[0].trim());
        let end_res = parse_duration_hhmmss(time_parts[1].trim());

        match (start_res, end_res) {
            (Ok(start), Ok(end)) => {
                let clean_name = name_str
                    .trim_start_matches(|c: char| !c.is_alphanumeric() && c != '[' && c != '(')
                    .trim()
                    .to_string();
                entries.push((clean_name, start, end));
            }
            _ => {
                log::warn!("[SplitCmd] Skipping line with invalid timestamps: {}", line);
            }
        }
    }

    if entries.is_empty() {
        return Err("No valid entries parsed from merge report".to_string());
    }

    log::info!("[SplitCmd] Parsed {} entries from merge report", entries.len());
    Ok(entries)
}

/// Chronologically sort and correct overlapping chapters to prevent duplicate content cuts.
pub fn correct_overlapping_chapters(chapters: &mut Vec<(String, f64, f64)>) -> Vec<String> {
    let mut warnings = Vec::new();
    if chapters.len() < 2 {
        return warnings;
    }

    chapters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut i = 0;
    while i < chapters.len() - 1 {
        let current_end = chapters[i].2;
        let next_start = chapters[i+1].1;

        if current_end > next_start {
            let title_curr = chapters[i].0.clone();
            let title_next = chapters[i+1].0.clone();
            warnings.push(format!(
                "Chapter '{}' overlaps with '{}'. Correcting end time from {:.2}s to {:.2}s.",
                title_curr, title_next, current_end, next_start
            ));

            chapters[i].2 = next_start;

            if chapters[i].2 <= chapters[i].1 {
                warnings.push(format!(
                    "Chapter '{}' has zero or negative duration after correction and was removed.",
                    title_curr
                ));
                chapters.remove(i);
                continue;
            }
        }
        i += 1;
    }
    warnings
}

/// Execute a split plan.
#[command]
pub async fn execute_split_plan(
    request: SplitExecuteRequest,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<SplitResult, String> {
    let job_id = request.plan.job_id.clone();
    log::info!("[SplitCmd] execute_split_plan: job_id={}, segments={}", job_id, request.plan.segments.len());

    // Per-Job Log Capture for split operations
    let _job_log_path = {
        let output_dir = &request.plan.output_dir;
        let job_name = std::path::Path::new(&request.plan.input_file)
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "split_output".to_string());
        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // ── Forensic Log Start ──────────────────────────────────────────────
    // Purely observational — writes a structured log to Desktop/PlaylistMerger Logs/
    match crate::forensic_log::start_forensic_log(
        &request.plan.output_dir,
        "split",
        &job_id,
        None, // playlist name not available in split context
        "split",
        &request.plan.output_dir,
        None,
        None,
        request.plan.segments.len(),
    ) {
        Ok(path) => {
            log::info!("[Split] Forensic log started at: {}", path.display());
        }
        Err(e) => {
            log::error!("[Split] FORENSIC LOG FAILED TO START: {}", e);
            // Don't fail the merge just because forensic log failed
        }
    }

    // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
    let _log_guard = crate::logger::JobLogGuard::new_empty();

    let settings = crate::services::settings::load_settings_internal();
    let ffmpeg_path = crate::ffmpeg::find_ffmpeg(settings.ffmpeg_path.as_deref())
        .map_err(|e| format!("ffmpeg not found: {}", e))?;
    let ffprobe_path = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref())
        .map_err(|e| format!("ffprobe not found: {}", e))?;

    // Register cancel flag in the same merge_state active_jobs map
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let mut ms = state.merge_state.lock().await;
        ms.active_jobs.insert(job_id.clone(), cancel_flag.clone());
    }

    let app_handle_clone = app_handle.clone();
    let progress_callback = move |progress: SplitProgress| {
        log::info!(
            "[SplitCmd] Progress: job={}, seg={}/{}, {:.0}%, stage={}",
            progress.job_id,
            progress.segment_index,
            progress.segment_count,
            progress.progress,
            progress.stage
        );
        let _ = app_handle_clone.emit("split-progress", &serde_json::to_value(&progress).unwrap_or_default());
    };

    let result = match crate::split::execute_split_with_options(
        &ffmpeg_path,
        Some(&ffprobe_path),
        &request.plan,
        request.subtitle_mode,
        request.export_srt,
        cancel_flag.clone(),
        progress_callback,
    ) {
        Ok(r) => r,
        Err(e) => {
            let _ = app_handle.emit("split-error", serde_json::json!({
                "jobId": job_id,
                "error": e,
            }));
            {
                let mut ms = state.merge_state.lock().await;
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(&e), Some(&request.plan.output_dir));
                crate::logger::stop_job_log(Some("[JOB_ERROR] Split failed"));
                ms.active_jobs.remove(&job_id);
            }
            return Err(e);
        }
    };

    // Cleanup cancel flag
    {
        let mut ms = state.merge_state.lock().await;
        crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.plan.output_dir));
        crate::logger::stop_job_log(Some("[JOB_COMPLETE] Split completed"));
        ms.active_jobs.remove(&job_id);
    }

    log::info!("[SplitCmd] Split complete: {} files created", result.output_paths.len());

    let _ = app_handle.emit("split-complete", serde_json::json!({
        "jobId": job_id,
        "outputPaths": result.output_paths,
        "totalSegments": result.output_paths.len(),
    }));

    Ok(result)
}

/// Cancel a running split job.
#[command]
pub async fn cancel_split(
    job_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    log::info!("[SplitCmd] cancel_split: job_id={}", job_id);

    let ms = state.merge_state.lock().await;
    if let Some(flag) = ms.active_jobs.get(&job_id) {
        flag.store(true, Ordering::Relaxed);
        log::info!("[SplitCmd] Cancel flag set for job {}", job_id);
        Ok(())
    } else {
        Err(format!("No active split job found with id: {}", job_id))
    }
}
