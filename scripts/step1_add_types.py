#!/usr/bin/env python3
"""Step 1: Add new types to media_validation_engine.rs for the 4-phase pipeline."""

path = 'src-tauri/src/ffmpeg/media_validation_engine.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

# Find the closing brace of DamageClassification enum
insert_after = None
for i, line in enumerate(lines):
    if 'Unsupported' in line and 'DamageClassification' not in line:
        # Find the closing } after Unsupported
        for j in range(i, min(i + 5, len(lines))):
            if lines[j].strip() == '}':
                insert_after = j
                break
        break

if insert_after is None:
    print("ERROR: Could not find DamageClassification closing brace")
    exit(1)

print(f"Inserting after line {insert_after + 1}")

new_types = """

/// Determines what happens to a file in the pipeline.
/// Assigned during Phase 1 (Analyze) and consumed by Phases 2-4.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FileDisposition {
    /// Healthy file -- merge directly, no repair or normalization needed.
    Healthy,
    /// Healthy but needs compatibility normalization (e.g., audio profile mismatch).
    NeedsNormalization,
    /// Has repairable damage -- will be repaired in Phase 2.
    Repairable(DamageClassification),
    /// Damage is too severe to repair -- quarantine this file.
    Unrepairable(DamageClassification),
}

/// Status of a file after Phase 2 repair.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RepairStatus {
    /// File was healthy -- repair was skipped.
    Skipped,
    /// Repair succeeded -- repaired_path is set.
    Succeeded,
    /// Repair failed -- file will be quarantined.
    Failed,
    /// File was unrepairable -- quarantined without attempting repair.
    Quarantined,
}

/// Status of a file after Phase 3 revalidation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RevalidationStatus {
    /// File was not repaired -- revalidation not needed.
    NotNeeded,
    /// Revalidation passed -- repaired file is healthy.
    Passed,
    /// Revalidation failed -- repaired file still has issues.
    Failed,
}

/// Complete lifecycle state for a single file through the 4-phase pipeline.
/// Every file gets one of these -- healthy or damaged.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileState {
    // -- Identity --
    pub file_index: usize,
    pub original_path: String,
    pub original_name: String,
    pub original_duration_secs: f64,

    // -- Phase 1: Analysis (read-only) --
    pub disposition: FileDisposition,
    pub damage_classification: DamageClassification,
    pub confidence: f32,
    pub analysis_reason: String,
    pub has_pts_issues: bool,
    pub has_dts_issues: bool,
    pub has_subtitle_issues: bool,
    pub has_container_issues: bool,
    pub has_video_decode_issues: bool,
    pub has_bitstream_issues: bool,
    pub has_packet_issues: bool,
    pub has_frame_issues: bool,
    pub has_attachment_issues: bool,
    pub has_timebase_issues: bool,
    pub has_vfr_instability: bool,
    pub analysis_duration_ms: f64,

    // -- Phase 2: Repair --
    pub repair_status: RepairStatus,
    pub fix_applied: Option<FixType>,
    pub repaired_path: Option<String>,
    pub repair_trace: Vec<RepairTraceEntry>,
    pub repair_duration_ms: f64,

    // -- Phase 3: Revalidation --
    pub revalidation_status: RevalidationStatus,
    pub revalidation_duration_ms: f64,

    // -- Final State --
    pub final_path: String,
}

/// Complete pipeline report for the entire playlist.
/// Contains one FileState per input file.
#[derive(Debug)]
pub struct PipelineReport {
    pub file_states: Vec<FileState>,
    pub total_count: usize,
    pub healthy_count: usize,
    pub needs_normalization_count: usize,
    pub repaired_count: usize,
    pub failed_repair_count: usize,
    pub quarantined_count: usize,
    pub total_analysis_duration_secs: f64,
    pub total_repair_duration_secs: f64,
    pub total_revalidation_duration_secs: f64,
}
"""

lines.insert(insert_after + 1, new_types)

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(lines)

print(f"Inserted {new_types.count(chr(10))} new lines of types")

# Verify
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

checks = [
    'pub enum FileDisposition',
    'pub enum RepairStatus',
    'pub enum RevalidationStatus',
    'pub struct FileState',
    'pub struct PipelineReport',
]
for c in checks:
    print(f"  [{'PASS' if c in content else 'FAIL'}] {c}")
