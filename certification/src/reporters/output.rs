// Reporters output formatting
use crate::reporters::{FinalReport, PhaseReport, TestResult, TestStatus};

pub struct OutputFormatter;

impl OutputFormatter {
    pub fn new() -> Self {
        Self
    }

    pub fn format_phase_summary(&self, phase: &PhaseReport) -> String {
        let status_icon = if phase.failed > 0 {
            "❌"
        } else if phase.warnings > 0 {
            "⚠️"
        } else {
            "✅"
        };

        format!(
            "{} Phase {} — Score: {:.1}/10 ({}/{} tests)",
            status_icon,
            phase.phase,
            phase.score(),
            phase.passed,
            phase.total_tests
        )
    }

    pub fn format_test_result(&self, result: &TestResult) -> String {
        let status_icon = match result.status {
            TestStatus::Pass => "✅",
            TestStatus::Fail => "❌",
            TestStatus::Warn => "⚠️",
            TestStatus::Skip => "⏭️",
        };

        format!(
            "{} {} - {} ({:.1}s)",
            status_icon,
            result.name,
            result.phase,
            result.duration_secs
        )
    }

    pub fn format_final_summary(&self, report: &FinalReport) -> String {
        let overall_icon = if report.total_failed > 0 {
            "❌"
        } else if report.total_warnings > 0 {
            "⚠️"
        } else {
            "✅"
        };

        format!(
            "{} OVERALL SCORE: {:.1}/10 ({}/{} tests passed)",
            overall_icon,
            report.overall_score,
            report.total_passed,
            report.total_tests
        )
    }
}

impl Default for OutputFormatter {
    fn default() -> Self {
        Self::new()
    }
}