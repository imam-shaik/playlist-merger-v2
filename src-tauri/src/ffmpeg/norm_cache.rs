use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// A signature uniquely identifying a normalization operation.
/// Two operations with the same source file and same signature will produce
/// identical normalized output — the second can reuse the first's result.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum NormSignature {
    /// Full video+audio re-encode (normalize_to_profile)
    Profile {
        vcodec: String,
        acodec: String,
        sample_rate: u32,
        fps_milli: Option<u64>,
        timescale: Option<u32>,
        width: Option<u32>,
        height: Option<u32>,
        channels: Option<u32>,
        whole_dur_ms: Option<u64>,
    },
    /// Timescale lossless remux (normalize_timescale_lossless)
    Timescale {
        target_timescale: u32,
    },
    /// Audio-only re-encode (normalize_audio_only)
    AudioOnly {
        acodec: String,
        sample_rate: u32,
        timescale: Option<u32>,
        channels: Option<u32>,
        whole_dur_ms: Option<u64>,
    },
}

/// In-memory normalization dedup cache shared across a single merge job.
///
/// Key insight: when Repeat expands a playlist (e.g. A,B,C → A,B,C,A,B,C),
/// the same source file appears multiple times with the SAME normalization
/// profile. Without this cache, each occurrence is re-normalized independently
/// — a huge waste.
///
/// Cache key: (canonical_source_path, NormSignature)
/// Cache value: path to the already-normalized file
///
/// Thread-safe via internal Mutex. Use `Arc<NormalizationCache>` for sharing.
pub struct NormalizationCache {
    cache: Mutex<HashMap<(String, NormSignature), PathBuf>>,
    /// Number of cache hits (files saved from re-normalization)
    hit_count: AtomicUsize,
}

impl NormalizationCache {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            hit_count: AtomicUsize::new(0),
        }
    }

    /// Check if a normalized version already exists for this (source, profile).
    /// Returns the cached normalized path if found.
    pub fn get(
        &self,
        source_path: &str,
        signature: &NormSignature,
    ) -> Option<PathBuf> {
        let cache = self.cache.lock().ok()?;
        let key = (source_path.to_string(), signature.clone());
        let cached = cache.get(&key)?.clone();

        // Validate the cached file still exists and is non-empty
        if cached.exists() && cached.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            self.hit_count.fetch_add(1, Ordering::Relaxed);
            Some(cached)
        } else {
            None
        }
    }

    /// Insert a normalized file into the cache for future reuse.
    pub fn insert(
        &self,
        source_path: &str,
        signature: NormSignature,
        normalized_path: PathBuf,
    ) {
        if let Ok(mut cache) = self.cache.lock() {
            let key = (source_path.to_string(), signature);
            cache.insert(key, normalized_path);
        }
    }
}

impl Default for NormalizationCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_source() -> PathBuf {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("norm_cache_test_{}_{}", std::process::id(), ts));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_cache_miss_then_hit() {
        let cache = Arc::new(NormalizationCache::new());
        let dir = test_source();

        let source = dir.join("test_source.mp4");
        std::fs::write(&source, b"fake video content").unwrap();

        let normalized = dir.join("test_normalized.mp4");
        std::fs::write(&normalized, b"fake normalized content").unwrap();

        let sig = NormSignature::Profile {
            vcodec: "libx264".to_string(),
            acodec: "aac".to_string(),
            sample_rate: 48000,
            fps_milli: Some(30000),
            timescale: Some(90000),
            width: Some(1920),
            height: Some(1080),
            channels: Some(2),
            whole_dur_ms: None,
        };

        // Miss
        assert!(cache.get(source.to_str().unwrap(), &sig).is_none());

        // Insert
        cache.insert(source.to_str().unwrap(), sig.clone(), normalized.clone());

        // Hit
        let result = cache.get(source.to_str().unwrap(), &sig);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), normalized);
    }

    #[test]
    fn test_different_signatures() {
        let cache = NormalizationCache::new();
        let dir = test_source();

        // Create actual files so cache validation passes
        let cached1 = dir.join("ts1.mp4");
        std::fs::write(&cached1, b"cached1").unwrap();

        let sig1 = NormSignature::Timescale { target_timescale: 90000 };
        let sig2 = NormSignature::Timescale { target_timescale: 180000 };
        let source = dir.join("source.mp4");
        std::fs::write(&source, b"src").unwrap();

        cache.insert(source.to_str().unwrap(), sig1.clone(), cached1.clone());
        assert!(cache.get(source.to_str().unwrap(), &sig2).is_none());
        assert!(cache.get(source.to_str().unwrap(), &sig1).is_some());
    }

    #[test]
    fn test_cache_miss_on_deleted_file() {
        let cache = NormalizationCache::new();
        let dir = test_source();

        let source = dir.join("source.mp4");
        std::fs::write(&source, b"content").unwrap();

        let normalized = dir.join("cached.mp4");
        std::fs::write(&normalized, b"cached content").unwrap();

        let sig = NormSignature::AudioOnly {
            acodec: "aac".to_string(),
            sample_rate: 44100,
            timescale: None,
            channels: Some(2),
            whole_dur_ms: None,
        };

        cache.insert(source.to_str().unwrap(), sig.clone(), normalized.clone());
        assert!(cache.get(source.to_str().unwrap(), &sig).is_some());

        // Delete the cached file — should now miss
        std::fs::remove_file(&normalized).unwrap();
        assert!(cache.get(source.to_str().unwrap(), &sig).is_none());
    }

    #[test]
    fn test_zero_length_file_miss() {
        let cache = NormalizationCache::new();
        let dir = test_source();

        let source = dir.join("source.mp4");
        std::fs::write(&source, b"content").unwrap();

        let normalized = dir.join("empty.mp4");
        std::fs::write(&normalized, b"").unwrap(); // zero-length

        let sig = NormSignature::Profile {
            vcodec: "libx264".to_string(),
            acodec: "aac".to_string(),
            sample_rate: 48000,
            fps_milli: None,
            timescale: None,
            width: None,
            height: None,
            channels: None,
            whole_dur_ms: None,
        };

        cache.insert(source.to_str().unwrap(), sig.clone(), normalized.clone());
        assert!(cache.get(source.to_str().unwrap(), &sig).is_none());
    }

    #[test]
    fn test_cache_miss_when_source_differs() {
        let cache = NormalizationCache::new();
        let dir = test_source();

        let source_a = dir.join("source_a.mp4");
        std::fs::write(&source_a, b"content_a").unwrap();
        let source_b = dir.join("source_b.mp4");
        std::fs::write(&source_b, b"content_b").unwrap();

        let normalized = dir.join("norm.mp4");
        std::fs::write(&normalized, b"norm").unwrap();

        let sig = NormSignature::Profile {
            vcodec: "libx264".to_string(),
            acodec: "aac".to_string(),
            sample_rate: 48000,
            fps_milli: None,
            timescale: None,
            width: None,
            height: None,
            channels: None,
            whole_dur_ms: None,
        };

        cache.insert(source_a.to_str().unwrap(), sig.clone(), normalized.clone());
        assert!(cache.get(source_a.to_str().unwrap(), &sig).is_some());
        assert!(cache.get(source_b.to_str().unwrap(), &sig).is_none());
    }
}
