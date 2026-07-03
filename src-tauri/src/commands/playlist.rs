use std::cmp::Reverse;
use tauri::command;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use chrono::{DateTime, Utc};

fn get_playlists_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("PlaylistMerger")
        .join("playlists")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistMeta {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub file_count: usize,
    pub total_duration: f64,
}

#[command]
pub async fn save_playlist(
    id: String,
    _name: String,
    data: serde_json::Value,
) -> Result<(), String> {
    let dir = get_playlists_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create playlists dir: {}", e))?;

    let file_path = dir.join(format!("{}.json", id));
    let json = serde_json::to_string_pretty(&data)
        .map_err(|e| format!("Serialization error: {}", e))?;

    std::fs::write(&file_path, json)
        .map_err(|e| format!("Failed to write playlist '{}': {}", file_path.display(), e))
}

#[command]
pub async fn load_playlist(id: String) -> Result<serde_json::Value, String> {
    let file_path = get_playlists_dir().join(format!("{}.json", id));

    if !file_path.exists() {
        return Err(format!("Playlist '{}' not found", id));
    }

    let json = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read playlist: {}", e))?;

    serde_json::from_str(&json)
        .map_err(|e| format!("Invalid playlist JSON: {}", e))
}

#[command]
pub async fn list_saved_playlists() -> Result<Vec<PlaylistMeta>, String> {
    let dir = get_playlists_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut playlists = Vec::new();

    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        if let Ok(json) = std::fs::read_to_string(&path) {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&json) {
                let meta = PlaylistMeta {
                    id: data["id"].as_str().unwrap_or("").to_string(),
                    name: data["name"].as_str().unwrap_or("Untitled").to_string(),
                    created_at: data["createdAt"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(Utc::now),
                    updated_at: data["updatedAt"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_else(Utc::now),
                    file_count: data["entries"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0),
                    total_duration: data["totalDuration"].as_f64().unwrap_or(0.0),
                };
                if !meta.id.is_empty() {
                    playlists.push(meta);
                }
            }
        }
    }

    playlists.sort_by_key(|a| Reverse(a.updated_at));
    Ok(playlists)
}

#[command]
pub async fn delete_playlist(id: String) -> Result<(), String> {
    let file_path = get_playlists_dir().join(format!("{}.json", id));
    if file_path.exists() {
        std::fs::remove_file(&file_path)
            .map_err(|e| format!("Failed to delete playlist: {}", e))?;
    }
    Ok(())
}
