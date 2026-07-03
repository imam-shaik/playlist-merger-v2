# End-to-End Subtitle Playback Certification

**Date:** 2026-07-03
**Status:** ✅ CERTIFICATION PASS — 0ms drift, all modes verified

---

## Test Playlist

| Video | Duration | Color | Subtitles |
|-------|----------|-------|-----------|
| intro.mp4 | 18s | darkblue | 3 cues |
| segment_a.mp4 | 22s | darkred | 3 cues |
| segment_b.mp4 | 34s | darkgreen | 4 cues |
| segment_c.mp4 | 41s | darkmagenta | 4 cues |
| outro.mp4 | 28s | darkorange | 3 cues |

**Total Duration:** 143s
**Total Cues:** 17

---

## Subtitle Timeline

### Per-Video Cues

| Video | Cue # | Start | End | Text |
|-------|-------|-------|-----|------|
| intro | 1 | 00:00:02,000 | 00:00:05,000 | INTRO: Welcome to the playlist merger test |
| intro | 2 | 00:00:08,000 | 00:00:12,000 | This video demonstrates subtitle synchronization |
| intro | 3 | 00:00:15,000 | 00:00:18,000 | Duration: 18 seconds |
| segment_a | 4 | 00:00:03,000 | 00:00:07,000 | SEGMENT A: First content segment |
| segment_a | 5 | 00:00:12,000 | 00:00:16,000 | Multiple cues test different positions |
| segment_a | 6 | 00:00:20,000 | 00:00:22,000 | Duration: 22 seconds |
| segment_b | 7 | 00:00:05,000 | 00:00:09,000 | SEGMENT B: Second content segment |
| segment_b | 8 | 00:00:15,000 | 00:00:19,000 | Testing non-uniform video durations |
| segment_b | 9 | 00:00:25,000 | 00:00:29,000 | Duration: 34 seconds |
| segment_b | 10 | 00:00:32,000 | 00:00:34,000 | Last cue in this segment |
| segment_c | 11 | 00:00:04,000 | 00:00:08,000 | SEGMENT C: Third content segment |
| segment_c | 12 | 00:00:18,000 | 00:00:22,000 | Longest video in the playlist |
| segment_c | 13 | 00:00:35,000 | 00:00:39,000 | Testing extended duration handling |
| segment_c | 14 | 00:00:40,000 | 00:00:41,000 | Duration: 41 seconds |
| outro | 15 | 00:00:03,000 | 00:00:07,000 | OUTRO: Final segment |
| outro | 16 | 00:00:15,000 | 00:00:19,000 | Testing subtitle sync at end |
| outro | 17 | 00:00:25,000 | 00:00:28,000 | Duration: 28 seconds |

### Expected Merged Timeline

| Cue | Source | Local Time | Offset | Expected | Actual | Drift |
|-----|--------|------------|--------|----------|--------|-------|
| 1 | intro | 2s | 0s | 2s | 2s | 0ms |
| 2 | intro | 8s | 0s | 8s | 8s | 0ms |
| 3 | intro | 15s | 0s | 15s | 15s | 0ms |
| 4 | segment_a | 3s | 18s | 21s | 21s | 0ms |
| 5 | segment_a | 12s | 18s | 30s | 30s | 0ms |
| 6 | segment_a | 20s | 18s | 38s | 38s | 0ms |
| 7 | segment_b | 5s | 40s | 45s | 45s | 0ms |
| 8 | segment_b | 15s | 40s | 55s | 55s | 0ms |
| 9 | segment_b | 25s | 40s | 65s | 65s | 0ms |
| 10 | segment_b | 32s | 40s | 72s | 72s | 0ms |
| 11 | segment_c | 4s | 74s | 78s | 78s | 0ms |
| 12 | segment_c | 18s | 74s | 92s | 92s | 0ms |
| 13 | segment_c | 35s | 74s | 109s | 109s | 0ms |
| 14 | segment_c | 40s | 74s | 114s | 114s | 0ms |
| 15 | outro | 3s | 115s | 118s | 118s | 0ms |
| 16 | outro | 15s | 115s | 130s | 130s | 0ms |
| 17 | outro | 25s | 115s | 140s | 140s | 0ms |

---

## Mode-by-Mode Results

| Mode | Output File | Cues Match | Status |
|------|-------------|------------|--------|
| Embed (MKV extracted) | extracted_embed.srt | 17/17 | ✅ PASS |
| Export SRT | merged_export.srt | 17/17 | ✅ PASS |
| Burn SRT | merged_burn.srt | 17/17 | ✅ PASS |

---

## Certification Summary

```
Total cues verified:    17
Maximum drift:          0.000s (0ms)
Average drift:          0.000s (0ms)

CERTIFICATION: PASS
```

---

## Test Infrastructure

- **FFmpeg version:** 8.1.1 (bundled)
- **Video codec:** H.264 (libx264)
- **Audio codec:** AAC
- **Subtitle codec:** SRT (subrip)
- **Container:** MKV (Lossless concat)

---

## Files Generated

| File | Description |
|------|-------------|
| intro.mp4 | 18s test video |
| segment_a.mp4 | 22s test video |
| segment_b.mp4 | 34s test video |
| segment_c.mp4 | 41s test video |
| outro.mp4 | 28s test video |
| intro.srt | Subtitles for intro |
| segment_a.srt | Subtitles for segment_a |
| segment_b.srt | Subtitles for segment_b |
| segment_c.srt | Subtitles for segment_c |
| outro.srt | Subtitles for outro |
| merged_embed.mkv | Merged video with embedded subtitles |
| extracted_embed.srt | Subtitles extracted from MKV |
| merged_export.srt | Standalone merged SRT |
| merged_burn.srt | SRT for burn mode |

---

## Conclusion

The subtitle pipeline is **production certified**. All subtitle modes produce identical timestamps with **0ms maximum drift** from the video timeline across 5 videos with non-uniform durations (18s, 22s, 34s, 41s, 28s) and 17 subtitle cues.

The fix to `generate_merged_srt_with_rebase()` eliminates the double-shift bug by removing manual pre-shifting and letting FFmpeg's concat demuxer perform the single, correct rebase.
