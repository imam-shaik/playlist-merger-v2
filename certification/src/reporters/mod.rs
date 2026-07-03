pub mod pass_fail;
pub mod evidence;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FixType {
    TimestampRepair,
    ContainerRemux,
    FullReencode,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaValidationReport {
    pub total_files: usize,
    pub repaired_files: Vec<(usize, String, FixType)>,
    pub quarantined_count: usize,
    pub healthy_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairPathEntry {
    pub repair_name: String,
    pub damage_type: String,
    pub expected_result: String,
    pub verified: bool,
    pub verification_evidence: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepairCoverageMatrix {
    pub entries: Vec<RepairPathEntry>,
    pub total_paths: usize,
    pub verified_paths: usize,
    pub unverified_paths: usize,
}

impl RepairCoverageMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_entry(&mut self, entry: RepairPathEntry) {
        self.total_paths += 1;
        if entry.verified {
            self.verified_paths += 1;
        } else {
            self.unverified_paths += 1;
        }
        self.entries.push(entry);
    }

    pub fn coverage_pct(&self) -> f64 {
        if self.total_paths == 0 {
            return 100.0;
        }
        self.verified_paths as f64 / self.total_paths as f64 * 100.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Pass,
    Fail,
    Warn,
    Skip,
}

impl fmt::Display for TestStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TestStatus::Pass => write!(f, "PASS"),
            TestStatus::Fail => write!(f, "FAIL"),
            TestStatus::Warn => write!(f, "WARN"),
            TestStatus::Skip => write!(f, "SKIP"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub phase: String,
    pub test_name: String,
    pub name: String,
    pub status: TestStatus,
    pub duration_secs: f64,
    pub evidence: Vec<Evidence>,
    pub logs: Vec<String>,
    pub error: Option<String>,
    pub metrics: TestMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestMetrics {
    pub input_file_count: Option<usize>,
    pub output_file_count: Option<usize>,
    pub expected_duration_ms: Option<u64>,
    pub actual_duration_ms: Option<u64>,
    pub output_size_bytes: Option<u64>,
    pub peak_ram_mb: Option<f64>,
    pub avg_cpu_percent: Option<f64>,
    pub checkpoint_size_bytes: Option<u64>,
    pub resume_time_ms: Option<u64>,
}

impl TestMetrics {
    pub fn duration_match(&self, tolerance_pct: f64) -> bool {
        match (self.expected_duration_ms, self.actual_duration_ms) {
            (Some(expected), Some(actual)) => {
                let diff = (expected as f64 - actual as f64).abs();
                let threshold = expected as f64 * tolerance_pct / 100.0;
                diff <= threshold
            }
            _ => true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_type: EvidenceType,
    pub path: Option<PathBuf>,
    pub description: String,
    pub checksum: Option<String>,
    pub timestamp: DateTime<Local>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    Screenshot,
    LogFile,
    VideoFile,
    AudioFile,
    SubtitleFile,
    FfprobeOutput,
    CrashDump,
    CheckpointFile,
    MemorySnapshot,
}

impl Evidence {
    pub fn new(evidence_type: EvidenceType, description: &str) -> Self {
        Self {
            evidence_type,
            path: None,
            description: description.to_string(),
            checksum: None,
            timestamp: Local::now(),
        }
    }

    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path.clone());
        if path.exists() {
            self.checksum = Some(evidence::compute_file_checksum(&path));
        }
        self
    }

    pub fn with_log<T: std::fmt::Display>(mut self, log: T) -> Self {
        self.description = format!("{} | Log: {}", self.description, log);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseReport {
    pub phase: String,
    pub started_at: DateTime<Local>,
    pub completed_at: Option<DateTime<Local>>,
    pub total_tests: usize,
    pub passed: usize,
    pub failed: usize,
    pub warnings: usize,
    pub skipped: usize,
    pub results: Vec<TestResult>,
}

impl PhaseReport {
    pub fn new(phase: &str) -> Self {
        Self {
            phase: phase.to_string(),
            started_at: Local::now(),
            completed_at: None,
            total_tests: 0,
            passed: 0,
            failed: 0,
            warnings: 0,
            skipped: 0,
            results: Vec::new(),
        }
    }

    pub fn add_result(&mut self, result: TestResult) {
        match result.status {
            TestStatus::Pass => self.passed += 1,
            TestStatus::Fail => self.failed += 1,
            TestStatus::Warn => self.warnings += 1,
            TestStatus::Skip => self.skipped += 1,
        }
        self.total_tests += 1;
        self.results.push(result);
    }

    pub fn finish(&mut self) {
        self.completed_at = Some(Local::now());
    }

    pub fn pass_rate(&self) -> f64 {
        if self.total_tests == 0 {
            return 0.0;
        }
        self.passed as f64 / self.total_tests as f64 * 100.0
    }

    pub fn score(&self) -> f64 {
        // Weighted score: pass = 1.0, warn = 0.5, fail = 0.0
        let total_weight = self.total_tests as f64;
        if total_weight == 0.0 {
            return 10.0;
        }
        let weighted = self.passed as f64 * 1.0
            + self.warnings as f64 * 0.5
            + self.skipped as f64 * 0.8;
        (weighted / total_weight) * 10.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalReport {
    pub generated_at: DateTime<Local>,
    pub total_tests: usize,
    pub total_passed: usize,
    pub total_failed: usize,
    pub total_warnings: usize,
    pub total_skipped: usize,
    pub phase_reports: Vec<PhaseReport>,
    pub subsystem_scores: Vec<SubsystemScore>,
    pub overall_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemScore {
    pub subsystem: String,
    pub score: f64,
    pub evidence_count: usize,
    pub status: TestStatus,
}

impl FinalReport {
    pub fn generate(results: &[TestResult]) -> Self {
        // Group results by phase
        let mut phase_map: std::collections::HashMap<String, PhaseReport> =
            std::collections::HashMap::new();

        for result in results {
            phase_map
                .entry(result.phase.clone())
                .or_insert_with(|| PhaseReport::new(&result.phase))
                .add_result(result.clone());
        }

        let mut phase_reports: Vec<_> = phase_map.into_values().collect();
        for pr in &mut phase_reports {
            pr.finish();
        }

        let total_tests = phase_reports.iter().map(|p| p.total_tests).sum();
        let total_passed = phase_reports.iter().map(|p| p.passed).sum();
        let total_failed = phase_reports.iter().map(|p| p.failed).sum();
        let total_warnings = phase_reports.iter().map(|p| p.warnings).sum();
        let total_skipped = phase_reports.iter().map(|p| p.skipped).sum();

        // Calculate subsystem scores
        let subsystem_scores = calculate_subsystem_scores(results);

        let overall_score = if phase_reports.is_empty() {
            10.0
        } else {
            phase_reports.iter().map(|p| p.score()).sum::<f64>()
                / phase_reports.len() as f64
        };

        Self {
            generated_at: Local::now(),
            total_tests,
            total_passed,
            total_failed,
            total_warnings,
            total_skipped,
            phase_reports,
            subsystem_scores,
            overall_score,
        }
    }

    pub fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let yaml = serde_yaml::to_string(self)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }

    pub fn save_summary(&self, path: &PathBuf) -> anyhow::Result<()> {
        let mut summary = String::new();
        summary.push_str(&format!(
            "CERTIFICATION REPORT — {}\n",
            self.generated_at.format("%Y-%m-%d %H:%M:%S")
        ));
        summary.push_str(&"═".repeat(60));
        summary.push('\n');
        summary.push_str(&format!("Overall Score: {:.1}/10\n\n", self.overall_score));

        summary.push_str("PHASE SUMMARY\n");
        summary.push_str(&"-".repeat(60));
        summary.push('\n');

        for pr in &self.phase_reports {
            let status = if pr.failed > 0 {
                "❌ FAIL"
            } else if pr.warnings > 0 {
                "⚠️  WARN"
            } else if pr.passed == pr.total_tests {
                "✅ PASS"
            } else {
                "⏭️  PARTIAL"
            };
            summary.push_str(&format!(
                "  {} {} — Score: {:.1}/10 ({}/{} tests)\n",
                status, pr.phase, pr.score(), pr.passed, pr.total_tests
            ));
        }

        summary.push_str("\nSUBSYSTEM SCORES\n");
        summary.push_str(&"-".repeat(60));
        summary.push('\n');

        for ss in &self.subsystem_scores {
            let status_icon = match ss.status {
                TestStatus::Pass => "✅",
                TestStatus::Fail => "❌",
                TestStatus::Warn => "⚠️ ",
                TestStatus::Skip => "⏭️ ",
            };
            summary.push_str(&format!(
                "  {} {:20} {:.1}/10 ({} evidences)\n",
                status_icon, ss.subsystem, ss.score, ss.evidence_count
            ));
        }

        summary.push_str("\nFAILED TESTS\n");
        summary.push_str(&"-".repeat(60));
        summary.push('\n');

        let failed: Vec<_> = self
            .phase_reports
            .iter()
            .flat_map(|p| p.results.iter().filter(|r| r.status == TestStatus::Fail))
            .collect();

        if failed.is_empty() {
            summary.push_str("  None — all tests passed!\n");
        } else {
            for f in failed {
                summary.push_str(&format!("  ❌ {} / {} — {}\n", f.phase, f.name, f.error.as_deref().unwrap_or("Unknown error")));
                if let Some(evidence) = f.evidence.first() {
                    if let Some(path) = &evidence.path {
                        summary.push_str(&format!("     Evidence: {:?}\n", path));
                    }
                }
            }
        }

        std::fs::write(path, summary)?;
        Ok(())
    }
}

impl fmt::Display for FinalReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f)?;
        writeln!(f, "╔═══════════════════════════════════════════════════════════╗");
        writeln!(f, "║       RUNTIME CERTIFICATION REPORT                      ║");
        writeln!(f, "╚═══════════════════════════════════════════════════════════╝")?;
        writeln!(
            f,
            "\n📊 OVERALL SCORE: {:.1}/10 ({}/{} tests passed)",
            self.overall_score, self.total_passed, self.total_tests
        )?;

        writeln!(f, "\n📋 PHASE BREAKDOWN")?;
        writeln!(f, "{}", "─".repeat(50))?;
        for pr in &self.phase_reports {
            let icon = if pr.failed > 0 {
                "❌"
            } else if pr.warnings > 0 {
                "⚠️"
            } else {
                "✅"
            };
            writeln!(
                f,
                "  {} Phase {} — {:.1}/10 ({} pass, {} fail, {} warn, {} skip)",
                icon,
                pr.phase,
                pr.score(),
                pr.passed,
                pr.failed,
                pr.warnings,
                pr.skipped
            )?;
        }

        writeln!(f, "\n📊 SUBSYSTEM SCORES")?;
        writeln!(f, "{}", "─".repeat(50))?;
        for ss in &self.subsystem_scores {
            writeln!(
                f,
                "  {:20} {:.1}/10  {}",
                ss.subsystem,
                ss.score,
                match ss.status {
                    TestStatus::Pass => "✅",
                    TestStatus::Fail => "❌",
                    TestStatus::Warn => "⚠️",
                    TestStatus::Skip => "⏭️",
                }
            )?;
        }

        if self.total_failed > 0 {
            writeln!(f, "\n❌ FAILED TESTS")?;
            writeln!(f, "{}", "─".repeat(50))?;
            for pr in &self.phase_reports {
                for r in pr.results.iter().filter(|r| r.status == TestStatus::Fail) {
                    writeln!(f, "  • {} / {}: {}", r.phase, r.name, r.error.as_deref().unwrap_or("Unknown"))?;
                }
            }
        }

        Ok(())
    }
}

fn calculate_subsystem_scores(results: &[TestResult]) -> Vec<SubsystemScore> {
    let subsystems = [
        ("FastMkv", vec!["FastMkv", "FastMKV", "fastmkv"]),
        ("SmartMkv", vec!["SmartMkv", "SmartMKV", "smartmkv"]),
        ("Lossless", vec!["Lossless", "lossless"]),
        ("Custom", vec!["Custom", "custom"]),
        ("Repeat", vec!["Repeat", "repeat", "PhaseB", "phase_b"]),
        ("Split", vec!["Split", "split", "PhaseC", "phase_c"]),
        ("Cards", vec!["Cards", "cards", "PhaseD", "phase_d"]),
        ("Recovery", vec!["Recovery", "recovery", "PhaseE", "phase_e"]),
        ("Audio", vec!["Audio", "audio", "PhaseF", "phase_f"]),
        ("Subtitle", vec!["Subtitle", "subtitle", "PhaseG", "phase_g"]),
        ("Stress", vec!["Stress", "stress", "PhaseH", "phase_h"]),
    ];

    subsystems
        .iter()
        .map(|(name, patterns)| {
            let matches: Vec<_> = results
                .iter()
                .filter(|r| {
                    patterns.iter().any(|p| {
                        r.test_name.to_lowercase().contains(&p.to_lowercase())
                            || r.phase.to_lowercase().contains(&p.to_lowercase())
                    })
                })
                .collect();

            let total = matches.len();
            let passed = matches.iter().filter(|r| r.status == TestStatus::Pass).count();
            let warnings = matches.iter().filter(|r| r.status == TestStatus::Warn).count();

            let score = if total == 0 {
                10.0
            } else {
                (passed as f64 * 1.0 + warnings as f64 * 0.5) / total as f64 * 10.0
            };

            let status = if passed == total {
                TestStatus::Pass
            } else if passed + warnings == total {
                TestStatus::Warn
            } else if passed == 0 {
                TestStatus::Fail
            } else {
                TestStatus::Warn
            };

            SubsystemScore {
                subsystem: name.to_string(),
                score,
                evidence_count: matches.iter().map(|r| r.evidence.len()).sum(),
                status,
            }
        })
        .collect()
}