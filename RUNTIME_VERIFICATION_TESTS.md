# Runtime Verification Test Suite

**Purpose:** Convert remaining 🟡 → ✅ or ❌ with real exports
**Status:** READY FOR EXECUTION

---

## Test Prerequisites

```
App: Fresh build with all fixes applied
OS: Windows
Debug: OFF (DEBUG = false)
Videos: 7+ mixed files in 2+ folders
Subtitles: Optional .srt files for subtitle tests
```

---

## TEST R1: Reports Metadata Verification

### R1A: SmartMKV Report

**Steps:**
1. Import 7 videos (2 folders, 3-4 files each)
2. Enable Cards ON (PerVideo, 2s, #00CCCC)
3. Select SmartMKV mode
4. Run merge
5. Open merge report

**Verify:**
```
□ Segment count = 13 (7 videos + 6 cards)
□ Cards marked as "Canvas" badge
□ Cards have correct color (#00CCCC)
□ Card duration = 2.0s
□ Video durations match source files
□ No missing segments
□ Thumbnails present (no 404)
□ Folder headers present
```

**Console Check:**
```javascript
// In DevTools, check:
// 1. No 404 errors for thumbnails
// 2. No "Cannot read property" errors
```

**Record Result:** PASS / FAIL

---

### R1B: FastMKV Report

**Steps:**
1. Import same 7 videos
2. Enable Cards ON (same settings)
3. Select FastMKV mode
4. Run merge
5. Open merge report

**Verify:**
```
□ Segment count = 13 (same as SmartMKV)
□ Cards marked as "Canvas" badge ← CRITICAL (was broken)
□ Cards have correct color (#00CCCC)
□ Card duration = 2.0s
□ Video durations match source files
□ is_card flags correct in report
```

**Compare with R1A:**
```
□ Same segment count
□ Same card positions
□ Same durations
□ Report structure identical
```

**Record Result:** PASS / FAIL

---

### R1C: Cards OFF Report

**Steps:**
1. Import 7 videos
2. Cards OFF
3. Run SmartMKV
4. Open report

**Verify:**
```
□ Segment count = 7 (no cards)
□ All entries are videos
□ No "Canvas" badges
□ No card colors shown
```

**Record Result:** PASS / FAIL

---

## TEST R2: Thumbnails Verification

### R2A: Thumbnail Lifecycle

**Steps:**
1. Complete merge with Cards ON
2. Open report immediately
3. Check all thumbnails load
4. Close report
5. Wait 10 seconds
6. Reopen report
7. Check thumbnails still load
8. Check browser console

**Verify:**
```
□ No 404 errors on first open
□ No 404 errors on reopen
□ All video thumbnails visible
□ Card thumbnails visible (if applicable)
□ No broken image icons
```

**Console Check:**
```javascript
// Should NOT see:
// "Failed to load resource: the server responded with a status of 404"
```

**Record Result:** PASS / FAIL

---

### R2B: Thumbnail After Playlist Clear

**Steps:**
1. Complete merge
2. Open report (thumbnails load)
3. Clear playlist (new playlist)
4. Reopen report

**Verify:**
```
□ Thumbnails may be cleared (expected)
□ No app crash
□ Graceful handling (fallback icon)
```

**Record Result:** PASS / FAIL

---

## TEST R3: FastMKV Pipeline Verification

### R3A: FastMKV + Cards + PerVideo

**Steps:**
1. Import 7 videos
2. Cards ON, PerVideo, 2s
3. FastMKV mode
4. Run merge

**Verify:**
```
□ Merge completes successfully
□ Output file created
□ Card count = 6
□ Cards visible in output video
□ Output duration = sum(videos) + 12s
□ No FFmpeg errors in logs
```

**Record Result:** PASS / FAIL

---

### R3B: FastMKV + Cards + PerFolder

**Steps:**
1. Import 7 videos (2+ folders)
2. Cards ON, PerFolder
3. FastMKV mode
4. Run merge

**Verify:**
```
□ Cards at folder boundaries only
□ Same card count as SmartMKV PerFolder
□ No orphan cards
```

**Record Result:** PASS / FAIL

---

### R3C: FastMKV Path Handling

**Steps:**
1. Import files with long paths (90+ chars)
2. FastMKV + Cards ON
3. Run merge

**Verify:**
```
□ No FFmpeg path errors
□ Card files created successfully
□ Output created successfully
□ No "\?" path errors
```

**Record Result:** PASS / FAIL

---

## TEST R4: Splits Verification

### R4A: Folder Split + Cards

**Steps:**
1. Import 7 videos (2+ folders)
2. Cards ON, PerVideo
3. Folder Split enabled
4. Run merge

**Verify:**
```
□ Multiple output files (one per folder)
□ Cards within each part
□ No orphan cards (cards without videos)
□ No card-video separation across parts
□ Each part report correct
```

**Record Result:** PASS / FAIL

---

### R4B: Duration Split + Cards

**Steps:**
1. Import 7 videos (~5 min total)
2. Cards ON, PerVideo (2s each)
3. Duration Split: 2 minutes
4. Run merge

**Verify:**
```
□ ~3 output files
□ Cards distributed correctly
□ No split mid-card
□ Duration per part ≈ 2 min
```

**Record Result:** PASS / FAIL

---

### R4C: Count Split + Cards

**Steps:**
1. Import 7 videos
2. Cards ON, PerVideo
3. Count Split: 3 files per part
4. Run merge

**Verify:**
```
□ 3 output files (3+3+1)
□ Cards within each part
□ No orphan cards
```

**Record Result:** PASS / FAIL

---

### R4D: Folder + Parts + Cards

**Steps:**
1. Import 7 videos (2 folders)
2. Cards ON, PerVideo
3. Folder + Parts: 2 parts per folder
4. Run merge

**Verify:**
```
□ Multiple parts per folder
□ Cards within each part
□ No cross-part card separation
```

**Record Result:** PASS / FAIL

---

## TEST R5: Subtitle + Cards Verification

### R5A: Subtitles + Cards ON

**Steps:**
1. Import 7 videos
2. Import 1+ .srt subtitle files
3. Cards ON, PerVideo
4. Subtitle mode: Embed
5. Run SmartMKV

**Verify:**
```
□ Cards rendered correctly
□ Subtitles embedded correctly
□ Subtitle timing correct
□ No subtitle-card interference
```

**Record Result:** PASS / FAIL

---

### R5B: Subtitles + Cards + Split

**Steps:**
1. Import 7 videos + subtitles
2. Cards ON
3. Folder Split enabled
4. Run merge

**Verify:**
```
□ Subtitles in each part
□ Cards in each part
□ Subtitle indexes correct per part
```

**Record Result:** PASS / FAIL

---

## TEST R6: Progress Events Verification

### R6A: SmartMKV Progress

**Steps:**
1. Import 7 videos (mix compatible/incompatible)
2. Cards ON
3. SmartMKV mode
4. Watch progress during merge

**Verify:**
```
□ Phase labels correct (Analysing → Validating → Normalising → Merging)
□ Current file name shown
□ Progress % updates
□ Normalization dashboard shows:
  - Files Scanned: 7
  - Needs Normalization: X (actual count)
  - Already Compatible: Y (7-X)
□ No counter mismatch (was showing 8 with 7 files)
```

**Record Result:** PASS / FAIL

---

### R6B: FastMKV Progress

**Steps:**
1. Import 7 videos
2. Cards ON
3. FastMKV mode
4. Watch progress

**Verify:**
```
□ Phase labels correct (FastMKV specific)
□ Progress % updates
□ Completion event fires
□ No hanging progress
```

**Record Result:** PASS / FAIL

---

### R6C: Progress Parity

**Steps:**
1. Run SmartMKV with 7 videos
2. Note progress events
3. Run FastMKV with same 7 videos
4. Note progress events
5. Compare

**Verify:**
```
□ Same number of progress events
□ Same phase transitions
□ Same file names shown
□ Completion fires in both
```

**Record Result:** PASS / FAIL

---

## Certification Summary

| Test | Category | Status |
|------|----------|--------|
| R1A | SmartMKV Report | PENDING |
| R1B | FastMKV Report | PENDING |
| R1C | Cards OFF Report | PENDING |
| R2A | Thumbnail Lifecycle | PENDING |
| R2B | Thumbnail After Clear | PENDING |
| R3A | FastMKV + Cards PerVideo | PENDING |
| R3B | FastMKV + Cards PerFolder | PENDING |
| R3C | FastMKV Path Handling | PENDING |
| R4A | Folder Split + Cards | PENDING |
| R4B | Duration Split + Cards | PENDING |
| R4C | Count Split + Cards | PENDING |
| R4D | Folder + Parts + Cards | PENDING |
| R5A | Subtitles + Cards | PENDING |
| R5B | Subtitles + Cards + Split | PENDING |
| R6A | SmartMKV Progress | PENDING |
| R6B | FastMKV Progress | PENDING |
| R6C | Progress Parity | PENDING |

---

## Instructions

1. Run each test in order
2. Check the boxes as you verify
3. Record PASS/FAIL for each test
4. Report any FAIL items with details
5. Update this document with results
