use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub timestamp: String,
    pub elapsed_ms: u64,
    pub event_type: String,
    pub file_index: Option<usize>,
    pub file_path: Option<String>,
    pub concat_index: Option<usize>,
    pub output_timestamp_ms: Option<i64>,
    pub output_duration_ms: Option<i64>,
    pub progress_pct: Option<f64>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeTimeline {
    pub job_id: String,
    pub start_time: String,
    pub start_instant: u64,
    pub events: Vec<RuntimeEvent>,
    pub file_completions: Vec<FileCompletion>,
    pub milestones: Vec<Milestone>,
    pub crashed: bool,
    pub crash_event_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCompletion {
    pub file_index: usize,
    pub file_path: String,
    pub elapsed_ms: u64,
    pub output_timestamp_ms: i64,
    pub output_duration_ms: i64,
    pub packets_processed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub name: String,
    pub elapsed_ms: u64,
    pub description: String,
}

pub struct RuntimeTimelineRecorder {
    job_id: String,
    start_instant: Instant,
    start_time: String,
    events: Arc<Mutex<Vec<RuntimeEvent>>>,
    file_completions: Arc<Mutex<Vec<FileCompletion>>>,
    milestones: Arc<Mutex<Vec<Milestone>>>,
    crashed: Arc<Mutex<bool>>,
    crash_event_index: Arc<Mutex<Option<usize>>>,
    last_event_index: Arc<Mutex<usize>>,
}

impl RuntimeTimelineRecorder {
    pub fn new(job_id: String) -> Self {
        let start_instant = Instant::now();
        let start_time = chrono_now_string();

        Self {
            job_id,
            start_instant,
            start_time,
            events: Arc::new(Mutex::new(Vec::new())),
            file_completions: Arc::new(Mutex::new(Vec::new())),
            milestones: Arc::new(Mutex::new(Vec::new())),
            crashed: Arc::new(Mutex::new(false)),
            crash_event_index: Arc::new(Mutex::new(None)),
            last_event_index: Arc::new(Mutex::new(0)),
        }
    }

    pub fn record_event(&self, event_type: &str, file_index: Option<usize>, file_path: Option<String>, details: Option<String>) {
        let elapsed = self.start_instant.elapsed().as_millis() as u64;
        let event = RuntimeEvent {
            timestamp: chrono_now_string(),
            elapsed_ms: elapsed,
            event_type: event_type.to_string(),
            file_index,
            file_path,
            concat_index: None,
            output_timestamp_ms: None,
            output_duration_ms: None,
            progress_pct: None,
            details,
        };

        let mut events = self.events.lock().unwrap();
        let idx = events.len();
        events.push(event);
        *self.last_event_index.lock().unwrap() = idx;
    }

    pub fn record_file_start(&self, file_index: usize, file_path: &str) {
        let elapsed = self.start_instant.elapsed().as_millis() as u64;
        let event = RuntimeEvent {
            timestamp: chrono_now_string(),
            elapsed_ms: elapsed,
            event_type: "FILE_START".to_string(),
            file_index: Some(file_index),
            file_path: Some(file_path.to_string()),
            concat_index: Some(file_index),
            output_timestamp_ms: None,
            output_duration_ms: None,
            progress_pct: None,
            details: None,
        };

        let mut events = self.events.lock().unwrap();
        events.push(event);
    }

    pub fn record_file_complete(&self, file_index: usize, file_path: &str, output_timestamp_ms: i64, output_duration_ms: i64) {
        let elapsed = self.start_instant.elapsed().as_millis() as u64;

        let completion = FileCompletion {
            file_index,
            file_path: file_path.to_string(),
            elapsed_ms: elapsed,
            output_timestamp_ms,
            output_duration_ms,
            packets_processed: None,
        };

        self.file_completions.lock().unwrap().push(completion);

        let event = RuntimeEvent {
            timestamp: chrono_now_string(),
            elapsed_ms: elapsed,
            event_type: "FILE_COMPLETE".to_string(),
            file_index: Some(file_index),
            file_path: Some(file_path.to_string()),
            concat_index: Some(file_index),
            output_timestamp_ms: Some(output_timestamp_ms),
            output_duration_ms: Some(output_duration_ms),
            progress_pct: None,
            details: None,
        };

        self.events.lock().unwrap().push(event);
    }

    pub fn record_progress(&self, file_index: usize, output_timestamp_ms: i64, progress_pct: f64) {
        let event = RuntimeEvent {
            timestamp: chrono_now_string(),
            elapsed_ms: self.start_instant.elapsed().as_millis() as u64,
            event_type: "PROGRESS".to_string(),
            file_index: Some(file_index),
            file_path: None,
            concat_index: Some(file_index),
            output_timestamp_ms: Some(output_timestamp_ms),
            output_duration_ms: None,
            progress_pct: Some(progress_pct),
            details: None,
        };

        self.events.lock().unwrap().push(event);
    }

    pub fn record_milestone(&self, name: &str, description: &str) {
        let elapsed = self.start_instant.elapsed().as_millis() as u64;

        let milestone = Milestone {
            name: name.to_string(),
            elapsed_ms: elapsed,
            description: description.to_string(),
        };

        self.milestones.lock().unwrap().push(milestone);
    }

    pub fn record_crash(&self, details: &str) {
        *self.crashed.lock().unwrap() = true;
        let idx = *self.last_event_index.lock().unwrap();
        *self.crash_event_index.lock().unwrap() = Some(idx);

        let event = RuntimeEvent {
            timestamp: chrono_now_string(),
            elapsed_ms: self.start_instant.elapsed().as_millis() as u64,
            event_type: "CRASH".to_string(),
            file_index: None,
            file_path: None,
            concat_index: None,
            output_timestamp_ms: None,
            output_duration_ms: None,
            progress_pct: None,
            details: Some(details.to_string()),
        };

        self.events.lock().unwrap().push(event);
    }

    pub fn generate_report(&self) -> RuntimeTimeline {
        RuntimeTimeline {
            job_id: self.job_id.clone(),
            start_time: self.start_time.clone(),
            start_instant: self.start_instant.elapsed().as_millis() as u64,
            events: self.events.lock().unwrap().clone(),
            file_completions: self.file_completions.lock().unwrap().clone(),
            milestones: self.milestones.lock().unwrap().clone(),
            crashed: *self.crashed.lock().unwrap(),
            crash_event_index: *self.crash_event_index.lock().unwrap(),
        }
    }
}

fn chrono_now_string() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    let secs = now.as_secs();
    let millis = now.subsec_millis();
    format!("{}.{:03}", secs, millis)
}

impl std::fmt::Display for RuntimeTimeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║                  RUNTIME TIMELINE                                        ║")?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════════════╝")?;
        writeln!(f)?;
        writeln!(f, "Job ID: {}", self.job_id)?;
        writeln!(f, "Start Time: {}", self.start_time)?;
        writeln!(f, "Total Events: {}", self.events.len())?;
        writeln!(f, "Files Completed: {}", self.file_completions.len())?;
        writeln!(f, "Crashed: {}", if self.crashed { "YES ❌" } else { "NO ✅" })?;
        writeln!(f)?;

        if !self.milestones.is_empty() {
            writeln!(f, "MILESTONES:")?;
            for m in &self.milestones {
                writeln!(f, "  [{:>8}ms] {} — {}", m.elapsed_ms, m.name, m.description)?;
            }
            writeln!(f)?;
        }

        writeln!(f, "FILE COMPLETIONS:")?;
        for fc in &self.file_completions {
            writeln!(f, "  File {:>3}: {}ms, output_ts={}, duration={}",
                fc.file_index, fc.elapsed_ms, fc.output_timestamp_ms, fc.output_duration_ms)?;
        }
        writeln!(f)?;

        if let Some(crash_idx) = self.crash_event_index {
            writeln!(f, "CRASH DETAILS:")?;
            if let Some(event) = self.events.get(crash_idx) {
                writeln!(f, "  Elapsed: {}ms", event.elapsed_ms)?;
                if let Some(ref details) = event.details {
                    writeln!(f, "  Details: {}", details)?;
                }
            }
        }

        Ok(())
    }
}

pub struct RuntimeTimelineLogger {
    recorder: Arc<RuntimeTimelineRecorder>,
}

impl RuntimeTimelineLogger {
    pub fn new(job_id: String) -> Self {
        Self {
            recorder: Arc::new(RuntimeTimelineRecorder::new(job_id)),
        }
    }

    pub fn recorder(&self) -> Arc<RuntimeTimelineRecorder> {
        self.recorder.clone()
    }

    pub fn log_to_file(&self, path: &PathBuf) -> std::io::Result<()> {
        let timeline = self.recorder.generate_report();
        let json = serde_json::to_string_pretty(&timeline).unwrap();
        std::fs::write(path, json)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CumulativeTimestampDrift {
    pub file_index: usize,
    pub file_path: String,
    pub expected_start: i64,
    pub actual_start: i64,
    pub drift_ms: i64,
    pub expected_duration: i64,
    pub actual_duration: i64,
    pub duration_delta_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistDriftReport {
    pub file_completions: Vec<FileCompletion>,
    pub drifts: Vec<CumulativeTimestampDrift>,
    pub total_drift_ms: i64,
    pub max_drift_ms: i64,
    pub drift_at_failure: Option<i64>,
    pub failure_file_index: Option<usize>,
}

pub fn compute_playlist_drift(completions: &[FileCompletion]) -> PlaylistDriftReport {
    let mut drifts = Vec::new();
    let mut cumulative_expected = 0i64;
    let mut total_drift = 0i64;
    let mut max_drift = 0i64;

    for (_i, completion) in completions.iter().enumerate() {
        let expected_start = cumulative_expected;
        let drift = completion.output_timestamp_ms - expected_start;

        cumulative_expected += completion.output_duration_ms;
        total_drift += drift.abs();
        max_drift = max_drift.max(drift.abs());

        drifts.push(CumulativeTimestampDrift {
            file_index: completion.file_index,
            file_path: completion.file_path.clone(),
            expected_start,
            actual_start: completion.output_timestamp_ms,
            drift_ms: drift,
            expected_duration: 0,
            actual_duration: completion.output_duration_ms,
            duration_delta_ms: 0,
        });
    }

    PlaylistDriftReport {
        file_completions: completions.to_vec(),
        drifts,
        total_drift_ms: total_drift,
        max_drift_ms: max_drift,
        drift_at_failure: None,
        failure_file_index: None,
    }
}

impl std::fmt::Display for PlaylistDriftReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║              CUMULATIVE TIMESTAMP DRIFT REPORT                           ║")?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════════════╝")?;
        writeln!(f)?;
        writeln!(f, "Files analyzed: {}", self.file_completions.len())?;
        writeln!(f, "Total drift: {}ms ({:.1}s)", self.total_drift_ms, self.total_drift_ms as f64 / 1000.0)?;
        writeln!(f, "Max single drift: {}ms ({:.1}s)", self.max_drift_ms, self.max_drift_ms as f64 / 1000.0)?;
        writeln!(f)?;

        writeln!(f, "DRIFT PER FILE:")?;
        writeln!(f, "  {:>5} | {:>10} | {:>10} | {:>10} | {:>8}",
            "File", "Expected", "Actual", "Drift", "Status")?;
        writeln!(f, "  {}", "-".repeat(65))?;
        for drift in &self.drifts {
            let status = if drift.drift_ms.abs() < 100 {
                "OK"
            } else if drift.drift_ms.abs() < 1000 {
                "WARN"
            } else {
                "ERROR"
            };
            writeln!(f, "  {:>5} | {:>10} | {:>10} | {:>+10} | {}",
                drift.file_index,
                drift.expected_start,
                drift.actual_start,
                drift.drift_ms,
                status)?;
        }

        Ok(())
    }
}