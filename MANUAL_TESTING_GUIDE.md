# Manual Testing Guide

**Status:** READY FOR EXECUTION
**Date:** 2026-06-11

---

## Quick Start

```powershell
# 1. Setup test environment
.\scripts\test-setup.ps1

# 2. Build and run app
cd src-tauri
cargo tauri dev

# 3. Follow test instructions below
```

---

## Test Environment

```
C:\test\merge\
├── folder_a\    (Place 3-4 .mp4 files here)
├── folder_b\    (Place 3-4 .mp4 files here)
└── test.srt     (Optional subtitle file)
```

---

## M1: Import Test Files

**Steps:**
1. Open Playlist Merger app
2. Click "Import Folder"
3. Select `C:\test\merge\folder_a`
4. Wait for files to load
5. Click "Import Folder" again
6. Select `C:\test\merge\folder_b`
7. Verify 6-8 files shown in playlist

**Expected:** All files listed with thumbnails

**Result:** PENDING

---

## M2: SmartMKV + Cards Test

**Steps:**
1. In merge panel:
   - Enable Cards: **ON**
   - Card Color: `#00CCCC`
   - Frequency: **PerVideo**
   - Duration: **2**
2. Select Mode: **SmartMKV**
3. Click "Merge"
4. Wait for completion
5. Open merge report

**Verify:**
```
□ Segment count = 13 (7 videos + 6 cards)
□ Cards show "Canvas" badge
□ Cards have #00CCCC color
□ Card duration = 2.0s
□ No 404 errors in console
□ Thumbnails visible
```

**Result:** PENDING

---

## M3: FastMKV + Cards Test

**Steps:**
1. Import same files again
2. Enable Cards: **ON** (same settings)
3. Select Mode: **FastMKV**
4. Run merge
5. Open report

**Verify:**
```
□ Same as M2
□ Cards correctly identified (is_card=true)
□ No path errors
```

**Compare with M2:**
```
□ Same segment count
□ Same card positions
□ Report structure identical
```

**Result:** PENDING

---

## M4: Folder Split + Cards Test

**Steps:**
1. Import files from both folders
2. Enable Cards: **ON**
3. Enable Split: **Folder Split**
4. Run merge

**Verify:**
```
□ Multiple output files (one per folder)
□ Cards within each part
□ No orphan cards
□ Each part report correct
```

**Result:** PENDING

---

## M5: Progress Events Test

**Steps:**
1. Run SmartMKV with 7 videos
2. Watch progress panel during merge

**Verify:**
```
□ Phase labels correct:
  - Analysing Files
  - Validating Files
  - Normalising Files
  - Merging Timeline
□ Current file name shown
□ Progress % updates
□ Dashboard shows:
  - Files Scanned: 7
  - Needs Normalization: X
  - Already Compatible: Y
```

**Result:** PENDING

---

## Recording Results

```powershell
# After each test, record result:
.\scripts\test-record.ps1 -TestName "M2" -Status "PASS"
.\scripts\test-record.ps1 -TestName "M3" -Status "FAIL" -Notes "Card count wrong"

# View all results:
.\scripts\test-results.ps1
```

---

## Certification Checklist

| Test | Description | Status |
|------|-------------|--------|
| M1 | Import Test Files | PENDING |
| M2 | SmartMKV + Cards | PENDING |
| M3 | FastMKV + Cards | PENDING |
| M4 | Folder Split + Cards | PENDING |
| M5 | Progress Events | PENDING |

---

## Exit Criteria

**Agent 1 = PRODUCTION CERTIFIED when:**

```
M1: PASS
M2: PASS
M3: PASS
M4: PASS
M5: PASS
```

Or documented exceptions with reasons.
