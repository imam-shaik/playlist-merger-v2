#!/usr/bin/env python3
"""Steps 5-6: Add batch methods and integrate into validate_input_files."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

print(f'Original total lines: {len(lines)}')

# Find end of revalidate_single (line 8167, closing brace)
insert_after = 8167  # 0-based: line 8167 is index 8166

# Step 5: Add batch methods
batch_methods = '''

    // ─── 4-PHASE PIPELINE BATCH METHODS ─────────────────────────────────────

    /// Phase 1 (batch): Analyze all files without modifying anything.
    /// Returns a PipelineReport with FileState for every file.
    pub fn analyze_all(
        &self,
        input_files: &[String],
        cancel_flag: Option<&std::sync::Arc<AtomicBool>>,
    ) -> PipelineReport {
        let mut file_states = Vec::with_capacity(input_files.len());

        for (i, file_path) in input_files.iter().enumerate() {
            // Check cancel
            if let Some(flag) = cancel_flag {
                if flag.load(Ordering::Relaxed) {
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

    /// Phase 2 (batch): Repair all damaged files.
    /// Only files with FileDisposition::Repairable are processed.
    /// Returns the updated PipelineReport.
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
                if flag.load(Ordering::Relaxed) {
                    log::warn!("[REPAIR_DAMAGED] Cancelled");
                    break;
                }
            }

            let state = report.file_states.remove(idx);
            log::info!("[REPAIR_DAMAGED] Repairing: {}", state.original_name);
            let repaired = self.repair_single(state, input_dir);
            // Re-insert at the same index (shifted by removal)
            report.file_states.insert(idx, repaired);
        }

        report
    }

    /// Phase 3 (batch): Revalidate all repaired files.
    /// Only files with RepairStatus::Succeeded are revalidated.
    /// Returns the updated PipelineReport.
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
                if flag.load(Ordering::Relaxed) {
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

    /// Execute the full 4-phase pipeline: Analyze → Repair → Revalidate → Report.
    /// This is the main entry point for the new architecture.
    pub fn run_pipeline(
        &self,
        input_files: &[String],
        input_dir: &std::path::Path,
        cancel_flag: Option<&std::sync::Arc<AtomicBool>>,
    ) -> PipelineReport {
        let pipeline_start = std::time::Instant::now();

        // Phase 1: Analyze
        log::info!("[PIPELINE] Phase 1: Analyzing {} files", input_files.len());
        let mut report = self.analyze_all(input_files, cancel_flag);
        log::info!(
            "[PIPELINE] Phase 1 complete: {} healthy, {} damaged, {} unrepairable",
            report.healthy_count, report.damaged_count, report.quarantined_count
        );

        // Phase 2: Repair
        if report.damaged_count > 0 {
            log::info!("[PIPELINE] Phase 2: Repairing {} damaged files", report.damaged_count);
            report = self.repair_damaged(report, input_dir, cancel_flag);
            let repaired = report.file_states.iter()
                .filter(|s| s.repair_status == RepairStatus::Succeeded)
                .count();
            log::info!("[PIPELINE] Phase 2 complete: {} repaired, {} failed",
                repaired, report.damaged_count - repaired);
        } else {
            log::info!("[PIPELINE] Phase 2: Skipped (no damaged files)");
        }

        // Phase 3: Revalidate
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

        // Update final paths for healthy files
        for state in &mut report.file_states {
            if state.final_path.is_empty() {
                state.final_path = state.original_path.clone();
            }
        }

        let pipeline_ms = pipeline_start.elapsed().as_millis() as f64;
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
'''

lines.insert(insert_after, batch_methods)
print(f'Inserted batch methods after line {insert_after}')

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f'New total lines: {len(lines)}')
print('[DONE] Batch methods and run_pipeline added')
