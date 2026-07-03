# Agent 1 — Production Verification Guide

**Purpose:** Convert PENDING → PASS/FAIL with real video exports
**Scope:** Cards, Reports, Completion Events, Duration, Path Handling, Thumbnails, Splits

---

## Pre-Test Setup

### Required Materials

```
7 video files (mixed resolutions, codecs, fps)
Organized in 2+ folders (for folder split testing)
Total duration: 2-5 minutes each
```

### Test Environment

```
App: Fresh build with all fixes applied
OS: Windows (for \?\ path testing)
Debug: OFF (DEBUG = false in App.tsx)
```

---

## Phase 1: Cards Runtime Certification

### Test 1A: SmartMKV + Cards ON + PerVideo

**Steps:**
1. Import 7 videos
2. Enable Cards (color: #00CCCC)
3. Set Frequency: PerVideo
4. Set Duration: 2 seconds
5. Select SmartMKV mode
6. Run merge

**Expected Results:**
- Card count: 6 (skip first in PerVideo)
- Each card appears BEFORE each video (except first)
- Card duration: 2 seconds each
- Total output duration: sum(videos) + 12 seconds

**Verify in output:**
```
Video 1 (no card before)
Card 1
Video 2
Card 2
Video 3
Card 3
Video 4
Card 4
Video 5
Card 5
Video 6
Card 6
Video 7
```

**Record:**
- [ ] Card count = 6
- [ ] Cards have colored background
- [ ] Cards have text (folder name or "Video N")
- [ ] Total duration correct

---

### Test 1B: FastMKV + Cards ON + PerVideo

**Steps:**
1. Import same 7 videos
2. Enable Cards (same settings)
3. Select FastMKV mode
4. Run merge

**Expected Results:**
- Same as Test 1A
- Identical card count, positions, durations

**Record:**
- [ ] Card count = 6
- [ ] Output matches SmartMKV

---

### Test 1C: Cards ON + PerFolder

**Steps:**
1. Import 7 videos (2+ folders)
2. Set Frequency: PerFolder
3. Run SmartMKV
4. Run FastMKV

**Expected Results:**
- Cards only at folder boundaries
- Same card count in both modes

**Record:**
- [ ] SmartMKV card count = FastMKV card count
- [ ] Cards appear at folder changes

---

## Phase 2: Report Certification

### Test 2A: SmartMKV Report

**Steps:**
1. Complete Test 1A (SmartMKV + Cards)
2. Open merge report
3. Check segment list

**Verify:**
```
- All 7 videos listed
- All 6 cards listed
- Cards marked as "Canvas" or is_card=true
- Correct card colors (#00CCCC)
- Correct durations (2s for cards)
- Thumbnails present (no 404 errors)
```

**Record:**
- [ ] Video count = 7
- [ ] Card count = 6
- [ ] No 404 errors
- [ ] Durations correct

---

### Test 2B: FastMKV Report

**Steps:**
1. Complete Test 1B (FastMKV + Cards)
2. Open merge report
3. Check segment list

**Verify:**
- Same as Test 2A
- Cards correctly marked (is_card=true)

**Record:**
- [ ] Video count = 7
- [ ] Card count = 6
- [ ] is_card flags correct
- [ ] Matches SmartMKV report

---

## Phase 3: Split Certification

### Test 3A: Folder Split + Cards

**Steps:**
1. Import 7 videos (2+ folders)
2. Enable Cards ON
3. Enable Folder Split
4. Run merge

**Verify:**
```
- Multiple output files (one per folder)
- Cards within each part (not orphaned)
- No card-video separation across parts
- Each part has correct card count
```

**Record:**
- [ ] Output parts = folder count
- [ ] No orphan cards
- [ ] Cards within correct parts

---

### Test 3B: Duration Split + Cards

**Steps:**
1. Import 7 videos (total ~5 min)
2. Enable Cards ON
3. Set Duration Split: 2 minutes
4. Run merge

**Verify:**
```
- 3 output files (~2 min each)
- Cards distributed correctly
- No card at split boundary
```

**Record:**
- [ ] Parts ≈ 3
- [ ] No split mid-card
- [ ] Duration correct per part

---

### Test 3C: Count Split + Cards

**Steps:**
1. Import 7 videos
2. Enable Cards ON
3. Set Count Split: 3 files per part
4. Run merge

**Verify:**
```
- 3 output files (3+3+1)
- Cards within each part
```

**Record:**
- [ ] Parts = 3
- [ ] Cards correct per part

---

## Phase 4: Thumbnail Certification

### Test 4A: Thumbnail Lifecycle

**Steps:**
1. Complete any merge with Cards ON
2. Open report immediately
3. Check all thumbnails load
4. Close report
5. Reopen report
6. Check thumbnails still load
7. Restart app
8. Check thumbnails (may be cleared)

**Verify:**
```
- No 404 errors in console
- All video thumbnails display
- All card thumbnails display (if applicable)
```

**Record:**
- [ ] No 404 errors on first open
- [ ] No 404 errors on reopen
- [ ] Graceful handling after restart

---

## Phase 5: Dashboard Certification

### Test 5A: Normalization Dashboard

**Steps:**
1. Import 7 videos (mix of compatible/incompatible)
2. Run SmartMKV
3. Watch normalization phase

**Verify:**
```
"Files Scanned: 7"
"Needs Normalization: X" (actual count)
"Already Compatible: Y" (7 - X)
```

**Record:**
- [ ] Total matches file count
- [ ] No double-counting (was 8, should be 7)
- [ ] Progress bar accurate

---

## Phase 6: Completion Event Certification

### Test 6A: Segment Metadata

**Steps:**
1. Complete merge with Cards ON
2. Check browser console for merge-complete event

**Verify:**
```json
{
  "segments": [
    { "is_card": false, "card_color": null, "duration": ... },
    { "is_card": true, "card_color": "#00CCCC", "duration": 2.0 },
    ...
  ]
}
```

**Record:**
- [ ] is_card flags correct
- [ ] card_color correct
- [ ] Segment count = videos + cards
- [ ] Durations correct

---

## Phase 7: Real Workload Certification

### Test 7A: 50+ Files

**Steps:**
1. Import 50+ videos
2. Run SmartMKV (Cards ON)
3. Run FastMKV (Cards ON)
4. Compare outputs

**Record:**
| Metric | SmartMKV | FastMKV |
|--------|----------|---------|
| Runtime | | |
| Card count | | |
| Output duration | | |
| Output size | | |
| Memory usage | | |

---

## Certification Summary

| Phase | Test | Status |
|-------|------|--------|
| 1A | SmartMKV Cards | PENDING |
| 1B | FastMKV Cards | PENDING |
| 1C | PerFolder Cards | PENDING |
| 2A | SmartMKV Report | PENDING |
| 2B | FastMKV Report | PENDING |
| 3A | Folder Split | PENDING |
| 3B | Duration Split | PENDING |
| 3C | Count Split | PENDING |
| 4A | Thumbnails | PENDING |
| 5A | Dashboard | PENDING |
| 6A | Completion Events | PENDING |
| 7A | 50+ Files | PENDING |

---

## Final Certification

**Code Audit:** ✅ PASS
**Production Verification:** PENDING

**Overall Status:** PENDING

**Sign-Off Date:** PENDING
**Auditor:** Agent 1

---

## Instructions for User

1. Run each test in order
2. Check the boxes as you verify
3. Record any failures
4. Update this document with actual results
5. Report any FAIL items for fixing
