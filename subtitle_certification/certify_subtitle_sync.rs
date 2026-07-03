// ──────────────────────────────────────────────────────────────────────────────
// SUBTITLE / AUDIO / VIDEO SYNC CERTIFICATION
// ──────────────────────────────────────────────────────────────────────────────
//
// Tests all 4 subtitle modes (Embed, Burn, ExportSrt, SrtMergeOnly) to
// determine whether merged subtitles are synchronized with merged audio/video.
//
// Test Setup:
//   Video 1 (10s) + SRT cue at 05:00  -> "Subtitle from FILE ONE"
//   Video 2 (10s) + SRT cue at 05:00  -> "Subtitle from FILE TWO"
//   Video 3 (10s) + SRT cue at 05:00  -> "Subtitle from FILE THREE"
//
// Expected merged output:
//   File 1 subtitle at 00:00:05
//   File 2 subtitle at 00:00:15  (10s offset applied)
//   File 3 subtitle at 00:00:25  (20s offset applied)
//
// If timestamps are NOT offset, all subtitles appear at 00:00:05 -> BUG CONFIRMED

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const FIXTURE_DIR: &str = "./../subtitle_certification";

fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  SUBTITLE / AUDIO / VIDEO SYNC CERTIFICATION");
    println!("═══════════════════════════════════════════════════════════════");
    println!();

    let fixture_dir = PathBuf::from(FIXTURE_DIR);
    let temp_dir = fixture_dir.join("output");
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp output dir");

    // Resolve FFmpeg paths
    let ffmpeg = playlist_merger_lib::certification_api::find_ffmpeg(None::<&str>)
        .unwrap_or_else(|_| PathBuf::from("ffmpeg"));
    let ffprobe = playlist_merger_lib::certification_api::find_ffprobe(None::<&str>)
        .unwrap_or_else(|_| PathBuf::from("ffprobe"));

    println!("FFmpeg  at: {}", ffmpeg.display());
    println!("FFprobe at: {}", ffprobe.display());
    println!();

    // Build canonical file lists
    let video_rel = ["video_1.mp4", "video_2.mp4", "video_3.mp4"];
    let srt_rel   = ["video_1.srt", "video_2.srt", "video_3.srt"];

    let mut abs_video_paths: Vec<String> = Vec::new();
    let mut abs_sub_paths:  Vec<Option<String>> = Vec::new();
    let mut durations: Vec<f64> = Vec::new();

    for fname in &video_rel {
        let p = fixture_dir.join(fname);
        let abs = std::fs::canonicalize(&p)
            .unwrap_or_else(|_| p.clone());
        abs_video_paths.push(abs.to_string_lossy().to_string());
        let d = probe_duration(&ffprobe, &abs);
        durations.push(d);
        println!("  {:20} -> {:.2} s", fname, d);
    }

    for fname in &srt_rel {
        let p = fixture_dir.join(fname);
        let abs = std::fs::canonicalize(&p).unwrap_or(p);
        abs_sub_paths.push(Some(abs.to_string_lossy().to_string()));
    }

    let total_duration: f64 = durations.iter().sum();
    println!("\n  Total duration: {:.2}s (expect 30s)", total_duration);
    println!();

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let progress = |_: playlist_merger_lib::certification_api::MergeProgress| {};

    let modes: Vec<(&str, &str, _)> = vec![
        ("embed",      "Embed (stream-copy)",
         playlist_merger_lib::certification_api::SubtitleMode::Embed),
        ("burn",       "Burn (re-encode)",
         playlist_merger_lib::certification_api::SubtitleMode::Burn),
        ("export_srt", "Export SRT",
         playlist_merger_lib::certification_api::SubtitleMode::ExportSrt),
        ("srt_merge",  "SRT merge only",
         playlist_merger_lib::certification_api::SubtitleMode::SrtMergeOnly),
    ];

    // Shared input names
    let input_names: Vec<String> = vec!["Video1".into(), "Video2".into(), "Video3".into()];

    for (tag, desc, mode) in &modes {
        println!("--- Mode: {} ({}) -------------------------", tag, desc);

        let out_mp4 = temp_dir.join(format!("merged_{}.mp4", tag));
        let concat_list = temp_dir.join(format!("concat_{}.txt", tag));
        let subs_list   = temp_dir.join(format!("subs_{}.txt", tag));

        // 1. Write video concat list
        let vrefs: Vec<&Path> = abs_video_paths.iter().map(|s| Path::new(s.as_str())).collect();
        if let Err(e) = playlist_merger_lib::certification_api::write_concat_list_with_durations(
            &vrefs, Some(&durations), &concat_list, true,
        ) {
            eprintln!("  FAIL: write_concat_list -> {}", e);
            continue;
        }

        // 2. Write subtitle concat list (subtitle paths are already absolute)
        let sub_ok = playlist_merger_lib::certification_api::write_subtitle_concat_list(
            &abs_sub_paths, &durations, &subs_list, &temp_dir, "srt",
        ).is_ok();

        let sub_list_path = if sub_ok { Some(subs_list.clone()) } else { None };

        // 3. Build config
        let config = playlist_merger_lib::certification_api::MergeConfig {
            input_files:        abs_video_paths.clone(),
            input_names:        input_names.clone(),
            input_durations:    durations.clone(),
            subtitle_list_path: sub_list_path.clone(),
            output_path:        out_mp4.to_string_lossy().to_string(),
            total_duration,
            mode:               playlist_merger_lib::certification_api::MergeMode::Lossless,
            video_codec:        None,
            audio_codec:        None,
            video_crf:          None,
            video_preset:       None,
            audio_bitrate:      None,
            target_resolution:  None,
            target_fps:         None,
            hw_accel:           None,
            split_config:       None,
            subtitle_files:     abs_sub_paths.clone(),
            card_config:        None,
            segment_is_card:    vec![false, false, false],
            naming_config:      None,
            subtitle_mode:      mode.clone(),
            export_merged_srt:  true,
            burn_subtitle_path: None,
            mkvmerge_succeeded_before_ffmpeg: false,
        };

        // 4. Run merge
        let t0 = std::time::Instant::now();
        let result = playlist_merger_lib::certification_api::run_merge_blocking(
            &ffmpeg, &config, &concat_list, cancel_flag.clone(), progress,
        );
        let elapsed = t0.elapsed();
        let _ = std::fs::remove_file(&concat_list);
        let _ = std::fs::remove_file(&subs_list);

        // If embed/burn/export with Lossless failed, try Custom for burn
        let mut final_path = out_mp4.clone();
        if let Err(ref e) = result {
            eprintln!("  Merge failed: {}", e);

            if *tag == "burn" {
                println!("  Burn with Lossless not possible -> trying Custom + pre-generated SRT...");
                let burn_mp4 = temp_dir.join("merged_burn_custom.mp4");
                let burn_srt = temp_dir.join("merged_burn_sub.srt");
                // generate the merged SRT first
                let _ = playlist_merger_lib::certification_api::generate_merged_srt(
                    &ffmpeg, &subs_list, &burn_srt,
                );
                if !burn_srt.exists() {
                    eprintln!("  Could not generate burn SRT, skip");
                    continue;
                }
                let cfg2 = playlist_merger_lib::certification_api::MergeConfig {
                    output_path:        burn_mp4.to_string_lossy().to_string(),
                    mode:               playlist_merger_lib::certification_api::MergeMode::Custom,
                    video_codec:        Some("libx264".into()),
                    audio_codec:        Some("aac".into()),
                    video_crf:          Some(23),
                    video_preset:       Some("veryfast".into()),
                    audio_bitrate:      Some("128k".into()),
                    burn_subtitle_path: Some(burn_srt.clone()),
                    ..config
                };
                // Need to re-create the concat list (already deleted above)
                if let Err(e2) = playlist_merger_lib::certification_api::write_concat_list_with_durations(
                    &vrefs, Some(&durations), &concat_list, true,
                ) {
                    eprintln!("  Failed to re-create concat list: {}", e2);
                    continue;
                }
                match playlist_merger_lib::certification_api::run_merge_blocking(
                    &ffmpeg, &cfg2, &concat_list, cancel_flag.clone(), progress,
                ) {
                    Ok(_) => { final_path = burn_mp4; println!("  Burn with Custom OK"); }
                    Err(e3) => { eprintln!("  Burn retry also failed: {}", e3); continue; }
                }
            } else {
                continue;
            }
        }

        println!("  Merge OK  ({:.1}s)", elapsed.as_secs_f64());

        // 5. Probe output
        let sz = std::fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
        println!("  Size: {:.1} MB", sz as f64 / 1_000_000.0);

        let probe = probe_media(&ffprobe, &final_path);
        if let Ok(ref info) = probe {
            println!("  Streams: {}v / {}a / {}s",
                info.video_count, info.audio_count, info.subtitle_count);
            for s in &info.subtitle_streams {
                println!("    sub #{}- {} lang={:?}", s.index, s.codec, s.language);
            }
        }

        // 6. Try to extract an SRT from the output (for embed/burn)
        let mut srt_content: Option<String> = None;

        // Check for generated SRT files first
        let generated_srts = vec![
            temp_dir.join(format!("merged_{}.srt", tag)),
            temp_dir.join("merged_burn_sub.srt"),
        ];
        for s in &generated_srts {
            if s.exists() {
                if let Ok(c) = std::fs::read_to_string(s) {
                    srt_content = Some(c);
                    break;
                }
            }
        }

        // If no generated SRT found, try extracting from the video
        if srt_content.is_none() && probe.as_ref().map(|p| p.subtitle_count > 0).unwrap_or(false) {
            let ex = temp_dir.join(format!("extracted_{}.srt", tag));
            if std::process::Command::new(&ffmpeg)
                .args(&["-y", "-i", &final_path.to_string_lossy(),
                       "-map", "0:s:0", "-c:s", "srt",
                       &ex.to_string_lossy()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false) && ex.exists()
            {
                srt_content = std::fs::read_to_string(&ex).ok();
            }
        }

        // 7. Analyse SRT timestamps
        if let Some(ref srt) = srt_content {
            println!("\n  -- Extracted subtitle timestamps --");
            analyze_srt_timestamps(srt, &durations);
        } else {
            println!("  (no subtitle SRT content available)");
        }
        println!();
    }

    println!("═══════════════════════════════════════════════════════════════");
    println!("  CERTIFICATION SUMMARY");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("  Expected (sync CORRECT):");
    println!("    Cue #1 (File 1) @ 00:00:05,000");
    println!("    Cue #2 (File 2) @ 00:00:15,000  (+10s offset)");
    println!("    Cue #3 (File 3) @ 00:00:25,000  (+20s offset)");
    println!();
    println!("  Expected (sync BROKEN):");
    println!("    All cues @ 00:00:05,000  (no offset applied)");
    println!();
    println!("  See per-mode output above for actual evidence.");
    println!("═══════════════════════════════════════════════════════════════");
}

// -----------------------------------------------------------------------
fn probe_duration(ffprobe: &Path, file: &Path) -> f64 {
    std::process::Command::new(ffprobe)
        .args(&["-v", "error", "-show_entries", "format=duration",
                "-of", "default=noprint_wrappers=1:nokey=1",
                &file.to_string_lossy()])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(0.0)
}

struct MediaProbe {
    video_count: usize,
    audio_count: usize,
    subtitle_count: usize,
    subtitle_streams: Vec<SubStreamInfo>,
}

struct SubStreamInfo { index: u32, codec: String, language: Option<String> }

fn probe_media(ffprobe: &Path, file: &Path) -> Result<MediaProbe, String> {
    let o = std::process::Command::new(ffprobe)
        .args(&["-v", "error",
                "-show_entries", "stream=index,codec_type,codec_name:stream_tags=language",
                "-of", "json", &file.to_string_lossy()])
        .output().map_err(|e| format!("ffprobe: {}", e))?;
    let j: serde_json::Value =
        serde_json::from_slice(&o.stdout).map_err(|e| format!("json: {}", e))?;
    let mut v=0usize; let mut a=0usize; let mut s=0usize; let mut ss=Vec::new();
    if let Some(arr) = j["streams"].as_array() {
        for st in arr {
            match st["codec_type"].as_str().unwrap_or("") {
                "video" => v += 1,
                "audio" => a += 1,
                "subtitle" => {
                    s += 1;
                    ss.push(SubStreamInfo {
                        index: st["index"].as_u64().unwrap_or(0) as u32,
                        codec: st["codec_name"].as_str().unwrap_or("?").into(),
                        language: st["tags"]["language"].as_str().map(String::from),
                    });
                }
                _ => {}
            }
        }
    }
    Ok(MediaProbe { video_count: v, audio_count: a, subtitle_count: s, subtitle_streams: ss })
}

// -----------------------------------------------------------------------
fn analyze_srt_timestamps(content: &str, _source_durations: &[f64]) {
    let mut cues: Vec<(f64, f64, String)> = Vec::new();
    let (mut cs, mut ce, mut ct) = (None, None, String::new());

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            if let (Some(s), Some(e)) = (cs, ce) {
                cues.push((s, e, ct.trim().to_string()));
            }
            cs = None; ce = None; ct.clear();
            continue;
        }
        if line.chars().all(|c| c.is_ascii_digit()) && cs.is_none() { continue; }
        if line.contains("-->") {
            let parts: Vec<&str> = line.split("-->").collect();
            if parts.len() == 2 {
                cs = parse_srt(parts[0].trim());
                ce = parse_srt(parts[1].trim());
            }
        } else {
            if !ct.is_empty() { ct.push('\n'); }
            ct.push_str(line);
        }
    }
    if let (Some(s), Some(e)) = (cs, ce) { cues.push((s, e, ct.trim().to_string())); }

    if cues.is_empty() {
        println!("  (no subtitle cues found)");
        return;
    }

    println!("  {} cue(s):", cues.len());
    for (i, (s, e, t)) in cues.iter().enumerate() {
        let txt = if t.len() > 50 { format!("{}...", &t[..50]) } else { t.clone() };
        println!("    #{:2}: {} --> {} | \"{}\"",
            i+1, fmt_srt(*s), fmt_srt(*e), txt);
    }

    if cues.len() < 2 {
        println!("  (only 1 cue, can't determine offset)");
        return;
    }

    let any_5  = cues.iter().skip(1).any(|c| (c.0 - 5.0).abs() < 1.0);
    let any_15 = cues.iter().any(|c| (c.0 - 15.0).abs() < 2.0);
    let any_25 = cues.iter().any(|c| (c.0 - 25.0).abs() < 2.0);

    if any_15 || any_25 {
        println!("  -> SYNC OK: timestamps correctly offset by cumulative duration");
        if any_15 { println!("     -> cue near 15s = File 2 at correct position"); }
        if any_25 { println!("     -> cue near 25s = File 3 at correct position"); }
    } else if any_5 && cues.len() >= 2 {
        println!("  -> SYNC BROKEN: multiple cues still at ~5s (no offset applied)");
        println!("     Expected File2@15s, File3@25s but all near 5s");
    } else {
        println!("  -> unknown timestamp pattern");
    }
}

fn parse_srt(s: &str) -> Option<f64> {
    let s = s.replace(',', ".");
    let p: Vec<&str> = s.split(':').collect();
    if p.len() == 3 {
        Some(p[0].parse::<f64>().ok()? * 3600.0
           + p[1].parse::<f64>().ok()? * 60.0
           + p[2].parse::<f64>().ok()?)
    } else { None }
}

fn fmt_srt(sec: f64) -> String {
    let ms = (sec * 1000.0).round() as u64;
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1_000;
    let f = ms % 1_000;
    format!("{:02}:{:02}:{:02},{:03}", h, m, s, f)
}
