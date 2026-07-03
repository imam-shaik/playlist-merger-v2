#!/usr/bin/env python3
"""Rewrite all three repair methods with correct API signatures and field names."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

print(f'Original total lines: {len(lines)}')

# Step 1: Find and remove old repair methods (lines 7872 to 8132)
old_start = 7871  # 0-based for line 7872
old_end = 8133    # 0-based for line 8134 (closing brace of impl block)

# Verify
print(f'Line 7872: {lines[old_start].rstrip()[:80]}')
print(f'Line 8134: {lines[old_end].rstrip()[:80]}')

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
    pub fn repair_single(&self, mut state: FileState, _input_dir: &std::path::Path) -> FileState {
        let repair_start = std::time::Instant::now();
        let file_name = state.original_name.clone();
        let original_path = state.original_path.clone();
        let file_index = state.file_index;
        let original_size = std::fs::metadata(&original_path).map(|m| m.len()).unwrap_or(0);

        match &state.disposition {
            FileDisposition::Healthy | FileDisposition::NeedsNormalization | FileDisposition::Unrepairable(_) => {
                state.repair_status = match &state.disposition {
                    FileDisposition::Healthy => RepairStatus::Skipped,
                    FileDisposition::NeedsNormalization => RepairStatus::Skipped,
                    FileDisposition::Unrepairable(_) => RepairStatus::Quarantined,
                    _ => RepairStatus::Skipped,
                };
                state.repair_duration_ms = repair_start.elapsed().as_millis() as f64;
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
                                let post_fix = self.check_critical_post_repair(&fixed_path, original_size);
                                let orig_identity = self.get_stream_identity(&original_path);
                                let rep_identity = self.get_stream_identity(&fixed_path);
                                let stream_issues = self.check_stream_identity(&orig_identity, &rep_identity);

                                if post_fix.is_empty() && stream_issues.is_empty() {
                                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                                    log::info!("[REPAIR_SINGLE] {} - Subtitle repair PASSED ({:.1}ms)", file_name, phase_ms);
                                    state.repair_status = RepairStatus::Succeeded;
                                    state.fix_applied = Some(FixType::ContainerRemux);
                                    state.repaired_path = Some(fixed_path.clone());
                                    state.final_path = fixed_path.clone();
                                    state.repair_trace.push(RepairTraceEntry {
                                        function: "SubtitleRemux".to_string(),
                                        outcome: "succeeded".to_string(),
                                        output_path: Some(fixed_path),
                                        details: format!("Subtitle repair passed in {:.1}ms", phase_ms),
                                        effectiveness: None,
                                    });
                                    state.repair_duration_ms = repair_start.elapsed().as_millis() as f64;
                                    return state;
                                } else {
                                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                                    let mut issues = post_fix;
                                    issues.extend(stream_issues);
                                    log::warn!("[REPAIR_SINGLE] {} - Subtitle repair passed but revalidation failed: {:?}", file_name, issues);
                                    let _ = std::fs::remove_file(&fixed_path);
                                    state.repair_trace.push(RepairTraceEntry {
                                        function: "SubtitleRemux".to_string(),
                                        outcome: "failed".to_string(),
                                        output_path: None,
                                        details: format!("Revalidation failed: {}", issues.join("; ")),
                                        effectiveness: None,
                                    });
                                    log::info!("[REPAIR_SINGLE] {} - Subtitle repair failed, escalating to Phase B", file_name);
                                }
                            }
                            None => {
                                state.repair_trace.push(RepairTraceEntry {
                                    function: "SubtitleRemux".to_string(),
                                    outcome: "failed".to_string(),
                                    output_path: None,
                                    details: "try_fix_subtitle_remux returned None".to_string(),
                                    effectiveness: None,
                                });
                            }
                        }
                        // Fall through to Phase B: Timestamp repair
                        self.repair_timestamp_or_reencode(state, file_index, &original_path, original_size)
                    }
                    DamageClassification::TimestampDamage
                    | DamageClassification::ContainerDamage
                    | DamageClassification::NeedsReencode
                    | DamageClassification::VideoDecodeFailure
                    | DamageClassification::BitstreamCorruption
                    | DamageClassification::PacketCorruption
                    | DamageClassification::VideoFrameCorruption => {
                        self.repair_timestamp_or_reencode(state, file_index, &original_path, original_size)
                    }
                    DamageClassification::Healthy | DamageClassification::Unsupported => {
                        state.repair_status = RepairStatus::Skipped;
                        state.repair_duration_ms = repair_start.elapsed().as_millis() as f64;
                        state
                    }
                }
            }
        }
    }

    /// Phase B: Timestamp/Container repair via remux.
    fn repair_timestamp_or_reencode(&self, mut state: FileState, file_index: usize, original_path: &str, original_size: u64) -> FileState {
        let file_name = state.original_name.clone();
        let phase_start = std::time::Instant::now();

        log::info!("[REPAIR_SINGLE] {} - Phase B: Timestamp/Container repair attempt", file_name);

        match self.try_fix_timestamp_repair(file_index, original_path) {
            Some(fixed_path) => {
                let post_fix = self.check_critical_post_repair(&fixed_path, original_size);
                let orig_identity = self.get_stream_identity(original_path);
                let rep_identity = self.get_stream_identity(&fixed_path);
                let stream_issues = self.check_stream_identity(&orig_identity, &rep_identity);

                if post_fix.is_empty() && stream_issues.is_empty() {
                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                    log::info!("[REPAIR_SINGLE] {} - Timestamp repair PASSED ({:.1}ms)", file_name, phase_ms);
                    state.repair_status = RepairStatus::Succeeded;
                    state.fix_applied = Some(FixType::TimestampRepair);
                    state.repaired_path = Some(fixed_path.clone());
                    state.final_path = fixed_path.clone();
                    state.repair_trace.push(RepairTraceEntry {
                        function: "TimestampRemux".to_string(),
                        outcome: "succeeded".to_string(),
                        output_path: Some(fixed_path),
                        details: format!("Timestamp repair passed in {:.1}ms", phase_ms),
                        effectiveness: None,
                    });
                    state.repair_duration_ms = repair_start.elapsed().as_millis() as f64;
                    return state;
                } else {
                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                    let mut issues = post_fix;
                    issues.extend(stream_issues);
                    log::warn!("[REPAIR_SINGLE] {} - Timestamp repair passed but revalidation failed: {:?}", file_name, issues);
                    let _ = std::fs::remove_file(&fixed_path);
                    state.repair_trace.push(RepairTraceEntry {
                        function: "TimestampRemux".to_string(),
                        outcome: "failed".to_string(),
                        output_path: None,
                        details: format!("Revalidation failed: {}", issues.join("; ")),
                        effectiveness: None,
                    });
                }
            }
            None => {
                let phase_ms = phase_start.elapsed().as_millis() as f64;
                state.repair_trace.push(RepairTraceEntry {
                    function: "TimestampRemux".to_string(),
                    outcome: "failed".to_string(),
                    output_path: None,
                    details: "try_fix_timestamp_repair returned None".to_string(),
                    effectiveness: None,
                });
            }
        }

        log::info!("[REPAIR_SINGLE] {} - Timestamp repair failed, escalating to Phase C (re-encode)", file_name);
        self.repair_reencode_only(state, file_index, original_path, original_size)
    }

    /// Phase C: Re-encode (last resort).
    fn repair_reencode_only(&self, mut state: FileState, file_index: usize, original_path: &str, original_size: u64) -> FileState {
        let file_name = state.original_name.clone();
        let phase_start = std::time::Instant::now();

        log::info!("[REPAIR_SINGLE] {} - Phase C: Re-encode attempt", file_name);

        match self.try_fix_reencode(file_index, original_path) {
            Some(fixed_path) => {
                let post_fix = self.check_critical_post_repair(&fixed_path, original_size);
                let orig_identity = self.get_stream_identity(original_path);
                let rep_identity = self.get_stream_identity(&fixed_path);
                let stream_issues = self.check_stream_identity(&orig_identity, &rep_identity);

                if post_fix.is_empty() && stream_issues.is_empty() {
                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                    log::info!("[REPAIR_SINGLE] {} - Re-encode PASSED ({:.1}ms)", file_name, phase_ms);
                    state.repair_status = RepairStatus::Succeeded;
                    state.fix_applied = Some(FixType::FullReencode);
                    state.repaired_path = Some(fixed_path.clone());
                    state.final_path = fixed_path.clone();
                    state.repair_trace.push(RepairTraceEntry {
                        function: "Reencode".to_string(),
                        outcome: "succeeded".to_string(),
                        output_path: Some(fixed_path),
                        details: format!("Re-encode passed in {:.1}ms", phase_ms),
                        effectiveness: None,
                    });
                    state.repair_duration_ms = repair_start.elapsed().as_millis() as f64;
                    return state;
                } else {
                    let phase_ms = phase_start.elapsed().as_millis() as f64;
                    let mut issues = post_fix;
                    issues.extend(stream_issues);
                    log::warn!("[REPAIR_SINGLE] {} - Re-encode passed but revalidation failed: {:?}", file_name, issues);
                    let _ = std::fs::remove_file(&fixed_path);
                    state.repair_trace.push(RepairTraceEntry {
                        function: "Reencode".to_string(),
                        outcome: "failed".to_string(),
                        output_path: None,
                        details: format!("Revalidation failed: {}", issues.join("; ")),
                        effectiveness: None,
                    });
                }
            }
            None => {
                let phase_ms = phase_start.elapsed().as_millis() as f64;
                state.repair_trace.push(RepairTraceEntry {
                    function: "Reencode".to_string(),
                    outcome: "failed".to_string(),
                    output_path: None,
                    details: "try_fix_reencode returned None".to_string(),
                    effectiveness: None,
                });
            }
        }

        log::warn!("[REPAIR_SINGLE] {} - All repair attempts failed, quarantining", file_name);
        state.repair_status = RepairStatus::Quarantined;
        state.disposition = FileDisposition::Unrepairable(DamageClassification::Unsupported);
        state.repair_duration_ms = phase_start.elapsed().as_millis() as f64;
        state
    }
'''

# Insert before the closing brace of impl block
lines.insert(old_start, new_methods)
print(f'Inserted corrected repair methods')

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f'New total lines: {len(lines)}')
print('[DONE] Corrected repair methods inserted')
