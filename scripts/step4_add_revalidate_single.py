#!/usr/bin/env python3
"""Step 4: Add revalidate_single method to MediaValidationEngine."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

print(f'Original total lines: {len(lines)}')

# Find the end of repair_reencode_only (line 8100)
insert_after = None
for i, line in enumerate(lines):
    if 'fn repair_reencode_only' in line and i > 7500:
        depth = 0
        found = False
        for j in range(i, len(lines)):
            for ch in lines[j]:
                if ch == '{': depth += 1; found = True
                elif ch == '}': depth -= 1
            if found and depth == 0:
                insert_after = j
                break
        break

if insert_after is None:
    print('[ERROR] Could not find repair_reencode_only end')
    exit(1)

print(f'Inserting revalidate_single after line {insert_after + 1}')

revalidate_single = '''
    /// Phase 3: Revalidate a repaired file to ensure it's merge-ready.
    ///
    /// **GUARANTEE**: Only reads the file, never modifies it.
    /// Returns the updated FileState with revalidation results populated.
    pub fn revalidate_single(&self, mut state: FileState) -> FileState {
        let reval_start = std::time::Instant::now();
        let file_name = state.original_name.clone();

        // Determine which file to validate
        let path_to_validate = match &state.repaired_path {
            Some(p) => p.clone(),
            None => state.final_path.clone(),
        };

        // If no repair was done or repair failed, skip revalidation
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

        // Run the full validation suite on the repaired file
        let validation_result = self.validate_single_impl(
            state.file_index,
            &path_to_validate,
            state.original_duration_secs,
        );

        let reval_ms = reval_start.elapsed().as_millis() as f64;

        // Determine revalidation outcome based on validation status
        match validation_result.status {
            ValidationStatus::Clean => {
                log::info!("[REVALIDATE_SINGLE] {} - PASSED ({:.1}ms)", file_name, reval_ms);
                state.revalidation_status = RevalidationStatus::Passed;
                state.final_path = path_to_validate;
            }
            ValidationStatus::RepairedRemux | ValidationStatus::RepairedReencode => {
                // File still has issues but they were repaired during revalidation
                log::warn!("[REVALIDATE_SINGLE] {} - Repaired again during revalidation ({:.1}ms)", file_name, reval_ms);
                state.revalidation_status = RevalidationStatus::Passed;
                // Update the final path to the re-repaired file
                if let Some(repaired) = &validation_result.repaired_path {
                    state.final_path = repaired.clone();
                    state.repaired_path = Some(repaired.clone());
                }
            }
            ValidationStatus::Quarantined => {
                log::warn!("[REVALIDATE_SINGLE] {} - FAILED, quarantined ({:.1}ms)", file_name, reval_ms);
                state.revalidation_status = RevalidationStatus::Failed;
                state.disposition = FileDisposition::Unrepairable(state.damage_classification.clone());
            }
            _ => {
                // Other statuses (shouldn't happen for repaired files)
                log::warn!("[REVALIDATE_SINGLE] {} - Unexpected status {:?} ({:.1}ms)", file_name, validation_result.status, reval_ms);
                state.revalidation_status = RevalidationStatus::Failed;
            }
        }

        state.revalidation_duration_ms = reval_ms;
        state
    }
'''

lines.insert(insert_after + 1, revalidate_single)
print(f'Inserted revalidate_single')

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f'New total lines: {len(lines)}')
print('[DONE] revalidate_single added')
