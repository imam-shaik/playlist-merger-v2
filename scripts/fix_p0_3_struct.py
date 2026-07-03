#!/usr/bin/env python
"""Update StreamIdentity struct with new fields for P0-3"""
import os

STRUCTS_RS = os.path.join(os.path.dirname(__file__), '..', 'src-tauri', 'src', 'ffmpeg', 'media_validation_engine', 'types', 'structs.rs')

with open(STRUCTS_RS, 'r', encoding='utf-8') as f:
    content = f.read()

OLD_STRUCT = """#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamIdentity {
    pub video_count: usize,
    pub audio_count: usize,
    pub subtitle_count: usize,
    pub total_streams: usize,
    pub video_codecs: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub duration: Option<f64>,
}"""

NEW_STRUCT = """#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamIdentity {
    pub video_count: usize,
    pub audio_count: usize,
    pub subtitle_count: usize,
    pub total_streams: usize,
    pub video_codecs: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub duration: Option<f64>,
    // P0-3: Rich metadata for post-repair identity verification
    #[serde(default)]
    pub video_languages: Vec<Option<String>>,
    #[serde(default)]
    pub audio_languages: Vec<Option<String>>,
    #[serde(default)]
    pub video_color_transfer: Vec<Option<String>>,
    #[serde(default)]
    pub video_color_space: Vec<Option<String>>,
    #[serde(default)]
    pub video_rotation: Vec<Option<i32>>,
}

impl StreamIdentity {
    pub fn empty() -> Self {
        Self {
            video_count: 0,
            audio_count: 0,
            subtitle_count: 0,
            total_streams: 0,
            video_codecs: Vec::new(),
            audio_codecs: Vec::new(),
            duration: None,
            video_languages: Vec::new(),
            audio_languages: Vec::new(),
            video_color_transfer: Vec::new(),
            video_color_space: Vec::new(),
            video_rotation: Vec::new(),
        }
    }
}"""

if OLD_STRUCT in content:
    content = content.replace(OLD_STRUCT, NEW_STRUCT, 1)
    print("Updated StreamIdentity struct with new fields + empty() constructor")
else:
    print("Pattern not found")

with open(STRUCTS_RS, 'w', encoding='utf-8') as f:
    f.write(content)

print("Done writing structs.rs")
