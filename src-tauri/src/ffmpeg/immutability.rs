use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use serde::{Serialize, Deserialize};

/// Audio properties captured at normalization time.
/// Used to verify that a previously-normalized file is compatible with the
/// current merge target without re-encoding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioFingerprint {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate: Option<u64>,
    pub profile: Option<String>,
}

/// A single entry in the immutability registry.
/// Records that a file was normalized and must never be re-encoded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmutabilityEntry {
    /// Original source file path (before normalization)
    pub source_path: String,
    /// Path to the normalized file (the one to use in concat)
    pub normalized_path: String,
    /// Audio properties of the normalized file
    pub audio_fingerprint: AudioFingerprint,
    /// Job ID that performed the normalization
    pub job_id: String,
    /// Unix timestamp when normalization was performed
    pub timestamp: i64,
}

/// Persistent registry tracking which files have been normalized.
///
/// Once a file is registered as "audio immutable", it must NEVER be re-encoded.
/// The concat step uses `-c:a copy` for all files when any file is registered.
///
/// # Persistence
/// The registry is saved to disk alongside the recovery checkpoint, so it
/// survives application restarts and can be used for cross-job deduplication.
///
/// # Thread Safety
/// All operations are guarded by a Mutex. Use `Arc<ImmutabilityRegistry>`
/// for sharing across threads.
#[derive(Debug)]
pub struct ImmutabilityRegistry {
    entries: Mutex<HashMap<String, ImmutabilityEntry>>,
}

impl ImmutabilityRegistry {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Register a file as audio-immutable after normalization.
    ///
    /// # Arguments
    /// * `source_path` - Original file path (before normalization)
    /// * `normalized_path` - Path to the normalized output file
    /// * `fingerprint` - Audio properties of the normalized file
    /// * `job_id` - Job that performed the normalization
    pub fn register(
        &self,
        source_path: &str,
        normalized_path: &str,
        fingerprint: AudioFingerprint,
        job_id: &str,
    ) {
        let entry = ImmutabilityEntry {
            source_path: source_path.to_string(),
            normalized_path: normalized_path.to_string(),
            audio_fingerprint: fingerprint,
            job_id: job_id.to_string(),
            timestamp: chrono::Local::now().timestamp(),
        };

        if let Ok(mut entries) = self.entries.lock() {
            log::info!(
                "[IMMUTABILITY] Registered: {} → {} (codec={}, sr={}, ch={}, br={:?})",
                source_path,
                normalized_path,
                entry.audio_fingerprint.codec,
                entry.audio_fingerprint.sample_rate,
                entry.audio_fingerprint.channels,
                entry.audio_fingerprint.bitrate,
            );
            entries.insert(source_path.to_string(), entry);
        }
    }

    /// Look up whether a file has been normalized.
    ///
    /// Returns the `ImmutabilityEntry` if the file is registered AND the
    /// normalized file still exists on disk.
    pub fn lookup(&self, source_path: &str) -> Option<ImmutabilityEntry> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(source_path)?;

        // Validate the normalized file still exists
        if Path::new(&entry.normalized_path).exists() {
            Some(entry.clone())
        } else {
            log::warn!(
                "[IMMUTABILITY] Entry found for {} but normalized file missing: {}",
                source_path,
                entry.normalized_path
            );
            None
        }
    }

    /// Check if a file is registered as audio-immutable.
    /// This is a fast check that doesn't validate file existence.
    pub fn is_immutable(&self, source_path: &str) -> bool {
        self.entries.lock().map(|e| e.contains_key(source_path)).unwrap_or(false)
    }

    /// Get the normalized path for a file, if registered.
    pub fn get_normalized_path(&self, source_path: &str) -> Option<String> {
        self.lookup(source_path).map(|e| e.normalized_path)
    }

    /// Get the audio fingerprint for a file, if registered.
    pub fn get_fingerprint(&self, source_path: &str) -> Option<AudioFingerprint> {
        self.lookup(source_path).map(|e| e.audio_fingerprint)
    }

    /// Validate that a registered file's audio properties match the target profile.
    ///
    /// Returns `true` if the file can be safely stream-copied into the target.
    pub fn is_compatible_with(
        &self,
        source_path: &str,
        target_codec: &str,
        target_sample_rate: u32,
        target_channels: u32,
    ) -> bool {
        let entry = match self.lookup(source_path) {
            Some(e) => e,
            None => return false,
        };

        let fp = &entry.audio_fingerprint;
        let compatible = fp.codec == target_codec
            && fp.sample_rate == target_sample_rate
            && fp.channels == target_channels;

        if !compatible {
            log::info!(
                "[IMMUTABILITY] Incompatible: {} has ({}, {}Hz, {}ch) but target needs ({}, {}Hz, {}ch)",
                source_path,
                fp.codec, fp.sample_rate, fp.channels,
                target_codec, target_sample_rate, target_channels,
            );
        }

        compatible
    }

    /// Get the number of registered entries.
    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all registered source paths.
    pub fn source_paths(&self) -> Vec<String> {
        self.entries.lock().map(|e| e.keys().cloned().collect()).unwrap_or_default()
    }

    /// Get all registered entries.
    pub fn all_entries(&self) -> Vec<ImmutabilityEntry> {
        self.entries.lock().map(|e| e.values().cloned().collect()).unwrap_or_default()
    }

    // ── Persistence ──────────────────────────────────────────────────────────

    /// Save the registry to a JSON file.
    ///
    /// This is typically called alongside the recovery checkpoint save.
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let entries = self.entries.lock().map_err(|e| e.to_string())?;
        let serializable: Vec<&ImmutabilityEntry> = entries.values().collect();
        let json = serde_json::to_string_pretty(&serializable)
            .map_err(|e| format!("Serialization failed: {}", e))?;
        std::fs::write(path, json)
            .map_err(|e| format!("Failed to write registry to {}: {}", path.display(), e))?;
        log::info!(
            "[IMMUTABILITY] Registry saved: {} entries → {}",
            entries.len(),
            path.display()
        );
        Ok(())
    }

    /// Load the registry from a JSON file.
    ///
    /// Entries whose normalized files no longer exist on disk are skipped.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let entries: Vec<ImmutabilityEntry> = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse registry: {}", e))?;

        let registry = Self::new();
        let mut loaded = 0;
        let mut skipped = 0;

        for entry in entries {
            // Only load entries whose normalized file still exists
            if Path::new(&entry.normalized_path).exists() {
                if let Ok(mut map) = registry.entries.lock() {
                    map.insert(entry.source_path.clone(), entry);
                    loaded += 1;
                }
            } else {
                skipped += 1;
            }
        }

        log::info!(
            "[IMMUTABILITY] Registry loaded from {}: {} entries ({} skipped, file missing)",
            path.display(),
            loaded,
            skipped
        );

        Ok(registry)
    }

    /// Get the standard registry path alongside a recovery checkpoint.
    pub fn registry_path_for_job(output_dir: &Path, job_id: &str) -> PathBuf {
        output_dir.join(format!("immutability_{}.json", job_id))
    }
}

impl Default for ImmutabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_lookup() {
        let registry = ImmutabilityRegistry::new();
        let fp = AudioFingerprint {
            codec: "aac".into(),
            sample_rate: 48000,
            channels: 2,
            bitrate: Some(192000),
            profile: Some("LC".into()),
        };

        // Create a temp file to simulate the normalized file
        let tmp = std::env::temp_dir().join("immutability_test_norm.mp4");
        std::fs::write(&tmp, b"fake").unwrap();

        registry.register(
            "/tmp/original.mp4",
            tmp.to_str().unwrap(),
            fp.clone(),
            "job-123",
        );

        // Lookup should succeed
        let entry = registry.lookup("/tmp/original.mp4");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.normalized_path, tmp.to_str().unwrap());
        assert_eq!(entry.audio_fingerprint.codec, "aac");
        assert_eq!(entry.audio_fingerprint.sample_rate, 48000);
        assert_eq!(entry.job_id, "job-123");

        // Cleanup
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_lookup_missing_file() {
        let registry = ImmutabilityRegistry::new();
        let fp = AudioFingerprint {
            codec: "aac".into(),
            sample_rate: 48000,
            channels: 2,
            bitrate: Some(192000),
            profile: None,
        };

        // Register with a non-existent path
        registry.register(
            "/tmp/original.mp4",
            "/tmp/nonexistent_norm.mp4",
            fp,
            "job-123",
        );

        // Lookup should fail because normalized file doesn't exist
        assert!(registry.lookup("/tmp/original.mp4").is_none());
    }

    #[test]
    fn test_is_immutable() {
        let registry = ImmutabilityRegistry::new();
        let fp = AudioFingerprint {
            codec: "aac".into(),
            sample_rate: 48000,
            channels: 2,
            bitrate: None,
            profile: None,
        };

        let tmp = std::env::temp_dir().join("immutability_test_imm.mp4");
        std::fs::write(&tmp, b"fake").unwrap();

        assert!(!registry.is_immutable("/tmp/file.mp4"));
        registry.register("/tmp/file.mp4", tmp.to_str().unwrap(), fp, "j");
        assert!(registry.is_immutable("/tmp/file.mp4"));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_compatibility_check() {
        let registry = ImmutabilityRegistry::new();
        let fp = AudioFingerprint {
            codec: "aac".into(),
            sample_rate: 48000,
            channels: 2,
            bitrate: Some(192000),
            profile: None,
        };

        let tmp = std::env::temp_dir().join("immutability_test_compat.mp4");
        std::fs::write(&tmp, b"fake").unwrap();

        registry.register("/tmp/file.mp4", tmp.to_str().unwrap(), fp, "j");

        // Compatible
        assert!(registry.is_compatible_with("/tmp/file.mp4", "aac", 48000, 2));

        // Incompatible codec
        assert!(!registry.is_compatible_with("/tmp/file.mp4", "mp3", 48000, 2));

        // Incompatible sample rate
        assert!(!registry.is_compatible_with("/tmp/file.mp4", "aac", 44100, 2));

        // Incompatible channels
        assert!(!registry.is_compatible_with("/tmp/file.mp4", "aac", 48000, 6));

        // Unknown file
        assert!(!registry.is_compatible_with("/tmp/unknown.mp4", "aac", 48000, 2));

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_persistence_roundtrip() {
        let registry = ImmutabilityRegistry::new();
        let fp = AudioFingerprint {
            codec: "aac".into(),
            sample_rate: 48000,
            channels: 2,
            bitrate: Some(192000),
            profile: Some("LC".into()),
        };

        let tmp = std::env::temp_dir().join("immutability_test_persist.mp4");
        std::fs::write(&tmp, b"fake").unwrap();

        registry.register(
            "/tmp/original.mp4",
            tmp.to_str().unwrap(),
            fp.clone(),
            "job-456",
        );

        // Save
        let save_path = std::env::temp_dir().join("immutability_test_save.json");
        registry.save_to_file(&save_path).unwrap();

        // Load
        let loaded = ImmutabilityRegistry::load_from_file(&save_path).unwrap();
        assert_eq!(loaded.len(), 1);

        let entry = loaded.lookup("/tmp/original.mp4").unwrap();
        assert_eq!(entry.audio_fingerprint, fp);
        assert_eq!(entry.job_id, "job-456");

        // Cleanup
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&save_path);
    }

    #[test]
    fn test_empty_registry() {
        let registry = ImmutabilityRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(registry.source_paths().is_empty());
        assert!(registry.all_entries().is_empty());
    }

    #[test]
    fn test_overwrite_existing_entry() {
        let registry = ImmutabilityRegistry::new();
        let fp1 = AudioFingerprint {
            codec: "aac".into(),
            sample_rate: 44100,
            channels: 2,
            bitrate: Some(128000),
            profile: None,
        };
        let fp2 = AudioFingerprint {
            codec: "aac".into(),
            sample_rate: 48000,
            channels: 2,
            bitrate: Some(192000),
            profile: Some("LC".into()),
        };

        let tmp1 = std::env::temp_dir().join("immutability_test_overwrite1.mp4");
        let tmp2 = std::env::temp_dir().join("immutability_test_overwrite2.mp4");
        std::fs::write(&tmp1, b"fake1").unwrap();
        std::fs::write(&tmp2, b"fake2").unwrap();

        registry.register("/tmp/file.mp4", tmp1.to_str().unwrap(), fp1, "j1");
        registry.register("/tmp/file.mp4", tmp2.to_str().unwrap(), fp2, "j2");

        assert_eq!(registry.len(), 1);
        let entry = registry.lookup("/tmp/file.mp4").unwrap();
        assert_eq!(entry.normalized_path, tmp2.to_str().unwrap());
        assert_eq!(entry.audio_fingerprint.sample_rate, 48000);

        let _ = std::fs::remove_file(&tmp1);
        let _ = std::fs::remove_file(&tmp2);
    }
}
