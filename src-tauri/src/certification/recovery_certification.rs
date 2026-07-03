
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct RecoveryFingerprint {
    pub completed_phases: Vec<CompletedPhase>,
    pub skipped_phases: Vec<String>,
    pub checkpoint_version: u32,
    pub recovered_files: Vec<String>,
    pub resume_point: Option<String>,
    pub recovery_path: RecoveryPath,
    pub is_warm_start: bool,
    pub fingerprint_hash: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Hash)]
pub struct CompletedPhase {
    pub phase_name: String,
    pub phase_index: usize,
    pub files_processed: usize,
    pub output_files: Vec<String>,
    pub checksum: String,
    pub completed_at_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum RecoveryPath {
    Fresh,
    Resumed,
    CancelledRestart,
    FailedResume,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckpointState {
    pub version: u32,
    pub phase: String,
    pub phase_index: usize,
    pub current_file_index: usize,
    pub total_files: usize,
    pub normalized_files: Vec<NormalizedFile>,
    pub temp_files: Vec<TempFile>,
    pub concat_lists: Vec<ConcatListState>,
    pub timestamp_ms: u64,
    pub is_complete: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Hash)]
pub struct NormalizedFile {
    pub index: usize,
    pub original_path: String,
    pub normalized_path: Option<String>,
    pub was_normalized: bool,
    pub normalization_type: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Hash)]
pub struct TempFile {
    pub path: String,
    pub created_at_ms: u64,
    pub size_bytes: u64,
    pub is_valid: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Hash)]
pub struct ConcatListState {
    pub path: String,
    pub entry_count: usize,
    pub content_hash: String,
    pub is_complete: bool,
}

pub struct RecoveryCertReport {
    pub total_tests: usize,
    pub passed: bool,
    pub layer_reports: Vec<RecoveryLayerReport>,
    pub recovery_fingerprints: Vec<RecoveryFingerprint>,
    pub total_duration_ms: u128,
}

pub struct RecoveryLayerReport {
    pub layer_name: String,
    pub passed: bool,
    pub checks: Vec<RecoveryCheck>,
}

pub struct RecoveryCheck {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
    pub severity: CheckSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CheckSeverity {
    Critical,
    Error,
    Warning,
    Info,
}

pub struct RecoveryCertifier {
    checkpoint_version: u32,
}

impl RecoveryCertifier {
    pub fn new(checkpoint_version: u32) -> Self {
        RecoveryCertifier { checkpoint_version }
    }

    pub fn verify_checkpoint_integrity(&self, checkpoint: &CheckpointState) -> RecoveryLayerReport {
        let mut report = RecoveryLayerReport {
            layer_name: "Checkpoint Integrity".to_string(),
            passed: true,
            checks: vec![],
        };

        // Version check
        let version_match = checkpoint.version == self.checkpoint_version;
        report.checks.push(RecoveryCheck {
            name: "Checkpoint Version Match".to_string(),
            passed: version_match,
            expected: format!("{}", self.checkpoint_version),
            actual: format!("{}", checkpoint.version),
            severity: if version_match { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !version_match { report.passed = false; }

        // Phase consistency
        let phase_index_valid = checkpoint.phase_index <= checkpoint.total_files;
        report.checks.push(RecoveryCheck {
            name: "Phase Index Valid".to_string(),
            passed: phase_index_valid,
            expected: format!("<={}", checkpoint.total_files),
            actual: format!("{}", checkpoint.phase_index),
            severity: if phase_index_valid { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !phase_index_valid { report.passed = false; }

        // Current file index consistency
        let current_file_valid = checkpoint.current_file_index <= checkpoint.total_files;
        report.checks.push(RecoveryCheck {
            name: "Current File Index Valid".to_string(),
            passed: current_file_valid,
            expected: format!("<={}", checkpoint.total_files),
            actual: format!("{}", checkpoint.current_file_index),
            severity: if current_file_valid { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !current_file_valid { report.passed = false; }

        // Normalized files count consistency
        let normalized_count_match = checkpoint.normalized_files.len() == checkpoint.current_file_index;
        report.checks.push(RecoveryCheck {
            name: "Normalized Files Count".to_string(),
            passed: normalized_count_match,
            expected: format!("{}", checkpoint.current_file_index),
            actual: format!("{}", checkpoint.normalized_files.len()),
            severity: if normalized_count_match { CheckSeverity::Info } else { CheckSeverity::Error },
        });

        // All normalized files should have output paths if was_normalized=true
        for (i, nf) in checkpoint.normalized_files.iter().enumerate() {
            let has_output = nf.was_normalized == (nf.normalized_path.is_some());
            if !has_output {
                report.checks.push(RecoveryCheck {
                    name: format!("Normalized File {} State", i),
                    passed: false,
                    expected: if nf.was_normalized { "normalized_path set".to_string() } else { "no normalized_path".to_string() },
                    actual: format!("was_normalized={} path={:?}", nf.was_normalized, nf.normalized_path),
                    severity: CheckSeverity::Error,
                });
                report.passed = false;
            }
        }

        // Temp files validity
        let valid_temp_count = checkpoint.temp_files.iter().filter(|t| t.is_valid).count();
        let all_temps_valid = valid_temp_count == checkpoint.temp_files.len();
        report.checks.push(RecoveryCheck {
            name: "All Temp Files Valid".to_string(),
            passed: all_temps_valid,
            expected: format!("{}", checkpoint.temp_files.len()),
            actual: format!("{}", valid_temp_count),
            severity: if all_temps_valid { CheckSeverity::Info } else { CheckSeverity::Warning },
        });

        // Timestamp validity
        let timestamp_valid = checkpoint.timestamp_ms > 0;
        report.checks.push(RecoveryCheck {
            name: "Checkpoint Timestamp Valid".to_string(),
            passed: timestamp_valid,
            expected: ">0".to_string(),
            actual: format!("{}ms", checkpoint.timestamp_ms),
            severity: if timestamp_valid { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !timestamp_valid { report.passed = false; }

        report
    }

    pub fn verify_no_duplicate_execution(&self, checkpoint: &CheckpointState) -> RecoveryLayerReport {
        let mut report = RecoveryLayerReport {
            layer_name: "No Duplicate Execution".to_string(),
            passed: true,
            checks: vec![],
        };

        // Check for files that were "normalized" multiple times
        let mut seen_indices = std::collections::BTreeSet::new();
        let mut duplicates = Vec::new();

        for nf in &checkpoint.normalized_files {
            if !seen_indices.insert(nf.index) {
                duplicates.push(nf.index);
            }
        }

        let no_duplicates = duplicates.is_empty();
        report.checks.push(RecoveryCheck {
            name: "No Duplicate File Indices".to_string(),
            passed: no_duplicates,
            expected: "0 duplicates".to_string(),
            actual: if duplicates.is_empty() {
                "0 duplicates".to_string()
            } else {
                format!("{} duplicates: {:?}", duplicates.len(), &duplicates[..3.min(duplicates.len())])
            },
            severity: if no_duplicates { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !no_duplicates { report.passed = false; }

        // Check if all current file indices are consecutive
        let consecutive = self.check_indices_consecutive(&checkpoint.normalized_files);
        report.checks.push(RecoveryCheck {
            name: "Indices Consecutive".to_string(),
            passed: consecutive,
            expected: "true".to_string(),
            actual: if consecutive { "consecutive".to_string() } else { "gap detected".to_string() },
            severity: if consecutive { CheckSeverity::Info } else { CheckSeverity::Error },
        });

        report
    }

    fn check_indices_consecutive(&self, files: &[NormalizedFile]) -> bool {
        if files.is_empty() {
            return true;
        }
        let mut sorted = files.iter().map(|f| f.index).collect::<Vec<_>>();
        sorted.sort();
        for (i, &idx) in sorted.iter().enumerate() {
            if idx != i {
                return false;
            }
        }
        true
    }

    pub fn verify_temp_resource_integrity(&self, checkpoint: &CheckpointState) -> RecoveryLayerReport {
        let mut report = RecoveryLayerReport {
            layer_name: "Temporary Resource Integrity".to_string(),
            passed: true,
            checks: vec![],
        };

        // Check each temp file
        for (i, tf) in checkpoint.temp_files.iter().enumerate() {
            let path_valid = !tf.path.is_empty();
            let size_valid = tf.size_bytes > 0;
            let timestamp_valid = tf.created_at_ms > 0;
            let is_valid = tf.is_valid && path_valid && size_valid && timestamp_valid;

            report.checks.push(RecoveryCheck {
                name: format!("Temp File {} Valid", i),
                passed: is_valid,
                expected: "valid".to_string(),
                actual: format!("path={} size={} created={} valid={}",
                    if path_valid { "ok" } else { "EMPTY" },
                    if size_valid { format!("{}B", tf.size_bytes) } else { "0".to_string() },
                    if timestamp_valid { "ok" } else { "0" },
                    tf.is_valid
                ),
                severity: if is_valid { CheckSeverity::Info } else { CheckSeverity::Warning },
            });
        }

        // Check concat lists
        for (i, cl) in checkpoint.concat_lists.iter().enumerate() {
            let path_valid = !cl.path.is_empty();
            let entries_valid = cl.entry_count > 0;
            let hash_valid = !cl.content_hash.is_empty();
            let is_complete = cl.is_complete;

            let all_valid = path_valid && entries_valid && hash_valid && is_complete;
            report.checks.push(RecoveryCheck {
                name: format!("Concat List {} Valid", i),
                passed: all_valid,
                expected: "all valid".to_string(),
                actual: format!("path={} entries={} hash={} complete={}",
                    if path_valid { "ok" } else { "EMPTY" },
                    cl.entry_count,
                    if hash_valid { "ok" } else { "EMPTY" },
                    is_complete
                ),
                severity: if all_valid { CheckSeverity::Info } else { CheckSeverity::Warning },
            });
        }

        report
    }

    pub fn compute_recovery_fingerprint(&self, checkpoint: &CheckpointState) -> RecoveryFingerprint {
        let completed_phases = checkpoint.normalized_files.iter().enumerate().map(|(i, nf)| {
            CompletedPhase {
                phase_name: "Normalization".to_string(),
                phase_index: i,
                files_processed: i + 1,
                output_files: nf.normalized_path.iter().cloned().collect(),
                checksum: format!("{:016x}", nf.index),
                completed_at_ms: checkpoint.timestamp_ms,
            }
        }).collect();

        let recovery_path = if checkpoint.is_complete {
            RecoveryPath::Fresh
        } else if checkpoint.current_file_index > 0 {
            RecoveryPath::Resumed
        } else {
            RecoveryPath::Fresh
        };

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};

        // Hash all checkpoint properties
        checkpoint.version.hash(&mut hasher);
        checkpoint.phase_index.hash(&mut hasher);
        checkpoint.current_file_index.hash(&mut hasher);
        checkpoint.total_files.hash(&mut hasher);
        checkpoint.is_complete.hash(&mut hasher);

        for nf in &checkpoint.normalized_files {
            nf.hash(&mut hasher);
        }

        let fingerprint_hash = format!("{:016x}", hasher.finish());

        RecoveryFingerprint {
            completed_phases,
            skipped_phases: vec![],
            checkpoint_version: checkpoint.version,
            recovered_files: checkpoint.normalized_files.iter()
                .filter_map(|f| f.normalized_path.clone())
                .collect(),
            resume_point: Some(checkpoint.phase.clone()),
            recovery_path,
            is_warm_start: checkpoint.current_file_index > 0,
            fingerprint_hash,
        }
    }

    pub fn verify_recovery_equivalence(&self, fresh: &CheckpointState, recovered: &CheckpointState) -> RecoveryLayerReport {
        let mut report = RecoveryLayerReport {
            layer_name: "Recovery Semantic Equivalence".to_string(),
            passed: true,
            checks: vec![],
        };

        // Version must match
        let version_match = fresh.version == recovered.version;
        report.checks.push(RecoveryCheck {
            name: "Checkpoint Version Match".to_string(),
            passed: version_match,
            expected: format!("{}", fresh.version),
            actual: format!("{}", recovered.version),
            severity: if version_match { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !version_match { report.passed = false; }

        // Total files must match
        let total_match = fresh.total_files == recovered.total_files;
        report.checks.push(RecoveryCheck {
            name: "Total Files Match".to_string(),
            passed: total_match,
            expected: format!("{}", fresh.total_files),
            actual: format!("{}", recovered.total_files),
            severity: if total_match { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !total_match { report.passed = false; }

        // Normalized file indices must match
        let fresh_indices: Vec<usize> = fresh.normalized_files.iter().map(|f| f.index).collect();
        let recovered_indices: Vec<usize> = recovered.normalized_files.iter().map(|f| f.index).collect();
        let indices_match = fresh_indices == recovered_indices;
        report.checks.push(RecoveryCheck {
            name: "Normalized File Indices Match".to_string(),
            passed: indices_match,
            expected: format!("{:?}", fresh_indices),
            actual: format!("{:?}", recovered_indices),
            severity: if indices_match { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !indices_match { report.passed = false; }

        // Normalization types must match
        let fresh_types: Vec<String> = fresh.normalized_files.iter().map(|f| f.normalization_type.clone()).collect();
        let recovered_types: Vec<String> = recovered.normalized_files.iter().map(|f| f.normalization_type.clone()).collect();
        let types_match = fresh_types == recovered_types;
        report.checks.push(RecoveryCheck {
            name: "Normalization Types Match".to_string(),
            passed: types_match,
            expected: format!("{:?}", fresh_types),
            actual: format!("{:?}", recovered_types),
            severity: if types_match { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !types_match { report.passed = false; }

        // Concat list content hashes must match
        let fresh_hashes: Vec<String> = fresh.concat_lists.iter().map(|c| c.content_hash.clone()).collect();
        let recovered_hashes: Vec<String> = recovered.concat_lists.iter().map(|c| c.content_hash.clone()).collect();
        let hashes_match = fresh_hashes == recovered_hashes;
        report.checks.push(RecoveryCheck {
            name: "Concat List Hashes Match".to_string(),
            passed: hashes_match,
            expected: format!("{:?}", fresh_hashes),
            actual: format!("{:?}", recovered_hashes),
            severity: if hashes_match { CheckSeverity::Info } else { CheckSeverity::Critical },
        });
        if !hashes_match { report.passed = false; }

        report
    }
}

impl Default for RecoveryCertifier {
    fn default() -> Self {
        Self::new(1)
    }
}

pub fn certify_recovery_idempotency<F1, F2>(
    playlist_name: &str,
    build_fresh_checkpoint: F1,
    build_recovered_checkpoint: F2,
) -> RecoveryCertReport
where
    F1: Fn() -> Option<CheckpointState>,
    F2: Fn() -> Option<CheckpointState>,
{
    use std::time::Instant;
    let start = Instant::now();

    let certifier = RecoveryCertifier::new(1);

    log::info!("[CERT:RECOVERY] ═══════════════════════════════════════════════════════════");
    log::info!("[CERT:RECOVERY] RECOVERY IDEMPOTENCY CERTIFICATION");
    log::info!("[CERT:RECOVERY] Playlist: {}", playlist_name);
    log::info!("[CERT:RECOVERY] ═══════════════════════════════════════════════════════════");

    // Build checkpoints
    let fresh = match build_fresh_checkpoint() {
        Some(c) => c,
        None => {
            log::error!("[CERT:RECOVERY] Failed to build fresh checkpoint");
            return RecoveryCertReport {
                total_tests: 1,
                passed: false,
                layer_reports: vec![],
                recovery_fingerprints: vec![],
                total_duration_ms: start.elapsed().as_millis(),
            };
        }
    };

    let recovered = match build_recovered_checkpoint() {
        Some(c) => c,
        None => {
            log::error!("[CERT:RECOVERY] Failed to build recovered checkpoint");
            return RecoveryCertReport {
                total_tests: 1,
                passed: false,
                layer_reports: vec![],
                recovery_fingerprints: vec![],
                total_duration_ms: start.elapsed().as_millis(),
            };
        }
    };

    // Layer 1: Fresh checkpoint integrity
    let fresh_integrity = certifier.verify_checkpoint_integrity(&fresh);
    log::info!("[CERT:RECOVERY] Layer 1 - Fresh Checkpoint: {}", if fresh_integrity.passed { "✅ PASS" } else { "❌ FAIL" });

    // Layer 2: Recovered checkpoint integrity
    let recovered_integrity = certifier.verify_checkpoint_integrity(&recovered);
    log::info!("[CERT:RECOVERY] Layer 2 - Recovered Checkpoint: {}", if recovered_integrity.passed { "✅ PASS" } else { "❌ FAIL" });

    // Layer 3: No duplicate execution (fresh)
    let fresh_no_dup = certifier.verify_no_duplicate_execution(&fresh);
    log::info!("[CERT:RECOVERY] Layer 3 - No Duplicate (fresh): {}", if fresh_no_dup.passed { "✅ PASS" } else { "❌ FAIL" });

    // Layer 4: No duplicate execution (recovered)
    let recovered_no_dup = certifier.verify_no_duplicate_execution(&recovered);
    log::info!("[CERT:RECOVERY] Layer 4 - No Duplicate (recovered): {}", if recovered_no_dup.passed { "✅ PASS" } else { "❌ FAIL" });

    // Layer 5: Temp resource integrity
    let fresh_temp = certifier.verify_temp_resource_integrity(&fresh);
    let recovered_temp = certifier.verify_temp_resource_integrity(&recovered);
    log::info!("[CERT:RECOVERY] Layer 5 - Temp Resources (fresh): {}", if fresh_temp.passed { "✅ PASS" } else { "⚠️  WARN" });
    log::info!("[CERT:RECOVERY] Layer 5 - Temp Resources (recovered): {}", if recovered_temp.passed { "✅ PASS" } else { "⚠️  WARN" });

    // Layer 6: Recovery equivalence
    let equivalence = certifier.verify_recovery_equivalence(&fresh, &recovered);
    log::info!("[CERT:RECOVERY] Layer 6 - Recovery Equivalence: {}", if equivalence.passed { "✅ PASS" } else { "❌ FAIL" });

    // Compute fingerprints
    let fresh_fp = certifier.compute_recovery_fingerprint(&fresh);
    let recovered_fp = certifier.compute_recovery_fingerprint(&recovered);

    log::info!("[CERT:RECOVERY] Fresh Recovery Fingerprint: {}", &fresh_fp.fingerprint_hash[..12]);
    log::info!("[CERT:RECOVERY] Recovered Fingerprint: {}", &recovered_fp.fingerprint_hash[..12]);

    let fingerprints_match = fresh_fp.fingerprint_hash == recovered_fp.fingerprint_hash;
    log::info!("[CERT:RECOVERY] Fingerprints Match: {}", if fingerprints_match { "✅ PASS" } else { "❌ FAIL" });

    let all_passed = fresh_integrity.passed
        && recovered_integrity.passed
        && fresh_no_dup.passed
        && recovered_no_dup.passed
        && equivalence.passed
        && fingerprints_match;

    log::info!("[CERT:RECOVERY] ═══════════════════════════════════════════════════════════");
    if all_passed {
        log::info!("[CERT:RECOVERY] ✅ RECOVERY CERTIFICATION PASSED");
    } else {
        log::error!("[CERT:RECOVERY] ❌ RECOVERY CERTIFICATION FAILED");
    }
    log::info!("[CERT:RECOVERY] Duration: {}ms", start.elapsed().as_millis());
    log::info!("[CERT:RECOVERY] ═══════════════════════════════════════════════════════════");

    RecoveryCertReport {
        total_tests: 6,
        passed: all_passed,
        layer_reports: vec![
            fresh_integrity,
            recovered_integrity,
            fresh_no_dup,
            recovered_no_dup,
            fresh_temp,
            recovered_temp,
            equivalence,
        ],
        recovery_fingerprints: vec![fresh_fp, recovered_fp],
        total_duration_ms: start.elapsed().as_millis(),
    }
}

impl std::fmt::Display for RecoveryCertReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "╔══════════════════════════════════════════════════════════════════════╗")?;
        writeln!(f, "║              RECOVERY IDEMPOTENCY CERTIFICATION REPORT              ║")?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  Total tests: {:52} ║", self.total_tests)?;
        writeln!(f, "║  Duration: {:54}ms ║", self.total_duration_ms)?;
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        writeln!(f, "║  LAYER RESULTS:")?;
        for (i, layer) in self.layer_reports.iter().enumerate() {
            let status = if layer.passed { "✅ PASS" } else { "❌ FAIL" };
            writeln!(f, "║    Layer {} - {}: {}                            ║", i + 1, layer.layer_name, status)?;
            for check in &layer.checks {
                if !check.passed {
                    writeln!(f, "║      ⚠️  {}                                         ║", check.name)?;
                    writeln!(f, "║         expected={} actual={}     ║",
                        check.expected.chars().take(20).collect::<String>(),
                        check.actual.chars().take(20).collect::<String>())?;
                }
            }
        }
        if !self.recovery_fingerprints.is_empty() {
            let fp1 = &self.recovery_fingerprints[0];
            let fp2 = self.recovery_fingerprints.get(1);
            writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
            writeln!(f, "║  RECOVERY FINGERPRINTS:")?;
            writeln!(f, "║    Fresh:      {}                                 ║", &fp1.fingerprint_hash[..20])?;
            if let Some(fp2) = fp2 {
                writeln!(f, "║    Recovered:  {}                                 ║", &fp2.fingerprint_hash[..20])?;
            }
        }
        writeln!(f, "╠══════════════════════════════════════════════════════════════════════╣")?;
        let overall = if self.passed { "✅ RECOVERY CERTIFIED" } else { "❌ RECOVERY FAILED" };
        writeln!(f, "║  OVERALL: {:57} ║", overall)?;
        writeln!(f, "╚══════════════════════════════════════════════════════════════════════╝")?;
        Ok(())
    }
}