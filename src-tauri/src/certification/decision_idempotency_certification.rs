use std::time::Instant;

pub use crate::ffmpeg::normalization::MergePlan;

pub struct DecisionIdempotencyReport {
    pub total_runs: usize,
    pub passed: bool,
    pub run_jsons: Vec<String>,
    pub plan_hashes: Vec<String>,
    pub comparisons: Vec<ComparisonResult>,
    pub total_duration_ms: u128,
    pub first_error_run: Option<usize>,
}

pub struct ComparisonResult {
    pub run_a: usize,
    pub run_b: usize,
    pub plans_identical: bool,
    pub json_identical: bool,
    pub dominant_match: bool,
    pub decisions_match: bool,
}

pub fn certify_decision_idempotency<F>(
    playlist_name: &str,
    runs: usize,
    probe_and_build: F,
) -> DecisionIdempotencyReport
where
    F: Fn(usize) -> Option<MergePlan>,
{
    let start = Instant::now();
    let mut run_jsons = Vec::new();
    let mut plan_hashes = Vec::new();
    let mut comparisons = Vec::new();
    let mut first_error_run = None;

    log::info!("[CERT:IDEMPOTENCY] ═══════════════════════════════════════════════════════════");
    log::info!("[CERT:IDEMPOTENCY] DECISION IDEMPOTENCY CERTIFICATION");
    log::info!("[CERT:IDEMPOTENCY] Playlist: {}", playlist_name);
    log::info!("[CERT:IDEMPOTENCY] Runs: {}", runs);
    log::info!("[CERT:IDEMPOTENCY] ═══════════════════════════════════════════════════════════");

    for run_id in 0..runs {
        let run_start = Instant::now();
        log::info!("[CERT:IDEMPOTENCY] Run {}/{} starting...", run_id + 1, runs);

        let plan = match probe_and_build(run_id) {
            Some(p) => p,
            None => {
                log::error!("[CERT:IDEMPOTENCY] Run {} failed - could not build merge plan", run_id + 1);
                break;
            }
        };

        let json = plan.to_json();
        let hash = hash_json(&json);

        run_jsons.push(json);
        plan_hashes.push(hash.clone());

        let run_duration = run_start.elapsed().as_millis();
        log::info!("[CERT:IDEMPOTENCY] Run {}/{} completed in {}ms", run_id + 1, runs, run_duration);
        log::info!("[CERT:IDEMPOTENCY]   Dominant: {} files, {} outliers, {} audio outliers",
            plan.analysis_summary.total_files,
            plan.analysis_summary.outlier_count,
            plan.analysis_summary.audio_outlier_count);
        log::info!("[CERT:IDEMPOTENCY]   Decisions: {:?}", plan.file_decisions.iter().map(|d| format!("{:?}", d.decision)).collect::<Vec<_>>());

        if run_id > 0 {
            let prev_json = &run_jsons[run_id - 1];
            let json_identical = run_jsons[run_id] == *prev_json;

            let prev_plan: MergePlan = serde_json::from_str(prev_json).unwrap_or_else(|_| {
                log::error!("[CERT:IDEMPOTENCY] Failed to parse previous JSON");
                MergePlan {
                    backend: String::new(),
                    dominant_profile: crate::ffmpeg::normalization::DominantProfilePlan {
                        v_codec: None,
                        v_profile: None,
                        v_resolution: None,
                        v_fps: None,
                        v_time_base: None,
                        v_pixel_format: None,
                        v_color_space: None,
                        v_color_transfer: None,
                        a_codec: None,
                        a_sample_rate: None,
                        a_channels: None,
                        a_channel_layout: None,
                        container_format: None,
                        match_count: 0,
                        total_count: 0,
                    },
                    file_decisions: vec![],
                    smartmkv_breakdown: crate::ffmpeg::normalization::SmartMkvBreakdown {
                        normalize: vec![],
                        remux: vec![],
                        skip: vec![],
                    },
                    analysis_summary: crate::ffmpeg::normalization::AnalysisSummary {
                        total_files: 0,
                        outlier_count: 0,
                        audio_outlier_count: 0,
                    },
                }
            });

            let plans_identical = plan.equals(&prev_plan);
            let dominant_match = plan.dominant_profile == prev_plan.dominant_profile;
            let decisions_match = plan.file_decisions == prev_plan.file_decisions;

            let comparison = ComparisonResult {
                run_a: run_id,
                run_b: run_id + 1,
                plans_identical,
                json_identical,
                dominant_match,
                decisions_match,
            };

            comparisons.push(comparison);

            if !plans_identical || !json_identical {
                first_error_run = first_error_run.or(Some(run_id + 1));
                log::error!("[CERT:IDEMPOTENCY] ═══════════════════════════════════════════════════════════");
                log::error!("[CERT:IDEMPOTENCY] Run {} vs Run {}: ❌ DIFFERENCES DETECTED", run_id, run_id + 1);
                log::error!("[CERT:IDEMPOTENCY]   Plans identical: {}", plans_identical);
                log::error!("[CERT:IDEMPOTENCY]   JSON identical: {}", json_identical);
                log::error!("[CERT:IDEMPOTENCY]   Dominant profile match: {}", dominant_match);
                log::error!("[CERT:IDEMPOTENCY]   File decisions match: {}", decisions_match);

                if !dominant_match {
                    log::error!("[CERT:IDEMPOTENCY]   DOMINANT PROFILE MISMATCH:");
                    log::error!("[CERT:IDEMPOTENCY]     Run {}: {:?}", run_id, prev_plan.dominant_profile);
                    log::error!("[CERT:IDEMPOTENCY]     Run {}: {:?}", run_id + 1, plan.dominant_profile);
                }

                if !decisions_match {
                    log::error!("[CERT:IDEMPOTENCY]   FILE DECISIONS MISMATCH:");
                    for (i, (d1, d2)) in prev_plan.file_decisions.iter().zip(plan.file_decisions.iter()).enumerate() {
                        if d1 != d2 {
                            log::error!("[CERT:IDEMPOTENCY]     File {}: {:?} vs {:?}", i, d1.decision, d2.decision);
                            if !d1.remaining_outliers.is_empty() || !d2.remaining_outliers.is_empty() {
                                log::error!("[CERT:IDEMPOTENCY]       Run {} remaining: {:?}", run_id, d1.remaining_outliers);
                                log::error!("[CERT:IDEMPOTENCY]       Run {} remaining: {:?}", run_id + 1, d2.remaining_outliers);
                            }
                        }
                    }
                }
            } else {
                log::info!("[CERT:IDEMPOTENCY] Run {} vs Run {}: ✅ IDENTICAL", run_id, run_id + 1);
            }
        }
    }

    let total_duration = start.elapsed().as_millis();

    let all_identical = comparisons.iter().all(|c| c.plans_identical && c.json_identical);
    let passed = all_identical && comparisons.len() == runs - 1;

    log::info!("[CERT:IDEMPOTENCY] ═══════════════════════════════════════════════════════════");
    if passed {
        log::info!("[CERT:IDEMPOTENCY] ✅ CERTIFICATION PASSED - All {} runs identical", runs);
    } else {
        log::error!("[CERT:IDEMPOTENCY] ❌ CERTIFICATION FAILED - Differences found");
        if let Some(err_run) = first_error_run {
            log::error!("[CERT:IDEMPOTENCY]   First difference detected at run {}", err_run);
        }
    }
    log::info!("[CERT:IDEMPOTENCY] Total duration: {}ms", total_duration);
    log::info!("[CERT:IDEMPOTENCY] ═══════════════════════════════════════════════════════════");

    DecisionIdempotencyReport {
        total_runs: runs,
        passed,
        run_jsons,
        plan_hashes,
        comparisons,
        total_duration_ms: total_duration,
        first_error_run,
    }
}

impl std::fmt::Display for DecisionIdempotencyReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║         DECISION IDEMPOTENCY CERTIFICATION REPORT                   ║")?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  Total runs: {:53} ║", self.total_runs)?;
        writeln!(f, "║  Total duration: {:47}ms ║", self.total_duration_ms)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  RUN COMPARISON RESULTS:")?;
        for comp in &self.comparisons {
            let status = if comp.plans_identical && comp.json_identical { "✅ PASS" } else { "❌ FAIL" };
            writeln!(f, "║    Run {} vs Run {}: {}                                      ║", comp.run_a + 1, comp.run_b + 1, status)?;
            if !comp.dominant_match {
                writeln!(f, "║      ⚠️  Dominant profile differs                                  ║")?;
            }
            if !comp.decisions_match {
                writeln!(f, "║      ⚠️  File decisions differ                                    ║")?;
            }
        }
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        let overall = if self.passed { "✅ CERTIFIED - IDEMPOTENT" } else { "❌ FAILED - NON-IDEMPOTENT" };
        writeln!(f, "║  OVERALL: {:57} ║", overall)?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════╝")?;
        Ok(())
    }
}

fn hash_json(json: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    json.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}