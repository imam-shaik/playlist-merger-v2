pub mod requirements;

use anyhow::Result;
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MediaAsset {
    pub name: String,
    pub path: Option<PathBuf>,
    pub required: bool,
    pub description: String,
    pub codec: Option<String>,
    pub container: Option<String>,
}

impl MediaAsset {
    pub fn available(&self) -> bool {
        self.path.as_ref().map_or(false, |p| p.exists())
    }

    pub fn missing_reason(&self) -> Option<String> {
        if self.available() {
            None
        } else if self.required {
            Some(format!("REQUIRED: {}", self.description))
        } else {
            Some(format!("OPTIONAL: {}", self.description))
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaAssets {
    pub assets: HashMap<String, MediaAsset>,
    pub total_count: usize,
    pub available_count: usize,
}

impl MediaAssets {
    pub fn discover(path: &Path) -> Result<Self> {
        let mut assets = HashMap::new();

        // H.264 MP4 files
        for i in 1..=5 {
            let name = format!("h264_720p_{}", i);
            let path = path.join(format!("{}.mp4", name));
            assets.insert(
                name.clone(),
                MediaAsset {
                    name,
                    path: if path.exists() { Some(path) } else { None },
                    required: i <= 2,
                    description: "H.264 720p MP4 test video".to_string(),
                    codec: Some("h264".to_string()),
                    container: Some("mp4".to_string()),
                },
            );
        }

        // H.265 MP4 files
        for i in 1..=3 {
            let name = format!("h265_1080p_{}", i);
            let path = path.join(format!("{}.mp4", name));
            assets.insert(
                name.clone(),
                MediaAsset {
                    name,
                    path: if path.exists() { Some(path) } else { None },
                    required: i == 1,
                    description: "H.265 1080p MP4 test video".to_string(),
                    codec: Some("hevc".to_string()),
                    container: Some("mp4".to_string()),
                },
            );
        }

        // VP9 WebM files
        for i in 1..=2 {
            let name = format!("vp9_720p_{}", i);
            let path = path.join(format!("{}.webm", name));
            assets.insert(
                name.clone(),
                MediaAsset {
                    name,
                    path: if path.exists() { Some(path) } else { None },
                    required: false,
                    description: "VP9 720p WebM test video".to_string(),
                    codec: Some("vp9".to_string()),
                    container: Some("webm".to_string()),
                },
            );
        }

        // MKV files
        for i in 1..=3 {
            let name = format!("mkv_multi_audio_{}", i);
            let path = path.join(format!("{}.mkv", name));
            assets.insert(
                name.clone(),
                MediaAsset {
                    name,
                    path: if path.exists() { Some(path) } else { None },
                    required: i == 1,
                    description: "MKV with multiple audio streams".to_string(),
                    codec: None,
                    container: Some("mkv".to_string()),
                },
            );
        }

        // Audio test files
        for (suffix, desc, required) in [
            ("aac_stereo_44100", "AAC Stereo 44.1kHz", true),
            ("aac_stereo_48000", "AAC Stereo 48kHz", false),
            ("aac_51_48000", "AAC 5.1 48kHz", true),
            ("aac_71_48000", "AAC 7.1 48kHz", false),
            ("aac_commentary", "AAC Commentary track", false),
            ("mp3_stereo", "MP3 Stereo", false),
        ] {
            let name = suffix.to_string();
            let path = path.join(format!("{}.mp4", name));
            assets.insert(
                name.clone(),
                MediaAsset {
                    name,
                    path: if path.exists() { Some(path) } else { None },
                    required,
                    description: desc.to_string(),
                    codec: Some(suffix.split('_').next().unwrap_or("unknown").to_string()),
                    container: Some("mp4".to_string()),
                },
            );
        }

        // Subtitle test files
        for (suffix, ext, desc, required) in [
            ("srt_embedded", "mp4", "Embedded SRT subtitles", true),
            ("ass_embedded", "mp4", "Embedded ASS subtitles", false),
            ("pgs_embedded", "mkv", "PGS subtitles (bitmap)", true),
            ("vobsub_embedded", "mkv", "VobSub subtitles (bitmap)", false),
            ("external_srt", "srt", "External SRT file", true),
            ("external_ass", "ass", "External ASS file", false),
        ] {
            let name = suffix.to_string();
            let path = path.join(format!("{}.{}", name, ext));
            assets.insert(
                name.clone(),
                MediaAsset {
                    name,
                    path: if path.exists() { Some(path) } else { None },
                    required,
                    description: desc.to_string(),
                    codec: None,
                    container: Some(ext.to_string()),
                },
            );
        }

        // Large files for stress testing
        for i in 1..=10 {
            let name = format!("large_1gb_{}", i);
            let path = path.join(format!("{}.mp4", name));
            assets.insert(
                name.clone(),
                MediaAsset {
                    name,
                    path: if path.exists() { Some(path) } else { None },
                    required: false,
                    description: "1GB test file for stress testing".to_string(),
                    codec: Some("h264".to_string()),
                    container: Some("mp4".to_string()),
                },
            );
        }

        let available_count = assets.values().filter(|a| a.available()).count();
        let total_count = assets.len();

        Ok(Self {
            assets,
            total_count,
            available_count,
        })
    }

    pub fn get(&self, name: &str) -> Option<&MediaAsset> {
        self.assets.get(name)
    }

    pub fn get_path(&self, name: &str) -> Option<PathBuf> {
        self.assets.get(name).and_then(|a| a.path.clone())
    }

    pub fn has(&self, name: &str) -> bool {
        self.get(name).map_or(false, |a| a.available())
    }

    pub fn missing_count(&self) -> usize {
        self.assets.values().filter(|a| !a.available()).count()
    }

    pub fn required_missing(&self) -> Vec<&MediaAsset> {
        self.assets
            .values()
            .filter(|a| a.required && !a.available())
            .collect()
    }

    pub fn can_run_phase_a(&self) -> bool {
        self.has("h264_720p_1") && self.has("h264_720p_2") && self.has("h264_720p_3")
    }

    pub fn can_run_phase_b(&self) -> bool {
        self.has("h264_720p_1") && self.has("h264_720p_2")
    }

    pub fn can_run_phase_c(&self) -> bool {
        self.has("h264_720p_1") && self.has("h264_720p_2") && self.has("h264_720p_3")
    }

    pub fn can_run_phase_f(&self) -> bool {
        self.has("aac_stereo_44100") && self.has("aac_51_48000")
    }

    pub fn can_run_phase_g(&self) -> bool {
        self.has("srt_embedded") && self.has("pgs_embedded")
    }

    pub fn can_run_phase_h(&self) -> usize {
        // Count how many large files we have for stress testing
        (1..=10)
            .filter(|i| self.has(&format!("large_1gb_{}", i)))
            .count()
    }

    pub fn print_summary(&self) {
        println!("\n📁 Media Asset Summary");
        println!("─────────────────────────────");
        println!(
            "  Available: {}/{} assets",
            self.available_count, self.total_count
        );

        let required_missing = self.required_missing();
        if !required_missing.is_empty() {
            println!("\n⚠️  REQUIRED assets missing:");
            for asset in &required_missing {
                println!("    - {}", asset.missing_reason().unwrap());
            }
        }

        // Group by category
        println!("\n📋 Asset Categories:");
        let categories = [
            ("Video (H.264)", vec!["h264_720p_1", "h264_720p_2", "h264_720p_3", "h264_720p_4", "h264_720p_5"]),
            ("Video (H.265)", vec!["h265_1080p_1", "h265_1080p_2", "h265_1080p_3"]),
            ("Video (VP9)", vec!["vp9_720p_1", "vp9_720p_2"]),
            ("MKV Multi-Audio", vec!["mkv_multi_audio_1", "mkv_multi_audio_2", "mkv_multi_audio_3"]),
            ("Audio", vec!["aac_stereo_44100", "aac_stereo_48000", "aac_51_48000", "aac_71_48000", "aac_commentary"]),
            ("Subtitles", vec!["srt_embedded", "ass_embedded", "pgs_embedded", "vobsub_embedded", "external_srt"]),
        ];

        for (category, names) in categories {
            let available: Vec<_> = names
                .iter()
                .filter(|n| self.has(n))
                .collect();
            if !available.is_empty() || names.iter().any(|n| self.assets.get(*n).map_or(false, |a| a.required)) {
                println!("  {}: {}/{}", category, available.len(), names.len());
            }
        }

        let stress_capable = self.can_run_phase_h();
        if stress_capable >= 5 {
            println!("  Stress test: ✅ {} files (can run 500+ test)", stress_capable);
        } else if stress_capable > 0 {
            println!(
                "  Stress test: ⚠️  {} files (limited to {} file test)",
                stress_capable,
                stress_capable * 100
            );
        } else {
            println!("  Stress test: ❌ No large files available");
        }
    }
}

impl fmt::Display for MediaAssets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Media Assets ({}/{} available)", self.available_count, self.total_count)?;
        for asset in self.assets.values() {
            let status = if asset.available() { "✅" } else if asset.required { "❌" } else { "○" };
            writeln!(f, "  {} {} - {}", status, asset.name, asset.description)?;
        }
        Ok(())
    }
}