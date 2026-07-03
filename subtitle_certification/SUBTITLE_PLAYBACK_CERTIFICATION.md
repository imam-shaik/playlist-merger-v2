# Subtitle Playback Certification Report

**Date:** 2026-07-03
**Status:** ✅ CERTIFICATION PASS — All modes produce identical timeline, 0ms drift

---

## Summary

The subtitle double-shift bug in `generate_merged_srt_with_rebase()` has been fixed.
All subtitle modes now produce identical timestamps, verified at runtime.

---

## Fix Applied

**File:** `src-tauri/src/ffmpeg/mod.rs:522-660`

**Change:** Removed manual pre-shifting of timestamps from `generate_merged_srt_with_rebase()`.
The function now writes raw SRTs with duration directives and lets FFmpeg's concat demuxer
perform the single, correct rebase.

**Before:**
```
Manual offset + Concat demuxer offset = Double shift
```

**After:**
```
Concat demuxer offset only = Correct single rebase
```

---

## Runtime Certification Results

### Test Corpus: Non-Uniform Durations (7s + 12s + 9s)

| Segment | Video Duration | Cue Position | Expected | Actual | Drift |
|---------|---------------|--------------|----------|--------|-------|
| Video A | 7.0s | 3s | 00:00:03,000 | 00:00:03,000 | 0ms |
| Video B | 12.0s | 3s | 00:00:10,000 | 00:00:10,000 | 0ms |
| Video C | 9.0s | 3s | 00:00:22,000 | 00:00:22,000 | 0ms |

### Mode-by-Mode Verification

| Mode | Output | Cue 1 | Cue 2 | Cue 3 | Status |
|------|--------|-------|-------|-------|--------|
| Embed (MKV extracted) | `extracted_embed_fixed.srt` | 03s | 10s | 22s | ✅ PASS |
| Export SRT | `export_fixed.srt` | 03s | 10s | 22s | ✅ PASS |
| Burn SRT | `burn_fixed.srt` | 03s | 10s | 22s | ✅ PASS |
| generate_merged_srt_with_rebase | `fixed_result.srt` | 03s | 10s | 22s | ✅ PASS |

**Maximum Drift:** 0ms across all modes
**Average Drift:** 0ms across all modes

### Multi-Cue Test (2 cues per segment)

| Cue | Segment | Expected | Actual | Drift |
|-----|---------|----------|--------|-------|
| A1 | Video A (8s) | 00:00:02,000 | 00:00:02,000 | 0ms |
| A2 | Video A (8s) | 00:00:06,000 | 00:00:06,000 | 0ms |
| B1 | Video B (10s) | 00:00:11,000 | 00:00:11,000 | 0ms |
| B2 | Video B (10s) | 00:00:15,000 | 00:00:15,000 | 0ms |
| C1 | Video C (7s) | 00:00:19,000 | 00:00:19,000 | 0ms |
| C2 | Video C (7s) | 00:00:22,000 | 00:00:22,000 | 0ms |

---

## Forensic Timeline Certification

```
╔════════════════════════════════════════════════════════════════╗
║   FORENSIC TIMELINE CERTIFICATION REPORT                     ║
╚════════════════════════════════════════════════════════════════╝

  ┌─────────────────────────────────────────────────────────────────┐
  │ Segment             │ Video Start │ Sub Start │ Difference      │
  ├─────────────────────────────────────────────────────────────────┤
  │  1. A-Intro            │       2.0s  │      2.0s  │   +0.000s ✅     │
  │  2. A-Outro            │       5.0s  │      5.0s  │   +0.000s ✅     │
  │  3. B-Start            │       8.0s  │      8.0s  │   +0.000s ✅     │
  │  4. B-Mid              │      13.0s  │     13.0s  │   +0.000s ✅     │
  │  5. B-End              │      17.0s  │     17.0s  │   +0.000s ✅     │
  │  6. C-Open             │      19.5s  │     19.5s  │   +0.000s ✅     │
  │  7. C-Close            │      23.0s  │     23.0s  │   +0.000s ✅     │
  └─────────────────────────────────────────────────────────────────┘

  Maximum Drift: 0.000s (0ms)
  Average Drift: 0.000s (0ms)

  ╔══════════════════════════════════════════════════════════════╗
  ║                    CERTIFICATION: PASS                     ║
  ╚══════════════════════════════════════════════════════════════╝
```

---

## Regression Tests Added

**File:** `src-tauri/src/ffmpeg/subtitle_sync_certification_test.rs`

| Test | Description | Status |
|------|-------------|--------|
| `test_uniform_durations` | 10+10+10s, cues at 5s each | ✅ PASS |
| `test_nonuniform_durations` | 7+12+9s, cues at 3s each | ✅ PASS |
| `test_multi_cue_segments` | 2 cues per segment | ✅ PASS |
| `test_missing_subtitle_segment` | None → dummy handling | ✅ PASS |
| `test_very_short_segments` | 0.5s segments | ✅ PASS |
| `test_forensic_timeline_certification` | Full certification report | ✅ PASS |

---

## Files Changed

| File | Change |
|------|--------|
| `src-tauri/src/ffmpeg/mod.rs` | Fixed `generate_merged_srt_with_rebase()` — removed pre-shifting |
| `src-tauri/src/ffmpeg/subtitle_sync_certification_test.rs` | Fixed incorrect comment, added 6 regression tests |

---

## What Was NOT Changed

- SmartMKV repair pipeline
- Normalization pipeline
- Media validation
- Merge ordering
- Duration calculation
- Checkpoint/resume
- `write_subtitle_concat_list()` (was already correct)
- `generate_merged_srt()` (was already correct)

---

## Conclusion

The subtitle timeline synchronization bug has been **fixed and verified at runtime**.
All subtitle modes (Embed, Burn, Export SRT, SRT Merge Only) now produce identical
timestamps with **0ms maximum drift** from the video timeline.
