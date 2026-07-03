use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateContext {
    pub filename: String,
    pub extension: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_num: Option<String>,
    pub index: usize,
    pub start_time: f64,
    pub end_time: f64,
    pub duration: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_duration: Option<f64>,
    pub date: String,
    pub time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_label: Option<String>,
}

impl TemplateContext {
    pub fn format_time(seconds: f64) -> String {
        let total_secs = seconds.max(0.0);
        let h = (total_secs / 3600.0).floor() as u32;
        let m = ((total_secs % 3600.0) / 60.0).floor() as u32;
        let s = (total_secs % 60.0).floor() as u32;
        format!("{:02}-{:02}-{:02}", h, m, s)
    }

    pub fn format_total_duration(seconds: f64) -> String {
        if seconds >= 3600.0 {
            let h = (seconds / 3600.0).floor() as u32;
            let m = ((seconds % 3600.0) / 60.0).floor() as u32;
            format!("{:02}h{:02}m", h, m)
        } else if seconds >= 60.0 {
            let m = (seconds / 60.0).floor() as u32;
            let s = (seconds % 60.0).floor() as u32;
            format!("{:02}m{:02}s", m, s)
        } else {
            format!("{:02}s", seconds.floor() as u32)
        }
    }

}