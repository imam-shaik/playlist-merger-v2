use tauri::command;
use crate::types::AppSettings;
use crate::services::settings::{load_settings_internal, save_settings_internal};

#[command]
pub async fn get_settings() -> Result<AppSettings, String> {
    tokio::task::spawn_blocking(load_settings_internal)
        .await
        .map_err(|e| format!("Task panicked: {}", e))
}

#[command]
pub async fn save_settings(settings: AppSettings) -> Result<(), String> {
    tokio::task::spawn_blocking(move || save_settings_internal(&settings))
        .await
        .map_err(|e| format!("Task panicked: {}", e))?
}

#[command]
pub async fn get_ffmpeg_path() -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(|| {
        let settings = load_settings_internal();
        let ffmpeg = crate::ffmpeg::find_ffmpeg(settings.ffmpeg_path.as_deref());
        let ffprobe = crate::ffmpeg::find_ffprobe(settings.ffprobe_path.as_deref());

        serde_json::json!({
            "ffmpeg": ffmpeg.as_ref().ok().map(|p| p.to_string_lossy().into_owned()),
            "ffprobe": ffprobe.as_ref().ok().map(|p| p.to_string_lossy().into_owned()),
            "ffmpegFound": ffmpeg.is_ok(),
            "ffprobeFound": ffprobe.is_ok(),
            "ffmpegError": ffmpeg.err().map(|e| e.to_string()),
            "ffprobeError": ffprobe.err().map(|e| e.to_string()),
        })
    })
    .await
    .map_err(|e| format!("Task panicked: {}", e))
}
