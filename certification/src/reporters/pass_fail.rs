// Pass/Fail determination helpers
use crate::reporters::TestResult;
use crate::reporters::TestStatus;

// Threshold tolerances
pub const DURATION_TOLERANCE_PCT: f64 = 5.0; // 5% duration mismatch allowed
pub const SIZE_TOLERANCE_PCT: f64 = 10.0; // 10% size mismatch allowed

pub fn determine_status(
    output_exists: bool,
    duration_matches: bool,
    has_video: bool,
    has_audio: bool,
    ffprobe_valid: bool,
    error: Option<&str>,
) -> TestStatus {
    if !output_exists {
        return TestStatus::Fail;
    }

    if let Some(e) = error {
        if e.contains("crash") || e.contains("panic") {
            return TestStatus::Fail;
        }
        if e.contains("timeout") {
            return TestStatus::Warn;
        }
    }

    if !ffprobe_valid {
        return TestStatus::Fail;
    }

    if !has_video {
        return TestStatus::Fail;
    }

    // has_audio is optional for some tests
    if !duration_matches {
        return TestStatus::Fail;
    }

    TestStatus::Pass
}

pub fn summarize_results(results: &[TestResult]) -> (usize, usize, usize, usize) {
    let mut passed = 0;
    let mut failed = 0;
    let mut warnings = 0;
    let mut skipped = 0;

    for r in results {
        match r.status {
            TestStatus::Pass => passed += 1,
            TestStatus::Fail => failed += 1,
            TestStatus::Warn => warnings += 1,
            TestStatus::Skip => skipped += 1,
        }
    }

    (passed, failed, warnings, skipped)
}

pub fn calculate_score(results: &[TestResult]) -> f64 {
    let total = results.len() as f64;
    if total == 0.0 {
        return 10.0;
    }

    let (passed, failed, warnings, skipped) = summarize_results(results);

    let weighted = (passed as f64 * 1.0)
        + (warnings as f64 * 0.5)
        + (skipped as f64 * 0.8)
        + (failed as f64 * 0.0);

    (weighted / total) * 10.0
}

pub fn format_summary(results: &[TestResult], phase: &str) -> String {
    let (passed, failed, warnings, skipped) = summarize_results(results);
    let score = calculate_score(results);

    format!(
        "Phase {} Summary: {}/{} passed (score {:.1}/10) | {} fail, {} warn, {} skip",
        phase,
        passed,
        results.len(),
        score,
        failed,
        warnings,
        skipped
    )
}