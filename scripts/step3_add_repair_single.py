#!/usr/bin/env python3
"""Step 3: Add repair_single method to MediaValidationEngine."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Find the end of analyze_single method (look for the closing brace after FileState construction)
# We'll insert repair_single right after analyze_single ends.
# Find the line that has the last return of analyze_single (the FileState construction)

insert_after = None
for i, line in enumerate(lines):
    # The last line of analyze_single should return a FileState
    if 'FileState {' in line and i > 1550:
        # Find the closing brace of this struct
        brace_count = 0
        for j in range(i, min(i + 40, len(lines))):
            brace_count += lines[j].count('{') - lines[j].count('}')
            if brace_count == 0:
                # This is the closing brace
                # Skip any trailing whitespace/empty lines
                k = j + 1
                while k < len(lines) and lines[k].strip() in ('', '}'):
                    k += 1
                # Check if next is a closing brace (end of function)
                if k < len(lines) and lines[k].strip() == '}':
                    insert_after = k  # After the function's closing brace
                    break
        if insert_after:
            break

if not insert_after:
    # Fallback: search for pattern
    for i, line in enumerate(lines):
        if line.strip() == '}' and i > 1550 and i < 1620:
            insert_after = i
            break

print(f'Found insertion point at line {insert_after + 1}')

repair_single_method = '''
    /// Phase 2: Attempt repair on a single damaged file.
    ///
    /// **GUARANTEE**: Only modifies the file if a repair succeeds.
    /// If all repair attempts fail, the file is quarantined (original preserved).
    ///
    /// Returns the updated FileState with repair attempt history recorded.
    pub fn repair_single(&self, mut state: FileState, input_dir: &std::path::Path) -> FileState {
        let repair_start = std::time::Instant::now();
        let file_name = state.identity.file_name.clone();
        let original_path = state.identity.original_path.clone();

        match &state.disposition {
            FileDisposition::Healthy | FileDisposition::NeedsNormalization | FileDisposition::Unrepairable(_) => {
                // Nothing to repair -- return as-is
                state.repair_status = match &state.disposition {
                    FileDisposition::Healthy => RepairStatus::Skipped,
                    FileDisposition::NeedsNormalization => RepairStatus::Skipped,
                    FileDisposition::Unrepairable(_) => RepairStatus::Quarantined,
                    _ => RepairStatus::Skipped,
                };
                return state;
            }
            FileDisposition::Repairable(damage) => {
                let damage = damage.clone();

                // Repair dispatch: least destructive first
                // Phase A: Subtitle repair
                // Phase B: Timestamp/Container repair
                // Phase C: Re-encode (last resort)
                match damage {
                    DamageClassification::SubtitleDamage => {
                        // Phase A: Subtitle-specific repair
                        log::info!("[REPAIR_SINGLE] {} - Phase A: Subtitle repair attempt", file_name);
                        let phase_start = std::time::Instant::now();

                        match self.try_fix_subtitle_remux(&original_path, input_dir) {
                            Some((fixed_path, result)) => {
                                // Revalidate the repaired file
                                match self.check_critical_post_repair(&fixed_path) {
                                    Ok(()) => {
                                        // Check stream identity
                                        match self.check_stream_identity(&original_path, &fixed_path) {
                                            Ok(()) => {
                                                let phase_ms = phase_start.elapsed().as_millis() as f64;
                                                log::info!("[REPAIR_SINGLE] {} - Subtitle repair PASSED ({:.1}ms)", file_name, phase_ms);
                                                state.repair_status = RepairStatus::Succeeded;
                                                state.repair_method = Some("SubtitleRemux".to_string());
                                                state.current_path = Some(fixed_path.to_string_lossy().to_string());
                                                state.final_path = Some(fixed_path.to_string_lossy().to_string());
                                                state.repair_attempts.push(RepairTraceEntry {
                                                    method: "SubtitleRemux".to_string(),
                                                    status: RepairAttemptStatus::Succeeded,
                                                    output_path: Some(fixed_path.to_string_lossy().to_string()),
                                                    duration_ms: phase_ms,
                                                    error: None,
                                                });
                                                state.repair_duration_ms = Some(repair_start.elapsed().as_millis() as f64);
                                                return state;
                                            }
                                            Err(e) => {
                                                log::warn!("[REPAIR_SINGLE] {} - Subtitle repair passed but stream identity check failed: {}", file_name, e);
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
                                        log::warn!("[REPAIR_SINGLE] {} - Subtitle repair passed but critical post-repair check failed: {}", file_name, e);
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
                        self.repair_timestamp_or_reencode(state, &damage, input_dir)
                    }
                    DamageClassification::TimestampDamage | DamageClassification::ContainerDamage => {
                        self.repair_timestamp_or_reencode(state, &damage, input_dir)
                    }
                    DamageClassification::VariousCorruptions => {
                        // Multiple corruption types -- try timestamp repair first, then re-encode
                        self.repair_timestamp_or_reencode(state, &damage, input_dir)
                    }
                    DamageClassification::NeedsReencode => {
                        // Already known to need re-encode -- go straight to Phase C
                        self.repair_reencode_only(state, input_dir)
                    }
                    DamageClassification::AttachmentDamage => {
                        // Attachment issues -- try container remux first
                        self.repair_timestamp_or_reencode(state, &damage, input_dir)
                    }
                    DamageClassification::Healthy | DamageClassification::Unsupported => {
                        // Should not reach here (handled by outer match)
                        state.repair_status = RepairStatus::Skipped;
                        state.repair_duration_ms = Some(repair_start.elapsed().as_millis() as f64);
                        state
                    }
                }
            }
        }
    }

    /// Phase B: Timestamp/Container repair via remux.
    fn repair_timestamp_or_reencode(&self, mut state: FileState, damage: &DamageClassification, input_dir: &std::path::Path) -> FileState {
        let file_name = state.identity.file_name.clone();
        let original_path = state.identity.original_path.clone();
        let phase_start = std::time::Instant::now();
        let phase_ms;

        log::info!("[REPAIR_SINGLE] {} - Phase B: Timestamp/Container repair attempt", file_name);

        match self.try_fix_timestamp_repair(&original_path, input_dir) {
            Some((fixed_path, _result)) => {
                match self.check_critical_post_repair(&fixed_path) {
                    Ok(()) => {
                        match self.check_stream_identity(&original_path, &fixed_path) {
                            Ok(()) => {
                                phase_ms = phase_start.elapsed().as_millis() as f64;
                                log::info!("[REPAIR_SINGLE] {} - Timestamp repair PASSED ({:.1}ms)", file_name, phase_ms);
                                state.repair_status = RepairStatus::Succeeded;
                                state.repair_method = Some("TimestampRemux".to_string());
                                state.current_path = Some(fixed_path.to_string_lossy().to_string());
                                state.final_path = Some(fixed_path.to_string_lossy().to_string());
                                state.repair_attempts.push(RepairTraceEntry {
                                    method: "TimestampRemux".to_string(),
                                    status: RepairAttemptStatus::Succeeded,
                                    output_path: Some(fixed_path.to_string_lossy().to_string()),
                                    duration_ms: phase_ms,
                                    error: None,
                                });
                                state.repair_duration_ms = Some(phase_ms);
                                return state;
                            }
                            Err(e) => {
                                phase_ms = phase_start.elapsed().as_millis() as f64;
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
                        phase_ms = phase_start.elapsed().as_millis() as f64;
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
                phase_ms = phase_start.elapsed().as_millis() as f64;
                state.repair_attempts.push(RepairTraceEntry {
                    method: "TimestampRemux".to_string(),
                    status: RepairAttemptStatus::Failed,
                    output_path: None,
                    duration_ms: phase_ms,
                    error: Some("try_fix_timestamp_repair returned None".to_string()),
                });
            }
        }

        // Phase B failed -- escalate to Phase C: Re-encode
        log::info!("[REPAIR_SINGLE] {} - Timestamp repair failed, escalating to Phase C (re-encode)", file_name);
        self.repair_reencode_only(state, input_dir)
    }

    /// Phase C: Re-encode (last resort).
    fn repair_reencode_only(&self, mut state: FileState, input_dir: &std::path::Path) -> FileState {
        let file_name = state.identity.file_name.clone();
        let original_path = state.identity.original_path.clone();
        let phase_start = std::time::Instant::now();

        log::info!("[REPAIR_SINGLE] {} - Phase C: Re-encode attempt", file_name);

        match self.try_fix_reencode(&original_path, input_dir) {
            Some((fixed_path, _result)) => {
                match self.check_critical_post_repair(&fixed_path) {
                    Ok(()) => {
                        match self.check_stream_identity(&original_path, &fixed_path) {
                            Ok(()) => {
                                let phase_ms = phase_start.elapsed().as_millis() as f64;
                                log::info!("[REPAIR_SINGLE] {} - Re-encode PASSED ({:.1}ms)", file_name, phase_ms);
                                state.repair_status = RepairStatus::Succeeded;
                                state.repair_method = Some("Reencode".to_string());
                                state.current_path = Some(fixed_path.to_string_lossy().to_string());
                                state.final_path = Some(fixed_path.to_string_lossy().to_string());
                                state.repair_attempts.push(RepairTraceEntry {
                                    method: "Reencode".to_string(),
                                    status: RepairAttemptStatus::Succeeded,
                                    output_path: Some(fixed_path.to_string_lossy().to_string()),
                                    duration_ms: phase_ms,
                                    error: None,
                                });
                                state.repair_duration_ms = Some(phase_ms);
                                return state;
                            }
                            Err(e) => {
                                let phase_ms = phase_start.elapsed().as_millis() as f64;
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
                        let phase_ms = phase_start.elapsed().as_millis() as f64;
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
                let phase_ms = phase_start.elapsed().as_millis() as f64;
                state.repair_attempts.push(RepairTraceEntry {
                    method: "Reencode".to_string(),
                    status: RepairAttemptStatus::Failed,
                    output_path: None,
                    duration_ms: phase_ms,
                    error: Some("try_fix_reencode returned None".to_string()),
                });
            }
        }

        // All repair attempts failed -- quarantine
        log::warn!("[REPAIR_SINGLE] {} - All repair attempts failed, quarantining", file_name);
        state.repair_status = RepairStatus::Quarantined;
        state.disposition = FileDisposition::Unrepairable(state.disposition.classification().cloned().unwrap_or(DamageClassification::Unsupported));
        state.repair_duration_ms = Some(phase_start.elapsed().as_millis() as f64);
        state
    }
'''

# Insert after the function closing brace
if insert_after is not None:
    # The method needs to be inside the impl block
    # Find the last } before the next pub fn or end of impl
    insert_line = repair_single_method
    lines.insert(insert_after + 1, insert_line)
    print(f'[OK] Inserted repair_single method after line {insert_after + 1}')
else:
    # Fallback: insert before the last closing brace of impl block
    for i in range(len(lines) - 1, 0, -1):
        if lines[i].strip() == '}' and i > 2400:
            lines.insert(i + 1, repair_single_method)
            print(f'[OK] Inserted repair_single method before line {i + 1} (fallback)')
            break

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print('[DONE] repair_single method added')
