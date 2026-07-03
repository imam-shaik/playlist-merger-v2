use super::*;

impl MediaValidationEngine {
    pub fn run_pipeline(
        &self,
        input_files: &[String],
        input_dir: &std::path::Path,
        cancel_flag: Option<&std::sync::Arc<AtomicBool>>,
    ) -> PipelineReport {
        let pipeline_start = std::time::Instant::now();

        log::info!("[PIPELINE] Phase 1: Analyzing {} files", input_files.len());
        let mut report = self.analyze_all(input_files, cancel_flag);
        log::info!(
            "[PIPELINE] Phase 1 complete: {} healthy, {} damaged, {} unrepairable",
            report.healthy_count,
            report.damaged_count(),
            report.quarantined_count
        );

        if report.damaged_count() > 0 {
            log::info!("[PIPELINE] Phase 2: Repairing {} damaged files", report.damaged_count());
            report = self.repair_damaged(report, input_dir, cancel_flag);
            let repaired = report.file_states.iter()
                .filter(|s| s.repair_status == RepairStatus::Succeeded)
                .count();
            log::info!("[PIPELINE] Phase 2 complete: {} repaired, {} failed",
                repaired, report.damaged_count() - repaired);
        } else {
            log::info!("[PIPELINE] Phase 2: Skipped (no damaged files)");
        }

        let needs_reval = report.file_states.iter()
            .filter(|s| s.repair_status == RepairStatus::Succeeded && s.repaired_path.is_some())
            .count();
        if needs_reval > 0 {
            log::info!("[PIPELINE] Phase 3: Revalidating {} repaired files", needs_reval);
            report = self.revalidate_repaired(report, cancel_flag);
            let failed_reval = report.file_states.iter()
                .filter(|s| s.revalidation_status == RevalidationStatus::Failed)
                .count();
            log::info!("[PIPELINE] Phase 3 complete: {} failed revalidation", failed_reval);
        } else {
            log::info!("[PIPELINE] Phase 3: Skipped (no repaired files)");
        }

        for state in &mut report.file_states {
            if state.final_path.is_empty() && state.repair_status != RepairStatus::Quarantined {
                state.final_path = state.original_path.clone();
            }
        }

        let pipeline_ms = pipeline_start.elapsed().as_millis() as f64;
        report.total_duration_ms = pipeline_ms;
        log::info!(
            "[PIPELINE] Complete in {:.1}s: {} total, {} healthy, {} repaired, {} quarantined",
            pipeline_ms / 1000.0,
            report.total_count,
            report.healthy_count,
            report.repaired_count,
            report.quarantined_count
        );

        report
    }

    pub fn analyze_all(
        &self,
        input_files: &[String],
        cancel_flag: Option<&std::sync::Arc<AtomicBool>>,
    ) -> PipelineReport {
        let mut file_states = Vec::with_capacity(input_files.len());

        for (i, file_path) in input_files.iter().enumerate() {
            if let Some(flag) = cancel_flag {
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    log::warn!("[ANALYZE_ALL] Cancelled at file {}/{}", i + 1, input_files.len());
                    break;
                }
            }

            log::info!("[ANALYZE_ALL] Analyzing {}/{}: {}", i + 1, input_files.len(), file_path);
            let state = self.analyze_single(i, file_path);
            file_states.push(state);
        }

        PipelineReport::new(file_states)
    }

    pub fn repair_damaged(
        &self,
        mut report: PipelineReport,
        input_dir: &std::path::Path,
        cancel_flag: Option<&std::sync::Arc<AtomicBool>>,
    ) -> PipelineReport {
        let damaged: Vec<usize> = report.needs_repair_indices();

        if damaged.is_empty() {
            log::info!("[REPAIR_DAMAGED] No files need repair");
            return report;
        }

        log::info!("[REPAIR_DAMAGED] Repairing {} damaged files", damaged.len());

        for &idx in &damaged {
            if let Some(flag) = cancel_flag {
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    log::warn!("[REPAIR_DAMAGED] Cancelled");
                    break;
                }
            }

            let state = report.file_states.remove(idx);
            log::info!("[REPAIR_DAMAGED] Repairing: {}", state.original_name);
            let repaired = self.repair_single(state, input_dir);
            report.file_states.insert(idx, repaired);
        }

        report
    }

    pub fn revalidate_repaired(
        &self,
        mut report: PipelineReport,
        cancel_flag: Option<&std::sync::Arc<AtomicBool>>,
    ) -> PipelineReport {
        let repaired_indices: Vec<usize> = report.file_states.iter().enumerate()
            .filter(|(_, s)| s.repair_status == RepairStatus::Succeeded && s.repaired_path.is_some())
            .map(|(i, _)| i)
            .collect();

        if repaired_indices.is_empty() {
            log::info!("[REVALIDATE_REPAIRED] No repaired files to revalidate");
            return report;
        }

        log::info!("[REVALIDATE_REPAIRED] Revalidating {} repaired files", repaired_indices.len());

        for &idx in &repaired_indices {
            if let Some(flag) = cancel_flag {
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    log::warn!("[REVALIDATE_REPAIRED] Cancelled");
                    break;
                }
            }

            let state = report.file_states.remove(idx);
            log::info!("[REVALIDATE_REPAIRED] Revalidating: {}", state.original_name);
            let revalidated = self.revalidate_single(state);
            report.file_states.insert(idx, revalidated);
        }

        report
    }
}

impl PipelineReport {
    pub fn new(file_states: Vec<FileState>) -> Self {
        let total_count = file_states.len();
        let mut healthy_count = 0;
        let mut needs_normalization_count = 0;
        let mut _repairable_count = 0;
        let mut _unrepairable_count = 0;
        let mut repaired_count = 0;
        let mut failed_repair_count = 0;
        let mut quarantined_count = 0;

        let mut untouched_count = 0;
        let mut compatibility_remux_count = 0;
        let mut repair_remux_count = 0;
        let mut subtitle_repair_count = 0;
        let mut reencode_count = 0;

        for state in &file_states {
            match &state.disposition {
                FileDisposition::Healthy => healthy_count += 1,
                FileDisposition::NeedsNormalization => needs_normalization_count += 1,
                FileDisposition::Repairable(_) => _repairable_count += 1,
                FileDisposition::Unrepairable(_) => _unrepairable_count += 1,
            }

            match state.repair_status {
                RepairStatus::Succeeded => repaired_count += 1,
                RepairStatus::Failed | RepairStatus::Quarantined => {
                    failed_repair_count += 1;
                    if state.final_path.is_empty() {
                        quarantined_count += 1;
                    }
                }
                RepairStatus::Skipped => {}
            }

            match (&state.disposition, &state.repair_status, &state.fix_applied) {
                (FileDisposition::Healthy, RepairStatus::Skipped, None) => {
                    untouched_count += 1;
                }
                (FileDisposition::NeedsNormalization, RepairStatus::Succeeded, Some(FixType::ContainerRemux)) => {
                    compatibility_remux_count += 1;
                }
                (FileDisposition::Repairable(_), RepairStatus::Succeeded, Some(FixType::ContainerRemux))
                    if state.repair_reason.as_ref().map(|r| r.contains("subtitle")).unwrap_or(false) => {
                    subtitle_repair_count += 1;
                }
                (FileDisposition::Repairable(_), RepairStatus::Succeeded, Some(FixType::TimestampRepair | FixType::ContainerRemux)) => {
                    repair_remux_count += 1;
                }
                (FileDisposition::Repairable(_), RepairStatus::Succeeded, Some(FixType::FullReencode)) => {
                    reencode_count += 1;
                }
                (_, RepairStatus::Quarantined, _) => {}
                (FileDisposition::Repairable(_), RepairStatus::Failed, _) => {}
                _ => {}
            }
        }

        Self {
            file_states,
            total_count,
            healthy_count,
            needs_normalization_count,
            repaired_count,
            failed_repair_count,
            quarantined_count,
            untouched_count,
            compatibility_remux_count,
            repair_remux_count,
            subtitle_repair_count,
            reencode_count,
            total_analysis_duration_secs: 0.0,
            total_repair_duration_secs: 0.0,
            total_revalidation_duration_secs: 0.0,
            total_duration_ms: 0.0,
        }
    }

    pub fn needs_repair_indices(&self) -> Vec<usize> {
        self.file_states.iter()
            .enumerate()
            .filter(|(_, s)| matches!(s.disposition, FileDisposition::Repairable(_)))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn damaged_count(&self) -> usize {
        self.file_states.iter()
            .filter(|s| matches!(s.disposition, FileDisposition::Repairable(_)))
            .count()
    }

    #[allow(dead_code)]
    pub fn all_pass(&self) -> bool {
        self.quarantined_count == 0 && self.failed_repair_count == 0
    }

    pub fn repaired_files_as_tuples(&self) -> Vec<(usize, String, FixType)> {
        self.file_states.iter()
            .filter(|s| s.repair_status == RepairStatus::Succeeded)
            .filter_map(|s| {
                s.repaired_path.as_ref().map(|path| {
                    (s.file_index, path.clone(), s.fix_applied.as_ref().cloned().unwrap_or(FixType::None))
                })
            })
            .collect()
    }

    pub fn quarantined_file_names(&self) -> Vec<(usize, String)> {
        self.file_states.iter()
            .filter(|s| s.final_path.is_empty())
            .map(|s| (s.file_index, s.original_name.clone()))
            .collect()
    }

    pub fn timestamp_damage_count(&self) -> usize {
        self.file_states.iter()
            .filter(|s| matches!(s.damage_classification, DamageClassification::TimestampDamage))
            .count()
    }

    pub fn container_damage_count(&self) -> usize {
        self.file_states.iter()
            .filter(|s| matches!(s.damage_classification, DamageClassification::ContainerDamage))
            .count()
    }

    pub fn subtitle_damage_count(&self) -> usize {
        self.file_states.iter()
            .filter(|s| matches!(s.damage_classification, DamageClassification::SubtitleDamage))
            .count()
    }

    pub fn to_media_validation_report(&self, total_duration_ms: u128) -> MediaValidationReport {
        let mut report = MediaValidationReport {
            total_files: self.total_count,
            clean_count: self.healthy_count,
            fixed_count: self.repaired_count,
            quarantined_count: self.quarantined_count,
            quarantined_files: self.quarantined_file_names(),
            repaired_files: self.repaired_files_as_tuples(),
            total_duration_ms,
            timestamp_damage_count: self.timestamp_damage_count(),
            container_damage_count: self.container_damage_count(),
            subtitle_damage_count: self.subtitle_damage_count(),
            reencode_count: self.reencode_count,
            file_results: Vec::new(),
        };

        for state in &self.file_states {
            let status = match state.repair_status {
                RepairStatus::Succeeded => {
                    if matches!(state.fix_applied, Some(FixType::FullReencode)) {
                        ValidationStatus::RepairedReencode
                    } else {
                        ValidationStatus::RepairedRemux
                    }
                }
                RepairStatus::Quarantined => ValidationStatus::Quarantined,
                RepairStatus::Skipped | RepairStatus::Failed => ValidationStatus::Clean,
            };

            report.file_results.push(MediaValidationResult {
                file_index: state.file_index,
                file_path: state.original_path.clone(),
                status,
                damage_classification: Some(state.damage_classification.clone()),
                confidence: state.confidence,
                validation_reason: state.analysis_reason.clone(),
                repaired_path: state.repaired_path.clone(),
                fix_applied: state.fix_applied.clone(),
                analysis_duration_ms: state.analysis_duration_ms,
                repair_duration_ms: state.repair_duration_ms,
            });
        }

        report
    }
}

#[allow(dead_code)]
impl FileState {
    pub fn is_healthy(&self) -> bool {
        matches!(self.disposition, FileDisposition::Healthy)
    }

    pub fn is_damaged(&self) -> bool {
        matches!(self.disposition, FileDisposition::Repairable(_) | FileDisposition::Unrepairable(_))
    }

    pub fn is_quarantined(&self) -> bool {
        self.final_path.is_empty()
    }

    pub fn needs_repair(&self) -> bool {
        matches!(self.disposition, FileDisposition::Repairable(_))
    }

    pub fn needs_normalization(&self) -> bool {
        matches!(self.disposition, FileDisposition::NeedsNormalization)
    }

    pub fn was_repaired(&self) -> bool {
        self.repair_status == RepairStatus::Succeeded
    }

    pub fn will_merge(&self) -> bool {
        !self.final_path.is_empty()
    }
}

impl MediaValidationReport {
    pub fn summary(&self) -> String {
        format!(
            "MediaValidationReport {{ total: {}, clean: {}, fixed: {}, quarantined: {}, reencode: {} }}",
            self.total_files,
            self.clean_count,
            self.fixed_count,
            self.quarantined_count,
            self.reencode_count
        )
    }

    pub fn all_pass(&self) -> bool {
        self.quarantined_count == 0
    }
}

impl MediaValidationResult {
    pub fn is_quarantined(&self) -> bool {
        matches!(self.status, ValidationStatus::Quarantined)
    }

    pub fn is_fixed(&self) -> bool {
        matches!(self.status, ValidationStatus::RepairedRemux | ValidationStatus::RepairedReencode)
    }

    pub fn effective_path(&self) -> &str {
        self.repaired_path.as_deref().unwrap_or(&self.file_path)
    }
}