use super::*;

impl MediaValidationEngine {
    pub fn revalidate_single(&self, mut state: FileState) -> FileState {
        let reval_start = std::time::Instant::now();
        let file_name = state.original_name.clone();

        let path_to_validate = match &state.repaired_path {
            Some(p) => p.clone(),
            None => state.final_path.clone(),
        };

        if state.repair_status == RepairStatus::Skipped
            || state.repair_status == RepairStatus::Quarantined
            || path_to_validate == state.original_path
        {
            log::info!("[REVALIDATE_SINGLE] {} - Skipped (repair status: {:?})", file_name, state.repair_status);
            state.revalidation_status = RevalidationStatus::NotNeeded;
            state.revalidation_duration_ms = reval_start.elapsed().as_millis() as f64;
            return state;
        }

        log::info!("[REVALIDATE_SINGLE] {} - Validating repaired file", file_name);

        let validation_result = self.validate_single_impl(
            state.file_index,
            &path_to_validate,
            state.original_duration_secs,
        );

        let reval_ms = reval_start.elapsed().as_millis() as f64;

        match validation_result.status {
            ValidationStatus::Clean => {
                log::info!("[REVALIDATE_SINGLE] {} - PASSED ({:.1}ms)", file_name, reval_ms);
                state.revalidation_status = RevalidationStatus::Passed;
                state.final_path = path_to_validate;
            }
            ValidationStatus::RepairedRemux | ValidationStatus::RepairedReencode => {
                log::warn!("[REVALIDATE_SINGLE] {} - Repaired again during revalidation ({:.1}ms)", file_name, reval_ms);
                state.revalidation_status = RevalidationStatus::Passed;
                if let Some(repaired) = &validation_result.repaired_path {
                    state.final_path = repaired.clone();
                    state.repaired_path = Some(repaired.clone());
                }
            }
            ValidationStatus::Quarantined => {
                log::warn!("[REVALIDATE_SINGLE] {} - FAILED, quarantined ({:.1}ms)", file_name, reval_ms);
                // P0-1: Properly quarantine the file when revalidation fails.
                // Clear repaired_path and final_path so apply_validation_results()
                // will remove this file from the merge input list.
                state.repair_status = RepairStatus::Quarantined;
                state.revalidation_status = RevalidationStatus::Failed;
                state.disposition = FileDisposition::Unrepairable(state.damage_classification.clone());
                state.repaired_path = None;
                state.final_path.clear();
                state.repair_trace.push(RepairTraceEntry {
                    function: "Revalidation".to_string(),
                    outcome: "quarantined".to_string(),
                    output_path: None,
                    details: format!("Revalidation failed in {:.1}ms, file quarantined", reval_ms),
                    effectiveness: None,
                });
            }
        }

        state.revalidation_duration_ms = reval_ms;
        state
    }

    fn validate_single_impl(&self, file_index: usize, file_path: &str, _original_duration_secs: f64) -> MediaValidationResult {
        // P0-2 FIX: Run full Phase 1 analysis instead of shallow container check.
        // This ensures repaired files are validated for PTS, DTS, time_base,
        // subtitle integrity, bitstream, packet, frame, and decode status.
        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return MediaValidationResult {
                file_index,
                file_path: file_path.to_string(),
                status: ValidationStatus::Quarantined,
                damage_classification: Some(DamageClassification::Unsupported),
                confidence: 0.0,
                validation_reason: "repaired file does not exist".to_string(),
                repaired_path: None,
                fix_applied: None,
                analysis_duration_ms: 0.0,
                repair_duration_ms: 0.0,
            };
        }

        let reval_start = std::time::Instant::now();
        let file_state = self.analyze_single(file_index, file_path);
        let reval_ms = reval_start.elapsed().as_millis() as f64;

        match &file_state.disposition {
            FileDisposition::Healthy => {
                log::info!("[REVALIDATE] Full analysis PASSED for {}", file_path);
                MediaValidationResult {
                    file_index,
                    file_path: file_path.to_string(),
                    status: ValidationStatus::Clean,
                    damage_classification: None,
                    confidence: 1.0,
                    validation_reason: format!("full revalidation passed ({:.0}ms)", reval_ms),
                    repaired_path: None,
                    fix_applied: None,
                    analysis_duration_ms: reval_ms,
                    repair_duration_ms: 0.0,
                }
            }
            FileDisposition::Repairable(damage) => {
                log::warn!("[REVALIDATE] Full analysis found remaining damage in {}: {:?}", file_path, damage);
                MediaValidationResult {
                    file_index,
                    file_path: file_path.to_string(),
                    status: ValidationStatus::Quarantined,
                    damage_classification: Some(damage.clone()),
                    confidence: 0.5,
                    validation_reason: format!("full revalidation found remaining damage: {:?}", damage),
                    repaired_path: None,
                    fix_applied: None,
                    analysis_duration_ms: reval_ms,
                    repair_duration_ms: 0.0,
                }
            }
            FileDisposition::NeedsNormalization => {
                // P0-2 FIX: A repaired file that still needs normalization means the
                // repair was insufficient for full compatibility. Flag as warning so
                // the caller knows normalization will still be needed at merge time.
                log::warn!("[REVALIDATE] Full analysis: repaired file still needs normalization {} (will be normalized at merge time)", file_path);
                MediaValidationResult {
                    file_index,
                    file_path: file_path.to_string(),
                    status: ValidationStatus::Clean,
                    damage_classification: None,
                    confidence: 0.8,
                    validation_reason: format!("revalidation passed but normalization still needed ({:.0}ms)", reval_ms),
                    repaired_path: None,
                    fix_applied: None,
                    analysis_duration_ms: reval_ms,
                    repair_duration_ms: 0.0,
                }
            }
            FileDisposition::Unrepairable(damage) => {
                log::error!("[REVALIDATE] Full analysis found unrepairable damage in {}: {:?}", file_path, damage);
                MediaValidationResult {
                    file_index,
                    file_path: file_path.to_string(),
                    status: ValidationStatus::Quarantined,
                    damage_classification: Some(damage.clone()),
                    confidence: 0.0,
                    validation_reason: format!("full revalidation found unrepairable damage: {:?}", damage),
                    repaired_path: None,
                    fix_applied: None,
                    analysis_duration_ms: reval_ms,
                    repair_duration_ms: 0.0,
                }
            }
        }
    }
}