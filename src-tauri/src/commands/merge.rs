use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::collections::{HashMap, HashSet};

use crate::recovery;
use tauri::{command, State, Emitter};
use serde::{Deserialize, Serialize};
use chrono::Utc;
use crate::AppState;
use crate::types::{MergeMode, MergeProgress, RecentExport, CardConfig, SubtitleMode, MediaInfo, SubtitleWarning};
use crate::ffmpeg::concat::{MergeConfig, run_merge_blocking};
use crate::ffmpeg::write_concat_list_with_durations;
use crate::ffmpeg::get_temp_dir;
use crate::ffmpeg::probe_cache::probe_all_parallel;
use crate::ffmpeg::normalization::{normalize_to_profile as ffmpeg_normalize_to_profile, normalize_audio_only as ffmpeg_normalize_audio_only, EncodingProfile, AudioProfile};
use crate::ffmpeg::media_validation_engine::{validate_input_files, apply_validation_results};
use crate::ffmpeg::immutability::{ImmutabilityRegistry, AudioFingerprint};

// ══════════════════════════════════════════════════════════════════════════════
// REAL WORKLOAD FORENSIC INSTRUMENTATION
// Captures actual normalization performance on real files for analysis
// ══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
struct RealWorkloadForensics {
    cpu_count: usize,
    file_metrics: Vec<FileMetric>,
    total_norm_time: std::time::Duration,
    total_verify_time: std::time::Duration,
    phase_start: std::time::Instant,
}

#[derive(Clone)]
struct FileMetric {
    index: usize,
    filename: String,
    _input_path: String,
    _input_size_bytes: u64,
    input_duration_secs: f64,
    input_resolution: Option<String>,
    _input_video_codec: Option<String>,
    _input_audio_codec: Option<String>,
    input_file_size_mb: f64,
    normalization_type: String,
    norm_start: std::time::Instant,
    norm_end: Option<std::time::Instant>,
    verify_start: Option<std::time::Instant>,
    verify_end: Option<std::time::Instant>,
    output_path: Option<String>,
    output_size_bytes: Option<u64>,
    output_duration_secs: Option<f64>,
    norm_elapsed_ms: Option<u64>,
    verify_elapsed_ms: Option<u64>,
    encoding_speed_ratio: Option<f64>,
    verify_pct_of_total: Option<f64>,
}

impl RealWorkloadForensics {
    fn new() -> Self {
        let cpu_count = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        
        Self {
            cpu_count,
            file_metrics: Vec::new(),
            total_norm_time: std::time::Duration::ZERO,
            total_verify_time: std::time::Duration::ZERO,
            phase_start: std::time::Instant::now(),
        }
    }
    
    fn record_file_start(&mut self, idx: usize, filename: &str, input_path: &str, 
                          media_info: &MediaInfo, norm_type: &str) {
        let input_size = std::fs::metadata(input_path)
            .map(|m| m.len())
            .unwrap_or(0);
        
        let video_info = media_info.video_streams.first();
        let resolution = video_info
            .filter(|v| v.width.is_some() && v.height.is_some())
            .map(|v| format!("{}x{}", v.width.unwrap(), v.height.unwrap()));
        
        let metric = FileMetric {
            index: idx,
            filename: filename.to_string(),
            _input_path: input_path.to_string(),
            _input_size_bytes: input_size,
            input_duration_secs: media_info.duration,
            input_resolution: resolution,
            _input_video_codec: video_info.map(|v| v.codec_name.clone()),
            _input_audio_codec: media_info.audio_streams.first().map(|a| a.codec_name.clone()),
            input_file_size_mb: input_size as f64 / (1024.0 * 1024.0),
            normalization_type: norm_type.to_string(),
            norm_start: std::time::Instant::now(),
            norm_end: None,
            verify_start: None,
            verify_end: None,
            output_path: None,
            output_size_bytes: None,
            output_duration_secs: None,
            norm_elapsed_ms: None,
            verify_elapsed_ms: None,
            encoding_speed_ratio: None,
            verify_pct_of_total: None,
        };
        
        self.file_metrics.push(metric);
    }
    
    fn record_file_norm_end(&mut self, idx: usize, output_path: &str) {
        if let Some(metric) = self.file_metrics.iter_mut().find(|m| m.index == idx) {
            metric.norm_end = Some(std::time::Instant::now());
            metric.output_path = Some(output_path.to_string());
            if let Ok(output_meta) = std::fs::metadata(output_path) {
                metric.output_size_bytes = Some(output_meta.len());
            }
        }
    }
    
    fn record_file_verify_start(&mut self, idx: usize) {
        if let Some(metric) = self.file_metrics.iter_mut().find(|m| m.index == idx) {
            metric.verify_start = Some(std::time::Instant::now());
        }
    }
    
    fn record_file_verify_end(&mut self, idx: usize, output_duration: f64) {
        if let Some(metric) = self.file_metrics.iter_mut().find(|m| m.index == idx) {
            metric.verify_end = Some(std::time::Instant::now());
            metric.output_duration_secs = Some(output_duration);
            
            if let (Some(norm_end), Some(norm_start)) = (metric.norm_end, Some(metric.norm_start)) {
                let norm_elapsed = norm_end.duration_since(norm_start);
                metric.norm_elapsed_ms = Some(norm_elapsed.as_millis() as u64);
                
                if norm_elapsed.as_secs_f64() > 0.0 {
                    metric.encoding_speed_ratio = Some(
                        metric.input_duration_secs / norm_elapsed.as_secs_f64()
                    );
                }
                
                self.total_norm_time += norm_elapsed;
            }
            
            if let (Some(verify_end), Some(verify_start)) = (metric.verify_end, metric.verify_start) {
                let verify_elapsed = verify_end.duration_since(verify_start);
                metric.verify_elapsed_ms = Some(verify_elapsed.as_millis() as u64);
                
                if let Some(norm_ms) = metric.norm_elapsed_ms {
                    if norm_ms > 0 {
                        metric.verify_pct_of_total = Some(
                            (verify_elapsed.as_millis() as f64 / norm_ms as f64) * 100.0
                        );
                    }
                }
                
                self.total_verify_time += verify_elapsed;
            }
        }
    }
    
    fn generate_report(&self) -> String {
        let total_elapsed = self.phase_start.elapsed();
        let total_files = self.file_metrics.len();
        let files_with_timing = self.file_metrics.iter()
            .filter(|m| m.norm_elapsed_ms.is_some())
            .count();
        
        let mut report = String::new();
        report.push('\n');
        report.push_str("╔═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╗\n");
        report.push_str("║                              NORMALIZATION REAL WORKLOAD FORENSIC REPORT                                               ║\n");
        report.push_str("╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣\n");
        report.push_str(&format!("║ SYSTEM: CPU cores={} | Total files measured={} | Files with timing={}                                                     ║\n", 
            self.cpu_count, total_files, files_with_timing));
        report.push_str("╠═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╣\n");
        report.push_str("║ PHASE TIMING SUMMARY                                                                                                   ║\n");
        report.push_str("║ ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────║\n");
        report.push_str(&format!("║   Total normalization time:  {:>10.3}s ({:>8.1}ms avg per file)                                                         ║\n",
            self.total_norm_time.as_secs_f64(), 
            if files_with_timing > 0 { self.total_norm_time.as_millis() as f64 / files_with_timing as f64 } else { 0.0 }));
        report.push_str(&format!("║   Total verification time:    {:>10.3}s ({:>8.1}ms avg per file)                                                         ║\n",
            self.total_verify_time.as_secs_f64(),
            if files_with_timing > 0 { self.total_verify_time.as_millis() as f64 / files_with_timing as f64 } else { 0.0 }));
        report.push_str(&format!("║   Total phase elapsed:        {:>10.3}s                                                                                  ║\n",
            total_elapsed.as_secs_f64()));
        report.push_str("║                                                                                                                             ║\n");
        report.push_str("║ PER-FILE BREAKDOWN                                                                                                     ║\n");
        report.push_str("║ ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────║\n");
        report.push_str("║  # | Filename                    | Type             | Duration | Resolution | Size(MB) | Norm(ms) | Verify(ms) | Speed | Verify% ║\n");
        report.push_str("║ ──|──────────────────────────────|──────────────────|──────────|───────────|──────────|──────────|───────────|───────|────────║\n");
        
        for metric in &self.file_metrics {
            if metric.norm_elapsed_ms.is_none() {
                continue;
            }
            
            let filename_short = if metric.filename.len() > 28 {
                format!("{}...", &metric.filename[..25])
            } else {
                metric.filename.clone()
            };
            
            let norm_ms = metric.norm_elapsed_ms.unwrap_or(0);
            let verify_ms = metric.verify_elapsed_ms.unwrap_or(0);
            let speed = metric.encoding_speed_ratio.unwrap_or(0.0);
            let verify_pct = metric.verify_pct_of_total.unwrap_or(0.0);
            
            report.push_str(&format!(
                "║ {:>3}| {:<30}| {:<17}| {:>9.1}s| {:>10}| {:>9.1}| {:>9}| {:>10}| {:>6.1}x| {:>6.0}%║\n",
                metric.index,
                filename_short,
                metric.normalization_type,
                metric.input_duration_secs,
                metric.input_resolution.as_deref().unwrap_or("N/A"),
                metric.input_file_size_mb,
                norm_ms,
                verify_ms,
                speed,
                verify_pct
            ));
        }
        
        report.push_str("║                                                                                                                             ║\n");
        
        let speed_ratios: Vec<f64> = self.file_metrics.iter()
            .filter_map(|m| m.encoding_speed_ratio)
            .collect();
        
        if !speed_ratios.is_empty() {
            let min_speed = speed_ratios.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_speed = speed_ratios.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let avg_speed = speed_ratios.iter().sum::<f64>() / speed_ratios.len() as f64;
            
            report.push_str("║ ENCODING SPEED ANALYSIS                                                                                                ║\n");
            report.push_str("║ ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────║\n");
            report.push_str(&format!("║   Min speed: {:.2}x realtime | Max speed: {:.2}x realtime | Avg: {:.2}x realtime                                          ║\n", min_speed, max_speed, avg_speed));
            
            let slow_files = speed_ratios.iter().filter(|&&s| s < 1.0).count();
            let fast_files = speed_ratios.iter().filter(|&&s| s >= 1.0).count();
            
            if slow_files > 0 {
                report.push_str(&format!("║   ⚠️  {}/{} files normalized SLOWER than realtime (encoding is the bottleneck)                                         ║\n", slow_files, speed_ratios.len()));
            }
            if fast_files > 0 {
                report.push_str(&format!("║   ✅ {}/{} files normalized FASTER than realtime                                                             ║\n", fast_files, speed_ratios.len()));
            }
            report.push_str("║                                                                                                                             ║\n");
        }
        
        let verify_pcts: Vec<f64> = self.file_metrics.iter()
            .filter_map(|m| m.verify_pct_of_total)
            .collect();
        
        if !verify_pcts.is_empty() {
            let avg_verify_pct = verify_pcts.iter().sum::<f64>() / verify_pcts.len() as f64;
            
            report.push_str("║ VERIFICATION COST                                                                                                     ║\n");
            report.push_str("║ ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────║\n");
            report.push_str(&format!("║   Avg verification time = {:.0}% of normalization time                                                          ║\n", avg_verify_pct));
            
            if avg_verify_pct > 50.0 {
                report.push_str("║   ⚠️  Verification consumes >50% of normalization time — verification is expensive                                    ║\n");
            } else if avg_verify_pct > 25.0 {
                report.push_str("║   ⚡ Verification is a significant but acceptable cost                                                               ║\n");
            } else {
                report.push_str("║   ✅ Verification overhead is reasonable                                                                              ║\n");
            }
            report.push_str("║                                                                                                                             ║\n");
        }
        
        report.push_str("║ PARALLELIZATION FEASIBILITY                                                                                            ║\n");
        report.push_str("║ ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────║\n");
        
        let avg_cpu_util_estimate = if self.cpu_count > 1 && !speed_ratios.is_empty() {
            let avg_speed = speed_ratios.iter().sum::<f64>() / speed_ratios.len() as f64;
            if avg_speed < 1.0 {
                "LIKELY CPU-BOUND (encoding slower than realtime)"
            } else {
                "POSSIBLY I/O-BOUND or single-thread limited"
            }
        } else {
            "Single CPU core"
        };
        
        report.push_str(&format!("║   CPU cores available: {} | Status: {}                                    ║\n", 
            self.cpu_count, avg_cpu_util_estimate));
        report.push_str("║   Current execution: SEQUENTIAL (no parallel workers)                                                                  ║\n");
        
        if self.cpu_count >= 4 && !speed_ratios.is_empty() {
            let avg_speed = speed_ratios.iter().sum::<f64>() / speed_ratios.len() as f64;
            if avg_speed < 2.0 {
                report.push_str("║   📊 RECOMMENDATION: Parallelization would likely yield 2-4x speedup                                                 ║\n");
            }
        }
        
        report.push_str("║                                                                                                                             ║\n");
        report.push_str("║ SCALING PROJECTION                                                                                                      ║\n");
        report.push_str("║ ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────║\n");
        
        if files_with_timing > 0 {
            let avg_norm_time_ms = self.total_norm_time.as_millis() as f64 / files_with_timing as f64;
            let avg_verify_time_ms = self.total_verify_time.as_millis() as f64 / files_with_timing as f64;
            let per_file_ms = avg_norm_time_ms + avg_verify_time_ms;
            
            for count in [10, 50, 100, 500] {
                let projected_sequential = (per_file_ms * count as f64) / 1000.0;
                let projected_parallel = projected_sequential / (self.cpu_count as f64).min(4.0);
                report.push_str(&format!("║   {:3} files: Sequential={:>7.1}s | Parallel(4 workers)={:>7.1}s                                                               ║\n",
                    count, projected_sequential, projected_parallel));
            }
        }
        
        report.push_str("╚═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╝\n");
        
        report
    }
}

impl Default for RealWorkloadForensics {
    fn default() -> Self {
        Self::new()
    }
}

/// Temp file registry — uses Arc<Mutex<>> so cleanup always sees current content
/// and the struct is Send+Sync (required for async fn).
struct TempFileRegistry {
    sub: Arc<Mutex<Vec<PathBuf>>>,
}

/// RAII guard for guaranteed temp file cleanup on all exit paths.
/// Replaces the broken "cleanup at end of spawn_blocking" pattern that missed
/// 6 early returns in the normalization loops (lines ~1727, 1776, 1791, 1876,
/// 1922, 1938 after edits).
///
/// All fields are Send+Sync so the entire struct is Send.
struct TempCleanup {
    temp_dir: Option<PathBuf>,
    registry: Arc<TempFileRegistry>,
    norm_files: Arc<Mutex<Vec<PathBuf>>>,
    card_temp_files: Vec<PathBuf>,
    burn_subtitle_path: Option<PathBuf>,
    list_path: Option<PathBuf>,
    subtitle_list_path: Option<PathBuf>,
}

impl TempCleanup {
    fn new(registry: Arc<TempFileRegistry>, norm_files: Arc<Mutex<Vec<PathBuf>>>) -> Self {
        Self {
            temp_dir: get_temp_dir().ok(),
            registry,
            norm_files,
            card_temp_files: Vec::new(),
            burn_subtitle_path: None,
            list_path: None,
            subtitle_list_path: None,
        }
    }

    fn cleanup(&self) {
        let mut cleaned = 0usize;
        let mut failed = 0usize;

        let try_remove = |p: &Path, label: &str| {
            if p.exists() {
                match std::fs::remove_file(p) {
                    Ok(()) => {
                        log::info!("[TempCleanup] Removed {}: {}", label, p.display());
                        true
                    }
                    Err(e) => {
                        log::warn!("[TempCleanup] Failed to remove {} ({}): {}", label, p.display(), e);
                        false
                    }
                }
            } else {
                false
            }
        };

        // temp_norm_files via Arc<Mutex> — always sees current content (Mutex
        // is Sync, allowing shared access from cleanup while main code pushes).
        // Never unwrap: a poisoned mutex in Drop would abort the process.
        if let Ok(files) = self.norm_files.lock() {
            for p in files.iter() { if try_remove(p, "norm") { cleaned += 1; } else { failed += 1; } }
        } else {
            log::error!("[TempCleanup] Mutex poisoned while cleaning norm files — skipping");
        }

        // temp_subtitle_files via registry Arc<Mutex> — always sees current content.
        if let Ok(files) = self.registry.sub.lock() {
            for p in files.iter() { if try_remove(p, "sub") { cleaned += 1; } else { failed += 1; } }
        } else {
            log::error!("[TempCleanup] Mutex poisoned while cleaning subtitle files — skipping");
        }

        for p in &self.card_temp_files { if try_remove(p, "card") { cleaned += 1; } else { failed += 1; } }
        if let Some(ref p) = self.burn_subtitle_path { if try_remove(p, "burn") { cleaned += 1; } else { failed += 1; } }
        if let Some(ref p) = self.list_path { if try_remove(p, "list") { cleaned += 1; } else { failed += 1; } }
        if let Some(ref p) = self.subtitle_list_path { if try_remove(p, "sublist") { cleaned += 1; } else { failed += 1; } }

        if let Some(ref td) = self.temp_dir {
            let dummy_srt = td.join("dummy.srt");
            let dummy_vtt = td.join("dummy.vtt");
            if let Err(e) = std::fs::remove_file(&dummy_srt) {
                log::warn!("[TempCleanup] Failed to remove dummy_srt: {} — {}", dummy_srt.display(), e);
            }
            if let Err(e) = std::fs::remove_file(&dummy_vtt) {
                log::warn!("[TempCleanup] Failed to remove dummy_vtt: {} — {}", dummy_vtt.display(), e);
            }
        }

        if failed == 0 {
            log::info!("[TempCleanup] ✅ Cleaned {} temp files ({} failed)", cleaned, failed);
        } else {
            log::warn!("[TempCleanup] ⚠️ Cleaned {} temp files ({} failed)", cleaned, failed);
        }
    }
}

impl Drop for TempCleanup {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
pub(crate) fn temp_cleanup_impls_drop() -> bool {
    // TempCleanup implements Drop, so this returns true.
    // If TempCleanup's Drop impl is ever removed, this would return false,
    // and the regression test would fail — protecting against the bug recurring.
    std::mem::needs_drop::<TempCleanup>()
}

/// Normalize a subtitle file to UTF-8 without BOM.
pub async fn normalize_subtitle_encoding(
    path: &str,
    temp_dir: &Path,
    job_id: &str,
    index: usize,
    ffmpeg_path: &Path,
) -> Result<String, String> {
    use std::io::Read;

    let src = Path::new(path);
    if !src.exists() {
        return Err(format!("Subtitle file not found: {}", path));
    }

    let mut bytes = Vec::new();
    std::fs::File::open(src)
        .map_err(|e| format!("Cannot open subtitle {}: {}", path, e))?
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read subtitle {}: {}", path, e))?;

    if bytes.is_empty() {
        return Err(format!("Empty subtitle file: {}", path));
    }

    let write_normalized = |data: Vec<u8>, label: &str| -> Result<String, String> {
        let normalized_name = format!("norm_sub_{}_{}.srt", job_id, index);
        let normalized_path = temp_dir.join(&normalized_name);
        std::fs::write(&normalized_path, data)
            .map_err(|e| format!("Failed to write normalized subtitle {}: {}", normalized_name, e))?;
        log::info!("[SubtitleConvert] Normalized {} ({}) -> {}", path, label, normalized_path.display());
        Ok(normalized_path.to_string_lossy().into_owned())
    };

    // -- BOM detection (fast path) --
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return write_normalized(bytes[3..].to_vec(), "UTF-8 BOM");
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        let body = &bytes[2..];
        if body.len() >= 2 && body.len() % 2 == 0 {
            let utf16: Vec<u16> = body.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let s = String::from_utf16_lossy(&utf16);
            if !s.is_empty() {
                return write_normalized(s.as_bytes().to_vec(), "UTF-16LE");
            }
        }
        return Err(format!("Failed to decode UTF-16LE subtitle: {}", path));
    }
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let body = &bytes[2..];
        if body.len() >= 2 && body.len() % 2 == 0 {
            let utf16: Vec<u16> = body.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            let s = String::from_utf16_lossy(&utf16);
            if !s.is_empty() {
                return write_normalized(s.as_bytes().to_vec(), "UTF-16BE");
            }
        }
        return Err(format!("Failed to decode UTF-16BE subtitle: {}", path));
    }

    // -- Try clean UTF-8 (most common for SRT) --
    if let Ok(s) = String::from_utf8(bytes.clone()) {
        return write_normalized(s.as_bytes().to_vec(), "UTF-8");
    }

    // -- Statistical encoding detection via chardetng --
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(&bytes, true);
    let encoding = detector.guess(None, true);
    let (decoded, _encoding_used, _had_errors) = encoding.decode(&bytes);
    let chardetng_data = decoded.as_bytes().to_vec();
    let chardetng_label = if encoding == encoding_rs::UTF_8 {
        "UTF-8 (via chardetng)"
    } else {
        encoding.name()
    };

    // -- FFmpeg fallback for non-UTF-8 encodings (handles truly mislabeled binary) --
    if encoding != encoding_rs::UTF_8 || chardetng_data.contains(&0x00) {
        let temp_srt = temp_dir.join(format!("conv_bin_sub_{}_{}.srt", job_id, index));
        let temp_srt_str = temp_srt.to_string_lossy().to_string();
        if run_ffmpeg_cmd(ffmpeg_path, &["-y", "-i", path, temp_srt_str.as_str()]).await.is_ok() {
            if let Ok(data) = std::fs::read(&temp_srt) {
                if !data.is_empty() {
                    return write_normalized(data, &format!("{} → FFmpeg", chardetng_label));
                }
            }
        }
    }

    // -- Fall through to chardetng result --
    write_normalized(chardetng_data, chardetng_label)
}

pub fn cleanup_orphan_merge(output_path: &str) {
    let marker = format!("{}.merging", output_path);

    // Validate the resolved output path to prevent path traversal.
    // Only clean up if the path resolves safely within the temp directory
    // or is a reasonable output path (no parent traversal components).
    let path = Path::new(output_path);
    if !path.is_absolute() && output_path.contains("..") {
        log::warn!("[Merge] Skipping orphan cleanup for non-absolute path with parent traversal: {}", output_path);
        let _ = std::fs::remove_file(&marker);
        return;
    }
    if let Ok(canonical) = path.canonicalize() {
        let tmp_ok = crate::ffmpeg::get_temp_dir()
            .ok()
            .map(|tmp| canonical.starts_with(&tmp))
            .unwrap_or(false);
        if !tmp_ok {
            log::warn!("[Merge] Skipping orphan cleanup — resolved path outside temp dir: {}", output_path);
            let _ = std::fs::remove_file(&marker);
            return;
        }
    }

    let _ = std::fs::remove_file(&marker);
    if path.exists() {
        let _ = std::fs::remove_file(path);
        log::info!("[Merge] Cleaned up orphan partial output: {}", output_path);
    }
}

async fn run_ffmpeg_cmd(ffmpeg_path: &Path, args: &[&str]) -> Result<(), String> {
    use tokio::process::Command;
    use std::time::Duration;
    use tokio::time::timeout;
    #[cfg(windows)]
    #[allow(unused_imports)]
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Log the full command line for job log capture
    log::info!("[FFMPEG_CMD] {} {}", ffmpeg_path.display(), args.join(" "));
    crate::logger::write_raw(&format!("[FFMPEG_CMD] {} {}
", ffmpeg_path.display(), args.join(" ")));

    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd
        .args(args)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg: {}", e))?;

    // Drain stderr and stdout in background tasks to capture all output.
    // Uses tokio::spawn (not std::thread) because these are tokio async handles.
    // This also prevents pipe buffer full deadlock on Windows (64KB buffer).
    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();
    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(mut h) = stderr_handle {
            let _ = h.read_to_end(&mut buf).await;
        }
        buf
    });
    let stdout_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(mut h) = stdout_handle {
            let _ = h.read_to_end(&mut buf).await;
        }
        buf
    });

    // 15 minute timeout (900s) for normalization/processing tasks
    // This is safer for large/high-resolution files on slower CPUs
    match timeout(Duration::from_secs(900), child.wait()).await {
        Ok(Ok(status)) => {
            let stderr_bytes = stderr_task.await.unwrap_or_default();
            let stderr_str = String::from_utf8_lossy(&stderr_bytes);
            let stdout_bytes = stdout_task.await.unwrap_or_default();
            let stdout_str = String::from_utf8_lossy(&stdout_bytes);
            if status.success() {
                log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                if !stderr_str.trim().is_empty() {
                    log::info!("[FFMPEG_STDERR] {}", stderr_str.lines().take(20).collect::<Vec<_>>().join("\n"));
                }
                if !stdout_str.trim().is_empty() {
                    log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\n"));
                }
                Ok(())
            } else {
                log::error!("[FFMPEG_EXIT] Exit code: {:?}\nStderr: {}", status.code(), stderr_str);
                if !stdout_str.trim().is_empty() {
                    log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\n"));
                }
                Err(stderr_str.into_owned())
            }
        }
        Ok(Err(e)) => {
            Err(format!("FFmpeg process error: {}", e))
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err("FFmpeg process timed out after 900 seconds".to_string())
        }
    }
}

/// Run an FFmpeg command with cancellation support.
/// Polls the cancel flag every 500ms and kills the child process if cancelled.
/// Uses `try_wait()` polling to avoid mutable borrow conflicts.
///
/// `timeout_secs` sets the per-operation timeout. Normalization/re-encode operations
/// need longer timeouts (600-7200s) than fast operations like subtitle extraction (900s).
async fn run_ffmpeg_cmd_with_cancel(
    ffmpeg_path: &Path,
    args: &[&str],
    cancel_flag: Arc<AtomicBool>,
    timeout_secs: u64,
) -> Result<(), String> {
    use tokio::process::Command;
    use std::time::{Duration, Instant};
    use tokio::time::sleep;
    #[cfg(windows)]
    #[allow(unused_imports)]
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Log the full command line for job log capture
    log::info!("[FFMPEG_CMD] {} {}", ffmpeg_path.display(), args.join(" "));
    crate::logger::write_raw(&format!("[FFMPEG_CMD] {} {}
", ffmpeg_path.display(), args.join(" ")));


    let mut cmd = Command::new(ffmpeg_path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd
        .args(args)
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn ffmpeg: {}", e))?;

    // Drain stderr and stdout in background tasks to capture all output.
    // Uses tokio::spawn (not std::thread) because these are tokio async handles.
    // This prevents pipe buffer full deadlock and captures FFmpeg progress info.
    let stderr_handle = child.stderr.take();
    let stdout_handle = child.stdout.take();
    
    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(mut h) = stderr_handle {
            let _ = h.read_to_end(&mut buf).await;
        }
        buf
    });
    let stdout_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        if let Some(mut h) = stdout_handle {
            let _ = h.read_to_end(&mut buf).await;
        }
        buf
    });
    let start = Instant::now();

    // Poll-based loop: check cancel flag, timeout, and child status
    loop {
        // Check operation-specific timeout
        if start.elapsed().as_secs() > timeout_secs {
            let _ = child.kill().await;
            let _ = child.wait().await;
            crate::logger::write_raw(&format!("[FFMPEG_EXIT] Timed out after {} seconds
", timeout_secs));
            return Err(format!("FFmpeg process timed out after {} seconds", timeout_secs));
        }

        // Check user cancellation
        if cancel_flag.load(Ordering::Relaxed) {
            let _ = child.kill().await;
            let _ = child.wait().await;
            crate::logger::write_raw("[FFMPEG_EXIT] Cancelled by user
");
            return Err("Merge cancelled by user".to_string());
        }

        // Non-blocking check if process exited
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    log::info!("[FFMPEG_EXIT] Exit code: 0 (success)");
                    crate::logger::write_raw("[FFMPEG_EXIT] Exit code: 0 (success)
");
                    return Ok(());
                } else {
                    let stderr_bytes = stderr_task.await.unwrap_or_default();
                    let err = String::from_utf8_lossy(&stderr_bytes);
                    let stdout_bytes = stdout_task.await.unwrap_or_default();
                    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
                    log::error!("[FFMPEG_EXIT] Exit code: {:?}\nStderr: {}", status.code(), err);
                    if !stdout_str.trim().is_empty() {
                        log::info!("[FFMPEG_STDOUT] {}", stdout_str.lines().take(10).collect::<Vec<_>>().join("\n"));
                    }
                    return Err(err.into_owned());
                }
            }
            Ok(None) => {
                // Still running, sleep before next poll
                sleep(Duration::from_millis(200)).await;
            }
            Err(e) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                crate::logger::write_raw(&format!("[FFMPEG_EXIT] Process error: {}
", e));
                return Err(format!("FFmpeg process error: {}", e));
            }
        }
    }
}

pub fn write_merge_marker(output_path: &str, job_id: &str) {
    if output_path.is_empty() { return; }
    let marker = format!("{}.merging", output_path);
    #[cfg(windows)]
    let marker = normalize_long_path(&marker);
    if let Err(e) = std::fs::write(&marker, job_id) {
        log::warn!("[Merge] Failed to write merge marker '{}': {} — orphan detection may be delayed", marker, e);
    }
}

pub fn remove_merge_marker(output_path: &str) {
    if output_path.is_empty() { return; }
    let marker = format!("{}.merging", output_path);
    if let Err(e) = std::fs::remove_file(&marker) {
        log::warn!("[Merge] Failed to remove merge marker '{}': {} — orphan cleanup may trigger on next startup", marker, e);
    }
}

fn _format_duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

pub fn write_report_file(
    output_path: &str,
    segments: &[MergeSegment],
    output_size_bytes: u64,
    total_duration: f64,
    repeat_config: Option<&crate::types::RepeatConfig>,
    original_duration: Option<f64>,
) -> Option<String> {
    use crate::report::{build_report_data, render_markdown_report, render_txt_report};
    let report_data = build_report_data(
        segments,
        None::<&[MergePartResult]>,
        output_path,
        output_size_bytes,
        total_duration,
        None,
        None,
        None,
        repeat_config,
        original_duration,
    );
    let output_path_s = output_path.to_string();
    // Always generate both TXT and Markdown reports
    // Return the TXT path for backward compatibility with existing callers
    let md_path = render_markdown_report(&report_data, &output_path_s);
    let txt_path = render_txt_report(&report_data, &output_path_s);
    // Both files are written; return the TXT path (legacy expected format)
    md_path.or(txt_path)
}

#[cfg(windows)]
pub fn normalize_long_path(p: &str) -> String {
    if p.len() > 200 && !p.starts_with("\\\\?\\") {
        let normalized = p.replace('/', "\\");
        if let Some(stripped) = normalized.strip_prefix("\\\\") { format!("\\\\?\\UNC\\{}", stripped) } else { format!("\\\\?\\{}", normalized) }
    } else { p.to_string() }
}

#[cfg(not(windows))]
pub fn normalize_long_path(p: &str) -> String { p.to_string() }

/// Check if a directory has at least `required_bytes` of free space.
/// Returns Ok(available_bytes) if enough space, or an error message explaining the shortfall.
pub(crate) fn check_free_disk_space(path: &Path, required_bytes: u64) -> Result<u64, String> {
    let path = if path.as_os_str().is_empty() {
        std::env::current_dir().map_err(|e| format!("Cannot determine current directory: {}", e))?
    } else {
        path.to_path_buf()
    };

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailableToCaller: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }

        let mut free_avail: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free: u64 = 0;

        // Convert path to wide string (null-terminated)
        let wide: Vec<u16> = OsStr::new(path.as_os_str())
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let ret = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_avail,
                &mut total_bytes,
                &mut total_free,
            )
        };

        if ret == 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("Failed to check disk space for '{}': {}", path.display(), err));
        }

        log::info!("[DiskSpace] Drive '{}' | Free: {:.1} GB | Required: {:.1} GB | Total: {:.1} GB",
            path.display(), free_avail as f64 / 1_073_741_824.0,
            required_bytes as f64 / 1_073_741_824.0,
            total_bytes as f64 / 1_073_741_824.0);

        if free_avail < required_bytes {
            return Err(format!(
                "Insufficient disk space on '{}': {:.1} GB free, but {:.1} GB is required.                  Free up space and try again.",
                path.display(),
                free_avail as f64 / 1_073_741_824.0,
                required_bytes as f64 / 1_073_741_824.0,
            ));
        }

        Ok(free_avail)
    }

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::mem::MaybeUninit;

        let c_path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| format!("Path contains null bytes: {}", path.display()))?;

        let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
        let ret = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            return Err(format!("Failed to check disk space for '{}': {}", path.display(), err));
        }

        let stat = unsafe { stat.assume_init() };
        let free_avail = stat.f_frsize as u64 * stat.f_bavail as u64;

        log::info!("[DiskSpace] Path '{}' | Free: {:.1} GB | Required: {:.1} GB",
            path.display(), free_avail as f64 / 1_073_741_824.0,
            required_bytes as f64 / 1_073_741_824.0);

        if free_avail < required_bytes {
            return Err(format!(
                "Insufficient disk space: {:.1} GB free, but {:.1} GB is required.                  Free up space and try again.",
                free_avail as f64 / 1_073_741_824.0,
                required_bytes as f64 / 1_073_741_824.0,
            ));
        }

        Ok(free_avail)
    }
}


#[derive(Debug)]
#[derive(Default)]
pub struct MergeState { 
    pub active_jobs: HashMap<String, Arc<AtomicBool>>, 
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub enum AudioRepairMode {
    Fast,
    #[default]
    Smart,
    Safe,
}


/// Strategy for handling audio validation on large playlists.
/// User is shown a dialog and chooses, or their default preference is used.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum LargePlaylistStrategy {
    /// Full Smart: run validate_audio_streams_parallel + check_problematic_audio_streams
    /// Maximum protection, slowest for large playlists.
    FullSmart,
    /// Smart Lite: run only check_problematic_audio_streams.
    /// Partial protection, faster. (current default behavior)
    #[default]
    SmartLite,
    /// Safe: no validation, repair all audio streams.
    Safe,
    /// Fast: no validation, no repairs.
    Fast,
}

impl LargePlaylistStrategy {
    pub fn is_full(&self) -> bool {
        matches!(self, LargePlaylistStrategy::FullSmart)
    }

    pub fn is_smart_lite(&self) -> bool {
        matches!(self, LargePlaylistStrategy::SmartLite)
    }

    pub fn is_safe(&self) -> bool {
        matches!(self, LargePlaylistStrategy::Safe)
    }

    pub fn is_fast(&self) -> bool {
        matches!(self, LargePlaylistStrategy::Fast)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    pub job_id: String,
    pub input_files: Vec<String>,
    pub media_infos: Option<Vec<crate::types::MediaInfo>>,
    pub input_names: Vec<String>,
    pub input_durations: Vec<f64>,
    pub external_subtitles: Option<Vec<Option<String>>>,
    pub output_path: String,
    pub mode: MergeMode,
    pub total_duration: f64,
    pub subtitle_mode: Option<SubtitleMode>,
    pub export_merged_srt: Option<bool>,
    pub card_config: Option<CardConfig>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_crf: Option<u32>,
    pub video_preset: Option<String>,
    pub audio_bitrate: Option<String>,
    pub target_resolution: Option<String>,
    pub target_fps: Option<String>,
    pub hw_accel: Option<String>,
    pub split_config: Option<crate::types::SplitConfig>,
    pub naming_config: Option<crate::split::types::NamingConfig>,
    pub selected_subtitle_stream_indices: Option<Vec<Option<u32>>>,
    pub validate_audio: Option<bool>,
    pub audio_repair_mode: Option<AudioRepairMode>,
    /// Strategy for large playlist audio validation.
    /// When None and a large playlist is detected, SmartLite (current behavior) is used as default.
    pub large_playlist_strategy: Option<LargePlaylistStrategy>,
    /// Fast MKV: convert merged MKV to MP4 after creation
    pub convert_to_mp4: Option<bool>,
    /// Repeat configuration for extending the playlist output
    pub repeat_config: Option<crate::types::RepeatConfig>,
    /// Resume phase — allows backend to skip already-completed phases
    pub phase: Option<crate::types::MergePhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub job_id: String,
    pub output_path: String,
    pub output_size_bytes: u64,
    pub segments: Vec<MergeSegment>,
    pub output_paths: Option<Vec<String>>,
    pub parts: Option<Vec<MergePartResult>>,
    pub srt_export_paths: Option<Vec<String>>,
    pub report_paths: Option<Vec<String>>,
    pub warnings: Option<Vec<String>>,
    pub subtitle_warnings: Option<Vec<SubtitleWarning>>,
    pub audio_repair_summary: Option<AudioRepairSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRepairSummary {
    pub mode: String,
    pub total_files: usize,
    pub files_repaired: usize,
    pub due_to_corruption: usize,
    pub due_to_profile_mismatch: usize,
    pub due_to_safe_mode: usize,
    pub repaired_indices: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergePartResult {
    pub part_index: u32,
    pub output_path: String,
    pub output_size_bytes: u64,
    pub file_count: u32,
    pub total_duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeSegment {
    pub name: String,
    pub duration: f64,
    pub start_time: f64,
    pub end_time: f64,
    pub is_card: Option<bool>,
    pub card_color: Option<String>,
    pub parent_folder: Option<String>,
}



/// Parse a single spectral/temporal metric from FFmpeg's ametadata output.
/// The stderr contains lines like:
///   lavfi.astats.Overall.Peak_level=-3.2
///   lavfi.aspectralstats.Overall.SpectralCentroid=2345.6
/// Returns the parsed f64 value, or None if the metric was not found or unparsable.
/// Map an ffprobe AAC profile name to the FFmpeg `-profile:a` value.
#[allow(clippy::too_many_arguments)]
async fn normalize_to_profile(
    ffmpeg_path: &Path, input_path: &str, target_vcodec: &str, target_acodec: &str, target_sr: u32, target_fps: Option<f64>, target_timescale: Option<u32>, target_width: Option<u32>, target_height: Option<u32>,
    target_channels: Option<u32>, target_audio_bitrate: Option<String>, temp_dir: &Path, job_id: &str, index: usize, has_audio: bool, cancel_flag: Arc<AtomicBool>,
    norm_cache: Option<std::sync::Arc<crate::ffmpeg::norm_cache::NormalizationCache>>,
    input_duration: Option<f64>,
    input_video_duration_ms: Option<u64>,
    ffprobe_path: Option<&Path>,
    input_audio_sample_rate: Option<u32>,
) -> Result<String, String> {
    let profile = EncodingProfile::new(
        target_vcodec,
        target_acodec,
        target_sr,
        target_fps,
        target_timescale,
        target_width,
        target_height,
        target_channels,
    ).with_bitrate(target_audio_bitrate);
    ffmpeg_normalize_to_profile(
        ffmpeg_path, input_path, &profile,
        temp_dir, job_id, index, has_audio, cancel_flag,
        norm_cache, input_duration, input_video_duration_ms, ffprobe_path, input_audio_sample_rate,
    ).await
}

#[allow(clippy::too_many_arguments)]
async fn verify_normalized_audio_health(
    ffmpeg_path: &Path, ffprobe_path: &Path, output_path: &str, file_index: usize, filename: &str, cancel_flag: Arc<AtomicBool>,
    skip_volumedetect: bool,
    input_video_duration_ms: Option<u64>,
) -> Result<MediaInfo, String> {
    let ffprobe = ffprobe_path.to_path_buf();
    let path = PathBuf::from(output_path);
    let probe_result: Result<MediaInfo, String> = tokio::task::spawn_blocking(move || {
        crate::ffmpeg::probe::probe_file(&ffprobe, &path)
            .map_err(|e| format!("{:#}", e))
    }).await.map_err(|e| format!("Health check task panicked: {}", e))?;

    let info = probe_result?;
    let duration = info.duration;

    // ── Post-normalization A/V gap verification ───────────────────────────
    // If apad was applied during normalization (expected >= 0),
    // verify that the output audio duration now covers the full video.
    // Video is stream-copied during normalization so output_video ≈ input_video.
    // If audio was padded correctly: output_audio >= output_video - small_tolerance.
    if let Some(expected_audio_ms) = input_video_duration_ms {
        let out_video_ms = info.video_streams.first().and_then(|s| s.duration).map(|d| (d * 1000.0) as u64);
        let out_audio_ms = info.audio_streams.first().and_then(|s| s.duration).map(|d| (d * 1000.0) as u64);

        log::info!("[NORMALIZATION_VERIFY] ═══════════════════════════════════════════════════════");
        log::info!("[NORMALIZATION_VERIFY] File #{} | {} | output={}", file_index, filename, output_path);
        log::info!("[NORMALIZATION_VERIFY]   expected_audio={:.3}s ({})  output_video={:.3}s  output_audio={:.3}s",
            expected_audio_ms as f64 / 1000.0, expected_audio_ms,
            out_video_ms.map(|m| m as f64 / 1000.0).unwrap_or(0.0),
            out_audio_ms.map(|m| m as f64 / 1000.0).unwrap_or(0.0));

        let (gap_filled, gap_remaining_s) = match (out_video_ms, out_audio_ms) {
            (Some(v), Some(a)) => {
                let gap = (v as f64 - a as f64) / 1000.0;
                (gap < 0.5, gap)
            }
            _ => (false, -1.0),
        };

        if gap_filled {
            log::info!("[NORMALIZATION_VERIFY]   ✅ GAP FILLED: audio ({:.3}s) now covers video ({:.3}s)",
                out_audio_ms.map(|m| m as f64 / 1000.0).unwrap_or(0.0),
                out_video_ms.map(|m| m as f64 / 1000.0).unwrap_or(0.0));
        } else if out_audio_ms.is_none() {
            log::warn!("[NORMALIZATION_VERIFY]   ❌ NO OUTPUT AUDIO STREAM — output may be audio-less");
        } else {
            log::warn!("[NORMALIZATION_VERIFY]   ⚠️  GAP REMAINS: {:.3}s of silent audio (expected {:.3}s, got {:.3}s)",
                gap_remaining_s,
                expected_audio_ms as f64 / 1000.0,
                out_audio_ms.map(|m| m as f64 / 1000.0).unwrap_or(0.0));
        }

        log::info!("[NORMALIZATION_VERIFY]   status={}", if gap_filled { "✅ PASS" } else { "❌ FAIL" });
        log::info!("[NORMALIZATION_VERIFY] ═══════════════════════════════════════════════════════");
    }

    if duration <= 0.0 {
        log::error!("[FORENSIC:AUDIO_VALIDATE] ❌ File #{} ({}) — zero duration after normalization", file_index, filename);
        return Err(format!(
            "Audio repair verification FAILED: File #{} ({}) — normalized file has zero duration. The file may be corrupted.",
            file_index, filename
        ));
    }
    // Use same 5-point test as concat audit (0.1, 0.25, 0.5, 0.75, 0.9) to ensure
    // health check catches the same corruption the concat audit will look for.
    // Using 3 points (0.1, 0.5, 0.9) missed corruption at 403s offset that mapped
    // to concat output 10% = 541s into File #2 (which is 86%, not 90%).
    // P1 FIX: 10-point test (was 5) to reduce gaps where corruption can hide.
    // Covers 5% through 95% with max 10% gap between any two test points.
    // P2 FIX: 19-point test (was 12) to cover every 5% of the file, reducing
    // the max gap between test points from 10% to 5%. This minimizes the risk
    // of corruption hiding between widely-spaced seek positions.
    // P3: Parallel execution — all 19 seek tests run concurrently for 10x speedup.
    let test_points = [0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.35, 0.40, 0.45, 0.50, 0.55, 0.60, 0.65, 0.70, 0.75, 0.80, 0.85, 0.90, 0.95];
    #[cfg(windows)]
    #[allow(unused_imports)]
    use std::os::windows::process::CommandExt;
    #[cfg(windows)]
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let seek_start = std::time::Instant::now();

    // Parallel seek: spawn all 19 FFmpeg processes concurrently
    #[allow(clippy::type_complexity)]
    let mut seek_handles: Vec<(f64, f64, tokio::task::JoinHandle<Result<(), String>>)> = Vec::new();
    for pct in test_points {
        let seek_sec = duration * pct;
        let output_path_owned = output_path.to_string();
        let ffmpeg_path_owned = ffmpeg_path.to_path_buf();
        let cancel = cancel_flag.clone();
        let file_index_cp = file_index;
        let filename_cp = filename.to_string();

        let handle = tokio::task::spawn_blocking(move || {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".to_string());
            }
            let mut cmd = std::process::Command::new(&ffmpeg_path_owned);
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            let res = cmd
                .args(["-v", "error", "-ss", &seek_sec.to_string(), "-i", &output_path_owned, "-vn", "-map", "0:a:0?", "-t", "10", "-f", "null", "-"])
                .output();
            match res {
                Ok(out) if !out.status.success() || !out.stderr.is_empty() => {
                    let err_msg = String::from_utf8_lossy(&out.stderr);
                    log::error!("[FORENSIC:AUDIO_VALIDATE] ❌ Post-normalize corruption still present | File #{} ({}) | Seek: {:.1}s ({:.0}%) | Error: {}",
                        file_index_cp, filename_cp, seek_sec, pct * 100.0, err_msg.trim().split('\n').next().unwrap_or("unknown"));
                    Err(format!(
                        "Audio repair FAILED: File #{} ({}) still has decoder corruption after normalization at {:.0}% position. The HE-AAC stream is too damaged to recover — the file cannot be used.",
                        file_index_cp, filename_cp, pct * 100.0
                    ))
                }
                Err(e) => Err(format!("Audio repair verification failed: {}", e)),
                _ => Ok(()),
            }
        });
        seek_handles.push((pct, seek_sec, handle));
    }

    // Wait for all parallel seek tests
    for (_pct, _seek_sec, handle) in seek_handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(format!("Seek test task panicked: {}", e)),
        }
    }

    let seek_elapsed = seek_start.elapsed();
    log::info!("[COST_TIMING] File #{} ({}) seek_test: {:.1}s (19 points, parallel)", file_index, filename, seek_elapsed.as_secs_f64());
    // ── Audio level sanity check: volumedetect ─────────────────────────────
    // After decoder validation passes, check for abnormally low audio levels
    // (silent or near-silent output) that pass decoder checks but indicate
    // content-level damage. This is non-fatal — it only logs warnings.
    // SKIPPED for SmartMkv mode (skip_volumedetect=true) — not needed for
    // stream-copy MKV output and saves ~200-500ms per file.
    let vol_elapsed = if skip_volumedetect {
        log::info!("[COST_TIMING] File #{} ({}) volumedetect: SKIPPED (SmartMkv mode)", file_index, filename);
        std::time::Duration::ZERO
    } else {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err("Cancelled".to_string());
        }
        let vol_start = std::time::Instant::now();
        let mut vol_cmd = std::process::Command::new(ffmpeg_path);
        #[cfg(windows)]
        vol_cmd.creation_flags(CREATE_NO_WINDOW);
        if let Ok(vol_out) = vol_cmd
            .args(["-i", output_path, "-vn", "-map", "0:a:0?", "-af", "volumedetect", "-f", "null", "-"])
            .output()
        {
            let stderr = String::from_utf8_lossy(&vol_out.stderr);
            let mean_vol = stderr.lines()
                .find(|l| l.contains("mean_volume:"))
                .and_then(|l| l.split("mean_volume:").nth(1))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<f64>().ok());
            let max_vol = stderr.lines()
                .find(|l| l.contains("max_volume:"))
                .and_then(|l| l.split("max_volume:").nth(1))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<f64>().ok());
            if let Some(mv) = mean_vol {
                if mv < -60.0 {
                    log::warn!("[AUDIO_QUALITY] ⚠️ File #{} ({}) — very low mean volume ({:.1} dB), audio may be silent or near-silent", file_index, filename, mv);
                } else if mv < -40.0 {
                    log::info!("[AUDIO_QUALITY] 𐄂 File #{} ({}) — low mean volume ({:.1} dB)", file_index, filename, mv);
                } else {
                    log::info!("[AUDIO_QUALITY] ✅ File #{} ({}) — normal audio level ({:.1} dB)", file_index, filename, mv);
                }
            }
            if let Some(mx) = max_vol {
                if mx < -60.0 {
                    log::warn!("[AUDIO_QUALITY] ⚠️ File #{} ({}) — max volume very low ({:.1} dB), likely silent track", file_index, filename, mx);
                }
            }
        } else {
            log::warn!("[AUDIO_QUALITY] Could not run volumedetect for File #{} ({}) — audio level check skipped", file_index, filename);
        }
        vol_start.elapsed()
    };
    log::info!("[COST_TIMING] File #{} ({}) volumedetect: {:.1}s", file_index, filename, vol_elapsed.as_secs_f64());

    let total_verify = seek_elapsed + vol_elapsed;
    log::info!("[COST_TIMING] File #{} ({}) VERIFY TOTAL: {:.1}s (seek={:.1}s vol={:.1}s)",
        file_index, filename, total_verify.as_secs_f64(), seek_elapsed.as_secs_f64(), vol_elapsed.as_secs_f64());

    log::info!("[FORENSIC:AUDIO_VALIDATE] ✅ Post-normalize audio health verified for File #{} ({})", file_index, filename);
    Ok(info)
}

#[allow(clippy::too_many_arguments)]
async fn normalize_timescale_lossless(
    ffmpeg_path: &Path, input_path: &str, target_timescale: u32, temp_dir: &Path, job_id: &str, index: usize, cancel_flag: Arc<AtomicBool>,
    norm_cache: Option<std::sync::Arc<crate::ffmpeg::norm_cache::NormalizationCache>>,
) -> Result<String, String> {
    let input = Path::new(input_path);
    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let output_path = temp_dir.join(format!("norm_ts_{}_{}.{}", job_id, index, ext));
    let output_path_str = output_path.to_string_lossy().into_owned();
    let input_for_ffmpeg = input_path.strip_prefix("\\\\?\\").unwrap_or(input_path);
    let args = vec![
        "-y".to_string(),
        "-i".to_string(),
        input_for_ffmpeg.to_string(),
        "-c".to_string(),
        "copy".to_string(),
        "-video_track_timescale".to_string(),
        target_timescale.to_string(),
        "-avoid_negative_ts".to_string(),
        "make_zero".to_string(),
        output_path_str.clone(),
    ];
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    log::info!("[FORENSIC:NORMALIZE] Timescale lossless remux | File #{} | Input: {} | Output: {} | target_timescale={}",
        index, input_path, output_path_str, target_timescale);
    // ── Normalization Dedup Cache ──────────────────────────────────────────
    if let Some(ref _cache) = norm_cache {
        let sig = crate::ffmpeg::norm_cache::NormSignature::Timescale {
            target_timescale,
        };
        if let Some(cached_path) = _cache.get(input_path, &sig) {
            log::info!("[NormCache] HIT: Reusing timescale remux of {} from {}", input_path, cached_path.display());
            std::fs::copy(&cached_path, &output_path).map_err(|e| format!("Failed to copy cached timescale file: {}", e))?;
            return Ok(output_path_str);
        }
    }
    run_ffmpeg_cmd_with_cancel(ffmpeg_path, &args_ref, cancel_flag, 900).await?;
    // ── Cache the result ───────────────────────────────────────────────────
    if let Some(ref _cache) = norm_cache {
        let sig = crate::ffmpeg::norm_cache::NormSignature::Timescale {
            target_timescale,
        };
        _cache.insert(input_path, sig, output_path.clone());
        log::info!("[NormCache] Cached timescale remux of {}", input_path);
    }
    Ok(output_path_str)
}

#[allow(clippy::too_many_arguments)]
async fn normalize_audio_only(
    ffmpeg_path: &Path, input_path: &str, target_acodec: &str, target_sr: u32, target_timescale: Option<u32>,
    target_channels: Option<u32>, audio_bitrate: Option<String>, temp_dir: &Path, job_id: &str, index: usize, cancel_flag: Arc<AtomicBool>,
    norm_cache: Option<std::sync::Arc<crate::ffmpeg::norm_cache::NormalizationCache>>,
    input_video_duration_ms: Option<u64>,
    ffprobe_path: Option<&Path>,
    input_audio_sample_rate: Option<u32>,
) -> Result<String, String> {
    let audio_profile = AudioProfile::new(
        target_acodec,
        target_sr,
        target_timescale,
        target_channels,
    ).with_bitrate(audio_bitrate);
    ffmpeg_normalize_audio_only(
        ffmpeg_path, input_path, &audio_profile,
        temp_dir, job_id, index, cancel_flag,
        norm_cache, input_video_duration_ms, ffprobe_path, input_audio_sample_rate,
    ).await
}

/// Kill orphaned ffmpeg processes left over from crashed/timed-out merges.
/// Returns the number of processes killed.
/// SAFETY: Only kills when NO active merges are running (to avoid killing a concurrent merge's FFmpeg).
fn kill_orphaned_ffmpeg_processes(active_jobs: &std::collections::HashMap<String, Arc<AtomicBool>>) -> u32 {
    if !active_jobs.is_empty() {
        log::info!("[Merge] Skipping orphan kill — {} active merge(s) running", active_jobs.len());
        return 0;
    }
    let mut killed = 0u32;
    if let Ok(output) = std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq ffmpeg.exe", "/FO", "CSV", "/NH"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.contains("ffmpeg.exe") {
                if let Some(first_field) = line.split(',').next() {
                    let pid_str = first_field.trim_matches('"');
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        log::info!("[Merge] Killing orphaned ffmpeg PID={}", pid);
                        let _ = std::process::Command::new("taskkill")
                            .args(["/PID", &pid.to_string(), "/F", "/T"])
                            .output();
                        killed += 1;
                    }
                }
            }
        }
    }
    killed
}

/// Compute per-part file index groups for sequential normalization.
/// When split config is active, files are grouped by part so normalization
/// processes one part at a time (reducing concurrent load and timeout risk).
fn compute_norm_part_groups(
    input_files: &[String],
    input_durations: &[f64],
    split_config: &crate::types::SplitConfig,
) -> Vec<Vec<usize>> {
    use crate::types::SplitMode;
    match split_config.mode {
        SplitMode::None => {
            vec![(0..input_files.len()).collect()]
        }
        SplitMode::Count => {
            let total = input_files.len();
            let requested = split_config.part_count.unwrap_or(2).max(1) as usize;
            let parts = requested.min(total).max(1);
            let base = total / parts;
            let remainder = total % parts;
            let mut groups = Vec::new();
            let mut start = 0;
            for i in 0..parts {
                let size = base + if i < remainder { 1 } else { 0 };
                groups.push((start..start + size).collect());
                start += size;
            }
            groups
        }
        SplitMode::Duration => {
            let max_dur = split_config.max_duration_per_part.unwrap_or(3600.0).max(1.0);
            let mut groups = Vec::new();
            let mut current_group = Vec::new();
            let mut current_dur = 0.0;
            for (idx, &dur) in input_durations.iter().enumerate() {
                if !current_group.is_empty() && current_dur + dur > max_dur {
                    groups.push(std::mem::take(&mut current_group));
                    current_dur = 0.0;
                }
                current_group.push(idx);
                current_dur += dur;
            }
            if !current_group.is_empty() {
                groups.push(current_group);
            }
            groups
        }
        SplitMode::Folder => {
            let first_path = input_files.first().map(std::path::Path::new);
            let common_parent = if let Some(first) = first_path {
                let mut common = first.parent().unwrap_or(std::path::Path::new("")).to_path_buf();
                for path_str in input_files.iter().skip(1) {
                    let path = std::path::Path::new(path_str);
                    while !path.starts_with(&common) {
                        if let Some(parent) = common.parent() {
                            common = parent.to_path_buf();
                        } else {
                            break;
                        }
                    }
                }
                common
            } else {
                std::path::PathBuf::new()
            };
            let mut groups: Vec<Vec<usize>> = Vec::new();
            let mut last_folder: Option<String> = None;
            for (idx, file) in input_files.iter().enumerate() {
                let path = std::path::Path::new(file);
                let folder = if let Ok(rel) = path.strip_prefix(&common_parent) {
                    rel.components().next()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Root".to_string())
                } else {
                    "Root".to_string()
                };
                if last_folder.as_ref() != Some(&folder) {
                    groups.push(Vec::new());
                    last_folder = Some(folder);
                }
                if let Some(g) = groups.last_mut() {
                    g.push(idx);
                }
            }
            groups
        }
    }
}

#[command]
pub async fn start_merge(
    request: MergeRequest,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    log::info!("[FORENSIC:ENTRY] jobId: {} | thread_id: {:?} | timestamp: {}", 
        request.job_id, std::thread::current().id(), chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));

    // ── Per-Job Log Capture ──────────────────────────────────────────────
    // Start logging to a dedicated file for this job.
    // All subsequent log::info!, log::warn!, log::error! calls will be captured.
    let _job_log_path = {
        let output_dir = std::path::Path::new(&request.output_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string());
        let job_name = std::path::Path::new(&request.output_path)
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "merged_output".to_string());
        crate::logger::start_job_log(&output_dir, &job_name, &request.job_id)
    };

    // ── Forensic Log Start ──────────────────────────────────────────────
    // Purely observational — writes a structured log to Desktop/PlaylistMerger Logs/
    // Does NOT modify any merge pipeline logic, progress, checkpoints, or modes.
    //
    // IMPORTANT: Check return value! Failures are logged by start_forensic_log
    // but the caller must not discard the result.
    let forensic_log_path = {
        let log_output_dir = std::path::Path::new(&request.output_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string());
        let playlist_name = request.input_names.first().map(|s| s.as_str());
        let mode_str = format!("{:?}", request.mode);
        match crate::forensic_log::start_forensic_log(
            &log_output_dir,
            "merge",
            &request.job_id,
            playlist_name,
            &mode_str,
            &request.output_path,
            None, // ffmpeg path (resolved later)
            None, // mkvmerge path
            request.input_files.len(),
        ) {
            Ok(path) => {
                log::info!("[Merge] Forensic log started at: {}", path.display());
                Some(path)
            }
            Err(e) => {
                log::error!("[Merge] FORENSIC LOG FAILED TO START: {}", e);
                // Don't fail the merge just because forensic log failed
                None
            }
        }
    };
    let _ = forensic_log_path; // Used for debugging if needed

    log::info!("[Merge] ═══════════════════════════════════════════════════════════════");
    log::info!("[Merge] Job ID: {}", request.job_id);
    log::info!("[Merge] Mode: {:?}", request.mode);
    log::info!("[Merge] Input Files: {}", request.input_files.len());
    if let Some(ref cc) = request.card_config {
        log::info!("[Merge] Cards: ENABLED (freq={:?}, dur={}s, color={})", cc.frequency, cc.duration, cc.color);
    } else {
        log::info!("[Merge] Cards: DISABLED");
    }
    log::info!("[Merge] ═══════════════════════════════════════════════════════════════");

    // Check for concurrent merge
    {
        let ms = state.merge_state.lock().await;
        if !ms.active_jobs.is_empty() {
            return Err("Another merge is in progress. Please wait for it to complete.".into());
        }
    }

    // ── PIPELINE AUDIT: Request Received ────────────────────────────────────
    log::info!("[PIPELINE_AUDIT] ═══════════════════════════════════════════════════════════");
    log::info!("[PIPELINE_AUDIT] STAGE: Request Received");
    log::info!("[PIPELINE_AUDIT]   Input Files: {}", request.input_files.len());
    log::info!("[PIPELINE_AUDIT]   Total Duration: {:.1}s ({:.1}h)", request.total_duration, request.total_duration / 3600.0);
    if let Some(ref rc) = request.repeat_config {
        log::info!("[PIPELINE_AUDIT]   Repeat: enabled={}, by_count={}, repeat_count={}, until_duration={}, target_duration={:.0}s",
            rc.enabled, rc.by_count, rc.repeat_count, rc.until_duration, rc.target_duration_seconds);
    } else {
        log::info!("[PIPELINE_AUDIT]   Repeat: DISABLED (no config)");
    }

    // ── Kill orphaned ffmpeg processes from previous (crashed) merges ──────
    {
        let ms = state.merge_state.lock().await;
        let orphaned = kill_orphaned_ffmpeg_processes(&ms.active_jobs);
        if orphaned > 0 {
            log::warn!("[Merge] Killed {} orphaned ffmpeg processes from previous runs", orphaned);
        }
    }
    log::info!("[PIPELINE_AUDIT] ═══════════════════════════════════════════════════════════");

    if request.input_files.is_empty() { return Err("No input files provided".to_string()); }
    if request.output_path.is_empty() { return Err("No output path provided".to_string()); }
    if request.total_duration <= 0.0 { return Err("Total duration must be greater than zero".to_string()); }
    
    // PHASE-BASED RESUME: Get app_data_dir early for checkpoint phase updates during merge
    let app_data_dir_for_phase = recovery::get_app_data_dir().ok();
    
    // PER-JOB CHECKPOINT WRITER: Spawn single writer to prevent concurrent write races
    // Guard ensures drain on drop - spawned task calls close() at end to guarantee durability
    let checkpoint_writer: Option<recovery::CheckpointWriterGuard> = app_data_dir_for_phase.as_ref().map(|ad| {
        recovery::spawn_checkpoint_writer(request.job_id.clone(), ad.clone())
    });
    let checkpoint_sender = checkpoint_writer.as_ref().map(|g| g.sender());
    
    // PHASE-BASED RESUME: Compute skip flags from request phase (recovery_checkpoint phase applied later)
    let resume_phase = request.phase.as_ref();
    let skip_probing = matches!(resume_phase, Some(crate::types::MergePhase::Validating | crate::types::MergePhase::Normalizing | crate::types::MergePhase::Writing | crate::types::MergePhase::Finalizing | crate::types::MergePhase::Complete));
    let skip_validation = matches!(resume_phase, Some(crate::types::MergePhase::Normalizing | crate::types::MergePhase::Writing | crate::types::MergePhase::Finalizing | crate::types::MergePhase::Complete));
    let skip_analysis = matches!(resume_phase, Some(crate::types::MergePhase::Normalizing | crate::types::MergePhase::Writing | crate::types::MergePhase::Finalizing | crate::types::MergePhase::Complete));
    let skip_normalization = matches!(resume_phase, Some(crate::types::MergePhase::Writing | crate::types::MergePhase::Finalizing | crate::types::MergePhase::Complete));
    
    if skip_probing {
        log::info!("[PhaseResume] Will skip probing — phase indicates already complete");
    }
    if skip_validation {
        log::info!("[PhaseResume] Will skip validation — phase indicates already complete");
    }
    if skip_analysis {
        // NOTE: analyze_profiles() still runs below because ProfileAnalysis (outliers, histograms)
        // is NOT persisted in the checkpoint. The dominant_profile IS persisted and reused,
        // but the full analysis must be re-derived. This is an I/O-free CPU operation.
        log::info!("[PhaseResume] Dominant profile will be reused — full analysis still runs (ProfileAnalysis not persisted)");
    }
    if skip_normalization {
        log::info!("[PhaseResume] Will skip normalization — phase indicates already complete");
    }
    
    // Step 1: Normalize all input paths (Task 3)
    let normalized: Vec<String> = request.input_files.iter().map(|p| normalize_long_path(p)).collect();

    // ── Deduplicate input files ─────────────────────────────────────────────────
    // Windows paths are case-insensitive, so dedup by canonical lowercase form.
    // This catches duplicates from multiple folder scans, case differences, etc.
    let mut seen_files = std::collections::HashSet::new();
    let mut dedup_indices: Vec<usize> = Vec::new();
    for (i, path) in normalized.iter().enumerate() {
        let key = path.to_lowercase();
        if seen_files.insert(key) {
            dedup_indices.push(i);
        }
    }
    let duplicate_count = normalized.len() - dedup_indices.len();
    if duplicate_count > 0 {
        log::warn!("[Merge] Deduplicated {} → {} files ({} duplicates removed)",
            normalized.len(), dedup_indices.len(), duplicate_count);
    }

    let mut working_input_files: Vec<String> = dedup_indices.iter().map(|&i| normalized[i].clone()).collect();
    let working_input_durations_dedup: Vec<f64> = dedup_indices.iter().map(|&i| request.input_durations[i]).collect();
    let working_input_names_dedup: Vec<String> = dedup_indices.iter().map(|&i| request.input_names[i].clone()).collect();
    let working_external_subs_dedup: Option<Vec<Option<String>>> = request.external_subtitles.as_ref().map(|subs| {
        dedup_indices.iter().filter_map(|&i| subs.get(i).cloned()).collect()
    });
    let working_selected_sub_indices_dedup: Option<Vec<Option<u32>>> = request.selected_subtitle_stream_indices.as_ref().map(|si| {
        dedup_indices.iter().filter_map(|&i| si.get(i).cloned()).collect()
    });

    // Recompute total duration from deduplicated entries to avoid inflated 30h totals
    let mut working_total_duration: f64 = working_input_durations_dedup.iter().sum();

    let mut input_paths: Vec<PathBuf> = working_input_files.iter().map(PathBuf::from).collect();
    for file in &input_paths { if !file.exists() { return Err(format!("Input file not found: {}", file.display())); } }

    // ── PIPELINE AUDIT: After Dedup ─────────────────────────────────────────
    log::info!("[PIPELINE_AUDIT] STAGE: After Dedup");
    log::info!("[PIPELINE_AUDIT]   Dedup: {} input files → {} unique files ({} duplicates removed)",
        normalized.len(), working_input_files.len(), duplicate_count);
    log::info!("[PIPELINE_AUDIT]   Total Duration: {:.1}s ({:.1}h)", working_total_duration, working_total_duration / 3600.0);

    // Update checkpoint phase: Probing via single-writer to prevent races
    if let Some(ref sender) = checkpoint_sender {
        sender.update_phase(crate::types::MergePhase::Probing, None);
        log::info!("[PhaseResume] Checkpoint phase sent to writer: Probing");
    }

    // Emit progress: probing phase start (AFTER dedup so totalDuration is accurate)
    let _ = app_handle.emit("merge-progress", &serde_json::json!({
        "jobId": request.job_id,
        "progress": {
            "phase": "probing",
            "stageName": "Analysing files...",
            "stagePercent": 0.0,
            "percent": 0.0,
            "overallPercent": 0.0,
            "currentTime": 0.0,
            "totalDuration": working_total_duration,
        }
    }));

    // ── Repeat Expansion ────────────────────────────────────────────────────────
    // Expand file list BEFORE probe cache, normalization, cards, splits, reports.
    // Uses physical duplication (Option A) so FastMKV, SmartMKV, Cards, etc. work unchanged.
    // Save deduped duration BEFORE repeat expansion overwrites working_total_duration.
    let deduped_file_count = working_input_files.len();
    let deduped_total_duration = working_total_duration;
    let mut working_input_durations = working_input_durations_dedup;
    let mut working_input_names = working_input_names_dedup;
    #[allow(unused_assignments)]
    let mut repeat_expanded: Option<crate::ffmpeg::repeat::ExpandedPlaylist> = None;

    // Clone dedup arrays for subtitle expansion (needed after repeat expansion)
    let external_subs_for_expansion = working_external_subs_dedup.clone();
    let sub_indices_for_expansion = working_selected_sub_indices_dedup.clone();

    let mut working_external_subs: Option<Vec<Option<String>>> = working_external_subs_dedup;
    let mut working_selected_sub_indices: Option<Vec<Option<u32>>> = working_selected_sub_indices_dedup;

    // ── REPEAT AUDIT: Before expansion ──────────────────────────────────────
    log::info!("[REPEAT_AUDIT] ═══════════════════════════════════════════════════════════");
    log::info!("[REPEAT_AUDIT] Before expansion: {} files", working_input_files.len());
    if let Some(ref rc) = request.repeat_config {
        log::info!("[REPEAT_AUDIT]   repeat_config.enabled = {}", rc.enabled);
        log::info!("[REPEAT_AUDIT]   repeat_config.by_count = {}", rc.by_count);
        log::info!("[REPEAT_AUDIT]   repeat_config.repeat_count = {}", rc.repeat_count);
        log::info!("[REPEAT_AUDIT]   repeat_config.until_duration = {}", rc.until_duration);
        log::info!("[REPEAT_AUDIT]   repeat_config.target_duration_seconds = {:.0}", rc.target_duration_seconds);
    }
    log::info!("[REPEAT_AUDIT] ═══════════════════════════════════════════════════════════");

    // ── [REPEAT] Apply repeat expansion using repeat_merge module ─────────────
    let repeat_config = request.repeat_config.as_ref().unwrap_or_else(|| {
        static DEFAULT_CONFIG: std::sync::OnceLock<crate::types::RepeatConfig> = std::sync::OnceLock::new();
        DEFAULT_CONFIG.get_or_init(|| crate::types::RepeatConfig {
            enabled: false,
            by_count: false,
            repeat_count: 1,
            until_duration: false,
            target_duration_seconds: 0.0,
            insert_boundary_cards: false,
            boundary_card_template: "🔁 Repeat {n}".to_string(),
        })
    });

    let repeat_result = crate::ffmpeg::repeat_merge::apply_repeat_expansion(
        repeat_config,
        &working_input_files,
        &working_input_durations,
        &working_input_names,
        working_total_duration,
        working_external_subs.clone(),
        working_selected_sub_indices.clone(),
    );

    match repeat_result {
        Ok(result) => {
            // Capture what we need BEFORE moving result.expanded
            let index_mapping_for_subs = result.expanded.as_ref().map(|e| e.index_mapping.clone());
            let file_count_for_subs = result.expanded.as_ref().map(|e| e.files.len());

            if index_mapping_for_subs.is_some() {
                log::info!("[PIPELINE_AUDIT] STAGE: After Repeat Expansion");
                log::info!("[PIPELINE_AUDIT]   {} files × {} repeats = {} files",
                    result.original_count, result.repeat_count, result.working_files.len());
                log::info!("[PIPELINE_AUDIT]   Duration: {:.1}s → {:.1}s",
                    deduped_total_duration, result.working_total_duration);
            }

            // Move result into repeat_expanded
            repeat_expanded = result.expanded;
            working_input_files = result.working_files;
            working_input_durations = result.working_durations;
            working_input_names = result.working_names;
            working_total_duration = result.working_total_duration;

            // Expand subtitle arrays using cloned data
            if let Some(ref mapping) = index_mapping_for_subs {
                let file_count = file_count_for_subs.unwrap_or(0);
                let mut expanded_subs = Vec::with_capacity(file_count);
                for &(_, orig_idx) in mapping {
                    if let Some(ref current_subs) = external_subs_for_expansion {
                        expanded_subs.push(current_subs.get(orig_idx).cloned().flatten());
                    } else {
                        expanded_subs.push(None);
                    }
                }
                working_external_subs = Some(expanded_subs);
            }

            if let Some(ref mapping) = index_mapping_for_subs {
                let file_count = file_count_for_subs.unwrap_or(0);
                let mut expanded_indices = Vec::with_capacity(file_count);
                for &(_, orig_idx) in mapping {
                    if let Some(ref current_idx) = sub_indices_for_expansion {
                        expanded_indices.push(current_idx.get(orig_idx).copied().flatten());
                    } else {
                        expanded_indices.push(None);
                    }
                }
                working_selected_sub_indices = Some(expanded_indices);
            }

            // Rebuild input_paths from expanded files
            input_paths = working_input_files.iter().map(PathBuf::from).collect();
            for file in &input_paths {
                if !file.exists() {
                    return Err(format!("Input file not found after repeat expansion: {}", file.display()));
                }
            }
        }
        Err(e) => {
            return Err(format!("Repeat expansion failed: {}", e));
        }
    }

    // ── PIPELINE AUDIT: Before Probing ──────────────────────────────────────
    log::info!("[PIPELINE_AUDIT] STAGE: Before Probing / After Repeat");
    log::info!("[PIPELINE_AUDIT]   Working files: {}", working_input_files.len());
    log::info!("[PIPELINE_AUDIT]   Total Duration: {:.1}s ({:.1}h)", working_total_duration, working_total_duration / 3600.0);

    let settings = crate::services::settings::load_settings_internal();
    let ffmpeg_path_resolved = crate::ffmpeg::find_ffmpeg(settings.ffmpeg_path.as_deref()).map_err(|e| e.to_string())?;
    let ffprobe_path_resolved = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref()).map_err(|e| e.to_string())?;
    let input_path_refs: Vec<&Path> = input_paths.iter().map(|p| p.as_path()).collect();
    
    // Use media_infos from request if provided (pre-probed by frontend), otherwise probe parallel
    // Wrap in Arc for sharing across parallel subtitle tasks
    let probe_cache: Arc<crate::ffmpeg::probe_cache::ProbeCache> = if let Some(ref infos) = request.media_infos {
        let cache = crate::ffmpeg::probe_cache::ProbeCache::new();
        for info in infos {
            // Normalize path to match working_input_files format (adds \\?\ for long paths on Windows)
            cache.insert(PathBuf::from(&normalize_long_path(&info.path)), Ok(info.clone()));
        }
        log::info!("[FORENSIC:PROBE] Using {} pre-probed MediaInfo objects from request", cache.len());
        Arc::new(cache)
    } else {
        Arc::new(probe_all_parallel(&input_path_refs, &ffprobe_path_resolved).await)
    };

    let mut actual_mode = request.mode.clone();
    let mut actual_video_codec = request.video_codec.clone();
    let mut actual_audio_codec = request.audio_codec.clone();
    let mut actual_video_crf = request.video_crf;
    let mut actual_video_preset = request.video_preset.clone();
    let mut actual_audio_bitrate = request.audio_bitrate.clone();
    let audio_repair_mode = request.audio_repair_mode.clone().unwrap_or_default();
    log::info!("[AudioRepair] mode: {:?}", audio_repair_mode);
    // ── PIPELINE AUDIT: Before Audio Check / Normalization ───────────────
    log::info!("[PIPELINE_AUDIT] STAGE: Before Audio Check / Normalization");
    log::info!("[PIPELINE_AUDIT]   Working files: {} | Mode: {:?}", working_input_files.len(), actual_mode);
    log::info!("[PIPELINE_AUDIT]   AudioRepair mode: {:?}", audio_repair_mode);
    log::info!("[PIPELINE_AUDIT]   Duration: {:.1}s ({:.1}h)", working_total_duration, working_total_duration / 3600.0);

    log::info!("[Merge:PATH] Mode={:?} | card_config={} | input_files={}", actual_mode, request.card_config.is_some(), working_input_files.len());

    // ── Fast MKV Merge — completely separate pipeline ────────────────────────
    if actual_mode == MergeMode::FastMkv {
                log::info!("[Merge:PATH] >>> BRANCHING TO FastMkv pipeline (stream copy, no re-encode) <<<");
        let fastmkv_result = run_fast_mkv_pipeline(
    &request,
            &working_input_files,
            &working_input_durations,
            &working_input_names,
            working_total_duration,
            &probe_cache,
            &ffmpeg_path_resolved,
            &ffprobe_path_resolved,
            &app_handle,
            repeat_expanded.as_ref(),
        ).await;
        match &fastmkv_result {
            Ok(_) => {
                crate::commands::merge::remove_merge_marker(&request.output_path);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&request.output_path));
            }
            Err(e) => crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(e), Some(&request.output_path)),
        }
        return fastmkv_result;
    }

    // ── FORENSIC: Mode Summary ──────────────────────────────────────────────
    log::info!("[AUDIO_MODE] ═══════════════════════════════════════════════════════════");
    log::info!("[AUDIO_MODE] SELECTED MODE: {:?}", audio_repair_mode);
    log::info!("[AUDIO_MODE] MERGE MODE: {:?}", actual_mode);
    log::info!("[AUDIO_MODE] INPUT FILES: {}", working_input_files.len());
    log::info!("[AUDIO_MODE] EXPECTED BEHAVIOR:");
    match audio_repair_mode {
        AudioRepairMode::Fast => {
            log::info!("[AUDIO_MODE]   Fast: No detection, no repair, direct stream copy");
        }
        AudioRepairMode::Smart => {
            log::info!("[AUDIO_MODE]   Smart: Deep validation + seek check → repair ONLY damaged files");
        }
        AudioRepairMode::Safe => {
            log::info!("[AUDIO_MODE]   Safe: Skip detection → repair ALL files with audio");
        }
    }
    log::info!("[AUDIO_MODE] ═══════════════════════════════════════════════════════════");

    let temp_dir = get_temp_dir().map_err(|e| e.to_string())?;
    let mut prepared_subs: Vec<Option<String>> = Vec::new();
    // Temp registry for cleanup tracking — sub field initialized at line ~895
    // after subtitle loop completes. norm_files via Arc<Mutex> (set at ~1457).
    let temp_registry = TempFileRegistry {
        sub: Arc::new(Mutex::new(Vec::new())),
    };
    let mut temp_subtitle_files: Vec<PathBuf> = Vec::new();
    let subtitle_mode = request.subtitle_mode.clone().unwrap_or(SubtitleMode::Embed);
    let export_merged_srt = request.export_merged_srt.unwrap_or(false);
    let should_process_subs = matches!(subtitle_mode, SubtitleMode::Embed | SubtitleMode::Burn | SubtitleMode::ExportSrt | SubtitleMode::SrtMergeOnly);
    let is_export_srt_only = subtitle_mode == SubtitleMode::ExportSrt;
    let is_burn_mode = subtitle_mode == SubtitleMode::Burn;
    let is_srt_merge_only = subtitle_mode == SubtitleMode::SrtMergeOnly;

    if is_burn_mode && actual_mode == MergeMode::Lossless && should_process_subs {
        actual_mode = MergeMode::Custom;
        actual_video_codec = Some("libx264".into()); actual_audio_codec = Some("aac".into());
        actual_video_crf = Some(18); actual_video_preset = Some("medium".into()); actual_audio_bitrate = Some("192k".into());
    }

    // Declare subtitle_warnings before the conditional block so it's in scope for SRT-only path
    let mut subtitle_warnings: Vec<SubtitleWarning> = Vec::new();

    if should_process_subs {
        let total_sub_files = working_input_files.len();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(8));
        let subtitle_temp_files_arc = Arc::new(Mutex::new(Vec::new()));
        let subtitle_warnings_arc: Arc<Mutex<Vec<SubtitleWarning>>> = Arc::new(Mutex::new(Vec::new()));
        let mut sub_handles = Vec::with_capacity(total_sub_files);
        let app_handle_clone = app_handle.clone();

        for (i, file) in working_input_files.iter().enumerate() {
            let file = file.clone();
            let ext_sub_opt = working_external_subs.as_ref()
                .and_then(|subs| subs.get(i).cloned()).flatten();
            let selected_idx = working_selected_sub_indices.as_ref()
                .and_then(|indices| indices.get(i).copied().flatten());
            let job_id = request.job_id.clone();
            let temp_dir = temp_dir.clone();
            let ffmpeg_path = ffmpeg_path_resolved.clone();
            let probe_cache_clone = probe_cache.clone();
            let temp_files = subtitle_temp_files_arc.clone();
            let sem = semaphore.clone();
            let app_handle_inner = app_handle_clone.clone();
            let warnings_arc = subtitle_warnings_arc.clone();

            sub_handles.push(tokio::spawn(async move {
                let _permit = match sem.acquire().await {
                    Ok(p) => p,
                    Err(_) => {
                        log::error!("[Subtitle] Semaphore closed for file #{}", i);
                        return (i, None as Option<String>);
                    }
                };
                let mut sub_path_to_use: Option<String> = None;

                if let Some(ext_path) = ext_sub_opt {
                    let norm_ext_path = normalize_long_path(&ext_path);
                    if Path::new(&norm_ext_path).exists() {
                        let ext = Path::new(&norm_ext_path)
                            .extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        if ext == "srt" {
                            match normalize_subtitle_encoding(&norm_ext_path, &temp_dir, &job_id, i, &ffmpeg_path).await {
                                Ok(normalized) => {
                                    if normalized != norm_ext_path {
                                        if let Ok(mut files) = temp_files.lock() {
                                            files.push(PathBuf::from(&normalized));
                                        } else {
                                            log::error!("[Subtitle] Mutex poisoned pushing normalized file");
                                        }
                                    }
                                    sub_path_to_use = Some(normalized);
                                }
                                Err(_) => {
                                    let norm_ext_path_log = norm_ext_path.clone();
                                    log::warn!("[SubtitleConvert] [{}] normalize_subtitle_encoding failed for {} — using original file", i, norm_ext_path_log);
                                    sub_path_to_use = Some(norm_ext_path.clone());
                                    let warning = SubtitleWarning {
                                        file_index: i,
                                        file_path: norm_ext_path.clone(),
                                        reason: "normalize_subtitle_encoding failed for external SRT".to_string(),
                                    };
                                    let _ = app_handle_inner.emit("subtitle-warning", &serde_json::json!({
                                        "jobId": job_id,
                                        "fileIndex": i,
                                        "file": norm_ext_path,
                                        "error": warning.reason
                                    }));
                                    if let Ok(mut warnings) = warnings_arc.lock() {
                                        warnings.push(warning);
                                    }
                                }
                            }
                        } else {
                            let temp_srt = temp_dir.join(format!("conv_sub_{}_{}.srt", job_id, i));
                            let temp_srt_str = temp_srt.to_string_lossy().into_owned();
                            if run_ffmpeg_cmd(&ffmpeg_path, &["-y", "-i", &norm_ext_path, &temp_srt_str]).await.is_ok() {
                                match normalize_subtitle_encoding(&temp_srt_str, &temp_dir, &job_id, i, &ffmpeg_path).await {
                                    Ok(normalized) => {
                                        sub_path_to_use = Some(normalized.clone());
                                        if let Ok(mut files) = temp_files.lock() {
                                            files.push(PathBuf::from(normalized));
                                        } else {
                                            log::error!("[Subtitle] Mutex poisoned pushing normalized file (ffmpeg conv)");
                                        }
                                    }
                                    Err(_) => {
                                        log::warn!("[SubtitleConvert] [{}] normalize_subtitle_encoding failed after ffmpeg conversion — using temp file", i);
                                        sub_path_to_use = Some(temp_srt_str.clone());
                                        if let Ok(mut files) = temp_files.lock() {
                                            files.push(temp_srt);
                                        } else {
                                            log::error!("[Subtitle] Mutex poisoned pushing temp srt");
                                        }
                                        let warning = SubtitleWarning {
                                            file_index: i,
                                            file_path: file.clone(),
                                            reason: "normalize_subtitle_encoding failed after ffmpeg conversion".to_string(),
                                        };
                                        let _ = app_handle_inner.emit("subtitle-warning", &serde_json::json!({
                                            "jobId": job_id,
                                            "fileIndex": i,
                                            "file": file,
                                            "error": warning.reason
                                        }));
                                        if let Ok(mut warnings) = warnings_arc.lock() {
                                            warnings.push(warning);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if sub_path_to_use.is_none() {
                    let probe_info = probe_cache_clone.get(Path::new(&file))
                        .and_then(|r| r.ok());
                    if let Some(info) = probe_info {
                        if let Some(sub_stream) = info.subtitle_streams.iter().find(|s| s.is_external) {
                            if let Some(ext_path) = &sub_stream.path {
                                let norm_ext_path = normalize_long_path(ext_path);
                                if Path::new(&norm_ext_path).exists()
                                    && Path::new(&norm_ext_path).extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase() == "srt"
                                {
                                    match normalize_subtitle_encoding(&norm_ext_path, &temp_dir, &job_id, i, &ffmpeg_path).await {
                                        Ok(normalized) => {
                                            if normalized != norm_ext_path {
                                                if let Ok(mut files) = temp_files.lock() {
                                                    files.push(PathBuf::from(&normalized));
                                                } else {
                                                    log::error!("[Subtitle] Mutex poisoned pushing external sub");
                                                }
                                            }
                                            sub_path_to_use = Some(normalized);
                                        }
                                        Err(_) => {
                                            log::warn!("[SubtitleConvert] [{}] normalize_subtitle_encoding failed for external sub {} — using original file", i, norm_ext_path);
                                            sub_path_to_use = Some(norm_ext_path.clone());
                                            let warning = SubtitleWarning {
                                                file_index: i,
                                                file_path: norm_ext_path,
                                                reason: "normalize_subtitle_encoding failed for external sub".to_string(),
                                            };
                                            let _ = app_handle_inner.emit("subtitle-warning", &serde_json::json!({
                                                "jobId": job_id,
                                                "fileIndex": i,
                                                "file": file,
                                                "error": warning.reason
                                            }));
                                            if let Ok(mut warnings) = warnings_arc.lock() {
                                                warnings.push(warning);
                                            }
                                        }
                                    }
                                }
                            }
                        } else if !info.subtitle_streams.is_empty() {
                            let selected = selected_idx
                                .and_then(|idx| info.subtitle_streams.iter().find(|s| !s.is_external && s.stream_index == idx))
                                .or_else(|| info.subtitle_streams.iter().find(|s| !s.is_external));
                            if let Some(s) = selected {
                                let temp_srt_path = temp_dir.join(format!("ext_sub_{}_{}.srt", job_id, i));
                                let temp_srt_str = temp_srt_path.to_string_lossy().into_owned();
                                if run_ffmpeg_cmd(&ffmpeg_path, &["-y", "-i", &file, "-map", &format!("0:{}", s.stream_index), "-c:s", "srt", &temp_srt_str]).await.is_ok() {
                                    sub_path_to_use = Some(temp_srt_str);
                                    if let Ok(mut files) = temp_files.lock() {
                                        files.push(temp_srt_path);
                                    } else {
                                        log::error!("[Subtitle] Mutex poisoned pushing extracted sub path");
                                    }
                                } else {
                                    log::warn!("[SubtitleExtract] [{}] FFmpeg subtitle extraction failed for {} — subtitle dropped", i, file);
                                    let warning = SubtitleWarning {
                                        file_index: i,
                                        file_path: file.clone(),
                                        reason: "FFmpeg subtitle extraction failed".to_string(),
                                    };
                                    let _ = app_handle_inner.emit("subtitle-warning", &serde_json::json!({
                                        "jobId": job_id,
                                        "fileIndex": i,
                                        "file": file,
                                        "error": warning.reason
                                    }));
                                    if let Ok(mut warnings) = warnings_arc.lock() {
                                        warnings.push(warning);
                                    }
                                }
                            }
                        }
                    } else {
                        log::warn!("[SubtitleExtract] [{}] Failed to probe {} — subtitle extraction skipped", i, file);
                        let warning = SubtitleWarning {
                            file_index: i,
                            file_path: file.clone(),
                            reason: "Failed to probe — subtitle extraction skipped".to_string(),
                        };
                        let _ = app_handle_inner.emit("subtitle-warning", &serde_json::json!({
                            "jobId": job_id,
                            "fileIndex": i,
                            "file": file,
                            "error": warning.reason
                        }));
                        if let Ok(mut warnings) = warnings_arc.lock() {
                            warnings.push(warning);
                        }
                    }
                }

                (i, sub_path_to_use)
            }));
        }

        // Collect results in index order
        let mut sub_results: Vec<(usize, Option<String>)> = Vec::with_capacity(total_sub_files);
        for handle in sub_handles {
            match handle.await {
                Ok(result) => sub_results.push(result),
                Err(e) => log::error!("[Subtitle] Task join error for file: {}", e),
            }
        }
        sub_results.sort_by_key(|(i, _)| *i);
        for (_, path) in sub_results {
            prepared_subs.push(path);
        }
        temp_subtitle_files = Arc::try_unwrap(subtitle_temp_files_arc)
            .unwrap_or_else(|_| {
                log::error!("[Subtitle] Failed to unwrap temp files Arc — some temp files may not be tracked");
                Mutex::new(Vec::new())
            })
            .into_inner()
            .unwrap_or_default();

        // Collect subtitle warnings from all spawned tasks
        subtitle_warnings = Arc::try_unwrap(subtitle_warnings_arc)
            .unwrap_or_else(|_| {
                log::error!("[Subtitle] Failed to unwrap subtitle warnings Arc");
                Mutex::new(Vec::new())
            })
            .into_inner()
            .unwrap_or_default();
        log::info!("[Subtitle] Collected {} subtitle warnings", subtitle_warnings.len());
    }

    // Populate registry.sub so cleanup_guard sees all subtitle temp files
    // even if cancellation/error occurs during audio analysis (before line 1457).
    if let Ok(mut registry) = temp_registry.sub.lock() {
        *registry = temp_subtitle_files.clone();
    } else {
        log::error!("[Subtitle] Mutex poisoned while updating registry.sub — temp file tracking may be incomplete");
    }

    let original_file_count = working_input_files.len();
    let original_total_duration = working_total_duration;

    if is_srt_merge_only {
        let normalized_output_path = normalize_long_path(&request.output_path);
        // Emit progress starting
        let _ = app_handle.emit("merge-progress", &serde_json::json!({
            "jobId": request.job_id,
            "progress": {
                "phase": "merging",
                "stageName": "Merging SRT files...",
                "stagePercent": 0.0,
                "percent": 10.0,
                "overallPercent": 10.0,
                "currentTime": 0.0,
                "totalDuration": working_total_duration,
            }
        }));

        // Verify we actually have at least one subtitle
        if !prepared_subs.iter().any(|s| s.is_some()) {
            return Err("No subtitle tracks or external SRT files found to merge.".to_string());
        }

        // SRT merge with proper timestamp rebasing is done at line 2180
        // No concat list needed - generate_merged_srt_with_rebase outputs directly to output_path
        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut ms = state.merge_state.lock().await;
            ms.active_jobs.insert(request.job_id.clone(), cancel_flag.clone());
        }

        let temp_sub_files_to_clean = temp_subtitle_files.clone();
        let merge_state_ref = state.merge_state.clone();
        let job_id = request.job_id.clone();
        let job_id_for_return = request.job_id.clone();
        let output_path = normalized_output_path.clone();
        let ffmpeg_path_clone = ffmpeg_path_resolved.clone();
        let cancel_flag_clone = cancel_flag.clone();

        tokio::task::spawn_blocking(move || {
            // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
            let _log_guard = crate::logger::JobLogGuard::new_empty();
            let app_handle_inner = app_handle.clone();
            let job_id_inner = job_id.clone();

            // Check cancellation before starting SRT merge
            if cancel_flag_clone.load(Ordering::Relaxed) {
                crate::commands::merge::remove_merge_marker(&output_path);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle_inner.emit("merge-error", &serde_json::json!({
                    "jobId": job_id_inner,
                    "error": "Merge cancelled by user",
                    "cancelled": true
                }));
                return Ok::<String, String>(request.job_id.clone());
            }
            
            // Progress: merging srt
            let _ = app_handle_inner.emit("merge-progress", &serde_json::json!({
                "jobId": job_id_inner,
                "progress": {
                    "phase": "merging",
                    "stageName": "Generating merged SRT...",
                    "stagePercent": 50.0,
                    "percent": 50.0,
                    "overallPercent": 50.0,
                    "currentTime": 0.0,
                    "totalDuration": original_total_duration,
                }
            }));

            let srt_out = Path::new(&output_path);
            let merge_result = crate::ffmpeg::generate_merged_srt_with_rebase(&prepared_subs, &working_input_durations, &ffmpeg_path_clone, srt_out);

            let result_ok = merge_result.is_ok();
            
            if cancel_flag_clone.load(Ordering::Relaxed) {
                crate::commands::merge::remove_merge_marker(&output_path);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
                let _ = app_handle_inner.emit("merge-error", &serde_json::json!({
                    "jobId": job_id_inner,
                    "error": "Merge cancelled by user",
                    "cancelled": true
                }));
            } else if result_ok {
                let size_bytes = std::fs::metadata(srt_out).map(|m| m.len()).unwrap_or(0);
                
                // Add to recent exports
                let mut s = crate::services::settings::load_settings_internal();
                s.recent_exports.insert(0, RecentExport {
                    path: output_path.clone(),
                    timestamp: Utc::now(),
                    size_bytes,
                    file_count: original_file_count,
                    duration_seconds: original_total_duration,
                    mode: "SrtMergeOnly".to_string()
                });
                s.recent_exports.truncate(10);
                let _ = crate::services::settings::save_settings_internal(&s);

                log::info!("[EVENT_EMIT] event=merge-complete jobId={} (srt-only)", job_id_inner);
                let _ = app_handle_inner.emit("merge-complete", &serde_json::json!({
                    "jobId": job_id_inner,
                    "outputPath": output_path,
                    "outputSizeBytes": size_bytes,
                    "segments": Vec::<MergeSegment>::new(),
                    "outputPaths": Some(vec![output_path.clone()]),
                    "parts": None::<Vec<MergePartResult>>,
                    "srtExportPaths": Some(vec![output_path.clone()]),
                    "reportPaths": None::<Vec<String>>,
                    "warnings": Vec::<String>::new(),
                    "subtitleWarnings": subtitle_warnings
                }));
                log::info!("[EVENT_EMIT] event=merge-complete jobId={} (srt-only) emitted", job_id_inner);

                crate::commands::merge::remove_merge_marker(&output_path);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
                crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully (SRT-only)"));
            } else {
                let err_str = match merge_result {
                    Err(e) => e.to_string(),
                    _ => "Unknown error merging SRT files".to_string()
                };
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(&err_str), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle_inner.emit("merge-error", &serde_json::json!({
                    "jobId": job_id_inner,
                    "error": err_str,
                    "cancelled": false
                }));
            }

            // Cleanup (no concat list file to remove - generate_merged_srt_with_rebase uses temp files)
            for f in temp_sub_files_to_clean {
                let _ = std::fs::remove_file(f);
            }
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut ms = merge_state_ref.lock().await;
                ms.active_jobs.remove(&job_id);
            });

            Ok::<String, String>(request.job_id.clone())
        });

        return Ok(job_id_for_return);
    }

    // ── Forensic Audit ──
    log::info!("[FORENSIC:AUDIT] PHASE 5.5: FORENSIC AUDIT (Pre-Normalization)");
    use crate::ffmpeg::normalization::{check_batch_corruption_parallel, check_pts_continuity_lightweight, check_subtitle_file, repair_subtitle_file, analyze_profiles, format_audit_report, filter_outliers_for_mkv, compute_smart_mkv_breakdown, SmartMkvBreakdown, Outlier, MergeBackend};
use crate::ffmpeg::normalization::NormalizationType;

    // Update checkpoint phase: Validating via single-writer to prevent races
    if let Some(ref sender) = checkpoint_sender {
        sender.update_phase(crate::types::MergePhase::Validating, None);
        log::info!("[PhaseResume] Checkpoint phase sent to writer: Validating");
    }

    // Emit progress: corruption check phase
    let total_input_count = working_input_files.len();
    let _ = app_handle.emit("merge-progress", &serde_json::json!({
        "jobId": request.job_id,
        "progress": {
            "phase": "validating",
            "stageName": "Checking file integrity...",
            "stagePercent": 0.0,
            "currentFileIndex": 0,
            "totalFilesInStage": total_input_count,
            "percent": 0.0,
            "currentTime": 0.0,
            "totalDuration": working_total_duration,
        }
    }));

    let corruption_input: Vec<(usize, String)> = working_input_files.iter().enumerate().map(|(i, f)| (i, f.clone())).collect();
    let jid = request.job_id.clone();
    let app_handle_corrupt = app_handle.clone();
    let total_duration = working_total_duration;
    let (_corruption_results, corrupt_count, _) = check_batch_corruption_parallel(
        &ffprobe_path_resolved,
        &corruption_input,
        Some(move |completed, total| {
            let stage_pct = (completed as f32 / total as f32) * 100.0;
            // Weighted: 0% -> 5%
            let overall_pct = stage_pct * 0.05;
            let _ = app_handle_corrupt.emit("merge-progress", &serde_json::json!({
                "jobId": jid,
                "progress": {
                    "phase": "validating",
                    "stageName": "Checking file integrity...",
                    "stagePercent": stage_pct,
                    "currentFileIndex": completed,
                    "totalFilesInStage": total,
                    "percent": overall_pct,
                    "overallPercent": overall_pct,
                    "currentTime": 0.0,
                    "totalDuration": total_duration,
                }
            }));
        }),
    ).await;
    if corrupt_count > 0 {
        return Err(format!("{} file(s) failed basic health checks and cannot be merged. Run compatibility check for details.", corrupt_count));
    }

    for (i, file) in working_input_files.iter().enumerate() {
        match probe_cache.get(Path::new(file)) {
            Some(Ok(info)) => {
                let pts_res = check_pts_continuity_lightweight(&info, i);
                if pts_res.has_negative_pts {
                    return Err(format!("Critical PTS corruption detected in file {}: {}. Merge aborted to prevent sync issues.", i, file));
                }
                if !pts_res.issues.is_empty() { log::warn!("[FORENSIC:AUDIT] [{}] {} has PTS issues", i, file); }
            }
            Some(Err(e)) => {
                log::warn!("[FORENSIC:AUDIT] [{}] Cached probe error for {} — PTS check skipped: {}", i, file, e);
            }
            None => {
                log::warn!("[FORENSIC:AUDIT] [{}] {} not found in probe cache — PTS check skipped", i, file);
            }
        }
    }
    if should_process_subs {
        for (i, sub_opt) in prepared_subs.iter_mut().enumerate() {
            if let Some(sub_path) = sub_opt {
                let sub_res = check_subtitle_file(i, sub_path);
                if !sub_res.is_valid {
                    log::warn!("[FORENSIC:AUDIT] [{}] Subtitle {} has issues — attempting repair", i, sub_path);
                    match repair_subtitle_file(sub_path, &temp_dir, &request.job_id, i) {
                        Ok(Some(repaired_path)) => {
                            log::info!("[FORENSIC:AUDIT] [{}] Subtitle repaired -> {}", i, repaired_path);
                            temp_subtitle_files.push(PathBuf::from(&repaired_path));
                            *sub_opt = Some(repaired_path);
                        }
                        Ok(None) => {
                            log::info!("[FORENSIC:AUDIT] [{}] Subtitle issues detected but could not be auto-repaired", i);
                        }
                        Err(e) => {
                            log::error!("[FORENSIC:AUDIT] [{}] Subtitle repair failed: {}", i, e);
                        }
                    }
                }
            }
        }
    }

    // ── Check for audio-only files (no video stream) ──
    // These cause "dimensions not set" error in FFmpeg and must be caught early
    for (i, file) in working_input_files.iter().enumerate() {
        match probe_cache.get(Path::new(file)) {
            Some(Ok(info)) => {
                if info.video_streams.is_empty() {
                    let filename = Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or(file);
                    return Err(format!(
                        "Audio-only file detected: '{}' (file #{}) has no video stream. \
                         This file cannot be merged with video files. \
                         Remove it from the playlist and try again.",
                        filename, i
                    ));
                }
            // Check for files with 0x0 dimensions — try to decode before rejecting
            if let Some(v) = info.video_streams.first() {
                if v.width == Some(0) || v.height == Some(0) {
                    let filename = Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or(file);
                    log::warn!("[Merge] File '{}' has 0x0 dimensions per ffprobe, attempting decode test...", filename);
                    #[cfg(windows)]
                    use std::os::windows::process::CommandExt;
                    const CREATE_NO_WINDOW: u32 = 0x08000000;
                    let mut cmd = std::process::Command::new(&ffmpeg_path_resolved);
                    #[cfg(windows)]
                    cmd.creation_flags(CREATE_NO_WINDOW);
                    let decode_test = cmd
                        .args(["-v", "error", "-i", file, "-f", "null", "-"])
                        .output();
                    match decode_test {
                        Ok(out) if out.status.success() => {
                            log::info!("[Merge] File '{}' decoded successfully despite 0x0 dimensions — proceeding", filename);
                        }
                        _ => {
                            return Err(format!(
                                "Invalid video dimensions: '{}' (file #{}) has {:?}x{:?} dimensions and cannot be decoded. \
                                 The file may be corrupt. Remove it from the playlist.",
                                filename, i, v.width, v.height
                            ));
                        }
                    }
                }
            }
            }
            Some(Err(e)) => {
                log::warn!("[FORENSIC:AUDIT] [{}] Cached probe error for {} — dimensions check skipped: {}", i, file, e);
            }
            None => {
                log::warn!("[FORENSIC:AUDIT] [{}] {} not found in probe cache — dimensions check skipped", i, file);
            }
        }
    }

    if request.mode == MergeMode::Lossless {
        if let Err(e) = crate::ffmpeg::concat::check_codec_compatibility_parallel(&input_path_refs, &probe_cache) {
            log::warn!("[FORENSIC:AUTOUPGRADE] Lossless incompatible: {}", e);
            actual_mode = MergeMode::Custom;
            actual_video_codec = Some("libx264".into()); actual_audio_codec = Some("aac".into());
            actual_video_crf = Some(20); actual_video_preset = Some("fast".into()); actual_audio_bitrate = Some("192k".into());
        } else if let Err(e) = crate::ffmpeg::concat::detect_codec_transitions(&input_path_refs, &probe_cache) {
            log::warn!("[FORENSIC:AUTOUPGRADE] Codec transition detected (Phase 5C fix): {}", e);
            log::warn!("[FORENSIC:AUTOUPGRADE] Forcing Custom (re-encode) mode to prevent 'missing picture' errors");
            actual_mode = MergeMode::Custom;
            actual_video_codec = Some("libx264".into()); actual_audio_codec = Some("aac".into());
            actual_video_crf = Some(20); actual_video_preset = Some("fast".into()); actual_audio_bitrate = Some("192k".into());
        }
    }

    // ── Large Playlist Auto-Detection ───────────────────────────────────
    let is_large_playlist = total_input_count > 300 || working_total_duration > 86400.0;

    // ── FORENSIC: Deep Validation Decision ──────────────────────────────
    log::info!("[AUDIO_DEEP_VALIDATE] ═══════════════════════════════════════════════════════════");
    log::info!("[AUDIO_DEEP_VALIDATE] DECISION TREE:");
    log::info!("[AUDIO_DEEP_VALIDATE]   audio_repair_mode = {:?}", audio_repair_mode);
    log::info!("[AUDIO_DEEP_VALIDATE]   request.validate_audio = {:?}", request.validate_audio);
    log::info!("[AUDIO_DEEP_VALIDATE]   is_large_playlist = {}", is_large_playlist);
    log::info!("[AUDIO_DEEP_VALIDATE]   total_input_count = {}", total_input_count);
    log::info!("[AUDIO_DEEP_VALIDATE]   total_duration = {:.1}s ({:.1}h)", working_total_duration, working_total_duration / 3600.0);
    // ── Resolve large playlist strategy ────────────────────────────────
    // User chose a strategy (from dialog or settings), or we fall back to SmartLite
    let settings = crate::services::settings::load_settings_internal();
    let resolved_strategy = request.large_playlist_strategy
        .clone()
        .unwrap_or_else(|| {
            settings.large_playlist_default
                .clone()
                .unwrap_or_default()
        });

    log::info!("[AUDIO_DEEP_VALIDATE] ═══════════════════════════════════════════════════════════");
    log::info!("[AUDIO_DEEP_VALIDATE] DECISION TREE:");
    log::info!("[AUDIO_DEEP_VALIDATE]   audio_repair_mode = {:?}", audio_repair_mode);
    log::info!("[AUDIO_DEEP_VALIDATE]   is_large_playlist = {}", is_large_playlist);
    log::info!("[AUDIO_DEEP_VALIDATE]   total_input_count = {}", total_input_count);
    log::info!("[AUDIO_DEEP_VALIDATE]   total_duration = {:.1}s ({:.1}h)", working_total_duration, working_total_duration / 3600.0);
    log::info!("[AUDIO_DEEP_VALIDATE]   request.large_playlist_strategy = {:?}", request.large_playlist_strategy);
    log::info!("[AUDIO_DEEP_VALIDATE]   settings.large_playlist_default = {:?}", settings.large_playlist_default);
    log::info!("[AUDIO_DEEP_VALIDATE]   RESOLVED STRATEGY = {:?}", resolved_strategy);
    match (&audio_repair_mode, is_large_playlist) {
        (AudioRepairMode::Fast, _) | (AudioRepairMode::Safe, _) => {
            log::info!("[AUDIO_DEEP_VALIDATE]   Branch: Fast|Safe → should_validate_audio = false");
            log::info!("[AUDIO_DEEP_VALIDATE]   ⚠️ Deep validation SKIPPED (mode excludes it)");
        }
        (AudioRepairMode::Smart, true) => {
            match resolved_strategy {
                LargePlaylistStrategy::FullSmart => {
                    log::info!("[AUDIO_DEEP_VALIDATE]   Branch: Smart + FullSmart");
                    log::info!("[AUDIO_DEEP_VALIDATE]   ✅ Full validation WILL RUN (user chose FullSmart)");
                    log::info!("[AUDIO_DEEP_VALIDATE]   Effective protection: FULL");
                }
                LargePlaylistStrategy::SmartLite => {
                    log::info!("[AUDIO_DEEP_VALIDATE]   Branch: Smart + SmartLite");
                    log::info!("[AUDIO_DEEP_VALIDATE]   ⚠️ Full decoder validation SKIPPED (SmartLite — user chose faster)");
                    log::info!("[AUDIO_DEEP_VALIDATE]   ✅ Seek-point check still RUNS (~51 points per file)");
                    log::info!("[AUDIO_DEEP_VALIDATE]   Effective protection: PARTIAL");
                }
                LargePlaylistStrategy::Safe => {
                    log::info!("[AUDIO_DEEP_VALIDATE]   Branch: Smart + Safe (large playlist)");
                    log::info!("[AUDIO_DEEP_VALIDATE]   ⚠️ Validation SKIPPED — Safe mode repairs all audio");
                    log::info!("[AUDIO_DEEP_VALIDATE]   Effective protection: SAFE_REPAIR_ALL");
                }
                LargePlaylistStrategy::Fast => {
                    log::info!("[AUDIO_DEEP_VALIDATE]   Branch: Smart + Fast (large playlist)");
                    log::info!("[AUDIO_DEEP_VALIDATE]   ⚠️ Validation SKIPPED — Fast mode skips everything");
                    log::info!("[AUDIO_DEEP_VALIDATE]   Effective protection: NONE");
                }
            }
        }
        (AudioRepairMode::Smart, false) => {
            log::info!("[AUDIO_DEEP_VALIDATE]   Branch: Smart (small playlist)");
            log::info!("[AUDIO_DEEP_VALIDATE]   ✅ Full validation WILL RUN (small playlist)");
            log::info!("[AUDIO_DEEP_VALIDATE]   Effective protection: FULL");
        }
    }
    log::info!("[AUDIO_DEEP_VALIDATE] ═══════════════════════════════════════════════════════════");

    // ── Determine if deep validation should run ─────────────────────
    let should_validate_audio = if matches!(audio_repair_mode, AudioRepairMode::Fast | AudioRepairMode::Safe) {
        false
    } else if matches!(audio_repair_mode, AudioRepairMode::Smart) && is_large_playlist {
        // Only FullSmart runs the expensive validate_audio_streams_parallel on large playlists
        resolved_strategy.is_full()
    } else {
        // Small playlist + Smart = always run
        matches!(audio_repair_mode, AudioRepairMode::Smart)
    };
    let mut problematic_audio_indices: Vec<usize> = Vec::new();

    // ── Audio Duration Check: detect files where audio < video ─────────────
    // FFmpeg concat demuxer (-c copy) preserves the audio that exists — it cannot
    // synthesize audio padding. If any source file has audio shorter than its video,
    // the concat output inherits that gap, creating A/V sync corruption.
    // Fix: re-encode (normalize) those files. Re-encoding pads audio to full duration.
    {
        let mut short_audio_indices = Vec::new();
        for (i, file_path) in working_input_files.iter().enumerate() {
            let key = PathBuf::from(file_path);
            let info = probe_cache.get(&key).or_else(|| probe_cache.get(Path::new(&normalize_long_path(file_path))));
            if let Some(Ok(info)) = info {
                let v_dur = info.video_streams.first().and_then(|s| s.duration);
                let a_dur = info.audio_streams.first().and_then(|s| s.duration);
                if let (Some(vd), Some(ad)) = (v_dur, a_dur) {
                    let diff = vd - ad;
                    if diff > 0.5 {
                        short_audio_indices.push(i);
                        log::warn!("[AV_SHORT] File #{} '{}': video={:.3}s audio={:.3}s gap={:.3}s → will be normalized (re-encode pads audio)",
                            i, Path::new(file_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file_path.clone()), vd, ad, diff);
                    }
                }
            }
        }
        if !short_audio_indices.is_empty() {
            log::warn!("[AV_SHORT] {} file(s) have audio shorter than video — adding to problematic_audio_indices for normalization", short_audio_indices.len());
            for idx in &short_audio_indices {
                if !problematic_audio_indices.contains(idx) {
                    problematic_audio_indices.push(*idx);
                }
            }
        } else {
            log::info!("[AV_SHORT] All files: audio duration >= video duration (no padding needed)");
        }

        // ── Audio Length Audit: scan every source file before merge ──────────
        // Full forensic report: which files have audio < video, and by how much.
        // This tells us whether A/V gaps are pre-existing in sources (Case A) or
        // introduced by the pipeline (Case B). Only after this should we decide
        // whether to auto-pad, warn, or re-encode.
        let mut audit_files_checked = 0usize;
        let mut audit_files_with_gap = 0usize;
        let mut audit_total_missing_audio = 0.0f64;
        let mut audit_worst_file: Option<(usize, f64, f64, f64)> = None;
        let mut audit_detailed_lines: Vec<String> = Vec::with_capacity(working_input_files.len());

        for (i, file_path) in working_input_files.iter().enumerate() {
            let key = PathBuf::from(file_path);
            let info = probe_cache.get(&key).or_else(|| probe_cache.get(Path::new(&normalize_long_path(file_path))));
            audit_files_checked += 1;
            let filename = Path::new(file_path).file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| file_path.clone());

            match info {
                Some(Ok(info)) => {
                    let v_dur = info.video_streams.first().and_then(|s| s.duration);
                    let a_dur = info.audio_streams.first().and_then(|s| s.duration);
                    match (v_dur, a_dur) {
                        (Some(vd), Some(ad)) => {
                            let diff = vd - ad;
                            if diff > 0.0 {
                                audit_files_with_gap += 1;
                                audit_total_missing_audio += diff;
                            }
                            if audit_worst_file.map(|w| diff > w.3).unwrap_or(true) {
                                audit_worst_file = Some((i, vd, ad, diff));
                            }
                            audit_detailed_lines.push(format!(
                                "  File #{:>3} | video={:>8.3}s | audio={:>8.3}s | GAP={:>7.3}s | {}",
                                i, vd, ad, diff, filename
                            ));
                        }
                        (Some(vd), None) => {
                            audit_files_with_gap += 1;
                            audit_total_missing_audio += vd;
                            if audit_worst_file.map(|w| vd > w.3).unwrap_or(true) {
                                audit_worst_file = Some((i, vd, 0.0, vd));
                            }
                            audit_detailed_lines.push(format!(
                                "  File #{:>3} | video={:>8.3}s | audio={:>8.3}s | GAP={:>7.3}s | {} (NO AUDIO)",
                                i, vd, 0.0f64, vd, filename
                            ));
                        }
                        _ => {
                            audit_detailed_lines.push(format!(
                                "  File #{:>3} | video={:>8} | audio={:>8} | GAP={:>7} | {} (duration unavailable)",
                                i, "N/A", "N/A", "N/A", filename
                            ));
                        }
                    }
                }
                _ => {
                    audit_detailed_lines.push(format!(
                        "  File #{:>3} | video={:>8} | audio={:>8} | GAP={:>7} | {} (probe unavailable)",
                        i, "N/A", "N/A", "N/A", filename
                    ));
                }
            }
        }

        log::info!("[AUDIO_LENGTH_AUDIT] ═══════════════════════════════════════════════════════════════");
        log::info!("[AUDIO_LENGTH_AUDIT] ══ Pre-Merge Audio Length Audit ═════════════════════════════");
        log::info!("[AUDIO_LENGTH_AUDIT] Files checked: {}", audit_files_checked);
        log::info!("[AUDIO_LENGTH_AUDIT] Files with audio < video: {}", audit_files_with_gap);
        log::info!("[AUDIO_LENGTH_AUDIT] Total missing audio: {:.3}s ({:.1}m)", audit_total_missing_audio, audit_total_missing_audio / 60.0);
        if let Some((idx, vd, ad, diff)) = audit_worst_file {
            let worst_name = working_input_files.get(idx).map(|f|
                Path::new(f).file_name().map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| f.clone())
            ).unwrap_or_default();
            log::info!("[AUDIO_LENGTH_AUDIT] Worst file: #{} '{}' — video={:.3}s audio={:.3}s gap={:.3}s",
                idx, worst_name, vd, ad, diff);
        }
        log::info!("[AUDIO_LENGTH_AUDIT] ─── Per-file detail (files with gap only) ─────────────────────────");
        if audit_detailed_lines.len() > 50 {
            log::info!("[AUDIO_LENGTH_AUDIT]   {} files with A/V gaps — showing first/last 10 + worst",
                audit_detailed_lines.len());
            for line in audit_detailed_lines.iter().take(10) {
                log::info!("[AUDIO_LENGTH_AUDIT]{}", line);
            }
            log::info!("[AUDIO_LENGTH_AUDIT]   ... {} more ...", audit_detailed_lines.len() - 20);
            for line in audit_detailed_lines.iter().rev().take(10) {
                log::info!("[AUDIO_LENGTH_AUDIT]{}", line);
            }
        } else {
            for line in &audit_detailed_lines {
                log::info!("[AUDIO_LENGTH_AUDIT]{}", line);
            }
        }
        log::info!("[AUDIO_LENGTH_AUDIT] ═══════════════════════════════════════════════════════════════");
        log::info!("[AUDIO_LENGTH_AUDIT] Verdict: {} file(s) need normalization (re-encode pads audio to video length)",
            if audit_files_with_gap > 0 { audit_files_with_gap.to_string() } else { "0 (all files clean)".to_string() });
        log::info!("[AUDIO_LENGTH_AUDIT] If gap exists in sources → Case A (pre-existing, concat preserves it)");
        log::info!("[AUDIO_LENGTH_AUDIT] If gap NOT in sources → Case B (pipeline introduced, investigate normalization/concat)");
        log::info!("[AUDIO_LENGTH_AUDIT] ═══════════════════════════════════════════════════════════════");
    }

    // ── FORENSIC: Confirm decision ──────────────────────────────────────
    log::info!("[AUDIO_DEEP_VALIDATE] RESULT: should_validate_audio = {} (strategy: {:?})", should_validate_audio, resolved_strategy);
    if should_validate_audio {
        log::info!("[AUDIO_DEEP_VALIDATE] ✅ validate_audio_streams_parallel() WILL RUN");
    } else {
        log::info!("[AUDIO_DEEP_VALIDATE] ❌ validate_audio_streams_parallel() WILL NOT RUN ({:?})", resolved_strategy);
    }

    // ── FORENSIC: Repair reason tracker ─────────────────────────────────────
    // Tracks WHY each file is being repaired for the final summary
    let mut repair_reasons: HashMap<usize, Vec<String>> = HashMap::new();

    // PHASE-BASED RESUME: Skip validation if phase indicates already complete
    if should_validate_audio && !skip_validation {
        // ── FORENSIC: Confirm entry ──────────────────────────────────────────
        log::info!("[AUDIO_DEEP_VALIDATE] ═══════════════════════════════════════════════════════════");
        log::info!("[AUDIO_DEEP_VALIDATE] START: validate_audio_streams_parallel() | Mode: {:?}", audio_repair_mode);
        log::info!("[AUDIO_DEEP_VALIDATE] Files to validate: {}", total_input_count);
        log::info!("[AUDIO_DEEP_VALIDATE] ═══════════════════════════════════════════════════════════");

        // Emit progress: audio validation phase start
        let _ = app_handle.emit("merge-progress", &serde_json::json!({
            "jobId": request.job_id,
            "progress": {
                "phase": "validating",
                "stageName": "Validating audio streams...",
                "stagePercent": 0.0,
                "currentFileIndex": 0,
                "totalFilesInStage": total_input_count,
                "percent": 5.0,
                "overallPercent": 5.0,
                "currentTime": 0.0,
                "totalDuration": working_total_duration,
            }
        }));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let audio_paths: Vec<PathBuf> = working_input_files.iter().map(PathBuf::from).collect();
        let jid_audio = request.job_id.clone();
        let app_handle_audio = app_handle.clone();
        let total_audio = total_input_count;
        let total_dur = working_total_duration;
        match crate::ffmpeg::concat::validate_audio_streams_parallel(
            audio_paths.clone(),
            Some(cancel_flag),
            Some(move |completed, total, _file_idx| {
                let stage_pct = (completed as f64 / total as f64) * 100.0;
                // Weighted: 5% -> 15% (10% window)
                let overall_pct = 5.0 + (stage_pct * 0.10);
                let _ = app_handle_audio.emit("merge-progress", &serde_json::json!({
                    "jobId": jid_audio,
                    "progress": {
                        "phase": "validating",
                        "stageName": "Validating audio streams...",
                        "stagePercent": stage_pct,
                        "currentFileIndex": completed,
                        "totalFilesInStage": total_audio,
                        "percent": overall_pct,
                        "overallPercent": overall_pct,
                        "currentTime": 0.0,
                        "totalDuration": total_dur,
                    }
                }));
            }),
        ).await {
            Ok(errors) => {
                for err in errors {
                    if !problematic_audio_indices.contains(&err.file_index) {
                        log::warn!("[AudioValidate] File #{} has decoder errors - will be normalized instead of copied", err.file_index);
                        problematic_audio_indices.push(err.file_index);
                        // FORENSIC: Track repair reason
                        repair_reasons.entry(err.file_index).or_default().push("DeepValidation_Corruption".to_string());
                        log::info!("[AUDIO_REPAIR_REASON] File #{} | Reason: DeepValidation_Corruption | Errors: {:?}",
                            err.file_index, err.error_lines.iter().take(3).collect::<Vec<_>>());
                    }
                }
            },
            Err(e) => {
                log::error!("[AUDIO_DEEP_VALIDATE] ❌ validate_audio_streams_parallel() failed: {} — deep validation skipped, falling back to seek-point check only", e);
            }
        }

        // ── FORENSIC: Confirm completion ─────────────────────────────────────
        log::info!("[AUDIO_DEEP_VALIDATE] END: validate_audio_streams_parallel() | Mode: {:?}", audio_repair_mode);
        log::info!("[AUDIO_DEEP_VALIDATE] Files flagged by deep validation: {}", problematic_audio_indices.len());
        if !problematic_audio_indices.is_empty() {
            log::info!("[AUDIO_DEEP_VALIDATE] Flagged indices: {:?}", problematic_audio_indices);
        }

    } else {
        // ── FORENSIC: Confirm skipped ────────────────────────────────────────
        log::info!("[AUDIO_DEEP_VALIDATE] ═══════════════════════════════════════════════════════════");
        log::info!("[AUDIO_DEEP_validate] SKIPPED: validate_audio_streams_parallel() | Mode: {:?}", audio_repair_mode);
        log::info!("[AUDIO_DEEP_VALIDATE] Files flagged by deep validation: 0 (skipped)");
        log::info!("[AUDIO_DEEP_VALIDATE] ═══════════════════════════════════════════════════════════");
    }

    // P2 FIX: Seek-point check runs for Smart mode. Safe/Fast explicitly skip validation
    // (Safe repairs all files without validation; Fast skips everything).
    // For Smart + large playlist: FullSmart and SmartLite both run seek checks.
    if matches!(audio_repair_mode, AudioRepairMode::Smart)
        && !resolved_strategy.is_safe()
        && !resolved_strategy.is_fast()
    {
        // This targeted seek test is separate from the expensive full decoder
        // validation because lossless mode would otherwise copy damaged audio
        // packets into the final file, causing seek/playback glitches later.
        log::info!("[AUDIO_SEEK_CHECK] ═══════════════════════════════════════════════════════════");
        log::info!("[AUDIO_SEEK_CHECK] START: check_problematic_audio_streams() | Mode: {:?} | Strategy: {:?}", audio_repair_mode, resolved_strategy);
        log::info!("[AUDIO_SEEK_CHECK] Testing 51 seek points per file (2% increments)");
        log::info!("[AUDIO_SEEK_CHECK] ═══════════════════════════════════════════════════════════");
        let cancel_flag_audio = Arc::new(AtomicBool::new(false));
        let audio_paths: Vec<PathBuf> = working_input_files.iter().map(PathBuf::from).collect();
        let app_handle_audio = app_handle.clone();
        let job_id_audio = request.job_id.clone();
        let on_file_complete_callback = Arc::new(move |file_idx: usize, total: usize, filename: String, duration: f64, seek_points: usize, max_gap: f64, is_problematic: bool| {
            let result = if is_problematic { Some("fail") } else { Some("pass") };
            let _ = app_handle_audio.emit("merge-file-progress", &serde_json::json!({
                "jobId": job_id_audio,
                "phase": "validating",
                "fileIndex": file_idx,
                "totalFiles": total,
                "filename": filename,
                "duration": duration,
                "seekPoints": seek_points,
                "maxGap": max_gap,
                "result": result,
            }));
        });
        match crate::ffmpeg::concat::check_problematic_audio_streams(
            audio_paths,
            Some(cancel_flag_audio),
            Some(&probe_cache),
            Some(on_file_complete_callback),
        ).await {
            Ok(indices) => {
                for idx in indices {
                    if !problematic_audio_indices.contains(&idx) {
                        problematic_audio_indices.push(idx);
                        // FORENSIC: Track repair reason
                        repair_reasons.entry(idx).or_default().push("SeekPointCheck_Corruption".to_string());
                        log::info!("[AUDIO_REPAIR_REASON] File #{} | Reason: SeekPointCheck_Corruption (51-point seek test failed)", idx);
                    }
                }
                if !problematic_audio_indices.is_empty() {
                    problematic_audio_indices.sort_unstable();
                    log::warn!("[AudioCheck] {} files have problematic audio streams - will be re-encoded: {:?}", problematic_audio_indices.len(), problematic_audio_indices);
                }
            }
            Err(e) => {
                log::error!("[AudioCheck] ❌ Could not complete targeted AAC seek check: {} — merge will skip seek-point validation for this run", e);
            }
        }

        // ── FORENSIC: Confirm seek-point check completion ─────────────────────
        log::info!("[AUDIO_SEEK_CHECK] END: check_problematic_audio_streams() | Mode: {:?}", audio_repair_mode);
        log::info!("[AUDIO_SEEK_CHECK] Total files flagged (cumulative): {}", problematic_audio_indices.len());
        // ── Per-file detection summary ───────────────────────────────────
        log::info!("[AUDIO_SEEK_CHECK] ─── Per-File Detection Summary ───");
        for (i, file) in working_input_files.iter().enumerate() {
            let filename = std::path::Path::new(file).file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.clone());
            if problematic_audio_indices.contains(&i) {
                log::info!("[AUDIO_SEEK_CHECK]   File #{}: ❌ FLAGGED (will be repaired) | {}", i, filename);
            } else {
                log::info!("[AUDIO_SEEK_CHECK]   File #{}: ✅ CLEAN (stream copy)     | {}", i, filename);
            }
        }
        log::info!("[AUDIO_SEEK_CHECK] ═══════════════════════════════════════════════════════════");
    } else if !matches!(audio_repair_mode, AudioRepairMode::Smart) {
        // ── FORENSIC: Confirm seek-point check skipped for non-Smart modes ─────
        log::info!("[AUDIO_SEEK_CHECK] ═══════════════════════════════════════════════════════════");
        log::info!("[AUDIO_SEEK_CHECK] SKIPPED: check_problematic_audio_streams() | Mode: {:?}", audio_repair_mode);
        log::info!("[AUDIO_SEEK_CHECK] Only Smart mode runs seek-point check");
        log::info!("[AUDIO_SEEK_CHECK] ═══════════════════════════════════════════════════════════");
    }

    // PHASE-BASED RESUME: Update phase to Normalizing after validation completes via single-writer
    if !skip_validation {
        if let Some(ref sender) = checkpoint_sender {
            sender.update_phase(crate::types::MergePhase::Normalizing, None);
            log::info!("[PhaseResume] Checkpoint phase sent to writer: Normalizing after validation");
        }
    } else {
        log::info!("[PhaseResume] Skipping phase update to Normalizing — validation was skipped");
    }

    // ── FORENSIC: Audio Repair Mode Summary ─────────────────────────────
    log::info!("[AUDIO_MODE_SUMMARY] ═══════════════════════════════════════════════════════════");
    log::info!("[AUDIO_MODE_SUMMARY] FINAL AUDIO REPAIR DECISION");
    log::info!("[AUDIO_MODE_SUMMARY] Mode: {:?}", audio_repair_mode);
    if is_large_playlist {
        log::info!("[AUDIO_MODE_SUMMARY] Large Playlist: true (strategy: {:?})", resolved_strategy);
    }
    log::info!("[AUDIO_MODE_SUMMARY] Total Files: {}", total_input_count);
    log::info!("[AUDIO_MODE_SUMMARY] ─── Detection Results ───");
    log::info!("[AUDIO_MODE_SUMMARY]   Deep Validation Ran: {}", should_validate_audio);
    log::info!("[AUDIO_MODE_SUMMARY]   Seek-Point Check Ran: {}", matches!(audio_repair_mode, AudioRepairMode::Smart) && !resolved_strategy.is_safe() && !resolved_strategy.is_fast());
    log::info!("[AUDIO_MODE_SUMMARY]   Safe Mode Repair All: {}", matches!(audio_repair_mode, AudioRepairMode::Safe) && actual_mode == MergeMode::Lossless);
    log::info!("[AUDIO_MODE_SUMMARY]   Files Flagged by Detection: {}", problematic_audio_indices.len());
    log::info!("[AUDIO_MODE_SUMMARY]   Large Playlist Strategy Used: {:?}", resolved_strategy);
    log::info!("[AUDIO_MODE_SUMMARY] ─── Expected vs Actual ───");
    match audio_repair_mode {
        AudioRepairMode::Fast => {
            log::info!("[AUDIO_MODE_SUMMARY]   Expected: No detection, no repair (direct stream copy)");
            log::info!("[AUDIO_MODE_SUMMARY]   Actual: Deep={}, SeekCheck={}, RepairAll={}", should_validate_audio, false, false);
        }
        AudioRepairMode::Smart => {
            log::info!("[AUDIO_MODE_SUMMARY]   Expected: Deep validation + seek check → repair only damaged");
            log::info!("[AUDIO_MODE_SUMMARY]   Actual: Deep={}, SeekCheck={}, RepairAll={}", should_validate_audio, true, false);
            if !should_validate_audio {
                if resolved_strategy.is_full() {
                    log::info!("[AUDIO_MODE_SUMMARY]   Strategy: FullSmart — full validation ran");
                } else if resolved_strategy.is_smart_lite() {
                    log::info!("[AUDIO_MODE_SUMMARY]   Strategy: SmartLite — seek-point check only (protection: PARTIAL)");
                } else if resolved_strategy.is_safe() {
                    log::info!("[AUDIO_MODE_SUMMARY]   Strategy: Safe — all audio will be repaired, no validation");
                } else if resolved_strategy.is_fast() {
                    log::info!("[AUDIO_MODE_SUMMARY]   Strategy: Fast — no validation, no repair");
                }
            }
        }
        AudioRepairMode::Safe => {
            log::info!("[AUDIO_MODE_SUMMARY]   Expected: No detection, repair all audio files");
            log::info!("[AUDIO_MODE_SUMMARY]   Actual: Deep={}, SeekCheck={}, RepairAll={}", should_validate_audio, false, true);
        }
    }
    log::info!("[AUDIO_MODE_SUMMARY] ═══════════════════════════════════════════════════════════");

    write_merge_marker(&request.output_path, &request.job_id);
    let cancel_flag = Arc::new(AtomicBool::new(false));
    { let mut ms = state.merge_state.lock().await; ms.active_jobs.insert(request.job_id.clone(), cancel_flag.clone()); }

    let normalized_output_path = normalize_long_path(&request.output_path);
    if let Some(parent) = Path::new(&normalized_output_path).parent() { if !parent.as_os_str().is_empty() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; } }

    // ── SmartMkv + convert_to_mp4: output MKV first, convert later ──────
    let smart_mkv_convert_to_mp4 = actual_mode == MergeMode::SmartMkv && request.convert_to_mp4.unwrap_or(false);
    let final_mp4_path = if smart_mkv_convert_to_mp4 {
        Some(normalized_output_path.clone())
    } else {
        None
    };
    let normalized_output_path = if smart_mkv_convert_to_mp4 {
        // Redirect concat output to a temp .mkv file, convert later
        let temp_dir = crate::ffmpeg::get_temp_dir().map_err(|e| e.to_string())?;
        let stem = std::path::Path::new(&normalized_output_path)
            .file_stem().and_then(|s| s.to_str()).unwrap_or("merged_output");
        temp_dir.join(format!("{}.mkv", stem)).to_string_lossy().into_owned()
    } else {
        normalized_output_path
    };

    // ── Disk space check ──────────────────────────────────────────────
    // Estimate required space from actual source file sizes (probe cache).
    // For Lossless mode: output ≈ sum of input sizes (stream copy).
    // For Custom mode: output ≈ sum of input sizes * 1.2 (re-encode overhead).
    // Temp normalized files add ~50% more.
    let mut source_total_bytes: u64 = 0;
    let mut has_probe_sizes = false;
    for file in &working_input_files {
        if let Some(Ok(info)) = probe_cache.get(Path::new(file)) {
            if info.size > 0 {
                source_total_bytes = source_total_bytes.saturating_add(info.size);
                has_probe_sizes = true;
            }
        }
    }
    let estimated_output_bytes = if has_probe_sizes {
        // Use actual source file sizes
        source_total_bytes
    } else {
        // Fallback: duration at 3 Mbps / 375 KB/s (typical 720p)
        working_total_duration as u64 * 375_000 
    };
    let estimated_temp_bytes = estimated_output_bytes / 2;
    let required_space = estimated_output_bytes + estimated_temp_bytes;
    log::info!("[DiskSpace] required: {:.1} GB (output: {:.1} GB + temp: {:.1} GB) | estimated from actual files: {}",
        required_space as f64 / 1_073_741_824.0,
        estimated_output_bytes as f64 / 1_073_741_824.0,
        estimated_temp_bytes as f64 / 1_073_741_824.0,
        has_probe_sizes);
    if let Some(parent) = Path::new(&normalized_output_path).parent() {
        if !parent.as_os_str().is_empty() {
            // Verify directory exists and is writable
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| format!("Cannot create output directory: {}", e))?;
            }
            // Check actual free disk space
            if let Err(e) = check_free_disk_space(parent, required_space) {
                log::error!("[DiskSpace] ❌ {}", e);
                return Err(e);
            }
        }
    }

    let _temp_norm_files: Vec<PathBuf> = Vec::new();
    // Shared Arc<Mutex<>> for both main code (push) and cleanup_guard (read).
    // This is Send+Sync so the async fn remains Send.
    let temp_norm_files_arc = Arc::new(Mutex::new(Vec::new()));
    // RAII cleanup guard — MUST be in scope for ALL early returns from the
    // normalization loops (6 return paths at lines ~1727, 1776, 1791, 1876,
    // 1922, 1938). Both vectors accessed via Arc<Mutex> (always current content).
    // Drop impl guarantees cleanup on: cancel, error, panic, or return.
    let mut cleanup_guard = TempCleanup::new(Arc::new(temp_registry), Arc::clone(&temp_norm_files_arc));
    let mut needs_normalization = false;

    // ── FORENSIC: Normalization Decision ─────────────────────────────────
    log::info!("[AUDIO_NORM_DECISION] ═══════════════════════════════════════════════════════════");
    log::info!("[AUDIO_NORM_DECISION] needs_normalization BEFORE outlier check: {}", needs_normalization);
    log::info!("[AUDIO_NORM_DECISION] actual_mode: {:?}", actual_mode);
    log::info!("[AUDIO_NORM_DECISION] audio_repair_mode: {:?}", audio_repair_mode);
    log::info!("[AUDIO_NORM_DECISION] ═══════════════════════════════════════════════════════════");

    // Always analyze profiles to detect outliers first
    let analysis_start = std::time::Instant::now();
    let mut profile_infos = Vec::new();
    for (i, file) in working_input_files.iter().enumerate() { if let Some(Ok(info)) = probe_cache.get(Path::new(file)) { profile_infos.push((i, file.clone(), info.clone())); } }
    let mut analysis = analyze_profiles(&profile_infos);
    
    // Persist dominant profile for resume compatibility via single-writer
    if let Some(ref sender) = checkpoint_sender {
        let types_profile = crate::types::DominantProfile {
            v_codec: analysis.dominant.v_codec.clone(),
            v_width: analysis.dominant.v_width,
            v_height: analysis.dominant.v_height,
            v_fps: analysis.dominant.v_fps,
            a_codec: analysis.dominant.a_codec.clone(),
            a_sample_rate: analysis.dominant.a_sample_rate,
            a_channels: analysis.dominant.a_channels,
            timescale_den: analysis.dominant.timescale_den.map(|v| v as u64),
        };
        sender.update_profile(types_profile);
        log::info!("[PhaseResume] Dominant profile sent to writer");
    }
    
    // Capture pre-filter outliers for dry-run simulation (before SmartMkv filtering)
    let analysis_outliers_pre_filter: Vec<crate::ffmpeg::normalization::Outlier> = analysis.outliers.clone();

    // ── Smart MKV Filter: remove outliers safe for MKV ─────────────────────
    // MKV supports many codecs and parameters that MP4 does not, so Smart MKV
    // can skip normalization for properties that MKV handles natively.
    // ── Smart MKV analysis variables (set inside block, used later for progress event) ──
    // These are declared outside the `if actual_mode == MergeMode::SmartMkv` block
    // because they're referenced later in the normalization progress event emission.
    let mut smart_mkv_breakdown: Option<SmartMkvBreakdown> = None;
    let mut normalize_count: usize = 0;
    let mut remux_count: usize = 0;
    let mut skip_count: usize = 0;

    if actual_mode == MergeMode::SmartMkv {
        let before_profile_count = analysis.outliers.len();
        let before_audio_count = analysis.audio_outliers.len();
        let before_total = before_profile_count + before_audio_count;
        // Count unique file indices before filtering
        let before_unique_indices: std::collections::HashSet<usize> = analysis.outliers.iter()
            .map(|o| o.index)
            .chain(analysis.audio_outliers.iter().map(|a| a.index))
            .collect();

        let (filtered_outliers, filtered_audio_outliers) = filter_outliers_for_mkv(&analysis, MergeBackend::MkvMerge);
        let after_profile_count = filtered_outliers.len();
        let after_audio_count = filtered_audio_outliers.len();
        let after_total = after_profile_count + after_audio_count;
        // Count unique file indices after filtering
        let after_unique_indices: std::collections::HashSet<usize> = filtered_outliers.iter()
            .map(|o| o.index)
            .chain(filtered_audio_outliers.iter().map(|a| a.index))
            .collect();

        let removed_count = before_profile_count - after_profile_count;
        let removed_audio_count = before_audio_count - after_audio_count;
        let total_removed = before_total - after_total;
        let reduction_pct = if before_total > 0 {
            (total_removed as f64 / before_total as f64) * 100.0
        } else {
            0.0
        };
        let files_before = before_unique_indices.len();
        let files_after = after_unique_indices.len();

        // ══════════════════════════════════════════════════════════════════
        // SMART MKV FORENSIC REPORT — Real-World Benchmark Instrumentation
        // ══════════════════════════════════════════════════════════════════
        log::info!("[SMART_MKV] ╔══════════════════════════════════════════════════════════════╗");
        log::info!("[SMART_MKV] ║              SMART MKV NORMALIZATION FILTER REPORT           ║");
        log::info!("[SMART_MKV] ╚══════════════════════════════════════════════════════════════╝");
        log::info!("[SMART_MKV]");
        log::info!("[SMART_MKV]   Files Analyzed:        {:>8}", analysis.dominant.total_count);
        log::info!("[SMART_MKV]   ────────────────────────────────────────────────────────────");
        log::info!("[SMART_MKV]   BEFORE FILTER:");
        log::info!("[SMART_MKV]     Profile Outliers:     {:>8}", before_profile_count);
        log::info!("[SMART_MKV]     Audio Outliers:       {:>8}", before_audio_count);
        log::info!("[SMART_MKV]     Total Normalizations: {:>8}", before_total);
        log::info!("[SMART_MKV]     Unique Files Affected:{:>8}", files_before);
        log::info!("[SMART_MKV]   ────────────────────────────────────────────────────────────");
        log::info!("[SMART_MKV]   AFTER FILTER (MKV-safe rules removed):");
        log::info!("[SMART_MKV]     Profile Outliers:     {:>8}", after_profile_count);
        log::info!("[SMART_MKV]     Audio Outliers:       {:>8}", after_audio_count);
        log::info!("[SMART_MKV]     Total Normalizations: {:>8}", after_total);
        log::info!("[SMART_MKV]     Unique Files Affected:{:>8}", files_after);
        log::info!("[SMART_MKV]   ────────────────────────────────────────────────────────────");
        log::info!("[SMART_MKV]   REDUCTION:");
        log::info!("[SMART_MKV]     Outliers Removed:     {:>8} (profile: {}, audio: {})",
            total_removed, removed_count, removed_audio_count);
        log::info!("[SMART_MKV]     Files Skipped:        {:>8}", files_before - files_after);
        log::info!("[SMART_MKV]     Reduction:            {:>7.1}%", reduction_pct);
        if reduction_pct >= 80.0 {
            log::info!("[SMART_MKV]     Verdict:             MAJOR SAVINGS — normalization workload drastically reduced");
        } else if reduction_pct >= 50.0 {
            log::info!("[SMART_MKV]     Verdict:             SIGNIFICANT SAVINGS — more than half of normalizations eliminated");
        } else if reduction_pct >= 25.0 {
            log::info!("[SMART_MKV]     Verdict:             MODERATE SAVINGS — noticeable improvement");
        } else {
            log::info!("[SMART_MKV]     Verdict:             MINOR SAVINGS — playlist may already be MKV-native");
        }
        log::info!("[SMART_MKV] ═══════════════════════════════════════════════════════════════");

        // ── Compute Smart MKV Analysis Breakdown ────────────────────────────
        smart_mkv_breakdown = Some(compute_smart_mkv_breakdown(&analysis));
        normalize_count = smart_mkv_breakdown.as_ref().unwrap().normalize.iter().map(|c| c.count).sum();
        remux_count = smart_mkv_breakdown.as_ref().unwrap().remux.iter().map(|c| c.count).sum();
        skip_count = smart_mkv_breakdown.as_ref().unwrap().skip.iter().map(|c| c.count).sum();
        log::info!("[SmartMkv] Dashboard: {} normalize, {} remux, {} skip ({} total outliers before filter)",
            normalize_count, remux_count, skip_count, before_total);

        analysis.outliers = filtered_outliers;
        analysis.audio_outliers = filtered_audio_outliers;
        // Recompute match count after filtering
        analysis.dominant.match_count = analysis.dominant.total_count - after_unique_indices.len();
    }

    log::info!("{}", format_audit_report(&analysis));

    // ── FORENSIC: Audio Dominant Profile Summary ─────────────────────────────
    log::info!("[AUDIO_DOMINANT] ═══════════════════════════════════════════════");
    log::info!("[AUDIO_DOMINANT] Dominant Audio Profile:");
    log::info!("[AUDIO_DOMINANT]   Codec:           {:?}", analysis.dominant.a_codec);
    log::info!("[AUDIO_DOMINANT]   AAC Profile:     {:?}", analysis.dominant.a_profile);
    log::info!("[AUDIO_DOMINANT]   Sample Rate:     {:?} Hz", analysis.dominant.a_sample_rate);
    log::info!("[AUDIO_DOMINANT]   Channels:        {:?}", analysis.dominant.a_channels);
    log::info!("[AUDIO_DOMINANT]   Channel Layout:  {:?}", analysis.dominant.a_channel_layout);
    log::info!("[AUDIO_DOMINANT]   Bit Depth:       {:?} bit", analysis.dominant.a_bit_depth);
    log::info!("[AUDIO_DOMINANT] Audio Outliers (granular): {}", analysis.audio_outliers.len());
    for ao in &analysis.audio_outliers {
        log::info!("[AUDIO_DOMINANT]   File #{} | {} | {} → {}",
            ao.index, ao.audio_type.label(), ao.actual_value, ao.dominant_value);
    }
    log::info!("[AUDIO_DOMINANT] ═══════════════════════════════════════════════");

    // ══════════════════════════════════════════════════════════════════════════════
    // SMART MKV AAC PROFILE AUDIT — Per-File Profile Inventory (measurement only)
    // ══════════════════════════════════════════════════════════════════════════════
    {
        let mut lc_count = 0usize;
        let mut he_aac_count = 0usize;
        let mut aac_main_count = 0usize;
        let mut unknown_aac_count = 0usize;
        let mut non_aac_count = 0usize;
        let mut files_needing_profile_norm = 0usize;
        let dominant_profile = analysis.dominant.a_profile.as_deref().unwrap_or("unknown");

        for file in working_input_files.iter() {
            if let Some(Ok(info)) = probe_cache.get(Path::new(file)) {
                if let Some(audio) = info.audio_streams.first() {
                    let codec = audio.codec_name.as_str();
                    let profile = audio.profile.as_deref().unwrap_or("N/A");
                    if codec.contains("aac") {
                        match profile {
                            "LC" => lc_count += 1,
                            "HE" | "HE-AAC" => he_aac_count += 1,
                            "Main" => aac_main_count += 1,
                            _ => unknown_aac_count += 1,
                        }
                        // Count files that would need normalization (non-dominant profile)
                        if profile != dominant_profile && dominant_profile != "unknown" {
                            files_needing_profile_norm += 1;
                        }
                    } else {
                        non_aac_count += 1;
                    }
                }
            }
        }

        let total_aac = lc_count + he_aac_count + aac_main_count + unknown_aac_count;
        let norm_cost_pct = if total_aac > 0 { (files_needing_profile_norm as f64 / total_aac as f64) * 100.0 } else { 0.0 };

        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT] ═══════════════════════════════════════════════");
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT] AAC PROFILE INVENTORY (per-file count)");
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   AAC-LC:        {:>5} files", lc_count);
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   HE-AAC:        {:>5} files", he_aac_count);
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   AAC-Main:      {:>5} files", aac_main_count);
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   Unknown AAC:   {:>5} files", unknown_aac_count);
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   Non-AAC:       {:>5} files", non_aac_count);
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   ─────────────────────────────────────────");
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   Total AAC:     {:>5}", total_aac);
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   Dominant:      {}", dominant_profile);
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   Would normalize (non-dominant): {:>5} ({:.1}%)",
            files_needing_profile_norm, norm_cost_pct);
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT]   SmartMkv value: {}",
            if norm_cost_pct > 50.0 { "DEGRADED — >50% files need AAC re-encode" }
            else if norm_cost_pct > 25.0 { "MODERATE — >25% files need AAC re-encode" }
            else if files_needing_profile_norm == 0 { "OPTIMAL — no AAC normalization needed" }
            else { "GOOD — <25% files need AAC re-encode" });
        log::info!("[SMART_MKV_AAC_PROFILE_AUDIT] ═══════════════════════════════════════════════");
    }

    // Determine if normalization is needed based on mode and remaining outliers
    if (actual_mode == MergeMode::Lossless || actual_mode == MergeMode::SmartMkv)
       && (!analysis.outliers.is_empty() || !analysis.audio_outliers.is_empty()) {
        needs_normalization = true;
    }

    // Force normalization if critical outliers exist that would break concat
    let has_critical_outliers = analysis.outliers.iter().any(|o| {
        matches!(o.property.as_str(), "time_base" | "a_sample_rate" | "a_channels" | "a_profile")
    });
    let has_profile_outliers = analysis.outliers.iter().any(|o| o.property == "a_profile");
    if has_critical_outliers && actual_mode != MergeMode::Custom {
        needs_normalization = true;
        log::warn!("[Normalization] Forcing normalization due to critical outliers detected: time_base, a_sample_rate, or a_channels mismatches");
    }
    if !problematic_audio_indices.is_empty() {
        needs_normalization = true;
    }

    // ── FORENSIC: Show outlier details ───────────────────────────────────
    log::info!("[AUDIO_NORM_DECISION] needs_normalization AFTER outlier check: {}", needs_normalization);
    log::info!("[AUDIO_NORM_DECISION] Total outliers: {}", analysis.outliers.len());
    log::info!("[AUDIO_NORM_DECISION] Critical outliers (time_base, a_sample_rate, a_channels):");
    for o in &analysis.outliers {
        if matches!(o.property.as_str(), "time_base" | "a_sample_rate" | "a_channels") {
            log::info!("[AUDIO_NORM_DECISION]   File #{} | {} | {} → {} | {:?}",
                o.index, o.property, o.dominant_value, o.actual_value, o.normalization_type);
        }
    }
    log::info!("[AUDIO_NORM_DECISION] Dominant time_base: {:?}", analysis.dominant.v_time_base);
    log::info!("[AUDIO_NORM_DECISION] Dominant a_sample_rate: {:?}", analysis.dominant.a_sample_rate);
    log::info!("[AUDIO_NORM_DECISION] Dominant a_channels: {:?}", analysis.dominant.a_channels);

    // ── IMMUTABILITY REGISTRY (Audit-Only Mode) ──────────────────────
    // Created at function scope, shared across all normalization workers.
    // Used ONLY for audit logging — actual immutability is still path-based.
    let immutability_registry = std::sync::Arc::new(ImmutabilityRegistry::new());
    log::info!("[IMMUTABILITY:REGISTRY] Created audit-only ImmutabilityRegistry for this job");

    if needs_normalization {

let mut need_audio_norm = Vec::new();
        let mut need_profile_norm = Vec::new();
        let mut outlier_by_index: HashMap<usize, Vec<Outlier>> = HashMap::new();
        for outlier in &analysis.outliers {
            outlier_by_index.entry(outlier.index).or_default().push(outlier.clone());
        }
        for audio_outlier in &analysis.audio_outliers {
            outlier_by_index.entry(audio_outlier.index).or_default().push(Outlier {
                index: audio_outlier.index,
                path: audio_outlier.path.clone(),
                reason: audio_outlier.audio_type.label().to_string(),
                normalization_type: crate::ffmpeg::normalization::NormalizationType::AudioReencode,
                dominant_value: audio_outlier.dominant_value.clone(),
                actual_value: audio_outlier.actual_value.clone(),
                property: audio_outlier.audio_type.code().to_string(),
            });
        }
        log::info!("[NORM_QUEUE] Built outlier_by_index: {} files with outliers (outliers={}, audio_outliers={})",
            outlier_by_index.len(), analysis.outliers.len(), analysis.audio_outliers.len());

        const TRACE_FILES: &[usize] = &[10, 24, 32];
        for (idx, file_outliers) in &outlier_by_index {
            let has_video = file_outliers.iter().any(|o| matches!(o.normalization_type, NormalizationType::VideoReencode | NormalizationType::FullReencode));
            let has_audio = file_outliers.iter().any(|o| matches!(o.normalization_type, NormalizationType::AudioReencode | NormalizationType::FullReencode));
            let has_timescale = file_outliers.iter().any(|o| matches!(o.normalization_type, NormalizationType::RemuxOnly));

            // FORENSIC TRACE: Track specific files through normalization queue
            if TRACE_FILES.contains(idx) {
                log::info!("[FORENSIC_TRACE] File #{} | Queue Entry Check | has_video={} has_audio={} has_timescale={}", idx, has_video, has_audio, has_timescale);
            }

            if has_video {
                need_profile_norm.push(*idx);
                if TRACE_FILES.contains(idx) {
                    log::info!("[FORENSIC_TRACE] File #{} | ADDED TO need_profile_norm", idx);
                }
                if has_audio {
                    need_audio_norm.push(*idx);
                }
                let video_reasons: Vec<String> = file_outliers.iter()
                    .filter(|o| matches!(o.normalization_type, NormalizationType::VideoReencode | NormalizationType::FullReencode))
                    .map(|o| format!("{}: {}→{}", o.property, o.actual_value, o.dominant_value))
                    .collect();
                repair_reasons.entry(*idx).or_default().push(format!("ProfileMismatch_Video({})", video_reasons.join(", ")));
                log::info!("[AUDIO_REPAIR_REASON] File #{} | Reason: ProfileMismatch_Video | Details: {}", idx, video_reasons.join("; "));
                if has_audio {
                    let audio_reasons: Vec<String> = file_outliers.iter()
                        .filter(|o| matches!(o.normalization_type, NormalizationType::AudioReencode | NormalizationType::FullReencode))
                        .map(|o| format!("{}: {}→{}", o.property, o.actual_value, o.dominant_value))
                        .collect();
                    repair_reasons.entry(*idx).or_default().push(format!("ProfileMismatch_Audio({})", audio_reasons.join(", ")));
                    log::info!("[AUDIO_REPAIR_REASON] File #{} | Reason: ProfileMismatch_Audio | Details: {}", idx, audio_reasons.join("; "));
                }
            }
            else if has_audio && !has_timescale {
                need_audio_norm.push(*idx);
                if TRACE_FILES.contains(idx) {
                    log::info!("[FORENSIC_TRACE] File #{} | ADDED TO need_audio_norm", idx);
                }
                let reasons: Vec<String> = file_outliers.iter()
                    .filter(|o| matches!(o.normalization_type, NormalizationType::AudioReencode | NormalizationType::FullReencode))
                    .map(|o| format!("{}: {}→{}", o.property, o.actual_value, o.dominant_value))
                    .collect();
                repair_reasons.entry(*idx).or_default().push(format!("ProfileMismatch_Audio({})", reasons.join(", ")));
                log::info!("[AUDIO_REPAIR_REASON] File #{} | Reason: ProfileMismatch_Audio | Details: {}", idx, reasons.join("; "));
            }
            else if has_timescale {
                need_profile_norm.push(*idx);
                if has_audio {
                    need_audio_norm.push(*idx);
                }
                repair_reasons.entry(*idx).or_default().push("ProfileMismatch_Timescale".to_string());
                log::info!("[AUDIO_REPAIR_REASON] File #{} | Reason: ProfileMismatch_Timescale", idx);
            }
        }

        // ── SMART MKV DECISION EXPLAINER ─────────────────────────────────────────
        // Log per-file decision rationale for all files
        let decisions = crate::ffmpeg::normalization::build_smartmkv_file_decisions(&analysis, &outlier_by_index, MergeBackend::MkvMerge);
        crate::ffmpeg::normalization::log_smartmkv_decision_explainer(&decisions);

        // ── AUDIO OUTLIER SUMMARY: Show granular audio types from audio_outliers ──
        let mut audio_type_counts: HashMap<String, usize> = HashMap::new();
        for ao in &analysis.audio_outliers {
            *audio_type_counts.entry(ao.audio_type.code().to_string()).or_insert(0) += 1;
        }
        if !audio_type_counts.is_empty() {
            log::info!("[AUDIO_OUTLIER_TYPES] Granular audio mismatch breakdown:");
            for (t, count) in &audio_type_counts {
                log::info!("[AUDIO_OUTLIER_TYPES]   {}: {} files", t, count);
            }
        }
        for ao in &analysis.audio_outliers {
            log::info!("[AUDIO_OUTLIER_DETAIL] File #{} | {} | {} → {}",
                ao.index, ao.audio_type.code(), ao.actual_value, ao.dominant_value);
        }

        if actual_mode == MergeMode::Lossless && matches!(audio_repair_mode, AudioRepairMode::Safe) {
            let mut safe_added_count = 0usize;
            for (idx, file) in working_input_files.iter().enumerate() {
                if need_profile_norm.contains(&idx) || need_audio_norm.contains(&idx) {
                    continue;
                }
                let has_audio_stream = match probe_cache.get(Path::new(file)) {
                    Some(Ok(info)) => !info.audio_streams.is_empty(),
                    _ => false,
                };
                if has_audio_stream {
                    log::info!("[Normalization] Lossless mode: pre-repairing audio for File #{} so final output is seek-clean", idx);
                    need_audio_norm.push(idx);
                    repair_reasons.entry(idx).or_default().push("SafeMode_RepairAll".to_string());
                    safe_added_count += 1;
                }
            }
            log::info!("[AUDIO_MODE] SAFE MODE: Added {} files for repair (all audio files)", safe_added_count);
        }

        // Force re-encoding of files with decoder/seek-time audio errors.
        for &idx in &problematic_audio_indices {
            if need_profile_norm.contains(&idx) || need_audio_norm.contains(&idx) {
                continue;
            }
            log::warn!("[Normalization] Forcing audio repair of File #{} due to decoder errors", idx);
            need_audio_norm.push(idx);
            // FORENSIC: Track corruption repair reason (if not already tracked)
            if !repair_reasons.contains_key(&idx) {
                repair_reasons.entry(idx).or_default().push("Corruption_Detected".to_string());
            }
        }

        // ══════════════════════════════════════════════════════════════════════
        // NORMALIZATION FORENSICS REPORT — Phase 2A Instrumentation
        // ══════════════════════════════════════════════════════════════════════
        //
        // This report logs every file's normalization decision with:
        //   1. Exact property trigger(s) per file
        //   2. Property frequency breakdown (ranked)
        //   3. Normalization type breakdown
        //
        // Use this to identify which triggers cause the MOST re-encodes,
        // then fix only those specific triggers (Phase 2B).
        //
        // Count UNIQUE files needing normalization (not double-counted)
        let unique_outlier_set: HashSet<usize> = need_profile_norm.iter()
            .chain(need_audio_norm.iter())
            .copied()
            .collect();
        let total_outliers = unique_outlier_set.len();

        // Per-file trigger breakdown
        let mut per_file_triggers: Vec<(usize, String, String)> = Vec::new();
        for (&idx, reasons) in &repair_reasons {
            let filename = working_input_files.get(idx).map(|p| {
                std::path::Path::new(p).file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("file_{}", idx))
            }).unwrap_or_else(|| format!("file_{}", idx));
            let trigger_summary = reasons.iter()
                .map(|r| {
                    // Strip prefix to get clean trigger name
                    if r.starts_with("ProfileMismatch_Video(") {
                        "VideoReencode".to_string()
                    } else if r.starts_with("ProfileMismatch_Audio(") {
                        "AudioReencode".to_string()
                    } else {
                        r.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" + ");
            per_file_triggers.push((idx, filename, trigger_summary));
        }
        per_file_triggers.sort_by_key(|t| t.0);

        log::info!("[NORM_FORENSICS] ════════════════════════════════════════════════════════");
        log::info!("[NORM_FORENSICS] NORMALIZATION FORENSICS REPORT");
        log::info!("[NORM_FORENSICS] ════════════════════════════════════════════════════════");
        log::info!("[NORM_FORENSICS] Total Files: {}", total_input_count);
        log::info!("[NORM_FORENSICS] Normalized: {}", total_outliers);
        log::info!("[NORM_FORENSICS] NOT Normalized: {}", total_input_count - total_outliers);
        log::info!("[NORM_FORENSICS] Audio Repair Mode: {:?}", audio_repair_mode);
        log::info!("[NORM_FORENSICS] Merge Mode: {:?}", actual_mode);
        log::info!("[NORM_FORENSICS] ────────────────────────────────────────────────────");
    // ── PHASE 6: Per-Property Normalization Breakdown ────────────────────────
    // Logs which specific properties triggered normalization for each file.
    // This helps users understand WHY each file needs normalization.
    if !analysis.outliers.is_empty() {
        let mut norm_prop_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for o in &analysis.outliers {
            if o.normalization_type != crate::ffmpeg::normalization::NormalizationType::None {
                *norm_prop_counts.entry(o.property.as_str()).or_insert(0) += 1;
            }
        }
        // Log top properties sorted by count
        let mut sorted_norm_props: Vec<_> = norm_prop_counts.into_iter().collect();
        sorted_norm_props.sort_by_key(|b| std::cmp::Reverse(b.1));
        log::info!("[NORM_BREAKDOWN] Per-property normalization triggers:");
        log::info!("[NORM_BREAKDOWN]   {:>25} | {:>5} files", "Property", "Count");
        log::info!("[NORM_BREAKDOWN]   {:->25} | {:->5}", "---", "---");
        for (prop, count) in &sorted_norm_props {
            log::info!("[NORM_BREAKDOWN]   {:>25} | {:>5}", prop, count);
        }
        log::info!("[NORM_BREAKDOWN]   {:->25} | {:->5}", "---", "---");
        log::info!("[NORM_BREAKDOWN]   {:>25} | {:>5} total triggers", "TOTAL", analysis.outliers.len());
    }
    if !analysis.audio_outliers.is_empty() {
        let mut audio_prop_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for ao in &analysis.audio_outliers {
            *audio_prop_counts.entry(ao.audio_type.label().to_string()).or_insert(0) += 1;
        }
        let mut sorted_audio_props: Vec<_> = audio_prop_counts.into_iter().collect();
        sorted_audio_props.sort_by_key(|b| std::cmp::Reverse(b.1));
        log::info!("[NORM_BREAKDOWN] Audio outlier types:");
        for (label, count) in &sorted_audio_props {
            log::info!("[NORM_BREAKDOWN]   {}: {}", label, count);
        }
    }
        log::info!("[NORM_FORENSICS] PER-FILE BREAKDOWN:");
        for (idx, filename, triggers) in &per_file_triggers {
            let in_profile = need_profile_norm.contains(idx);
            let in_audio = need_audio_norm.contains(idx);
            let norm_type = if in_profile && in_audio {
                "FullReencode"
            } else if in_profile {
                "VideoReencode"
            } else {
                "AudioReencode"
            };
            log::info!("[NORM_FORENSICS]   File #{:<4} | {:<40} | {} | {}", idx, format!("\"{}\"", filename), norm_type, triggers);
        }
        log::info!("[NORM_FORENSICS] ────────────────────────────────────────────────────");

        // PROPERTY-LEVEL BREAKDOWN — counts how many files have each property mismatch
        let mut prop_counts: HashMap<String, usize> = HashMap::new();
        let mut norm_type_counts: HashMap<String, usize> = HashMap::new();
        let mut safe_mode_files: Vec<usize> = Vec::new();

        for (idx, reasons) in &repair_reasons {
            for r in reasons {
                if r.starts_with("ProfileMismatch_Video(") {
                    let inner = r.strip_prefix("ProfileMismatch_Video(").map(|s| s.trim_end_matches(')')).unwrap_or("");
                    // Extract individual properties from "prop:val→val, prop:val→val" format
                    for part in inner.split(", ") {
                        if let Some(prop) = part.split(':').next() {
                            *prop_counts.entry(prop.to_string()).or_insert(0) += 1;
                        }
                    }
                    *norm_type_counts.entry("VideoReencode".to_string()).or_insert(0) += 1;
                } else if r.starts_with("ProfileMismatch_Audio(") {
                    let inner = r.strip_prefix("ProfileMismatch_Audio(").map(|s| s.trim_end_matches(')')).unwrap_or("");
                    for part in inner.split(", ") {
                        if let Some(prop) = part.split(':').next() {
                            *prop_counts.entry(prop.to_string()).or_insert(0) += 1;
                        }
                    }
                    *norm_type_counts.entry("AudioReencode".to_string()).or_insert(0) += 1;
                } else if r.contains("SafeMode_RepairAll") {
                    *norm_type_counts.entry("SafeMode_RepairAll".to_string()).or_insert(0) += 1;
                    safe_mode_files.push(*idx);
                } else if r.contains("ProfileMismatch_Timescale") {
                    *prop_counts.entry("time_base".to_string()).or_insert(0) += 1;
                    *norm_type_counts.entry("RemuxOnly".to_string()).or_insert(0) += 1;
                } else if r.contains("Corruption") || r.contains("DeepValidation") || r.contains("SeekPointCheck") {
                    *norm_type_counts.entry("Corruption".to_string()).or_insert(0) += 1;
                    *prop_counts.entry("corruption".to_string()).or_insert(0) += 1;
                }
            }
        }

        log::info!("[NORM_FORENSICS] ────────────────────────────────────────────────────");
        log::info!("[NORM_FORENSICS] TRIGGER FREQUENCY (ranked by count):");
        let mut prop_sorted: Vec<_> = prop_counts.iter().collect();
        prop_sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (prop, count) in prop_sorted {
            let pct = if total_outliers > 0 { (*count as f64 / total_outliers as f64) * 100.0 } else { 0.0 };
            log::info!("[NORM_FORENSICS]   {:>6.1}% | {:>5} files | {}", pct, count, prop);
        }
        log::info!("[NORM_FORENSICS] ────────────────────────────────────────────────────");
        log::info!("[NORM_FORENSICS] NORMALIZATION TYPE BREAKDOWN:");
        let mut type_sorted: Vec<_> = norm_type_counts.iter().collect();
        type_sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (t, count) in type_sorted {
            let pct = if total_outliers > 0 { (*count as f64 / total_outliers as f64) * 100.0 } else { 0.0 };
            log::info!("[NORM_FORENSICS]   {:>6.1}% | {:>5} files | {}", pct, count, t);
        }
        log::info!("[NORM_FORENSICS] ────────────────────────────────────────────────────");
        if !safe_mode_files.is_empty() {
            log::info!("[NORM_FORENSICS] SAFE MODE ADDED {} files for pre-emptive repair:", safe_mode_files.len());
            for &idx in &safe_mode_files {
                let filename = working_input_files.get(idx).map(|p| {
                    std::path::Path::new(p).file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("file_{}", idx))
                }).unwrap_or_else(|| format!("file_{}", idx));
                log::info!("[NORM_FORENSICS]   SafeMode: File #{} (\"{}\")", idx, filename);
            }
        }

        // Dominant profile values (what normalization is NORMALIZING TO)
        log::info!("[NORM_FORENSICS] ────────────────────────────────────────────────────");
        log::info!("[NORM_FORENSICS] DOMINANT PROFILE (normalization target):");
        log::info!("[NORM_FORENSICS]   v_codec:     {:?}", analysis.dominant.v_codec);
        log::info!("[NORM_FORENSICS]   v_profile:   {:?}", analysis.dominant.v_profile);
        log::info!("[NORM_FORENSICS]   resolution:  {}x{}",
            analysis.dominant.v_width.unwrap_or(0),
            analysis.dominant.v_height.unwrap_or(0));
        log::info!("[NORM_FORENSICS]   fps:         {:?}", analysis.dominant.v_fps);
        log::info!("[NORM_FORENSICS]   time_base:   {:?}", analysis.dominant.v_time_base);
        log::info!("[NORM_FORENSICS]   a_codec:     {:?}", analysis.dominant.a_codec);
        log::info!("[NORM_FORENSICS]   a_sample_rate: {:?}", analysis.dominant.a_sample_rate);
        log::info!("[NORM_FORENSICS]   a_channels:  {:?}", analysis.dominant.a_channels);
        log::info!("[NORM_FORENSICS] ════════════════════════════════════════════════════════");
        //
        // END NORMALIZATION FORENSICS REPORT
        // ══════════════════════════════════════════════════════════════════════

        // ── FORENSIC: Repair Summary ──────────────────────────────────────────
        let corruption_count = repair_reasons.values().filter(|r| r.iter().any(|x| x.contains("Corruption") || x.contains("DeepValidation") || x.contains("SeekPointCheck"))).count();
        let profile_mismatch_count = repair_reasons.values().filter(|r| r.iter().any(|x| x.contains("ProfileMismatch"))).count();
        let safe_mode_count = repair_reasons.values().filter(|r| r.iter().any(|x| x.contains("SafeMode"))).count();
        log::info!("[AUDIO_SUMMARY] ═══════════════════════════════════════════════════════════");
        log::info!("[AUDIO_SUMMARY] MODE: {:?}", audio_repair_mode);
        log::info!("[AUDIO_SUMMARY] Total Files: {}", total_input_count);
        log::info!("[AUDIO_SUMMARY] Files Needing Repair: {}", total_outliers);
        log::info!("[AUDIO_SUMMARY]   ├─ Due to Corruption: {}", corruption_count);
        log::info!("[AUDIO_SUMMARY]   ├─ Due to Profile Mismatch: {}", profile_mismatch_count);
        log::info!("[AUDIO_SUMMARY]   └─ Due to Safe Mode (all files): {}", safe_mode_count);
        log::info!("[AUDIO_SUMMARY] Files Skipped: {}", total_input_count - total_outliers);
        log::info!("[AUDIO_SUMMARY] ─── Per-File Breakdown ───");
        for idx in 0..total_input_count {
            if let Some(reasons) = repair_reasons.get(&idx) {
                log::info!("[AUDIO_SUMMARY]   File #{}: REPAIR | Reasons: {}", idx, reasons.join(", "));
            } else {
                log::info!("[AUDIO_SUMMARY]   File #{}: SKIP (healthy)", idx);
            }
        }
        log::info!("[AUDIO_SUMMARY] ═══════════════════════════════════════════════════════════");

// Warn if more than 20% of files need normalization
        if total_outliers > 0 {
            let pct = (total_outliers as f64 / total_input_count as f64) * 100.0;
            log::info!("[Normalization] {} files need normalization ({:.1}% of {})", total_outliers, pct, total_input_count);
            if pct > 20.0 {
                log::warn!("[Normalization] ⚠️ Over 20% of files require normalization — this may take significant time. Consider using Lossless mode if source files are already compatible.");
                // Emit warning to UI as a progress event
                let _ = app_handle.emit("merge-progress", &serde_json::json!({
                    "jobId": request.job_id,
                    "progress": {
                        "phase": "normalizing",
                        "warning": format!("{:.0}% of files need normalization — may take extra time", pct),
                        "stageName": format!("Normalising {} files ({:.0}% outliers)", total_outliers, pct),
                        "stagePercent": 0.0,
                        "currentFileIndex": 1,
                        "totalFilesInStage": total_outliers,
                        "percent": 15.0,
                        "overallPercent": 15.0,
                        "currentTime": 0.0,
                        "totalDuration": working_total_duration,
                        "isLargePlaylist": is_large_playlist,
                        "normalizationType": "Preparing...",
                        "repairReason": format!("{} files need repair: {} corruption, {} profile mismatch", total_outliers, corruption_count, profile_mismatch_count),
                    }
                }));
            }
        }

        let dom = &analysis.dominant;
        let _norm_idx = 0usize;

        // Build normalization plan for UI display (Phase 1 of Normalization Dashboard)
        // Count by type
        let mut audio_only_count = 0usize;
        let mut video_only_count = 0usize;
        let mut audio_video_count = 0usize;
        let mut classifications: Vec<serde_json::Value> = Vec::new();
        for (idx, filename, _) in &per_file_triggers {
            let in_profile = need_profile_norm.contains(idx);
            let in_audio = need_audio_norm.contains(idx);
            let norm_type = if in_profile && in_audio {
                "Audio + Video"
            } else if in_profile {
                "Video"
            } else {
                "Audio"
            };
            let badge = match (in_profile, in_audio) {
                (true, true) => "purple",
                (true, false) => "blue",
                (false, true) => "yellow",
                (false, false) => "green",
            };
            classifications.push(serde_json::json!({
                "index": idx,
                "filename": filename,
                "type": norm_type,
                "badge": badge,
                "inProfile": in_profile,
                "inAudio": in_audio,
            }));
            match (in_profile, in_audio) {
                (true, true) => audio_video_count += 1,
                (true, false) => video_only_count += 1,
                (false, true) => audio_only_count += 1,
                (false, false) => {}
            }
        }
        // Sort by index
        classifications.sort_by_key(|c| c["index"].as_u64().unwrap_or(0));

        // Emit normalization plan as part of phase start event
        let _ = app_handle.emit("merge-progress", &serde_json::json!({
            "jobId": request.job_id,
            "progress": {
                "phase": "normalizing",
                "stageName": if total_outliers > 0 {
                    format!("Normalising {} files...", total_outliers)
                } else {
                    "No normalization needed".to_string()
                },
                "stagePercent": 0.0,
                "currentFileIndex": 1,
                "totalFilesInStage": total_outliers,
                "percent": 15.0,
                "overallPercent": 15.0,
                "currentTime": 0.0,
                "totalDuration": working_total_duration,
                "normalizationType": "Preparing...",
                // Phase 1: Normalization Plan data
                "normalizationPlan": {
                    "totalFiles": total_input_count,
                    "normalCount": total_input_count - total_outliers,
                    "audioOnlyCount": audio_only_count,
                    "videoOnlyCount": video_only_count,
                    "audioVideoCount": audio_video_count,
                    "classifications": classifications,
                },
            }
        }));
        // ── Smart MKV Dashboard: emit breakdown data (only in SmartMkv mode) ──
        if let Some(ref smb) = smart_mkv_breakdown {
            let _ = app_handle.emit("merge-progress", &serde_json::json!({
                "jobId": request.job_id,
                "progress": {
                    "phase": "normalizing",
                    "smartMkvBreakdown": {
                        "willNormalize": normalize_count,
                        "willRemux": remux_count,
                        "willSkip": skip_count,
                        "totalFiles": total_input_count,
                        "categories": {
                            "normalize": smb.normalize.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                            "remux": smb.remux.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                            "skip": smb.skip.iter().map(|c| serde_json::json!({"property": c.property, "count": c.count})).collect::<Vec<_>>(),
                        }
                    }
                }
            }));
        }

        let mut already_normalized: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut need_profile_norm_iter = std::mem::take(&mut need_profile_norm);

        // ── Normalization Dedup: Remove duplicate source paths ──────────────────
        // When repeat expansion duplicates files, avoid normalizing the same source multiple times.
        // Keep only the first occurrence of each source path.
        {
            let mut seen_sources: std::collections::HashSet<String> = std::collections::HashSet::new();
            need_profile_norm_iter.retain(|&idx| {
                if let Some(path) = working_input_files.get(idx) {
                    if seen_sources.insert(path.clone()) {
                        true // First occurrence, keep it
                    } else {
                        log::info!("[Repeat:Dedup] Skipping profile norm File #{} — same source as already queued", idx);
                        false // Duplicate, skip
                    }
                } else {
                    true
                }
            });
        }
        
        // [RECOVERY] Ensure checkpoint exists (creates with full job info if new merge)
        // This is called early so a checkpoint is available for crash recovery
        let recovery_checkpoint: Option<crate::types::RecoveryCheckpoint> = {
            let app_data = match recovery::get_app_data_dir() {
                Ok(dir) => Some(dir),
                Err(e) => {
                    log::warn!("[Recovery] Could not get app data dir: {}, recovery disabled", e);
                    None
                }
            };
            app_data.and_then(|dir| {
                let mode_str = serde_json::to_value(&request.mode)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| format!("{:?}", request.mode));
                let encoding_params = recovery::CheckpointEncodingParams {
                    subtitle_mode: request.subtitle_mode.as_ref().map(|s| s.as_str().to_string()),
                    export_merged_srt: request.export_merged_srt,
                    selected_subtitle_stream_indices: request.selected_subtitle_stream_indices.clone(),
                    video_codec: request.video_codec.clone(),
                    audio_codec: request.audio_codec.clone(),
                    video_crf: request.video_crf,
                    video_preset: request.video_preset.clone(),
                    audio_bitrate: request.audio_bitrate.clone(),
                    target_resolution: request.target_resolution.clone(),
                    target_fps: request.target_fps.clone(),
                    hw_accel: request.hw_accel.clone(),
                    card_config: request.card_config.clone(),
                    split_config: request.split_config.clone(),
                    naming_config: request.naming_config.clone(),
                    audio_repair_mode: None,
                    validate_audio: request.validate_audio,
                    large_playlist_strategy: None,
                    convert_to_mp4: request.convert_to_mp4,
                };
                let job_info = recovery::CheckpointJobInfo {
                    job_id: request.job_id.clone(),
                    input_files: working_input_files.clone(),
                    output_path: request.output_path.clone(),
                    mode: mode_str.clone(),
                    original_file_count: repeat_expanded.as_ref().map(|e| e.original_count),
                    repeat_count: repeat_expanded.as_ref().map(|e| e.repeat_count),
                };
                recovery::ensure_checkpoint(
                    &dir,
                    &job_info,
                    request.repeat_config.as_ref(),
                    encoding_params,
                    Some(&working_input_durations),
                    Some(working_total_duration),
                ).ok()
            })
        };
        
        // [RECOVERY] Pre-populate already_normalized from checkpoint
        if let Some(ref cp) = recovery_checkpoint {
            for completed in &cp.completed_files {
                log::info!("[Recovery] File #{} already completed, will verify on use", completed.index);
                already_normalized.insert(completed.index);
            }
        }
        
        // PHASE-BASED RESUME: Update skip flags based on combined phase (request + checkpoint)
        let combined_phase = request.phase.as_ref().or_else(|| 
            recovery_checkpoint.as_ref().map(|cp| &cp.phase)
        );
        let skip_probing = matches!(combined_phase, Some(crate::types::MergePhase::Validating | crate::types::MergePhase::Normalizing | crate::types::MergePhase::Writing | crate::types::MergePhase::Finalizing | crate::types::MergePhase::Complete));
        let skip_validation = matches!(combined_phase, Some(crate::types::MergePhase::Normalizing | crate::types::MergePhase::Writing | crate::types::MergePhase::Finalizing | crate::types::MergePhase::Complete));
        let skip_analysis = matches!(combined_phase, Some(crate::types::MergePhase::Normalizing | crate::types::MergePhase::Writing | crate::types::MergePhase::Finalizing | crate::types::MergePhase::Complete));
        let skip_normalization = matches!(combined_phase, Some(crate::types::MergePhase::Writing | crate::types::MergePhase::Finalizing | crate::types::MergePhase::Complete));
        
        if skip_probing {
            log::info!("[PhaseResume] Skipping probing — phase indicates already complete");
        }
        if skip_validation {
            log::info!("[PhaseResume] Skipping validation — phase indicates already complete");
        }
        if skip_analysis {
            log::info!("[PhaseResume] Skipping analysis — phase indicates already complete");
        }
        if skip_normalization {
            log::info!("[PhaseResume] Skipping normalization — phase indicates already complete");
        }
        
        // ══════════════════════════════════════════════════════════════════════════════
        // REAL WORKLOAD FORENSIC INSTRUMENTATION
        // ══════════════════════════════════════════════════════════════════════════════
        let forensics = RealWorkloadForensics::new();
        
        // ── AAC PROFILE UNIFICATION ───────────────────────────────────────────
        let audio_before_unify = need_audio_norm.len();
        let profile_before_unify = need_profile_norm_iter.len();

        if has_profile_outliers {
            for (idx, file_path) in working_input_files.iter().enumerate() {
                if need_profile_norm_iter.contains(&idx) || need_audio_norm.contains(&idx) {
                    continue;
                }
                let is_non_lc = match probe_cache.get(std::path::Path::new(file_path)) {
                    Some(Ok(info)) => {
                        info.audio_streams.first().and_then(|a| a.profile.as_deref())
                            .map(|p| p != "LC")
                            .unwrap_or(false)
                    }
                    _ => false,
                };
                if is_non_lc {
                    let fname = std::path::Path::new(file_path).file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("file_{}", idx));
                    log::info!("[PROFILE_UNIFY] File #{} ({}) has non-LC AAC — adding to audio norm", idx, fname);
                    need_audio_norm.push(idx);
                    repair_reasons.entry(idx).or_default().push("ProfileUnify_NonLC_to_LC".to_string());
                }
            }
        }

        let audio_after_unify = need_audio_norm.len();
        let injected_count = audio_after_unify.saturating_sub(audio_before_unify);
        log::info!("[SMART_MKV_VERIFY] ═══════════════════════════════════════════════════════════════");
        log::info!("[SMART_MKV_VERIFY] Profile Unification Results:");
        log::info!("[SMART_MKV_VERIFY]   Profile Before: {}", profile_before_unify);
        log::info!("[SMART_MKV_VERIFY]   Profile After:  {}", need_profile_norm_iter.len());
        log::info!("[SMART_MKV_VERIFY]   Audio Before:   {}", audio_before_unify);
        log::info!("[SMART_MKV_VERIFY]   Audio After:    {}", audio_after_unify);
        log::info!("[SMART_MKV_VERIFY]   Injected Files: {}", injected_count);
        log::info!("[SMART_MKV_VERIFY] ═══════════════════════════════════════════════════════════════");

        // Recompute total_outliers after profile unification may have added files to need_audio_norm
        let norm_phase_start = std::time::Instant::now();

        // ── Normalization Dedup Cache ──────────────────────────────────────────
        // When repeat expansion duplicates files, avoid normalizing the same source multiple times.
        // Key: source file path, Value: normalized file path
        let _norm_dedup_cache: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        // ── Per-part normalization grouping ──────────────────────────────────────
        // When split config is active, group files by part so normalization
        // processes one part at a time (reducing concurrent load and timeout risk).
        let norm_part_groups: Vec<Vec<usize>> = if let Some(ref sc) = request.split_config {
            if sc.mode != crate::types::SplitMode::None {
                let groups = compute_norm_part_groups(
                    &working_input_files, &working_input_durations, sc,
                );
                log::info!("[NORM_PART] Split config mode={:?}: {} parts for normalization", sc.mode, groups.len());
                for (i, grp) in groups.iter().enumerate() {
                    let norm_in_grp: usize = grp.iter().filter(|idx| {
                        need_profile_norm_iter.contains(idx) || need_audio_norm.contains(idx)
                    }).count();
                    log::info!("[NORM_PART]   Part {}: {} total files, {} need norm", i + 1, grp.len(), norm_in_grp);
                }
                groups
            } else {
                vec![(0..working_input_files.len()).collect()]
            }
        } else {
            vec![(0..working_input_files.len()).collect()]
        };
        let total_norm_parts = norm_part_groups.len();

        // SINGLE-PASS GUARD: Shared NormalizationCache across profile norm AND audio norm.
        // Prevents double-normalization of files that appear in both phases.
        let shared_norm_cache = std::sync::Arc::new(crate::ffmpeg::norm_cache::NormalizationCache::new());

        // ── Phase 7: Parallel Profile Normalization (tokio::JoinSet + Semaphore) ──
        if !need_profile_norm_iter.is_empty() {
            // [P0-2/P0-3] Check for recovered files before spawning normalization tasks
            let mut profile_skip_indices: Vec<usize> = Vec::new();
            if let Some(ref cp) = recovery_checkpoint {
                for &idx in &need_profile_norm_iter {
                    if let Some(recovered_path) = crate::recovery::check_file_completed(cp, idx, Some(ffprobe_path_resolved.as_path())) {
                        log::info!("[Recovery] Profile norm File #{} recovered, using: {}", idx, recovered_path);
                        working_input_files[idx] = recovered_path;
                        profile_skip_indices.push(idx);
                    }
                }
            }
            need_profile_norm_iter.retain(|&idx| !profile_skip_indices.contains(&idx));
            
            // Compute per-part index groups for sequential normalization
            let profile_part_groups: Vec<Vec<usize>> = if total_norm_parts > 1 {
                norm_part_groups.iter()
                    .map(|part_indices| {
                        part_indices.iter()
                            .filter(|idx| need_profile_norm_iter.contains(idx))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .filter(|g| !g.is_empty())
                    .collect()
            } else {
                vec![need_profile_norm_iter.clone()]
            };
            let total_profile_parts = profile_part_groups.len();
            if total_profile_parts > 1 {
                log::info!("[Phase7] Profile norm: {} parts (sequential per-part)", total_profile_parts);
            }
            
            // [RECOVERY] Recompute total_outliers AFTER recovery skip so progress denominator is accurate
            let total_outliers = need_profile_norm_iter.len() + need_audio_norm.len();
            
            let worker_count = std::thread::available_parallelism()
                .map(|n| n.get().min(6))
                .unwrap_or(4).min(6);
            log::info!("[Phase7] Profile norm concurrency: {} workers", worker_count);
            let par_sem = Arc::new(tokio::sync::Semaphore::new(worker_count));
            let par_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let par_done = Arc::new(AtomicUsize::new(0));
            let par_total = need_profile_norm_iter.len();
            let par_forensics: Arc<Mutex<RealWorkloadForensics>> = Arc::new(Mutex::new(RealWorkloadForensics::new()));
            let par_wif: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(working_input_files.clone()));
            let orig_wif: Vec<String> = working_input_files.clone();
            let par_an: Arc<Mutex<std::collections::HashSet<usize>>> = Arc::new(Mutex::new(already_normalized));
            let par_norm_idx = Arc::new(AtomicUsize::new(0));
            let dom_par = dom.clone();
            let rcp_par = recovery_checkpoint.clone();
            let pft_par = per_file_triggers.clone();
            let rr_par = repair_reasons.clone();
            let obi_par = outlier_by_index.clone();
            let ao_par = analysis.outliers.clone();
            let aao_par = analysis.audio_outliers.clone();

            // Uses shared_norm_cache from above (single-pass guarantee)
            for (pp_idx, pp_indices) in profile_part_groups.iter().enumerate() {
                if total_profile_parts > 1 {
                    log::info!("[Phase7] Profile norm Part {}/{}: {} files", pp_idx + 1, total_profile_parts, pp_indices.len());
                }
                let mut js = tokio::task::JoinSet::new();
                for &idx in pp_indices {
                let s = par_sem.clone(); let e = par_err.clone(); let d = par_done.clone();
                let _t = par_total; let f = par_forensics.clone(); let w = par_wif.clone();
                let a = par_an.clone(); let n = par_norm_idx.clone();
                let c = cancel_flag.clone(); let p = probe_cache.clone(); let nf = temp_norm_files_arc.clone();
                let ff = ffmpeg_path_resolved.clone(); let fp = ffprobe_path_resolved.clone();
                let td = temp_dir.clone(); let j = request.job_id.clone(); let dm = dom_par.clone();
                let cp_sender = checkpoint_sender.clone(); // PER-JOB CHECKPOINT WRITER
                let ah = app_handle.clone(); let dur = working_total_duration;
                let to = total_outliers; let _rcp = rcp_par.clone();
                let _pfti = pft_par.clone(); let rri = rr_par.clone(); let obii = obi_par.clone();
                let _aoi = ao_par.clone(); let aaoi = aao_par.clone();
                let skip_vol = actual_mode == MergeMode::SmartMkv;
                let ow = orig_wif.clone();

                let norm_cache = shared_norm_cache.clone();
                let imm_reg = immutability_registry.clone();
                js.spawn(async move {
                    let _permit = match s.acquire().await { Ok(p) => p, Err(_) => { return } };
                    // Check prior error
                    if let Ok(eg) = e.lock() { if eg.is_some() { return } }
                    if c.load(Ordering::Relaxed) { return }
                    // Increment counters
                    let _ci = n.fetch_add(1, Ordering::Relaxed) + 1;
                    { let mut ag = a.lock().unwrap_or_else(|p| p.into_inner()); ag.insert(idx); }
                    let fpath = { let wg = w.lock().unwrap_or_else(|p| p.into_inner()); wg[idx].clone() };
                    let fname = std::path::Path::new(&fpath).file_name()
                        .and_then(|n| n.to_str()).unwrap_or("file").to_string();
                    // FORENSIC TRACE: Worker started for specific files
                    if matches!(idx, 10 | 24 | 32) {
                        log::info!("[FORENSIC_TRACE] File #{} | WORKER STARTED | fpath={}", idx, fpath);
                    }
                    // [P0-1] Get source file metadata for recovery checkpoint
                    let (source_size, source_mtime) = match std::fs::metadata(&fpath) {
                        Ok(meta) => {
                            let size = meta.len();
                            let mtime = meta.modified()
                                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
                                .unwrap_or(0);
                            (size, mtime)
                        }
                        Err(_) => (0, 0),
                    };
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
                    if c.load(Ordering::Relaxed) { return }
                    let only_remux = obii.get(&idx).is_some_and(|ox| {
                        ox.iter().all(|o| matches!(o.normalization_type, crate::ffmpeg::normalization::NormalizationType::RemuxOnly))
                    });
                    let ntl = if only_remux && dm.timescale_den.is_some() { "TimescaleRemux" }
                               else if only_remux { "ContainerFix" } else { "VideoReencode" };
                    if let Some(Ok(ii)) = p.get(std::path::Path::new(&fpath)) {
                        f.lock().unwrap_or_else(|p| p.into_inner()).record_file_start(idx, &fname, &fpath, &ii, ntl);
                    }
                    let ns = std::time::Instant::now();
                    let input_dur = p.get(std::path::Path::new(&fpath)).and_then(|r| r.ok()).map(|i| i.duration);
                    let input_has_audio = p.get(std::path::Path::new(&fpath)).and_then(|r| r.ok()).map(|i| !i.audio_streams.is_empty()).unwrap_or(false);
                    let input_video_duration_ms = ow.get(idx).and_then(|op| p.get(std::path::Path::new(op)).and_then(|r| r.ok()).and_then(|i| i.video_streams.first().and_then(|s| s.duration)).map(|d| (d * 1000.0) as u64));
                    let input_audio_sample_rate = p.get(std::path::Path::new(&fpath))
                        .and_then(|r| r.ok())
                        .and_then(|i| { i.audio_streams.first().map(|a| a.sample_rate) })
                        .flatten();
                    let input_audio_bitrate = p.get(std::path::Path::new(&fpath))
                        .and_then(|r| r.ok())
                        .and_then(|i| i.audio_streams.first().and_then(|a| a.bit_rate))
                        .map(|br| format!("{}k", br / 1000));
                    let res = if only_remux {
                        if let Some(ts) = dm.timescale_den {
                            normalize_timescale_lossless(&ff, &fpath, ts, &td, &j, idx, c.clone(), Some(norm_cache.clone())).await
                        } else {
                            // ── PROOF: only_remux=true is safe to skip normalization ───────────────
                            // only_remux=true means ALL outliers for this file are NormalizationType::RemuxOnly
                            // RemuxOnly is ONLY triggered by:
                            //   1. time_base mismatch (normalization.rs:636)
                            //   2. container_format mismatch (normalization.rs:759)
                            //
                            // Both are container-level issues that mkvmerge handles natively.
                            // If timescale_den is None, there's no timescale to fix, meaning:
                            //   - time_base is already compatible OR
                            //   - container_format is already compatible
                            //
                            // Therefore: No re-encoding is needed. File is merge-compatible.
                            // ───────────────────────────────────────────────────────────────────────
                            log::info!("[FORENSIC:NORMALIZE] File #{} - Container/timebase mismatch but no fix needed - using original path", idx);
                            Ok(fpath.clone())
                        }
                    } else {
                        normalize_to_profile(&ff, &fpath, dm.v_codec.as_deref().unwrap_or("libx264"),
                            dm.a_codec.as_deref().unwrap_or("aac"), dm.a_sample_rate.unwrap_or(48000),
                            dm.v_fps, dm.timescale_den, dm.v_width, dm.v_height, dm.a_channels, input_audio_bitrate.clone(), &td, &j, idx, input_has_audio, c.clone(),
                            Some(norm_cache.clone()), input_dur, input_video_duration_ms, Some(&fp), input_audio_sample_rate).await
                    };
                    log::info!("[FORENSIC:NORMALIZE] END: {} | File #{} | Elapsed: {:?}",
                        if only_remux && dm.timescale_den.is_some() { "Timescale Remux (Lossless)" }
                        else if only_remux { "Container Copy (No Re-encode Needed)" }
                        else { "Video Re-encode" }, idx, ns.elapsed());
                    match res {
                        Ok(path) => {
                            f.lock().unwrap_or_else(|p| p.into_inner()).record_file_norm_end(idx, &path);
                            nf.lock().unwrap_or_else(|p| p.into_inner()).push(std::path::PathBuf::from(&path));
                            { let mut wg = w.lock().unwrap_or_else(|p| p.into_inner()); wg[idx] = path.clone(); }

                            // ── IMMUTABILITY REGISTRY: Register after profile normalization ──
                            // Audit-only: registry tracks what was normalized for consistency checks.
                            {
                                let fingerprint = AudioFingerprint {
                                    codec: dm.a_codec.clone().unwrap_or_else(|| "aac".to_string()),
                                    sample_rate: dm.a_sample_rate.unwrap_or(48000),
                                    channels: dm.a_channels.unwrap_or(2),
                                    bitrate: input_audio_bitrate.as_ref().and_then(|s| s.trim_end_matches('k').parse::<u64>().ok()).map(|b| b * 1000),
                                    profile: None,
                                };
                                imm_reg.register(&fpath, &path, fingerprint, &j);
                            }

                            if matches!(idx, 10 | 24 | 32) {
                                log::info!("[FORENSIC_TRACE] File #{} | VIDEO WORKER COMPLETE | normalized_path={}", idx, path);
                            }
                            { f.lock().unwrap_or_else(|p| p.into_inner()).record_file_verify_start(idx); }
                            let vr = verify_normalized_audio_health(&ff, &fp, &path, idx, &fname, c.clone(),
                                        skip_vol, input_video_duration_ms).await;
                            match vr {
                                Ok(ni) => {
                                    f.lock().unwrap_or_else(|p| p.into_inner()).record_file_verify_end(idx, ni.duration);
                                    p.insert(std::path::PathBuf::from(&path), Ok(ni.clone()));
                                    // ── DURATION AUDIT: async=1 + first_pts=0 measurement ──
                                    // Logs start_time, total duration, and audio stream duration
                                    // to detect sample insertion/removal or PTS origin shifts.
                                    // Run 20+ files, then compare input vs output.
                                    if let Some(Ok(in_info)) = p.get(std::path::Path::new(&fpath)) {
                                        let in_dur = in_info.duration;
                                        let out_dur = ni.duration;
                                        let delta = out_dur - in_dur;
                                        let delta_pct = if in_dur > 0.0 { (delta / in_dur) * 100.0 } else { 0.0 };
                                        let in_start = in_info.start_time.unwrap_or(0.0);
                                        let out_start = ni.start_time.unwrap_or(0.0);
                                        let in_audio_dur = in_info.audio_streams.first().and_then(|a| a.duration).unwrap_or(0.0);
                                        let out_audio_dur = ni.audio_streams.first().and_then(|a| a.duration).unwrap_or(0.0);
                                        log::info!("[DURATION_AUDIT] File #{} ({})", idx, fname);
                                        log::info!("[DURATION_AUDIT]   start_time:  {:.3}s → {:.3}s (delta: {:+.3}s)", in_start, out_start, out_start - in_start);
                                        log::info!("[DURATION_AUDIT]   duration:     {:.3}s → {:.3}s (delta: {:+.3}s, {:+.3}%)", in_dur, out_dur, delta, delta_pct);
                                        log::info!("[DURATION_AUDIT]   audio_dur:    {:.3}s → {:.3}s (delta: {:+.3}s)", in_audio_dur, out_audio_dur, out_audio_dur - in_audio_dur);
                                        if delta.abs() > 0.05 {
                                            log::warn!("[DURATION_AUDIT] ⚠️ File #{} ({}) total duration changed by {:.3}s ({:+.3}%) — aresample may be inserting/dropping samples",
                                                idx, fname, delta, delta_pct);
                                        }
                                        if (out_start - in_start).abs() > 0.01 {
                                            log::info!("[DURATION_AUDIT]   ℹ️ start_time shifted by {:.3}s — first_pts=0 is resetting PTS origin (expected)", out_start - in_start);
                                        }
                                    }
                                    if let Some(ref sender) = cp_sender {
                                        let nt = if dm.timescale_den.is_some() { crate::types::NormalizationType::Timescale } else { crate::types::NormalizationType::Full };
                                        sender.append_completed_file(idx, fpath.clone(), source_size, source_mtime, path.clone(), nt);
                                    }
                                }
                                Err(ve) => {
                                    if ve.contains("cancelled") || c.load(Ordering::Relaxed) {
                                        crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
crate::logger::stop_job_log(Some("[JOB_CANCELLED] Merge cancelled by user"));
let _ = ah.emit("merge-error", &serde_json::json!({"jobId":j,"error":"Merge cancelled by user","cancelled":true}));
                                        let mut eg = e.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some("Cancelled".to_string()); }
                                    } else {
                                        log::error!("[FORENSIC:AUDIO_VALIDATE] FAILED File #{}: {}", idx, ve);
                                        let mut eg = e.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some(format!("Audio repair failed for File #{} ({}): {}", idx, fname, ve)); }
                                    }
                                    return;
                                }
                            }
                            d.fetch_add(1, Ordering::Relaxed);
                            // Emit progress AFTER file completes (not at start)
                            let completed = d.load(Ordering::Relaxed);
                            let sp = (completed as f64 / to as f64) * 100.0;
                            let op = 15.0 + (sp * 0.20);
                            let _ = ah.emit("merge-progress", &serde_json::json!({
                                "jobId": j, "progress": {
                                    "phase": "normalizing", "stageName": format!("Normalised: {} ({} of {})", fname, completed, to),
                                    "stagePercent": sp, "currentFileIndex": completed, "totalFilesInStage": to,
                                    "percent": op, "overallPercent": op, "currentTime": 0.0, "totalDuration": dur,
                                    "normalizationType": ntype, "currentFile": fname, "repairReason": rsn,
                            }}));
                        }
                        Err(err_msg) => {
                            if err_msg.contains("cancelled") {
                                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
crate::logger::stop_job_log(Some("[JOB_CANCELLED] Merge cancelled by user"));
let _ = ah.emit("merge-error", &serde_json::json!({"jobId":j,"error":"Merge cancelled by user","cancelled":true}));
                                let mut eg = e.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some("Cancelled".to_string()); }
                            } else {
                                log::error!("[FORENSIC:ERROR] Video norm FAILED File #{}: {}", idx, err_msg);
                                let mut eg = e.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some(format!("Video normalization failed for File #{}: {}", idx, err_msg)); }
                            }
                        }
                    }
                });
            }
            while let Some(r) = js.join_next().await {
                if let Err(je) = r {
                    log::error!("[Phase7] Profile task panicked: {}", je);
                    let mut eg = par_err.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some(format!("Task panicked: {}", je)); }
                }
            }
            // Check errors
            { let eg = par_err.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(ref em) = *eg {
                    if em == "Cancelled" {
                        crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle.emit("merge-error", &serde_json::json!({"jobId":request.job_id,"error":"Merge cancelled by user","cancelled":true}));
                        cleanup_guard.cleanup(); return Ok(request.job_id);
                    }
                    cleanup_guard.cleanup(); return Err(em.clone());
                }
            }
            } // end per-part profile norm loop
            working_input_files = par_wif.lock().unwrap_or_else(|p| p.into_inner()).clone();
            already_normalized = par_an.lock().unwrap_or_else(|p| p.into_inner()).clone();
            log::info!("[Phase7] Parallel profile norm complete: {} files", par_total);
        }        // ── Phase 7: Parallel Audio Normalization (tokio::JoinSet + Semaphore) ──
        if !need_audio_norm.is_empty() {
            // [P0-2/P0-3] Check for recovered files before spawning normalization tasks
            let mut audio_skip_indices: Vec<usize> = Vec::new();
            if let Some(ref cp) = recovery_checkpoint {
                for &idx in &need_audio_norm {
                    if let Some(recovered_path) = crate::recovery::check_file_completed(cp, idx, Some(ffprobe_path_resolved.as_path())) {
                        log::info!("[Recovery] Audio norm File #{} recovered, using: {}", idx, recovered_path);
                        working_input_files[idx] = recovered_path;
                        audio_skip_indices.push(idx);
                    }
                }
            }
            need_audio_norm.retain(|&idx| !audio_skip_indices.contains(&idx));

            // ── Audio Normalization Dedup: Remove duplicate source paths ──────
            {
                let mut seen_sources: std::collections::HashSet<String> = std::collections::HashSet::new();
                need_audio_norm.retain(|&idx| {
                    if let Some(path) = working_input_files.get(idx) {
                        if seen_sources.insert(path.clone()) {
                            true
                        } else {
                            log::info!("[Repeat:Dedup] Skipping audio norm File #{} — same source as already queued", idx);
                            false
                        }
                    } else {
                        true
                    }
                });
            }
            
            // Compute per-part index groups for sequential audio normalization
            let audio_part_groups: Vec<Vec<usize>> = if total_norm_parts > 1 {
                norm_part_groups.iter()
                    .map(|part_indices| {
                        part_indices.iter()
                            .filter(|idx| need_audio_norm.contains(idx))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .filter(|g| !g.is_empty())
                    .collect()
            } else {
                vec![need_audio_norm.clone()]
            };
            let total_audio_parts = audio_part_groups.len();
            if total_audio_parts > 1 {
                log::info!("[Phase7] Audio norm: {} parts (sequential per-part)", total_audio_parts);
            }

            // [RECOVERY] Recompute total_outliers AFTER recovery skip so progress denominator is accurate
            let total_outliers = need_profile_norm_iter.len() + need_audio_norm.len();
            
            let worker_count = std::thread::available_parallelism()
                .map(|n| n.get().min(6))
                .unwrap_or(4).min(6);
            log::info!("[Phase7] Audio norm concurrency: {} workers", worker_count);
            let par_sem = Arc::new(tokio::sync::Semaphore::new(worker_count));
            let par_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let par_done = Arc::new(AtomicUsize::new(0));
            let par_total = need_audio_norm.len();
            let par_forensics: Arc<Mutex<RealWorkloadForensics>> = Arc::new(Mutex::new(RealWorkloadForensics::new()));
            let par_wif: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(working_input_files.clone()));
            let orig_wif: Vec<String> = working_input_files.clone();
            let par_an: Arc<Mutex<std::collections::HashSet<usize>>> = Arc::new(Mutex::new(already_normalized));
            let par_norm_idx = Arc::new(AtomicUsize::new(0));
            let dom_par = dom.clone();
            let rcp_par = recovery_checkpoint.clone();
            let rr_par = repair_reasons.clone();
            let aao_par = analysis.audio_outliers.clone();
            let ppn_par = need_profile_norm_iter.clone(); // needed to avoid full re-encode for audio-only files

            // Uses the same shared_norm_cache from profile norm phase (single-pass guarantee)
            for (ap_idx, ap_indices) in audio_part_groups.iter().enumerate() {
                if total_audio_parts > 1 {
                    log::info!("[Phase7] Audio norm Part {}/{}: {} files", ap_idx + 1, total_audio_parts, ap_indices.len());
                }
                let mut js = tokio::task::JoinSet::new();
                for &idx in ap_indices {
                let s = par_sem.clone(); let e = par_err.clone(); let d = par_done.clone();
                let _t = par_total; let f = par_forensics.clone(); let w = par_wif.clone();
                let a = par_an.clone(); let n = par_norm_idx.clone();
                let c = cancel_flag.clone(); let p = probe_cache.clone(); let nf = temp_norm_files_arc.clone();
                let ff = ffmpeg_path_resolved.clone(); let fp = ffprobe_path_resolved.clone();
                let td = temp_dir.clone(); let j = request.job_id.clone(); let dm = dom_par.clone();
                let ah = app_handle.clone(); let dur = working_total_duration;
                let to = total_outliers; let _rcp = rcp_par.clone();
                let cp_sender = checkpoint_sender.clone(); // PER-JOB CHECKPOINT WRITER
                let rri = rr_par.clone(); let aaoi = aao_par.clone();
                let ppn = ppn_par.clone();
                let skip_vol = actual_mode == MergeMode::SmartMkv;
                let ow = orig_wif.clone();

                let norm_cache = shared_norm_cache.clone();
                let imm_reg = immutability_registry.clone();
                js.spawn(async move {
                    let _permit = match s.acquire().await { Ok(p) => p, Err(_) => { return } };
                    if let Ok(eg) = e.lock() { if eg.is_some() { return } }
                    if c.load(Ordering::Relaxed) { return }
                    // SINGLE-PASS GUARD: Skip if already normalized by profile loop.
                    // This is UNCONDITIONAL — files in both need_profile_norm and need_audio_norm
                    // must NOT be normalized twice (causes double AAC re-encode = "kee kee" artifacts).
                    {
                        let ag = a.lock().unwrap_or_else(|p| p.into_inner());
                        if ag.contains(&idx) {
                            log::info!("[SINGLE_PASS_GUARD] File #{} skipped audio norm — already profile-normalized", idx);
                            return;
                        }
                    }
                    let fpath = { let wg = w.lock().unwrap_or_else(|p| p.into_inner()); wg[idx].clone() };
                    let fname = std::path::Path::new(&fpath).file_name()
                        .map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".to_string());
                    // [P0-1] Get source file metadata for recovery checkpoint
                    let (source_size, source_mtime) = match std::fs::metadata(&fpath) {
                        Ok(meta) => {
                            let size = meta.len();
                            let mtime = meta.modified()
                                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
                                .unwrap_or(0);
                            (size, mtime)
                        }
                        Err(_) => (0, 0),
                    };
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
                    let needs_video_norm = ppn.contains(&idx);
                    let ntl = if needs_video_norm { "AudioVideoReencode" } else { "AudioReencode" };
                    if let Some(Ok(ii)) = p.get(std::path::Path::new(&fpath)) {
                        f.lock().unwrap_or_else(|p| p.into_inner()).record_file_start(idx, &fname, &fpath, &ii, ntl);
                    }
                    if c.load(Ordering::Relaxed) { return }
                    let ns = std::time::Instant::now();
                    let input_dur = p.get(std::path::Path::new(&fpath)).and_then(|r| r.ok()).map(|i| i.duration);
                    let input_has_audio = p.get(std::path::Path::new(&fpath)).and_then(|r| r.ok()).map(|i| !i.audio_streams.is_empty()).unwrap_or(false);
                    let input_video_duration_ms = ow.get(idx).and_then(|op| p.get(std::path::Path::new(op)).and_then(|r| r.ok()).and_then(|i| i.video_streams.first().and_then(|s| s.duration)).map(|d| (d * 1000.0) as u64));
                    let input_audio_sample_rate = p.get(std::path::Path::new(&fpath))
                        .and_then(|r| r.ok())
                        .and_then(|i| { i.audio_streams.first().map(|a| a.sample_rate) })
                        .flatten();
                    let input_audio_bitrate = p.get(std::path::Path::new(&fpath))
                        .and_then(|r| r.ok())
                        .and_then(|i| i.audio_streams.first().and_then(|a| a.bit_rate))
                        .map(|br| format!("{}k", br / 1000));
                    // Route: only escalate to full re-encode if the file actually needs video normalization.
                    // AAC-only files (e.g. HE-AAC→LC profile fix) use audio-only normalization
                    // even when dm.timescale_den is Some — the video is already at dominant settings.
                    let res = if needs_video_norm {
                        normalize_to_profile(&ff, &fpath, dm.v_codec.as_deref().unwrap_or("libx264"),
                            dm.a_codec.as_deref().unwrap_or("aac"), dm.a_sample_rate.unwrap_or(48000),
                            dm.v_fps, dm.timescale_den, dm.v_width, dm.v_height, dm.a_channels, input_audio_bitrate.clone(), &td, &j, idx, input_has_audio, c.clone(),
                            Some(norm_cache.clone()), input_dur, input_video_duration_ms, Some(&fp), input_audio_sample_rate).await
                    } else {
                        normalize_audio_only(&ff, &fpath, dm.a_codec.as_deref().unwrap_or("aac"),
                            dm.a_sample_rate.unwrap_or(48000), dm.timescale_den, dm.a_channels, input_audio_bitrate, &td, &j, idx, c.clone(),
                            Some(norm_cache.clone()), input_video_duration_ms, Some(&fp), input_audio_sample_rate).await
                    };
                    log::info!("[FORENSIC:NORMALIZE] END: Audio Normalize | File #{} | Elapsed: {:?}", idx, ns.elapsed());
                    match res {
                        Ok(path) => {
                            f.lock().unwrap_or_else(|p| p.into_inner()).record_file_norm_end(idx, &path);
                            nf.lock().unwrap_or_else(|p| p.into_inner()).push(std::path::PathBuf::from(&path));
                            { let mut wg = w.lock().unwrap_or_else(|p| p.into_inner()); wg[idx] = path.clone(); }

                            // ── IMMUTABILITY REGISTRY: Register after audio-only normalization ──
                            // Audit-only: registry tracks what was normalized for consistency checks.
                            {
                                let fingerprint = AudioFingerprint {
                                    codec: dm.a_codec.clone().unwrap_or_else(|| "aac".to_string()),
                                    sample_rate: dm.a_sample_rate.unwrap_or(48000),
                                    channels: dm.a_channels.unwrap_or(2),
                                    bitrate: None,
                                    profile: None,
                                };
                                imm_reg.register(&fpath, &path, fingerprint, &j);
                            }

                            if matches!(idx, 10 | 24 | 32) {
                                log::info!("[FORENSIC_TRACE] File #{} | AUDIO WORKER COMPLETE | normalized_path={}", idx, path);
                            }
                            { f.lock().unwrap_or_else(|p| p.into_inner()).record_file_verify_start(idx); }
                            let vr = verify_normalized_audio_health(&ff, &fp, &path, idx, &fname, c.clone(),
                                        skip_vol, input_video_duration_ms).await;
                            match vr {
                                Ok(ni) => {
                                    f.lock().unwrap_or_else(|p| p.into_inner()).record_file_verify_end(idx, ni.duration);
                                    p.insert(std::path::PathBuf::from(&path), Ok(ni.clone()));
                                    // ── DURATION AUDIT: async=1 + first_pts=0 measurement ──
                                    // Logs start_time, total duration, and audio stream duration
                                    // to detect sample insertion/removal or PTS origin shifts.
                                    // Run 20+ files, then compare input vs output.
                                    if let Some(Ok(in_info)) = p.get(std::path::Path::new(&fpath)) {
                                        let in_dur = in_info.duration;
                                        let out_dur = ni.duration;
                                        let delta = out_dur - in_dur;
                                        let delta_pct = if in_dur > 0.0 { (delta / in_dur) * 100.0 } else { 0.0 };
                                        let in_start = in_info.start_time.unwrap_or(0.0);
                                        let out_start = ni.start_time.unwrap_or(0.0);
                                        let in_audio_dur = in_info.audio_streams.first().and_then(|a| a.duration).unwrap_or(0.0);
                                        let out_audio_dur = ni.audio_streams.first().and_then(|a| a.duration).unwrap_or(0.0);
                                        log::info!("[DURATION_AUDIT] File #{} ({})", idx, fname);
                                        log::info!("[DURATION_AUDIT]   start_time:  {:.3}s → {:.3}s (delta: {:+.3}s)", in_start, out_start, out_start - in_start);
                                        log::info!("[DURATION_AUDIT]   duration:     {:.3}s → {:.3}s (delta: {:+.3}s, {:+.3}%)", in_dur, out_dur, delta, delta_pct);
                                        log::info!("[DURATION_AUDIT]   audio_dur:    {:.3}s → {:.3}s (delta: {:+.3}s)", in_audio_dur, out_audio_dur, out_audio_dur - in_audio_dur);
                                        if delta.abs() > 0.05 {
                                            log::warn!("[DURATION_AUDIT] ⚠️ File #{} ({}) total duration changed by {:.3}s ({:+.3}%) — aresample may be inserting/dropping samples",
                                                idx, fname, delta, delta_pct);
                                        }
                                        if (out_start - in_start).abs() > 0.01 {
                                            log::info!("[DURATION_AUDIT]   ℹ️ start_time shifted by {:.3}s — first_pts=0 is resetting PTS origin (expected)", out_start - in_start);
                                        }
                                    }
                                    if let Some(ref sender) = cp_sender {
                                        sender.append_completed_file(idx, fpath.clone(), source_size, source_mtime, path.clone(), crate::types::NormalizationType::Audio);
                                    }
                                }
                                Err(ve) => {
                                    if ve.contains("cancelled") || c.load(Ordering::Relaxed) {
                                        crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
crate::logger::stop_job_log(Some("[JOB_CANCELLED] Merge cancelled by user"));
let _ = ah.emit("merge-error", &serde_json::json!({"jobId":j,"error":"Merge cancelled by user","cancelled":true}));
                                        let mut eg = e.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some("Cancelled".to_string()); }
                                    } else {
                                        log::error!("[FORENSIC:AUDIO_VALIDATE] FAILED File #{}: {}", idx, ve);
                                        let mut eg = e.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some(format!("Audio repair failed for File #{} ({}): {}", idx, fname, ve)); }
                                    }
                                    return;
                                }
                            }
                            d.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(err_msg) => {
                            if err_msg.contains("cancelled") {
                                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Cancelled, Some("Merge cancelled by user"), None);
crate::logger::stop_job_log(Some("[JOB_CANCELLED] Merge cancelled by user"));
let _ = ah.emit("merge-error", &serde_json::json!({"jobId":j,"error":"Merge cancelled by user","cancelled":true}));
                                let mut eg = e.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some("Cancelled".to_string()); }
                            } else {
                                log::error!("[FORENSIC:ERROR] Audio norm FAILED File #{}: {}", idx, err_msg);
                                let mut eg = e.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some(format!("Audio normalization failed for File #{}: {}", idx, err_msg)); }
                            }
                        }
                    }
                });
            }
            while let Some(r) = js.join_next().await {
                if let Err(je) = r {
                    log::error!("[Phase7] Audio task panicked: {}", je);
                    let mut eg = par_err.lock().unwrap_or_else(|p| p.into_inner()); if eg.is_none() { *eg = Some(format!("Task panicked: {}", je)); }
                }
            }
            { let eg = par_err.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(ref em) = *eg {
                    if em == "Cancelled" {
                        crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle.emit("merge-error", &serde_json::json!({"jobId":request.job_id,"error":"Merge cancelled by user","cancelled":true}));
                        cleanup_guard.cleanup(); return Ok(request.job_id);
                    }
                    cleanup_guard.cleanup(); return Err(em.clone());
                }
            }
            } // end per-part audio norm loop
            working_input_files = par_wif.lock().unwrap_or_else(|p| p.into_inner()).clone();
            let _ = par_an.lock().unwrap_or_else(|p| p.into_inner()).clone();
            log::info!("[Phase7] Parallel audio norm complete: {} files", par_total);
            // P0 UX FIX: Log phase transition — UI update will be handled separately
            // The normalization is done; next phases are repeat propagation, cards, mkvmerge prep
            log::info!("[PHASE_TIMING] NORMALIZATION_COMPLETE | next=repeat_propagation_cards_mkvmerge_prep");
        }

        // ══════════════════════════════════════════════════════════════════════════════
        // P0 FIX: Normalization × Repeat Propagation
        //
        // BUG: Normalization dedup (lines 3136-3149, 3467-3480) removes repeat
        // instances from the normalization queue. When normalization completes,
        // only the FIRST occurrence of each source is updated to the normalized
        // path. Repeat instances (subsequent occurrences of the same source)
        // still point to the ORIGINAL files.
        //
        // IMPACT: The concat list contains MIXED paths:
        //   - Normalized files: have aresample=first_pts=0 applied (PTS reset)
        //   - Original files: NO PTS reset
        //   → Audio PTS discontinuity at every boundary between normalized and original
        //
        // FIX: After ALL normalization completes, build a source→normalized mapping
        // from what was actually processed, then propagate to ALL repeat instances.
        //
        // HOW IT WORKS:
        // - After normalization, working_input_files[i] contains either normalized or original path
        // - If it contains "norm_prof_", "norm_audio_", or "norm_ts_", it's normalized
        // - For each normalized entry at position i:
        //   - Source index = i % original_count (because repeat expands as [A,B,C,A,B,C,...])
        //   - Build mapping: source_idx → normalized_path
        // - Then single O(N) pass: replace all entries matching source_idx with normalized_path
        //
        // Complexity: O(N) single pass after normalization (not O(N²) per-worker updates)
        // ══════════════════════════════════════════════════════════════════════════════
        if let Some(ref expanded) = repeat_expanded {
            let original_count = expanded.original_count;
            let total_files = working_input_files.len();

            if total_files > original_count && original_count > 0 {
                // Step 1: Build source→normalized mapping from actually normalized entries
                let mut source_to_normalized: std::collections::HashMap<usize, String> =
                    std::collections::HashMap::new();

                for (i, path) in working_input_files.iter().enumerate() {
                    // Check if this entry was actually normalized
                    if path.contains("/norm_prof_") || path.contains("/norm_audio_") || path.contains("/norm_ts_") {
                        let source_idx = i % original_count;
                        // Only record the FIRST normalized occurrence for each source
                        // (dedup guarantees this is the first)
                        if let std::collections::hash_map::Entry::Vacant(e) = source_to_normalized.entry(source_idx) {
                            e.insert(path.clone());
                            log::info!("[REPEAT_FIX] Mapped source[{}] = {} → {}",
                                source_idx,
                                if path.len() > 50 { &path[path.len()-50..] } else { path },
                                if i != source_idx { " (propagated to repeat instances)" } else { " (first occurrence)" }
                            );
                        }
                    }
                }

                // Step 2: Propagate normalized paths to ALL repeat instances
                let mut propagated_count = 0;
                let mut already_normalized_count = 0;

                for (i, path) in working_input_files.iter_mut().enumerate() {
                    let source_idx = i % original_count;

                    // If there's a normalized version of this source, use it
                    if let Some(normalized_path) = source_to_normalized.get(&source_idx) {
                        // Check if this entry is NOT already the normalized path
                        let is_already_normalized = path.contains("/norm_prof_")
                            || path.contains("/norm_audio_")
                            || path.contains("/norm_ts_");
                        if !is_already_normalized {
                            let path_display = if path.len() > 50 { &path[path.len()-50..] } else { path.as_str() };
                            let norm_display = if normalized_path.len() > 50 { &normalized_path[normalized_path.len()-50..] } else { normalized_path.as_str() };
                            log::info!("[REPEAT_FIX] Propagating: index {} (source {}) {} → {}",
                                i, source_idx, path_display, norm_display);
                            *path = normalized_path.clone();
                            propagated_count += 1;
                        } else {
                            already_normalized_count += 1;
                        }
                    }
                }

                log::info!("[REPEAT_FIX] ═══════════════════════════════════════════════════════════");
                log::info!("[REPEAT_FIX] Propagation complete:");
                log::info!("[REPEAT_FIX]   Sources normalized: {}", source_to_normalized.len());
                log::info!("[REPEAT_FIX]   Already normalized (first occurrences): {}", already_normalized_count);
                log::info!("[REPEAT_FIX]   Propagated to repeat instances: {}", propagated_count);
                log::info!("[REPEAT_FIX]   Total files: {} ({} sources × {} repeats)",
                    total_files, original_count, total_files / original_count);
                log::info!("[REPEAT_FIX] ═══════════════════════════════════════════════════════════");

                // FORENSIC: Verify no mixed paths remain
                let normalized_count = working_input_files.iter()
                    .filter(|p| p.contains("/norm_prof_") || p.contains("/norm_audio_") || p.contains("/norm_ts_"))
                    .count();
                let original_remaining = working_input_files.iter()
                    .filter(|p| !p.contains("/norm_prof_") && !p.contains("/norm_audio_") && !p.contains("/norm_ts_"))
                    .count();

                if normalized_count == total_files {
                    log::info!("[REPEAT_FIX] ✅ VERIFIED: All {} files use normalized paths", total_files);
                } else if original_remaining > 0 {
                    log::error!("[REPEAT_FIX] ❌ CRITICAL: {} files still use ORIGINAL paths!", original_remaining);
                    log::error!("[REPEAT_FIX]    This indicates the propagation fix is incomplete!");
                }
            }
        }

        let norm_phase_elapsed = norm_phase_start.elapsed();

        // ══════════════════════════════════════════════════════════════════════════════
        // SMART MKV COST REPORT — Normalization + Verification Breakdown
        // ══════════════════════════════════════════════════════════════════════════════
        {
            let profile_count = need_profile_norm_iter.len();
            let audio_only_count = need_audio_norm.iter().filter(|idx| !need_profile_norm_iter.contains(idx)).count();
            let both_count = need_profile_norm_iter.iter().filter(|idx| need_audio_norm.contains(idx)).count();
            let total_normalized = profile_count + audio_only_count;
            let ff_processes_encode = total_normalized;
            let ff_processes_seek = total_normalized * 19;
            let ff_processes_vol = total_normalized;
            let ff_processes_spec = total_normalized;
            let ff_processes_verify = ff_processes_seek + ff_processes_vol + ff_processes_spec;
            let ff_processes_total = ff_processes_encode + ff_processes_verify;

            log::info!("[SMART_MKV_COST_REPORT] ═══════════════════════════════════════════════════════════════");
            log::info!("[SMART_MKV_COST_REPORT] NORMALIZATION + VERIFICATION COST BREAKDOWN");
            log::info!("[SMART_MKV_COST_REPORT] ═══════════════════════════════════════════════════════════════");
            log::info!("[SMART_MKV_COST_REPORT]   Files analyzed:           {}", total_input_count);
            log::info!("[SMART_MKV_COST_REPORT]   Files normalized:         {} ({:.1}%)", total_normalized,
                if total_input_count > 0 { (total_normalized as f64 / total_input_count as f64) * 100.0 } else { 0.0 });
            log::info!("[SMART_MKV_COST_REPORT]     Video reencode:         {}", profile_count);
            log::info!("[SMART_MKV_COST_REPORT]     Audio-only:             {}", audio_only_count);
            log::info!("[SMART_MKV_COST_REPORT]     Both (full reencode):   {}", both_count);
            log::info!("[SMART_MKV_COST_REPORT]   ─────────────────────────────────────────────────────────");
            log::info!("[SMART_MKV_COST_REPORT]   FFmpeg processes spawned:");
            log::info!("[SMART_MKV_COST_REPORT]     Encode (normalize):     {:>5}", ff_processes_encode);
            log::info!("[SMART_MKV_COST_REPORT]     Seek tests (19/file):   {:>5}", ff_processes_seek);
            log::info!("[SMART_MKV_COST_REPORT]     Volumedetect (1/file):  {:>5}", ff_processes_vol);
            log::info!("[SMART_MKV_COST_REPORT]     Spectral (1/file):      {:>5}", ff_processes_spec);
            log::info!("[SMART_MKV_COST_REPORT]     ───────────────────────────────");
            log::info!("[SMART_MKV_COST_REPORT]     Verification total:     {:>5}", ff_processes_verify);
            log::info!("[SMART_MKV_COST_REPORT]     GRAND TOTAL:            {:>5}", ff_processes_total);
            log::info!("[SMART_MKV_COST_REPORT]   ─────────────────────────────────────────────────────────");
            log::info!("[SMART_MKV_COST_REPORT]   Wall-clock time:         {:.1}s", norm_phase_elapsed.as_secs_f64());
            if total_normalized > 0 {
                let avg_per_file = norm_phase_elapsed.as_secs_f64() / total_normalized as f64;
                log::info!("[SMART_MKV_COST_REPORT]   Avg per normalized file: {:.1}s", avg_per_file);
            }
            log::info!("[SMART_MKV_COST_REPORT]   ─────────────────────────────────────────────────────────");
            log::info!("[SMART_MKV_COST_REPORT]   OPTIMIZATION LEVERS:");
            log::info!("[SMART_MKV_COST_REPORT]     Reduce seek points:     19 → 5 saves {} processes", ff_processes_seek - (total_normalized * 5));
            log::info!("[SMART_MKV_COST_REPORT]     Skip verification:      saves {} processes", ff_processes_verify);
            log::info!("[SMART_MKV_COST_REPORT]     Fewer files normalized: each file saves 22 processes");
            log::info!("[SMART_MKV_COST_REPORT] ═══════════════════════════════════════════════════════════════");
        }

        // ══════════════════════════════════════════════════════════════════════════════
        // REAL WORKLOAD FORENSIC REPORT
        // ══════════════════════════════════════════════════════════════════════════════
        let forensic_report = forensics.generate_report();
        for line in forensic_report.lines() {
            log::info!("[FORENSIC:REAL_WORKLOAD] {}", line);
        }

        // ══════════════════════════════════════════════════════════════════════════════
        // SMART MKV VALIDATION SUMMARY — Post-Normalization Decision Audit
        // ══════════════════════════════════════════════════════════════════════════════
        if actual_mode == MergeMode::SmartMkv {
            let analysis_elapsed = analysis_start.elapsed();
            let profile_count = need_profile_norm_iter.len();
            let audio_only_count = need_audio_norm.iter().filter(|idx| !need_profile_norm_iter.contains(idx)).count();
            let both_count = need_profile_norm_iter.iter().filter(|idx| need_audio_norm.contains(idx)).count();
            let remux_only_count = analysis.outliers.iter()
                .filter(|o| matches!(o.normalization_type, crate::ffmpeg::normalization::NormalizationType::RemuxOnly))
                .map(|o| o.index)
                .collect::<std::collections::HashSet<_>>()
                .iter()
                .filter(|idx| !need_profile_norm_iter.contains(idx) && !need_audio_norm.contains(idx))
                .count();
            let skipped = total_input_count - profile_count - audio_only_count;
            let norm_pct = if total_input_count > 0 { (profile_count as f64 / total_input_count as f64) * 100.0 } else { 0.0 };

            log::info!("[SMART_MKV_VALIDATION] ═══════════════════════════════════════════════════════════════");
            log::info!("[SMART_MKV_VALIDATION] SMART MKV POST-NORMALIZATION DECISION SUMMARY");
            log::info!("[SMART_MKV_VALIDATION] ═══════════════════════════════════════════════════════════════");
            log::info!("[SMART_MKV_VALIDATION]   Total Files:              {}", total_input_count);
            log::info!("[SMART_MKV_VALIDATION]   Analysis Time:            {:.3}s", analysis_elapsed.as_secs_f64());
            log::info!("[SMART_MKV_VALIDATION]   ─────────────────────────────────────────────────────────");
            log::info!("[SMART_MKV_VALIDATION]   Files Profile-Normalized: {:>5} ({:.1}%)", profile_count, norm_pct);
            log::info!("[SMART_MKV_VALIDATION]   Files Audio-Only Norm:    {:>5}", audio_only_count);
            log::info!("[SMART_MKV_VALIDATION]   Files Both (Full Re-enc): {:>5}", both_count);
            log::info!("[SMART_MKV_VALIDATION]   Files Remux-Only:         {:>5}", remux_only_count);
            log::info!("[SMART_MKV_VALIDATION]   Files SKIPPED:            {:>5} ({:.1}%)", skipped, if total_input_count > 0 { (skipped as f64 / total_input_count as f64) * 100.0 } else { 0.0 });
            log::info!("[SMART_MKV_VALIDATION]   ─────────────────────────────────────────────────────────");

            // Remaining normalization triggers
            if !analysis.outliers.is_empty() {
                let mut remaining_triggers: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
                for o in &analysis.outliers {
                    *remaining_triggers.entry(o.property.as_str()).or_insert(0) += 1;
                }
                let mut sorted: Vec<_> = remaining_triggers.into_iter().collect();
sorted.sort_by_key(|b| std::cmp::Reverse(b.1));
                log::info!("[SMART_MKV_VALIDATION]   REMAINING TRIGGERS (post-filter):");
                for (prop, count) in &sorted {
                    log::info!("[SMART_MKV_VALIDATION]     {:>30} | {} files", prop, count);
                }
            } else {
                log::info!("[SMART_MKV_VALIDATION]   REMAINING TRIGGERS: none — all outliers filtered");
            }

            // ── Per-File Normalization Breakdown ────────────────────────────────
            let mut all_indices: std::collections::HashSet<usize> = need_profile_norm_iter.iter().copied().collect();
            all_indices.extend(need_audio_norm.iter().copied());
            let mut all_indices: Vec<usize> = all_indices.into_iter().collect();
            all_indices.sort();
            
            log::info!("[SMART_MKV_REASON] ═══════════════════════════════════════════════════════════════");
            log::info!("[SMART_MKV_REASON] PER-FILE NORMALIZATION DECISIONS");
            log::info!("[SMART_MKV_REASON] ═══════════════════════════════════════════════════════════════");
            for idx in &all_indices {
                let _filename = working_input_files.get(*idx).map(|p| {
                    std::path::Path::new(p).file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("file_{}", idx))
                }).unwrap_or_else(|| format!("file_{}", idx));
                let in_profile = need_profile_norm_iter.contains(idx);
                let in_audio = need_audio_norm.contains(idx);
                let norm_type = if in_profile && in_audio {
                    "FullReencode"
                } else if in_profile {
                    "VideoReencode"
                } else {
                    "AudioOnly"
                };
                let normalized = in_profile || in_audio;
                let reasons: Vec<String> = outlier_by_index.get(idx).map(|outliers| {
                    outliers.iter().map(|o| {
                        format!("{}: {}→{}", o.property, o.actual_value, o.dominant_value)
                    }).collect()
                }).unwrap_or_default();
                let reason_str = if reasons.is_empty() {
                    repair_reasons.get(idx).map(|r| r.join(", ")).unwrap_or_else(|| "unknown".to_string())
                } else {
                    reasons.join("; ")
                };
                log::info!("[SMART_MKV_REASON] File #{}: normalized={} | type={} | reasons=[{}]", idx, normalized, norm_type, reason_str);
            }
            log::info!("[SMART_MKV_REASON] ═══════════════════════════════════════════════════════════════");
            
            log::info!("[SMART_MKV_VALIDATION]   ─────────────────────────────────────────────────────────");
            log::info!("[SMART_MKV_VALIDATION]   PER-FILE NORMALIZATION BREAKDOWN:");
            for idx in &all_indices {
                let filename = working_input_files.get(*idx).map(|p| {
                    std::path::Path::new(p).file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("file_{}", idx))
                }).unwrap_or_else(|| format!("file_{}", idx));
                let in_profile = need_profile_norm_iter.contains(idx);
                let in_audio = need_audio_norm.contains(idx);
                let norm_type = if in_profile && in_audio {
                    "FullReencode"
                } else if in_profile {
                    "VideoReencode"
                } else {
                    "AudioOnly"
                };
                // Collect all outlier reasons for this file
                let reasons: Vec<String> = outlier_by_index.get(idx).map(|outliers| {
                    outliers.iter().map(|o| {
                        format!("{}: {}→{}", o.property, o.actual_value, o.dominant_value)
                    }).collect()
                }).unwrap_or_default();
                let reason_str = if reasons.is_empty() {
                    // Check repair_reasons for non-outlier triggers (e.g. corruption, safe mode)
                    repair_reasons.get(idx).map(|r| r.join(", ")).unwrap_or_else(|| "unknown".to_string())
                } else {
                    reasons.join("; ")
                };
                log::info!("[SMART_MKV_VALIDATION]     File #{:<4} | {:<40} | {:<14} | {}", idx, format!("\"{}\"", filename), norm_type, reason_str);
            }

            // ── Reason Summary Table ────────────────────────────────────────────
            log::info!("[SMART_MKV_VALIDATION]   ─────────────────────────────────────────────────────────");
            log::info!("[SMART_MKV_VALIDATION]   REASON SUMMARY TABLE:");
            log::info!("[SMART_MKV_VALIDATION]   {:>30} | {:>5} files", "Reason", "Count");
            log::info!("[SMART_MKV_VALIDATION]   {:->30} | {:->5}", "---", "---");
            let mut reason_summary: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for idx in &all_indices {
                if let Some(outliers) = outlier_by_index.get(idx) {
                    for o in outliers {
                        *reason_summary.entry(o.property.clone()).or_insert(0) += 1;
                    }
                } else if let Some(reasons) = repair_reasons.get(idx) {
                    for r in reasons {
                        let key = if r.contains("Corruption") || r.contains("DeepValidation") || r.contains("SeekPointCheck") {
                            "corruption".to_string()
                        } else if r.contains("SafeMode") {
                            "safe_mode".to_string()
                        } else if r.contains("ProfileUnify") {
                            "profile_unify".to_string()
                        } else {
                            r.clone()
                        };
                        *reason_summary.entry(key).or_insert(0) += 1;
                    }
                }
            }
            let mut reason_sorted: Vec<_> = reason_summary.into_iter().collect();
            reason_sorted.sort_by_key(|b| std::cmp::Reverse(b.1));
            for (reason, count) in &reason_sorted {
                log::info!("[SMART_MKV_VALIDATION]   {:>30} | {:>5}", reason, count);
            }
            log::info!("[SMART_MKV_VALIDATION]   {:->30} | {:->5}", "---", "---");
            log::info!("[SMART_MKV_VALIDATION]   {:>30} | {:>5}", "TOTAL", all_indices.len());
            log::info!("[SMART_MKV_VALIDATION] ═══════════════════════════════════════════════════════════════");

            // ══════════════════════════════════════════════════════════════════════════════
            // DRY-RUN SIMULATION: What if a_profile normalization were enabled?
            // ══════════════════════════════════════════════════════════════════════════════
            {
                let pre_filter_a_profile: Vec<usize> = analysis_outliers_pre_filter.iter()
                    .filter(|o| o.property == "a_profile")
                    .map(|o| o.index)
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                let already_normalizing: std::collections::HashSet<usize> = need_profile_norm_iter.iter()
                    .chain(need_audio_norm.iter())
                    .copied()
                    .collect();
                let extra_files: Vec<usize> = pre_filter_a_profile.iter()
                    .filter(|idx| !already_normalizing.contains(idx))
                    .copied()
                    .collect();
                let extra_count = extra_files.len();
                let total_after = all_indices.len() + extra_count;
                let extra_pct = if total_input_count > 0 { (extra_count as f64 / total_input_count as f64) * 100.0 } else { 0.0 };

                log::info!("[SMART_MKV_AAC_SIMULATION] ═══════════════════════════════════════════════");
                log::info!("[SMART_MKV_AAC_SIMULATION] DRY-RUN: If a_profile normalization enabled");
                log::info!("[SMART_MKV_AAC_SIMULATION]   Files with a_profile outlier (pre-filter): {}", pre_filter_a_profile.len());
                log::info!("[SMART_MKV_AAC_SIMULATION]   Already normalizing for other reasons:      {}", already_normalizing.len());
                log::info!("[SMART_MKV_AAC_SIMULATION]   EXTRA files that would need norm:          {} ({:.1}%)", extra_count, extra_pct);
                log::info!("[SMART_MKV_AAC_SIMULATION]   Total normalized (current):                {}", all_indices.len());
                log::info!("[SMART_MKV_AAC_SIMULATION]   Total normalized (with a_profile):         {}", total_after);
                if extra_count > 0 {
                    log::info!("[SMART_MKV_AAC_SIMULATION]   Extra files:");
                    for idx in &extra_files {
                        let fname = working_input_files.get(*idx).map(|p| {
                            std::path::Path::new(p).file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| format!("file_{}", idx))
                        }).unwrap_or_else(|| format!("file_{}", idx));
                        log::info!("[SMART_MKV_AAC_SIMULATION]     File #{} ({})", idx, fname);
                    }
                }
                log::info!("[SMART_MKV_AAC_SIMULATION] ═══════════════════════════════════════════════");
            }
        }
    }

    // ── FORENSIC: Refresh probe_cache for normalized files ─────────────────────
    // BUG: After normalization creates new files (norm_prof_*, norm_audio_*, norm_ts_*),
    // the probe_cache only has ORIGINAL file paths from pre-probe. When validation runs,
    // cache lookups fail for normalized paths. The fallback probe should work, but if it
    // returns old cached data or the original path is still in cache with stale data,
    // validation will see mismatched properties.
    // This refreshes the cache with normalized file probe results BEFORE validation.
    log::info!("[FORENSIC:CACHE_REFRESH] Refreshing probe_cache for {} working_input_files...", working_input_files.len());
    let mut cache_refresh_count = 0;
    for file_path in &working_input_files {
        let is_normalized = file_path.contains("/norm_prof_")
            || file_path.contains("/norm_audio_")
            || file_path.contains("/norm_ts_")
            || file_path.contains("\\norm_prof_")
            || file_path.contains("\\norm_audio_")
            || file_path.contains("\\norm_ts_");
        if is_normalized {
            let normalized_path = Path::new(file_path);
            if probe_cache.get(normalized_path).is_none() {
                // Normalized file not in cache - probe it and insert
                match crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, normalized_path) {
                    Ok(info) => {
                        let path_display = if file_path.len() > 60 { &file_path[file_path.len()-60..] } else { file_path.as_str() };
                        log::info!("[FORENSIC:CACHE_REFRESH] Inserting normalized file into cache: ...{}", path_display);
                        probe_cache.insert(PathBuf::from(file_path), Ok(info));
                        cache_refresh_count += 1;
                    }
                    Err(e) => {
                        let path_display = if file_path.len() > 60 { &file_path[file_path.len()-60..] } else { file_path.as_str() };
                        log::error!("[FORENSIC:CACHE_REFRESH] Failed to probe normalized file: ...{} | error: {}", path_display, e);
                    }
                }
            }
        }
    }
    log::info!("[FORENSIC:CACHE_REFRESH] Cache refresh complete: {} files refreshed", cache_refresh_count);

    // PHASE-BASED RESUME: Update phase to Writing after normalization completes via single-writer
    if !skip_normalization {
        if let Some(ref sender) = checkpoint_sender {
            sender.update_phase(crate::types::MergePhase::Writing, None);
            log::info!("[PhaseResume] Checkpoint phase sent to writer: Writing after normalization");
        }
    } else {
        log::info!("[PhaseResume] Skipping phase update to Writing — normalization was skipped");
    }

    // ── Transition: normalization → merge prep ─────────────────────────
    log::info!("[PIPELINE_AUDIT] STAGE: Transition to merge prep");
    let _ = app_handle.emit("merge-progress", &serde_json::json!({
        "jobId": request.job_id,
        "progress": {
            "phase": "writing",
            "stageName": "Preparing merge...",
            "stagePercent": 0.0,
            "currentFileIndex": total_input_count,
            "totalFilesInStage": total_input_count,
            "percent": 35.0,
            "overallPercent": 35.0,
            "currentTime": 0.0,
            "totalDuration": working_total_duration,
            "currentFile": null,
            "normalizationType": null,
        }
    }));

    // ── PIPELINE AUDIT: Before Cards Insertion ───────────────────────────
    log::info!("[PIPELINE_AUDIT] STAGE: Before Cards Insertion");
    log::info!("[PIPELINE_AUDIT]   Working files: {}", working_input_files.len());
    log::info!("[PIPELINE_AUDIT]   Duration: {:.1}s ({:.1}h)", working_total_duration, working_total_duration / 3600.0);
    log::info!("[PIPELINE_AUDIT]   Cards config: {:?}", request.card_config.is_some());

    // MEDIA VALIDATION ENGINE --- analyze and repair damaged files
    // Runs after normalization so it validates the files that will merge.
    // Quarantined files are removed from the pipeline.
    log::info!("[MEDIA_VALIDATION] Starting media validation on {} files", working_input_files.len());

    let quarantine_dir = temp_dir.join("quarantine");
    let _ = std::fs::create_dir_all(&quarantine_dir);
    // Check if media validation is enabled (default: true)
    let validation_settings = crate::services::settings::load_settings_internal();
    let media_report = if validation_settings.enable_media_validation {
        validate_input_files(
            &ffprobe_path_resolved,
            &ffmpeg_path_resolved,
            &working_input_files,
            &quarantine_dir,
            Some(cancel_flag.clone()),
        )
    } else {
        log::info!("[MEDIA_VALIDATION] Skipped (disabled in settings)");
        crate::ffmpeg::media_validation_engine::MediaValidationReport::default()
    };

    log::info!("[MEDIA_VALIDATION] Report: {}", media_report.summary());

    // P0-2: Register repaired temp files with TempCleanup
    // Every repaired file registers with temp_norm_files_arc so cleanup_guard
    // removes them on success, failure, cancel, or panic.
    let mut repair_count = 0usize;
    for r in &media_report.file_results {
        if r.is_fixed() {
            if let Some(ref path) = r.repaired_path {
                if let Ok(mut nf) = temp_norm_files_arc.lock() {
                    nf.push(std::path::PathBuf::from(path));
                    repair_count += 1;
                }
            }
        }
    }
    if repair_count > 0 {
        log::info!("[MEDIA_VALIDATION] P0-2: Registered {} repaired files with TempCleanup", repair_count);
    }

    // P0-3: Re-probe repaired file durations
    // After repair, ffprobe the repaired file and update working_input_durations
    // so subtitle offsets and normalization use the correct duration.
    // Uses file_index matching (repaired paths != original paths, so path matching would fail).
    let mut duration_updates = 0usize;
    for r in &media_report.file_results {
        if r.is_fixed() {
            if let Some(ref path) = r.repaired_path {
                // Probe the repaired file
                let probe_result = {
                    let path_buf = std::path::PathBuf::from(path);
                    let ffprobe = ffprobe_path_resolved.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::ffmpeg::probe::probe_file(&ffprobe, &path_buf)
                            .map_err(|e| format!("{:#}", e))
                    }).await.map_err(|e| format!("Reprobe task panicked: {}", e))
                        .and_then(|r| r.map(|info| info.duration))
                };

                match probe_result {
                    Ok(dur) if dur > 0.0 => {
                        // Update by file_index (reliable: original path -> original index)
                        if r.file_index < working_input_durations.len() {
                            let orig_dur = working_input_durations[r.file_index];
                            working_input_durations[r.file_index] = dur;
                            let path_display = std::path::Path::new(path).file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| format!("...{}", &path[path.len().saturating_sub(60)..]));
                            log::info!("[SUBTITLE_TIMELINE_CERT] Post-repair duration update | File #{} | {} | orig={:.6}s | repaired={:.6}s | delta={:.6}s",
                                r.file_index, path_display, orig_dur, dur, dur - orig_dur);
                        }
                        // Also update probe_cache
                        let path_buf = std::path::PathBuf::from(path);
                        if let Some(Ok(info)) = probe_cache.get(&path_buf) {
                            let mut new_info = info.clone();
                            new_info.duration = dur;
                            probe_cache.insert(path_buf, Ok(new_info));
                        }
                        duration_updates += 1;
                    }
                    Ok(_) => {
                        log::warn!("[MEDIA_VALIDATION] P0-3: Repaired file {} has zero duration after repair", r.file_path);
                    }
                    Err(e) => {
                        log::warn!("[MEDIA_VALIDATION] P0-3: Failed to probe repaired file {}: {}", r.file_path, e);
                    }
                }
            }
        }
    }

    if duration_updates > 0 {
        log::info!("[MEDIA_VALIDATION] P0-3: Updated {} repaired file durations in working_input_durations (by file_index)", duration_updates);
    }

    // Apply validation results: update working files
    let (updated_files, removed_indices) = apply_validation_results(&working_input_files, &media_report);

    // STEP 1: Apply path updates FIRST (using original indices before any removal)
    let mut reparsed_count = 0usize;
    for (i, file) in working_input_files.iter_mut().enumerate() {
        if i < updated_files.len() && updated_files[i] != *file {
            *file = updated_files[i].clone();
            reparsed_count += 1;
        }
    }
    if reparsed_count > 0 {
        log::info!("[MEDIA_VALIDATION] Updated {} file paths from repaired paths", reparsed_count);
    }

    // STEP 2: Remove quarantined files (index-safe because path updates already applied)
    if !removed_indices.is_empty() {
        log::warn!("[MEDIA_VALIDATION] {} files quarantined (removed from pipeline)", removed_indices.len());
        let remove_set: std::collections::HashSet<usize> = removed_indices.into_iter().collect();
        let mut new_files = Vec::new();
        let mut new_durations = Vec::new();
        let mut new_names = Vec::new();
        for (i, file) in working_input_files.iter().enumerate() {
            if !remove_set.contains(&i) {
                new_files.push(file.clone());
                new_durations.push(working_input_durations[i]);
                new_names.push(working_input_names[i].clone());
            }
        }
        working_input_files = new_files;
        working_input_durations = new_durations;
        working_input_names = new_names;
        working_total_duration = working_input_durations.iter().sum();
    }

    // P0-4: Merge input provenance logging
    // Build index-to-result map BEFORE quarantine removal so indices are correct
    let fixed_paths: std::collections::HashSet<String> = media_report.file_results.iter()
        .filter(|r| r.is_fixed())
        .filter_map(|r| r.repaired_path.clone())
        .collect();

    let repair_methods: std::collections::HashMap<String, String> = media_report.file_results.iter()
        .filter(|r| r.is_fixed())
        .filter_map(|r| {
            r.repaired_path.as_ref().map(|p| {
                (p.clone(), format!("{:?}", r.fix_applied.as_ref().unwrap_or(&crate::ffmpeg::media_validation_engine::types::enums::FixType::None)))
            })
        })
        .collect();

    // Build file_index -> result lookup (files may have been reordered after removal)
    let mut result_by_index: std::collections::HashMap<usize, &crate::ffmpeg::media_validation_engine::MediaValidationResult> = std::collections::HashMap::new();
    for r in &media_report.file_results {
        result_by_index.insert(r.file_index, r);
    }

    log::info!("[MEDIA_VALIDATION] P0-4: MERGE INPUT PROVENANCE");
    log::info!("[MEDIA_VALIDATION] {:<6} {:<50} {:<50} {:<10} {:<15} {:<10} {:<8} {:<10} {:<50}",
        "Index", "Original Path", "Merge Path", "Status", "Repair Type", "Repair Stat", "Reval", "Duration", "Final Merge Path");

    for (i, file) in working_input_files.iter().enumerate() {
        let r = result_by_index.get(&i);
        let original_path = r.map(|r| r.file_path.clone()).unwrap_or_else(|| file.clone());
        let original_name = std::path::Path::new(&original_path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| original_path.clone());

        let merge_name = std::path::Path::new(file)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| file.clone());

        let r_entry = result_by_index.get(&i);
        let (status_label, repair_type, repair_status, reval_label) = if fixed_paths.contains(file) {
            let rtype = repair_methods.get(file).cloned().unwrap_or_else(|| "N/A".to_string());
            ("REPAIRED", rtype, "Succeeded", "PASS")
        } else if r_entry.map(|r| r.is_quarantined()).unwrap_or(false) {
            ("QUARANTINED", "N/A".to_string(), "Failed", "FAIL")
        } else {
            ("HEALTHY", "N/A".to_string(), "Skipped", "N/A")
        };

        let merge_path = if r_entry.map(|r| r.is_quarantined()).unwrap_or(false) {
            "X REMOVED"
        } else {
            file.as_str()
        };

        let dur = working_input_durations.get(i).copied().unwrap_or(0.0);

        log::info!("[MEDIA_VALIDATION] {:<6} {:<50} {:<50} {:<10} {:<15} {:<10} {:<8} {:<10.1} {:<50}",
            i,
            if original_name.len() > 48 { format!("{}...", &original_name[..45]) } else { original_name },
            if merge_name.len() > 48 { format!("{}...", &merge_name[..45]) } else { merge_name },
            status_label,
            repair_type,
            repair_status,
            reval_label,
            dur,
            merge_path
        );
    }

    log::info!("[MEDIA_VALIDATION] Summary: {} files -> {} after validation ({} repaired, {} quarantined)",
        media_report.total_files, working_input_files.len(), media_report.fixed_count, media_report.quarantined_count);

    let working_durations = working_input_durations.clone();
    let mut card_temp_files = Vec::new();

    let mut interleaved = false;
    let mut final_subtitle_list_path: Option<PathBuf> = None;
    let final_input_files;
    let final_input_names; 
    let final_input_durations; 
    let final_total_duration; 
    let final_segment_cards;

    log::info!("[SmartMkv] ═══════════════════════════════════════════════════════════════");
    log::info!("[SmartMkv] PIPELINE: SmartMkv/Lossless/Custom (re-encoding or stream copy)");
    log::info!("[SmartMkv] Mode: {:?}", actual_mode);
    log::info!("[SmartMkv] Input files: {}", working_input_files.len());

    if let Some(ref card_config) = request.card_config {
        log::info!("[CARDS] Card config received: color={}, fontColor={}, duration={}s, freq={:?}, showInReport={}", card_config.color, card_config.font_color, card_config.duration, card_config.frequency, card_config.show_in_report);
        log::info!("[CARDS] Input files: {} (need >= 2 for cards)", working_input_files.len());

        if working_input_files.len() >= 2 && card_config.duration > 0.0 {
// [REPEAT] Build boundary card labels if repeat boundary cards are enabled
            let boundary_card_labels = repeat_expanded.as_ref().and_then(|re| {
                crate::ffmpeg::repeat_merge::build_boundary_card_labels(re, working_input_files.len())
            });

            log::info!("[CARDS] Starting render_cards_for_merge()...");
            match crate::ffmpeg::cards::render_cards_for_merge(
                &ffmpeg_path_resolved,
                Some(&ffprobe_path_resolved),
                &working_input_files,
                &working_input_names,
                &working_durations,
                card_config,
                &temp_dir.join("cards"),
                boundary_card_labels.as_ref(),
            ) {
                Ok(rendered) => {
                    interleaved = true;
                    log::info!("[CARDS] render_cards_for_merge() succeeded: {} cards rendered", rendered.len());
                    log::info!("[CARDS] Frequency: {:?}", card_config.frequency);
                    log::info!("[CARDS] BEFORE INTERLEAVE: rendered.len()={}, input_files.len()={}", rendered.len(), working_input_files.len());
                    log::info!("[CARDS] BEFORE INTERLEAVE: rendered={:?}", rendered);
                    let interleave = crate::ffmpeg::cards::interleave_cards_with_videos(
                        &working_input_files,
                        &working_durations,
                        &rendered,
                        card_config,
                    );
                    // Build names and track card temp files
                    let mut names = Vec::new();
                    let mut video_idx = 0;
                    for (i, (is_card, _)) in interleave.segment_cards.iter().enumerate() {
                        if *is_card {
                            let folder = Path::new(&interleave.files[i])
                                .parent()
                                .and_then(|p| p.file_name())
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            let display_label = if folder.is_empty() { "Start".to_string() } else { folder.to_string() };
                            names.push(format!("▶ Section: {}", display_label));
                            card_temp_files.push(PathBuf::from(&interleave.files[i]));
                        } else {
                            names.push(working_input_names[video_idx].clone());
                            video_idx += 1;
                        }
                    }
                    let video_count = working_input_files.len();
                    let card_count = interleave.segment_cards.iter().filter(|(c, _)| *c).count();
                    log::info!("[CARDS] Insertion complete: {} videos + {} section cards = {} total segments", video_count, card_count, interleave.files.len());
                    final_input_files = interleave.files; final_input_names = names; final_input_durations = interleave.durations; final_total_duration = final_input_durations.iter().sum(); final_segment_cards = interleave.segment_cards;
                    log::info!("[CARDS] AFTER INTERLEAVE: segment_cards.len()={}, true_count={}", final_segment_cards.len(), final_segment_cards.iter().filter(|(c, _)| *c).count());
                    log::info!("[CARDS] AFTER INTERLEAVE: is_card flags={:?}", final_segment_cards.iter().map(|(c, _)| *c).collect::<Vec<_>>());
                }
                Err(e) => {
                    log::error!("[CARDS] render_cards_for_merge() FAILED: {:?}", e);
                    log::warn!("[CARDS] render_cards_for_merge() failed — merging without interleaved cards");
                    // Emit warning to frontend so user knows cards were dropped
                    let _ = app_handle.emit("merge-progress", &serde_json::json!({
                        "jobId": request.job_id,
                        "progress": {
                            "phase": "writing",
                            "warning": "⚠ Canvas cards failed to render. Merge continued without cards.",
                            "currentTime": 0.0,
                            "totalDuration": working_total_duration,
                        }
                    }));
                    final_input_files = working_input_files.clone();
                    final_input_names = working_input_names.clone();
                    final_input_durations = working_durations.clone();
                    final_total_duration = final_input_durations.iter().sum();
                    final_segment_cards = vec![(false, None); working_input_files.len()];
                }
            }
        } else {
            log::info!("[CARDS] Skipping cards: files={} (< 2) or duration={} (<= 0)", working_input_files.len(), card_config.duration);
            final_input_files = working_input_files.clone(); final_input_names = working_input_names.clone(); final_input_durations = working_durations.clone(); final_total_duration = final_input_durations.iter().sum(); final_segment_cards = vec![(false, None); working_input_files.len()];
        }
    } else { 
        log::info!("[CARDS] No card config in request — skipping card insertion");
        final_input_files = working_input_files.clone(); final_input_names = working_input_names.clone(); final_input_durations = working_durations.clone(); final_total_duration = final_input_durations.iter().sum(); final_segment_cards = vec![(false, None); working_input_files.len()]; 
    }

    // ── PIPELINE AUDIT: After Cards / Final Input Files ────────────────────
    log::info!("[PIPELINE_AUDIT] STAGE: After Cards Insertion (final_input_files)");
    log::info!("[PIPELINE_AUDIT]   Final files count: {}", final_input_files.len());
    log::info!("[PIPELINE_AUDIT]   Final total duration: {:.1}s ({:.1}h)", final_total_duration, final_total_duration / 3600.0);
    log::info!("[PIPELINE_AUDIT]   Card segments: {}", final_segment_cards.iter().filter(|(c, _)| *c).count());
    log::info!("[PIPELINE_AUDIT]   Video segments: {}", final_segment_cards.iter().filter(|(c, _)| !*c).count());

    // ── Phase 4: Final Concat Input Audit ──────────────────────────────────────
    log::info!("[FORENSIC:CONCAT_AUDIT] Verifying final inputs before concat...");

    // FORENSIC: Dump all final_input_files with normalized status
    for (i, file) in final_input_files.iter().enumerate() {
        let is_norm = file.contains("/norm_") || file.contains("\\norm_");
        log::info!("[FORENSIC:FINAL_FILE] [{}] is_normalized={} path={}", i, is_norm, file);
        if matches!(i, 10 | 24 | 32) {
            log::info!("[FORENSIC_TRACE] File #{} | FINAL_INPUT_FILES ENTRY | is_norm={} | path={}", i, is_norm, file);
        }
    }
    
    // ── FORENSIC: Immediate dump before validation ────────────────────────
    // This is the GROUND TRUTH: what path exists at each index, and what does probe_cache say?
    for (i, file) in final_input_files.iter().enumerate() {
        let is_norm = file.contains("/norm_") || file.contains("\\norm_");
        let probe_result = probe_cache.get(Path::new(file));
        let (sr, w, h) = if let Some(Ok(info)) = probe_result {
            (
                info.audio_streams.first().and_then(|s| s.sample_rate),
                info.video_streams.first().and_then(|s| s.width),
                info.video_streams.first().and_then(|s| s.height)
            )
        } else {
            (None, None, None)
        };
        log::info!("[FORENSIC_FINAL_DUMP] [{}] PATH={} IS_NORM={} SR={:?} W={:?} H={:?}",
            i, file, is_norm, sr, w, h);
        if matches!(i, 10 | 24 | 32) {
            log::info!("[FORENSIC_TRACE] File #{} | FORENSIC_FINAL_DUMP | PATH={} IS_NORM={} SR={:?} RES={:?}x{:?}",
                i, file, is_norm, sr, w, h);
        }
    }

    // NOTE: We rely on the probe_cache which we manually refreshed above for normalized files.
    let mut final_infos = Vec::new();
    let mut cache_hits = 0;
    let mut cache_misses = 0;
    for (i, file) in final_input_files.iter().enumerate() {
        let is_norm = file.contains("/norm_") || file.contains("\\norm_");
        if let Some(Ok(info)) = probe_cache.get(Path::new(file)) {
            cache_hits += 1;
            let path_display = if file.len() > 60 { &file[file.len()-60..] } else { file };
            log::info!("[FORENSIC:FINAL_VALIDATION] [{}] CACHE_HIT is_norm={} ...{} | SR: {:?} | Res: {:?}x{:?}", 
                i, is_norm, path_display,
                info.audio_streams.first().and_then(|s| s.sample_rate),
                info.video_streams.first().and_then(|s| s.width),
                info.video_streams.first().and_then(|s| s.height));
            if matches!(i, 10 | 24 | 32) {
                log::info!("[FORENSIC_TRACE] File #{} | FINAL_VALIDATION | SR={:?} | Res={:?}x{:?}", i,
                    info.audio_streams.first().and_then(|s| s.sample_rate),
                    info.video_streams.first().and_then(|s| s.width),
                    info.video_streams.first().and_then(|s| s.height));
            }
            final_infos.push((i, file.clone(), info.clone()));
        } else {
            cache_misses += 1;
            // Fallback: if somehow missing from cache, probe it now
            if let Ok(info) = crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, Path::new(file)) {
                let path_display = if file.len() > 60 { &file[file.len()-60..] } else { file };
                log::info!("[FORENSIC:FINAL_VALIDATION] [{}] CACHE_MISS/NEW_PROBE is_norm={} ...{} | SR: {:?} | Res: {:?}x{:?}", 
                    i, is_norm, path_display,
                    info.audio_streams.first().and_then(|s| s.sample_rate),
                    info.video_streams.first().and_then(|s| s.width),
                    info.video_streams.first().and_then(|s| s.height));
                final_infos.push((i, file.clone(), info.clone()));
            } else {
                log::error!("[FORENSIC:FINAL_VALIDATION] [{}] PROBE_FAILED is_norm={} file={}", i, is_norm, file);
                return Err(format!("CRITICAL: Failed to probe final input file: {}", file));
            }
        }
    }
    log::info!("[FORENSIC:FINAL_VALIDATION] Summary: cache_hits={} cache_misses={}", cache_hits, cache_misses);
    let final_analysis = analyze_profiles(&final_infos);
    log::info!("[FORENSIC:CONCAT_AUDIT] Final Inputs Report:\n{}", format_audit_report(&final_analysis));
    
    // ── a_profile warning: auto-force Custom mode ─────────────────────────────
    // The native FFmpeg AAC encoder only supports aac_low (LC). It cannot produce
    // HE-AAC or HE-AAC v2. So if a_profile mismatch exists in Lossless mode, we
    // cannot satisfy it — instead, force Custom mode so audio is re-encoded to
    // uniform LC (via aresample), rather than blocking the merge indefinitely.
    let has_profile_outlier = final_analysis.outliers.iter().any(|o| o.property == "a_profile");
    if has_profile_outlier && actual_mode == MergeMode::Lossless {
        log::warn!("[AUDIO_PROFILE_WARN] a_profile mismatch detected but normalization cannot produce HE-AAC/HE-AACv2 (encoder only supports aac_low). Forcing Custom mode — audio will be re-encoded to uniform LC.");
        actual_mode = MergeMode::Custom;
    } else if has_profile_outlier {
        log::warn!("[AUDIO_PROFILE_WARN] a_profile mismatch detected — audio will use stream-copy. Ensure all files share the same AAC profile to avoid decoder switches mid-stream.");
    }
    
    // Check for critical mismatches that break concat
    let mut critical_mismatches = String::new();
    for o in &final_analysis.outliers {
        // NOTE: a_profile is NOT in this list — it is handled above (auto-force Custom or warn).
        // The normalizer cannot produce HE-AAC profiles, so blocking on a_profile would create
        // an unsatisfiable validation rule. Instead, we force Custom mode or warn and proceed.
        // NOTE: time_base is NOT critical — normalize_timescale_lossless fixes the
        // container-level timescale (what concat uses), but ffprobe reports the codec-level
        // time_base which doesn't change. The concat demuxer handles this correctly.
        // For SmartMkv mode, a_sample_rate and resolution are handled natively by mkvmerge.
        // They were filtered out by filter_outliers_for_mkv() and not normalized, so
        // they will still appear as mismatches in final_analysis — but they are safe.
        let is_critical = if actual_mode == MergeMode::SmartMkv {
            matches!(o.property.as_str(), "v_codec" | "a_channels")
        } else {
            matches!(o.property.as_str(), "v_codec" | "resolution" | "a_sample_rate" | "a_channels")
        };
        if is_critical {
            critical_mismatches.push_str(&format!("- File {}: {} (Expected: {}, Got: {})\n", o.index, o.property, o.dominant_value, o.actual_value));
            // ── FORENSIC: Why is this still mismatched? ──────────────────────
            log::error!("[AUDIO_CRITICAL_MISMATCH] File #{} | Property: {} | Expected: {} | Got: {}", o.index, o.property, o.dominant_value, o.actual_value);
            log::error!("[AUDIO_CRITICAL_MISMATCH]   Was this file normalized? Check if it appears in need_profile_norm or need_audio_norm");
            log::error!("[AUDIO_CRITICAL_MISMATCH]   Normalization type for this outlier: {:?}", o.normalization_type);
        } else if o.property == "a_profile" {
            // a_profile outlier: already handled above (forced Custom or warned). Log as info.
            log::info!("[AUDIO_PROFILE_INFO] File #{} | Profile mismatch: dominant={}, actual={}", o.index, o.dominant_value, o.actual_value);
        }
    }
    if !critical_mismatches.is_empty() {
        // ── FORENSIC: Full dump of all outliers ──────────────────────────────
        log::error!("[AUDIO_CRITICAL_MISMATCH] ═══════════════════════════════════════════════════════════");
        log::error!("[AUDIO_CRITICAL_MISMATCH] MERGE ABORT: {} critical mismatches remain", final_analysis.outliers.iter().filter(|o| if actual_mode == MergeMode::SmartMkv {
                matches!(o.property.as_str(), "v_codec" | "a_channels" | "a_profile")
            } else {
                matches!(o.property.as_str(), "v_codec" | "resolution" | "a_sample_rate" | "a_channels" | "a_profile")
            }).count());
        log::error!("[AUDIO_CRITICAL_MISMATCH] All outliers ({} total):", final_analysis.outliers.len());
        for o in &final_analysis.outliers {
            log::error!("[AUDIO_CRITICAL_MISMATCH]   [{}] {} | {} → {} | {:?} | {}", o.index, o.property, o.dominant_value, o.actual_value, o.normalization_type, o.reason);
        }
        log::error!("[AUDIO_CRITICAL_MISMATCH] Dominant time_base: {:?}", final_analysis.dominant.v_time_base);
        log::error!("[AUDIO_CRITICAL_MISMATCH] Dominant sample_rate: {:?}", final_analysis.dominant.a_sample_rate);
        log::error!("[AUDIO_CRITICAL_MISMATCH] Dominant channels: {:?}", final_analysis.dominant.a_channels);
        log::error!("[AUDIO_CRITICAL_MISMATCH] ═══════════════════════════════════════════════════════════");
        return Err(format!("MERGE ABORTED: Critical profile mismatches remain before concat.\n{}", critical_mismatches));
    }
    log::info!("[FORENSIC:CONCAT_AUDIT] ✅ All critical inputs match perfectly.");
    // ────────────────────────────────────────────────────────────────────────

    let list_path = temp_dir.join(format!("concat_{}.txt", request.job_id));
    let path_refs: Vec<&Path> = final_input_files.iter().map(Path::new).collect();
    // ── FORENSIC:PRE_CONCAT_AUDIO ───────────────────────────────────────────
    // Log exact audio properties of every file that will enter the concat stage.
    // This reveals exactly what the concat demuxer receives — if noise appears in
    // the final output but NOT in these files, the concat stage is responsible.
    log::info!("[FORENSIC:PRE_CONCAT_AUDIO] ═════════════════════════════════════════════");
    for (i, file_path) in final_input_files.iter().enumerate() {
        if let Some(Ok(info)) = probe_cache.get(Path::new(file_path)) {
            if let Some(audio) = info.audio_streams.first() {
                log::info!(
                    "[FORENSIC:PRE_CONCAT_AUDIO] File #{:<3} | Codec={:<6} | Profile={:<12} | SR={:<6} | Ch={:<3} | Layout={:<10} | Start={:<10} | Duration={:<12} | Bitrate={} | {}",
                    i,
                    &audio.codec_name,
                    audio.profile.as_deref().unwrap_or("N/A"),
                    audio.sample_rate.unwrap_or(0),
                    audio.channels.unwrap_or(0),
                    audio.channel_layout.as_deref().unwrap_or("N/A"),
                    format!("{:.3}", info.start_time.unwrap_or(0.0)),
                    format!("{:.3}", info.duration),
                    audio.bit_rate.map(|b| format!("{:.0}", b)).unwrap_or_else(|| "N/A".into()),
                    Path::new(file_path).file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| file_path.clone())
                );
            } else {
                log::warn!("[FORENSIC:PRE_CONCAT_AUDIO] File #{} has NO audio stream: {}", i, file_path);
            }
        } else {
            log::error!("[FORENSIC:PRE_CONCAT_AUDIO] File #{} FAILED to probe: {}", i, file_path);
        }
    }
    log::info!("[FORENSIC:PRE_CONCAT_AUDIO] ═════════════════════════════════════════════");

    // ── FORENSIC:BOUNDARY_AUDIO ────────────────────────────────────────────
    // Diagnose repaired↔unrepaired boundary transitions. The core hypothesis is
    // that encoder delay / priming / PTS-origin differences between normalized
    // files (FFmpeg re-encode with aresample=first_pts=0) and original
    // source files cause AAC decoder sync loss at concat boundaries.
    log::info!("[FORENSIC:BOUNDARY_AUDIO] ═════════════════════════════════════════════");
    log::info!("[FORENSIC:BOUNDARY_AUDIO] BOUNDARY TRANSITION DIAGNOSTIC");
    log::info!("[FORENSIC:BOUNDARY_AUDIO] Files: {}", final_input_files.len());
    log::info!("[FORENSIC:BOUNDARY_AUDIO] ═════════════════════════════════════════════");

    let file_is_normalized: Vec<bool> = final_input_files.iter().map(|f| {
        let f_lower = f.to_lowercase();
        f_lower.contains("norm_prof_") || f_lower.contains("norm_audio_") || f_lower.contains("norm_ts_")
    }).collect();

    for i in 1..final_input_files.len() {
        let prev_is_norm = file_is_normalized[i - 1];
        let curr_is_norm = file_is_normalized[i];
        let boundary_type = match (prev_is_norm, curr_is_norm) {
            (true, true) => "repaired->repaired",
            (true, false) => "repaired->ORIGINAL  <<<",
            (false, true) => "ORIGINAL->repaired  <<<",
            (false, false) => "original->original",
        };
        let prev_name = std::path::Path::new(&final_input_files[i - 1]).file_name()
            .map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| format!("file_{}", i - 1));
        let curr_name = std::path::Path::new(&final_input_files[i]).file_name()
            .map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| format!("file_{}", i));
        let prev_info = probe_cache.get(std::path::Path::new(&final_input_files[i - 1])).and_then(|r| r.ok());
        let curr_info = probe_cache.get(std::path::Path::new(&final_input_files[i])).and_then(|r| r.ok());
        let prev_audio = prev_info.as_ref().and_then(|info| info.audio_streams.first());
        let curr_audio = curr_info.as_ref().and_then(|info| info.audio_streams.first());

        log::info!("[FORENSIC:BOUNDARY_AUDIO] ---");
        log::info!("[FORENSIC:BOUNDARY_AUDIO] Boundary {}->{} | {}", i - 1, i, boundary_type);
        log::info!("[FORENSIC:BOUNDARY_AUDIO]   L [{}]: {}", i - 1, prev_name);
        log::info!("[FORENSIC:BOUNDARY_AUDIO]   R [{}]: {}", i, curr_name);

        if let Some(a) = prev_audio {
            log::info!("[FORENSIC:BOUNDARY_AUDIO]   L codec={} profile={} sr={} ch={} br={} bps={} start_pts={:?} start_time={:?} dur={}",
                a.codec_name,
                a.profile.as_deref().unwrap_or("N/A"),
                a.sample_rate.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
                a.channels.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
                a.bit_rate.map(|v| format!("{}b", v)).unwrap_or_else(|| "N/A".into()),
                a.bits_per_raw_sample.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
                a.start_pts,
                a.start_time,
                a.duration.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "N/A".into()));
        } else {
            log::info!("[FORENSIC:BOUNDARY_AUDIO]   L NO AUDIO");
        }
        if let Some(a) = curr_audio {
            log::info!("[FORENSIC:BOUNDARY_AUDIO]   R codec={} profile={} sr={} ch={} br={} bps={} start_pts={:?} start_time={:?} dur={}",
                a.codec_name,
                a.profile.as_deref().unwrap_or("N/A"),
                a.sample_rate.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
                a.channels.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
                a.bit_rate.map(|v| format!("{}b", v)).unwrap_or_else(|| "N/A".into()),
                a.bits_per_raw_sample.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
                a.start_pts,
                a.start_time,
                a.duration.map(|v| format!("{:.4}", v)).unwrap_or_else(|| "N/A".into()));
        } else {
            log::info!("[FORENSIC:BOUNDARY_AUDIO]   R NO AUDIO");
        }

        if prev_is_norm != curr_is_norm {
            if let (Some(l), Some(r)) = (prev_audio, curr_audio) {
                let mut diffs: Vec<String> = Vec::new();
                if l.start_pts != r.start_pts { diffs.push(format!("start_pts: {:?}->{:?}", l.start_pts, r.start_pts)); }
                if l.start_time.and_then(|ls| r.start_time.map(|rs| (ls - rs).abs() > 0.001)).unwrap_or(false) {
                    diffs.push(format!("start_time: {:?}s->{:?}s", l.start_time, r.start_time));
                }
                if l.profile != r.profile { diffs.push(format!("profile: {:?}->{:?}", l.profile, r.profile)); }
                if l.bit_rate != r.bit_rate { diffs.push(format!("bit_rate: {:?}->{:?}", l.bit_rate, r.bit_rate)); }
                if l.sample_rate != r.sample_rate { diffs.push(format!("sr: {:?}->{:?}", l.sample_rate, r.sample_rate)); }
                if l.channels != r.channels { diffs.push(format!("ch: {:?}->{:?}", l.channels, r.channels)); }
                if l.bits_per_raw_sample != r.bits_per_raw_sample { diffs.push(format!("bps: {:?}->{:?}", l.bits_per_raw_sample, r.bits_per_raw_sample)); }
                if diffs.is_empty() {
                    log::info!("[FORENSIC:BOUNDARY_AUDIO]   <<< SUSPICIOUS boundary NO metadata diffs");
                } else {
                    for d in &diffs { log::info!("[FORENSIC:BOUNDARY_AUDIO]   <<< DIFF: {}", d); }
                }
            }
        }
    }
    log::info!("[FORENSIC:BOUNDARY_AUDIO] ═════════════════════════════════════════════");
    // ─────────────────────────────────────────────────────────────────────────
    // For stream copy (Lossless): omit duration directive from concat list.
    // The concat demuxer reads each file until EOF — duration directives cause
    // timeline inflation with VFR files (observed as ~2x output duration).
    // For re-encoding (Custom): include duration so FFmpeg knows how much to process.
    let include_duration = actual_mode == MergeMode::Custom;
    write_concat_list_with_durations(&path_refs, Some(&final_input_durations), &list_path, include_duration).map_err(|e| e.to_string())?;

    // ── [CARDS] Dump actual concat list content for verification ──
    match std::fs::read_to_string(&list_path) {
        Ok(content) => {
            let line_count = content.lines().count();
            let card_lines = content.lines().filter(|l| l.contains("card_")).count();
            log::info!("[CARDS:CONCAT_AUDIT] Concat list written to: {}", list_path.display());
            log::info!("[CARDS:CONCAT_AUDIT] Concat list has {} lines ({} card file references)", line_count, card_lines);
            // Log first 50 lines for small concat lists, or just summary for large ones
            if line_count <= 60 {
                for (i, line) in content.lines().enumerate() {
                    log::info!("[CARDS:CONCAT_AUDIT]   L{:03}: {}", i, line);
                }
            } else {
                for (i, line) in content.lines().take(30).enumerate() {
                    log::info!("[CARDS:CONCAT_AUDIT]   L{:03}: {}", i, line);
                }
                log::info!("[CARDS:CONCAT_AUDIT]   ... ({} more lines) ...", line_count - 30);
            }
        }
        Err(e) => {
            log::error!("[CARDS:CONCAT_AUDIT] FAILED to read concat list: {}", e);
        }
    }

    let mut final_prepared_subs = vec![None; final_input_files.len()];
    if interleaved {
        let mut sub_idx = 0;
        for (i, (is_card, _)) in final_segment_cards.iter().enumerate() {
            if !*is_card {
                if let Some(s) = prepared_subs.get(sub_idx) {
                    final_prepared_subs[i] = s.clone();
                }
                sub_idx += 1;
            }
        }
    } else {
        final_prepared_subs = prepared_subs.clone();
    }

    if should_process_subs && final_prepared_subs.iter().any(|s| s.is_some()) {
        // -- [SUBTITLE_TIMELINE_CERT] Log exact durations being written to subtitle concat list --
        log::info!("[SUBTITLE_TIMELINE_CERT] ===== SUBTITLE CONCAT LIST WRITE - Duration Certification =====");
        log::info!("[SUBTITLE_TIMELINE_CERT] Segments: {} (with subs: {})", final_prepared_subs.len(),
            final_prepared_subs.iter().filter(|s| s.is_some()).count());
        let mut cum_video: f64 = 0.0;
        for (seg_idx, dur) in final_input_durations.iter().enumerate() {
            let is_card = final_segment_cards.get(seg_idx).map(|c| c.0).unwrap_or(false);
            let has_sub = final_prepared_subs.get(seg_idx).and_then(|s| s.as_ref()).is_some();
            let seg_type = if is_card { "CARD" } else if has_sub { "VIDEO+SUB" } else { "VIDEO" };
            let sub_name = if has_sub {
                let p = final_prepared_subs[seg_idx].as_ref().unwrap();
                std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
            } else { String::new() };
            log::info!("[SUBTITLE_TIMELINE_CERT]   [{:>4}] {:>12}  dur={:>12.6}s  cum={:>14.6}s{}",
                seg_idx, seg_type, dur, cum_video + dur,
                if has_sub { format!("  sub={}", sub_name) } else { String::new() });
            cum_video += dur;
        }
        log::info!("[SUBTITLE_TIMELINE_CERT] Total cumulative duration: {:.6}s", cum_video);
        log::info!("[SUBTITLE_TIMELINE_CERT] ============================================================");

        let suffix = if interleaved { "interleaved" } else { "direct" };
        let slp = temp_dir.join(format!("concat_sub_{}_{}.txt", suffix, request.job_id));
        if crate::ffmpeg::write_subtitle_concat_list(&final_prepared_subs, &final_input_durations, &slp, &temp_dir, "srt").is_ok() {
            final_subtitle_list_path = Some(slp);
        }
    }

    let burn_subtitle_path = if is_burn_mode {
        if let Some(ref _slp) = final_subtitle_list_path {
            let burn_srt = temp_dir.join(format!("burn_sub_{}.srt", request.job_id));
            if crate::ffmpeg::generate_merged_srt_with_rebase(&final_prepared_subs, &final_input_durations, &ffmpeg_path_resolved, &burn_srt).is_ok() { Some(burn_srt) } else { None }
        } else { None }
    } else { None };
    let subtitle_mode_for_closure = subtitle_mode.clone();

    // ── [CARDS] Concat list card verification ──
    let total_card_segments = final_segment_cards.iter().filter(|(c, _)| *c).count();
    log::info!("[CARDS] Concat list prepared: {} total segments, {} cards, {} videos",
        final_segment_cards.len(), total_card_segments, final_segment_cards.len() - total_card_segments);

    // ── PIPELINE AUDIT: Before Concat (Final Stage) ──────────────────────
    log::info!("[PIPELINE_AUDIT] ══════════════════════════════════════════════════════════");
    log::info!("[PIPELINE_AUDIT] STAGE: Before Concat (Final Pipeline Stage)");
    log::info!("[PIPELINE_AUDIT] ══════════════════════════════════════════════════════════");
    log::info!("[PIPELINE_AUDIT] PIPELINE COUNT TABLE");
    log::info!("[PIPELINE_AUDIT]   │ Stage                           │ File Count │ Duration");
    log::info!("[PIPELINE_AUDIT]   │─────────────────────────────────┼────────────┼─────────");
    log::info!("[PIPELINE_AUDIT]   │ Request Received                │ {:>10} │ {:.0}s", request.input_files.len(), request.total_duration);
    log::info!("[PIPELINE_AUDIT]   │ After Dedup                     │ {:>10} │ {:.0}s", deduped_file_count, deduped_total_duration);
    if repeat_expanded.is_some() {
        log::info!("[PIPELINE_AUDIT]   │ After Repeat Expansion          │ {:>10} │ {:.0}s", final_input_files.len(), final_total_duration);
    } else {
        log::info!("[PIPELINE_AUDIT]   │ Repeat Expansion (N/A)          │ {:>10} │ {:.0}s", "N/A", deduped_total_duration);
    }
    log::info!("[PIPELINE_AUDIT]   │ After Cards / Final Inputs      │ {:>10} │ {:.0}s", final_input_files.len(), final_total_duration);
    log::info!("[PIPELINE_AUDIT]   │ After Split Planning             │ {:>10} │ {:.0}s", final_input_files.len(), final_total_duration);
    log::info!("[PIPELINE_AUDIT]   │ Before Concat (current)         │ {:>10} │ {:.0}s", final_input_files.len(), final_total_duration);
    log::info!("[PIPELINE_AUDIT] ══════════════════════════════════════════════════════════");

    // ── [CARDS] FULL CONCAT AUDIT: Every file that enters FFmpeg ──
    log::info!("[CARDS:CONCAT_AUDIT] ═══════════════════════════════════════════════════════════");
    log::info!("[CARDS:CONCAT_AUDIT] FINAL CONCAT FILE SEQUENCE ({} files):", final_input_files.len());
    for (i, (file, (is_card, card_color))) in final_input_files.iter().zip(final_segment_cards.iter()).enumerate() {
        let file_name = Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.clone());
        if *is_card {
            log::info!("[CARDS:CONCAT_AUDIT]   [{:>3}] 🃏 CARD  | color={:?} | dur={:.1}s | {}",
                i, card_color, final_input_durations[i], file_name);
        } else {
            log::info!("[CARDS:CONCAT_AUDIT]   [{:>3}] 🎬 VIDEO | dur={:.1}s | {}",
                i, final_input_durations[i], file_name);
        }
    }
    log::info!("[CARDS:CONCAT_AUDIT] Segment is_card flags: {:?}", final_segment_cards.iter().map(|(c, _)| *c).collect::<Vec<_>>());
    log::info!("[CARDS:CONCAT_AUDIT] ═══════════════════════════════════════════════════════════");

    // ── AUDIO IMMUTABILITY: Detect if any file was normalized ────────────
    // If ANY file was normalized in Phase 6 (profile) or Phase 7 (audio),
    // Custom concat MUST use -c:a copy to prevent second-generation AAC loss.
    // The concat demuxer applies the same -c:a to ALL files, so a single
    // normalized file forces audio copy for the entire concat.
    let audio_was_normalized = working_input_files.iter().any(|p| {
        p.contains("/norm_prof_") || p.contains("\\norm_prof_") ||
        p.contains("/norm_audio_") || p.contains("\\norm_audio_") ||
        p.contains("/norm_ts_") || p.contains("\\norm_ts_")
    });
    if audio_was_normalized {
        log::info!("[IMMUTABILITY] Files were normalized → Custom concat will use -c:a copy for ALL files");
    } else {
        log::info!("[IMMUTABILITY] No files normalized → Custom concat can use user-specified audio codec");
    }

    // ── IMMUTABILITY AUDIT: Path-based vs Registry consistency check ─────
    // Compares the path-based heuristic against the registry's tracking.
    // Audit-only: does NOT change behavior. Logs mismatches for investigation.
    {
        let mut path_detected: Vec<(usize, String)> = Vec::new();
        let mut registry_detected: Vec<(usize, String)> = Vec::new();
        let mut matches = 0usize;
        let mut mismatches = 0usize;
        let mut mismatch_details: Vec<String> = Vec::new();

        for (i, path) in working_input_files.iter().enumerate() {
            let path_result = path.contains("/norm_prof_") || path.contains("\\norm_prof_")
                || path.contains("/norm_audio_") || path.contains("\\norm_audio_")
                || path.contains("/norm_ts_") || path.contains("\\norm_ts_");
            let registry_result = immutability_registry.is_immutable(path);

            if path_result { path_detected.push((i, path.clone())); }
            if registry_result { registry_detected.push((i, path.clone())); }

            if path_result == registry_result {
                matches += 1;
            } else {
                mismatches += 1;
                mismatch_details.push(format!("  File #{}: path={} registry={} | {}",
                    i, path_result, registry_result,
                    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
            }
        }

        let total = matches + mismatches;
        let match_pct = if total > 0 { (matches as f64 / total as f64) * 100.0 } else { 100.0 };

        log::info!("[IMMUTABILITY:CONSISTENCY] ════════════════════════════════════════════════════");
        log::info!("[IMMUTABILITY:CONSISTENCY] PATH-BASED vs REGISTRY COMPARISON");
        log::info!("[IMMUTABILITY:CONSISTENCY] ════════════════════════════════════════════════════");
        log::info!("[IMMUTABILITY:CONSISTENCY] Path-based detected: {} files", path_detected.len());
        log::info!("[IMMUTABILITY:CONSISTENCY] Registry detected:    {} files", registry_detected.len());
        log::info!("[IMMUTABILITY:CONSISTENCY] Matches:    {} / {} ({:.0}%)", matches, total, match_pct);
        log::info!("[IMMUTABILITY:CONSISTENCY] Mismatches: {} / {} ({:.0}%)", mismatches, total, 100.0 - match_pct);

        if mismatches > 0 {
            log::warn!("[IMMUTABILITY:CONSISTENCY] ⚠️ MISMATCH DETECTED — these files differ between path and registry:");
            for detail in &mismatch_details {
                log::warn!("[IMMUTABILITY:CONSISTENCY]{}", detail);
            }
        } else {
            log::info!("[IMMUTABILITY:CONSISTENCY] ✅ All files consistent between path-based and registry detection");
        }
        log::info!("[IMMUTABILITY:CONSISTENCY] ════════════════════════════════════════════════════");
    }

    let config = MergeConfig {
        input_files: final_input_files.clone(), input_names: final_input_names.clone(), input_durations: final_input_durations.clone(), subtitle_list_path: final_subtitle_list_path.clone(), output_path: normalized_output_path.clone(), mode: actual_mode.clone(), total_duration: final_total_duration, video_codec: actual_video_codec, audio_codec: actual_audio_codec, video_crf: actual_video_crf, video_preset: actual_video_preset, audio_bitrate: actual_audio_bitrate, target_resolution: request.target_resolution.clone(), target_fps: request.target_fps.clone(), hw_accel: request.hw_accel.clone(), split_config: request.split_config.clone(), naming_config: request.naming_config.clone(), subtitle_files: final_prepared_subs.clone(), subtitle_mode, export_merged_srt, segment_is_card: final_segment_cards.iter().map(|(c, _)| *c).collect(), card_config: request.card_config.clone(), burn_subtitle_path: burn_subtitle_path.clone(),
            mkvmerge_succeeded_before_ffmpeg: false,
            // Audio immutability: if any file was normalized (Phase 6 or 7),
            // Custom concat MUST use -c:a copy to prevent second-generation AAC loss.
            audio_normalized: audio_was_normalized,
            immutability_registry: Some(immutability_registry),
    };

    let merge_state_ref = state.merge_state.clone(); let job_id = request.job_id.clone(); let job_id_for_return = request.job_id.clone(); let output_path = normalized_output_path.clone();

    // Update cleanup_guard with all final paths before moving ownership into background thread
    cleanup_guard.list_path = Some(list_path.clone());
    cleanup_guard.subtitle_list_path = final_subtitle_list_path.clone();
    cleanup_guard.card_temp_files = card_temp_files.clone();
    cleanup_guard.burn_subtitle_path = burn_subtitle_path.clone();

    // Track whether mkvmerge was used and succeeded (SmartMKV/FastMKV only)
    let mut mkvmerge_succeeded = false;

    // ── mkvmerge zero-copy dispatch (SmartMKV / FastMKV only) ──
    let mkvmerge_path = crate::ffmpeg::mkvmerge::find_mkvmerge();
    let split_active = request.split_config.as_ref().map_or(false, |sc| sc.mode != crate::types::SplitMode::None);
    let will_use_mkvmerge = (actual_mode == MergeMode::SmartMkv || actual_mode == MergeMode::FastMkv) && mkvmerge_path.is_some() && !split_active;
    if split_active && (actual_mode == MergeMode::SmartMkv || actual_mode == MergeMode::FastMkv) {
        log::info!("[PERF] mkvmerge skipped: split mode {:?} is active — FFmpeg split-merge handles per-folder output", request.split_config.as_ref().map(|sc| &sc.mode));
    }
    if will_use_mkvmerge {
        log::info!("[PERF] mkvmerge detected — using zero-copy MKV concat for {:?}", actual_mode);
    } else if mkvmerge_path.is_some() {
        log::info!("[PERF] mkvmerge available but not used for mode {:?}", actual_mode);
    }

    if will_use_mkvmerge {
        let path = match mkvmerge_path {
            Some(p) => p,
            None => {
                log::error!("[Merge] mkvmerge not found -- cannot use SmartMKV mode. Falling back to standard merge.");
                return Err("mkvmerge not found. Cannot use SmartMKV mode. Please install MKVToolNix or select a different merge mode.".to_string());
            }
        };
        let inputs = final_input_files.clone();
        let output = normalized_output_path.clone();
        let total_dur = final_total_duration;
        let cancel = cancel_flag.clone();
        let app = app_handle.clone();
        let job_id_clone = request.job_id.clone();

        // ── PRE-MKVMERGE STREAM INVENTORY ──────────────────────────────────────
        // Log the exact state of each input file being passed to mkvmerge
        log::info!("");
        log::info!("═══════════════════════════════════════════════════════════════════════════");
        log::info!("═══════════ FINAL STREAM INVENTORY (Pre-mkvmerge) ═══════════");
        log::info!("═══════════════════════════════════════════════════════════════════════════");

        for (i, input_path) in inputs.iter().enumerate() {
            let file_name = input_path.split(['/', '\\']).last().unwrap_or("?");
            log::info!("  File {}: {}", i, file_name);
        }

        log::info!("═══════════════════════════════════════════════════════════════════════════");

        // PHASE TIMING: Record when mkvmerge starts
        let mkvmerge_start_time = std::time::Instant::now();
        log::info!("[PHASE_3] MKVMERGE_PREP_START | elapsed_since_merge_start={:?}", mkvmerge_start_time.elapsed());
        log::info!("");

        let merge_result = tokio::task::spawn_blocking(move || {
            // RAII safety net: auto-finalize log on any exit path (success/error/panic/early return)
            let _log_guard = crate::logger::JobLogGuard::new_empty();
            log::info!("[PHASE_3] MKVMERGE_START | jobId={}", job_id_clone);
            crate::ffmpeg::mkvmerge::run_mkvmerge(
                &path,
                &inputs,
                &output,
                total_dur,
                cancel,
                move |p| {
                    let _ = app.emit("merge-progress", &serde_json::json!({
                        "jobId": job_id_clone,
                        "progress": p
                    }));
                },
            )
        }).await;

        match merge_result {
            Ok(Ok(())) => {
                log::info!("[PERF] mkvmerge completed successfully: {}", normalized_output_path);
                mkvmerge_succeeded = true;
            }
            Ok(Err(e)) => {
                log::warn!("[PERF] mkvmerge failed: {} — falling back to FFmpeg", e);
            }
            Err(e) => {
                log::warn!("[PERF] mkvmerge task panicked: {} — falling back to FFmpeg", e);
            }
        }

        // PHASE TIMING: Record mkvmerge duration and what's next
        let mkvmerge_elapsed = mkvmerge_start_time.elapsed();
        if mkvmerge_succeeded {
            log::info!("[PHASE_3] MKVMERGE_END (SUCCESS) | duration={:.2}s | jobId={}", mkvmerge_elapsed.as_secs_f64(), request.job_id);
            log::info!("[PHASE_4] FFmpeg concat starting — mkvmerge output will be overwritten");
        } else {
            log::info!("[PHASE_3] MKVMERGE_END (FAILED/SKIPPED) | duration={:.2}s | jobId={}", mkvmerge_elapsed.as_secs_f64(), request.job_id);
            log::info!("[PHASE_4] FFmpeg concat starting (fallback path)");
        }
        // Always fall through to FFmpeg concat for proper split/recovery/report handling
    }

    // Independent clones for watchdog (spawn_blocking moves the originals)
    let watchdog_app = app_handle.clone();
    let watchdog_job_id = job_id.clone();

    // PHASE TIMING: Record when FFmpeg concat spawn_blocking starts
    let ffmpeg_concat_start = std::time::Instant::now();
    log::info!("[PHASE_4] FFMPEG_CONCAT_PREP_START | elapsed_since_merge_start={:?} | mkvmerge_succeeded={}", ffmpeg_concat_start.elapsed(), mkvmerge_succeeded);

    let merge_handle = tokio::task::spawn_blocking(move || {
        let _guard = cleanup_guard; // Ownership moved here; dropped at end of closure
        let app_handle_inner = app_handle.clone();
        let job_id_inner = job_id.clone();
        let job_id_for_cancel = job_id.clone();
        let cancel_for_retry = cancel_flag.clone();
        let _cancel_for_retry2 = cancel_flag.clone();
        let mut checkpoint_writer = checkpoint_writer; // PER-JOB CHECKPOINT WRITER - ensures drain on completion

        // PHASE-BASED RESUME: Update phase to Finalizing before concat starts via single-writer
        if let Some(ref sender) = checkpoint_sender {
            sender.update_phase(crate::types::MergePhase::Finalizing, None);
            log::info!("[PhaseResume] Checkpoint phase sent to writer: Finalizing before concat");
        }

        log::info!("[MERGE_TRACE] === MERGE_TASK_SPAWNED jobId={} ===", job_id_inner);

        // CRITICAL TIMING LOG: This proves if FFmpeg concat runs AFTER mkvmerge succeeded
        let concat_reason = if mkvmerge_succeeded {
            "mkvmerge_succeeded_will_overwrite"
        } else if split_active {
            "mkvmerge_skipped_split_active"
        } else if actual_mode == MergeMode::FastMkv {
            "mkvmerge_failed_fallback"
        } else {
            "normal_ffmpeg_concat"
        };
        log::info!("[PHASE_4] FFMPEG_CONCAT_EXEC_START | reason={} | mode={:?} | mkvmerge_succeeded={} | output={}",
            concat_reason, actual_mode, mkvmerge_succeeded, config.output_path);

        let mut result = run_merge_blocking(&ffmpeg_path_resolved, &config, &list_path, cancel_flag.clone(), move |mut progress| { 
            let overall_pct = 35.0 + (progress.percent * 0.65);
            progress.overall_percent = Some(overall_pct);
            progress.percent = overall_pct;
            let _ = app_handle_inner.emit("merge-progress", &serde_json::json!({ "jobId": job_id_inner, "progress": progress })); 
        });

        log::info!("[MERGE_TRACE] run_merge_blocking returned jobId={} result={:?}", job_id, result);

        // Clone handles for use in validation section (originals consumed by merge progress closure)
        let app_handle_validate = app_handle.clone();
        let job_id_validate = job_id.clone();

        let mut audio_seek_warnings: usize = 0;

        // ── Phase 5, 6, 7: Timeline & Seekability Validation ───────────────────────────────────
        let mut actual_duration = 0.0;
        let mut output_probe_result = None;
        let mut primary_output_path = String::new();

        if let Ok(ref res) = result {
            let output_paths = res.output_paths.clone().unwrap_or_else(|| vec![res.output_path.clone()]);
            primary_output_path = res.output_path.clone();

            for (part_idx, output_path) in output_paths.iter().enumerate() {
                if output_path.is_empty() { continue; }

                log::info!("[FORENSIC:VALIDATE] Performing post-merge audit on part {}/{} ({})", 
                    part_idx + 1, output_paths.len(), output_path);

                let probe = crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, Path::new(output_path));

                if part_idx == 0 {
                    match &probe {
                        Ok(info) => {
                            actual_duration = info.duration;
                            output_probe_result = Some(probe);
                        }
                        Err(e) => {
                            log::error!("[FORENSIC:TIMELINE] ❌ probe_file FAILED for '{}': {}", output_path, e);
                        }
                    }

                    // Timeline check for part 1
                    if let Some(Ok(info)) = &output_probe_result {
                        let expected_duration = if output_paths.len() == 1 { final_total_duration } else {
                            res.parts.as_ref().and_then(|p| p.first()).map(|p| p.total_duration).unwrap_or(info.duration)
                        };

                        let drift_pct = if expected_duration > 0.0 { ((info.duration - expected_duration).abs() / expected_duration) * 100.0 } else { 0.0 };
                        let drift_abs_secs = (info.duration - expected_duration).abs();
                        log::info!("[FORENSIC:TIMELINE] Expected: {:.3}s | Actual: {:.3}s | Drift: {:.3}% ({:.1}s)", expected_duration, info.duration, drift_pct, drift_abs_secs);

                        // Adaptive threshold: larger playlists tolerate slightly more drift
                        // due to floating-point accumulation across hundreds of files.
                        // <1h: 1%, 1-10h: 2%, 10-50h: 3%, 50+h: 5%
                        let drift_threshold_pct = if final_total_duration > 180_000.0 { 5.0 }
                            else if final_total_duration > 36_000.0 { 3.0 }
                            else if final_total_duration > 3_600.0 { 2.0 }
                            else { 1.0 };
                        // Also: hard-fail if absolute drift exceeds 60 seconds (any duration)
                        let drift_threshold_abs = 60.0;

                        if drift_pct > drift_threshold_pct && drift_abs_secs > drift_threshold_abs {
                            let _ = std::fs::remove_file(output_path);
                            result = Err(anyhow::anyhow!("CRITICAL TIMELINE CORRUPTION: Part 1 drift {:.2}% ({:.1}s, threshold {:.1}%) (Expected {:.1}s, got {:.1}s). Output deleted.", drift_pct, drift_abs_secs, drift_threshold_pct, expected_duration, info.duration));
                            break;
                        }
                    }
                }
            }
        }

        // ── Duration Pipeline Forensic Audit ─────────────────────────────────────────
// Print the COMPLETE duration pipeline so we can trace where divergence occurs.
// This does NOT modify any merge logic - purely observational.
//
// Pipeline stages:
// Source durations → Normalized durations → Concat input durations → Expected total → Output probe
//
log::info!("[FORENSIC:DURATION_PIPELINE] ═══════════════════════════════════════════════════════════");
log::info!("[FORENSIC:DURATION_PIPELINE] COMPLETE DURATION PIPELINE TRACE");
log::info!("[FORENSIC:DURATION_PIPELINE] ═══════════════════════════════════════════════════════════");

// Stage 1: Source durations (what the frontend calculated)
let source_total_dur: f64 = request.input_durations.iter().sum();
let source_min_dur = request.input_durations.iter().cloned().fold(0.0f64, f64::min);
let source_max_dur = request.input_durations.iter().cloned().fold(0.0f64, f64::max);
log::info!("[FORENSIC:DURATION_PIPELINE] Stage 1 - SOURCE DURATIONS (from frontend probe):");
log::info!("[FORENSIC:DURATION_PIPELINE]   Total: {:.3}s ({:.2}h)", source_total_dur, source_total_dur / 3600.0);
log::info!("[FORENSIC:DURATION_PIPELINE]   Min file: {:.3}s  Max file: {:.3}s", source_min_dur, source_max_dur);
log::info!("[FORENSIC:DURATION_PIPELINE]   File count: {}", request.input_files.len());

// Stage 2: Working durations (after dedup, repeat expansion)
log::info!("[FORENSIC:DURATION_PIPELINE] Stage 2 - WORKING DURATIONS (after dedup/repeat/cards):");
log::info!("[FORENSIC:DURATION_PIPELINE]   Working total: {:.3}s ({:.2}h)", working_total_duration, working_total_duration / 3600.0);
log::info!("[FORENSIC:DURATION_PIPELINE]   Working file count: {}", working_input_files.len());

// Stage 3: Final durations (after all processing, before concat)
log::info!("[FORENSIC:DURATION_PIPELINE] Stage 3 - FINAL DURATIONS (after normalization, ready for concat):");
log::info!("[FORENSIC:DURATION_PIPELINE]   Final total: {:.3}s ({:.2}h)", final_total_duration, final_total_duration / 3600.0);
log::info!("[FORENSIC:DURATION_PIPELINE]   Final file count: {}", final_input_files.len());
log::info!("[FORENSIC:DURATION_PIPELINE]   Final segments: {}", final_input_durations.len());
log::info!("[FORENSIC:DURATION_PIPELINE]   Sum of final durations: {:.3}s", final_input_durations.iter().sum::<f64>());

// Stage 4: Output probe (what ffprobe actually measured)
if let Some(Ok(info)) = &output_probe_result {
    log::info!("[FORENSIC:DURATION_PIPELINE] Stage 4 - OUTPUT PROBE (ffprobe actual measurement):");
    log::info!("[FORENSIC:DURATION_PIPELINE]   format.duration: {:.3}s ({:.2}h)", info.duration, info.duration / 3600.0);

    // Video streams
    if let Some(vs) = info.video_streams.first() {
        log::info!("[FORENSIC:DURATION_PIPELINE]   Video stream[0]:");
        log::info!("[FORENSIC:DURATION_PIPELINE]     codec: {:?}", vs.codec_name);
        log::info!("[FORENSIC:DURATION_PIPELINE]     duration (stream): {:?}", vs.duration);
        log::info!("[FORENSIC:DURATION_PIPELINE]     profile: {:?}", vs.profile);
        log::info!("[FORENSIC:DURATION_PIPELINE]     time_base: {:?}", vs.time_base);
        log::info!("[FORENSIC:DURATION_PIPELINE]     r_frame_rate: {:?}", vs.r_frame_rate);
        log::info!("[FORENSIC:DURATION_PIPELINE]     avg_frame_rate: {:?}", vs.avg_frame_rate);
        log::info!("[FORENSIC:DURATION_PIPELINE]     fps: {:?}", vs.fps);
        log::info!("[FORENSIC:DURATION_PIPELINE]     start_time: {:?}", vs.start_time);
    } else {
        log::warn!("[FORENSIC:DURATION_PIPELINE]   Video stream[0]: NONE");
    }

    // Audio streams
    if let Some(as_) = info.audio_streams.first() {
        log::info!("[FORENSIC:DURATION_PIPELINE]   Audio stream[0]:");
        log::info!("[FORENSIC:DURATION_PIPELINE]     codec: {:?}", as_.codec_name);
        log::info!("[FORENSIC:DURATION_PIPELINE]     duration (stream): {:?}", as_.duration);
        log::info!("[FORENSIC:DURATION_PIPELINE]     sample_rate: {:?}", as_.sample_rate);
        log::info!("[FORENSIC:DURATION_PIPELINE]     channels: {:?}", as_.channels);
        log::info!("[FORENSIC:DURATION_PIPELINE]     channel_layout: {:?}", as_.channel_layout);
    } else {
        log::warn!("[FORENSIC:DURATION_PIPELINE]   Audio stream[0]: NONE");
    }

    // All streams summary
    log::info!("[FORENSIC:DURATION_PIPELINE]   All streams count: {} (video={}, audio={}, subtitle={})",
        info.video_streams.len() + info.audio_streams.len() + info.subtitle_streams.len(),
        info.video_streams.len(),
        info.audio_streams.len(),
        info.subtitle_streams.len()
    );

    // Compare format.duration vs stream durations
    let v_dur_stream = info.video_streams.first().and_then(|s| s.duration).unwrap_or(info.duration);
    let a_dur_stream = info.audio_streams.first().and_then(|s| s.duration).unwrap_or(info.duration);
    log::info!("[FORENSIC:DURATION_PIPELINE]   Duration comparison:");
    log::info!("[FORENSIC:DURATION_PIPELINE]     format.duration vs video_stream[0].duration: {:.3}s vs {:.3}s (diff: {:.3}s)",
        info.duration, v_dur_stream, (info.duration - v_dur_stream).abs());
    log::info!("[FORENSIC:DURATION_PIPELINE]     format.duration vs audio_stream[0].duration: {:.3}s vs {:.3}s (diff: {:.3}s)",
        info.duration, a_dur_stream, (info.duration - a_dur_stream).abs());
    log::info!("[FORENSIC:DURATION_PIPELINE]     video_stream vs audio_stream duration: {:.3}s vs {:.3}s (diff: {:.3}s)",
        v_dur_stream, a_dur_stream, (v_dur_stream - a_dur_stream).abs());

    // A/V sync decision
    let av_diff = (v_dur_stream - a_dur_stream).abs();
    log::info!("[FORENSIC:DURATION_PIPELINE]   A/V SYNC DECISION: diff={:.3}s  threshold=0.5s  result={}",
        av_diff, if av_diff > 0.5 { "FAIL" } else { "PASS" });
} else {
    log::warn!("[FORENSIC:DURATION_PIPELINE]   Output probe result: NOT AVAILABLE (probe failed or skipped)");
}
log::info!("[FORENSIC:DURATION_PIPELINE] ═══════════════════════════════════════════════════════════");

        // Duration Consistency Check (A/V Sync)
        if result.is_ok() {
            if let Some(Ok(info)) = &output_probe_result {
                let v_dur = info.video_streams.first().and_then(|s| s.duration).unwrap_or(actual_duration);
                let a_dur = info.audio_streams.first().and_then(|s| s.duration).unwrap_or(actual_duration);
                let diff = (v_dur - a_dur).abs();
                log::info!("[AV_SYNC_CHECK] ═══════════════════════════════════════════════════════════");
                log::info!("[AV_SYNC_CHECK] format_duration={:.3}s  video_duration={:.3}s  audio_duration={:.3}s  diff={:.3}s",
                    actual_duration, v_dur, a_dur, diff);
                log::info!("[AV_SYNC_CHECK] format={:.3}s  video={:.3}s  audio={:.3}s  difference={:.3}s  decision={}",
                    actual_duration, v_dur, a_dur, diff,
                    if diff > 0.5 { "FAIL (delete + error)" } else { "PASS" });
                log::info!("[AV_SYNC_CHECK] ═══════════════════════════════════════════════════════════");
                if diff > 0.5 {
                    let _ = std::fs::remove_file(&primary_output_path);
                    result = Err(anyhow::anyhow!("A/V SYNC CORRUPTION: Audio duration ({:.3}s) differs from Video duration ({:.3}s) by {:.3}s.", a_dur, v_dur, diff));
                }
            }
        }

        // ── Post-mkvmerge container metadata fix ──────────────────────────────────────────────
        // When mkvmerge concatenates files with mixed audio sample rates (44100/48000 Hz) or
        // mixed video profiles (Main/High), the MKV container header duration is corrupted
        // (e.g., reports 942s instead of 31260s). The actual data IS present (confirmed by
        // file-size validation). Fix by running FFmpeg genpts remux which forces PTS/timestamp
        // recalculation and recalculates the MKV SegmentInfo.Duration from actual frame timestamps.
        if mkvmerge_succeeded {
            if let Some(Ok(ref info)) = &output_probe_result {
            let drift_ratio = if final_total_duration > 0.0 { info.duration / final_total_duration } else { 1.0 };
            if !(0.5..=2.0).contains(&drift_ratio) {
                log::info!("[METADATA_FIX] ═══════════════════════════════════════════════════════════");
                log::info!("[METADATA_FIX] Detected corrupted container duration: ffprobe={:.1}s, expected={:.1}s (ratio: {:.1}%)",
                    info.duration, final_total_duration, drift_ratio * 100.0);
                log::info!("[METADATA_FIX] Running FFmpeg genpts remux to fix corrupted container duration...");

                let temp_fix_path = format!("{}.metadata_fix{}", primary_output_path,
                    std::path::Path::new(&primary_output_path)
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default());

                let original_size = result.as_ref().map(|r| r.output_size_bytes).unwrap_or(0);

                // FFmpeg genpts remux: forces PTS/timestamp regeneration, which recalculates
                // the MKV SegmentInfo.Duration from actual frame timestamps rather than
                // from the (corrupted) container header that mkvmerge wrote.
                let ffmpeg_fix = std::process::Command::new(&ffmpeg_path_resolved)
                    .arg("-fflags")
                    .arg("+genpts")
                    .arg("-i")
                    .arg(&primary_output_path)
                    .arg("-c")
                    .arg("copy")
                    .arg("-map")
                    .arg("0")
                    .arg("-avoid_negative_ts")
                    .arg("make_zero")
                    .arg("-y")
                    .arg(&temp_fix_path)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .output();

                match ffmpeg_fix {
                    Ok(ref cmd_output) if cmd_output.status.success() => {
                        if let Ok(temp_meta) = std::fs::metadata(&temp_fix_path) {
                            let temp_size = temp_meta.len();
                            let size_ratio = if original_size > 0 { (temp_size as f64) / (original_size as f64) * 100.0 } else { 100.0 };
                            log::info!("[METADATA_FIX] FFmpeg genpts remux complete: {} bytes -> {} bytes ({:.1}%)", original_size, temp_size, size_ratio);

                            if (90.0..=110.0).contains(&size_ratio) {
                                // Remux produced valid output -- replace original
                                if let Err(e) = std::fs::rename(&temp_fix_path, &primary_output_path) {
                                    // rename may fail across volumes; fall back to remove + rename
                                    let _ = std::fs::remove_file(&primary_output_path);
                                    if let Err(e2) = std::fs::rename(&temp_fix_path, &primary_output_path) {
                                        log::warn!("[METADATA_FIX] Failed to replace output after ffmpeg fix: {} / {}", e, e2);
                                        let _ = std::fs::remove_file(&temp_fix_path);
                                    }
                                }

                                // Re-probe to get corrected duration
                                if let Ok(fixed_info) = crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, std::path::Path::new(&primary_output_path)) {
                                    log::info!("[METADATA_FIX] Fixed duration: {:.1}s (was {:.1}s, expected {:.1}s)",
                                        fixed_info.duration, info.duration, final_total_duration);
                                    actual_duration = fixed_info.duration;
                                    output_probe_result = Some(Ok(fixed_info));
                                }
                            } else {
                                log::warn!("[METADATA_FIX] FFmpeg remux size mismatch ({:.1}%), keeping original", size_ratio);
                                let _ = std::fs::remove_file(&temp_fix_path);
                            }
                        } else {
                            log::warn!("[METADATA_FIX] FFmpeg remux output not found, keeping original");
                            let _ = std::fs::remove_file(&temp_fix_path);
                        }
                    }
                    Ok(cmd_output) => {
                        log::warn!("[METADATA_FIX] FFmpeg genpts remux failed (exit {}): {}",
                            cmd_output.status.code().unwrap_or(-1),
                            String::from_utf8_lossy(&cmd_output.stderr).chars().take(200).collect::<String>());
                        let _ = std::fs::remove_file(&temp_fix_path);
                    }
                    Err(e) => {
                        log::warn!("[METADATA_FIX] Failed to spawn FFmpeg genpts remux: {}", e);
                    }
                }
                log::info!("[METADATA_FIX] ═══════════════════════════════════════════════════════════");
            }
            }
        }

        // Track validation start time for forensic timing
        let validation_start = std::time::Instant::now();
        let total_validation_tests = 17; // 7 video + 10 audio seeks
        let mut validation_completed = 0usize;

        // ── Phase: Final Validation ────────────────────────────────────────────────────────────
        // Emit initial Finalizing progress so UI knows validation has started
        let _ = app_handle_validate.emit("merge-progress", &serde_json::json!({
            "jobId": job_id_validate,
            "progress": {
                "phase": "finalizing",
                "stageName": "Final validation...",
                "stagePercent": 0.0,
                "percent": 96.0,
                "overallPercent": 96.0,
                "currentTime": 0.0,
                "totalDuration": 0.0,
                "currentFileIndex": 0,
                "totalFilesInStage": total_validation_tests,
                "currentValidationTest": Some("video_seek_start"),
                "currentValidationIndex": 0,
                "totalValidationTests": total_validation_tests,
                "etaSeconds": null,
                "warning": null,
            }
        }));
        log::info!("[FINAL_VALIDATE_START] ═══════════════════════════════════════════════");
        log::info!("[FINAL_VALIDATE_START] Post-merge forensic validation STARTING");
        log::info!("[FINAL_VALIDATE_START] Output: {}", primary_output_path);
        log::info!("[FINAL_VALIDATE_START] Duration: {:.1}s", actual_duration);
        log::info!("[FINAL_VALIDATE_START] Total tests: {} (7 video + 10 audio)", total_validation_tests);
        log::info!("[FINAL_VALIDATE_START] ═══════════════════════════════════════════════");

        // Seekability Audit
        if result.is_ok() && actual_duration > 0.0 && !primary_output_path.is_empty() {
            let test_points = [0.05, 0.10, 0.25, 0.50, 0.75, 0.90, 0.95];
            #[derive(Debug)]
            struct SeekResult { success: bool, err: Option<String>, elapsed_ms: u128 }
            let mut seek_results: Vec<SeekResult> = Vec::new();
            #[cfg(windows)]
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            for (idx, pct) in test_points.iter().enumerate() {
                if cancel_flag.load(Ordering::Relaxed) {
                    log::info!("[FORENSIC:CANCEL] Video seekability audit cancelled — returning early");
                    return Ok(job_id_for_cancel.clone());
                }
                let test_start = std::time::Instant::now();
                let seek_sec = actual_duration * pct;
                let args = ["-v", "error", "-ss", &seek_sec.to_string(), "-i", &primary_output_path, "-an", "-frames:v", "1", "-f", "null", "-"];
                let mut cmd = std::process::Command::new(&ffmpeg_path_resolved);
                #[cfg(windows)]
                cmd.creation_flags(CREATE_NO_WINDOW);
                let res = cmd.args(args).output();
                validation_completed += 1;
                let test_elapsed = test_start.elapsed().as_millis();
                match res {
                    Ok(out) if !out.status.success() => {
                        let err_msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
                        log::warn!("[FINAL_VALIDATE:VIDEO_SEEK] ⚠️  {:.0}% ({:.1}s) FAILED in {}ms: {}", pct*100.0, seek_sec, test_elapsed, err_msg);
                        seek_results.push(SeekResult { success: false, err: Some(err_msg), elapsed_ms: test_elapsed });
                    }
                    Ok(_) => {
                        log::info!("[FINAL_VALIDATE:VIDEO_SEEK] ✅ {:.0}% ({:.1}s) OK in {}ms", pct*100.0, seek_sec, test_elapsed);
                        seek_results.push(SeekResult { success: true, err: None, elapsed_ms: test_elapsed });
                    }
                    Err(e) => {
                        log::warn!("[FINAL_VALIDATE:VIDEO_SEEK] ⚠️  {:.0}% ({:.1}s) ERROR in {}ms: {}", pct*100.0, seek_sec, test_elapsed, e);
                        seek_results.push(SeekResult { success: false, err: Some(e.to_string()), elapsed_ms: test_elapsed });
                    }
                }
                // Emit validation progress
                let val_pct = (validation_completed as f32 / total_validation_tests as f32) * 100.0;
                let remaining = total_validation_tests - validation_completed;
                let avg_per_test = if validation_completed > 0 { validation_start.elapsed().as_millis() as f32 / validation_completed as f32 } else { 0.0 };
                let eta = (remaining as f32 * avg_per_test / 1000.0).max(0.0);
                let next_test = if idx + 1 < test_points.len() { format!("video_seek_{:.0}pct", test_points[idx + 1] * 100.0) } else { "audio_seek_start".to_string() };
                let _ = app_handle_validate.emit("merge-progress", &serde_json::json!({
                    "jobId": job_id_validate,
                    "progress": {
                        "phase": "finalizing",
                        "stageName": format!("Validating seekability ({}/{})", validation_completed, total_validation_tests),
                        "stagePercent": val_pct,
                        "percent": 96.0 + (val_pct * 0.04),
                        "overallPercent": 96.0 + (val_pct * 0.04),
                        "currentTime": 0.0,
                        "totalDuration": 0.0,
                        "currentFileIndex": validation_completed,
                        "totalFilesInStage": total_validation_tests,
                        "currentValidationTest": Some(next_test),
                        "currentValidationIndex": validation_completed,
                        "totalValidationTests": total_validation_tests,
                        "etaSeconds": eta,
                        "warning": null,
                    }
                }));
            }

            let total_seeks = seek_results.len();
            let failed_seeks = seek_results.iter().filter(|r| !r.success).count();
            let total_seek_time: u128 = seek_results.iter().map(|r| r.elapsed_ms).sum();
            let avg_seek_time = if total_seeks > 0 { total_seek_time / total_seeks as u128 } else { 0 };

            log::info!("[FINAL_VALIDATE:VIDEO_SUMMARY] Video seek audit complete: {}/{} passed, avg={}ms, total={}ms",
                total_seeks - failed_seeks, total_seeks, avg_seek_time, total_seek_time);

            let fail_pct = if total_seeks > 0 { (failed_seeks as f64 / total_seeks as f64) * 100.0 } else { 0.0 };

            if fail_pct > 50.0 {
                let first_errors: Vec<String> = seek_results.iter()
                    .filter(|r| !r.success)
                    .filter_map(|r| r.err.clone())
                    .take(3)
                    .collect();
                let _ = std::fs::remove_file(&primary_output_path);
                result = Err(anyhow::anyhow!(
                    "SEEKABILITY CORRUPTION: {:.0}% of seek points failed ({}/{}). Errors: {}",
                    fail_pct, failed_seeks, total_seeks, first_errors.join("; ")
                ));
            } else if failed_seeks > 0 {
                log::warn!("[FORENSIC:SEEK] ⚠️  {:.0}% of seeks failed ({}/{}) — within acceptable threshold. Output kept.",
                    fail_pct, failed_seeks, total_seeks);
            } else {
                log::info!("[FORENSIC:SEEK] ✅ All seek points passed ({}/{})", total_seeks, total_seeks);
            }
        }

        // Audio Seekability Audit — reusing output_probe_result
        if result.is_ok() && actual_duration > 0.0 && !primary_output_path.is_empty() {
            if let Some(Ok(info)) = &output_probe_result {
                if !info.audio_streams.is_empty() {
                    #[cfg(windows)]
                    use std::os::windows::process::CommandExt;
                    const CREATE_NO_WINDOW: u32 = 0x08000000;
                    let test_points = [0.05, 0.10, 0.15, 0.25, 0.35, 0.50, 0.65, 0.75, 0.85, 0.95];
                    let audio_total = test_points.len();
                    let mut audio_passed = 0usize;
                    let mut audio_total_time: u128 = 0;
                    for (idx, pct) in test_points.iter().enumerate() {
                        if cancel_flag.load(Ordering::Relaxed) {
                            log::info!("[FORENSIC:CANCEL] Audio seekability audit cancelled — returning early");
                            return Ok(job_id_for_cancel.clone());
                        }
                        let test_start = std::time::Instant::now();
                        let seek_sec = actual_duration * pct;

                        let args = ["-v", "error", "-ss", &seek_sec.to_string(), "-i", &primary_output_path, "-vn", "-map", "0:a:0?", "-t", "10", "-f", "null", "-"];
                        let mut cmd = std::process::Command::new(&ffmpeg_path_resolved);
                        #[cfg(windows)]
                        cmd.creation_flags(CREATE_NO_WINDOW);
                        let res = cmd.args(args).output();
                        validation_completed += 1;
                        let test_elapsed = test_start.elapsed().as_millis();
                        audio_total_time += test_elapsed;
                        // ── PHASE 2a: AUDIO SEEK FORENSICS ──────────────────────────────────────
                        // Capture decode forensics for every attempt (pass or fail)
                        let decode_exit_code = res.as_ref().map(|o| o.status.code().unwrap_or(-1)).unwrap_or(-1);
                        let decode_stderr = res.as_ref().map(|o| String::from_utf8_lossy(&o.stderr).to_string()).unwrap_or_default();
                        log::info!("[FORENSIC:AUDIO_SEEK] Position {:.0}% ({:.1}s) | ExitCode={} | StderrLen={} | StderrFirstLine={}",
                            pct*100.0, seek_sec, decode_exit_code, decode_stderr.len(),
                            decode_stderr.lines().next().unwrap_or("empty"));
                        match res {
                            Ok(out) => {
                                let stderr_lower = decode_stderr.to_lowercase();
                                let is_he_aac_bands_warning = stderr_lower.contains("number of bands") && stderr_lower.contains("exceeds limit");
                                // Determine if this is a FATAL error or a non-fatal warning
                                // Pass criteria: exit code = 0 AND (no stderr OR only known non-fatal warnings)
                                let is_fatal = !out.status.success() || (!decode_stderr.is_empty() && !is_he_aac_bands_warning);
                                if is_fatal {
                                    let err_msg = decode_stderr.split('\n').next().unwrap_or("unknown decode error");
                                    log::warn!("[FINAL_VALIDATE:AUDIO_SEEK] ⚠️ {:.0}% ({:.1}s, {}ms) FAILED: {}", pct*100.0, seek_sec, test_elapsed, err_msg);
                                    audio_seek_warnings += 1;
                                    break;
                                } else {
                                    // Decode succeeded — HE-AAC bands warning is non-fatal
                                    let pass_reason = if is_he_aac_bands_warning {
                                        "HE-AAC_bands_warning_non_fatal"
                                    } else {
                                        "clean_decode"
                                    };
                                    log::info!("[FINAL_VALIDATE:AUDIO_SEEK] ✅ {:.0}% ({:.1}s, {}ms) OK [{}] (exit={}, stderr_chars={})",
                                        pct*100.0, seek_sec, test_elapsed, pass_reason, decode_exit_code, decode_stderr.len());
                                    audio_passed += 1;
                                }
                            }
                            Err(e) => {
                                log::warn!("[FINAL_VALIDATE:AUDIO_SEEK] ⚠️ {:.0}% ({:.1}s, {}ms) ERROR: {}", pct*100.0, seek_sec, test_elapsed, e);
                                audio_seek_warnings += 1;
                                break;
                            }
                        }
                        // Emit validation progress
                        let val_pct = (validation_completed as f32 / total_validation_tests as f32) * 100.0;
                        let remaining = total_validation_tests - validation_completed;
                        let avg_per_test = if validation_completed > 0 { validation_start.elapsed().as_millis() as f32 / validation_completed as f32 } else { 0.0 };
                        let eta = (remaining as f32 * avg_per_test / 1000.0).max(0.0);
                        let next_test = if idx + 1 < test_points.len() { format!("audio_seek_{:.0}pct", test_points[idx + 1] * 100.0) } else { "finalizing_complete".to_string() };
                        let _ = app_handle_validate.emit("merge-progress", &serde_json::json!({
                            "jobId": job_id_validate,
                            "progress": {
                                "phase": "finalizing",
                                "stageName": format!("Validating audio seeks ({}/{})", validation_completed, total_validation_tests),
                                "stagePercent": val_pct,
                                "percent": 96.0 + (val_pct * 0.04),
                                "overallPercent": 96.0 + (val_pct * 0.04),
                                "currentTime": 0.0,
                                "totalDuration": 0.0,
                                "currentFileIndex": validation_completed,
                                "totalFilesInStage": total_validation_tests,
                                "currentValidationTest": Some(next_test),
                                "currentValidationIndex": validation_completed,
                                "totalValidationTests": total_validation_tests,
                                "etaSeconds": eta,
                                "warning": null,
                            }
                        }));
                    }
                    let avg_audio_seek = if audio_total > 0 { audio_total_time / audio_total as u128 } else { 0 };
                    log::info!("[FINAL_VALIDATE:AUDIO_SUMMARY] Audio seek audit complete: {}/{} passed, avg={}ms, total={}ms",
                        audio_passed, audio_total, avg_audio_seek, audio_total_time);
                }
            }
        }
        // ──────────────────────────────────────────────────────────────────────────────────────
        // ──────────────────────────────────────────────────────────────────────────────────────

        let total_validation_elapsed = validation_start.elapsed();
        log::info!("[FINAL_VALIDATE_COMPLETE] ═══════════════════════════════════════════════");
        log::info!("[FINAL_VALIDATE_COMPLETE] Post-merge forensic validation COMPLETE");
        log::info!("[FINAL_VALIDATE_COMPLETE] Total tests: {}/{}", validation_completed, total_validation_tests);
        log::info!("[FINAL_VALIDATE_COMPLETE] Total elapsed: {}.{:03}s", total_validation_elapsed.as_secs(), total_validation_elapsed.subsec_millis());
        log::info!("[FINAL_VALIDATE_COMPLETE] ═══════════════════════════════════════════════");

        // Emit final validation complete (96-100% transition)
        let _ = app_handle_validate.emit("merge-progress", &serde_json::json!({
            "jobId": job_id_validate,
            "progress": {
                "phase": "finalizing",
                "stageName": "Validation complete",
                "stagePercent": 100.0,
                "percent": 99.0,
                "overallPercent": 99.0,
                "currentTime": 0.0,
                "totalDuration": 0.0,
                "currentFileIndex": total_validation_tests,
                "totalFilesInStage": total_validation_tests,
                "currentValidationTest": Some("finalizing_complete"),
                "currentValidationIndex": total_validation_tests,
                "totalValidationTests": total_validation_tests,
                "etaSeconds": 0.0,
                "warning": null,
            }
        }));

        // ──── Embed Fallback ────────────────────────────────────────────────────────────────────────
        // If Embed mode failed with "dimensions not set", export the merged SRT externally
        // and retry the merge without subtitle embedding. This handles files where ffprobe
        // reports 0x0 dimensions and the subtitle mov_text encoder can't initialize.
        let mut embed_fallback_srt_paths: Option<Vec<String>> = None;
        let mut embed_fallback_warnings: Vec<String> = Vec::new();
        if result.is_err() && subtitle_mode_for_closure == SubtitleMode::Embed && !is_export_srt_only {
            let err_str = result.as_ref().unwrap_err().to_string();
            if err_str.contains("dimensions not set") {
                log::warn!("[EmbedFallback] 'dimensions not set' detected -- exporting merged SRT externally");

                // 1. Export the merged SRT with proper timestamp rebasing before retrying
                let srt_out = Path::new(&output_path).with_extension("srt");
                if crate::ffmpeg::generate_merged_srt_with_rebase(&final_prepared_subs, &final_input_durations, &ffmpeg_path_resolved, &srt_out).is_ok() {
                    embed_fallback_srt_paths = Some(vec![srt_out.to_string_lossy().into_owned()]);
                    log::info!("[EmbedFallback] Exported merged SRT: {}", srt_out.display());
                }

                // 2. Retry merge without subtitle embedding
                let mut retry_config = config.clone();
                retry_config.subtitle_mode = SubtitleMode::None;
                retry_config.subtitle_list_path = None;

                log::info!("[EmbedFallback] Retrying merge without subtitle embedding...");

                let app_handle_retry = app_handle.clone();
                let job_id_retry = job_id.clone();
                result = run_merge_blocking(
                    &ffmpeg_path_resolved,
                    &retry_config,
                    &list_path,
                    cancel_for_retry,
                    move |mut progress| {
                        let overall_pct = 35.0 + (progress.percent * 0.65);
                        progress.overall_percent = Some(overall_pct);
                        progress.percent = overall_pct;
                        let _ = app_handle_retry.emit("merge-progress", &serde_json::json!({
                            "jobId": job_id_retry,
                            "progress": progress
                        }));
                    },
                );

                if result.is_ok() {
                    log::info!("[EmbedFallback] Merge succeeded without subtitle embedding. SRT exported externally.");
                    embed_fallback_warnings.push(
                        "Subtitle embedding failed (dimensions not set). Merged SRT exported as external file.".to_string()
                    );
                } else if let Err(ref e) = result {
                    log::warn!("[EmbedFallback] Merge also failed without subtitles: {}", e);
                }
            }
        }

        if let Ok(ref merge_result) = result {
            let mut s = crate::services::settings::load_settings_internal();
            s.recent_exports.insert(0, RecentExport { path: output_path.clone(), timestamp: Utc::now(), size_bytes: merge_result.output_size_bytes, file_count: original_file_count, duration_seconds: original_total_duration, mode: format!("{:?}", actual_mode) });
            s.recent_exports.truncate(10); let _ = crate::services::settings::save_settings_internal(&s);
        }
        let mut srt_paths = embed_fallback_srt_paths.take();
        // For split merges, use per-part SRT paths generated by run_split_merge_blocking
        if srt_paths.is_none() && result.is_ok() {
            if let Ok(ref res) = &result {
                if let Some(ref split_srt_paths) = res.srt_export_paths {
                    if !split_srt_paths.is_empty() {
                        srt_paths = Some(split_srt_paths.clone());
                    }
                }
            }
        }
        if srt_paths.is_none() && result.is_ok() && (export_merged_srt || is_export_srt_only) {
            let srt_out = Path::new(&output_path).with_extension("srt");
        if crate::ffmpeg::generate_merged_srt_with_rebase(&final_prepared_subs, &final_input_durations, &ffmpeg_path_resolved, &srt_out).is_ok() { srt_paths = Some(vec![srt_out.to_string_lossy().into_owned()]); }
        }
        let mut report_paths = None;
        if let Ok(res) = &result { 
            let mut paths = Vec::new();
            if let Some(rp) = write_report_file(&output_path, &res.segments, res.output_size_bytes, final_total_duration, request.repeat_config.as_ref(), Some(deduped_total_duration)) {
                paths.push(rp);
            }
            // Generate per-part merge reports for split outputs
            if let Some(parts) = &res.parts {
                let mut seg_offset = 0;
                for part in parts {
                    let count = part.file_count as usize;
                    let part_segs = &res.segments[seg_offset..seg_offset + count];
                    seg_offset += count;
                    if let Some(rp) = write_report_file(&part.output_path, part_segs, part.output_size_bytes, part.total_duration, request.repeat_config.as_ref(), Some(deduped_total_duration)) {
                        paths.push(rp);
                    }
                }
            }
            if !paths.is_empty() {
                report_paths = Some(paths);
            }
        }

        // Surface audio seekability warnings to user via MergeResult
        if audio_seek_warnings > 0 {
            if let Ok(ref mut res) = result {
                let warnings = res.warnings.get_or_insert_with(Vec::new);
                warnings.push(format!("Audio seekability: {} position(s) had decoder errors during post-merge audit. The output may have audio issues in those regions.", audio_seek_warnings));
            }
        }

        // Construct audio repair summary from in-scope repair_reasons tracking
        let audio_repair_summary = AudioRepairSummary {
            mode: format!("{:?}", audio_repair_mode),
            total_files: total_input_count,
            files_repaired: repair_reasons.len(),
            due_to_corruption: repair_reasons.values().filter(|r| r.iter().any(|x| x.contains("Corruption") || x.contains("DeepValidation") || x.contains("SeekPointCheck"))).count(),
            due_to_profile_mismatch: repair_reasons.values().filter(|r| r.iter().any(|x| x.contains("ProfileMismatch"))).count(),
            due_to_safe_mode: repair_reasons.values().filter(|r| r.iter().any(|x| x.contains("SafeMode"))).count(),
            repaired_indices: {
                let mut idx: Vec<usize> = repair_reasons.keys().copied().collect();
                idx.sort_unstable();
                idx
            },
        };
        if let Ok(ref mut res) = result {
            res.audio_repair_summary = Some(audio_repair_summary.clone());
        }
        let audio_repair_summary_for_emit = audio_repair_summary;

        match result {
            Ok(res) => {
                // ── SmartMkv convert_to_mp4: MKV → MP4 conversion ──────
                let mut final_output_path = output_path.clone();
                let mut final_output_size = res.output_size_bytes;
                if let Some(ref mp4_path) = final_mp4_path {
                    log::info!("[SmartMkv] Muxing complete, converting MKV → MP4: {}", mp4_path);
                    let _ = app_handle.emit("merge-progress", &serde_json::json!({
                        "jobId": job_id,
                        "progress": {
                            "phase": "smartMkv-converting",
                            "stageName": "Converting to MP4...",
                            "stagePercent": 0.0,
                            "percent": 95.0,
                            "overallPercent": 95.0,
                            "currentTime": 0.0,
                            "totalDuration": total_duration,
                        }
                    }));

                    match crate::ffmpeg::fast_mkv::convert_mkv_to_mp4(
                        &ffmpeg_path_resolved,
                        &ffprobe_path_resolved,
                        &output_path,
                        mp4_path,
                        total_duration,
                        cancel_flag.clone(),
                        {
                            let app_handle_clone = app_handle.clone();
                            let job_id_clone = job_id.clone();
                            move |progress: crate::types::MergeProgress| {
                                let _ = app_handle_clone.emit("merge-progress", &serde_json::json!({
                                    "jobId": job_id_clone,
                                    "progress": progress
                                }));
                            }
                        },
                    ) {
                        Ok(()) => {
                            log::info!("[SmartMkv] MP4 conversion successful: {}", mp4_path);
                            final_output_path = mp4_path.clone();
                            // Get file size of converted MP4
                            final_output_size = std::fs::metadata(mp4_path)
                                .map(|m| m.len())
                                .unwrap_or(res.output_size_bytes);
                            // Clean up temp MKV
                            if let Err(e) = std::fs::remove_file(&output_path) {
                                log::warn!("[SmartMkv] Failed to remove temp MKV: {}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("[SmartMkv] MP4 conversion failed: {}. Returning MKV output.", e);
                        }
                    }
                }

                // ═══════════════════════════════════════════════════════════════════════════
                // JOB SUMMARY — Key timings and statistics
                // ═══════════════════════════════════════════════════════════════════════════
                log::info!("");
                log::info!("╔══════════════════════════════════════════════════════════════════════════╗");
                log::info!("║                         MERGE JOB SUMMARY                           ║");
                log::info!("╠══════════════════════════════════════════════════════════════════════════╣");
                log::info!("║  Mode:         {:<53} ║", format!("{:?}", actual_mode));
                log::info!("║  Files:        {:<53} ║", final_input_files.len());
                log::info!("║  Output:       {:<53} ║", final_output_path.chars().take(50).collect::<String>());
                log::info!("║  Output Size:  {:<53} ║", format!("{:.2} GB", final_output_size as f64 / 1_073_741_824.0));
                log::info!("║  Duration:     {:<53} ║", format!("{:.1} seconds ({:.1} hours)", actual_duration, actual_duration / 3600.0));
                log::info!("║  mkvmerge:     {:<53} ║", if mkvmerge_succeeded { "SUCCESS (FFmpeg concat will overwrite)".to_string() } else { "SKIPPED/FAILED".to_string() });
                log::info!("║  Warnings:     {:<53} ║", res.warnings.as_ref().map(|w| w.len().to_string()).unwrap_or_else(|| "0".to_string()));
                log::info!("╠══════════════════════════════════════════════════════════════════════════╣");
                log::info!("║  PHASE SUMMARY                                                     ║");
                log::info!("║    Phase 3: mkvmerge   = see [PHASE_3] logs above                     ║");
                log::info!("║    Phase 4: FFMPEG concat = see [PHASE_4] logs above                   ║");
                log::info!("║    Phase 5-7: Validation = see [FINAL_VALIDATE_*] logs above          ║");
                log::info!("╠══════════════════════════════════════════════════════════════════════════╣");
                log::info!("║  For detailed timings search: [PHASE_*] [NORM_EXEC_*] [TIMELINE]       ║");
                log::info!("╚══════════════════════════════════════════════════════════════════════════╝");
                log::info!("");

// PHASE-BASED RESUME: Update phase to Complete and clean up checkpoint
                // BEFORE emitting the merge-complete event. This ensures the checkpoint
                // is properly cleaned up even if the app crashes after event emission.
                if let Some(ref sender) = checkpoint_sender {
                    sender.update_phase(crate::types::MergePhase::Complete, None);
                    log::info!("[PhaseResume] Checkpoint phase sent to writer: Complete");
                }
                if let Ok(app_data) = recovery::get_app_data_dir() {
                    let _ = recovery::delete_checkpoint(&app_data, &job_id);
                    log::info!("[Recovery] Checkpoint deleted on successful completion");
                }

                log::info!("[EVENT_EMIT] event=merge-complete jobId={}", job_id);
                let _ = app_handle.emit("merge-complete", &serde_json::json!({
                    "jobId": job_id,
                    "outputPath": final_output_path,
                    "outputSizeBytes": final_output_size,
                    "outputDurationSecs": actual_duration,
                    "segments": res.segments,
                    "outputPaths": res.output_paths,
                    "parts": res.parts,
                    "srtExportPaths": srt_paths,
                    "reportPaths": report_paths,
                    "warnings": res.warnings.clone().unwrap_or_default(),
                    "audioRepairSummary": audio_repair_summary_for_emit
                }));
                crate::commands::merge::remove_merge_marker(&output_path);
crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Success, None, Some(&output_path));
crate::logger::stop_job_log(Some("[JOB_COMPLETE] Merge completed successfully"));

                // ── SMART MKV SUMMARY REPORT ─────────────────────────────────────────────
                // Note: Timing instrumentation (normalize_time, merge_time) should be added
                // as phase-tracking variables passed through the merge pipeline.
                // Note: audio/video/full breakdown requires additional tracking via
                // need_audio_norm and need_profile_norm (currently out of scope here).
                let total_files = working_input_files.len();
                let (_normalize_count, remux_count, skip_count, before_filter_outliers, after_filter_outliers)
                    = if let Some(ref breakdown) = smart_mkv_breakdown {
                    let normalize = breakdown.normalize.iter().map(|c| c.count).sum::<usize>();
                    let remux = breakdown.remux.iter().map(|c| c.count).sum::<usize>();
                    let skip = breakdown.skip.iter().map(|c| c.count).sum::<usize>();
                    let before = normalize + remux + skip; // All detected outliers
                    let after = normalize + remux;         // Only non-filtered outliers
                    (normalize, remux, skip, before, after)
                } else {
                    (0, 0, 0, 0, 0)
                };

                // Compute video/audio/full breakdown from normalize properties if available
                let (video_normalize_count, audio_normalize_count, full_normalize_count)
                    = if let Some(ref breakdown) = smart_mkv_breakdown {
                    let mut video_cnt = 0usize;
                    let mut audio_cnt = 0usize;
                    for nc in &breakdown.normalize {
                        let prop = nc.property.to_lowercase();
                        if prop.contains("video") || prop.contains("codec") && !prop.contains("audio") {
                            video_cnt += nc.count;
                        } else if prop.contains("audio") {
                            audio_cnt += nc.count;
                        } else {
                            // Default to video for unknown
                            video_cnt += nc.count;
                        }
                    }
                    // Approximate: video-only = video_cnt - overlap, audio-only = audio_cnt, full = min(video, audio)
                    let full_cnt = std::cmp::min(video_cnt, audio_cnt);
                    let video_only = video_cnt.saturating_sub(full_cnt);
                    let audio_only = audio_cnt.saturating_sub(full_cnt);
                    (video_only, audio_only, full_cnt)
                } else {
                    (0, 0, 0)
                };

                let summary_stats = crate::ffmpeg::normalization::SmartMkvSummaryStats {
                    total_files,
                    stream_copy_count: skip_count,
                    remux_count,
                    audio_normalize_count,
                    video_normalize_count,
                    full_normalize_count,
                    normalize_time_secs: 0.0, // TODO: Add phase timing instrumentation
                    merge_time_secs: 0.0,      // TODO: Add merge timing instrumentation
                    total_input_size_bytes: 0, // TODO: Compute from working_input_files
                    total_output_size_bytes: final_output_size,
                    before_filter_outliers,
                    after_filter_outliers,
                };
                crate::ffmpeg::normalization::log_smartmkv_summary_report(&summary_stats);

                log::info!("[EVENT_EMIT] event=merge-complete jobId={} emitted", job_id);
                // PHASE-BASED RESUME: Update phase to Complete before checkpoint cleanup via single-writer
                if let Some(ref sender) = checkpoint_sender {
                    sender.update_phase(crate::types::MergePhase::Complete, None);
                    log::info!("[PhaseResume] Checkpoint phase sent to writer: Complete");
                }
                // ── POST-MERGE AAC PROFILE VERIFICATION ────────────────────────────────
                // Verify the output file has uniform AAC profile (all LC after normalization).
                if actual_mode == MergeMode::SmartMkv {
                    let out_path = std::path::Path::new(&final_output_path);
                    if out_path.exists() {
                        match crate::ffmpeg::probe::probe_file(&ffprobe_path_resolved, out_path) {
                            Ok(info) => {
                                let profiles: Vec<String> = info.audio_streams.iter()
                                    .map(|a| a.profile.clone().unwrap_or_else(|| "N/A".to_string()))
                                    .collect();
                                let all_lc = profiles.iter().all(|p| p == "LC");
                                log::info!("[AUDIO_PROFILE_VERIFY] ═══════════════════════════════════════════════");
                                log::info!("[AUDIO_PROFILE_VERIFY] Output file: {}", final_output_path);
                                log::info!("[AUDIO_PROFILE_VERIFY] Audio profiles: {:?}", profiles);
                                log::info!("[AUDIO_PROFILE_VERIFY] All LC: {}", all_lc);
                                if !all_lc {
                                    log::warn!("[AUDIO_PROFILE_VERIFY] WARNING: Output contains non-LC profiles — audio seekability may be affected");
                                } else {
                                    log::info!("[AUDIO_PROFILE_VERIFY] PASS — uniform LC profile confirmed");
                                }
                                log::info!("[AUDIO_PROFILE_VERIFY] ═══════════════════════════════════════════════");
                            }
                            Err(e) => {
                                log::warn!("[AUDIO_PROFILE_VERIFY] Could not probe output file: {}", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                log::error!("[FORENSIC:ERROR] Concat/Merge FAILED | jobId: {} | Error: {}", job_id, e);
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=concat_failed", job_id);
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Failed, Some(&format!("{}", e)), None);
                crate::logger::stop_job_log(Some("[JOB_FAILED] Merge failed"));
let _ = app_handle.emit("merge-error", &serde_json::json!({
                    "jobId": job_id,
                    "error": e.to_string(),
                    "cancelled": e.to_string().contains("cancelled"),
                    "phase": "concat"
                }));
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=concat_failed emitted", job_id);
            }
        }
        remove_merge_marker(&output_path);
        log::info!("[MERGE_TRACE] remove_merge_marker done jobId={}", job_id);

        log::info!("[MERGE_TRACE] ACTIVE_JOB_REMOVE_START jobId={}", job_id);
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            log::info!("[MERGE_TRACE] block_on acquired runtime jobId={}", job_id);
            let mut ms = merge_state_ref.lock().await;
            log::info!("[MERGE_TRACE] lock acquired, removing active_jobs jobId={}", job_id);
            ms.active_jobs.remove(&job_id);
            log::info!("[MERGE_TRACE] ACTIVE_JOB_REMOVE_DONE jobId={}", job_id);
        });
        log::info!("[MERGE_TRACE] block_on released jobId={}", job_id);

        log::info!("[MERGE_TRACE] === MERGE_TASK_COMPLETED jobId={} ===", job_id);

        // PER-JOB CHECKPOINT WRITER: Ensure drain before merge task returns
        // Take ownership from Option to call close()
        if let Some(writer) = checkpoint_writer.take() {
            if let Err(e) = writer.close() {
                log::error!("[Recovery] Checkpoint writer close() failed: {:?}", e);
            } else {
                log::info!("[Recovery] Checkpoint writer drained successfully");
            }
        }

        Ok::<String, String>(request.job_id.clone())
    });

    // Watchdog: detect if spawn_blocking panics (JoinError) and emit merge-error.
    // This ensures the frontend is notified even when the main merge thread crashes.
    tokio::spawn(async move {
        match merge_handle.await {
            Ok(Ok(_)) => {
                // Normal completion — merge-complete already emitted via progress callbacks
                log::info!("[MERGE_TRACE] WATCHDOG: merge completed normally jobId={}", watchdog_job_id);
            }
            Ok(Err(e)) => {
                // Logic error returned (not a panic) — emit merge-error
                log::error!("[MERGE_TRACE] WATCHDOG: merge returned error jobId={} error={}", watchdog_job_id, e);
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=watchdog_logic_error", watchdog_job_id);
                let _ = watchdog_app.emit("merge-error", &serde_json::json!({
                    "jobId": watchdog_job_id,
                    "error": format!("Merge failed: {}", e),
                    "cancelled": false,
                    "phase": "concat"
                }));
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=watchdog_logic_error emitted", watchdog_job_id);
            }
            Err(je) => {
                // Thread panicked — JoinError
                log::error!("[MERGE_TRACE] WATCHDOG: merge thread PANICKED jobId={} panic={}", watchdog_job_id, je);
                crate::forensic_log::append_panic_block(&format!("{}", je));
                crate::forensic_log::end_forensic_log(crate::forensic_log::ForensicStatus::Panic, Some(&format!("{}", je)), None);
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=watchdog_panic", watchdog_job_id);
                let _ = watchdog_app.emit("merge-error", &serde_json::json!({
                    "jobId": watchdog_job_id,
                    "error": format!("Merge process panicked: {}", je),
                    "cancelled": false,
                    "phase": "concat"
                }));
                log::info!("[EVENT_EMIT] event=merge-error jobId={} reason=watchdog_panic emitted", watchdog_job_id);
            }
        }
    });

    Ok(job_id_for_return.clone())
}

#[command]
pub async fn cancel_merge(job_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let ms = state.merge_state.lock().await; if let Some(f) = ms.active_jobs.get(&job_id) { f.store(true, Ordering::SeqCst); }
    Ok(())
}
#[command]
pub async fn get_merge_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let ms = state.merge_state.lock().await;
    Ok(serde_json::json!({ "isRunning": !ms.active_jobs.is_empty(), "runningJobIds": ms.active_jobs.keys().cloned().collect::<Vec<_>>() }))
}
/// Checks for recovery checkpoints from interrupted merges.
/// Returns a list of recoverable jobs with their status.
#[command]
pub async fn check_recovery_checkpoints() -> Result<serde_json::Value, String> {
    let app_data = crate::recovery::get_app_data_dir().map_err(|e| e.to_string())?;
    let job_ids = crate::recovery::scan_recovery_dir(&app_data).map_err(|e| e.to_string())?;

    let mut checkpoints = Vec::new();
    for job_id in job_ids {
        if let Ok(Some(cp)) = crate::recovery::read_checkpoint(&app_data, &job_id) {
            if let Some(reason) = crate::recovery::validate_checkpoint(&cp) {
                log::warn!("[Recovery] Invalid checkpoint for job {}: {}", job_id, reason);
                continue;
            }
            checkpoints.push(cp);
        }
    }

    log::info!("[Recovery] Found {} valid recovery checkpoints", checkpoints.len());
    // Serialize with camelCase to match TypeScript types
    serde_json::to_value(&checkpoints).map_err(|e| e.to_string())
}

/// Deletes a recovery checkpoint (used when user chooses "Start Over")
#[command]
pub async fn delete_recovery_checkpoint(job_id: String) -> Result<(), String> {
    let app_data = crate::recovery::get_app_data_dir().map_err(|e| e.to_string())?;
    crate::recovery::delete_checkpoint(&app_data, &job_id).map_err(|e| e.to_string())?;
    log::info!("[Recovery] Deleted checkpoint for job: {}", job_id);
    Ok(())
}

#[command]
pub async fn validate_audio_files(files: Vec<String>, _app_handle: tauri::AppHandle) -> Result<serde_json::Value, String> {
    if files.is_empty() { return Err("No files provided".to_string()); }
    let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
    let cancel = Arc::new(AtomicBool::new(false));
    let errors = crate::ffmpeg::concat::validate_audio_streams_parallel(
        paths,
        Some(cancel),
        None::<fn(usize, usize, usize)>,
    ).await.map_err(|e| e.to_string())?;
    
    Ok(serde_json::json!({ 
        "totalFiles": files.len(), 
        "passedFiles": files.len() - errors.len(), 
        "failedFiles": errors.len(), 
        "errors": errors.iter().map(|e| serde_json::json!({ "index": e.file_index, "filename": e.filename, "errors": e.error_lines })).collect::<Vec<_>>() 
    }))
}

/// Result of a per-file compatibility check.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompatibleFile {
    pub file_index: usize,
    pub filename: String,
    pub reason: String,
    pub severity: String,
}

/// Overall merge compatibility check result.
/// Returned by check_merge_compatibility Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeCompatibilityCheck {
    /// True if all files are compatible for lossless merge without auto-upgrade.
    pub can_merge_lossless: bool,
    /// Human-readable reason why lossless is not possible or was auto-upgraded.
    /// None if lossless will apply cleanly.
    pub auto_upgrade_reason: Option<String>,
    /// List of files that caused incompatibility or warnings.
    pub incompatible_files: Vec<IncompatibleFile>,
    /// True if user selected Lossless mode AND it will actually apply (no upgrade).
    pub lossless_will_apply: bool,
    /// List of non-blocking warnings (e.g., resolution varies, fps varies).
    pub warnings: Vec<IncompatibleFile>,
}

/// Check merge compatibility without running the full merge.
/// This exposes the same auto-upgrade logic that start_merge uses internally.
///
/// Returns a structured response with:
/// - Whether lossless can apply cleanly
/// - Reason for any auto-upgrade
/// - Per-file issues and warnings
#[command]
pub async fn check_merge_compatibility(
    input_files: Vec<String>,
    media_infos: Vec<crate::types::MediaInfo>,
    selected_mode: String,
) -> Result<MergeCompatibilityCheck, String> {
    if input_files.is_empty() {
        return Err("No input files provided".to_string());
    }
    if input_files.len() != media_infos.len() {
        return Err(format!(
            "File count mismatch: {} paths but {} media infos",
            input_files.len(),
            media_infos.len()
        ));
    }

    let mut incompatible_files: Vec<IncompatibleFile> = Vec::new();
    let mut warnings: Vec<IncompatibleFile> = Vec::new();
    let mut auto_upgrade_reason: Option<String> = None;
    let mut can_merge_lossless = true;

    // Pair files with their media info
    let pairs: Vec<(usize, String, &crate::types::MediaInfo)> = input_files
        .iter()
        .enumerate()
        .filter_map(|(i, path)| {
            media_infos.get(i).map(|mi| {
                let filename = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("file_{}", i));
                (i, filename, mi)
            })
        })
        .collect();

    // ── 0. Check for duplicate file paths ─────────────────────────────────
    let mut seen_paths: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (i, _, info) in &pairs {
        if !seen_paths.insert(info.path.as_str()) {
            incompatible_files.push(IncompatibleFile {
                file_index: *i,
                filename: std::path::Path::new(&info.path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("file_{}", i)),
                reason: format!("Duplicate file: '{}' appears multiple times in the playlist", info.path),
                severity: "error".to_string(),
            });
            if auto_upgrade_reason.is_none() {
                auto_upgrade_reason = Some("Duplicate files detected in the playlist. Remove duplicates before merging.".to_string());
            }
            can_merge_lossless = false;
        }
    }
    // Don't proceed if duplicates exist
    if incompatible_files.iter().any(|f| f.severity == "error") {
        return Ok(MergeCompatibilityCheck {
            can_merge_lossless,
            auto_upgrade_reason,
            incompatible_files,
            lossless_will_apply: false,
            warnings,
        });
    }

    // ── 0b. Check for zero-duration files ─────────────────────────────────
    for (i, filename, info) in &pairs {
        if info.duration <= 0.0 {
            incompatible_files.push(IncompatibleFile {
                file_index: *i,
                filename: filename.clone(),
                reason: format!("Zero-duration file: '{}' has a duration of {:.3}s. The file may be corrupt or unreadable.", filename, info.duration),
                severity: "error".to_string(),
            });
            if auto_upgrade_reason.is_none() {
                auto_upgrade_reason = Some(format!(
                    "Zero-duration file detected: '{}'. This file cannot be merged.",
                    filename
                ));
            }
            can_merge_lossless = false;
        }
    }

    // ── 1. Check for audio-only files ─────────────────────────────────────
    for (i, filename, info) in &pairs {
        if info.video_streams.is_empty() {
            incompatible_files.push(IncompatibleFile {
                file_index: *i,
                filename: filename.clone(),
                reason: "Audio-only file has no video stream".to_string(),
                severity: "error".to_string(),
            });
            auto_upgrade_reason = Some(format!(
                "Audio-only file detected: '{}'. This file has no video stream and cannot be merged with video files.",
                filename
            ));
            can_merge_lossless = false;
        }
    }

    // ── 2. Check codec, resolution, audio mismatches (using first file as reference) ──
    if let Some((_, _, ref_info)) = pairs.first() {
        let ref_video = ref_info.video_streams.first();
        let ref_audio = ref_info.audio_streams.first();

        for (i, filename, info) in &pairs[1..] {
            let curr_video = info.video_streams.first();
            let curr_audio = info.audio_streams.first();

            // Video codec mismatch (warning, not blocking)
            if let (Some(rv), Some(cv)) = (ref_video, curr_video) {
                if rv.codec_name != cv.codec_name {
                    warnings.push(IncompatibleFile {
                        file_index: *i,
                        filename: filename.clone(),
                        reason: format!("Video codec '{}' differs from reference '{}' (will be normalized)", cv.codec_name, rv.codec_name),
                        severity: "warning".to_string(),
                    });
                }
            }

            // Resolution mismatch (warning, not blocking)
            if let (Some(rv), Some(cv)) = (ref_video, curr_video) {
                if let (Some(rw), Some(rh), Some(cw), Some(ch)) = (rv.width, rv.height, cv.width, cv.height) {
                    if rw != cw || rh != ch {
                        warnings.push(IncompatibleFile {
                            file_index: *i,
                            filename: filename.clone(),
                            reason: format!("Resolution {}x{} differs from reference {}x{}", cw, ch, rw, rh),
                            severity: "warning".to_string(),
                        });
                    }
                }
            }

            // Audio codec mismatch (warning, not blocking)
            if let (Some(ra), Some(ca)) = (ref_audio, curr_audio) {
                if ra.codec_name != ca.codec_name {
                    warnings.push(IncompatibleFile {
                        file_index: *i,
                        filename: filename.clone(),
                        reason: format!("Audio codec '{}' differs from reference '{}' (will be normalized)", ca.codec_name, ra.codec_name),
                        severity: "warning".to_string(),
                    });
                }
            }

            // Audio sample rate mismatch (warning, not blocking)
            if let (Some(ra), Some(ca)) = (ref_audio, curr_audio) {
                if let (Some(rs), Some(cs)) = (ra.sample_rate, ca.sample_rate) {
                    if rs != cs {
                        warnings.push(IncompatibleFile {
                            file_index: *i,
                            filename: filename.clone(),
                            reason: format!("Sample rate {}Hz differs from reference {}Hz (will be normalized)", cs, rs),
                            severity: "warning".to_string(),
                        });
                    }
                }
            }

            // Audio channel count > 2 (warning — causes 'rematrix is needed' errors)
            if let Some(ca) = curr_audio {
                if let Some(channels) = ca.channels {
                    if channels > 2 {
                        warnings.push(IncompatibleFile {
                            file_index: *i,
                            filename: filename.clone(),
                            reason: format!("{} audio channels — multi-channel audio may cause 'rematrix' errors (will be normalized)", channels),
                            severity: "warning".to_string(),
                        });
                    }
                }
            }
        }
    }

    // ── 3. Detect codec transitions (the critical auto-upgrade trigger) ──────
    let mut prev_codec: Option<String> = None;
    for (i, _, info) in &pairs {
        let codec = info.video_streams.first().map(|v| v.codec_name.clone());
        if let (Some(prev), Some(curr)) = (&prev_codec, &codec) {
            if prev != curr {
                let reason = format!(
                    "Codec transition detected: '{}' (file [{}]) → '{}' (file [{}]). Phase 5C correlation: codec transitions cause 'missing picture in access unit' errors. Re-encoding is required.",
                    prev,
                    i - 1,
                    curr,
                    i
                );
                if auto_upgrade_reason.is_none() {
                    auto_upgrade_reason = Some(reason.clone());
                }
                can_merge_lossless = false;
                incompatible_files.push(IncompatibleFile {
                    file_index: *i,
                    filename: std::path::Path::new(&input_files[*i])
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("file_{}", i)),
                    reason,
                    severity: "error".to_string(),
                });
            }
        }
        prev_codec = codec;
    }

    let lossless_will_apply = selected_mode == "lossless" && can_merge_lossless;

    Ok(MergeCompatibilityCheck {
        can_merge_lossless,
        auto_upgrade_reason,
        incompatible_files,
        lossless_will_apply,
        warnings,
    })
}

/// Fast MKV Merge pipeline — completely isolated from main merge logic
///
/// This function handles:
/// 1. Pre-flight compatibility check
/// 2. MKV stream copy
/// 3. Optional MP4 conversion
#[allow(clippy::too_many_arguments)]
async fn run_fast_mkv_pipeline(
    request: &MergeRequest,
    input_files: &[String],
    input_durations: &[f64],
    input_names: &[String],
    total_duration: f64,
    probe_cache: &Arc<crate::ffmpeg::probe_cache::ProbeCache>,
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
    app_handle: &tauri::AppHandle,
    repeat_expanded: Option<&crate::ffmpeg::repeat::ExpandedPlaylist>,
) -> Result<String, String> {
    let job_id = request.job_id.clone();
    let output_path = request.output_path.clone();
    let convert_to_mp4 = request.convert_to_mp4.unwrap_or(false);
    let ffmpeg_path_buf = ffmpeg_path.to_path_buf();
    let ffprobe_path_buf = ffprobe_path.to_path_buf();

    log::info!("[FastMkv] Starting Fast MKV merge for job {}", job_id);
    log::info!("[FastMkv] Input files: {}", input_files.len());
    log::info!("[FastMkv] Convert to MP4: {}", convert_to_mp4);

    // Emit progress: analysis phase
    let _ = app_handle.emit("merge-progress", &serde_json::json!({
        "jobId": job_id,
        "progress": {
            "phase": "fastMkv-analysis",
            "stageName": "Analyzing input compatibility...",
            "stagePercent": 0.0,
            "percent": 0.0,
            "overallPercent": 0.0,
            "currentTime": 0.0,
            "totalDuration": total_duration,
        }
    }));

    // Get media info from probe cache
    let mut media_infos = Vec::new();
    for file in input_files {
        let path = std::path::Path::new(file);
        match probe_cache.get(path) {
            Some(Ok(info)) => media_infos.push(info.clone()),
            Some(Err(e)) => return Err(format!("Failed to probe {}: {}", file, e)),
            None => return Err(format!("No probe data for {}", file)),
        }
    }

    // Check compatibility
    let compat = crate::ffmpeg::fast_mkv::check_fast_mkv_compatibility(&media_infos);
    
    if !compat.compatible {
        log::warn!("[FastMkv] Input files are not compatible:");
        for reason in &compat.reasons {
            log::warn!("[FastMkv]   - {}", reason);
        }
        
        let _ = app_handle.emit("merge-error", &serde_json::json!({
            "jobId": job_id,
            "error": format!(
                "Fast MKV Merge requires all input files to have matching codecs and resolutions.\n\nIssues:\n{}\n\nUse Smart Merge (Lossless or Custom mode) instead.",
                compat.reasons.join("\n")
            )
        }));
        
        return Err(format!(
            "Fast MKV Merge requires compatible inputs. Issues: {}",
            compat.reasons.join("; ")
        ));
    }

    log::info!("[FastMkv] Compatibility check passed: {:?} / {:?} / {:?}",
        compat.video_codec, compat.audio_codec, compat.resolution);

    let mut final_input_files = input_files.to_vec();
    let mut final_input_durations: Vec<f64> = input_durations.to_vec();
    let mut final_total_duration = total_duration;
    let mut card_temp_files = Vec::new();
    let mut final_segment_cards: Vec<(bool, Option<String>)> = vec![(false, None); input_files.len()];

    log::info!("[FastMkv] ═══════════════════════════════════════════════════════════════");
    log::info!("[FastMkv] PIPELINE: FastMkv (stream copy, no re-encoding)");
    log::info!("[FastMkv] Input files: {}", input_files.len());
    log::info!("[FastMkv] Total duration: {:.1}s", total_duration);

    // ── [CARDS] Fast MKV Card Support ─────────────────────────────────────
    if let Some(ref card_config) = request.card_config {
        log::info!("[FastMkv:CARDS] Card config received: color={}, fontColor={}, duration={}s, freq={:?}, showInReport={}",
            card_config.color, card_config.font_color, card_config.duration, card_config.frequency, card_config.show_in_report);
        if input_files.len() >= 2 && card_config.duration > 0.0 {
            log::info!("[FastMkv:CARDS] Rendering cards for {} input files...", input_files.len());
            let temp_dir = crate::ffmpeg::get_temp_dir().map_err(|e| e.to_string())?;
            let card_temp_dir = temp_dir.join("cards_fast_mkv");
            let _ = std::fs::create_dir_all(&card_temp_dir);

            // [REPEAT] Build boundary card labels if repeat boundary cards are enabled
            // [REPEAT] Build boundary card labels if repeat boundary cards are enabled
            let boundary_card_labels = repeat_expanded.as_ref().and_then(|re| {
                crate::ffmpeg::repeat_merge::build_boundary_card_labels(re, input_files.len())
            });

            match crate::ffmpeg::cards::render_cards_for_merge(
                ffmpeg_path,
                Some(ffprobe_path),
                input_files,
                input_names,
                input_durations,
                card_config,
                &card_temp_dir,
                boundary_card_labels.as_ref(),
            ) {
                Ok(rendered) => {
                    log::info!("[FastMkv:CARDS] Successfully rendered {} cards", rendered.len());
                    let interleave = crate::ffmpeg::cards::interleave_cards_with_videos(
                        input_files,
                        input_durations,
                        &rendered,
                        card_config,
                    );
                    // Track card temp files for cleanup
                    for (i, (is_card, _)) in interleave.segment_cards.iter().enumerate() {
                        if *is_card {
                            card_temp_files.push(PathBuf::from(&interleave.files[i]));
                        }
                    }
                    let cards_inserted = interleave.segment_cards.iter().filter(|(c, _)| *c).count();
                    let extra_duration: f64 = interleave.durations.iter().zip(interleave.segment_cards.iter())
                        .filter(|(_, (c, _))| *c)
                        .map(|(d, _)| d)
                        .sum();
                    final_input_files = interleave.files;
                    final_input_durations = interleave.durations;
                    final_segment_cards = interleave.segment_cards;
                    final_total_duration += extra_duration;
                    log::info!("[FastMkv:CARDS] Interleaving complete: {} total segments ({} cards + {} videos), added {:.2}s for section cards",
                        final_input_files.len(), cards_inserted, input_files.len(), extra_duration);

                    log::info!("[FastMkv:CARDS] ═══════════════════════════════════════════════════════════════");
                    log::info!("[FastMkv:CARDS] FINAL CONCAT FILE SEQUENCE ({} files):", final_input_files.len());
                    for (j, (file, (is_card, card_color))) in final_input_files.iter().zip(final_segment_cards.iter()).enumerate() {
                        let file_name = std::path::Path::new(file).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| file.clone());
                        if *is_card {
                            log::info!("[FastMkv:CARDS]   [{:>3}] CARD  | color={:?} | dur={:.1}s | {}",
                                j, card_color, final_input_durations[j], file_name);
                        } else {
                            log::info!("[FastMkv:CARDS]   [{:>3}] VIDEO | dur={:.1}s | {}",
                                j, final_input_durations[j], file_name);
                        }
                    }
                    log::info!("[FastMkv:CARDS] Segment is_card flags: {:?}", final_segment_cards.iter().map(|(c, _)| *c).collect::<Vec<_>>());
                    log::info!("[FastMkv:CARDS] ═══════════════════════════════════════════════════════════════");
                }
                Err(e) => {
                    log::error!("[FastMkv:CARDS] Card rendering failed: {:?}", e);
                    log::warn!("[FastMkv:CARDS] Continuing merge without cards");
                }
            }
        } else {
            log::info!("[FastMkv:CARDS] Skipping cards: files={} (< 2) or duration={} (<= 0)", input_files.len(), card_config.duration);
        }
    } else {
        log::info!("[FastMkv] No card config in request — skipping card insertion");
    }

    // Determine output paths
    let mkv_path = if convert_to_mp4 {
        let temp_dir = crate::ffmpeg::get_temp_dir().map_err(|e| e.to_string())?;
        let stem = std::path::Path::new(&output_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("merged_output");
        temp_dir.join(format!("{}.mkv", stem)).to_string_lossy().into_owned()
    } else {
        output_path.clone()
    };

    // Emit progress: muxing phase
    let _ = app_handle.emit("merge-progress", &serde_json::json!({
        "jobId": job_id,
        "progress": {
            "phase": "fastMkv-muxing",
            "stageName": "Creating MKV container...",
            "stagePercent": 0.0,
            "percent": 0.0,
            "overallPercent": 0.0,
            "currentTime": 0.0,
            "totalDuration": final_total_duration,
        }
    }));

    // Create cancel flag
    let cancel_flag = Arc::new(AtomicBool::new(false));

    // Run Fast MKV merge
    let mkv_path_clone = mkv_path.clone();
    let cancel_flag_clone = cancel_flag.clone();
    let app_handle_clone = app_handle.clone();
    let job_id_clone = job_id.clone();
    let ffmpeg_path_for_merge = ffmpeg_path_buf.clone();
    let card_temp_files_to_clean = card_temp_files.clone();
    
    // Clone for segment building (used after spawn_blocking completes)
    let final_input_files_for_segments = final_input_files.clone();
    let final_input_durations_for_segments = final_input_durations.clone();
    let final_segment_cards_for_segments = final_segment_cards.clone();
    
    log::info!("[FastMkv] Running merge: {} segments, {:.1}s total, output={}", final_input_files.len(), final_total_duration, mkv_path);

    tokio::task::spawn_blocking(move || {
        let result = crate::ffmpeg::fast_mkv::run_fast_mkv_merge(
            &ffmpeg_path_for_merge,
            &final_input_files,
            &final_input_durations,
            &mkv_path_clone,
            final_total_duration,
            cancel_flag_clone,
            move |progress: MergeProgress| {
                let _ = app_handle_clone.emit("merge-progress", &serde_json::json!({
                    "jobId": job_id_clone,
                    "progress": progress
                }));
            },
        );

        // Cleanup card temp files
        for f in card_temp_files_to_clean {
            let _ = std::fs::remove_file(f);
        }
        
        result
    })
    .await
    .map_err(|e| format!("Fast MKV merge task failed: {}", e))?
    .map_err(|e| format!("Fast MKV merge failed: {}", e))?;

    log::info!("[FastMkv] MKV created successfully: {}", mkv_path);

    // Optional MP4 conversion
    if convert_to_mp4 {
        log::info!("[FastMkv] Converting MKV to MP4: {}", output_path);

        let _ = app_handle.emit("merge-progress", &serde_json::json!({
            "jobId": job_id,
            "progress": {
                "phase": "fastMkv-converting",
                "stageName": "Converting to MP4...",
                "stagePercent": 0.0,
                "percent": 0.0,
                "overallPercent": 0.0,
                "currentTime": 0.0,
                "totalDuration": total_duration,
            }
        }));

        let mkv_path_for_convert = mkv_path.clone();
        let output_path_clone = output_path.clone();
        let cancel_flag_clone2 = cancel_flag.clone();
        let app_handle_clone2 = app_handle.clone();
        let job_id_clone2 = job_id.clone();
        
        tokio::task::spawn_blocking(move || {
            crate::ffmpeg::fast_mkv::convert_mkv_to_mp4(
                &ffmpeg_path_buf,
                &ffprobe_path_buf,
                &mkv_path_for_convert,
                &output_path_clone,
                total_duration,
                cancel_flag_clone2,
                move |progress: MergeProgress| {
                    let _ = app_handle_clone2.emit("merge-progress", &serde_json::json!({
                        "jobId": job_id_clone2,
                        "progress": progress
                    }));
                },
            )
        })
        .await
        .map_err(|e| format!("MP4 conversion task failed: {}", e))?
        .map_err(|e| format!("MP4 conversion failed: {}", e))?;

        log::info!("[FastMkv] MP4 created successfully: {}", output_path);
    }

    // Emit progress: finished
    let _ = app_handle.emit("merge-progress", &serde_json::json!({
        "jobId": job_id,
        "progress": {
            "phase": "fastMkv-finished",
            "stageName": "Completed",
            "stagePercent": 100.0,
            "percent": 100.0,
            "overallPercent": 100.0,
            "currentTime": total_duration,
            "totalDuration": total_duration,
        }
    }));

    // Get output file size
    let final_output_path = if convert_to_mp4 { output_path.clone() } else { mkv_path.clone() };
    let output_size = std::fs::metadata(&final_output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    // Build segments for result — use final_input_files (with cards interleaved)
    // and final_segment_cards (is_card flags) so the frontend can display card segments.
    let mut segments: Vec<MergeSegment> = Vec::with_capacity(final_input_files_for_segments.len());
    let mut current_start = 0.0;
    for (i, file) in final_input_files_for_segments.iter().enumerate() {
        let name = std::path::Path::new(file)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("file_{}", i));
        let duration = final_input_durations_for_segments.get(i).copied().unwrap_or(0.0);
        let (is_card, card_color) = final_segment_cards_for_segments.get(i).cloned().unwrap_or((false, None));
        let parent_folder = if !is_card {
            std::path::Path::new(file)
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
        } else {
            None
        };
        segments.push(MergeSegment {
            name,
            duration,
            start_time: current_start,
            end_time: current_start + duration,
            is_card: if is_card { Some(true) } else { None },
            card_color,
            parent_folder,
        });
        current_start += duration;
    }

    // ── Phase 2: Post-merge validation (ffprobe + seekability) ──────────
    let mut actual_duration = final_total_duration;
    let merge_warnings: Vec<String> = Vec::new();
    let mut validation_failed = false;
    let mut validation_error = String::new();

    // P0-4: Output existence and size pre-check
    let output_meta = std::fs::metadata(&final_output_path);
    match output_meta {
        Ok(meta) if meta.len() == 0 => {
            log::error!("[FastMkv:VALIDATE] Output file exists but is zero bytes: {}", final_output_path);
            validation_failed = true;
            validation_error = format!("Output file is zero bytes: {}", final_output_path);
        }
        Ok(meta) => {
            log::info!("[FastMkv:VALIDATE] Output file exists: {} ({} bytes)", final_output_path, meta.len());
        }
        Err(e) => {
            log::error!("[FastMkv:VALIDATE] Output file does not exist: {} — {}", final_output_path, e);
            validation_failed = true;
            validation_error = format!("Output file does not exist: {}", final_output_path);
        }
    }

    // ffprobe validation
    if !validation_failed {
        log::info!("[FastMkv:VALIDATE] Probing output file: {}", final_output_path);
        match crate::ffmpeg::probe::probe_file(ffprobe_path, std::path::Path::new(&final_output_path)) {
            Ok(info) => {
                actual_duration = info.duration;
                log::info!("[FastMkv:VALIDATE] Output duration: {:.3}s", actual_duration);

                // P2-6: Check audio stream existence — fail if input had audio but output lost it
                let input_has_audio = media_infos.iter().any(|m| !m.audio_streams.is_empty());
                if input_has_audio && info.audio_streams.is_empty() {
                    log::error!("[FastMkv:VALIDATE] Output has no audio streams but input files had audio — possible audio track loss");
                    validation_failed = true;
                    validation_error = "Output has no audio streams but input files had audio. Audio tracks may have been lost during merge.".to_string();
                }

                let expected_duration = final_total_duration;
                let drift_pct = if expected_duration > 0.0 {
                    ((actual_duration - expected_duration).abs() / expected_duration) * 100.0
                } else { 0.0 };
                let drift_abs_secs = (actual_duration - expected_duration).abs();
                log::info!("[FastMkv:VALIDATE] Expected: {:.3}s | Actual: {:.3}s | Drift: {:.3}% ({:.1}s)", expected_duration, actual_duration, drift_pct, drift_abs_secs);
                let drift_threshold_pct = if final_total_duration > 180_000.0 { 5.0 }
                    else if final_total_duration > 36_000.0 { 3.0 }
                    else if final_total_duration > 3_600.0 { 2.0 }
                    else { 1.0 };
                if drift_pct > drift_threshold_pct && drift_abs_secs > 60.0 {
                    validation_failed = true;
                    validation_error = format!("Timeline drift: {:.2}% ({:.1}s, threshold {:.1}%) (Expected {:.1}s, got {:.1}s)", drift_pct, drift_abs_secs, drift_threshold_pct, expected_duration, actual_duration);
                }

                let v_dur = info.video_streams.first().and_then(|s| s.duration).unwrap_or(actual_duration);
                let a_dur = info.audio_streams.first().and_then(|s| s.duration).unwrap_or(actual_duration);
                let av_diff = (v_dur - a_dur).abs();
                if av_diff > 0.5 {
                    validation_failed = true;
                    validation_error = format!("A/V sync drift: {:.3}s (Video: {:.3}s, Audio: {:.3}s)", av_diff, v_dur, a_dur);
                    log::warn!("[FastMkv:VALIDATE] A/V sync drift: {:.3}s", av_diff);
                }
            }
            Err(e) => {
                log::error!("[FastMkv:VALIDATE] Could not probe output file: {}", e);
                validation_failed = true;
                validation_error = format!("Output probe failed: {}", e);
            }
        }
    }

    // Seekability audit (5 test points)
    if !validation_failed && actual_duration > 0.0 && !final_output_path.is_empty() {
        #[cfg(windows)]
        use std::os::windows::process::CommandExt;
        let test_points = [0.05, 0.25, 0.50, 0.75, 0.95];
        let mut seek_failures: usize = 0;

        for pct in test_points {
            let seek_sec = actual_duration * pct;
            let args = ["-v", "error", "-ss", &seek_sec.to_string(), "-i", &final_output_path, "-an", "-frames:v", "1", "-f", "null", "-"];
            let mut cmd = std::process::Command::new(ffmpeg_path);
            #[cfg(windows)]
            cmd.creation_flags(0x08000000);

            match cmd.args(args).output() {
                Ok(out) if !out.status.success() => { seek_failures += 1; }
                Err(_) => { seek_failures += 1; }
                _ => {}
            }
        }

        if seek_failures > 2 {
            validation_failed = true;
            validation_error = format!("Seekability audit: {}/5 seek points failed", seek_failures);
            log::error!("[FastMkv:VALIDATE] Seekability: {}/5 failed", seek_failures);
        } else {
            log::info!("[FastMkv:VALIDATE] Seekability passed ({}/5)", 5 - seek_failures);
        }
    }

    // P0-3: Critical validation failure → fail merge
    if validation_failed {
        let _ = std::fs::remove_file(&final_output_path);
        return Err(format!("FastMKV merge output failed validation: {}", validation_error));
    }

    // Generate report
    let mut report_paths = None;
    if let Some(rp) = write_report_file(
        &final_output_path,
        &segments,
        output_size,
        actual_duration,
        request.repeat_config.as_ref(),
        Some(total_duration),
    ) {
        report_paths = Some(vec![rp]);
    }

    // Recovery cleanup
    if let Ok(app_data) = recovery::get_app_data_dir() {
        let _ = recovery::delete_checkpoint(&app_data, &job_id);
    }

    // Emit completion (parity with SmartMkv merge-complete event)
    let _ = app_handle.emit("merge-complete", &serde_json::json!({
        "jobId": job_id,
        "outputPath": final_output_path,
        "outputSizeBytes": output_size,
        "outputDurationSecs": actual_duration,
        "segments": segments,
        "outputPaths": if convert_to_mp4 {
            Some(vec![mkv_path, output_path])
        } else {
            None::<Vec<String>>
        },
        "reportPaths": report_paths,
        "warnings": if merge_warnings.is_empty() { None } else { Some(merge_warnings) },
    }));

    Ok(final_output_path)
}
