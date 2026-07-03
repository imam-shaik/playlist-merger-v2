//! Runtime Certification Binary for Subtitle Timeline Engine
//!
//! This binary runs runtime certification tests against the subtitle timeline engine
//! using generated test media to verify real-world behavior.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --bin subtitle-runtime-cert
//! ```

use std::process::Command;

use playlist_merger_lib::certification::{
    CertificationConfig, run_certification_suite,
};

fn main() {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║       Subtitle Timeline Runtime Certification Suite                ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    let config = CertificationConfig {
        test_output_dir: std::env::temp_dir().join("subtitle_certification"),
        ffmpeg_path: find_ffmpeg(),
        mkvmerge_path: find_mkvmerge(),
        verbose: true,
    };

    println!("FFmpeg path: {}", config.ffmpeg_path);
    println!("MKVMerge path: {:?}\n", config.mkvmerge_path);

    let results = run_certification_suite(&config);

    println!("\n╔══════════════════════════════════════════════════════════════════╗");
    println!("║                      CERTIFICATION SUMMARY                          ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    for result in &results {
        let status = if result.passed { "✅ PASS" } else { "❌ FAIL" };
        println!("  {} - {} ({:.2}ms)", status, result.test_name, result.duration_ms as f64);
        for error in &result.errors {
            println!("       ERROR: {}", error);
        }
        for warning in &result.warnings {
            println!("       WARNING: {}", warning);
        }
    }

    println!("\n─────────────────────────────────────────────────────────────────");
    println!("  Total:  {} tests", results.len());
    println!("  Passed: {} tests", passed);
    println!("  Failed: {} tests", failed);
    println!("─────────────────────────────────────────────────────────────────");

    if failed == 0 {
        println!("\n🎉 All runtime certification tests PASSED!");
        std::process::exit(0);
    } else {
        println!("\n⚠️  {} certification test(s) FAILED!", failed);
        std::process::exit(1);
    }
}

fn find_ffmpeg() -> String {
    // Try common FFmpeg locations
    let candidates = vec![
        "ffmpeg".to_string(),
        "ffmpeg.exe".to_string(),
        "C:\\ffmpeg\\bin\\ffmpeg.exe".to_string(),
        "C:\\Program Files\\ffmpeg\\bin\\ffmpeg.exe".to_string(),
    ];

    for candidate in &candidates {
        if let Ok(output) = Command::new(candidate).arg("-version").output() {
            if output.status.success() {
                return candidate.clone();
            }
        }
    }

    candidates[0].clone()
}

fn find_mkvmerge() -> Option<String> {
    let candidates = vec![
        "mkvmerge".to_string(),
        "mkvmerge.exe".to_string(),
        "C:\\mkvtool\\mkvmerge.exe".to_string(),
        "C:\\Program Files\\MKVToolNix\\mkvmerge.exe".to_string(),
    ];

    for candidate in &candidates {
        if let Ok(output) = Command::new(candidate).arg("--version").output() {
            if output.status.success() {
                return Some(candidate.clone());
            }
        }
    }

    None
}