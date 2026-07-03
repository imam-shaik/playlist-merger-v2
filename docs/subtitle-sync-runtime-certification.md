# Subtitle Sync Runtime Certification Checklist

## Production Certification Status: 🔬 UNIT TESTS COMPLETE | ⏳ AWAITING RUNTIME VERIFICATION

**Feature**: Part-wise merge subtitle synchronization
**Implementation**: `create_rebased_srt_for_part()` in `concat.rs`
**Date**: 2026-06-25

---

## Executive Summary

This document certifies the subtitle synchronization fix for part-wise merge operations. The fix addresses a bug where FFmpeg's concat demuxer preserved original SRT timestamps instead of rebasing them to the part-relative timeline.

**Root Cause**: `generate_merged_srt()` used FFmpeg concat demuxer without timestamp rebasing
**Fix**: Pre-rebase SRT files using `create_rebased_srt_for_part()` before FFmpeg concat
**Unit Test Status**: ✅ 23/23 tests passing (see below)

---

## Unit Test Certification (✅ Complete)

### Test Modules

| Module | Tests | Status |
|--------|-------|--------|
| `subtitle_sync_certification_test` | 11 | ✅ PASS |
| `subtitle_randomized_stress_test` | 7 | ✅ PASS |
| `subtitle_runtime_verifier` | 5 | ✅ PASS |
| **TOTAL** | **23** | ✅ |

### Tests Added (2026-06-25)

**cue_count_integrity_test module (4 tests):**
```rust
test_cue_count_preserved_no_loss ... ok
test_cue_numbering_sequential_per_part ... ok
test_multipart_cue_distribution ... ok
test_boundary_crossing_cues_trimmed_not_discarded ... ok
```

**subtitle_randomized_stress_test module (7 tests):**
```rust
test_randomized_stress_cue_integrity ... ok
test_randomized_stress_no_duplicates ... ok
test_randomized_stress_no_overlaps ... ok
test_randomized_stress_no_timestamp_reversal ... ok
test_randomized_stress_sequential_numbering ... ok
test_randomized_boundary_crossing_cues ... ok
test_randomized_stress_comprehensive ... ok
```

**subtitle_runtime_verifier module (5 tests):**
```rust
test_verify_part_within_tolerance ... ok
test_verify_part_exceeds_tolerance ... ok
test_certification_summary_all_pass ... ok
test_format_verification_report ... ok
test_timestamp_conversion_roundtrip ... ok
```

---

## Automated Runtime Verifier

A programmatic runtime verifier is available at `src/ffmpeg/subtitle_runtime_verifier.rs`:

```rust
use crate::ffmpeg::subtitle_runtime_verifier::SubtitleRuntimeVerifier;

// Create verifier with 50ms tolerance
let verifier = SubtitleRuntimeVerifier::new(50);

// Verify each part's SRT
let result = verifier.verify_part_srt(
    part_index: 3,
    expected_first_cue_ms: 4220,
    expected_last_cue_ms: 598120,
    srt_path: Path::new("output/part_3.srt"),
    expected_cue_count: 250,
);

// Generate certification report
let summary = verifier.generate_certification_summary(&[result], modes);
println!("{}", verifier.format_certification_report(&summary));
```

### Sample Certification Report Output

```
======================================
SUBTITLE RUNTIME CERTIFICATION
======================================

Parts Tested:
5

Subtitle Modes:
Embed
Burn
ExportSrt
SrtMergeOnly

Cue Integrity:
PASS

Cue Count:
PASS

Cue Numbering:
PASS

Boundary Cues:
PASS

Timeline Rebase:
PASS

Audio Sync:
PASS

Video Sync:
PASS

Maximum Subtitle Drift:
8 ms

Average Drift:
2 ms

CERTIFICATION:
PASS
======================================
```

---

## Test Environments

### Players
- [ ] VLC (latest)
- [ ] MPV
- [ ] MPC-HC (optional)

### FFmpeg Versions
- [ ] System FFmpeg
- [ ] Bundled FFmpeg (if applicable)

---

## Part-wise Merge Tests

### Basic Part Counts

| Test Case | Videos | Parts | Status | Notes |
|-----------|--------|-------|--------|-------|
| 2 videos → 2 parts | 2 | 2 | ⏳ | |
| 3 videos → 5 parts | 3 | 5 | ⏳ | |
| 30+ videos → multiple parts | 30+ | 10+ | ⏳ | Stress test |

### Merge Modes

| Test Case | Mode | Status | Notes |
|-----------|------|--------|-------|
| 2 videos → 5 parts + SmartMKV | SmartMKV | ⏳ | |
| 2 videos → 5 parts + Lossless | Lossless | ⏳ | |
| 2 videos → 5 parts + Custom/Re-encode | Custom | ⏳ | |

---

## Subtitle Modes

### All modes must be tested for EACH part count configuration

| Mode | 2-part Test | 5-part Test | 10+ part Test |
|------|-------------|-------------|---------------|
| **Embed** | ⏳ | ⏳ | ⏳ |
| **Burn** | ⏳ | ⏳ | ⏳ |
| **ExportSrt** | ⏳ | ⏳ | ⏳ |
| **SrtMergeOnly** | ⏳ | ⏳ | ⏳ |

---

## Sync Verification (Per Part)

For each generated part, verify subtitle synchronization at three points:

### Part 1

| Checkpoint | Time Position | Expected | Actual | Status |
|------------|--------------|----------|--------|--------|
| First subtitle sync | 0-5% of video | Subtitle appears WITH speech | | ⏳ |
| Middle subtitle sync | 45-55% | Subtitle synchronized | | ⏳ |
| Last subtitle sync | 95-100% | Subtitle ends WITH video | | ⏳ |

### Part 2 (and subsequent parts)

| Checkpoint | Time Position | Expected | Actual | Status |
|------------|--------------|----------|--------|--------|
| First subtitle sync | 0-5% of part | Subtitle starts at 00:00 | | ⏳ |
| Middle subtitle sync | 45-55% | Subtitle synchronized | | ⏳ |
| Last subtitle sync | 95-100% | Subtitle ends WITH part | | ⏳ |

---

## Integrity Checks

### Cue Count Integrity ⭐ CRITICAL

**Unit Test Status**: ✅ VERIFIED (test_cue_count_preserved_no_loss passes)

```
Original SRT total cues = Sum of (Part 1 + Part 2 + ... + Part N)
```

| Test | Original Cues | Part 1 | Part 2 | Part 3 | Part 4 | Total | Status |
|------|--------------|--------|--------|--------|--------|-------|--------|
| 2-part test | 1000 | 250 | 250 | - | - | 1000 | ⏳ |
| 5-part test | 1000 | 200 | 200 | 200 | 200 | 1000 | ⏳ |
| 10+ part test | 1000 | 100 | 100 | 100 | 100 | 1000 | ⏳ |

### No Gaps Between Parts

```
Part 1 last subtitle ends at: XX:XX
Part 2 first subtitle starts at: 00:00

Expected: Gap of 0 seconds (cues should be continuous)
```

| Test | Part 1 Last End | Part 2 First Start | Gap | Status |
|------|-----------------|-------------------|-----|--------|
| 2-part test | | | 0s | ⏳ |
| 5-part test | | | 0s | ⏳ |

### No Overlaps Between Parts

```
Part 1 last subtitle ends at: XX:XX
Part 2 first subtitle starts at: YY:YY

Expected: No overlap (Part 2 should start at 00:00)
```

| Test | Part 1 Last End | Part 2 First Start | Overlap | Status |
|------|-----------------|-------------------|---------|--------|
| 2-part test | | | None | ⏳ |

### Cue Numbering Preserved

Verify cue indices are sequential within each part:

```
Part 1:
  Cue 1, Cue 2, Cue 3, ... Cue N

Part 2:
  Cue 1, Cue 2, Cue 3, ... Cue M
```

| Test | Part 1 | Part 2 | Part 3 | Status |
|------|--------|--------|--------|--------|
| Sequential numbering | ⏳ | ⏳ | ⏳ | |

---

## Edge Case Tests

### Boundary-Crossing Cue

**Policy**: Cues that cross part boundaries are trimmed (visible portion kept).

```
Original cue: 58.500 --> 60.800
Part boundary: 60.000

Output after rebase:
  Start: max(58.5 - 60, 0) = 0.000
  End: 60.8 - 60 = 0.800

Result: 0.000 --> 0.800 (cue trimmed, not discarded)
```

| Test | Cue Before | Boundary | Cue After | Status |
|------|-----------|----------|-----------|--------|
| Unit test | 58.5s | 60.0s | 0.8s | ✅ PROVEN |
| Cue starts before, ends after | 59.5s | 60.0s | 1.2s | ⏳ |

### Empty Subtitle File

| Test | Expected Behavior | Status |
|------|-------------------|--------|
| Input video has no SRT | Part output has no subtitle track | ⏳ |
| Mixed: some videos have subs, some don't | Only videos with subs get subtitle | ⏳ |

### UTF-8 / Emoji

| Test | Subtitle Content | Status |
|------|------------------|--------|
| Japanese | 日本語字幕 | ⏳ |
| Korean | 한국어 자막 | ⏳ |
| Arabic | نص عربي | ⏳ |
| Emoji | Hello 😊 World | ⏳ |

### Multiple Subtitle Tracks

| Test | Tracks | Status |
|------|--------|--------|
| English only | 1 | ⏳ |
| English + Spanish | 2 | ⏳ |
| English + Japanese + Spanish | 3 | ⏳ |

**Verification**: Each track should be rebased independently.

### Very Large Subtitle Files

| Test | Cue Count | Status |
|------|-----------|--------|
| 100 cues | 100 | ⏳ |
| 1000 cues | 1000 | ⏳ |
| 5000+ cues | 5000+ | ⏳ |

### Files Without Subtitles

| Test | Expected | Status |
|------|----------|--------|
| Video without SRT | No subtitle track | ⏳ |

---

## ExportSrt Verification

For each exported SRT file:

1. Open in text editor
2. Verify first cue starts at approximately 00:00 (within tolerance)
3. Verify timestamps are NOT at original video timeline

```
WRONG (before fix):
  Part 2 SRT: 10:00, 10:05, 10:15, ...

CORRECT (after fix):
  Part 2 SRT: 00:00, 00:05, 00:15, ...
```

| Test | Part 1 First Cue | Part 2 First Cue | Part 3 First Cue | Status |
|------|------------------|------------------|------------------|--------|
| 3-part test | ~00:00 | ~00:00 | ~00:00 | ⏳ |

---

## Burn Subtitle Verification

For burned subtitles:

1. Watch each part in player (VLC/MPV)
2. Listen for spoken dialogue
3. Verify subtitle appears WITH speech, not before or after

| Test | Part | Speech Time | Subtitle Time | Difference | Status |
|------|------|-------------|---------------|------------|--------|
| 2-part | 1 | | | <100ms | ⏳ |
| 2-part | 2 | | | <100ms | ⏳ |
| 5-part | 1 | | | <100ms | ⏳ |
| 5-part | 2 | | | <100ms | ⏳ |
| 5-part | 3 | | | <100ms | ⏳ |
| 5-part | 4 | | | <100ms | ⏳ |
| 5-part | 5 | | | <100ms | ⏳ |

---

## Embedded Subtitle Verification

Extract and verify embedded subtitles:

```bash
# Extract subtitle from part 2
ffmpeg -i part2.mp4 -map 0:s:0 -c:s srt part2_extracted.srt
```

Then compare timestamps with video.

| Test | Extracted First Cue | Video Time | Status |
|------|---------------------|------------|--------|
| Part 1 | | 0:00 | ⏳ |
| Part 2 | | 0:00 | ⏳ |

---

## SmartMKV Specific Tests

| Test | Description | Status |
|------|-------------|--------|
| FPS normalization | Ensure subtitle sync after FPS change | ⏳ |
| Resolution change | Ensure subtitle sync after resize | ⏳ |
| Codec change | H264→H265, subtitle sync maintained | ⏳ |

---

## Sign-off Section

### Unit Test Evidence (✅ Complete)

```
cargo test --lib subtitle_sync
...
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 208 filtered out

Tests:
- test_prove_direct_split_is_correct
- test_prove_concat_demuxer_preserves_timestamps
- test_offset_type_is_constant_not_growing
- test_mathematical_proof_rebase_needed
- test_part_boundary_subtitle_continuity
- test_all_subtitle_modes_affected
- test_direct_split_uses_correct_logic
- test_cue_count_preserved_no_loss
- test_cue_numbering_sequential_per_part
- test_multipart_cue_distribution
- test_boundary_crossing_cues_trimmed_not_discarded
```

### Implementation Review

| Item | Reviewer | Date | Signature |
|------|----------|------|-----------|
| Code review | | | |
| Architecture | | | |
| Unit tests | Kilo | 2026-06-25 | ✅ Complete |
| Integration test | | | |

### Runtime Certification

| Test Suite | Tester | Date | Result |
|------------|--------|------|--------|
| 2-part basic | | | |
| 5-part basic | | | |
| 10+ part stress | | | |
| Edge cases | | | |
| Player verification | | | |

### Final Approval

| Role | Name | Date | Status |
|------|------|------|--------|
| Developer | | | |
| QA | | | |
| Product Owner | | | |

---

## Known Limitations

1. **Boundary-crossing cues**: Trimmed, not discarded (policy decision documented above)
2. **ASS/SSA subtitles**: Not tested (SRT-only implementation)
3. **VFR (Variable Frame Rate)**: May have additional sync challenges

---

## Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-06-25 | | Initial certification checklist |