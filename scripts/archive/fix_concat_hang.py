#!/usr/bin/env python3
"""Fix concat.rs: Add try_wait polling with timeout to run_merge_blocking() and run_split_merge_blocking()."""
import os

BASE = os.path.join(os.path.dirname(os.path.dirname(__file__)), 'src-tauri', 'src', 'ffmpeg')
path = os.path.join(BASE, 'concat.rs')

with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

changes_applied = 0

# ============================================================
# FIX 1: Add use for std::time::{Duration, Instant} 
# ============================================================
# The file already has std::time used in some places via Instant. 
# Let's find the existing use lines and add if needed.

old_import = "use anyhow::{Result, anyhow, Context};"
new_import = "use anyhow::{Result, anyhow, Context};\nuse std::time::{Duration, Instant};"
if old_import in content and "use std::time::{Duration, Instant}" not in content:
    content = content.replace(old_import, new_import, 1)
    changes_applied += 1
    print("FIX 1: Added Duration/Instant imports")
else:
    print("FIX 1: Imports already present or not found")

# ============================================================
# FIX 2: Add timeout + try_wait BEFORE the stderr reading loop in run_merge_blocking()
# 
# Current structure (around line 1700-1855):
#   1. spawn_ffmpeg
#   2. Take stderr, create reader
#   3. reader.lines() loop (blocks forever if FFmpeg hangs)
#   4. After loop: child.wait() (also blocks)
#   5. Process status
#
# Fix: Move all the stderr reading + purging + progress emitting to a background thread.
# Main thread polls try_wait() with timeout + cancel checks.
# ============================================================

# The key section to modify starts after spawn_ffmpeg and the stderr setup

old_section_start = """    let mut cmd = Command::new(ffmpeg_path);
    cmd.args(&args);
    let mut child = spawn_ffmpeg(&mut cmd)?;

    let stderr = child.stderr.take().ok_or_else(|| {
        anyhow::anyhow!("INTERNAL ERROR: stderr not available after spawn_ffmpeg. This indicates a programming error - spawn_ffmpeg must be called before reading stderr.")
    })?;
    let reader = BufReader::new(stderr);
    let total = config.total_duration;
    let mut block_reader = ProgressBlockReader::new();
    let mut last_emit_ms = std::time::Instant::now();
    const MIN_EMIT_INTERVAL_MS: u64 = 100;
    let mut stderr_log: Vec<String> = Vec::new();

    // Track which normalized files are still on disk so we can purge them incrementally
    let mut active_norm_files: Vec<(usize, std::path::PathBuf)> = Vec::new();
    for (i, file_path) in config.input_files.iter().enumerate() {
        let p = std::path::PathBuf::from(file_path);
        // Only track files that are actually in the temp directory (normalized outliers)
        if p.starts_with(&temp_dir) {
            active_norm_files.push((i, p));
        }
    }
    // Sort by index so we can purge in order
    active_norm_files.sort_by_key(|(idx, _)| *idx);

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(ref l) => l,
            Err(_) => break,
        };

        // ── INCREMENTAL PURGE LOGIC ──────────────────────────────────────
        // FFmpeg concat demuxer logs: "[concat @ 0x...] Opening 'file.mp4' for reading"
        // When FFmpeg opens file N, we can safely delete normalized file N-1 
        // because the demuxer only keeps one input file handle open at a time.
        if line.contains("Opening '") && line.contains("' for reading") {
            if let Some(start_quote) = line.find("'") {
                if let Some(end_quote) = line[start_quote+1..].find("'") {
                    let opened_file = &line[start_quote+1..start_quote+1+end_quote];
                    // Find the index of the file FFmpeg just opened
                    if let Some(opened_idx) = config.input_files.iter().position(|f| f.replace('\\\\', "/").contains(opened_file)) {
                        // We can now purge any normalized files with index < opened_idx
                        let mut i = 0;
                        while i < active_norm_files.len() {
                            if active_norm_files[i].0 < opened_idx {
                                let (idx, path) = active_norm_files.remove(i);
                                if path.exists() {
                                    if let Err(e) = std::fs::remove_file(&path) {
                                        log::warn!("[FORENSIC:PURGE] Failed to incremental purge File #{}: {}", idx, e);
                                    } else {
                                        log::info!("[FORENSIC:PURGE] Incremental purge success: File #{} ({})", idx, path.display());
                                    }
                                }
                                // Don't increment i because we removed an element
                            } else {
                                i += 1;
                            }
                        }
                    }
                }
            }
        }
        // ─────────────────────────────────────────────────────────────────

        if cancel_flag.load(Ordering::Relaxed) {
            force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(&config.output_path));

            on_progress(MergeProgress {
                percent: 0.0,
                current_time: 0.0,
                total_duration: total,
                speed: None,
                fps: None,
                current_file: None,
                current_segment_index: None,
                remaining_duration: None,
                bytes_written: None,
                eta_seconds: None,
                phase: MergePhase::Cancelled,
                overall_percent: None,
                stage_name: None,
                stage_percent: None,
                current_file_index: None,
                total_files_in_stage: None,
                warning: None,
                is_large_playlist: false,
            });
            return Err(anyhow!("Merge cancelled by user"));
        }

        stderr_log.push(line.clone());
        if stderr_log.len() > 30 {
            stderr_log.remove(0);
        }

        if let Some(block) = block_reader.feed_line(line) {
            if let Some(prog) = parse_progress_block(&block) {
                let elapsed = prog.out_time_seconds.unwrap_or(0.0).max(0.0);
                let percent = calc_progress_percent(elapsed, total);

                // Determine current segment index and remaining duration
                let mut current_idx = 0;
                let mut remaining_dur = total;
                for (i, seg) in segments_ref.iter().enumerate() {
                    if elapsed >= seg.start_time && elapsed < seg.end_time {
                        current_idx = i;
                    }
                    if elapsed >= seg.end_time {
                        remaining_dur -= seg.duration;
                    }
                }
                if current_idx < segments_ref.len() {
                    let seg = &segments_ref[current_idx];
                    let processed_in_seg = (elapsed - seg.start_time).max(0.0);
                    remaining_dur -= processed_in_seg;
                }

                let eta = prog.speed
                    .filter(|&s| s > 0.01)
                    .map(|speed| ((total - elapsed) as f32 / speed).max(0.0));

                let now = std::time::Instant::now();
                if now.duration_since(last_emit_ms).as_millis() >= MIN_EMIT_INTERVAL_MS as u128 {
                    last_emit_ms = now;
                    on_progress(MergeProgress {
                        percent,
                        current_time: elapsed,
                        total_duration: total,
                        speed: prog.speed,
                        fps: prog.fps,
                        current_file: None,
                        current_segment_index: Some(current_idx),
                        remaining_duration: Some(remaining_dur.max(0.0)),
                        bytes_written: prog.total_size_bytes,
                        eta_seconds: eta,
                        phase: MergePhase::Writing,
                        overall_percent: None,
                        stage_name: Some("Merging...".into()),
                        stage_percent: Some(percent),
                        current_file_index: None,
                        total_files_in_stage: None,
                        warning: None,
                        is_large_playlist: false,
                    });
                }

                if prog.is_end { break; }
            }
        }
    }

    let status = child.wait().with_context(|| "Failed to wait for ffmpeg process")?;"""

new_section_merged = """    let mut cmd = Command::new(ffmpeg_path);
    cmd.args(&args);
    let mut child = spawn_ffmpeg(&mut cmd)?;

    let stderr = child.stderr.take().ok_or_else(|| {
        anyhow::anyhow!("INTERNAL ERROR: stderr not available after spawn_ffmpeg. This indicates a programming error - spawn_ffmpeg must be called before reading stderr.")
    })?;

    // ── BACKGROUND STREAMING THREAD ─────────────────────────────────────
    // Move all stderr reading (progress parsing, purge tracking) to a 
    // background thread so the main thread can poll try_wait() with
    // timeout + cancellation. This prevents infinite blocking if FFmpeg hangs.
    let total = config.total_duration;
    let segmented_refs = segments_ref.clone();
    let cancel_shared = cancel_flag.clone();
    let output_path_owned = config.output_path.clone();
    let purge_input_files: Vec<String> = config.input_files.clone();
    let temp_dir_owned = temp_dir.clone();
    let progress_cb = move |mp: MergeProgress| { on_progress(mp); };

    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut block_reader = ProgressBlockReader::new();
        let mut stderr_log: Vec<String> = Vec::new();
        let mut last_emit_ms = Instant::now();
        const MIN_EMIT_INTERVAL_MS: u64 = 100;

        // Track which normalized files are still on disk so we can purge them incrementally
        let mut active_norm_files: Vec<(usize, std::path::PathBuf)> = Vec::new();
        for (i, file_path) in purge_input_files.iter().enumerate() {
            let p = std::path::PathBuf::from(file_path);
            if p.starts_with(&temp_dir_owned) {
                active_norm_files.push((i, p));
            }
        }
        active_norm_files.sort_by_key(|(idx, _)| *idx);

        for line_result in reader.lines() {
            let line = match line_result {
                Ok(ref l) => l,
                Err(_) => break,
            };

            // ── INCREMENTAL PURGE LOGIC ──────────────────────────────────
            if line.contains("Opening '") && line.contains("' for reading") {
                if let Some(start_quote) = line.find("'") {
                    if let Some(end_quote) = line[start_quote+1..].find("'") {
                        let opened_file = &line[start_quote+1..start_quote+1+end_quote];
                        if let Some(opened_idx) = purge_input_files.iter().position(|f| f.replace('\\\\', "/").contains(opened_file)) {
                            let mut i = 0;
                            while i < active_norm_files.len() {
                                if active_norm_files[i].0 < opened_idx {
                                    let (idx, path) = active_norm_files.remove(i);
                                    if path.exists() {
                                        if let Err(e) = std::fs::remove_file(&path) {
                                            log::warn!("[FORENSIC:PURGE] Failed to incremental purge File #{}: {}", idx, e);
                                        } else {
                                            log::info!("[FORENSIC:PURGE] Incremental purge success: File #{} ({})", idx, path.display());
                                        }
                                    }
                                } else {
                                    i += 1;
                                }
                            }
                        }
                    }
                }
            }

            if cancel_shared.load(Ordering::Relaxed) {
                break; // Exit thread — main thread handles cleanup
            }

            stderr_log.push(line.clone());
            if stderr_log.len() > 30 {
                stderr_log.remove(0);
            }

            if let Some(block) = block_reader.feed_line(line) {
                if let Some(prog) = parse_progress_block(&block) {
                    let elapsed = prog.out_time_seconds.unwrap_or(0.0).max(0.0);
                    let percent = calc_progress_percent(elapsed, total);

                    let mut current_idx = 0;
                    let mut remaining_dur = total;
                    for (i, seg) in segmented_refs.iter().enumerate() {
                        if elapsed >= seg.start_time && elapsed < seg.end_time {
                            current_idx = i;
                        }
                        if elapsed >= seg.end_time {
                            remaining_dur -= seg.duration;
                        }
                    }
                    if current_idx < segmented_refs.len() {
                        let seg = &segmented_refs[current_idx];
                        let processed_in_seg = (elapsed - seg.start_time).max(0.0);
                        remaining_dur -= processed_in_seg;
                    }

                    let eta = prog.speed
                        .filter(|&s| s > 0.01)
                        .map(|speed| ((total - elapsed) as f32 / speed).max(0.0));

                    let now = Instant::now();
                    if now.duration_since(last_emit_ms).as_millis() >= MIN_EMIT_INTERVAL_MS as u128 {
                        last_emit_ms = now;
                        progress_cb(MergeProgress {
                            percent,
                            current_time: elapsed,
                            total_duration: total,
                            speed: prog.speed,
                            fps: prog.fps,
                            current_file: None,
                            current_segment_index: Some(current_idx),
                            remaining_duration: Some(remaining_dur.max(0.0)),
                            bytes_written: prog.total_size_bytes,
                            eta_seconds: eta,
                            phase: MergePhase::Writing,
                            overall_percent: None,
                            stage_name: Some("Merging...".into()),
                            stage_percent: Some(percent),
                            current_file_index: None,
                            total_files_in_stage: None,
                            warning: None,
                            is_large_playlist: false,
                        });
                    }

                    if prog.is_end { break; }
                }
            }
        }
        stderr_log
    });

    // ── MAIN THREAD: POLL try_wait() WITH TIMEOUT ────────────────────────
    // The stderr thread processes progress and purges files.
    // This thread supervises the child process with a 6-hour timeout.
    const CONCAT_TIMEOUT_SECS: u64 = 21600; // 6 hours for large merges
    let start = Instant::now();
    let status = loop {
        if start.elapsed().as_secs() > CONCAT_TIMEOUT_SECS {
            force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(&config.output_path));
            on_progress(MergeProgress {
                percent: 0.0, current_time: 0.0, total_duration: total,
                speed: None, fps: None, current_file: None,
                current_segment_index: None, remaining_duration: None,
                bytes_written: None, eta_seconds: None,
                phase: MergePhase::Failed, overall_percent: None,
                stage_name: None, stage_percent: None,
                current_file_index: None, total_files_in_stage: None,
                warning: None, is_large_playlist: false,
            });
            return Err(anyhow!("FFmpeg concat timed out after {} seconds", CONCAT_TIMEOUT_SECS));
        }
        if cancel_flag.load(Ordering::Relaxed) {
            force_kill_process_tree(&mut child);
            let _ = child.wait();
            cleanup_partial_output(Path::new(&config.output_path));
            on_progress(MergeProgress {
                percent: 0.0, current_time: 0.0, total_duration: total,
                speed: None, fps: None, current_file: None,
                current_segment_index: None, remaining_duration: None,
                bytes_written: None, eta_seconds: None,
                phase: MergePhase::Cancelled, overall_percent: None,
                stage_name: None, stage_percent: None,
                current_file_index: None, total_files_in_stage: None,
                warning: None, is_large_playlist: false,
            });
            return Err(anyhow!("Merge cancelled by user"));
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("FFmpeg process wait error: {}", e));
            }
        }
    };

    // Join stderr thread to collect any remaining lines
    let stderr_log: Vec<String> = stderr_thread.join().unwrap_or_default();"""

if old_section_start in content:
    content = content.replace(old_section_start, new_section_merged, 1)
    changes_applied += 1
    print("FIX 2: Applied timeout + try_wait to run_merge_blocking()")
else:
    print("FIX 2: Could not find old section - checking for partial match...")
    # Debug: find where the section exists
    idx = content.find("let mut cmd = Command::new(ffmpeg_path);")
    if idx >= 0:
        print(f"  Found 'let mut cmd' at char {idx}")
    else:
        print("  Could not find 'let mut cmd = Command::new(ffmpeg_path)'")

# ============================================================
# FIX 3: Fix the stderr_log usage after the join - it's now a Vec<String>
# The code after the old child.wait() uses stderr_log, and references status
# ============================================================

# After the stderr thread joins, the code reads:
#   log::info!("[FFMPEG_CONCAT_COMPLETE] output={} success={} elapsed={:?}", ...);
#   if !status.success() { ... cleanup_partial_output ... use stderr_log ... }
# This should still work since we named the variable the same.

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print(f"Updated {path} - {changes_applied} changes applied")
