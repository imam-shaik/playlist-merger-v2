//! Health check command for playlist files.
//!
//! ## Health Status Audit Matrix
//!
//! | Status | Created Today? | Delivered Today? | Visible Today? | Creation Location |
//! |--------|---------------|-----------------|---------------|------------------|
//! | `healthy` | ✅ YES | ✅ YES | ✅ YES | `check_file_corruption_quick:1310` |
//! | `unreadable` | ✅ YES | ✅ YES | ✅ YES | `check_file_corruption_quick:1301,1321` |
//! | `corrupted` | ✅ YES | ✅ YES | ✅ YES | `check_file_corruption_quick:1304` |
//! | `healthy_with_warnings` | ✅ YES | ✅ YES | ✅ YES | `stage2_upgrade_health` |
//! | `seekability_issue` | ✅ YES | ✅ YES | ✅ YES | `stage2_upgrade_health` |
//! | `minor_metadata_issue` | ✅ YES | ✅ YES | ✅ YES | `stage2_upgrade_health` |
//!
//! ### Phase 2 (Stage 2) Integration
//!
//! After Stage 1 (`check_batch_corruption_parallel`) marks a file as Healthy,
//! Stage 2 runs additional checks:
//! - Metadata sanitization via `MediaProfile::from_media_info` dimension/fps filtering
//! - Quick seek test to detect MPEG-TS seeking artifacts
//!
//! Files that pass Stage 1 but fail Stage 2 are upgraded to one of the
//! intermediate statuses (`healthy_with_warnings`, `seekability_issue`,
//! `minor_metadata_issue`).

use serde::Serialize;
use std::path::Path;
use crate::ffmpeg::normalization::{check_batch_corruption_parallel, FileHealth, FileHealthStatus};
use crate::ffmpeg::probe::probe_file;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHealthForTransport {
    pub status: String,
    pub path: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub technical_details: Option<String>,
    pub confidence: u8,
    pub can_merge_lossless: bool,
    pub can_merge_custom: bool,
    pub auto_repair: bool,
}

impl FileHealthForTransport {
    fn from_health(health: FileHealth) -> Self {
        let status_str = match &health.status {
            FileHealthStatus::Healthy { .. } => "healthy",
            FileHealthStatus::HealthyWithWarnings { .. } => "healthy_with_warnings",
            FileHealthStatus::SeekabilityIssue { .. } => "seekability_issue",
            FileHealthStatus::MinorMetadataIssue { .. } => "minor_metadata_issue",
            FileHealthStatus::Unreadable { .. } => "unreadable",
            FileHealthStatus::Corrupted { .. } => "corrupted",
        };

        let (message, technical_details) = match &health.status {
            FileHealthStatus::Healthy { .. } => {
                ("Container structure verified".to_string(), None)
            }
            FileHealthStatus::HealthyWithWarnings { errors, fail_count, total_count, .. } => {
                let msg = format!("MPEG-TS seeking artifacts ({}/{} seek points failed — minor)", fail_count, total_count);
                let details = if errors.is_empty() { None } else { Some(errors.join("; ")) };
                (msg, details)
            }
            FileHealthStatus::SeekabilityIssue { errors, fail_count, total_count, .. } => {
                let msg = format!("Multiple seeking failures ({}/{} seek points failed)", fail_count, total_count);
                let details = if errors.is_empty() { None } else { Some(errors.join("; ")) };
                (msg, details)
            }
            FileHealthStatus::MinorMetadataIssue { field, raw_value, fallback, .. } => {
                let msg = format!("Invalid {} metadata ({}), using fallback: {}", field, raw_value, fallback);
                (msg, Some(format!("raw: {}, fallback: {}", raw_value, fallback)))
            }
            FileHealthStatus::Unreadable { reason, .. } => {
                (format!("Cannot read file: {}", reason), Some(reason.clone()))
            }
            FileHealthStatus::Corrupted { reason, first_error, .. } => {
                (format!("Structural damage: {}", reason), Some(first_error.clone()))
            }
        };

        let confidence = match &health.status {
            FileHealthStatus::Healthy { confidence } => *confidence,
            FileHealthStatus::HealthyWithWarnings { confidence, .. } => *confidence,
            FileHealthStatus::SeekabilityIssue { confidence, .. } => *confidence,
            FileHealthStatus::MinorMetadataIssue { confidence, .. } => *confidence,
            FileHealthStatus::Unreadable { confidence, .. } => *confidence,
            FileHealthStatus::Corrupted { confidence, .. } => *confidence,
        };

        FileHealthForTransport {
            status: status_str.to_string(),
            path: health.path,
            message,
            technical_details,
            confidence,
            can_merge_lossless: health.status.can_merge_lossless(),
            can_merge_custom: health.status.can_merge_custom(),
            auto_repair: health.status.auto_repair_applies(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistHealthReportForTransport {
    pub total_files: usize,
    pub healthy_count: usize,
    pub healthy_with_warnings_count: usize,
    pub seekability_issue_count: usize,
    pub metadata_issue_count: usize,
    pub corrupted_count: usize,
    pub unreadable_count: usize,
    pub per_file: Vec<FileHealthForTransport>,
}

impl PlaylistHealthReportForTransport {
    fn from_results(results: Vec<FileHealth>) -> Self {
        let mut healthy_count = 0;
        let mut healthy_with_warnings_count = 0;
        let mut seekability_issue_count = 0;
        let mut metadata_issue_count = 0;
        let mut corrupted_count = 0;
        let mut unreadable_count = 0;

        for health in &results {
            match &health.status {
                FileHealthStatus::Healthy { .. } => healthy_count += 1,
                FileHealthStatus::HealthyWithWarnings { .. } => healthy_with_warnings_count += 1,
                FileHealthStatus::SeekabilityIssue { .. } => seekability_issue_count += 1,
                FileHealthStatus::MinorMetadataIssue { .. } => metadata_issue_count += 1,
                FileHealthStatus::Corrupted { .. } => corrupted_count += 1,
                FileHealthStatus::Unreadable { .. } => unreadable_count += 1,
            }
        }

        let total_files = results.len();
        let per_file: Vec<FileHealthForTransport> = results.into_iter()
            .map(FileHealthForTransport::from_health)
            .collect();

        PlaylistHealthReportForTransport {
            total_files,
            healthy_count,
            healthy_with_warnings_count,
            seekability_issue_count,
            metadata_issue_count,
            corrupted_count,
            unreadable_count,
            per_file,
        }
    }
}

/// Stage 2 upgrade: run additional checks on a file that passed Stage 1.
///
/// After `check_file_corruption_quick` marks a file as Healthy, this function
/// runs deeper analysis:
/// 1. Metadata sanitization — checks for 0x0 resolution, invalid FPS
/// 2. Quick seek test — detects MPEG-TS seeking artifacts
///
/// Returns a potentially upgraded FileHealth if issues are found,
/// or the original health if Stage 2 passes.
fn stage2_upgrade_health(
    ffprobe_path: &Path,
    ffmpeg_path: &Path,
    health: &FileHealth,
) -> FileHealth {
    let index = health.index;
    let path = health.path.clone();

    // ── 1. Metadata sanitization ───────────────────────────────────────
    // Run ffprobe with JSON output to get structured metadata, then check
    // for 0x0 resolution or invalid FPS using the same logic as
    // MediaProfile::from_media_info sanitization.
    if let Ok(media_info) = probe_file(ffprobe_path, Path::new(&path)) {
        if let Some(video) = media_info.video_streams.first() {
            // Check for 0x0 dimensions
            let has_zero_dim = match (video.width, video.height) {
                (Some(w), Some(h)) => w == 0 || h == 0,
                _ => false,
            };

            // Check for invalid FPS (< 1 or > 240, which is unrealistic)
            let has_invalid_fps = match video.fps {
                Some(fps) => !(1.0..=240.0).contains(&fps),
                None => false,
            };

            if has_zero_dim {
                return FileHealth::minor_metadata(
                    index, path,
                    "resolution".to_string(),
                    format!("{}x{}", video.width.unwrap_or(0), video.height.unwrap_or(0)),
                    "1920x1080".to_string(),
                );
            }

            if has_invalid_fps {
                return FileHealth::minor_metadata(
                    index, path,
                    "fps".to_string(),
                    format!("{}", video.fps.unwrap_or(0.0)),
                    if media_info.duration > 0.0 {
                        // Estimate FPS from duration and frame count if possible
                        format!("{:.3}", media_info.duration / 100.0)
                    } else {
                        "30".to_string()
                    },
                );
            }
        }

        // ── 2. Quick seek test ─────────────────────────────────────────
        // Run ffmpeg seek tests at 3 points (10%, 50%, 90%) to detect
        // MPEG-TS seeking artifacts. This is the same approach used in
        // the merge flow's seekability audit.
        //
        // Skip for very short files (<3s) because seeking to fractional
        // timestamps on sub-second files can produce false positive errors
        // from ffmpeg's decoder (e.g. "non-existing PPS referenced").
        if media_info.duration >= 3.0 {
            let test_points = [0.10, 0.50, 0.90];
            let mut seek_errors: Vec<String> = Vec::new();
            let mut fail_count = 0usize;

            for pct in &test_points {
                let seek_sec = media_info.duration * pct;
                #[cfg(windows)]
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                let mut cmd = std::process::Command::new(ffmpeg_path);
                #[cfg(windows)]
                cmd.creation_flags(CREATE_NO_WINDOW);
                let result = cmd
                    .args([
                        "-v", "error",
                        "-ss", &seek_sec.to_string(),
                        "-i", &path,
                        "-an",
                        "-frames:v", "1",
                        "-f", "null",
                        "-",
                    ])
                    .output();

                match result {
                    Ok(out) if !out.status.success() => {
                        let err_msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
                        seek_errors.push(err_msg);
                        fail_count += 1;
                    }
                    Ok(_) => { /* seek succeeded */ }
                    Err(e) => {
                        seek_errors.push(format!("ffmpeg execution failed: {}", e));
                        fail_count += 1;
                    }
                }
            }

            if fail_count > 0 {
                let total = test_points.len();
                let fail_pct = (fail_count as f64 / total as f64) * 100.0;

                if fail_pct > 50.0 {
                    return FileHealth::seekability_issue(
                        index, path,
                        seek_errors, fail_count, total,
                    );
                } else {
                    return FileHealth::healthy_with_warnings(
                        index, path,
                        seek_errors, fail_count, total,
                    );
                }
            }
        }
    }

    // All Stage 2 checks passed — return original healthy status
    FileHealth::healthy(index, path)
}

#[tauri::command]
pub async fn check_file_health(
    file_paths: Vec<String>,
) -> Result<PlaylistHealthReportForTransport, String> {
    log::info!("[FileHealth] check_file_health called for {} files", file_paths.len());

    if file_paths.is_empty() {
        return Ok(PlaylistHealthReportForTransport {
            total_files: 0,
            healthy_count: 0,
            healthy_with_warnings_count: 0,
            seekability_issue_count: 0,
            metadata_issue_count: 0,
            corrupted_count: 0,
            unreadable_count: 0,
            per_file: vec![],
        });
    }

    let settings = crate::services::settings::load_settings_internal();
    let ffprobe_path = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref())
        .map_err(|e| format!("Failed to find ffprobe: {}", e))?;
    let ffmpeg_path = crate::ffmpeg::find_ffmpeg(settings.ffmpeg_path.as_deref())
        .map_err(|e| format!("Failed to find ffmpeg: {}", e))?;

    let files: Vec<(usize, String)> = file_paths.into_iter().enumerate().collect();

    let (stage1_results, _, _) = check_batch_corruption_parallel(
        &ffprobe_path,
        &files,
        Some(|_completed: usize, _total: usize| {}),
    ).await;

    // ── Stage 2: Upgrade healthy files with deeper checks ─────────────
    let mut stage2_handles = Vec::with_capacity(stage1_results.len());
    let ffprobe = ffprobe_path.clone();
    let ffmpeg = ffmpeg_path.clone();

    for health in stage1_results {
        let ffprobe_inner = ffprobe.clone();
        let ffmpeg_inner = ffmpeg.clone();
        
        stage2_handles.push(tokio::task::spawn_blocking(move || {
            match &health.status {
                FileHealthStatus::Healthy { .. } => {
                    // Run Stage 2 upgrade on healthy files
                    stage2_upgrade_health(&ffprobe_inner, &ffmpeg_inner, &health)
                }
                _ => {
                    // Unreadable, Corrupted, or already a Stage 2 status — pass through
                    health
                }
            }
        }));
    }

    let mut final_results = Vec::with_capacity(stage2_handles.len());
    for handle in stage2_handles {
        match handle.await {
            Ok(health) => final_results.push(health),
            Err(e) => log::error!("[FileHealth] Stage 2 task panicked: {}", e),
        }
    }

    let report = PlaylistHealthReportForTransport::from_results(final_results);

    log::info!(
        "[FileHealth] Report: {} healthy, {} healthy_with_warnings, {} seekability_issue, {} metadata_issue, {} corrupted, {} unreadable, {} total",
        report.healthy_count,
        report.healthy_with_warnings_count,
        report.seekability_issue_count,
        report.metadata_issue_count,
        report.corrupted_count,
        report.unreadable_count,
        report.total_files
    );

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffmpeg::normalization::{FileHealth, FileHealthStatus};
    use serde_json;

    fn create_healthy_health() -> FileHealth {
        FileHealth {
            index: 0,
            path: "/test/video.mp4".to_string(),
            status: FileHealthStatus::Healthy { confidence: 100 },
        }
    }

    fn create_unreadable_health() -> FileHealth {
        FileHealth {
            index: 1,
            path: "/test/missing.mp4".to_string(),
            status: FileHealthStatus::Unreadable { reason: "No such file".to_string(), confidence: 100 },
        }
    }

    fn create_corrupted_health() -> FileHealth {
        FileHealth {
            index: 2,
            path: "/test/corrupt.mp4".to_string(),
            status: FileHealthStatus::Corrupted {
                reason: "Invalid data".to_string(),
                first_error: "moov atom not found".to_string(),
                confidence: 95,
            },
        }
    }

    fn create_healthy_with_warnings_health() -> FileHealth {
        FileHealth {
            index: 3,
            path: "/test/artifacts.ts".to_string(),
            status: FileHealthStatus::HealthyWithWarnings {
                errors: vec!["PPS error".to_string()],
                fail_count: 2,
                total_count: 10,
                confidence: 70,
            },
        }
    }

    fn create_seekability_issue_health() -> FileHealth {
        FileHealth {
            index: 4,
            path: "/test/bad_seek.ts".to_string(),
            status: FileHealthStatus::SeekabilityIssue {
                errors: vec!["non-existing PPS 0 referenced".to_string(), "decode_slice_header error".to_string()],
                fail_count: 6,
                total_count: 10,
                confidence: 65,
            },
        }
    }

    fn create_minor_metadata_health() -> FileHealth {
        FileHealth {
            index: 5,
            path: "/test/no_resolution.mp4".to_string(),
            status: FileHealthStatus::MinorMetadataIssue {
                field: "resolution".to_string(),
                raw_value: "0x0".to_string(),
                fallback: "1920x1080".to_string(),
                confidence: 95,
            },
        }
    }

    #[test]
    fn test_healthy_serialization() {
        let health = create_healthy_health();
        let transport = FileHealthForTransport::from_health(health.clone());

        let json = serde_json::to_string(&transport).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["status"], "healthy");
        assert_eq!(parsed["message"], "Container structure verified");
        assert!(parsed["technicalDetails"].is_null());
        assert_eq!(parsed["confidence"], 100);
        assert_eq!(parsed["canMergeLossless"], true);
        assert_eq!(parsed["canMergeCustom"], true);
        assert_eq!(parsed["autoRepair"], false);
    }

    #[test]
    fn test_unreadable_serialization() {
        let health = create_unreadable_health();
        let transport = FileHealthForTransport::from_health(health.clone());

        let json = serde_json::to_string(&transport).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["status"], "unreadable");
        assert_eq!(parsed["message"], "Cannot read file: No such file");
        assert_eq!(parsed["technicalDetails"], "No such file");
        assert_eq!(parsed["confidence"], 100);
        assert_eq!(parsed["canMergeLossless"], false);
        assert_eq!(parsed["canMergeCustom"], false);
        assert_eq!(parsed["autoRepair"], false);
    }

    #[test]
    fn test_corrupted_serialization() {
        let health = create_corrupted_health();
        let transport = FileHealthForTransport::from_health(health.clone());

        let json = serde_json::to_string(&transport).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["status"], "corrupted");
        assert_eq!(parsed["message"], "Structural damage: Invalid data");
        assert_eq!(parsed["technicalDetails"], "moov atom not found");
        assert_eq!(parsed["confidence"], 95);
        assert_eq!(parsed["canMergeLossless"], false);
        assert_eq!(parsed["canMergeCustom"], false);
        assert_eq!(parsed["autoRepair"], false);
    }

    #[test]
    fn test_healthy_with_warnings_serialization() {
        let health = create_healthy_with_warnings_health();
        let transport = FileHealthForTransport::from_health(health);

        let json = serde_json::to_string(&transport).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["status"], "healthy_with_warnings");
        assert!(parsed["message"].as_str().unwrap().contains("MPEG-TS seeking artifacts"));
        assert!(parsed["technicalDetails"].as_str().unwrap().contains("PPS error"));
        assert_eq!(parsed["confidence"], 70);
        assert_eq!(parsed["canMergeLossless"], false);
        assert_eq!(parsed["canMergeCustom"], true);
        assert_eq!(parsed["autoRepair"], true);
    }

    #[test]
    fn test_seekability_issue_serialization() {
        let health = create_seekability_issue_health();
        let transport = FileHealthForTransport::from_health(health);

        let json = serde_json::to_string(&transport).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["status"], "seekability_issue");
        assert!(parsed["message"].as_str().unwrap().contains("Multiple seeking failures"));
        assert!(parsed["technicalDetails"].as_str().unwrap().contains("PPS"));
        assert_eq!(parsed["confidence"], 65);
        assert_eq!(parsed["canMergeLossless"], false);
        assert_eq!(parsed["canMergeCustom"], true);
        assert_eq!(parsed["autoRepair"], true);
    }

    #[test]
    fn test_minor_metadata_issue_serialization() {
        let health = create_minor_metadata_health();
        let transport = FileHealthForTransport::from_health(health);

        let json = serde_json::to_string(&transport).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["status"], "minor_metadata_issue");
        assert!(parsed["message"].as_str().unwrap().contains("Invalid resolution metadata"));
        assert!(parsed["technicalDetails"].as_str().unwrap().contains("0x0"));
        assert_eq!(parsed["confidence"], 95);
        assert_eq!(parsed["canMergeLossless"], false);
        assert_eq!(parsed["canMergeCustom"], true);
        assert_eq!(parsed["autoRepair"], true);
    }

    #[test]
    fn test_playlist_report_serialization_all_statuses() {
        let results = vec![
            create_healthy_health(),
            create_unreadable_health(),
            create_corrupted_health(),
            create_healthy_with_warnings_health(),
            create_seekability_issue_health(),
            create_minor_metadata_health(),
        ];
        let report = PlaylistHealthReportForTransport::from_results(results);

        let json = serde_json::to_string(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["totalFiles"], 6);
        assert_eq!(parsed["healthyCount"], 1);
        assert_eq!(parsed["unreadableCount"], 1);
        assert_eq!(parsed["corruptedCount"], 1);
        assert_eq!(parsed["healthyWithWarningsCount"], 1);
        assert_eq!(parsed["seekabilityIssueCount"], 1);
        assert_eq!(parsed["metadataIssueCount"], 1);
        assert_eq!(parsed["perFile"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn test_empty_report_serialization() {
        let report = PlaylistHealthReportForTransport::from_results(vec![]);

        let json = serde_json::to_string(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["totalFiles"], 0);
        assert_eq!(parsed["healthyCount"], 0);
        assert_eq!(parsed["perFile"].as_array().unwrap().len(), 0);
    }
}
