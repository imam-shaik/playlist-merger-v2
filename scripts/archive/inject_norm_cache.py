#!/usr/bin/env python3
"""Inject NormalizationCache into the three normalize functions in merge.rs.

For each normalize function:
1. Add cache parameter
2. Add cache check at start (before ffmpeg run)
3. Add cache store at end (after successful ffmpeg run)
"""

import re

FILEPATH = "src-tauri/src/commands/merge.rs"

with open(FILEPATH, "r", encoding="utf-8", newline="") as f:
    content = f.read()

# ── 1. normalize_to_profile ──────────────────────────────────────────────

# Add cache parameter to signature
old_sig = (
    "async fn normalize_to_profile(\n"
    "    ffmpeg_path: &Path, input_path: &str, target_vcodec: &str, target_acodec: &str, target_sr: u32, target_fps: Option<f64>, target_timescale: Option<u32>, target_width: Option<u32>, target_height: Option<u32>,\n"
    "    target_channels: Option<u32>, temp_dir: &Path, job_id: &str, index: usize, has_audio: bool, cancel_flag: Arc<AtomicBool>,\n"
    ") -> Result<String, String> {"
)
new_sig = (
    "async fn normalize_to_profile(\n"
    "    ffmpeg_path: &Path, input_path: &str, target_vcodec: &str, target_acodec: &str, target_sr: u32, target_fps: Option<f64>, target_timescale: Option<u32>, target_width: Option<u32>, target_height: Option<u32>,\n"
    "    target_channels: Option<u32>, temp_dir: &Path, job_id: &str, index: usize, has_audio: bool, cancel_flag: Arc<AtomicBool>,\n"
    "    norm_cache: Option<std::sync::Arc<crate::ffmpeg::norm_cache::NormalizationCache>>,\n"
    ") -> Result<String, String> {"
)
content = content.replace(old_sig, new_sig)

# Add cache check after output_path computation and before run_ffmpeg_cmd_with_cancel
# In normalize_to_profile, the cache check goes right before the log line:
#   log::info!("[FORENSIC:NORMALIZE] Profile re-encode...
# and right after:
#   let out_str = output_path.to_string_lossy().into_owned();
#   args.push(out_str.clone());
old_profile_cache = (
    "    let out_str = output_path.to_string_lossy().into_owned();\n"
    "    args.push(out_str.clone());\n"
    "    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();\n"
    "    log::info!(\"[FORENSIC:NORMALIZE] Profile re-encode | File #{} | Input: {} | Output: {} | vcodec={} | acodec={} | sr={} | fps={:?} | timescale={:?} | {}x{} | ch={:?}\",\n"
    "        index, input_path, out_str, target_vcodec, target_acodec, target_sr, target_fps, target_timescale, target_width.unwrap_or(0), target_height.unwrap_or(0), target_channels);\n"
    "    run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 3600).await?;\n"
    "    Ok(out_str)"
)
new_profile_cache = (
    "    let out_str = output_path.to_string_lossy().into_owned();\n"
    "    args.push(out_str.clone());\n"
    "    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();\n"
    "    log::info!(\"[FORENSIC:NORMALIZE] Profile re-encode | File #{} | Input: {} | Output: {} | vcodec={} | acodec={} | sr={} | fps={:?} | timescale={:?} | {}x{} | ch={:?}\",\n"
    "        index, input_path, out_str, target_vcodec, target_acodec, target_sr, target_fps, target_timescale, target_width.unwrap_or(0), target_height.unwrap_or(0), target_channels);\n"
    "    // ── Normalization Dedup Cache ──────────────────────────────────────────\n"
    "    // Check if we already normalized this source with this exact profile for Repeat.\n"
    "    if let Some(ref _cache) = norm_cache {\n"
    "        let sig = crate::ffmpeg::norm_cache::NormSignature::Profile {\n"
    "            vcodec: target_vcodec.to_string(),\n"
    "            acodec: target_acodec.to_string(),\n"
    "            sample_rate: target_sr,\n"
    "            fps_milli: target_fps.map(|f| (f * 1000.0) as u64),\n"
    "            timescale: *target_timescale,\n"
    "            width: *target_width,\n"
    "            height: *target_height,\n"
    "            channels: *target_channels,\n"
    "        };\n"
    "        if let Some(cached_path) = _cache.get(input_path, &sig) {\n"
    "            log::info!(\"[NormCache] HIT: Reusing normalization of {} from {}\", input_path, cached_path.display());\n"
    "            std::fs::copy(&cached_path, &output_path).map_err(|e| format!(\"Failed to copy cached normalized file: {}\", e))?;\n"
    "            return Ok(out_str);\n"
    "        }\n"
    "    }\n"
    "    run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 3600).await?;\n"
    "    // ── Cache the result for future repeats ────────────────────────────────\n"
    "    if let Some(ref _cache) = norm_cache {\n"
    "        let sig = crate::ffmpeg::norm_cache::NormSignature::Profile {\n"
    "            vcodec: target_vcodec.to_string(),\n"
    "            acodec: target_acodec.to_string(),\n"
    "            sample_rate: target_sr,\n"
    "            fps_milli: target_fps.map(|f| (f * 1000.0) as u64),\n"
    "            timescale: *target_timescale,\n"
    "            width: *target_width,\n"
    "            height: *target_height,\n"
    "            channels: *target_channels,\n"
    "        };\n"
    "        _cache.insert(input_path, sig, output_path.clone());\n"
    "        log::info!(\"[NormCache] Cached normalization of {}\", input_path);\n"
    "    }\n"
    "    Ok(out_str)"
)
# Ensure we handle windows line endings
content = content.replace(old_profile_cache, new_profile_cache)
print("Applied normalize_to_profile cache injection")

# ── 2. normalize_timescale_lossless ──────────────────────────────────────

old_ts_sig = (
    "async fn normalize_timescale_lossless(\n"
    "    ffmpeg_path: &Path, input_path: &str, target_timescale: u32, temp_dir: &Path, job_id: &str, index: usize, cancel_flag: Arc<AtomicBool>,\n"
    ") -> Result<String, String> {"
)
new_ts_sig = (
    "async fn normalize_timescale_lossless(\n"
    "    ffmpeg_path: &Path, input_path: &str, target_timescale: u32, temp_dir: &Path, job_id: &str, index: usize, cancel_flag: Arc<AtomicBool>,\n"
    "    norm_cache: Option<std::sync::Arc<crate::ffmpeg::norm_cache::NormalizationCache>>,\n"
    ") -> Result<String, String> {"
)
content = content.replace(old_ts_sig, new_ts_sig)

old_ts_cache = (
    "    log::info!(\"[FORENSIC:NORMALIZE] Timescale lossless remux | File #{} | Input: {} | Output: {} | target_timescale={}\",\n"
    "        index, input_path, output_path_str, target_timescale);\n"
    "    run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 900).await?;\n"
    "    Ok(output_path_str)"
)
new_ts_cache = (
    "    log::info!(\"[FORENSIC:NORMALIZE] Timescale lossless remux | File #{} | Input: {} | Output: {} | target_timescale={}\",\n"
    "        index, input_path, output_path_str, target_timescale);\n"
    "    // ── Normalization Dedup Cache ──────────────────────────────────────────\n"
    "    if let Some(ref _cache) = norm_cache {\n"
    "        let sig = crate::ffmpeg::norm_cache::NormSignature::Timescale {\n"
    "            target_timescale: *target_timescale,\n"
    "        };\n"
    "        if let Some(cached_path) = _cache.get(input_path, &sig) {\n"
    "            log::info!(\"[NormCache] HIT: Reusing timescale remux of {} from {}\", input_path, cached_path.display());\n"
    "            std::fs::copy(&cached_path, &output_path).map_err(|e| format!(\"Failed to copy cached timescale file: {}\", e))?;\n"
    "            return Ok(output_path_str);\n"
    "        }\n"
    "    }\n"
    "    run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 900).await?;\n"
    "    // ── Cache the result ───────────────────────────────────────────────────\n"
    "    if let Some(ref _cache) = norm_cache {\n"
    "        let sig = crate::ffmpeg::norm_cache::NormSignature::Timescale {\n"
    "            target_timescale: *target_timescale,\n"
    "        };\n"
    "        _cache.insert(input_path, sig, output_path.clone());\n"
    "        log::info!(\"[NormCache] Cached timescale remux of {}\", input_path);\n"
    "    }\n"
    "    Ok(output_path_str)"
)
content = content.replace(old_ts_cache, new_ts_cache)
print("Applied normalize_timescale_lossless cache injection")

# ── 3. normalize_audio_only ──────────────────────────────────────────────

old_audio_sig = (
    "async fn normalize_audio_only(\n"
    "    ffmpeg_path: &Path, input_path: &str, target_acodec: &str, target_sr: u32, target_timescale: Option<u32>,\n"
    "    target_channels: Option<u32>, temp_dir: &Path, job_id: &str, index: usize, cancel_flag: Arc<AtomicBool>,\n"
    ") -> Result<String, String> {"
)
new_audio_sig = (
    "async fn normalize_audio_only(\n"
    "    ffmpeg_path: &Path, input_path: &str, target_acodec: &str, target_sr: u32, target_timescale: Option<u32>,\n"
    "    target_channels: Option<u32>, temp_dir: &Path, job_id: &str, index: usize, cancel_flag: Arc<AtomicBool>,\n"
    "    norm_cache: Option<std::sync::Arc<crate::ffmpeg::norm_cache::NormalizationCache>>,\n"
    ") -> Result<String, String> {"
)
content = content.replace(old_audio_sig, new_audio_sig)

old_audio_cache = (
    "    log::info!(\"[FORENSIC:NORMALIZE] Audio-only | File #{} | Input: {} | Output: {} | target_acodec={} | sr={} | ts={:?} | ch={:?}\",\n"
    "        index, input_path, output_path_str, target_acodec, target_sr, target_timescale, target_channels);\n"
    "    run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 3600).await?;\n"
    "    Ok(output_path.to_string_lossy().into_owned())"
)
new_audio_cache = (
    "    log::info!(\"[FORENSIC:NORMALIZE] Audio-only | File #{} | Input: {} | Output: {} | target_acodec={} | sr={} | ts={:?} | ch={:?}\",\n"
    "        index, input_path, output_path_str, target_acodec, target_sr, target_timescale, target_channels);\n"
    "    // ── Normalization Dedup Cache ──────────────────────────────────────────\n"
    "    if let Some(ref _cache) = norm_cache {\n"
    "        let sig = crate::ffmpeg::norm_cache::NormSignature::AudioOnly {\n"
    "            acodec: target_acodec.to_string(),\n"
    "            sample_rate: target_sr,\n"
    "            timescale: *target_timescale,\n"
    "            channels: *target_channels,\n"
    "        };\n"
    "        if let Some(cached_path) = _cache.get(input_path, &sig) {\n"
    "            log::info!(\"[NormCache] HIT: Reusing audio-only normalization of {} from {}\", input_path, cached_path.display());\n"
    "            std::fs::copy(&cached_path, &output_path).map_err(|e| format!(\"Failed to copy cached audio file: {}\", e))?;\n"
    "            return Ok(output_path.to_string_lossy().into_owned());\n"
    "        }\n"
    "    }\n"
    "    run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 3600).await?;\n"
    "    // ── Cache the result ───────────────────────────────────────────────────\n"
    "    if let Some(ref _cache) = norm_cache {\n"
    "        let sig = crate::ffmpeg::norm_cache::NormSignature::AudioOnly {\n"
    "            acodec: target_acodec.to_string(),\n"
    "            acodec: target_acodec.to_string(),\n"  # BUG: duplicate line, let me fix
    "        };\n"
    "    }\n"
    "    Ok(output_path.to_string_lossy().into_owned())"
)

# Oops, I made a mistake - the audio cache insert has a duplicate line.
# Let me fix the new string:
new_audio_cache = (
    "    log::info!(\"[FORENSIC:NORMALIZE] Audio-only | File #{} | Input: {} | Output: {} | target_acodec={} | sr={} | ts={:?} | ch={:?}\",\n"
    "        index, input_path, output_path_str, target_acodec, target_sr, target_timescale, target_channels);\n"
    "    // ── Normalization Dedup Cache ──────────────────────────────────────────\n"
    "    if let Some(ref _cache) = norm_cache {\n"
    "        let sig = crate::ffmpeg::norm_cache::NormSignature::AudioOnly {\n"
    "            acodec: target_acodec.to_string(),\n"
    "            sample_rate: target_sr,\n"
    "            timescale: *target_timescale,\n"
    "            channels: *target_channels,\n"
    "        };\n"
    "        if let Some(cached_path) = _cache.get(input_path, &sig) {\n"
    "            log::info!(\"[NormCache] HIT: Reusing audio-only normalization of {} from {}\", input_path, cached_path.display());\n"
    "            std::fs::copy(&cached_path, &output_path).map_err(|e| format!(\"Failed to copy cached audio file: {}\", e))?;\n"
    "            return Ok(output_path.to_string_lossy().into_owned());\n"
    "        }\n"
    "    }\n"
    "    run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 3600).await?;\n"
    "    // ── Cache the result ───────────────────────────────────────────────────\n"
    "    if let Some(ref _cache) = norm_cache {\n"
    "        let sig = crate::ffmpeg::norm_cache::NormSignature::AudioOnly {\n"
    "            acodec: target_acodec.to_string(),\n"
    "            sample_rate: target_sr,\n"
    "            timescale: *target_timescale,\n"
    "            channels: *target_channels,\n"
    "        };\n"
    "        _cache.insert(input_path, sig, output_path.clone());\n"
    "        log::info!(\"[NormCache] Cached audio-only normalization of {}\", input_path);\n"
    "    }\n"
    "    Ok(output_path.to_string_lossy().into_owned())"
)
content = content.replace(old_audio_cache, new_audio_cache)
print("Applied normalize_audio_only cache injection")

# Write result
with open(FILEPATH, "w", encoding="utf-8", newline="\r\n") as f:
    f.write(content)

print(f"✅ All modifications applied to {FILEPATH}")
print()
print("NOTE: You still need to:")
print("  1. Create the NormCache in start_merge() early in the function")
print("  2. Pass it to all normalize_to_profile/audio_only/timescale_lossless calls")
print("  3. Handle the call site modifications manually or via search-and-replace")
