# SmartMKV 4-Phase Pipeline Architecture

> **Status:** Design Document
> **Author:** Buffy (AI Agent)
> **Date:** July 1, 2026
> **Purpose:** Refactor SmartMKV's validation/repair pipeline from a coupled analyze-and-repair-in-one-pass model to a clean 4-phase separation.

---

## 1. Problem Statement

The current `validate_single_impl` in `media_validation_engine.rs` couples **analysis** and **repair** in a single function pass. When it detects container damage at Phase 1 (quick container check), it immediately attempts repair (remux → re-encode → quarantine) **before** running the deep checks (PTS, DTS, video decode, bitstream, packet integrity, etc.).

This means:

1. **Premature repair decisions:** A file with container damage gets repaired before we know if it also has subtitle damage or packet corruption.
2. **No holistic damage picture:** The repair strategy is chosen based on incomplete information.
3. **Interleaved phases:** The UI shows "Analyzing... Repairing... Analyzing..." instead of clear phase progression.
4. **Redundant work:** Files that are repaired go through the full repair pipeline, then the entire validation pipeline runs again on ALL files (not just repaired ones).

### Current Pipeline Flow

```
Probe → Media Validation (analyze+repair coupled) → Timestamp Cert → Subtitle Processing → Audio Validation → Normalization → mkvmerge → FFmpeg Concat
```

### Proposed Pipeline Flow

```
Phase 1: Analyze All Files (read-only)
Phase 2: Repair Only Damaged Files
Phase 3: Revalidate Only Repaired Files
Phase 4: Merge (healthy + repaired + normalized)
```

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     INPUT: Playlist (N files)                    │
└──────────────────────────┬──────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│  PHASE 1: ANALYZE ALL FILES (read-only, no modifications)       │
│                                                                  │
│  For each file:                                                  │
│    quick_container_check → deep checks → Phase 9 checks          │
│    → classify_damage_extended                                    │
│                                                                  │
│  Output: AnalysisReport {                                        │
│    file_results: Vec<AnalyzeResult>,                             │
│    healthy_count, damaged_count, unrepairable_count              │
│  }                                                               │
│                                                                  │
│  File statuses assigned:                                         │
│    • Healthy          → merge directly                           │
│    • Compatible       → merge directly (no normalization needed) │
│    • NeedsNorm        → needs compatibility normalization        │
│    • RepairableDamage → needs repair (timestamp/subtitle/etc.)   │
│    • Unrepairable     → quarantine                               │
└──────────────────────────┬──────────────────────────────────────┘
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
┌──────────────────────┐  ┌──────────────────────────────────────┐
│ Healthy/Compatible   │  │ Damaged Files                        │
│ → skip to Phase 4    │  │                                      │
└──────────────────────┘  │  PHASE 2: REPAIR ONLY DAMAGED FILES  │
                          │                                      │
                          │  Repair dispatch per DamageClass:     │
                          │    SubtitleDamage → try_fix_subtitle  │
                          │    TimestampDamage → try_fix_timestamp│
                          │    ContainerDamage → try_fix_remux    │
                          │    NeedsReencode → try_fix_reencode   │
                          │    VideoDecodeFailure → try_fix_reencode│
                          │    BitstreamCorruption → try_fix_reencode│
                          │    PacketCorruption → try_fix_reencode│
                          │                                      │
                          │  Output: RepairReport {               │
                          │    repaired: Vec<RepairedFile>,       │
                          │    failed: Vec<FailedRepair>,         │
                          │    quarantined: Vec<QuarantinedFile>  │
                          │  }                                    │
                          └──────────────┬───────────────────────┘
                                         │
                                         ▼
                          ┌──────────────────────────────────────┐
                          │  PHASE 3: REVALIDATE REPAIRED FILES  │
                          │                                      │
                          │  Only runs full validation on files  │
                          │  that were repaired in Phase 2.      │
                          │                                      │
                          │  Output: RevalidationReport {         │
                          │    passed: Vec<RepairedFile>,         │
                          │    failed: Vec<FailedRevalidation>    │
                          │  }                                    │
                          └──────────────┬───────────────────────┘
                                         │
                                         ▼
                          ┌──────────────────────────────────────┐
                          │  PHASE 4: MERGE                      │
                          │                                      │
                          │  Inputs:                             │
                          │    Healthy files (from Phase 1)      │
                          │    + Repaired files (from Phase 3)   │
                          │    + Normalized files (if needed)     │
                          │                                      │
                          │  Pipeline:                            │
                          │    Timestamp Cert → Subtitle → Audio │
                          │    → Normalize → mkvmerge → Verify   │
                          └──────────────────────────────────────┘
```

---

## 3. Data Model Changes

### 3.1 New Enum: `FileDisposition`

Each file gets a disposition after Phase 1 analysis. This replaces the current approach where `DamageClassification` serves double duty as both analysis result and repair trigger.

```rust
/// Determines what happens to a file in the pipeline.
/// Assigned during Phase 1 (Analyze) and consumed by Phases 2-4.
#[derive(Debug, Clone, PartialEq)]
pub enum FileDisposition {
    /// Healthy file — merge directly, no repair or normalization needed.
    Healthy,
    /// Healthy but needs compatibility normalization (e.g., audio profile mismatch).
    NeedsNormalization,
    /// Has repairable damage — will be repaired in Phase 2.
    Repairable(DamageClassification),
    /// Damage is too severe to repair — quarantine this file.
    Unrepairable(DamageClassification),
}
```

### 3.2 New Struct: `FileState` (Lifecycle Tracker)

**This is the core data model.** Every file in the playlist gets a `FileState` that tracks its complete lifecycle from original path through analysis, repair, revalidation, to final path. This provides full provenance for every file.

```rust
/// Complete lifecycle state for a single file through the 4-phase pipeline.
/// Every file gets one of these — healthy or damaged.
#[derive(Debug, Clone)]
pub struct FileState {
    // ── Identity ──
    pub file_index: usize,
    pub original_path: String,
    pub original_name: String,
    pub original_duration_secs: f64,

    // ── Phase 1: Analysis (read-only) ──
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

    // ── Phase 2: Repair ──
    pub repair_status: RepairStatus,
    pub fix_applied: Option<FixType>,
    pub repaired_path: Option<String>,
    pub repair_trace: Vec<RepairTraceEntry>,
    pub repair_duration_ms: f64,

    // ── Phase 3: Revalidation ──
    pub revalidation_status: RevalidationStatus,
    pub revalidation_duration_ms: f64,

    // ── Final State ──
    pub final_path: String,  // The path used for merge (original or repaired)
}

/// Status of a file after Phase 2 repair.
#[derive(Debug, Clone, PartialEq)]
pub enum RepairStatus {
    /// File was healthy — repair was skipped.
    Skipped,
    /// Repair succeeded — repaired_path is set.
    Succeeded,
    /// Repair failed — file will be quarantined.
    Failed,
    /// File was unrepairable — quarantined without attempting repair.
    Quarantined,
}

/// Status of a file after Phase 3 revalidation.
#[derive(Debug, Clone, PartialEq)]
pub enum RevalidationStatus {
    /// File was not repaired — revalidation not needed.
    NotNeeded,
    /// Revalidation passed — repaired file is healthy.
    Passed,
    /// Revalidation failed — repaired file still has issues.
    Failed,
}
```

### 3.3 Lifecycle Examples

**Healthy file — no repair needed:**
```
lecture83.mp4
  Analysis:    Healthy
  Repair:      Skipped
  Revalidation: NotNeeded
  Final:       lecture83.mp4 (original)
```

**Damaged file — timestamp repair succeeded:**
```
lecture91.mp4
  Analysis:    TimestampDamage
  Repair:      Succeeded (TimestampRepair)
  Revalidation: Passed
  Final:       lecture91_fixed.mkv (repaired)
```

**Damaged file — repair failed:**
```
lecture129.mp4
  Analysis:    VideoDecodeFailure
  Repair:      Failed
  Revalidation: NotNeeded
  Final:       (quarantined — excluded from merge)
```

### 3.4 Batch Report: `PipelineReport`

```rust
/// Complete pipeline report for the entire playlist.
/// Contains one FileState per input file.
#[derive(Debug)]
pub struct PipelineReport {
    pub file_states: Vec<FileState>,
    pub total_count: usize,
    pub healthy_count: usize,
    pub repaired_count: usize,
    pub failed_repair_count: usize,
    pub quarantined_count: usize,
    pub total_analysis_duration_secs: f64,
    pub total_repair_duration_secs: f64,
    pub total_revalidation_duration_secs: f64,
}
```

### 3.5 Backward Compatibility

The existing `MediaValidationResult` and `MediaValidationReport` structs remain unchanged for external callers. Internally, they are constructed from `FileState` data:

```rust
impl From<&FileState> for MediaValidationResult {
    fn from(state: &FileState) -> Self {
        MediaValidationResult {
            file_index: state.file_index,
            file_path: state.original_path.clone(),
            status: map_disposition_to_status(&state.disposition, &state.repair_status),
            // ... map other fields ...
        }
    }
}
```

---

## 4. API Changes

### 4.1 New Public Functions on `MediaValidationEngine`

```rust
impl MediaValidationEngine {
    // ── Phase 1: Analyze (100% read-only, side-effect free) ──
    /// Analyze all files without modifying them.
    /// Creates a FileState for each file with disposition and damage classification.
    /// MUST NOT create temp files, write to disk, or modify any input file.
    pub fn analyze_all(
        &self,
        files: &[(usize, &str)],
        cancel_flag: Option<&AtomicBool>,
    ) -> PipelineReport;

    // ── Phase 2: Repair (targeted, only damaged files) ──
    /// Repair only the files with disposition == Repairable.
    /// Updates FileState.repair_status and FileState.repaired_path.
    /// Does NOT touch Healthy or Unrepairable files.
    pub fn repair_damaged(
        &self,
        report: &mut PipelineReport,
        cancel_flag: Option<&AtomicBool>,
    );

    // ── Phase 3: Revalidate (repaired files only) ──
    /// Revalidate only files where repair_status == Succeeded.
    /// Updates FileState.revalidation_status and FileState.final_path.
    /// Does NOT revalidate healthy files.
    pub fn revalidate_repaired(
        &self,
        report: &mut PipelineReport,
        cancel_flag: Option<&AtomicBool>,
    );

    // ── Backward-compatible entry point ──
    /// Original entry point — now implemented as analyze_all + repair_damaged + revalidate_repaired.
    pub fn validate_input_files(
        ffprobe_path: &Path,
        ffmpeg_path: &Path,
        input_files: &[String],
        quarantine_dir: &Path,
        cancel_flag: Option<Arc<AtomicBool>>,
    ) -> MediaValidationReport;
}
```

### 4.2 Key Design Constraint: Phase 1 is Side-Effect Free

The `analyze_all` method MUST be provably read-only:

```rust
// ✅ ALLOWED in Phase 1:
//   - Reading file metadata via ffprobe
//   - Reading file packets via ffprobe (non-destructive)
//   - Computing DamageClassification
//   - Setting FileState.disposition

// ❌ FORBIDDEN in Phase 1:
//   - Creating temp files
//   - Running ffmpeg
//   - Modifying any input file
//   - Writing to quarantine_dir
//   - Any disk write whatsoever
```

This constraint is critical for resume support: if the app crashes after Phase 1, no files have been modified and the analysis can be cached and reused.

### 4.3 Internal Method Split

The current `validate_single_impl` (~500 lines) splits into three focused methods:

| Current Method | New Method | Scope |
|---|---|---|
| `validate_single_impl` (Phase 1 quick check + deep checks + classification) | `analyze_single` | Read-only analysis, populates FileState fields |
| `validate_single_impl` (repair dispatch: subtitle → timestamp → container → re-encode) | `repair_single` | Takes FileState, performs targeted repair, updates repair_status |
| `validate_single_impl` (post-repair verification) | `revalidate_single` | Full validation on repaired_path, updates revalidation_status |

---

## 5. Pipeline Integration

### 5.1 `start_merge` Restructured

```rust
// ═══════════════════════════════════════════════════════════════
// PHASE 1: ANALYZE ALL FILES (100% read-only, side-effect free)
// ═══════════════════════════════════════════════════════════════
let mut pipeline_report = engine.analyze_all(&indexed_files, cancel_flag.as_deref());

// Log analysis summary
log::info!("[PHASE_1_ANALYZE] Complete in {:.1}s: {} total | {} healthy | {} repairable | {} unrepairable",
    pipeline_report.total_analysis_duration_secs,
    pipeline_report.total_count,
    pipeline_report.healthy_count,
    pipeline_report.repaired_count + pipeline_report.failed_repair_count,
    pipeline_report.quarantined_count);

// Emit progress: "Analyzing playlist... 134/134 — 2 damaged files found"
emit_progress(app_handle, "phase-1-analyze", 100, &json!({
    "phase": "analyze",
    "total": pipeline_report.total_count,
    "healthy": pipeline_report.healthy_count,
    "damaged": pipeline_report.repaired_count + pipeline_report.failed_repair_count,
}));

// ═══════════════════════════════════════════════════════════════
// PHASE 2: REPAIR ONLY DAMAGED FILES
// ═══════════════════════════════════════════════════════════════
let repairable = pipeline_report.file_states.iter()
    .filter(|s| matches!(s.disposition, FileDisposition::Repairable(_)))
    .count();

if repairable > 0 {
    log::info!("[PHASE_2_REPAIR] Repairing {} damaged files...", repairable);
    engine.repair_damaged(&mut pipeline_report, cancel_flag.as_deref());
} else {
    log::info!("[PHASE_2_REPAIR] Skipped — no damaged files");
}

// ═══════════════════════════════════════════════════════════════
// PHASE 3: REVALIDATE ONLY REPAIRED FILES
// ═══════════════════════════════════════════════════════════════
let repaired = pipeline_report.file_states.iter()
    .filter(|s| s.repair_status == RepairStatus::Succeeded)
    .count();

if repaired > 0 {
    log::info!("[PHASE_3_REVALIDATE] Revalidating {} repaired files...", repaired);
    engine.revalidate_repaired(&mut pipeline_report, cancel_flag.as_deref());
} else {
    log::info!("[PHASE_3_REVALIDATE] Skipped — no repaired files");
}

// ═══════════════════════════════════════════════════════════════
// ASSEMBLE FINAL FILE LIST FOR MERGE
// ═══════════════════════════════════════════════════════════════
// Phase 4 receives only the final file list — it does NOT know
// which files were repaired. Every FileState has a final_path.
let merge_files: Vec<&FileState> = pipeline_report.file_states.iter()
    .filter(|s| !s.final_path.is_empty())  // non-empty = not quarantined
    .collect();

log::info!("[MERGE] {} files ready for merge ({} original + {} repaired)",
    merge_files.len(),
    merge_files.iter().filter(|s| s.repair_status == RepairStatus::Skipped).count(),
    merge_files.iter().filter(|s| s.repair_status == RepairStatus::Succeeded).count());
```

### 5.2 File List Assembly

After Phase 3, the final merge list is assembled from `FileState` entries:

```
For each FileState in pipeline_report:
  if final_path is not empty → include in merge
  if final_path is empty → quarantined, excluded from merge

Final merge list =
    Healthy files (final_path == original_path)
  + Successfully repaired files (final_path == repaired_path)
  - Quarantined files (final_path is empty)
```

### 5.3 Normalization Integration

Files with `disposition == NeedsNormalization` go through the existing normalization pipeline (Phase 7 in the current code). Files with `disposition == Healthy` skip normalization entirely — they go directly to mkvmerge.

The `final_path` is updated after normalization:

```rust
// After normalization loop:
for state in &mut pipeline_report.file_states {
    if state.disposition == FileDisposition::NeedsNormalization {
        // normalization updates state.final_path to the normalized output
    }
}
```

---

## 6. UI Progress Events

### New Event Structure

```json
{
  "phase": "analyze",
  "phase_number": 1,
  "phase_name": "Analyzing Playlist",
  "progress_percent": 100,
  "detail": "134 files analyzed — 2 damaged found",
  "file_progress": {
    "current": 134,
    "total": 134
  }
}
```

### Phase Progression

```
Phase 1: "Analyzing Playlist... 134/134"  →  "2 damaged files found"
Phase 2: "Repairing 2 files... 1/2"       →  "2/2 — all repaired"
Phase 3: "Verifying repairs... 1/2"       →  "2/2 — all verified"
Phase 4: "Merging playlist..."            →  "Complete — 143.9s"
```

This replaces the current interleaved "Analyzing... Repairing... Analyzing..." pattern.

---

## 7. Backward Compatibility

The existing `validate_input_files` public API remains unchanged. Internally, it now calls the 3-phase sequence:

```rust
pub fn validate_input_files(...) -> MediaValidationReport {
    let engine = MediaValidationEngine::new(...);
    let analysis = engine.analyze_all(...);
    let repairs = engine.repair_damaged(..., &analysis, ...);
    let revalidation = engine.revalidate_repaired(&repairs, ...);
    
    // Assemble into existing MediaValidationReport format
    MediaValidationReport {
        file_results: merge_results(analysis, repairs, revalidation),
        ...
    }
}
```

This means callers outside `merge.rs` (e.g., the certification framework, the Tauri command handler `validate_audio_files`) continue to work without changes.

---

## 8. Migration Strategy

### Step 1: Add New Types (no behavior change)
- Add `FileDisposition`, `AnalyzeResult`, `AnalyzeReport`, `RepairResult` to `media_validation_engine.rs`
- Add `disposition` field to `MediaValidationResult` (default to `Healthy`)
- Run `cargo check` to verify compilation

### Step 2: Extract `analyze_single` from `validate_single_impl` (no behavior change)
- Create `analyze_single` method that runs the analysis portion of `validate_single_impl` (Phase 1 quick check + Phase 2 deep checks + Phase 9 additional checks + classification) WITHOUT any repair attempts
- Return `AnalyzeResult` instead of `MediaValidationResult`
- Run `cargo check` to verify compilation

### Step 3: Extract `repair_single` from `validate_single_impl` (no behavior change)
- Create `repair_single` method that takes a `DamageClassification` and performs the targeted repair (subtitle → timestamp → container → re-encode)
- Return `RepairResult` instead of `MediaValidationResult`
- Run `cargo check` to verify compilation

### Step 4: Extract `revalidate_single` (no behavior change)
- Create `revalidate_single` method that runs the full validation suite on a repaired file path
- Return `MediaValidationResult`
- Run `cargo check` to verify compilation

### Step 5: Implement batch methods
- Implement `analyze_all`, `repair_damaged`, `revalidate_repaired` using the per-file methods
- Run `cargo check` to verify compilation

### Step 6: Refactor `validate_input_files` to use new phases
- Replace the current `validate_batch` call with the 3-phase sequence
- Ensure backward-compatible `MediaValidationReport` output
- Run `cargo check` to verify compilation

### Step 7: Refactor `start_merge` to use new phases
- Replace the current coupled validation block with the 4-phase pipeline
- Add proper progress events for each phase
- Add per-phase timing to the profiler
- Run `cargo check` to verify compilation

### Step 8: Update UI progress events
- Emit phase-specific progress events for the frontend
- Test with the Tauri dev server

---

## 9. Risk Assessment

| Risk | Impact | Mitigation |
|---|---|---|
| Analysis phase takes too long for large playlists | Medium | Phase 1 analysis is already the existing validation — same speed. The benefit is that repair is now isolated. |
| Repair decisions change without full context | Low | Actually IMPROVED — Phase 1 sees the full damage picture before any repair is attempted. |
| Backward compatibility breaks | High | `validate_input_files` API remains unchanged. Internal refactoring only. |
| Revalidation misses edge cases | Medium | `revalidate_single` runs the same full validation suite as the original. No reduction in coverage. |
| UI progress events break frontend | Low | Events use the same `merge-progress` channel with new phase identifiers. Frontend gracefully handles unknown phase names. |

---

## 10. Expected Benefits

| Metric | Before | After |
|---|---|---|
| Healthy file processing | Analyze + (no repair) | Analyze only (same) |
| Damaged file processing | Analyze + repair + re-analyze ALL files | Analyze ALL + repair DAMAGED + revalidate REPAIRED only |
| UI clarity | "Analyzing... Repairing... Analyzing..." | "Analyzing 134... Repairing 2... Verifying 2... Merging 134" |
| Repair decision quality | Based on partial analysis (Phase 1 quick check only) | Based on FULL analysis (all checks completed) |
| Redundant work | Full revalidation of all files after any repair | Revalidation of repaired files only |

---

## 11. Files to Modify

| File | Changes |
|---|---|
| `src-tauri/src/ffmpeg/media_validation_engine.rs` | Add new types, extract `analyze_single`/`repair_single`/`revalidate_single`, implement batch methods |
| `src-tauri/src/commands/merge.rs` | Restructure `start_merge` into 4 phases, update progress events, update profiler |
| `src/app/types/index.ts` | Add TypeScript types for new progress events (optional, for frontend) |

---

## 12. Testing Strategy

### Unit Tests
- `analyze_single` returns correct `FileDisposition` for known-good and known-damaged test files
- `repair_single` dispatches to the correct repair function based on `DamageClassification`
- `revalidate_single` confirms repaired file passes full validation
- `validate_input_files` backward-compatible output matches existing behavior

### Integration Tests (Certification Framework)
- Phase H (Stress Tests): Run 50/100/200 file merges through the new pipeline
- Phase A (FastMKV/SmartMKV Parity): Verify output quality matches before/after refactoring
- Phase E (Recovery): Verify repair escalation still works (subtitle → timestamp → container → re-encode)

### Runtime Validation
- Run the 134-file playlist through the new pipeline
- Verify the profiler output shows clean phase separation
- Verify no healthy files enter the repair pipeline
- Verify repaired files pass revalidation
