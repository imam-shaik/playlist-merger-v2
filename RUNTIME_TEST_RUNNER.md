# Runtime Test Runner

**Purpose:** Automated and manual tests for production certification
**Date:** 2026-06-11

---

## Automated Tests (Can Run Without Videos)

### T1: Compilation Check

```powershell
# Rust
cd src-tauri
cargo check 2>&1

# TypeScript
cd ..
npx tsc --noEmit 2>&1
```

**Expected:** Both pass with 0 errors

**Result:** PASS ✅

---

### T2: Import Verification

```powershell
# Check all imports resolve
npx tsc --noEmit 2>&1 | findstr "error"
```

**Expected:** No output (no errors)

**Result:** PASS ✅

---

### T3: Debug Code Removal

```powershell
# Should find NO console.log in critical paths
rg "console\.log.*CARDS:DEBUG" src/
rg "console\.log.*NormalizationProgressList" src/
rg "DEBUG = true" src/
```

**Expected:** No matches

**Result:** PASS ✅ (Verified 2026-06-11: No debug logs remain)

---

### T4: Path Handling Centralization

```powershell
# Should find NO local strip_extended_path_prefix definitions
rg "fn strip_extended_path_prefix" src-tauri/src/ --glob "!cards.rs"

# Should find NO direct calls without crate:: prefix
rg "strip_extended_path_prefix\(&" src-tauri/src/ --glob "!cards.rs" | rg -v "crate::"
```

**Expected:** No matches

**Result:** PASS ✅

---

### T5: Thumbnail Cleanup Removal

```powershell
# Should find NO calls to clearThumbnailCache
rg "clearThumbnailCache" src/ src-tauri/src/
```

**Expected:** Only definition in commands.ts, no calls

**Result:** PASS ✅

---

### T6: segment_is_card Fix

```powershell
# Should find NO "is_card: Some(false)" in completion events
rg "is_card: Some\(false\)" src-tauri/src/commands/merge.rs
```

**Expected:** No matches

**Result:** PASS ✅

---

### T7: Outlier Counting Fix

```powershell
# Should find NO double-counting
rg "need_profile_norm\.len\(\) \+ need_audio_norm\.len\(\)" src-tauri/src/
```

**Expected:** No matches

**Result:** PASS ✅

---

## Manual Tests (Require Videos)

### M1: Import Test Files

```
1. Create test folder: C:\test\merge\
2. Create subfolder: C:\test\merge\folder_a\
3. Create subfolder: C:\test\merge\folder_b\
4. Place 3-4 videos in each subfolder
5. Add 1 .srt subtitle file (optional)
```

---

### M2: SmartMKV + Cards Test

```
1. Open app
2. Import C:\test\merge\folder_a\ (3-4 videos)
3. Import C:\test\merge\folder_b\ (3-4 videos)
4. Enable Cards: ON
5. Set Color: #00CCCC
6. Set Frequency: PerVideo
7. Set Duration: 2
8. Select Mode: SmartMKV
9. Click Merge
10. Wait for completion
11. Open report
12. Verify:
    - Segment count = 13 (7 videos + 6 cards)
    - Cards show "Canvas" badge
    - Cards have #00CCCC color
    - No 404 errors
```

**Result:** PENDING

---

### M3: FastMKV + Cards Test

```
1. Import same files
2. Enable Cards: ON (same settings)
3. Select Mode: FastMKV
4. Run merge
5. Open report
6. Verify:
    - Same as M2
    - Cards correctly identified (is_card=true)
```

**Result:** PENDING

---

### M4: Folder Split + Cards Test

```
1. Import files from both folders
2. Enable Cards: ON
3. Enable Split: Folder Split
4. Run merge
5. Verify:
    - Multiple output files
    - Cards within each part
    - No orphan cards
```

**Result:** PENDING

---

### M5: Progress Events Test

```
1. Run SmartMKV with 7 videos
2. Watch progress panel
3. Verify:
    - Phase labels correct
    - File names shown
    - Progress % updates
    - Dashboard shows correct counts
```

**Result:** PENDING

---

## Test Results Summary

| Test | Type | Status | Verified |
|------|------|--------|----------|
| T1 | Auto | ✅ PASS | 2026-06-11 |
| T2 | Auto | ✅ PASS | 2026-06-11 |
| T3 | Auto | ✅ PASS | 2026-06-11 |
| T4 | Auto | ✅ PASS | 2026-06-11 |
| T5 | Auto | ✅ PASS | 2026-06-11 |
| T6 | Auto | ✅ PASS | 2026-06-11 |
| T7 | Auto | ✅ PASS | 2026-06-11 |
| M1 | Manual | PENDING | |
| M2 | Manual | PENDING | |
| M3 | Manual | PENDING | |
| M4 | Manual | PENDING | |
| M5 | Manual | PENDING | |

---

## Next Steps

1. Build app: `cargo tauri dev`
2. Run manual tests M1-M5
3. Record results
4. Update certification document
