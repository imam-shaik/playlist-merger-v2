#!/usr/bin/env python3
"""Fix repair_single placement: remove from wrong locations, insert into correct impl block."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

print(f'Original total lines: {len(lines)}')

# Step 1: Find and remove orphaned helper methods from inside analyze_single
# These are repair_timestamp_or_reencode and repair_reencode_only that appear
# right after the analyze_single closing brace
orphan_start = None
orphan_end = None

for i, line in enumerate(lines):
    if '/// Phase B: Timestamp/Container repair via remux.' in line and i < 1700:
        orphan_start = i
        break

if orphan_start is not None:
    # Find the end by tracking brace depth
    brace_depth = 0
    found_first = False
    for i in range(orphan_start, len(lines)):
        for ch in lines[i]:
            if ch == '{':
                brace_depth += 1
                found_first = True
            elif ch == '}':
                brace_depth -= 1
        if found_first and brace_depth == 0:
            orphan_end = i
            break

    if orphan_end:
        print(f'Orphaned helpers: lines {orphan_start + 1} to {orphan_end + 1}')
        del lines[orphan_start:orphan_end + 1]
        print(f'Removed {orphan_end - orphan_start + 1} orphaned lines')
    else:
        print('[WARN] Could not find end of orphaned helpers')
else:
    print('[INFO] No orphaned helpers found inside analyze_single')

# Step 2: Find and remove ALL repair methods from impl Default for MediaValidationResult
# This block starts around line 4297 (now shifted due to deletion)
default_impl_start = None
repair_in_default_start = None
repair_in_default_end = None

for i, line in enumerate(lines):
    if 'impl Default for MediaValidationResult' in line:
        default_impl_start = i
        break

if default_impl_start is not None:
    # Find the repair_single doc comment inside this block
    for i in range(default_impl_start, len(lines)):
        if '/// Phase 2: Attempt repair on a single damaged file.' in lines[i]:
            repair_in_default_start = i
            break

    if repair_in_default_start is not None:
        # Find the end of the last method (repair_reencode_only)
        brace_depth = 0
        found_first = False
        for i in range(repair_in_default_start, len(lines)):
            for ch in lines[i]:
                if ch == '{':
                    brace_depth += 1
                    found_first = True
                elif ch == '}':
                    brace_depth -= 1
            if found_first and brace_depth == 0:
                repair_in_default_end = i
                break

        if repair_in_default_end:
            print(f'Repair in Default impl: lines {repair_in_default_start + 1} to {repair_in_default_end + 1}')
            # Keep a blank line before the impl closing brace
            del lines[repair_in_default_start:repair_in_default_end + 1]
            print(f'Removed {repair_in_default_end - repair_in_default_start + 1} lines from Default impl')
        else:
            print('[WARN] Could not find end of repair methods in Default impl')

# Step 3: Find the correct impl MediaValidationEngine closing brace
# and insert all three methods there
engine_impl_end = None
for i, line in enumerate(lines):
    if 'impl MediaValidationEngine' in line and 'Default' not in line:
        # Find the matching closing brace
        brace_depth = 0
        found_first = False
        for j in range(i, len(lines)):
            for ch in lines[j]:
                if ch == '{':
                    brace_depth += 1
                    found_first = True
                elif ch == '}':
                    brace_depth -= 1
            if found_first and brace_depth == 0:
                engine_impl_end = j
                break
        break

if engine_impl_end:
    print(f'impl MediaValidationEngine closing brace at line {engine_impl_end + 1}')
else:
    print('[ERROR] Could not find impl MediaValidationEngine closing brace')
    exit(1)

# Step 4: Insert the repair_single methods before the closing brace
repair_methods = '''
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
                        log::info!("[REPAIR_SINGLE] {} - Phase A: Subtitle repair attempt", file_name);
                        let phase_start = std::time::Instant::now();
                        match self.try_fix_subtitle_remux(&original_path, input_dir) {
                            Some((fixed_path, _result)) => {
                                match self.check_critical_post_repair(&fixed_path) {
                                    Ok(()) => {
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
                        self.repair_timestamp_or_reencode(state, &damage, input_dir)
                    }
                    DamageClassification::TimestampDamage | DamageClassification::ContainerDamage => {
                        self.repair_timestamp_or_reencode(state, &damage, input_dir)
                    }
                    DamageClassification::VariousCorruptions => {
                        self.repair_timestamp_or_reencode(state, &damage, input_dir)
                    }
                    DamageClassification::NeedsReencode => {
                        self.repair_reencode_only(state, input_dir)
                    }
                    DamageClassification::AttachmentDamage => {
                        self.repair_timestamp_or_reencode(state, &damage, input_dir)
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
    fn repair_timestamp_or_reencode(&self, mut state: FileState, _damage: &DamageClassification, input_dir: &std::path::Path) -> FileState {
        let file_name = state.identity.file_name.clone();
        let original_path = state.identity.original_path.clone();
        let phase_start = std::time::Instant::now();

        log::info!("[REPAIR_SINGLE] {} - Phase B: Timestamp/Container repair attempt", file_name);

        match self.try_fix_timestamp_repair(&original_path, input_dir) {
            Some((fixed_path, _result)) => {
                match self.check_critical_post_repair(&fixed_path) {
                    Ok(()) => {
                        match self.check_stream_identity(&original_path, &fixed_path) {
                            Ok(()) => {
                                let phase_ms = phase_start.elapsed().as_millis() as f64;
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

        log::warn!("[REPAIR_SINGLE] {} - All repair attempts failed, quarantining", file_name);
        state.repair_status = RepairStatus::Quarantined;
        state.disposition = FileDisposition::Unrepairable(DamageClassification::Unsupported);
        state.repair_duration_ms = Some(repair_start.elapsed().as_millis() as f64);
        state
    }
'''

# Insert before the closing brace
lines[engine_impl_end:engine_impl_end] = repair_methods.split('\n')
print(f'Inserted repair methods before line {engine_impl_end + 1}')

print(f'New total lines: {len(lines)}')

with open(path, 'w', encoding='utf-8') as f:
    f.write('\n'.join(lines))

print('[DONE] All repair methods placed in correct impl block')
