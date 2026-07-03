// Reporters evidence module
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::path::Path;

pub fn compute_file_checksum(path: &Path) -> String {
    use std::fs::File;
    use std::io::Read;

    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };

    let mut buffer = Vec::new();
    let _ = file.read_to_end(&mut buffer);

    let mut hasher = Sha256::new();
    hasher.update(&buffer);
    let result = hasher.finalize();

    hex::encode(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub path: String,
    pub checksum: String,
    pub size: u64,
}

impl Evidence {
    pub fn new(path: &Path) -> Self {
        let checksum = compute_file_checksum(path);
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        Self {
            path: path.to_string_lossy().to_string(),
            checksum,
            size,
        }
    }
}