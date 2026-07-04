pub mod probe;
pub mod concat;
pub mod progress;
pub mod cards;
pub mod probe_cache;
pub mod normalization;
pub mod fast_mkv;
pub mod repeat;
pub mod repeat_merge;
pub mod norm_cache;
pub mod immutability;
pub mod mkvmerge;
pub mod section_planner;
pub mod subtitle_timeline;
pub mod verification;
pub mod media_validation_engine;

pub use concat::force_kill_process_tree;

#[cfg(test)]
pub mod normalization_pipeline_tests;

#[cfg(test)]
pub mod sample_rate_compatibility_test;

#[cfg(test)]
pub mod sample_rate_benchmark_test;

#[cfg(test)]
pub mod outlier_certification_test;

#[cfg(test)]
pub mod false_positive_impact_test;

#[cfg(test)]
pub mod post_fix_certification_test;

#[cfg(test)]
pub mod audio_seek_forensics_test;

#[cfg(test)]
pub mod audio_seek_dense_audit_test;

#[cfg(test)]
pub mod aac_profile_certification_test;

#[cfg(test)]
pub mod audio_seek_root_cause;

#[cfg(test)]
pub mod audio_corruption_forensics;

#[cfg(test)]
pub mod boundary_isolation;

#[cfg(test)]
pub mod sample_rate_safety_certification;

#[cfg(test)]
pub mod production_path_verification;

#[cfg(test)]
pub mod pts_dts_timeline_audit;

#[cfg(test)]
pub mod keyframe_boundary_audit;

#[cfg(test)]
pub mod boundary_correlation_audit;

#[cfg(test)]
pub mod codec_transition_fix_verify;

#[cfg(test)]
pub mod post_fix_regression_certification;

#[cfg(test)]
pub mod large_playlist_scalability_audit;

#[cfg(test)]
pub mod normalization_scaling_audit;

#[cfg(test)]
pub mod cancellation_recovery_audit;

#[cfg(test)]
pub mod tempcleanup_certification;

#[cfg(test)]
pub mod regression_tests;

#[cfg(test)]
pub mod production_certification_matrix;

#[cfg(test)]
pub mod normalization_performance_audit;

#[cfg(test)]
pub mod audio_transformation_trace;

#[cfg(test)]
pub mod audio_codec_certification_test;

#[cfg(test)]
pub mod smartmkv_decision_classification_test;

#[cfg(test)]
pub mod subtitle_sync_certification_test;

#[cfg(test)]
pub mod subtitle_randomized_stress_test;

pub mod subtitle_audit;

#[cfg(test)]
pub mod card_production_certification;

#[cfg(test)]
pub mod timeline_consistency_audit;

#[cfg(test)]
pub mod split_certification;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use anyhow::{Result, anyhow};
use once_cell::sync::Lazy;
use tokio::sync::Semaphore;

/// Global FFmpeg process governor.
/// Limits total concurrent FFmpeg processes across all phases to prevent
/// CPU oversubscription and process explosion.
///
/// Target: 8-12 concurrent FFmpeg globally (configurable via FFMPEG_GLOBAL_MAX).
static FFMPEG_GLOBAL_SEM: Lazy<Arc<Semaphore>> = Lazy::new(|| {
    let max_concurrent = std::env::var("FFMPEG_GLOBAL_MAX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10); // Default: 10 concurrent FFmpeg processes
    log::info!("[FFmpegGovernor] Global FFmpeg semaphore initialized with {} permits", max_concurrent);
    Arc::new(Semaphore::new(max_concurrent))
});

/// Get the global FFmpeg semaphore Arc for acquiring permits.
/// Call `.acquire().await` on the returned Arc to get a permit.
///
/// # Example
/// ```ignore
/// let sem = ffmpeg::ffmpeg_global_semaphore();
/// let _permit = sem.acquire().await?;
/// // Now safe to spawn FFmpeg process
/// let child = Command::new("ffmpeg")...spawn()?;
/// // _permit is dropped when done, releasing the permit
/// ```
pub fn ffmpeg_global_semaphore() -> Arc<Semaphore> {
    FFMPEG_GLOBAL_SEM.clone()
}

/// Locate ffmpeg binary — checks:
/// 1. Settings override path
/// 2. App bundled binaries next to executable
/// 3. System PATH
pub fn find_ffmpeg(override_path: Option<&str>) -> Result<PathBuf> {
    find_binary("ffmpeg", override_path)
}

pub fn find_ffprobe(override_path: Option<&str>) -> Result<PathBuf> {
    find_binary("ffprobe", override_path)
}

fn find_binary(name: &str, override_path: Option<&str>) -> Result<PathBuf> {
    // 1. Explicit override
    if let Some(p) = override_path {
        let p_trimmed = p.trim();
        if !p_trimmed.is_empty() {
            let path = PathBuf::from(p_trimmed);
            if is_valid_executable(&path) {
                return Ok(path);
            }
            // If the override path is a directory, check if the binary exists inside it
            if path.is_dir() {
                let bin_name = if cfg!(target_os = "windows") {
                    format!("{}.exe", name)
                } else {
                    name.to_string()
                };
                let candidate = path.join(&bin_name);
                if is_valid_executable(&candidate) {
                    return Ok(candidate);
                }
            }
            return Err(anyhow!("Configured binary '{}' not found or invalid at: {}", name, p_trimmed));
        }
    }

    // Binary filename (windows: .exe)
    let bin_name = if cfg!(target_os = "windows") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };

    // 2. Check current working directory (useful during development)
    if let Ok(cwd) = std::env::current_dir() {
        let candidates = [
            cwd.join("binaries").join(&bin_name),
            cwd.join(&bin_name),
            cwd.join("src-tauri").join("binaries").join(&bin_name),
        ];
        for candidate in &candidates {
            if is_valid_executable(candidate) {
                log::info!("Found {} in cwd: {}", name, candidate.display());
                return Ok(candidate.clone());
            }
        }
    }

    // 3. Bundled alongside executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidates = [
                dir.join("binaries").join(&bin_name),
                dir.join(&bin_name),
            ];
            for candidate in &candidates {
                if is_valid_executable(candidate) {
                    log::info!("Found bundled {}: {}", name, candidate.display());
                    return Ok(candidate.clone());
                }
            }
        }
    }

    // 4. System PATH
    which_bin(&bin_name)
        .ok_or_else(|| anyhow!(
            "{} not found. Install it (https://ffmpeg.org/download.html) or set a custom path in Settings.",
            name
        ))
}

fn is_valid_executable(path: &Path) -> bool {
    if !path.exists() || !path.is_file() {
        return false;
    }

    // Skip small placeholder files (Git LFS pointers or "placeholder" text)
    // Real ffmpeg/ffprobe are always > 1MB.
    if let Ok(meta) = path.metadata() {
        if meta.len() < 1000 {
            return false;
        }
    }

    // Verify it actually runs
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = std::process::Command::new(path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.arg("-version").output();

    match output {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

fn which_bin(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|p| p.exists())
    })
}

/// Stable temp directory for concat lists
pub fn get_temp_dir() -> Result<PathBuf> {
    let tmp = std::env::temp_dir().join("playlist_merger");
    std::fs::create_dir_all(&tmp)
        .map_err(|e| anyhow!("Cannot create temp dir: {}", e))?;
    Ok(tmp)
}

/// Write an ffmpeg concat list with optional per-file durations.
///
/// The `include_duration` parameter controls whether `duration` directives
/// are written. For stream copy (`-c copy`), duration directives should be
/// OMITTED because the concat demuxer reads each file until EOF — the duration
/// field is only necessary for re-encoding mode where FFmpeg needs to know
/// how much content to process from each input. Writing duration for VFR files
/// in stream copy mode causes the concat demuxer to mis-read file boundaries
/// and inflate the output timeline (observed as ~2x duration inflation).
pub fn write_concat_list_with_durations(
    files: &[&Path],
    durations: Option<&[f64]>,
    list_path: &Path,
    include_duration: bool,
) -> Result<()> {
    use std::fmt::Write as _;

    if let Some(d) = durations {
        if d.len() != files.len() {
            return Err(anyhow!(
                "Durations slice length ({}) does not match files length ({})",
                d.len(), files.len()
            ));
        }
    }

    let mut content = String::with_capacity(files.len() * 80);

    for (i, file) in files.iter().enumerate() {
        // FFmpeg's concat demuxer format 'file 'C:/path'' strictly requires 
        // forward slashes OR escaped backslashes inside single quotes.
        // We use forward slashes for maximum cross-platform compatibility.
        let mut raw = file.to_string_lossy().replace('\\', "/");

        // Strip //?/ prefix for concat demuxer compatibility.
        // The concat demuxer does not support the \\?\ extended-length path prefix,
        // even on modern Windows builds. FFmpeg's internal I/O layer handles
        // long paths correctly if we provide the standard drive/UNC format.
        if cfg!(windows) {
            if let Some(stripped) = raw.strip_prefix("//?/") {
                if let Some(unc_part) = stripped.strip_prefix("UNC/") {
                    // //?/UNC/server/share → //server/share
                    raw = format!("/{}", unc_part);
                } else {
                    // //?/C:/path → C:/path
                    raw = stripped.to_string();
                }
            }
        }

        // Security: reject paths with newlines or null bytes
        if raw.contains('\n') || raw.contains('\r') || raw.contains('\0') {
            return Err(anyhow!(
                "File path contains invalid characters (newline/null): {}",
                raw
            ));
        }

        // In the FFmpeg concat demuxer, file paths are single-quoted ('...').
        // Inside single quotes, the ONLY special character is the single quote itself,
        // which is escaped as '\''. Hashes (#) and backslashes are literal.
        let escaped = raw.replace('\'', "'\\''");

        writeln!(content, "file '{}'", escaped)
            .map_err(|e| anyhow!("Write error: {}", e))?;

        // Add duration directive ONLY when requested AND available.
        // For stream copy (-c copy): OMIT duration — concat demuxer reads until EOF.
        // For re-encoding (Custom mode): include duration to bound processing.
        if include_duration {
            if let Some(durations) = durations {
                let dur = durations[i];
                if dur > 0.0 {
                    writeln!(content, "duration {}", dur)
                        .map_err(|e| anyhow!("Write error: {}", e))?;
                }
            }
        }
    }

    std::fs::write(list_path, &content)
        .map_err(|e| anyhow!("Failed to write concat list to {}: {}", list_path.display(), e))?;

    log::debug!("Wrote concat list ({} files, duration={}) to {}", files.len(), include_duration, list_path.display());
    Ok(())
}

/// Write an ffmpeg concat list for subtitles.
/// If a segment is missing a subtitle, a dummy empty subtitle file is created
/// to maintain synchronization with the video segments.
pub fn write_subtitle_concat_list(
    subs: &[Option<String>],
    durations: &[f64],
    list_path: &Path,
    temp_dir: &Path,
    ext: &str,
) -> Result<()> {
    use std::fmt::Write as _;

    if subs.len() != durations.len() {
        return Err(anyhow!("Subtitles and durations count mismatch"));
    }

    let mut content = String::with_capacity(subs.len() * 80);
    
    // Create a dummy empty subtitle file for the specific extension
    let dummy_path = temp_dir.join(format!("dummy.{}", ext));
    if !dummy_path.exists() {
        let dummy_content = match ext {
            "srt" => "1\n00:00:00,000 --> 00:00:00,001\n \n",
            "vtt" => "WEBVTT\n\n00:00.000 --> 00:00.001\n ",
            _ => "", // Other formats might not play well with empty content but we try
        };
        std::fs::write(&dummy_path, dummy_content)?;
    }

    for (i, sub_opt) in subs.iter().enumerate() {
        let raw = if let Some(path) = sub_opt {
            path.replace('\\', "/")
        } else {
            // Use dummy file
            dummy_path.to_string_lossy().replace('\\', "/")
        };

        // Strip //?/ prefix for concat demuxer compatibility (same as video concat list)
        let unprefixed = if cfg!(windows) {
            if let Some(stripped) = raw.strip_prefix("//?/") {
                if let Some(unc_part) = stripped.strip_prefix("UNC/") {
                    format!("/{}", unc_part)
                } else {
                    stripped.to_string()
                }
            } else {
                raw
            }
        } else {
            raw
        };

        // Duration of this segment (FFmpeg concat demuxer can take duration hint)
        // This is important to keep multiple inputs in sync.
        let escaped = unprefixed.replace('\'', "'\\''");
        writeln!(content, "file '{}'", escaped)?;
        writeln!(content, "duration {}", durations[i])?;
    }

    std::fs::write(list_path, &content)?;
    Ok(())
}

/// Generate a standalone merged SRT file from a subtitle concat list using FFmpeg.
///
/// Reads the subtitle concat list (which points to individual SRT files with durations),
/// runs FFmpeg to concatenate them into a single SRT with proper continuous timestamps,
/// and writes the result to `output_srt_path`.
///
/// Returns `Ok(())` on success.
pub fn generate_merged_srt(
    ffmpeg_path: &Path,
    subtitle_concat_list_path: &Path,
    output_srt_path: &Path,
) -> Result<()> {
    if !subtitle_concat_list_path.exists() {
        return Err(anyhow!("Subtitle concat list not found: {:?}", subtitle_concat_list_path));
    }

    log::info!(
        "[SRTExport] Generating merged SRT from {} → {}",
        subtitle_concat_list_path.display(),
        output_srt_path.display()
    );

    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let mut cmd = std::process::Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .args([
            "-y",
            "-f", "concat",
            "-safe", "0",
            "-i", subtitle_concat_list_path.to_string_lossy().as_ref(),
            "-c:s", "srt",
            output_srt_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|e| anyhow!("Failed to spawn FFmpeg for SRT export: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg SRT export failed: {}", stderr));
    }

    if !output_srt_path.exists() {
        return Err(anyhow!("FFmpeg SRT export completed but output file not found: {:?}", output_srt_path));
    }

    let size = std::fs::metadata(output_srt_path).map(|m| m.len()).unwrap_or(0);
    log::info!("[SRTExport] Merged SRT written: {} ({} bytes)", output_srt_path.display(), size);
    Ok(())
}

/// Clean up a partial output file after a failed or cancelled merge.
 ///
 /// LOGIC:
 /// - If mkvmerge_succeeded is true and output exists, RENAME it to preserve evidence
 ///   (a valid mkvmerge output should NEVER be deleted)
 /// - Otherwise, delete the output file
 ///
 /// Logs but does NOT error — cleanup failures must not mask the real error.
 pub fn cleanup_partial_output(output_path: &Path, mkvmerge_succeeded: bool) {
     if !output_path.exists() {
         return;
     }

     if mkvmerge_succeeded {
         let preserved_path = output_path.with_extension("mkv.preserved");
         match std::fs::rename(output_path, &preserved_path) {
             Ok(()) => {
                 log::warn!("[PRESERVE_EVIDENCE] ⚠️  FFmpeg failed but mkvmerge output exists — PRESERVED at: {}", preserved_path.display());
                 log::warn!("[PRESERVE_EVIDENCE] A valid output was saved instead of deleted. This preserves forensic evidence.");
             }
             Err(e) => {
                 log::error!("[PRESERVE_EVIDENCE] ⚠️  Failed to preserve mkvmerge output: {}", e);
                 log::error!("[PRESERVE_EVIDENCE] Original file may still exist, attempting delete as fallback...");
                 if let Err(e2) = std::fs::remove_file(output_path) {
                     log::error!("[PRESERVE_EVIDENCE] Also failed to delete: {}", e2);
                 }
             }
         }
     } else {
         match std::fs::remove_file(output_path) {
             Ok(()) => log::info!("[CLEANUP] Removed partial output: {}", output_path.display()),
             Err(e) => log::warn!("[CLEANUP] Failed to remove partial output {}: {}", output_path.display(), e),
         }
     }
 }

/// Convenience wrapper for backward compatibility (when mkvmerge didn't run)
  #[allow(dead_code)]
  pub fn _cleanup_partial_output_simple(output_path: &Path) {
      cleanup_partial_output(output_path, false);
  }

/// Generate a merged SRT file from individual SRTs using FFmpeg's concat demuxer.
///
/// Writes a concat list pointing to the original (unmodified) SRT files with duration
/// directives, then runs FFmpeg to concatenate them. The concat demuxer handles timeline
/// rebasing automatically — each segment's timestamps are shifted by the cumulative
/// duration of all preceding segments.
///
/// For a timeline like:
///   Video1 (0-7s) + Video2 (7-19s) + Video3 (19-28s)
/// The output SRT should have:
///   Video1 cues at original positions (00:00:03, etc.)
///   Video2 cues shifted by +7s (00:00:10, etc.)
///   Video3 cues shifted by +19s (00:00:22, etc.)
///
/// NOTE: This function previously pre-shifted timestamps before feeding them to the
/// concat demuxer, causing a double-offset bug (manual offset + demuxer offset).
/// The fix removes the manual shifting and lets the concat demuxer perform the
/// single, correct rebase via duration directives.
///
/// # Arguments
/// * `subs` - Array of optional SRT file paths (one per segment)
/// * `durations` - Duration of each segment (used for cumulative offset calculation)
/// * `ffmpeg_path` - Path to FFmpeg executable
/// * `output_srt_path` - Path for the output merged SRT
pub fn generate_merged_srt_with_rebase(
    subs: &[Option<String>],
    durations: &[f64],
    ffmpeg_path: &Path,
    output_srt_path: &Path,
) -> Result<()> {
    use std::fmt::Write as _;
    use crate::ffmpeg::subtitle_timeline::SubtitleTimeline;

    if subs.len() != durations.len() {
        anyhow::bail!("Subs and durations count mismatch");
    }

    let temp_dir = std::env::temp_dir();

    // Create dummy file for missing subtitles
    let dummy_path = temp_dir.join("dummy_subtitle.srt");
    if !dummy_path.exists() {
        std::fs::write(&dummy_path, "1\n00:00:00,000 --> 00:00:00,001\n \n")?;
    }

    // Build concat list using ORIGINAL (unmodified) SRT files.
    // The concat demuxer rebases timestamps automatically based on duration directives.
    let concat_list_path = temp_dir.join("concat_rebased_subtitles.txt");
    let mut content = String::new();
    for (i, sub_opt) in subs.iter().enumerate() {
        let raw = if let Some(path_str) = sub_opt {
            let path = Path::new(path_str);
            if path.exists() {
                path_str.replace('\\', "/")
            } else {
                log::warn!("[SRT] SRT file {:?} does not exist, using dummy", path_str);
                dummy_path.to_string_lossy().replace('\\', "/")
            }
        } else {
            dummy_path.to_string_lossy().replace('\\', "/")
        };

        // Strip //?/ prefix for concat demuxer compatibility
        let unprefixed = if cfg!(windows) {
            if let Some(stripped) = raw.strip_prefix("//?/") {
                if let Some(unc_part) = stripped.strip_prefix("UNC/") {
                    format!("/{}", unc_part)
                } else {
                    stripped.to_string()
                }
            } else {
                raw
            }
        } else {
            raw
        };

        let escaped = unprefixed.replace('\'', "'\\''");
        writeln!(content, "file '{}'", escaped)?;
        writeln!(content, "duration {}", durations[i])?;
    }
    std::fs::write(&concat_list_path, &content)?;

    log::info!(
        "[SRTExport] Concat list written: {} ({} segments, {} bytes)",
        concat_list_path.display(),
        subs.len(),
        content.len()
    );

    // Run FFmpeg concat demuxer
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = std::process::Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let output = cmd
        .args([
            "-y",
            "-f", "concat",
            "-safe", "0",
            "-i", concat_list_path.to_string_lossy().as_ref(),
            "-c:s", "srt",
            output_srt_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|e| anyhow!("Failed to spawn FFmpeg for SRT export: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg SRT export failed: {}", stderr));
    }

    if !output_srt_path.exists() {
        return Err(anyhow!("FFmpeg SRT export completed but output file not found: {:?}", output_srt_path));
    }

    let size = std::fs::metadata(output_srt_path).map(|m| m.len()).unwrap_or(0);
    log::info!("[SRTExport] Merged SRT written: {} ({} bytes)", output_srt_path.display(), size);

    // Validate the output
    if let Ok(content) = std::fs::read_to_string(output_srt_path) {
        let timeline = SubtitleTimeline::from_srt(&content);
        let validation = timeline.validate();
        if !validation.is_valid {
            log::warn!("[SRTExport] Merged SRT validation issues: {:?}, file: {:?}", validation, output_srt_path);
        }
        log::info!("[SRTExport] Merged SRT: {} cues, validation: {}", timeline.cue_count(), if validation.is_valid { "OK" } else { "ISSUES" });
    }

    Ok(())
}
