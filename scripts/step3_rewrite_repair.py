#!/usr/bin/env python3
"""Rewrite all three repair methods with correct API signatures."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

print(f'Original total lines: {len(lines)}')

# Step 1: Remove old repair methods (lines 7872 to 8132)
old_start = 7871  # 0-based index for line 7872
old_end = 8131    # 0-based index for line 8132

# Verify we're removing the right lines
assert '/// Phase 2: Attempt repair on a single damaged file.' in lines[old_start], f'Unexpected start: {lines[old_start][:80]}'
assert lines[old_end].strip() == 'state', f'Unexpected end: {lines[old_end].strip()[:80]}'

del lines[old_start:old_end + 1]
print(f'Removed old repair methods: {old_end - old_start + 1} lines')

# Step 2: Insert corrected repair methods
new_methods = '''
    /// Phase 2: Attempt repair on a single damaged file.
    ///
    /// **GUARANTEE**: Only modifies the file if a repair succeeds.
    /// If all repair attempts fail, the file is quarantined (original preserved).
    ///
    /// Returns the updated FileState with repair attempt history recorded.
    pub fn repair_single(&self, mut state: FileState, input_dir: &std::path::Path) -> FileState {
        let repair_start = std::time::Instant::now();
        let file_name = state.original_name.clone();
        let original_path = state.original_path.clone();
        let file_index = state.file_index;

        match &state.disposition {
            FileDisposition::Healthy | FileDisposition::NeedsNormalization | FileDisposition::Unrepairable(_) => {
                state.repair_status = match &state.disposition {
                    FileDisposition::Healthy => RepairStatus::Skipped,
                    FileDisposition::NeedsNormalization => RepairStatus::Skipped,
                    FileDisposition::Unrepairable(_) => RepairStatus::Quarantined,
                    _ => RepairStatus::Skipped,
                };
                state.repair_duration_ms = Some(repair_start.elapsed().as_millis() as f64);
                return state;
            }
            FileDisposition::Repairable(damage) => {
                let damage = damage.clone();
                match damage {
                    DamageClassification::SubtitleDamage => {
                        // Phase A: Subtitle-specific repair
                        log::info!("[REPAIR_SINGLE] {} - Phase A: Subtitle repair attempt", file_name);
                        let phase_start = std::time::Instant::now();

                        match self.try_fix_subtitle_remux(file_index, &original_path) {
                            Some(fixed_path) => {
                                // Revalidate the repaired file
                                match self.check_critical_post_repair(&fixed_path) {
                                    Ok(()) => {
                                        match self.check_stream_identity(&original_path, &fixed_path) {
                                            Ok(()) => {
                                                let phase_ms = phase_start.elapsed().as_millis() as f64;
                                                log::info!("[REPAIR_SINGLE] {} - Subtitle repair PASSED ({:.1}ms)", file_name, phase_ms);
                                                state.repair_status = RepairStatus::Succeeded;
                                                state.repair_method = Some("SubtitleRemux".to_string());
                                                state.current_path = Some(fixed_path.clone());
                                                state.final_path = Some(fixed_path.clone());
                                                state.repair_attempts.push(RepairTraceEntry {
                                                    method: "SubtitleRemux".to_string(),
                                                    status: RepairAttemptStatus::Succeeded,
                                                    output_path: Some(fixed_path),
                                                    duration_ms: phase_ms,
                                                    error: None,
                                                });
                                                state.repair_duration_ms = Some(repair_start.elapsed().as_millis() as f64);
                                                return state;
                                            }
                                            Err(e) => {
                                                log::warn!("[REPAIR_SINGLE] {} - Subtitle repair passed but stream identity failed: {}", file_name, e);
                                                let _ = std::fs::remove_file(&fixed_path);
                                                state.repair_attempts.push(RepairTraceEntry {
                                                    method: "SubtitleRemux".to_string(),
                                                    status: RepairAttemptStatus::Failed,
                                                    output_path: None,
                                                    duration_ms: phase_start.elapsed().as_millis() as f64,
                                                    error: Some(format!("Stream identity check failed: {}", e)),
                                                });
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("[REPAIR_SINGLE] {} - Subtitle repair passed but critical post-repair failed: {}", file_name, e);
                                        let _ = std::fs::remove_file(&fixed_path);
                                        state.repair_attempts.push(RepairTraceEntry {
                                            method: "SubtitleRemux".to_string(),
                                            status: RepairAttemptStatus::Failed,
                                            output_path: None,
                                            duration_ms: phase_start.elapsed().as_millis() as f64,
                                            error: Some(format!("Critical post-repair check failed: {}", e)),
                                        });
                                    }
                                }
                                // Subtitle repair failed -- fall through to Phase B
                                log::info!("[REPAIR_SINGLE] {} - Subtitle repair failed, escalating to Phase B", file_name);
                            }
                            None => {
                                state.repair_attempts.push(RepairTraceEntry {
                                    method: "SubtitleRemux".to_string(),
                                    status: RepairAttemptStatus::Failed,
                                    output_path: None,
                                    duration_ms: phase_start.elapsed().as_millis() as f64,
                                    error: Some("try_fix_subtitle_remux returned None".to_string()),
                                });
                            }
                        }
                        // Fall through to Phase B: Timestamp repair
                        self.repair_timestamp_or_reencode(state, file_index, &original_path, input_dir)
                    }
                    DamageClassification::TimestampDamage | DamageClassification::ContainerDamage => {
                        self.repair_timestamp_or_reencode(state, file_index, &original_path, input_dir)
                    }
                    DamageClassification::Healthy | DamageClassification::Unsupported => {
                        state.repair_status = RepairStatus::Skipped;
                        state.repair_duration_ms = Some(repair_start.elapsed().as_millis() as f64);
                        state
                    }
                }
            }
        }
    }

    /// Phase B: Timestamp/Container repair via remux.
    fn repair_timestamp_or_reencode(&self, mut state: FileState, file_index: usize, original_path: &str, _input_dir: &std::path::Path) -> FileState {
        let file_name = state.original_name.clone();
        let phase_start = std::time::Instant::now();

        log::info!("[REPAIR_SINGLE] {} - Phase B: Timestamp/Container repair attempt", file_name);

        match self.try_fix_timestamp_repair(file_index, original_path) {
            Some(fixed_path) => {
                match self.check_critical_post_repair(&fixed_path) {
                    Ok(()) => {
                        match self.check_stream_identity(original_path, &fixed_path) {
                            Ok(()) => {
                                let phase_ms = phase_start.elapsed().as_millis() as f64;
                                log::info!("[REPAIR_SINGLE] {} - Timestamp repair PASSED ({:.1}ms)", file_name, phase_ms);
                                state.repair_status = RepairStatus::Succeeded;
                                state.repair_method = Some("TimestampRemux".to_string());
                                state.current_path = Some(fixed_path.clone());
                                state.final_path = Some(fixed_path.clone());
                                state.repair_attempts.push(RepairTraceEntry {
                                    method: "TimestampRemux".to_string(),
                                    status: RepairAttemptStatus::Succeeded,
                                    output_path: Some(fixed_path),
                                    duration_ms: phase_ms,
                                    error: None,
                                });
                                state.repair_duration_ms = Some(phase_ms);
                                return state;
                            }
                            Err(e) => {
                                let phase_ms = phase_start.elapsed().as_millis() as f64;
                                log::warn!("[REPAIR_SINGLE] {} - Timestamp repair passed but stream identity failed: {}", file_name, e);
                                let _ = std::fs::remove_file(&fixed_path);
                                state.repair_attempts.push(RepairTraceEntry {
                                    method: "TimestampRemux".to_string(),
                                    status: RepairAttemptStatus::Failed,
                                    output_path: None,
                                    duration_ms: phase_ms,
                                    error: Some(format!("Stream identity check failed: {}", e)),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let phase_ms = phase_start.elapsed().as_millis() as f64;
                        log::warn!("[REPAIR_SINGLE] {} - Timestamp repair passed but critical post-repair failed: {}", file_name, e);
                        let _ = std::fs::remove_file(&fixed_path);
                        state.repair_attempts.push(RepairTraceEntry {
                            method: "TimestampRemux".to_string(),
                            status: RepairAttemptStatus::Failed,
                            output_path: None,
                            duration_ms: phase_ms,
                            error: Some(format!("Critical post-repair check failed: {}", e)),
                        });
                    }
                }
            }
            None => {
                let phase_ms = phase_start.elapsed().as_millis() as f64;
                state.repair_attempts.push(RepairTraceEntry {
                    method: "TimestampRemux".to_string(),
                    status: RepairAttemptStatus::Failed,
                    output_path: None,
                    duration_ms: phase_ms,
                    error: Some("try_fix_timestamp_repair returned None".to_string()),
                });
            }
        }

        log::info!("[REPAIR_SINGLE] {} - Timestamp repair failed, escalating to Phase C (re-encode)", file_name);
        self.repair_reencode_only(state, file_index, original_path, input_dir)
    }

    /// Phase C: Re-encode (last resort).
    fn repair_reencode_only(&self, mut state: FileState, file_index: usize, original_path: &str, _input_dir: &std::path::Path) -> FileState {
        let file_name = state.original_name.clone();
        let repair_start = std::time::Instant::now();

        log::info!("[REPAIR_SINGLE] {} - Phase C: Re-encode attempt", file_name);

        match self.try_fix_reencode(file_index, original_path) {
            Some(fixed_path) => {
                match self.check_critical_post_repair(&fixed_path) {
                    Ok(()) => {
                        match self.check_stream_identity(original_path, &fixed_path) {
                            Ok(()) => {
                                let phase_ms = repair_start.elapsed().as_millis() as f64;
                                log::info!("[REPAIR_SINGLE] {} - Re-encode PASSED ({:.1}ms)", file_name, phase_ms);
                                state.repair_status = RepairStatus::Succeeded;
                                state.repair_method = Some("Reencode".to_string());
                                state.current_path = Some(fixed_path.clone());
                                state.final_path = Some(fixed_path.clone());
                                state.repair_attempts.push(RepairTraceEntry {
                                    method: "Reencode".to_string(),
                                    status: RepairAttemptStatus::Succeeded,
                                    output_path: Some(fixed_path),
                                    duration_ms: phase_ms,
                                    error: None,
                                });
                                state.repair_duration_ms = Some(phase_ms);
                                return state;
                            }
                            Err(e) => {
                                let phase_ms = repair_start.elapsed().as_millis() as f64;
                                log::warn!("[REPAIR_SINGLE] {} - Re-encode passed but stream identity failed: {}", file_name, e);
                                let _ = std::fs::remove_file(&fixed_path);
                                state.repair_attempts.push(RepairTraceEntry {
                                    method: "Reencode".to_string(),
                                    status: RepairAttemptStatus::Failed,
                                    output_path: None,
                                    duration_ms: phase_ms,
                                    error: Some(format!("Stream identity check failed: {}", e)),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let phase_ms = repair_start.elapsed().as_millis() as f64;
                        log::warn!("[REPAIR_SINGLE] {} - Re-encode passed but critical post-repair failed: {}", file_name, e);
                        let _ = std::fs::remove_file(&fixed_path);
                        state.repair_attempts.push(RepairTraceEntry {
                            method: "Reencode".to_string(),
                            status: RepairAttemptStatus::Failed,
                            output_path: None,
                            duration_ms: phase_ms,
                            error: Some(format!("Critical post-repair check failed: {}", e)),
                        });
                    }
                }
            }
            None => {
                let phase_ms = repair_start.elapsed().as_millis() as f64;
                state.repair_attempts.push(RepairTraceEntry {
                    method: "Reencode".to_string(),
                    status: RepairAttemptStatus::Failed,
                    output_path: None,
                    duration_ms: phase_ms,
                    error: Some("try_fix_reencode returned None".to_string()),
                });
            }
        }

        log::warn!("[REPAIR_SINGLE] {} - All repair attempts failed, quarantining", file_name);
        state.repair_status = RepairStatus::Quarantined;
        state.disposition = FileDisposition::Unrepairable(DamageClassification::Unsupported);
        state.repair_duration_ms = Some(repair_start.elapsed().as_millis() as f64);
        state
    }
'''

lines.insert(old_start, new_methods)
print(f'Inserted corrected repair methods ({len(new_methods.splitlines())} lines)')

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f'New total lines: {len(lines)}')
print('[DONE] Corrected repair methods inserted')
