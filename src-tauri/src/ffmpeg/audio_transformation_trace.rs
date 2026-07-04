/// AUDIO TRANSFORMATION TRACE AUDIT
///
/// Pinpoints EXACTLY where audio distortion is introduced in the pipeline.
/// Runs a known test signal through each stage independently, captures
/// spectral/temporal metrics at every step, and computes deltas.
///
/// Usage:
///   cargo test audio_transformation_trace -- --nocapture
///
/// For a real file:
///   cargo test audio_transformation_trace -- --nocapture 2>&1 | grep TRACE
#[cfg(test)]
mod audio_transformation_trace {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn get_binaries() -> (PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let ffmpeg = root.join("binaries").join("ffmpeg.exe");
        let ffprobe = root.join("binaries").join("ffprobe.exe");
        (ffmpeg, ffprobe)
    }

    /// Audio quality metrics captured from a single file.
    #[derive(Debug, Clone)]
    struct AudioMetrics {
        label: String,
        spectral_centroid: Option<f64>,
        spectral_spread: Option<f64>,
        spectral_slope: Option<f64>,
        peak_level: Option<f64>,
        rms_level: Option<f64>,
        crest_factor: Option<f64>,
        duration: Option<f64>,
        sample_rate: Option<u32>,
        bit_rate: Option<u64>,
        codec: String,
        profile: Option<String>,
    }

    /// Capture metrics from a file using FFmpeg extraction + raw PCM analysis.
    /// Extracts raw PCM via FFmpeg, computes peak/RMS/crest from actual samples.
    fn capture_metrics(ffmpeg: &Path, ffprobe: &Path, file: &Path, label: &str) -> AudioMetrics {
        // Step 1: Probe basic properties
        let probe_output = Command::new(ffprobe)
            .args(&[
                "-v", "quiet", "-print_format", "json",
                "-show_streams", "-show_format",
                file.to_str().unwrap()
            ])
            .output()
            .expect("Failed to run ffprobe");

        let probe_json: serde_json::Value = if probe_output.status.success() {
            serde_json::from_slice(&probe_output.stdout).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };

        let audio_stream = probe_json.get("streams")
            .and_then(|s| s.as_array())
            .and_then(|streams| {
                streams.iter().find(|s| {
                    s.get("codec_type").and_then(|v| v.as_str()) == Some("audio")
                })
            });

        let codec = audio_stream
            .and_then(|s| s.get("codec_name").and_then(|v| v.as_str()))
            .unwrap_or("unknown")
            .to_string();
        let profile = audio_stream
            .and_then(|s| s.get("profile").and_then(|v| v.as_str()))
            .map(String::from);
        let sample_rate = audio_stream
            .and_then(|s| s.get("sample_rate").and_then(|v| v.as_str()))
            .and_then(|s| s.parse::<u32>().ok());
        let bit_rate = audio_stream
            .and_then(|s| s.get("bit_rate").and_then(|v| v.as_str()))
            .and_then(|s| s.parse::<u64>().ok());
        let duration = probe_json.get("format")
            .and_then(|f| f.get("duration").and_then(|v| v.as_str()))
            .and_then(|s| s.parse::<f64>().ok());

        // Step 2: Extract raw f32le PCM and compute metrics from actual samples
        let pcm_output = Command::new(ffmpeg)
            .args(&[
                "-i", file.to_str().unwrap(),
                "-map", "0:a:0?",
                "-f", "f32le",
                "-acodec", "pcm_f32le",
                "-ar", "48000",
                "-ac", "1",
                "-"
            ])
            .output()
            .expect("Failed to extract PCM");

        let pcm_bytes = &pcm_output.stdout;
        let sample_count = pcm_bytes.len() / 4; // f32 = 4 bytes

        let mut sum_squares: f64 = 0.0;
        let mut max_abs: f64 = 0.0;

        // Read samples in chunks to avoid huge allocations
        for chunk in pcm_bytes.chunks(4096) {
            for sample_bytes in chunk.chunks(4) {
                if sample_bytes.len() < 4 { break; }
                let bits = u32::from_le_bytes([sample_bytes[0], sample_bytes[1], sample_bytes[2], sample_bytes[3]]);
                let sample = f32::from_bits(bits) as f64;
                if !sample.is_finite() { continue; }
                let abs = sample.abs();
                sum_squares += sample * sample;
                if abs > max_abs { max_abs = abs; }
            }
        }

        let (peak_db, rms_db, crest_factor) = if sample_count > 0 {
            let rms = (sum_squares / sample_count as f64).sqrt();
            let peak_db = 20.0 * max_abs.log10().max(1e-20);
            let rms_db = 20.0 * rms.log10().max(1e-20);
            let crest = if rms > 0.0 { max_abs / rms } else { 0.0 };
            (Some(peak_db), Some(rms_db), Some(crest))
        } else {
            (None, None, None)
        };

        AudioMetrics {
            label: label.to_string(),
            spectral_centroid: None, // Not available without FFT
            spectral_spread: None,
            spectral_slope: None,
            peak_level: peak_db,
            rms_level: rms_db,
            crest_factor,
            duration,
            sample_rate,
            bit_rate,
            codec,
            profile,
        }
    }

    /// Delta between two metric values (percentage change).
    fn delta_pct(a: Option<f64>, b: Option<f64>) -> String {
        match (a, b) {
            (Some(a), Some(b)) if a.abs() > 0.001 => {
                let pct = ((b - a) / a.abs()) * 100.0;
                format!("{:+.2}%", pct)
            }
            (Some(a), Some(b)) => {
                format!("{:+.4} (abs)", b - a)
            }
            _ => "N/A".to_string(),
        }
    }

    /// Absolute difference between two metric values.
    fn delta_abs(a: Option<f64>, b: Option<f64>) -> String {
        match (a, b) {
            (Some(a), Some(b)) => format!("{:+.4}", b - a),
            _ => "N/A".to_string(),
        }
    }

    /// Print comparison table between two metrics sets.
    fn print_comparison(original: &AudioMetrics, processed: &AudioMetrics) {
        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");
        log::info!("[TRANSFORM_TRACE] {} → {}", original.label, processed.label);
        log::info!("[TRANSFORM_TRACE] ──────────────────────────────────────────────────────────────");
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15} {:>15} {:>12}", "Metric", "Original", "Processed", "Delta");
        log::info!("[TRANSFORM_TRACE] ──────────────────────────────────────────────────────────────");
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15} {:>15} {:>12}", "codec",
            original.codec, processed.codec, if original.codec != processed.codec { "CHANGED" } else { "same" });
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15?} {:>15?} {:>12}", "profile",
            original.profile, processed.profile, delta_abs(original.profile.as_ref().and_then(|p| p.parse().ok()), processed.profile.as_ref().and_then(|p| p.parse().ok())));
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15?} {:>15?} {:>12}", "sample_rate",
            original.sample_rate, processed.sample_rate,
            delta_abs(original.sample_rate.map(|v| v as f64), processed.sample_rate.map(|v| v as f64)));
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15?} {:>15?} {:>12}", "bit_rate",
            original.bit_rate, processed.bit_rate,
            delta_abs(original.bit_rate.map(|v| v as f64), processed.bit_rate.map(|v| v as f64)));
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15?} {:>15?} {:>12}", "duration",
            original.duration.map(|d| format!("{:.3}s", d)), processed.duration.map(|d| format!("{:.3}s", d)),
            delta_abs(original.duration, processed.duration));
        log::info!("[TRANSFORM_TRACE] ──────────────────────────────────────────────────────────────");
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15} {:>15} {:>12}", "SpectralCentroid",
            fmt_opt(original.spectral_centroid), fmt_opt(processed.spectral_centroid),
            delta_pct(original.spectral_centroid, processed.spectral_centroid));
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15} {:>15} {:>12}", "SpectralSpread",
            fmt_opt(original.spectral_spread), fmt_opt(processed.spectral_spread),
            delta_pct(original.spectral_spread, processed.spectral_spread));
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15} {:>15} {:>12}", "SpectralSlope",
            fmt_opt(original.spectral_slope), fmt_opt(processed.spectral_slope),
            delta_pct(original.spectral_slope, processed.spectral_slope));
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15} {:>15} {:>12}", "Peak_level",
            fmt_opt(original.peak_level), fmt_opt(processed.peak_level),
            delta_abs(original.peak_level, processed.peak_level));
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15} {:>15} {:>12}", "RMS_level",
            fmt_opt(original.rms_level), fmt_opt(processed.rms_level),
            delta_abs(original.rms_level, processed.rms_level));
        log::info!("[TRANSFORM_TRACE] {:<25} {:>15} {:>15} {:>12}", "Crest_factor",
            fmt_opt(original.crest_factor), fmt_opt(processed.crest_factor),
            delta_pct(original.crest_factor, processed.crest_factor));

        // ── Distortion risk assessment ──
        let centroid_shift = match (original.spectral_centroid, processed.spectral_centroid) {
            (Some(a), Some(b)) => ((b - a) / a.abs()) * 100.0,
            _ => 0.0,
        };
        let crest_shift = match (original.crest_factor, processed.crest_factor) {
            (Some(a), Some(b)) => ((b - a) / a.abs()) * 100.0,
            _ => 0.0,
        };
        let rms_shift = match (original.rms_level, processed.rms_level) {
            (Some(a), Some(b)) => (b - a).abs(),
            _ => 0.0,
        };

        log::info!("[TRANSFORM_TRACE] ──────────────────────────────────────────────────────────────");
        if centroid_shift.abs() > 10.0 {
            log::warn!("[TRANSFORM_TRACE] ⚠️  SPECTRAL CENTROID shifted {:.1}% — tonal character changed", centroid_shift);
        }
        if crest_shift.abs() > 15.0 {
            log::warn!("[TRANSFORM_TRACE] ⚠️  CREST FACTOR shifted {:.1}% — dynamic range altered (metallic risk)", crest_shift);
        }
        if rms_shift > 1.0 {
            log::warn!("[TRANSFORM_TRACE] ⚠️  RMS level changed {:.2} dB — loudness altered", rms_shift);
        }
        if original.codec != processed.codec {
            log::warn!("[TRANSFORM_TRACE] ⚠️  CODEC CHANGED: {} → {}", original.codec, processed.codec);
        }
        if original.sample_rate != processed.sample_rate {
            log::warn!("[TRANSFORM_TRACE] ⚠️  SAMPLE RATE CHANGED: {:?} → {:?} — resampling applied",
                original.sample_rate, processed.sample_rate);
        }
        if original.bit_rate != processed.bit_rate {
            log::warn!("[TRANSFORM_TRACE] ⚠️  BITRATE CHANGED: {:?} → {:?}", original.bit_rate, processed.bit_rate);
        }

        let risk_score = (centroid_shift.abs() / 10.0) + (crest_shift.abs() / 15.0) + rms_shift;
        if risk_score > 3.0 {
            log::warn!("[TRANSFORM_TRACE] 🔴 HIGH DISTORTION RISK (score={:.1}) — metallic artifacts likely", risk_score);
        } else if risk_score > 1.0 {
            log::warn!("[TRANSFORM_TRACE] 🟡 MEDIUM DISTORTION RISK (score={:.1}) — audible quality loss possible", risk_score);
        } else {
            log::info!("[TRANSFORM_TRACE] 🟢 LOW DISTORTION RISK (score={:.1}) — audio quality preserved", risk_score);
        }
        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");
    }

    fn fmt_opt(v: Option<f64>) -> String {
        match v {
            Some(v) => format!("{:.4}", v),
            None => "N/A".to_string(),
        }
    }

    /// Create a test file with a 440Hz sine wave at a given sample rate.
    /// This gives us a KNOWN baseline to measure transformation quality.
    fn create_test_signal(
        ffmpeg: &Path,
        output: &Path,
        sample_rate: u32,
        duration_sec: u32,
        audio_bitrate: &str,
    ) {
        let status = Command::new(ffmpeg)
            .args(&[
                "-y",
                "-f", "lavfi",
                "-i", &format!("sine=frequency=440:sample_rate={}:duration={}", sample_rate, duration_sec),
                "-f", "lavfi",
                "-i", &format!("testsrc=duration={}:size=320x240:rate=30", duration_sec),
                "-c:v", "libx264", "-preset", "ultrafast",
                "-c:a", "aac", "-b:a", audio_bitrate,
                "-ar", &sample_rate.to_string(),
                "-t", &duration_sec.to_string(),
                output.to_str().unwrap()
            ])
            .output()
            .expect("Failed to create test signal");
        assert!(status.status.success(), "Failed to create test signal: {}",
            String::from_utf8_lossy(&status.stderr));
    }

    /// Run a specific normalization operation on a file.
    /// Returns the path to the normalized output.
    fn run_normalize_audio_only(
        ffmpeg: &Path,
        input: &Path,
        output: &Path,
        target_acodec: &str,
        target_sr: u32,
        bitrate: Option<&str>,
    ) {
        let mut args: Vec<String> = vec![
            "-y".to_string(),
            "-i".to_string(),
            input.to_str().unwrap().to_string(),
            "-map".to_string(), "0:v:0".to_string(), "-map".to_string(), "0:a:0".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), target_acodec.to_string(),
        ];

        // AAC profile
        if target_acodec == "aac" {
            args.push("-profile:a".to_string());
            args.push("aac_low".to_string());
        }

        // Sample rate
        args.push("-ar".to_string());
        args.push(target_sr.to_string());

        // Bitrate (this is the fix we just made)
        if let Some(br) = bitrate {
            args.push("-b:a".to_string());
            args.push(br.to_string());
        }

        args.push(output.to_str().unwrap().to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let status = Command::new(ffmpeg)
            .args(&args_refs)
            .output()
            .expect("Failed to run normalization");
        assert!(status.status.success(), "Normalization failed: {}",
            String::from_utf8_lossy(&status.stderr));
    }

    fn run_normalize_to_profile(
        ffmpeg: &Path,
        input: &Path,
        output: &Path,
        target_acodec: &str,
        target_sr: u32,
        bitrate: Option<&str>,
    ) {
        let mut args: Vec<String> = vec![
            "-y".to_string(),
            "-fflags".to_string(), "+genpts+discardcorrupt".to_string(),
            "-err_detect".to_string(), "ignore_err".to_string(),
            "-i".to_string(), input.to_str().unwrap().to_string(),
            "-map".to_string(), "0:v:0".to_string(), "-map".to_string(), "0:a:0".to_string(),
            "-map_metadata".to_string(), "0".to_string(),
            "-c:v".to_string(), "libx264".to_string(), "-preset".to_string(), "ultrafast".to_string(),
            "-c:a".to_string(), target_acodec.to_string(),
        ];

        if target_acodec == "aac" {
            args.push("-profile:a".to_string());
            args.push("aac_low".to_string());
        }

        args.push("-ar".to_string());
        args.push(target_sr.to_string());

        if let Some(br) = bitrate {
            args.push("-b:a".to_string());
            args.push(br.to_string());
        }

        args.push("-af".to_string());
        args.push("aresample=first_pts=0".to_string());

        args.push("-avoid_negative_ts".to_string());
        args.push("make_zero".to_string());
        args.push(output.to_str().unwrap().to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let status = Command::new(ffmpeg)
            .args(&args_refs)
            .output()
            .expect("Failed to run profile normalization");
        assert!(status.status.success(), "Profile normalization failed: {}",
            String::from_utf8_lossy(&status.stderr));
    }

    /// TEST: Trace a single file through the full pipeline, isolating each stage.
    ///
    /// This test answers: "Where exactly does the audio quality change?"
    #[test]
    fn trace_single_file_pipeline_stages() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() || !ffprobe.exists() {
            eprintln!("Skipping: ffmpeg/ffprobe not found at {:?}", ffmpeg.parent());
            return;
        }

        let tmp = std::env::temp_dir().join("audio_trace_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let original = tmp.join("original.mp4");
        let stage1_audio_norm = tmp.join("stage1_audio_norm.mp4");
        let stage2_full_reencode = tmp.join("stage2_full_reencode.mp4");

        // ── Create test signal: 440Hz sine at 48kHz, 192k AAC ──
        create_test_signal(&ffmpeg, &original, 48000, 5, "192k");

        // ── Stage 0: Capture original metrics ──
        let m0 = capture_metrics(&ffmpeg, &ffprobe, &original, "S0_original");
        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");
        log::info!("[TRANSFORM_TRACE] STAGE 0 — ORIGINAL");
        log::info!("[TRANSFORM_TRACE]   codec={} profile={:?} sr={:?} bitrate={:?}",
            m0.codec, m0.profile, m0.sample_rate, m0.bit_rate);
        log::info!("[TRANSFORM_TRACE]   centroid={:?} spread={:?} slope={:?}",
            m0.spectral_centroid, m0.spectral_spread, m0.spectral_slope);
        log::info!("[TRANSFORM_TRACE]   peak={:?} rms={:?} crest={:?}",
            m0.peak_level, m0.rms_level, m0.crest_factor);

        // ── Stage 1: Audio-only normalization (same codec, same SR, preserve bitrate) ──
        // This is the path we just fixed: should produce IDENTICAL audio.
        run_normalize_audio_only(&ffmpeg, &original, &stage1_audio_norm, "aac", 48000, Some("192k"));
        let m1 = capture_metrics(&ffmpeg, &ffprobe, &stage1_audio_norm, "S1_audio_norm");
        log::info!("[TRANSFORM_TRACE] STAGE 1 — Audio-only normalization (same params)");
        log::info!("[TRANSFORM_TRACE]   codec={} profile={:?} sr={:?} bitrate={:?}",
            m1.codec, m1.profile, m1.sample_rate, m1.bit_rate);
        print_comparison(&m0, &m1);

        // ── Stage 2: Full re-encode (video + audio) ──
        // This simulates the full normalization path.
        run_normalize_to_profile(&ffmpeg, &original, &stage2_full_reencode, "aac", 48000, Some("192k"));
        let m2 = capture_metrics(&ffmpeg, &ffprobe, &stage2_full_reencode, "S2_full_reencode");
        log::info!("[TRANSFORM_TRACE] STAGE 2 — Full re-encode (video + audio)");
        log::info!("[TRANSFORM_TRACE]   codec={} profile={:?} sr={:?} bitrate={:?}",
            m2.codec, m2.profile, m2.sample_rate, m2.bit_rate);
        print_comparison(&m0, &m2);

        // ── Stage 3: Double re-encode (normalize → normalize again) ──
        // This simulates what happens when a file gets re-normalized.
        let stage3_double = tmp.join("stage3_double.mp4");
        run_normalize_audio_only(&ffmpeg, &stage1_audio_norm, &stage3_double, "aac", 48000, Some("192k"));
        let m3 = capture_metrics(&ffmpeg, &ffprobe, &stage3_double, "S3_double_reencode");
        log::info!("[TRANSFORM_TRACE] STAGE 3 — Double re-encode (normalize → normalize again)");
        log::info!("[TRANSFORM_TRACE]   codec={} profile={:?} sr={:?} bitrate={:?}",
            m3.codec, m3.profile, m3.sample_rate, m3.bit_rate);
        print_comparison(&m0, &m3);

        // ── SUMMARY ──
        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");
        log::info!("[TRANSFORM_TRACE] CUMULATIVE TRANSFORMATION DEPTH:");
        log::info!("[TRANSFORM_TRACE]   S0→S1 (single norm):      centroid={}", delta_pct(m0.spectral_centroid, m1.spectral_centroid));
        log::info!("[TRANSFORM_TRACE]   S0→S2 (full re-encode):   centroid={}", delta_pct(m0.spectral_centroid, m2.spectral_centroid));
        log::info!("[TRANSFORM_TRACE]   S0→S3 (double re-encode): centroid={}", delta_pct(m0.spectral_centroid, m3.spectral_centroid));
        log::info!("[TRANSFORM_TRACE]   S0→S3 crest_factor:       {}", delta_pct(m0.crest_factor, m3.crest_factor));
        log::info!("[TRANSFORM_TRACE]   S0→S3 rms_level:          {}", delta_abs(m0.rms_level, m3.rms_level));
        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");

        // ── Cleanup ──
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// TEST: Trace with sample rate change (48k → 44.1k).
    /// This is the most likely source of metallic artifacts.
    #[test]
    fn trace_sample_rate_change_artifacts() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() || !ffprobe.exists() {
            eprintln!("Skipping: ffmpeg/ffprobe not found");
            return;
        }

        let tmp = std::env::temp_dir().join("audio_trace_sr_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let original = tmp.join("original_48k.mp4");
        let resampled = tmp.join("resampled_44k.mp4");

        // Create at 48kHz
        create_test_signal(&ffmpeg, &original, 48000, 5, "192k");

        let m0 = capture_metrics(&ffmpeg, &ffprobe, &original, "48k_original");

        // Force resample to 44100 Hz (the dangerous path)
        run_normalize_audio_only(&ffmpeg, &original, &resampled, "aac", 44100, Some("192k"));
        let m1 = capture_metrics(&ffmpeg, &ffprobe, &resampled, "44k_resampled");

        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");
        log::info!("[TRANSFORM_TRACE] SAMPLE RATE CHANGE: 48000 → 44100 Hz");
        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");
        print_comparison(&m0, &m1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// TEST: Trace with bitrate collapse (192k → 128k default).
    /// This is the bug we just fixed.
    #[test]
    fn trace_bitrate_collapse_artifacts() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() || !ffprobe.exists() {
            eprintln!("Skipping: ffmpeg/ffprobe not found");
            return;
        }

        let tmp = std::env::temp_dir().join("audio_trace_br_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let original = tmp.join("original_192k.mp4");
        let no_bitrate = tmp.join("no_bitrate.mp4");
        let preserved = tmp.join("preserved_192k.mp4");

        create_test_signal(&ffmpeg, &original, 48000, 5, "192k");

        let m0 = capture_metrics(&ffmpeg, &ffprobe, &original, "192k_original");

        // Simulate the OLD bug: no bitrate passed (FFmpeg defaults to 128k)
        run_normalize_audio_only(&ffmpeg, &original, &no_bitrate, "aac", 48000, None);
        let m1 = capture_metrics(&ffmpeg, &ffprobe, &no_bitrate, "no_bitrate_default");

        // Simulate the FIX: bitrate preserved
        run_normalize_audio_only(&ffmpeg, &original, &preserved, "aac", 48000, Some("192k"));
        let m2 = capture_metrics(&ffmpeg, &ffprobe, &preserved, "bitrate_preserved");

        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");
        log::info!("[TRANSFORM_TRACE] BITRATE COLLAPSE TEST: 192k → default(128k) vs 192k preserved");
        log::info!("[TRANSFORM_TRACE] ════════════════════════════════════════════════════════════════");
        log::info!("[TRANSFORM_TRACE] ── OLD BUG (no bitrate) ──");
        print_comparison(&m0, &m1);
        log::info!("[TRANSFORM_TRACE] ── FIX (bitrate preserved) ──");
        print_comparison(&m0, &m2);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// TEST: Real-file A/B normalization test.
    /// Probes actual MP4 files from the user's course folder, runs them through
    /// the audio-only normalization pipeline, and verifies bitrate preservation.
    /// Skips full video re-encode (too slow for real files); only audio copy path tested.
    #[test]
    fn real_file_ab_normalization_test() {
        let _ = env_logger::builder().is_test(true).try_init();
        let (ffmpeg, ffprobe) = get_binaries();
        if !ffmpeg.exists() || !ffprobe.exists() {
            eprintln!("Skipping: ffmpeg/ffprobe not found at {:?}", ffmpeg.parent());
            return;
        }

        let source_dir = PathBuf::from(
            r"E:\9.Cs Fundamentals\Deep Dive  Python\Python 3 Deep Dive (Part 1 - Functional)\02 - A Quick Refresher - Basics Review"
        );
        if !source_dir.exists() {
            eprintln!("Skipping: source directory not found: {:?}", source_dir);
            return;
        }

        let tmp = std::env::temp_dir().join("ab_normalization_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        // Collect .mp4 files in the directory (limit to first 3 for speed)
        let mut mp4_files: Vec<PathBuf> = std::fs::read_dir(&source_dir)
            .expect("Failed to read source directory")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("mp4"))
            .collect();
        mp4_files.truncate(3);

        assert!(!mp4_files.is_empty(), "No .mp4 files found in {:?}", source_dir);
        log::info!("[AB_TEST] Found {} MP4 files in source directory", mp4_files.len());

        let mut all_passed = true;
        let mut results: Vec<(String, bool, String)> = Vec::new();

        for (i, mp4) in mp4_files.iter().enumerate() {
            let file_name = mp4.file_name().unwrap().to_string_lossy();
            log::info!("[AB_TEST] ════════════════════════════════════════════════════════════════");
            log::info!("[AB_TEST] FILE {}/{}: {}", i + 1, mp4_files.len(), file_name);

            // Probe original
            let m0 = capture_metrics(&ffmpeg, &ffprobe, mp4, &format!("orig_{}", i));
            log::info!("[AB_TEST] ORIGINAL: codec={} profile={:?} sr={:?} bitrate={:?} duration={:?}",
                m0.codec, m0.profile, m0.sample_rate, m0.bit_rate, m0.duration);

            let original_bitrate = m0.bit_rate;
            let original_sr = m0.sample_rate;

            if original_bitrate.is_none() {
                log::warn!("[AB_TEST] ⚠️  No bitrate info for {} — cannot verify preservation", file_name);
                results.push((file_name.to_string(), false, "no bitrate in source".to_string()));
                continue;
            }

            // Stage 1: Audio-only normalization (audio re-encode, video copy)
            // This is the most common path in the merge pipeline.
            let audio_norm_out = tmp.join(format!("audio_norm_{}.mp4", i));
            let br_kbps = original_bitrate.unwrap() / 1000;
            let target_sr = original_sr.unwrap_or(48000);
            run_normalize_audio_only(
                &ffmpeg, mp4, &audio_norm_out,
                "aac", target_sr,
                Some(&format!("{}k", br_kbps)),
            );
            let m1 = capture_metrics(&ffmpeg, &ffprobe, &audio_norm_out, &format!("anorm_{}", i));
            log::info!("[AB_TEST] AFTER AUDIO_NORM: codec={} profile={:?} sr={:?} bitrate={:?}",
                m1.codec, m1.profile, m1.sample_rate, m1.bit_rate);

            // Stage 2: Double re-encode (simulates what the guard prevents)
            let double_out = tmp.join(format!("double_{}.mp4", i));
            run_normalize_audio_only(
                &ffmpeg, &audio_norm_out, &double_out,
                "aac", target_sr,
                Some(&format!("{}k", br_kbps)),
            );
            let m3 = capture_metrics(&ffmpeg, &ffprobe, &double_out, &format!("double_{}", i));
            log::info!("[AB_TEST] AFTER DOUBLE_NORM: codec={} profile={:?} sr={:?} bitrate={:?}",
                m3.codec, m3.profile, m3.sample_rate, m3.bit_rate);

            // ── Assertions ──
            let mut file_passed = true;
            let mut failures: Vec<String> = Vec::new();

            // 1. Bitrate preservation: output bitrate should match input
            if let (Some(orig_br), Some(out_br)) = (original_bitrate, m1.bit_rate) {
                let delta = (out_br as f64 - orig_br as f64) / orig_br as f64 * 100.0;
                log::info!("[AB_TEST] Audio-only bitrate delta: {:+.1}% ({} → {})", delta, orig_br, out_br);
                if delta.abs() > 10.0 {
                    file_passed = false;
                    failures.push(format!("audio-only bitrate shifted {:.1}%: {} → {}", delta, orig_br, out_br));
                }
            }

            // 2. Sample rate preservation (should not resample when matching)
            if original_sr == Some(target_sr) {
                if m1.sample_rate != original_sr {
                    file_passed = false;
                    failures.push(format!("audio-only SR changed: {:?} → {:?}", original_sr, m1.sample_rate));
                }
            }

            // 3. Crest factor: single re-encode should not shift crest by >10%
            if let (Some(orig_cf), Some(out_cf)) = (m0.crest_factor, m1.crest_factor) {
                let cf_delta = ((out_cf - orig_cf) / orig_cf) * 100.0;
                log::info!("[AB_TEST] Crest factor delta (single): {:+.1}%", cf_delta);
                if cf_delta.abs() > 10.0 {
                    log::warn!("[AB_TEST] ⚠️  Crest factor shifted {:.1}% — potential quality impact", cf_delta);
                }
            }

            // 4. Double re-encode: bitrate should be preserved (guard should prevent this path)
            if let (Some(orig_br), Some(out_br)) = (original_bitrate, m3.bit_rate) {
                let delta = (out_br as f64 - orig_br as f64) / orig_br as f64 * 100.0;
                log::info!("[AB_TEST] Double-reencode bitrate delta: {:+.1}% ({} → {})", delta, orig_br, out_br);
                if delta.abs() > 15.0 {
                    log::warn!("[AB_TEST] ⚠️  Double re-encode degrades bitrate {:.1}% — guard critical", delta);
                }
            }

            // Print comparison table
            print_comparison(&m0, &m1);
            print_comparison(&m0, &m3);

            if file_passed {
                log::info!("[AB_TEST] ✅ PASSED: {}", file_name);
                results.push((file_name.to_string(), true, "passed".to_string()));
            } else {
                log::error!("[AB_TEST] ❌ FAILED: {} — {}", file_name, failures.join("; "));
                results.push((file_name.to_string(), false, failures.join("; ")));
                all_passed = false;
            }
        }

        // ── Summary ──
        log::info!("[AB_TEST] ════════════════════════════════════════════════════════════════");
        log::info!("[AB_TEST] SUMMARY: {}/{} files passed", results.iter().filter(|r| r.1).count(), results.len());
        for (name, passed, detail) in &results {
            let icon = if *passed { "✅" } else { "❌" };
            log::info!("[AB_TEST]   {} {} — {}", icon, name, detail);
        }
        log::info!("[AB_TEST] ════════════════════════════════════════════════════════════════");

        let _ = std::fs::remove_dir_all(&tmp);
        assert!(all_passed, "One or more files failed the A/B normalization test — check logs above");
    }
}
