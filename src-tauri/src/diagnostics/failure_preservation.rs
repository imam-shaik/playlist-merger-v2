use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureEvidence {
    pub job_id: String,
    pub failure_timestamp: String,
    pub merge_mode: String,
    pub ffmpeg_version: Option<String>,
    pub ffprobe_version: Option<String>,
    pub mkvmerge_version: Option<String>,
    pub preserved_files: HashMap<String, String>,
    pub preserved_logs: Vec<LogEntry>,
    pub forensic_bundle: Option<String>,
    pub timeline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePreservationConfig {
    pub preserve_normalized_files: bool,
    pub preserve_concat_list: bool,
    pub preserve_temp_output: bool,
    pub preserve_stderr: bool,
    pub preserve_ffprobe_dumps: bool,
    pub preserve_runtime_timeline: bool,
    pub preserve_forensic_bundle: bool,
    pub desktop_path: PathBuf,
}

impl Default for EvidencePreservationConfig {
    fn default() -> Self {
        Self {
            preserve_normalized_files: true,
            preserve_concat_list: true,
            preserve_temp_output: true,
            preserve_stderr: true,
            preserve_ffprobe_dumps: true,
            preserve_runtime_timeline: true,
            preserve_forensic_bundle: true,
            desktop_path: dirs::desktop_dir().unwrap_or_else(|| PathBuf::from(".")),
        }
    }
}

pub struct EvidencePreserver {
    job_id: String,
    merge_mode: String,
    config: EvidencePreservationConfig,
    preserved: HashMap<String, PathBuf>,
}

impl EvidencePreserver {
    pub fn new(job_id: String, merge_mode: String) -> Self {
        Self {
            job_id,
            merge_mode,
            config: EvidencePreservationConfig::default(),
            preserved: HashMap::new(),
        }
    }

    pub fn with_config(job_id: String, merge_mode: String, config: EvidencePreservationConfig) -> Self {
        Self {
            job_id,
            merge_mode,
            config,
            preserved: HashMap::new(),
        }
    }

    pub fn preserve_concat_list(&mut self, path: &Path) -> Option<PathBuf> {
        if !self.config.preserve_concat_list {
            return None;
        }

        let dest = self.preserve_file(path, "concat_list")?;
        Some(dest)
    }

    pub fn preserve_normalized_file(&mut self, original_path: &str, normalized_path: &Path) -> Option<PathBuf> {
        if !self.config.preserve_normalized_files {
            return None;
        }

        let filename = Path::new(original_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let dest = self.preserve_file_internal(normalized_path, &format!("normalized_{}", filename))?;
        Some(dest)
    }

    pub fn preserve_temp_output(&mut self, path: &Path) -> Option<PathBuf> {
        if !self.config.preserve_temp_output {
            return None;
        }

        let dest = self.preserve_file(path, "temp_output")?;
        Some(dest)
    }

    pub fn preserve_stderr(&mut self, content: &str, stage: &str) -> Option<PathBuf> {
        if !self.config.preserve_stderr {
            return None;
        }

        let dest_dir = self.config.desktop_path.join(format!("forensic_{}", self.job_id));
        let _ = fs::create_dir_all(&dest_dir);

        let filename = format!("stderr_{}.txt", stage);
        let dest = dest_dir.join(&filename);

        if fs::write(&dest, content).is_ok() {
            self.preserved.insert(format!("stderr_{}", stage), dest.clone());
            Some(dest)
        } else {
            None
        }
    }

    pub fn preserve_runtime_timeline(&mut self, timeline_json: &str) -> Option<PathBuf> {
        if !self.config.preserve_runtime_timeline {
            return None;
        }

        let dest_dir = self.config.desktop_path.join(format!("forensic_{}", self.job_id));
        let _ = fs::create_dir_all(&dest_dir);

        let dest = dest_dir.join("runtime_timeline.json");

        if fs::write(&dest, timeline_json).is_ok() {
            self.preserved.insert("runtime_timeline".to_string(), dest.clone());
            Some(dest)
        } else {
            None
        }
    }

    pub fn preserve_forensic_bundle(&mut self, bundle: &str) -> Option<PathBuf> {
        if !self.config.preserve_forensic_bundle {
            return None;
        }

        let dest_dir = self.config.desktop_path.join(format!("forensic_{}", self.job_id));
        let _ = fs::create_dir_all(&dest_dir);

        let dest = dest_dir.join("forensic_bundle.json");

        if fs::write(&dest, bundle).is_ok() {
            self.preserved.insert("forensic_bundle".to_string(), dest.clone());
            Some(dest)
        } else {
            None
        }
    }

    pub fn capture_ffmpeg_version(&self) -> Option<String> {
        let output = Command::new("ffmpeg")
            .args(["-version"])
            .output()
            .ok()?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        stderr.lines().next().map(|s| s.to_string())
    }

    pub fn capture_mkvmerge_version(&self) -> Option<String> {
        let output = Command::new("mkvmerge")
            .args(["--version"])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines().next().map(|s| s.to_string())
    }

    pub fn generate_failure_report(&self) -> FailureEvidence {
        let preserved_files = self.preserved.iter()
            .map(|(k, v)| (k.clone(), v.to_string_lossy().into_owned()))
            .collect();

        let preserved_logs = self.preserved.iter()
            .filter(|(_, path)| path.exists())
            .map(|(name, path)| {
                let metadata = fs::metadata(path).ok();
                LogEntry {
                    name: name.clone(),
                    path: path.to_string_lossy().into_owned(),
                    size_bytes: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                    hash: self.compute_hash(path).ok(),
                }
            })
            .collect();

        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();

        FailureEvidence {
            job_id: self.job_id.clone(),
            failure_timestamp: format!("{}.{:03}", now.as_secs(), now.subsec_millis()),
            merge_mode: self.merge_mode.clone(),
            ffmpeg_version: self.capture_ffmpeg_version(),
            ffprobe_version: None,
            mkvmerge_version: self.capture_mkvmerge_version(),
            preserved_files,
            preserved_logs,
            forensic_bundle: None,
            timeline: None,
        }
    }

    pub fn save_failure_report(&self, path: &Path) -> std::io::Result<()> {
        let evidence = self.generate_failure_report();
        let json = serde_json::to_string_pretty(&evidence).unwrap();
        fs::write(path, json)
    }

    pub fn get_preserved_dir(&self) -> PathBuf {
        self.config.desktop_path.join(format!("forensic_{}", self.job_id))
    }

    fn preserve_file(&mut self, source: &Path, prefix: &str) -> Option<PathBuf> {
        let dest_dir = self.config.desktop_path.join(format!("forensic_{}", self.job_id));
        let _ = fs::create_dir_all(&dest_dir);

        let filename = source.file_name()
            .and_then(|n| n.to_str())
            .map(|s| format!("{}_{}", prefix, s))
            .unwrap_or_else(|| prefix.to_string());

        let dest = dest_dir.join(&filename);

        if fs::copy(source, &dest).is_ok() {
            self.preserved.insert(prefix.to_string(), dest.clone());
            Some(dest)
        } else {
            None
        }
    }

    fn preserve_file_internal(&mut self, source: &Path, filename: &str) -> Option<PathBuf> {
        let dest_dir = self.config.desktop_path.join(format!("forensic_{}", self.job_id));
        let _ = fs::create_dir_all(&dest_dir);

        let dest = dest_dir.join(filename);

        if fs::copy(source, &dest).is_ok() {
            self.preserved.insert(filename.to_string(), dest.clone());
            Some(dest)
        } else {
            None
        }
    }

    fn compute_hash(&self, path: &Path) -> Result<String, std::io::Error> {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let data = fs::read(path)?;
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        Ok(format!("{:x}", hasher.finish()))
    }
}

impl std::fmt::Display for FailureEvidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║                  FAILURE EVIDENCE REPORT                                 ║")?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════════════╝")?;
        writeln!(f)?;
        writeln!(f, "Job ID: {}", self.job_id)?;
        writeln!(f, "Failure Timestamp: {}", self.failure_timestamp)?;
        writeln!(f, "Merge Mode: {}", self.merge_mode)?;
        writeln!(f)?;

        if let Some(ref ver) = self.ffmpeg_version {
            writeln!(f, "FFmpeg: {}", ver)?;
        }
        if let Some(ref ver) = self.mkvmerge_version {
            writeln!(f, "Mkvmerge: {}", ver)?;
        }
        writeln!(f)?;

        writeln!(f, "PRESERVED FILES:")?;
        for (name, path) in &self.preserved_files {
            writeln!(f, "  {} → {}", name, path)?;
        }
        writeln!(f)?;

        if !self.preserved_logs.is_empty() {
            writeln!(f, "PRESERVED LOGS:")?;
            for log in &self.preserved_logs {
                writeln!(f, "  {} ({} bytes)", log.name, log.size_bytes)?;
            }
        }

        Ok(())
    }
}