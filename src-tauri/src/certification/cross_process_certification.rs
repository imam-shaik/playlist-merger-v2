use std::time::Instant;
use std::process;
use std::fs;

pub use crate::ffmpeg::normalization::MergePlan;

pub struct CrossProcessReport {
    pub total_runs: usize,
    pub passed: bool,
    pub run_jsons: Vec<String>,
    pub comparisons: Vec<CrossComparison>,
    pub total_duration_ms: u128,
}

pub struct CrossComparison {
    pub run_a: usize,
    pub run_b: usize,
    pub identical: bool,
}

pub fn certify_cross_process<R>(
    playlist_name: &str,
    runs: usize,
    build_plan: R,
) -> CrossProcessReport
where
    R: Fn() -> Option<MergePlan>,
{
    let start = Instant::now();
    let mut run_jsons = Vec::new();
    let mut comparisons = Vec::new();

    log::info!("[CERT:CROSS-PROCESS] ═══════════════════════════════════════════════════════════");
    log::info!("[CERT:CROSS-PROCESS] CROSS-PROCESS IDEMPOTENCY CERTIFICATION");
    log::info!("[CERT:CROSS-PROCESS] Playlist: {}", playlist_name);
    log::info!("[CERT:CROSS-PROCESS] Runs: {} (separate processes)", runs);
    log::info!("[CERT:CROSS-PROCESS] ═══════════════════════════════════════════════════════════");

    for run_id in 0..runs {
        let run_start = Instant::now();

        log::info!("[CERT:CROSS-PROCESS] Run {}/{} starting (process {})...",
            run_id + 1, runs, process::id());

        let plan = match build_plan() {
            Some(p) => p,
            None => {
                log::error!("[CERT:CROSS-PROCESS] Run {} failed - could not build merge plan", run_id + 1);
                break;
            }
        };

        let json = plan.to_json();
        run_jsons.push(json.clone());

        let run_duration = run_start.elapsed().as_millis();
        log::info!("[CERT:CROSS-PROCESS] Run {}/{} completed in {}ms", run_id + 1, runs, run_duration);

        if run_id > 0 {
            let prev_json = &run_jsons[run_id - 1];
            let identical = run_jsons[run_id] == *prev_json;

            comparisons.push(CrossComparison {
                run_a: run_id,
                run_b: run_id + 1,
                identical,
            });

            if identical {
                log::info!("[CERT:CROSS-PROCESS] Run {} vs Run {}: ✅ IDENTICAL", run_id, run_id + 1);
            } else {
                log::error!("[CERT:CROSS-PROCESS] Run {} vs Run {}: ❌ DIFFERENT", run_id, run_id + 1);
                log::error!("[CERT:CROSS-PROCESS] Run {} JSON length: {} bytes", run_id, prev_json.len());
                log::error!("[CERT:CROSS-PROCESS] Run {} JSON length: {} bytes", run_id + 1, run_jsons[run_id].len());
            }
        }
    }

    let total_duration = start.elapsed().as_millis();
    let all_identical = comparisons.iter().all(|c| c.identical);
    let passed = all_identical && comparisons.len() == runs - 1;

    log::info!("[CERT:CROSS-PROCESS] ═══════════════════════════════════════════════════════════");
    if passed {
        log::info!("[CERT:CROSS-PROCESS] ✅ CERTIFICATION PASSED - All {} runs identical", runs);
    } else {
        log::error!("[CERT:CROSS-PROCESS] ❌ CERTIFICATION FAILED - Differences found");
    }
    log::info!("[CERT:CROSS-PROCESS] Total duration: {}ms", total_duration);
    log::info!("[CERT:CROSS-PROCESS] ═══════════════════════════════════════════════════════════");

    CrossProcessReport {
        total_runs: runs,
        passed,
        run_jsons,
        comparisons,
        total_duration_ms: total_duration,
    }
}

impl std::fmt::Display for CrossProcessReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║       CROSS-PROCESS IDEMPOTENCY CERTIFICATION REPORT                ║")?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  Total runs: {:53} ║", self.total_runs)?;
        writeln!(f, "║  Total duration: {:47}ms ║", self.total_duration_ms)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  RUN COMPARISON RESULTS:")?;
        for comp in &self.comparisons {
            let status = if comp.identical { "✅ PASS" } else { "❌ FAIL" };
            writeln!(f, "║    Run {} vs Run {}: {}                                      ║", comp.run_a + 1, comp.run_b + 1, status)?;
        }
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        let overall = if self.passed { "✅ CERTIFIED - IDEMPOTENT" } else { "❌ FAILED - NON-IDEMPOTENT" };
        writeln!(f, "║  OVERALL: {:57} ║", overall)?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════╝")?;
        Ok(())
    }
}

pub fn save_plan_for_debug(plan: &MergePlan, path: &str) -> std::io::Result<()> {
    let json = plan.to_json();
    fs::write(path, json)?;
    Ok(())
}