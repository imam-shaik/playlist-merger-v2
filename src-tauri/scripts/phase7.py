#!/usr/bin/env python3
"""Phase 7: Parallel normalization with tokio::JoinSet + Semaphore"""

import sys
import io

# Force UTF-8 for stdout to handle emoji on Windows
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

MERGE_RS = 'src/commands/merge.rs'

with open(MERGE_RS, 'r', encoding='utf-8') as f:
    content = f.read()

# ==========================================================================
# Find exact anchor points
# ==========================================================================

profile_start = '        for idx in need_profile_norm_iter {'
audio_start  = '\n        for idx in need_audio_norm {'
forensic_marker = '\n        // ══════════════════════════════════════════════════════════════════════════════\n        // REAL WORKLOAD FORENSIC REPORT\n'

p_idx = content.find(profile_start)
a_idx = content.find(audio_start, p_idx + 1)
f_idx = content.find(forensic_marker, a_idx + 1)

if p_idx < 0: print("ERROR: profile loop not found"); sys.exit(1)
if a_idx < 0: print("ERROR: audio loop not found"); sys.exit(1)
if f_idx < 0: print("ERROR: forensic report not found"); sys.exit(1)

profile_text = content[p_idx:a_idx]
audio_text  = content[a_idx:f_idx]

print(f"[INFO] Profile loop: {len(profile_text)} chars at {p_idx}")
print(f"[INFO] Audio loop:   {len(audio_text)} chars at {a_idx}")
print(f"[INFO] Forensic:     at {f_idx}")

# ==========================================================================
# NEW PROFILE LOOP (parallel)
# ==========================================================================

new_profile = """        // ── Phase 7: Parallel Profile Normalization (tokio::JoinSet + Semaphore) ──
        if !need_profile_norm_iter.is_empty() {
            let par_sem = Arc::new(tokio::sync::Semaphore::new(4));
            let par_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let par_done = Arc::new(AtomicUsize::new(0));
            let par_total = need_profile_norm_iter.len();
            let par_forensics: Arc<Mutex<RealWorkloadForensics>> = Arc::new(Mutex::new(RealWorkloadForensics::new()));
            let par_wif: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(working_input_files));
            let par_an: Arc<Mutex<std::collections::HashSet<usize>>> = Arc::new(Mutex::new(already_normalized));
            let par_norm_idx = Arc::new(AtomicUsize::new(0));
            let dom_par = dom.clone();
            let rcp_par = recovery_checkpoint.clone();
            let pft_par = per_file_triggers.clone();
            let rr_par = repair_reasons.clone();
            let obi_par = outlier_by_index.clone();
            let ao_par = analysis.outliers.clone();
            let aao_par = analysis.audio_outliers.clone();

            let mut js = tokio::task::JoinSet::new();
            for &idx in &need_profile_norm_iter {
                let s = par_sem.clone(); let e = par_err.clone(); let d = par_done.clone();
                let t = par_total; let f = par_forensics.clone(); let w = par_wif.clone();
                let a = par_an.clone(); let n = par_norm_idx.clone();
                let c = cancel_flag.clone(); let p = probe_cache.clone(); let nf = temp_norm_files_arc.clone();
                let ff = ffmpeg_path_resolved.clone(); let fp = ffprobe_path_resolved.clone();
                let td = temp_dir.clone(); let j = request.job_id.clone(); let dm = dom_par.clone();
                let ah = app_handle.clone(); let dur = request.total_duration;
                let to = total_outliers; let rcp = rcp_par.clone();
                let pfti = pft_par.clone(); let rri = rr_par.clone(); let obii = obi_par.clone();
                let aoi = ao_par.clone(); let aaoi = aao_par.clone();

                js.spawn(async move {
                    let _permit = match s.acquire().await { Ok(p) => p, Err(_) => { return } };
                    // Check prior error
                    if let Ok(eg) = e.lock() { if eg.is_some() { return } }
                    if c.load(Ordering::Relaxed) { return }
                    // Increment counters
                    let ci = n.fetch_add(1, Ordering::Relaxed) + 1;
                    { let mut ag = a.lock().unwrap(); ag.insert(idx); }
                    let fpath = { let wg = w.lock().unwrap(); wg[idx].clone() };
                    let fname = std::path::Path::new(&fpath).file_name()
                        .and_then(|n| n.to_str()).unwrap_or("file").to_string();
                    // Norm type
                    let ntype = obii.get(&idx).map_or("Video Re-encode", |ox| {
                        if ox.iter().all(|o| matches!(o.normalization_type, crate::ffmpeg::normalization::NormalizationType::RemuxOnly)) {
                            if dm.timescale_den.is_some() { "Timescale Fix (Lossless)" } else { "Container Fix (Lossless)" }
                        } else { "Video Profile Normalization" }
                    });
                    // Repair reason
                    let vrr: String = rri.get(&idx).map(|r| r.join(", ")).unwrap_or_default();
                    let ard: String = aaoi.iter().filter(|ao| ao.index == idx)
                        .map(|ao| format!("AudioMismatch_{}({}->{})", ao.audio_type.code(), ao.actual_value, ao.dominant_value))
                        .collect::<Vec<_>>().join("; ");
                    let rsn = if ard.is_empty() { vrr.clone() } else if vrr.is_empty() { ard.clone() } else { format!("{} | {}", vrr, ard) };
                    // Progress
                    let sp = (ci as f64 / to as f64) * 100.0;
                    let op = 15.0 + (sp * 0.20);
                    let _ = ah.emit("merge-progress", &serde_json::json!({
                        "jobId": j, "progress": {
                            "phase": "normalizing", "stageName": format!("Normalising video: {}...", fname),
                            "stagePercent": sp, "currentFileIndex": ci, "totalFilesInStage": to,
                            "percent": op, "overallPercent": op, "currentTime": 0.0, "totalDuration": dur,
                            "normalizationType": ntype, "currentFile": fname, "repairReason": rsn,
                    }}));
                    // Forensics: before
                    if let Some(Ok(ii)) = p.get(std::path::Path::new(&fpath)) {
                        log::info!("[FORENSIC:BEFORE] File #{} | Path: {} | TB: {:?} | SR: {:?} | FPS: {:?}",
                            idx, fpath, ii.video_streams.first().and_then(|s| s.time_base.clone()),
                            ii.audio_streams.first().and_then(|s| s.sample_rate),
                            ii.video_streams.first().and_then(|s| s.fps));
                    }
                    if c.load(Ordering::Relaxed) { return }
                    let only_remux = obii.get(&idx).map_or(false, |ox| {
                        ox.iter().all(|o| matches!(o.normalization_type, crate::ffmpeg::normalization::NormalizationType::RemuxOnly))
                    });
                    let ntl = if only_remux && dm.timescale_den.is_some() { "TimescaleRemux" }
                               else if only_remux { "ContainerFix" } else { "VideoReencode" };
                    if let Some(Ok(ii)) = p.get(std::path::Path::new(&fpath)) {
                        f.lock().unwrap().record_file_start(idx, &fname, &fpath, &ii, ntl);
                    }
                    let ns = std::time::Instant::now();
                    let res = if only_remux {
                        if let Some(ts) = dm.timescale_den {
                            normalize_timescale_lossless(&ff, &fpath, ts, &td, &j, idx, c.clone()).await
                        } else {
                            normalize_to_profile(&ff, &fpath, dm.v_codec.as_deref().unwrap_or("libx264"),
                                dm.a_codec.as_deref().unwrap_or("aac"), dm.a_sample_rate.unwrap_or(48000),
                                dm.v_fps, dm.timescale_den, dm.v_width, dm.v_height, &td, &j, idx, true, c.clone()).await
                        }
                    } else {
                        normalize_to_profile(&ff, &fpath, dm.v_codec.as_deref().unwrap_or("libx264"),
                            dm.a_codec.as_deref().unwrap_or("aac"), dm.a_sample_rate.unwrap_or(48000),
                            dm.v_fps, dm.timescale_den, dm.v_width, dm.v_height, &td, &j, idx, true, c.clone()).await
                    };
                    log::info!("[FORENSIC:NORMALIZE] END: {} | File #{} | Elapsed: {:?}",
                        if only_remux && dm.timescale_den.is_some() { "Timescale Remux" } else { "Video Re-encode" }, idx, ns.elapsed());
                    match res {
                        Ok(path) => {
                            f.lock().unwrap().record_file_norm_end(idx, &path);
                            nf.lock().unwrap().push(std::path::PathBuf::from(&path));
                            { let mut wg = w.lock().unwrap(); wg[idx] = path.clone(); }
                            { f.lock().unwrap().record_file_verify_start(idx); }
                            let vr = verify_normalized_audio_health(&ff, &fp, &path, idx, &fname, c.clone()).await;
                            match vr {
                                Ok(ni) => {
                                    f.lock().unwrap().record_file_verify_end(idx, ni.duration);
                                    p.insert(std::path::PathBuf::from(&path), Ok(ni.clone()));
                                    if let Ok(ad) = crate::recovery::get_app_data_dir() {
                                        let nt = if dm.timescale_den.is_some() { crate::types::NormalizationType::Timescale } else { crate::types::NormalizationType::Full };
                                        let _ = crate::recovery::append_completed_file(&ad, &j, idx, "", 0, 0, &path, nt);
                                    }
                                }
                                Err(ve) => {
                                    if ve.contains("cancelled") || c.load(Ordering::Relaxed) {
                                        let _ = ah.emit("merge-error", &serde_json::json!({"jobId":j,"error":"Merge cancelled by user","cancelled":true}));
                                        let mut eg = e.lock().unwrap(); if eg.is_none() { *eg = Some("Cancelled".to_string()); }
                                    } else {
                                        log::error!("[FORENSIC:AUDIO_VALIDATE] FAILED File #{}: {}", idx, ve);
                                        let mut eg = e.lock().unwrap(); if eg.is_none() { *eg = Some(format!("Audio repair failed for File #{} ({}): {}", idx, fname, ve)); }
                                    }
                                    return;
                                }
                            }
                            d.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(err_msg) => {
                            if err_msg.contains("cancelled") {
                                let _ = ah.emit("merge-error", &serde_json::json!({"jobId":j,"error":"Merge cancelled by user","cancelled":true}));
                                let mut eg = e.lock().unwrap(); if eg.is_none() { *eg = Some("Cancelled".to_string()); }
                            } else {
                                log::error!("[FORENSIC:ERROR] Video norm FAILED File #{}: {}", idx, err_msg);
                                let mut eg = e.lock().unwrap(); if eg.is_none() { *eg = Some(format!("Video normalization failed for File #{}: {}", idx, err_msg)); }
                            }
                        }
                    }
                });
            }
            while let Some(r) = js.join_next().await {
                if let Err(je) = r {
                    log::error!("[Phase7] Profile task panicked: {}", je);
                    let mut eg = par_err.lock().unwrap(); if eg.is_none() { *eg = Some(format!("Task panicked: {}", je)); }
                }
            }
            // Check errors
            { let eg = par_err.lock().unwrap();
                if let Some(ref em) = *eg {
                    if em == "Cancelled" {
                        let _ = app_handle.emit("merge-error", &serde_json::json!({"jobId":request.job_id,"error":"Merge cancelled by user","cancelled":true}));
                        cleanup_guard.cleanup(); return Ok(request.job_id);
                    }
                    cleanup_guard.cleanup(); return Err(em.clone());
                }
            }
            working_input_files = Arc::try_unwrap(par_wif).unwrap_or_else(|_| { log::error!("[Phase7] wif leaked"); Mutex::new(Vec::new()) }).into_inner().unwrap_or_default();
            already_normalized = Arc::try_unwrap(par_an).unwrap_or_else(|_| { log::error!("[Phase7] an leaked"); Mutex::new(std::collections::HashSet::new()) }).into_inner().unwrap_or_default();
            log::info!("[Phase7] Parallel profile norm complete: {} files", par_total);
        }"""

# ==========================================================================
# NEW AUDIO LOOP (parallel)
# ==========================================================================

new_audio = """        // ── Phase 7: Parallel Audio Normalization (tokio::JoinSet + Semaphore) ──
        if !need_audio_norm.is_empty() {
            let par_sem = Arc::new(tokio::sync::Semaphore::new(4));
            let par_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let par_done = Arc::new(AtomicUsize::new(0));
            let par_total = need_audio_norm.len();
            let par_forensics: Arc<Mutex<RealWorkloadForensics>> = Arc::new(Mutex::new(RealWorkloadForensics::new()));
            let par_wif: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(working_input_files));
            let par_an: Arc<Mutex<std::collections::HashSet<usize>>> = Arc::new(Mutex::new(already_normalized));
            let par_norm_idx = Arc::new(AtomicUsize::new(0));
            let dom_par = dom.clone();
            let rcp_par = recovery_checkpoint.clone();
            let rr_par = repair_reasons.clone();
            let aao_par = analysis.audio_outliers.clone();

            let mut js = tokio::task::JoinSet::new();
            for &idx in &need_audio_norm {
                let s = par_sem.clone(); let e = par_err.clone(); let d = par_done.clone();
                let t = par_total; let f = par_forensics.clone(); let w = par_wif.clone();
                let a = par_an.clone(); let n = par_norm_idx.clone();
                let c = cancel_flag.clone(); let p = probe_cache.clone(); let nf = temp_norm_files_arc.clone();
                let ff = ffmpeg_path_resolved.clone(); let fp = ffprobe_path_resolved.clone();
                let td = temp_dir.clone(); let j = request.job_id.clone(); let dm = dom_par.clone();
                let ah = app_handle.clone(); let dur = request.total_duration;
                let to = total_outliers; let rcp = rcp_par.clone();
                let rri = rr_par.clone(); let aaoi = aao_par.clone();

                js.spawn(async move {
                    let _permit = match s.acquire().await { Ok(p) => p, Err(_) => { return } };
                    if let Ok(eg) = e.lock() { if eg.is_some() { return } }
                    if c.load(Ordering::Relaxed) { return }
                    // Skip if already normalized by profile loop
                    if dm.timescale_den.is_some() { let ag = a.lock().unwrap(); if ag.contains(&idx) { return } }
                    let fpath = { let wg = w.lock().unwrap(); wg[idx].clone() };
                    let fname = std::path::Path::new(&fpath).file_name()
                        .map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".to_string());
                    let ci = n.fetch_add(1, Ordering::Relaxed) + 1;
                    // Audio norm type labels
                    let sal: Vec<&str> = aaoi.iter().filter(|ao| ao.index == idx).map(|ao| ao.audio_type.label()).collect();
                    let ant: String = if !sal.is_empty() { sal.join(" + ") }
                        else if dm.timescale_den.is_some() { "Audio + Video Re-encode".to_string() }
                        else { "Audio Re-encode".to_string() };
                    let ard: String = aaoi.iter().filter(|ao| ao.index == idx)
                        .map(|ao| format!("{} ({}->{})", ao.audio_type.code(), ao.actual_value, ao.dominant_value))
                        .collect::<Vec<_>>().join("; ");
                    let brr: String = rri.get(&idx).map(|r| r.join(", ")).unwrap_or_default();
                    let rsn = if ard.is_empty() { brr.clone() } else { format!("{} | {}", brr, ard) };
                    let sp = (ci as f64 / to as f64) * 100.0;
                    let op = 15.0 + (sp * 0.20);
                    let _ = ah.emit("merge-progress", &serde_json::json!({
                        "jobId": j, "progress": {
                            "phase": "normalizing", "stageName": format!("Normalising audio: {}...", fname),
                            "stagePercent": sp, "currentFileIndex": ci, "totalFilesInStage": to,
                            "percent": op, "overallPercent": op, "currentTime": 0.0, "totalDuration": dur,
                            "normalizationType": ant, "currentFile": fname, "repairReason": rsn,
                    }}));
                    if let Some(Ok(ii)) = p.get(std::path::Path::new(&fpath)) {
                        log::info!("[FORENSIC:BEFORE] File #{} | Path: {} | TB: {:?} | SR: {:?} | FPS: {:?}",
                            idx, fpath, ii.video_streams.first().and_then(|s| s.time_base.clone()),
                            ii.audio_streams.first().and_then(|s| s.sample_rate),
                            ii.video_streams.first().and_then(|s| s.fps));
                    }
                    let ntl = if dm.timescale_den.is_some() { "AudioVideoReencode" } else { "AudioReencode" };
                    if let Some(Ok(ii)) = p.get(std::path::Path::new(&fpath)) {
                        f.lock().unwrap().record_file_start(idx, &fname, &fpath, &ii, ntl);
                    }
                    if c.load(Ordering::Relaxed) { return }
                    let ns = std::time::Instant::now();
                    let res = if dm.timescale_den.is_some() {
                        normalize_to_profile(&ff, &fpath, dm.v_codec.as_deref().unwrap_or("libx264"),
                            dm.a_codec.as_deref().unwrap_or("aac"), dm.a_sample_rate.unwrap_or(48000),
                            dm.v_fps, dm.timescale_den, dm.v_width, dm.v_height, &td, &j, idx, true, c.clone()).await
                    } else {
                        normalize_audio_only(&ff, &fpath, dm.a_codec.as_deref().unwrap_or("aac"),
                            dm.a_sample_rate.unwrap_or(48000), dm.timescale_den, &td, &j, idx, c.clone()).await
                    };
                    log::info!("[FORENSIC:NORMALIZE] END: Audio Normalize | File #{} | Elapsed: {:?}", idx, ns.elapsed());
                    match res {
                        Ok(path) => {
                            f.lock().unwrap().record_file_norm_end(idx, &path);
                            nf.lock().unwrap().push(std::path::PathBuf::from(&path));
                            { let mut wg = w.lock().unwrap(); wg[idx] = path.clone(); }
                            { f.lock().unwrap().record_file_verify_start(idx); }
                            let vr = verify_normalized_audio_health(&ff, &fp, &path, idx, &fname, c.clone()).await;
                            match vr {
                                Ok(ni) => {
                                    f.lock().unwrap().record_file_verify_end(idx, ni.duration);
                                    p.insert(std::path::PathBuf::from(&path), Ok(ni.clone()));
                                    if let Ok(ad) = crate::recovery::get_app_data_dir() {
                                        let _ = crate::recovery::append_completed_file(&ad, &j, idx, "", 0, 0, &path, crate::types::NormalizationType::Audio);
                                    }
                                }
                                Err(ve) => {
                                    if ve.contains("cancelled") || c.load(Ordering::Relaxed) {
                                        let _ = ah.emit("merge-error", &serde_json::json!({"jobId":j,"error":"Merge cancelled by user","cancelled":true}));
                                        let mut eg = e.lock().unwrap(); if eg.is_none() { *eg = Some("Cancelled".to_string()); }
                                    } else {
                                        log::error!("[FORENSIC:AUDIO_VALIDATE] FAILED File #{}: {}", idx, ve);
                                        let mut eg = e.lock().unwrap(); if eg.is_none() { *eg = Some(format!("Audio repair failed for File #{} ({}): {}", idx, fname, ve)); }
                                    }
                                    return;
                                }
                            }
                            d.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(err_msg) => {
                            if err_msg.contains("cancelled") {
                                let _ = ah.emit("merge-error", &serde_json::json!({"jobId":j,"error":"Merge cancelled by user","cancelled":true}));
                                let mut eg = e.lock().unwrap(); if eg.is_none() { *eg = Some("Cancelled".to_string()); }
                            } else {
                                log::error!("[FORENSIC:ERROR] Audio norm FAILED File #{}: {}", idx, err_msg);
                                let mut eg = e.lock().unwrap(); if eg.is_none() { *eg = Some(format!("Audio normalization failed for File #{}: {}", idx, err_msg)); }
                            }
                        }
                    }
                });
            }
            while let Some(r) = js.join_next().await {
                if let Err(je) = r {
                    log::error!("[Phase7] Audio task panicked: {}", je);
                    let mut eg = par_err.lock().unwrap(); if eg.is_none() { *eg = Some(format!("Task panicked: {}", je)); }
                }
            }
            { let eg = par_err.lock().unwrap();
                if let Some(ref em) = *eg {
                    if em == "Cancelled" {
                        let _ = app_handle.emit("merge-error", &serde_json::json!({"jobId":request.job_id,"error":"Merge cancelled by user","cancelled":true}));
                        cleanup_guard.cleanup(); return Ok(request.job_id);
                    }
                    cleanup_guard.cleanup(); return Err(em.clone());
                }
            }
            working_input_files = Arc::try_unwrap(par_wif).unwrap_or_else(|_| { log::error!("[Phase7] Audio wif leaked"); Mutex::new(Vec::new()) }).into_inner().unwrap_or_default();
            already_normalized = Arc::try_unwrap(par_an).unwrap_or_else(|_| { log::error!("[Phase7] Audio an leaked"); Mutex::new(std::collections::HashSet::new()) }).into_inner().unwrap_or_default();
            log::info!("[Phase7] Parallel audio norm complete: {} files", par_total);
        }"""

# ==========================================================================
# Apply both replacements
# ==========================================================================

content = content.replace(profile_text, new_profile, 1)
if content.count('need_profile_norm_iter') < 1:
    print("[FAIL] Profile loop replacement didn't apply!")
    sys.exit(1)
print("[OK] Profile norm loop replaced")

# Re-find audio loop in modified content
a_idx2 = content.find(audio_start)
f_idx2 = content.find(forensic_marker, a_idx2 + 1)
if a_idx2 < 0 or f_idx2 < 0:
    print("[FAIL] Could not re-find audio loop after profile replacement!")
    sys.exit(1)

audio_text2 = content[a_idx2:f_idx2]
content = content.replace(audio_text2, new_audio, 1)
if content.count(audio_start) > 0:
    print("[FAIL] Audio loop replacement didn't apply!")
    sys.exit(1)
print("[OK] Audio norm loop replaced")

with open(MERGE_RS, 'w', encoding='utf-8') as f:
    f.write(content)

print("[DONE] Phase 7 implementation written to merge.rs")
print(f"[INFO] File size: {len(content)} chars")
