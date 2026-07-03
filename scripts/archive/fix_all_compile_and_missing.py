#!/usr/bin/env python3
"""Fix all remaining compilation errors and apply missing hang fixes to split_merge + fast_mkv."""
import os

PROJECT = os.path.dirname(os.path.dirname(__file__))

# ============================================================
# FIX: concat.rs - Fix compilation errors and apply split_merge fix
# ============================================================
concat_path = os.path.join(PROJECT, 'src-tauri', 'src', 'ffmpeg', 'concat.rs')
with open(concat_path, 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Fix F bounds: add Sync + 'static
old_sig = "    F: Fn(MergeProgress) + Send + 'static,\n{"
new_sig = "    F: Fn(MergeProgress) + Send + Sync + 'static,\n{"
if old_sig in content:
    content = content.replace(old_sig, new_sig, 1)
    print("FIX 1: Added Sync bound to F in run_merge_blocking")
else:
    print("FIX 1: Could not find old F bounds")

# 2. Revert the on_progress -> progress_cb replacement in the SPLIT MERGE function
# The Python script replaced ALL `on_progress(` with `progress_cb(` which affected
# run_split_merge_blocking() that doesn't have the Arc wrapper.
# 
# The split merge function is AFTER the run_merge_blocking function.
# Let me find where run_split_merge_blocking starts and revert changes there.
marker = "fn run_split_merge_blocking<F>"
idx = content.find(marker)
if idx >= 0:
    # Find the first occurrence of `progress_cb(` AFTER the marker
    content_before = content[:idx]
    content_from_split = content[idx:]
    
    # In the split merge section, revert progress_cb( back to on_progress(
    content_from_split = content_from_split.replace("progress_cb(MergeProgress {", "on_progress(MergeProgress {")
    content = content_before + content_from_split
    print("FIX 2: Reverted on_progress in split merge function")
else:
    print("FIX 2: Could not find split merge function marker")

# 3. NOW apply the same timeout + try_wait fix to run_split_merge_blocking()
# But first let me check the structure of the split merge parts loop.
# The split merge function has a for loop over parts, each part spawns FFmpeg.
# We need to find the part where FFmpeg is spawned and stderr is read.

# Find the pattern in split merge: spawn_ffmpeg for a part
old_split_pattern = """        cmd.args(&args);
        let mut child = spawn_ffmpeg(&mut cmd)
            .with_context(|| format!("Failed to spawn ffmpeg for part {}", part_num))?;

        let stderr = child.stderr.take().ok_or_else(|| {
            anyhow::anyhow!("INTERNAL ERROR: stderr not available for part {} - spawn_ffmpeg misconfiguration", part_num)
        })?;
        let reader = BufReader::new(stderr);

        for line_result in reader.lines() {
            if cancel_flag.load(Ordering::Relaxed) {
                force_kill_process_tree(&mut child);
                let _ = child.wait();
                // Clean up all outputs including this part
                cleanup_partial_output(Path::new(&part.output_path));
                on_progress(MergeProgress {
                    percent: 0.0,
                    current_time: 0.0,
                    total_duration,
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

            let line = match line_result {
                Ok(ref l) => l,
                Err(_) => break,
            };

            stderr_log.push(line.clone());

            if let Some(block) = block_reader.feed_line(line) {
                if let Some(prog) = parse_progress_block(&block) {
                    let elapsed = prog.out_time_seconds.unwrap_or(0.0).max(0.0);
                    let percent = calc_progress_percent(elapsed, part_total_dur);

                    let eta = prog.speed
                        .filter(|&s| s > 0.01)
                        .map(|speed| ((part_total_dur - elapsed) as f32 / speed).max(0.0));

                    let global_time = part.start_time + elapsed;
                    let global_pct = ((global_time / total_duration) * 100.0) as f32;

                    let now = std::time::Instant::now();
                    if now.duration_since(last_emit).as_millis() >= 100 {
                        last_emit = now;
                        on_progress(MergeProgress {
                            percent: global_pct,
                            current_time: global_time,
                            total_duration,
                            speed: prog.speed,
                            fps: prog.fps,
                            current_file: None,
                            current_segment_index: None,
                            remaining_duration: Some((total_duration - global_time).max(0.0)),
                            bytes_written: prog.total_size_bytes,
                            eta_seconds: eta,
                            phase: MergePhase::Writing,
                            overall_percent: Some(global_pct),
                            stage_name: Some(format!("Part {}/{} (Merging...)", part_num, total_parts)),
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

        let status = child.wait().with_context(|| format!("FFmpeg part {} failed", part_num))?;"""

new_split_pattern = """        cmd.args(&args);
        let mut child = spawn_ffmpeg(&mut cmd)
            .with_context(|| format!("Failed to spawn ffmpeg for part {}", part_num))?;

        let stderr = child.stderr.take().ok_or_else(|| {
            anyhow::anyhow!("INTERNAL ERROR: stderr not available for part {} - spawn_ffmpeg misconfiguration", part_num)
        })?;

        // ── BACKGROUND STREAMING THREAD for this part ───────────────────
        let total_dur = total_duration;
        let part_td = part_total_dur;
        let part_st = part.start_time;
        let pn = part_num;
        let tp = total_parts;
        let output_path_clone = part.output_path.clone();
        let cancel_shared = cancel_flag.clone();
        let part_progress = on_progress;
        
        let stderr_thread = std::thread::spawn(move || {
            let mut stderr_log_local: Vec<String> = Vec::new();
            let reader = BufReader::new(stderr);
            let mut block_reader = ProgressBlockReader::new();
            let mut last_emit = Instant::now();

            for line_result in reader.lines() {
                if cancel_shared.load(Ordering::Relaxed) {
                    break;
                }
                let line = match line_result {
                    Ok(ref l) => l,
                    Err(_) => break,
                };
                stderr_log_local.push(line.clone());

                if let Some(block) = block_reader.feed_line(line) {
                    if let Some(prog) = parse_progress_block(&block) {
                        let elapsed = prog.out_time_seconds.unwrap_or(0.0).max(0.0);
                        let percent = calc_progress_percent(elapsed, part_td);
                        let global_time = part_st + elapsed;
                        let global_pct = ((global_time / total_dur) * 100.0) as f32;
                        let eta = prog.speed
                            .filter(|&s| s > 0.01)
                            .map(|speed| ((part_td - elapsed) as f32 / speed).max(0.0));

                        let now = Instant::now();
                        if now.duration_since(last_emit).as_millis() >= 100 {
                            last_emit = now;
                            part_progress(MergeProgress {
                                percent: global_pct,
                                current_time: global_time,
                                total_duration: total_dur,
                                speed: prog.speed,
                                fps: prog.fps,
                                current_file: None,
                                current_segment_index: None,
                                remaining_duration: Some((total_dur - global_time).max(0.0)),
                                bytes_written: prog.total_size_bytes,
                                eta_seconds: eta,
                                phase: MergePhase::Writing,
                                overall_percent: Some(global_pct),
                                stage_name: Some(format!("Part {}/{} (Merging...)", pn, tp)),
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
            stderr_log_local
        });

        // ── MAIN THREAD: POLL try_wait() WITH TIMEOUT ──────────────────
        const PART_TIMEOUT_SECS: u64 = 21600;
        let part_start = Instant::now();
        let status = loop {
            if part_start.elapsed().as_secs() > PART_TIMEOUT_SECS {
                force_kill_process_tree(&mut child);
                let _ = child.wait();
                cleanup_partial_output(Path::new(&output_path_clone));
                return Err(anyhow!("FFmpeg part {} timed out after {} seconds", part_num, PART_TIMEOUT_SECS));
            }
            if cancel_flag.load(Ordering::Relaxed) {
                force_kill_process_tree(&mut child);
                let _ = child.wait();
                cleanup_partial_output(Path::new(&output_path_clone));
                on_progress(MergeProgress {
                    percent: 0.0, current_time: 0.0, total_duration,
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
                Ok(None) => { std::thread::sleep(Duration::from_millis(250)); }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(anyhow!("FFmpeg part {} wait error: {}", part_num, e));
                }
            }
        };

        // Join stderr thread for this part
        let part_stderr: Vec<String> = stderr_thread.join().unwrap_or_default();
        // Merge into main stderr_log
        stderr_log.extend(part_stderr);"""

# Check if the old pattern exists (it might have been modified by previous scripts)
if old_split_pattern in content:
    content = content.replace(old_split_pattern, new_split_pattern, 1)
    print("FIX 3: Applied try_wait + timeout to run_split_merge_blocking()")
else:
    print("FIX 3: Could not find old split merge pattern - checking for partial matches...")
    # Let's check if the content still has parts of the split merge
    if "spawn_ffmpeg" in content[idx:]:
        print("  Found spawn_ffmpeg in split merge section")
    # Check if split merge is still using child.wait()
    if "child.wait()" in content[idx:]:
        print("  Found child.wait() in split merge section - needs manual fix")

with open(concat_path, 'w', encoding='utf-8') as f:
    f.write(content)
print("concat.rs done")

# ============================================================
# FIX: fast_mkv.rs - Apply try_wait + timeout to run_fast_mkv_merge and convert_mkv_to_mp4
# ============================================================
fast_path = os.path.join(PROJECT, 'src-tauri', 'src', 'ffmpeg', 'fast_mkv.rs')
with open(fast_path, 'r', encoding='utf-8') as f:
    fast = f.read()

fast_changes = 0

# Fix run_fast_mkv_merge: replace `let _ = child.wait();` with try_wait polling
# The function has two `let _ = child.wait();` - one for cancel and one for normal exit
# Both need to be wrapped in try_wait polling

# First, find the place where the normal child.wait() happens
# It's the last one in the function: `let _ = child.wait();` followed by concat list removal
old_wait_normal = """    let _ = child.wait();
    let _ = std::fs::remove_file(&concat_list_path);

    if !std::path::Path::new(output_path).exists() {
        anyhow::bail!("FFmpeg completed but output file was not created");
    }"""

new_wait_normal = """    let fast_start = Instant::now();
    const FAST_TIMEOUT_SECS: u64 = 21600;
    let fast_status = loop {
        if fast_start.elapsed().as_secs() > FAST_TIMEOUT_SECS {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(&concat_list_path);
            anyhow::bail!("Fast MKV merge timed out after {} seconds", FAST_TIMEOUT_SECS);
        }
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(&concat_list_path);
            anyhow::bail!("Merge cancelled");
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => { let _ = child.kill(); let _ = child.wait(); anyhow::bail!("FFmpeg wait error: {}", e); }
        }
    };
    let _ = std::fs::remove_file(&concat_list_path);"""

if old_wait_normal in fast:
    fast = fast.replace(old_wait_normal, new_wait_normal, 1)
    fast_changes += 1
    print("FIX 4: Applied try_wait + timeout to run_fast_mkv_merge()")
else:
    print("FIX 4: Could not find normal child.wait() in run_fast_mkv_merge")

# Fix convert_mkv_to_mp4: same pattern
# The function has `let _ = child.wait();` near the end
old_conv_wait = """    let _ = child.wait();

    if !std::path::Path::new(mp4_path).exists() {
        anyhow::bail!("FFmpeg completed but MP4 output file was not created");
    }"""

new_conv_wait = """    let convert_start = Instant::now();
    const CONVERT_TIMEOUT_SECS: u64 = 21600;
    loop {
        if convert_start.elapsed().as_secs() > CONVERT_TIMEOUT_SECS {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(mp4_path);
            anyhow::bail!("MP4 conversion timed out after {} seconds", CONVERT_TIMEOUT_SECS);
        }
        if cancel_flag.load(Ordering::Relaxed) {
            crate::ffmpeg::force_kill_process_tree(&mut child);
            let _ = child.wait();
            let _ = std::fs::remove_file(mp4_path);
            anyhow::bail!("MP4 conversion cancelled");
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => { let _ = child.kill(); let _ = child.wait(); anyhow::bail!("FFmpeg wait error: {}", e); }
        }
    }"""

if old_conv_wait in fast:
    fast = fast.replace(old_conv_wait, new_conv_wait, 1)
    fast_changes += 1
    print("FIX 5: Applied try_wait + timeout to convert_mkv_to_mp4()")
else:
    print("FIX 5: Could not find child.wait() in convert_mkv_to_mp4")

with open(fast_path, 'w', encoding='utf-8') as f:
    f.write(fast)
print(f"fast_mkv.rs: {fast_changes} fixes applied")
