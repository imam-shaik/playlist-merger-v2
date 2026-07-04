use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use tauri::{command, State, Emitter, AppHandle};

use crate::AppState;
use crate::types::{
    FolderForMerge, PartitionMethod, SectionPlan, SectionMergeRequest,
    SectionEvent, SectionResult, SectionMeta, MergeMode,
    RecoveryCheckpoint, SubtitleMode,
};
use crate::ffmpeg::section_planner::{
    compute_section_preview, resolve_file_paths_for_section, compute_playlist_hash,
};
use crate::ffmpeg::concat::{MergeConfig, run_merge_blocking};
use crate::ffmpeg::write_concat_list_with_durations;
use crate::ffmpeg::get_temp_dir;
use crate::ffmpeg::normalization::{
    analyze_profiles, normalize_to_profile, normalize_timescale_lossless,
    normalize_audio_only, filter_outliers_for_mkv, NormalizationType, Outlier,
    EncodingProfile, AudioProfile, MergeBackend,
};
use crate::ffmpeg::norm_cache::NormalizationCache;
use crate::ffmpeg::probe_cache::probe_all_parallel;
use crate::commands::merge::check_free_disk_space;
use crate::recovery;
use crate::recovery::CheckpointSender;

#[command]
pub async fn compute_section_preview_cmd(
    folders: Vec<FolderForMerge>,
    method: PartitionMethod,
    name_template: String,
) -> Result<Vec<SectionPlan>, String> {
    log::info!("[Section] Computing section preview for {} folders", folders.len());
    compute_section_preview(folders, method, name_template)
}

#[command]
pub async fn get_playlist_hash(folders: Vec<FolderForMerge>) -> String {
    compute_playlist_hash(&folders)
}

#[command]
pub async fn start_section_merge(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: SectionMergeRequest,
) -> Result<String, String> {
    log::info!("[Section] Starting section merge with {} folders", request.folders.len());


    
    // Check for concurrent merge
    {
        let ms = state.merge_state.lock().await;
        if !ms.active_jobs.is_empty() {
            return Err("Another merge is in progress. Please wait for it to complete.".into());
        }
    }
    
    let job_id = request.job_id.clone();

    // Per-Job Log Capture for section merge operations
    let _job_log_path = {
        let output_dir = &request.config.output_base_dir;
        let job_name = "section_merge".to_string();
        crate::logger::start_job_log(output_dir, &job_name, &job_id)
    };

    // ── Forensic Log Start ──────────────────────────────────────────────
    // Purely observational — writes a structured log to Desktop/PlaylistMerger Logs/
    match crate::forensic_log::start_forensic_log(
        &request.config.output_base_dir,
        "section_merge",
        &job_id,
        None, // playlist name not available in section context
        "section",
        &request.config.output_base_dir,
        None,
        None,
        request.folders.len(),
    ) {
        Ok(path) => {
            log::info!("[Section] Forensic log started at: {}", path.display());
        }
        Err(e) => {
            log::error!("[Section] FORENSIC LOG FAILED TO START: {}", e);
            // Don't fail the merge just because forensic log failed
        }
    }

    let cancel_flag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    
    {
        let mut ms = state.merge_state.lock().await;
        ms.active_jobs.insert(job_id.clone(), cancel_flag.clone());
    }
    
    let app_handle_clone = app_handle.clone();
    let folders = request.folders.clone();
    let config = request.config.clone();
    let job_id_clone = job_id.clone();
    
    // Clone state for use in async block
    let state_merge_state = state.merge_state.clone();
    
    tokio::spawn(async move {
        // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
        let _log_guard = crate::logger::JobLogGuard::new_empty();
        let result = run_section_merge_impl(
            &app_handle_clone,
            &job_id_clone,
            folders,
            &config,
            cancel_flag.clone(),
        ).await;
        
        {
            let mut ms = state_merge_state.lock().await;
            // P0 FIX: Report actual result status, not hardcoded Success.
            // Previously this always reported Success even when the merge failed.
            match &result {
                Ok(_) => crate::forensic_log::end_forensic_log(
                    crate::forensic_log::ForensicStatus::Success, None, Some(&request.config.output_base_dir)),
                Err(e) => crate::forensic_log::end_forensic_log(
                    crate::forensic_log::ForensicStatus::Failed, Some(e), Some(&request.config.output_base_dir)),
            }
            crate::logger::stop_job_log(Some("[JOB_COMPLETE] Section merge completed"));
            ms.active_jobs.remove(&job_id_clone);
        }
        
        match result {
            Ok(results) => {
                let success_count = results.iter().filter(|r| r.success).count();
                let total_count = results.len();
                
                log::info!("[Section] Section merge complete. Success: {}/{}", success_count, total_count);
                
                let _ = app_handle_clone.emit("section-merge-complete", serde_json::json!({
                    "jobId": job_id_clone,
                    "success": true,
                    "successCount": success_count,
                    "totalCount": total_count,
                    "results": results,
                }));
            }
            Err(e) => {
                log::error!("[Section] Section merge failed: {}", e);
                let _ = app_handle_clone.emit("section-merge-error", serde_json::json!({
                    "jobId": job_id_clone,
                    "error": e,
                }));
            }
        }
    });
    
    Ok(job_id)
}

async fn run_section_merge_impl(
    app_handle: &AppHandle,
    job_id: &str,
    folders: Vec<FolderForMerge>,
    config: &crate::types::SectionMergeConfig,
    cancel_flag: Arc<AtomicBool>,
) -> Result<Vec<SectionResult>, String> {
    let app_data = recovery::get_app_data_dir().map_err(|e| e.to_string())?;
    let sections_dir = PathBuf::from(&config.output_base_dir).join(&config.output_subfolder);

    let mut checkpoint_writer: Option<recovery::CheckpointWriterGuard> =
        Some(recovery::spawn_checkpoint_writer(job_id.to_string(), app_data.clone()));
    let checkpoint_sender: Option<CheckpointSender> = checkpoint_writer.as_ref().map(|g| g.sender());

    // ── Phase 3: Resume Section Merge ──
    let (mut section_results, plans, total_sections, start_idx) =
        match try_resume_section_merge(&app_data, job_id)? {
            Some(data) => {
                log::info!("[Section] Resuming from section {}/{} ({} completed)",
                    data.3 + 1, data.2, data.0.len());
                if data.3 >= data.2 as usize {
                    if let Some(ref sender) = checkpoint_sender {
                        sender.update_phase(crate::types::MergePhase::Complete, None);
                    }
                    drop(checkpoint_writer);
                    return Ok(data.0);
                }
                data
            }
            None => {
                log::info!("[Section] Fresh start: computing plans");
                let fresh_plans = compute_section_preview(
                    folders.clone(),
                    config.method.clone(),
                    config.name_template.clone(),
                )?;
                let total = fresh_plans.len() as u32;

                std::fs::create_dir_all(&sections_dir)
                    .map_err(|e| format!("Failed to create output directory: {}", e))?;

                let started_at = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);

                let checkpoint = RecoveryCheckpoint {
                    version: 3,
                    job_id: job_id.to_string(),
                    phase: crate::types::MergePhase::Preparing,
                    started_at,
                    input_files: Vec::new(),
                    output_path: sections_dir.to_string_lossy().to_string(),
                    mode: "sectionMerge".to_string(),
                    dominant_profile: crate::types::DominantProfile {
                        v_codec: None, v_width: None, v_height: None, v_fps: None,
                        a_codec: None, a_sample_rate: None, a_channels: None,
                        timescale_den: None,
                    },
                    completed_files: Vec::new(),
                    remaining_indices: Vec::new(),
                    repeat_config: None,
                    original_file_count: None,
                    repeat_count: None,
                    subtitle_mode: None,
                    export_merged_srt: None,
                    selected_subtitle_stream_indices: None,
                    video_codec: None,
                    audio_codec: None,
                    video_crf: None,
                    video_preset: None,
                    audio_bitrate: None,
                    target_resolution: None,
                    target_fps: None,
                    hw_accel: None,
                    card_config: None,
                    split_config: None,
                    naming_config: None,
                    audio_repair_mode: None,
                    validate_audio: None,
                    large_playlist_strategy: None,
                    convert_to_mp4: None,
                    input_durations: None,
                    total_duration: None,
                    section_meta: Some(SectionMeta {
                        completed_sections: Vec::new(),
                        current_section: None,
                        section_plans: fresh_plans.clone(),
                        section_results: Vec::new(),
                        total_sections: total,
                    }),
                };
                recovery::write_checkpoint(&app_data, &checkpoint).map_err(|e| e.to_string())?;

                (Vec::new(), fresh_plans, total, 0usize)
            }
        };

    // Find largest section for disk space calculation
    let largest_section_size = plans.iter()
        .map(|p| p.boundary.estimated_size_bytes)
        .max()
        .unwrap_or(0);

    for (idx, plan) in plans.iter().enumerate().skip(start_idx) {
        // Check cancellation
        if cancel_flag.load(Ordering::SeqCst) {
            if let Some(ref sender) = checkpoint_sender {
                sender.update_phase(crate::types::MergePhase::Cancelled, None);
            }
            return Err("Section merge cancelled".into());
        }

        // Update current section in checkpoint
        if let Some(ref sender) = checkpoint_sender {
            sender.update_current_section(idx as u32);
        }

        // Emit section start event
        let job_id_str = job_id.to_string();
        let _ = app_handle.emit("section-start", &SectionEvent {
            job_id: job_id_str.clone(),
            current_section: idx as u32,
            total_sections,
            section_name: plan.output_name.clone(),
            phase: "starting".into(),
            progress_percent: None,
            eta_seconds: None,
            error: None,
        });

        // Disk space validation before each section
        let required_space = section_merge_disk_space_required_internal(
            plan.boundary.estimated_size_bytes,
            largest_section_size,
        );

        if let Some(parent) = sections_dir.parent() {
            if let Err(e) = check_free_disk_space(parent, required_space) {
                let error_msg = format!(
                    "Insufficient disk space for section {}: {}. Required: {:.1} GB",
                    idx + 1,
                    e,
                    required_space as f64 / 1e9
                );
                log::error!("[Section] {}", error_msg);
                
                let _ = app_handle.emit("section-error", &SectionEvent {
                    job_id: job_id_str.clone(),
                    current_section: idx as u32,
                    total_sections,
                    section_name: plan.output_name.clone(),
                    phase: "failed".into(),
                    progress_percent: None,
                    eta_seconds: None,
                    error: Some(error_msg.clone()),
                });
                
                // Mark section as failed
                section_results.push(SectionResult {
                    section_index: idx as u32,
                    output_path: sections_dir.join(format!("{}.mkv", plan.output_name)).to_string_lossy().to_string(),
                    duration_secs: 0.0,
                    size_bytes: 0,
                    success: false,
                    error_message: Some(error_msg),
                });
                
                if let Some(ref sender) = checkpoint_sender {
                    sender.update_phase(crate::types::MergePhase::Failed, None);
                }
                return Err(format!("Section {} failed: insufficient disk space", idx + 1));
            }
        }
        
        let file_paths = resolve_file_paths_for_section(&folders, &plan.boundary);
        
        if file_paths.is_empty() {
            log::warn!("[Section] Skipping section {} - no files", idx);
            continue;
        }
        
        let output_path = sections_dir.join(format!("{}.mkv", plan.output_name));
        let output_path_str = output_path.to_string_lossy().to_string();
        
        // ── Section merge execution: probe → analyze → normalize → concat ──
        let section_merge_result: Result<(Vec<PathBuf>, PathBuf), String> = async {
            let mode = match config.base_merge_mode.as_str() {
                "lossless" => MergeMode::Lossless,
                "fastMkv" => MergeMode::FastMkv,
                "smartMkv" => MergeMode::SmartMkv,
                _ => MergeMode::Custom,
            };

            if mode == MergeMode::FastMkv {
                return Err("FastMkv mode is not supported for section merge".into());
            }

            // 1. Get ffmpeg/ffprobe paths
            let settings = crate::services::settings::load_settings_internal();
            let ffmpeg_path = crate::ffmpeg::find_ffmpeg(settings.ffmpeg_path.as_deref())
                .map_err(|e| format!("Failed to find ffmpeg: {}", e))?;
            let ffprobe_path = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref())
                .map_err(|e| format!("Failed to find ffprobe: {}", e))?;
            let temp_dir = get_temp_dir()
                .map_err(|e| format!("Failed to get temp dir: {}", e))?;

            // 2. SECTION_START
            log::info!("[SECTION_START] section={} files={} mode={:?} output={}",
                idx, file_paths.len(), mode, output_path_str);

            let input_paths: Vec<&Path> = file_paths.iter().map(Path::new).collect();
            let probe_cache = probe_all_parallel(&input_paths, &ffprobe_path).await;

            // 3. Build info tuples for analyze_profiles
            let mut infos = Vec::with_capacity(file_paths.len());
            for (i, path_str) in file_paths.iter().enumerate() {
                let media_info = probe_cache.get(Path::new(path_str))
                    .and_then(|r| r.ok())
                    .ok_or_else(|| format!("Failed to probe file #{}: {}", i, path_str))?;
                infos.push((i, path_str.clone(), media_info));
            }

            // 4. Analyze profiles
            let analysis = analyze_profiles(&infos);

            // 5. Determine which outliers need normalization
            let outliers_to_normalize: Vec<Outlier> = if mode == MergeMode::SmartMkv {
                let (filtered, _) = filter_outliers_for_mkv(&analysis, MergeBackend::MkvMerge);
                filtered
            } else {
                analysis.outliers.clone()
            };

            let normalized_count = outliers_to_normalize.len();
            log::info!("[SECTION_ANALYSIS] section={} files={} total_outliers={} to_normalize={}",
                idx, file_paths.len(), analysis.outliers.len(), normalized_count);

            // 6. Normalize each outlier against dominant profile
            let norm_cache = Arc::new(NormalizationCache::new());
            let mut normalized_paths: HashMap<usize, String> = HashMap::new();
            let mut temp_files: Vec<PathBuf> = Vec::new();
            let dom = &analysis.dominant;

            for outlier in &outliers_to_normalize {
                if cancel_flag.load(Ordering::SeqCst) {
                    log::info!("[SECTION_CLEANUP] section={} temp_deleted={} (cancel during normalization)",
                        idx, temp_files.len());
                    for f in &temp_files { let _ = std::fs::remove_file(f); }
                    return Err("Section merge cancelled".into());
                }

                let file_idx = outlier.index;
                let file_path = &file_paths[file_idx];
                let profile = &analysis.profiles[file_idx];

                let result = match outlier.normalization_type {
                    NormalizationType::RemuxOnly => {
                        if let Some(ts) = dom.timescale_den {
                            normalize_timescale_lossless(
                                &ffmpeg_path, file_path, ts, &temp_dir, job_id, file_idx,
                                cancel_flag.clone(), Some(norm_cache.clone()),
                            ).await
                        } else {
                            continue;
                        }
                    }
                    NormalizationType::AudioReencode => {
                        let input_video_duration_ms = probe_cache.get(std::path::Path::new(file_path))
                            .and_then(|r| r.ok())
                            .and_then(|mi| mi.video_streams.first().and_then(|s| s.duration))
                            .map(|d| (d * 1000.0) as u64);
                        let input_audio_sample_rate = probe_cache.get(std::path::Path::new(file_path))
                            .and_then(|r| r.ok())
                            .and_then(|mi| { mi.audio_streams.first().map(|a| a.sample_rate) })
                            .flatten();
                        let input_audio_bitrate = probe_cache.get(std::path::Path::new(file_path))
                            .and_then(|r| r.ok())
                            .and_then(|mi| mi.audio_streams.first().and_then(|a| a.bit_rate))
                            .map(|br| format!("{}k", br / 1000));
                        let audio_profile = AudioProfile::new(
                            dom.a_codec.as_deref().unwrap_or("aac"),
                            dom.a_sample_rate.unwrap_or(48000),
                            dom.timescale_den,
                            dom.a_channels,
                        ).with_bitrate(input_audio_bitrate);
                        normalize_audio_only(
                            &ffmpeg_path, file_path,
                            &audio_profile,
                            &temp_dir, job_id, file_idx,
                            cancel_flag.clone(), Some(norm_cache.clone()),
                            input_video_duration_ms,
                            Some(&ffprobe_path),
                            input_audio_sample_rate,
                        ).await
                    }
                    NormalizationType::VideoReencode | NormalizationType::FullReencode => {
                        let input_video_duration_ms = probe_cache.get(std::path::Path::new(file_path))
                            .and_then(|r| r.ok())
                            .and_then(|mi| mi.video_streams.first().and_then(|s| s.duration))
                            .map(|d| (d * 1000.0) as u64);
                        let input_audio_sample_rate = probe_cache.get(std::path::Path::new(file_path))
                            .and_then(|r| r.ok())
                            .and_then(|mi| { mi.audio_streams.first().map(|a| a.sample_rate) })
                            .flatten();
                        let input_audio_bitrate = probe_cache.get(std::path::Path::new(file_path))
                            .and_then(|r| r.ok())
                            .and_then(|mi| mi.audio_streams.first().and_then(|a| a.bit_rate))
                            .map(|br| format!("{}k", br / 1000));
                        let encoding_profile = EncodingProfile::new(
                            dom.v_codec.as_deref().unwrap_or("libx264"),
                            dom.a_codec.as_deref().unwrap_or("aac"),
                            dom.a_sample_rate.unwrap_or(48000),
                            dom.v_fps,
                            dom.timescale_den,
                            dom.v_width,
                            dom.v_height,
                            dom.a_channels,
                        ).with_bitrate(input_audio_bitrate);
                        normalize_to_profile(
                            &ffmpeg_path, file_path,
                            &encoding_profile,
                            &temp_dir, job_id, file_idx,
                            !profile.has_no_audio,
                            cancel_flag.clone(), Some(norm_cache.clone()),
                            None,
                            input_video_duration_ms,
                            Some(&ffprobe_path),
                            input_audio_sample_rate,
                        ).await
                    }
                    NormalizationType::None => continue,
                };

                match result {
                    Ok(norm_path) => {
                        normalized_paths.insert(file_idx, norm_path.clone());
                        temp_files.push(PathBuf::from(&norm_path));
                    }
                    Err(e) => {
                        log::info!("[SECTION_CLEANUP] section={} temp_deleted={} (normalization error file#{})",
                            idx, temp_files.len(), file_idx);
                        for f in &temp_files { let _ = std::fs::remove_file(f); }
                        return Err(format!("Normalization failed for file #{}: {}", file_idx, e));
                    }
                }
            }

            let temp_count = temp_files.len();
            log::info!("[SECTION_NORMALIZED] section={} normalized={} temp_files={}",
                idx, normalized_paths.len(), temp_count);

            // 7. Build final file list (use normalized paths where available)
            let final_files: Vec<String> = file_paths.iter().enumerate().map(|(i, p)| {
                normalized_paths.get(&i).cloned().unwrap_or_else(|| p.clone())
            }).collect();

            let final_names: Vec<String> = final_files.iter().map(|p| {
                Path::new(p).file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default()
            }).collect();

            let final_durations: Vec<f64> = vec![0.0; final_files.len()];
            let final_path_refs: Vec<&Path> = final_files.iter().map(Path::new).collect();

            // 8. SECTION_CONCAT
            let list_path = temp_dir.join(format!("section_concat_{}_{}.txt", job_id, idx));
            log::info!("[SECTION_CONCAT] section={} files={} output={} list={}",
                idx, final_path_refs.len(), output_path_str, list_path.display());

            write_concat_list_with_durations(
                &final_path_refs,
                None,
                &list_path,
                mode == MergeMode::Custom,
            ).map_err(|e| format!("Failed to write concat list: {}", e))?;
            // final_path_refs borrow ends here (last use)

            // 9. Build MergeConfig
            let out_path = output_path_str.clone();
            let merge_config = MergeConfig {
                input_files: final_files,
                input_names: final_names,
                input_durations: final_durations,
                subtitle_list_path: None,
                output_path: out_path,
                mode,
                total_duration: plan.boundary.duration_secs,
                video_codec: None,
                audio_codec: None,
                video_crf: None,
                video_preset: None,
                audio_bitrate: None,
                target_resolution: None,
                target_fps: None,
                hw_accel: None,
                split_config: None,
                naming_config: None,
                subtitle_files: Vec::new(),
                subtitle_mode: SubtitleMode::None,
                export_merged_srt: false,
                segment_is_card: Vec::new(),
                card_config: None,
                burn_subtitle_path: None,
                mkvmerge_succeeded_before_ffmpeg: false,
                // Audio immutability: if any file was normalized in this section,
                // Custom concat MUST use -c:a copy to prevent second-generation AAC loss.
                audio_normalized: !normalized_paths.is_empty(),
                immutability_registry: None, // Registry integration is future work
            };

            let ffmpeg_buf = ffmpeg_path.to_path_buf();
            let list_buf = list_path.clone();
            let cancel_clone = cancel_flag.clone();

            match tokio::task::spawn_blocking(move || {
                run_merge_blocking(
                    &ffmpeg_buf,
                    &merge_config,
                    &list_buf,
                    cancel_clone,
                    |_progress| {},
                )
            }).await {
                Ok(Ok(_merge_result)) => {}
                Ok(Err(e)) => {
                    log::info!("[SECTION_CLEANUP] section={} temp_deleted={} (merge error)",
                        idx, temp_files.len());
                    for f in &temp_files { let _ = std::fs::remove_file(f); }
                    return Err(format!("Merge failed: {}", e));
                }
                Err(e) => {
                    log::info!("[SECTION_CLEANUP] section={} temp_deleted={} (merge panic)",
                        idx, temp_files.len());
                    for f in &temp_files { let _ = std::fs::remove_file(f); }
                    return Err(format!("Merge task panicked: {}", e));
                }
            };

            Ok((temp_files, list_path))
        }.await;
        
        let section_result = match section_merge_result {
            Ok((section_temp_files, section_list_path)) => {
                let size_bytes = std::fs::metadata(&output_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                
                let result = SectionResult {
                    section_index: idx as u32,
                    output_path: output_path_str,
                    duration_secs: plan.boundary.duration_secs,
                    size_bytes,
                    success: true,
                    error_message: None,
                };
                
                // Update checkpoint with completed section FIRST
                if let Some(ref sender) = checkpoint_sender {
                    sender.append_section_result(result.clone());
                }
                
                // THEN cleanup temp files (preserve recovery on checkpoint failure)
                log::info!("[SECTION_CLEANUP] section={} temp_deleted={} concat_deleted=1",
                    idx, section_temp_files.len());
                for f in &section_temp_files { let _ = std::fs::remove_file(f); }
                let _ = std::fs::remove_file(&section_list_path);
                
                log::info!("[SECTION_COMPLETE] section={} output={} size={} success=true",
                    idx, result.output_path, size_bytes);
                
                // Emit complete event
                let _ = app_handle.emit("section-complete", &SectionEvent {
                    job_id: job_id_str.clone(),
                    current_section: idx as u32,
                    total_sections,
                    section_name: plan.output_name.clone(),
                    phase: "complete".into(),
                    progress_percent: Some(100.0),
                    eta_seconds: None,
                    error: None,
                });
                
                result
            }
            Err(e) => {
                let error_msg = e.clone();
                log::error!("[Section] Section {} failed: {}", idx, error_msg);
                
                let result = SectionResult {
                    section_index: idx as u32,
                    output_path: output_path_str,
                    duration_secs: 0.0,
                    size_bytes: 0,
                    success: false,
                    error_message: Some(error_msg.clone()),
                };
                
                // Update checkpoint with failed section
                if let Some(ref sender) = checkpoint_sender {
                    sender.append_section_result(result.clone());
                    sender.update_phase(crate::types::MergePhase::Failed, None);
                }
                
                // Emit error event
                let _ = app_handle.emit("section-error", &SectionEvent {
                    job_id: job_id_str,
                    current_section: idx as u32,
                    total_sections,
                    section_name: plan.output_name.clone(),
                    phase: "failed".into(),
                    progress_percent: None,
                    eta_seconds: None,
                    error: Some(error_msg),
                });
                
                return Err(format!("Section {} failed", idx + 1));
            }
        };
        
        section_results.push(section_result);
    }
    
    // Mark all complete
    if let Some(ref sender) = checkpoint_sender {
        sender.update_phase(crate::types::MergePhase::Complete, None);
    }
    
    log::info!("[Section] All sections complete. Success: {}", 
        section_results.iter().filter(|r| r.success).count());
    
    // Ensure drain before returning
    if let Some(writer) = checkpoint_writer.take() {
        if let Err(e) = writer.close() {
            log::error!("[Recovery] Checkpoint writer close() failed: {:?}", e);
        } else {
            log::info!("[Recovery] Checkpoint writer drained successfully");
        }
    }
    
    Ok(section_results)
}

/// Check for an existing section merge checkpoint and return resume data.
/// Returns `None` if no valid checkpoint exists (fresh start).
#[allow(clippy::type_complexity)]
fn try_resume_section_merge(
    app_data: &Path,
    job_id: &str,
) -> Result<Option<(Vec<SectionResult>, Vec<SectionPlan>, u32, usize)>, String> {
    let checkpoint = match recovery::read_checkpoint(app_data, job_id)
        .map_err(|e| format!("Failed to read checkpoint: {}", e))?
    {
        Some(cp) => cp,
        None => return Ok(None),
    };

    if checkpoint.mode != "sectionMerge" {
        log::info!("[Section] Existing checkpoint mode is '{}' — not a section merge, starting fresh", checkpoint.mode);
        return Ok(None);
    }

    let meta = match checkpoint.section_meta {
        Some(m) => m,
        None => {
            log::info!("[Section] Checkpoint has no section_meta — starting fresh");
            return Ok(None);
        }
    };

    let completed_count = meta.completed_sections.len();
    let start_idx = completed_count; // sections are processed sequentially

    log::info!("[Section] Resume checkpoint found: {}/{} completed, resuming from section {}",
        completed_count, meta.total_sections, start_idx + 1);

    Ok(Some((meta.section_results, meta.section_plans, meta.total_sections, start_idx)))
}

fn section_merge_disk_space_required_internal(
    section_size_bytes: u64,
    largest_section_bytes: u64,
) -> u64 {
    // Current section: source read + temp + output write
    let current = section_size_bytes 
        + (section_size_bytes / 2) 
        + section_size_bytes;
    // Next section source (readable while current outputs exist)
    let next_source = largest_section_bytes;
    // Recovery buffer
    let recovery = largest_section_bytes / 2;
    
    current + next_source + recovery
}

#[command]
pub async fn cancel_section_merge(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<(), String> {
    log::info!("[Section] Cancelling section merge: {}", job_id);
    
    let ms = state.merge_state.lock().await;
    if let Some(flag) = ms.active_jobs.get(&job_id) {
        flag.store(true, Ordering::SeqCst);
        Ok(())
    } else {
        Err("Job not found or already completed".into())
    }
}

#[command]
pub async fn get_section_merge_status(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let ms = state.merge_state.lock().await;
    Ok(serde_json::json!({
        "isRunning": !ms.active_jobs.is_empty(),
        "runningJobIds": ms.active_jobs.keys().cloned().collect::<Vec<_>>()
    }))
}

#[command]
pub async fn section_merge_disk_space_required(
    section_size_bytes: u64,
    largest_section_bytes: u64,
) -> u64 {
    section_merge_disk_space_required_internal(section_size_bytes, largest_section_bytes)
}