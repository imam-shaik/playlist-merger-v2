// Media requirements module - defines what media assets are needed

pub struct MediaRequirement {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub min_count: usize,
}

pub fn get_minimum_requirements() -> Vec<MediaRequirement> {
    vec![
        MediaRequirement {
            name: "h264_720p".to_string(),
            description: "H.264 720p MP4 test video".to_string(),
            required: true,
            min_count: 3,
        },
        MediaRequirement {
            name: "aac_stereo".to_string(),
            description: "AAC Stereo audio".to_string(),
            required: true,
            min_count: 1,
        },
        MediaRequirement {
            name: "aac_51".to_string(),
            description: "AAC 5.1 surround audio".to_string(),
            required: true,
            min_count: 1,
        },
        MediaRequirement {
            name: "srt_subtitle".to_string(),
            description: "SRT subtitle file".to_string(),
            required: true,
            min_count: 1,
        },
    ]
}

pub fn get_stress_test_requirements() -> Vec<MediaRequirement> {
    vec![MediaRequirement {
        name: "large_video".to_string(),
        description: "Large video file (~1GB) for stress testing".to_string(),
        required: false,
        min_count: 5,
    }]
}