# Subtitle Timeline Certification Report

**Date:** 2026-07-03
**Status:** ROOT CAUSE PROVEN WITH RUNTIME EVIDENCE
**Bug Severity:** HIGH — Affects Burn, SRT Export, and SRT Merge Only modes

---

## Executive Summary

The subtitle synchronization bug has been **proven with runtime evidence**. The root cause is a **double-shift bug** in `generate_merged_srt_with_rebase()` (mod.rs:541-660). This function pre-shifts subtitle timestamps by cumulative video durations, then feeds them through FFmpeg's concat demuxer with `duration` directives — causing the concat demuxer to rebase the already-shifted timestamps, resulting in a double offset.

**Affected modes:** Burn, SRT Export, SRT Merge Only
**Unaffected modes:** Embed (Lossless), Embed Fallback (video stream only)

---

## Runtime Test Results

### Test Corpus
- **Uniform:** 3 x 10s videos, each with 1 SRT cue at 5s
- **Non-uniform:** 7s + 12s + 9s videos, each with 1 SRT cue at 3s

### Embed Mode (Lossless) — ✅ CORRECT

```
Input:  videoA.srt @ 5s, videoB.srt @ 5s, videoC.srt @ 5s
Concat: file '.../videoA.srt' / duration 10 / file '.../videoB.srt' / duration 10 / ...
Output: Cue1 @ 00:00:05,000 / Cue2 @ 00:00:15,000 / Cue3 @ 00:00:25,000
```

Non-uniform:
```
Input:  videoA.srt @ 3s, videoB.srt @ 3s, videoC.srt @ 3s
Concat: duration 7.0 / duration 12.0 / duration 9.0
Output: Cue1 @ 00:00:03,000 / Cue2 @ 00:00:10,000 / Cue3 @ 00:00:22,000
Expected: 03s, 10s, 22s ← MATCH
```

### Export SRT (concat demuxer only) — ✅ CORRECT

```
Output: Cue1 @ 00:00:03,000 / Cue2 @ 00:00:10,000 / Cue3 @ 00:00:22,000
Expected: 03s, 10s, 22s ← MATCH
```

### generate_merged_srt_with_rebase — ❌ DOUBLE-SHIFTED

```
Input:  srtA_ps2 @ 03s (raw), srtB_ps2 @ 10s (pre-shifted by 7), srtC_ps2 @ 22s (pre-shifted by 19)
Concat: file '...srtA_ps2.srt' / duration 7 / file '...srtB_ps2.srt' / duration 12 / ...
Output: Cue1 @ 00:00:03,000 / Cue2 @ 00:00:17,000 / Cue3 @ 00:00:41,000
Expected: 03s, 10s, 22s
Actual:   03s, 17s, 41s ← DOUBLE-SHIFTED (0 + 7 = 7 extra)
```

**Drift per segment:**
| Segment | Expected | Actual | Drift |
|---------|----------|--------|-------|
| 1 | 03s | 03s | 0s |
| 2 | 10s | 17s | +7s (= dur[0]) |
| 3 | 22s | 41s | +19s (= dur[0]+dur[1]) |

---

## Root Cause Analysis

### The Bug: `generate_merged_srt_with_rebase()` (mod.rs:541-660)

```rust
// LINE 554-560: Compute cumulative offsets
let mut cumulative_offsets: Vec<f64> = Vec::new();
let mut cum: f64 = 0.0;
for i in 0..subs.len() {
    cumulative_offsets.push(cum);
    cum += durations[i];
}

// LINE 580-581: PRE-SHIFT each SRT by cumulative offset
timeline.add_offset(cumulative_offsets[i]);

// LINE 609-614: Build concat list with PRE-SHIFTED files + duration directives
writeln!(content, "file '{}'", escaped)?;
writeln!(content, "duration {}", durations[i])?;
```

**The concat demuxer's behavior with `duration` directives:**
1. Reads each SRT file
2. Adds the cumulative `duration` sum of all previous segments to each timestamp
3. Result: pre-shifted + rebase = double-shift

### Why Embed Mode Works

`write_subtitle_concat_list()` (mod.rs:366-424) writes **raw** SRTs with `duration` directives. The concat demuxer rebases the raw timestamps once — producing the correct output.

### Why Burn/Export/SRT Merge Fail

`generate_merged_srt_with_rebase()` pre-shifts timestamps, then feeds them through the same concat demuxer with `duration` directives — producing double-shifted output.

### The Existing Unit Test Has a WRONG Comment

`subtitle_sync_certification_test.rs` contains:
> "concat demuxer preserves the ORIGINAL timestamps"

**Runtime proof shows this is FALSE.** The concat demuxer DOES rebase timestamps when given `duration` directives.

---

## Affected Call Sites

| Location | Mode | Bug |
|----------|------|-----|
| merge.rs:2230 | SRT Merge Only | Double-shifted output SRT |
| merge.rs:5492 | Burn | Double-shifted burn SRT (subtitles in video are wrong) |
| merge.rs:6221 | Embed Fallback | Double-shifted external SRT |
| merge.rs:6270 | Export SRT alongside video | Double-shifted exported SRT |

---

## Recommended Fix

**Remove the pre-shifting from `generate_merged_srt_with_rebase()`.**

The function should write **raw** SRTs with `duration` directives to the concat list, identical to `write_subtitle_concat_list()`. The concat demuxer handles rebasing correctly.

### Option A: Rewrite `generate_merged_srt_with_rebase` (SAFEST)

Replace lines 554-614 with logic identical to `write_subtitle_concat_list`:
- Write raw SRT paths (no pre-shifting)
- Include `duration` directives
- Let the concat demuxer rebase

### Option B: Remove `generate_merged_srt_with_rebase` entirely

Replace all call sites with `write_subtitle_concat_list` + `generate_merged_srt`:
- Call `write_subtitle_concat_list` to build the concat list
- Call `generate_merged_srt` to run FFmpeg

### Option C: Keep pre-shifting but remove `duration` directives

If pre-shifting is desired (e.g., for validation), remove the `duration` lines from the concat list. The concat demuxer will use actual file content duration instead.

---

## Recommendation

**Implement Option A** — rewrite `generate_merged_srt_with_rebase` to match `write_subtitle_concat_list` behavior. This is the safest fix because:
1. Minimal code change
2. The concat demuxer already handles rebasing correctly (proven by runtime tests)
3. No behavioral change for Embed mode (which already uses `write_subtitle_concat_list`)
4. The fix can be verified with the same test corpus

---

## Files Referenced

- `src-tauri/src/ffmpeg/mod.rs:541-660` — `generate_merged_srt_with_rebase()` (BUGGY)
- `src-tauri/src/ffmpeg/mod.rs:366-424` — `write_subtitle_concat_list()` (CORRECT)
- `src-tauri/src/ffmpeg/mod.rs:433-530` — `generate_merged_srt()` (CORRECT)
- `src-tauri/src/commands/merge.rs:2230` — SRT merge only call site
- `src-tauri/src/commands/merge.rs:5484-5492` — Embed + Burn call sites
- `src-tauri/src/commands/merge.rs:6221` — Embed fallback call site
- `src-tauri/src/commands/merge.rs:6270` — Export SRT call site
- `subtitle_certification/` — Test fixtures and outputs
