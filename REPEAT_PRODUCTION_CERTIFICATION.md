# Repeat Feature — Production Certification

## Status: 🟡 Unit Tests Pass, Runtime Tests Pending

---

## Unit Test Results (All Pass)

| Module | Tests | Status |
|--------|-------|--------|
| `ffmpeg::repeat` | 7 tests | ✅ All Pass |
| `report::builder_tests` | 15 tests | ✅ All Pass |
| `cargo check --lib` | Compilation | ✅ Clean |
| `tsc --noEmit` | TypeScript | ✅ Clean |

---

## Test Matrix

| # | Test Case | Mode | Status | Notes |
|---|-----------|------|--------|-------|
| 1 | Repeat By Count | Count | ⬜ | Needs runtime |
| 2 | Repeat Until Duration | Duration | ⬜ | Needs runtime |
| 3 | SmartMKV + Repeat | SmartMKV | ⬜ | Needs runtime |
| 4 | FastMKV + Repeat | FastMKV | ⬜ | Needs runtime |
| 5 | Cards + PerRepeat | All | ⬜ | Needs runtime |
| 6a | Split (Count) + Repeat | Count Split | ⬜ | Needs runtime |
| 6b | Split (Duration) + Repeat | Duration Split | ⬜ | Needs runtime |
| 7 | Recovery + Repeat | Resume | ⬜ | Needs runtime |
| 8 | Subtitle + Repeat | Sub Expansion | ⬜ | Needs runtime |

---

## Test 1 — Repeat By Count

**Input:**
```
Video A (10s)
Video B (10s)
```

**Repeat:** Count = 3

**Expected Output:**
```
A
B
A
B
A
B
```

**Verification:**
- [ ] Timeline order: A, B, A, B, A, B
- [ ] Total duration: 60s
- [ ] Report summary shows repeat count
- [ ] Completion metadata correct

---

## Test 2 — Repeat Until Duration

**Input:**
```
Playlist = 25 min
```

**Target:** 2 hours

**Expected:** ~5 loops

**Verification:**
- [ ] Final duration ≥ target duration
- [ ] No missing files
- [ ] No truncated final loop
- [ ] Report summary shows duration target

---

## Test 3 — SmartMKV + Repeat

**Input:**
```
7 videos
Repeat ×10
```

**Expected:** 70 files processed

**Verification:**
- [ ] Normalization cache hits (normalize once, reuse 9 times)
- [ ] NOT normalizing 10 times per file
- [ ] Cache hit count in logs matches expectation
- [ ] Output quality correct

---

## Test 4 — FastMKV + Repeat

**Input:**
```
Any playlist
Repeat ×N
```

**Verification:**
- [ ] Repeat expansion works
- [ ] Cards inserted correctly
- [ ] Report generated correctly
- [ ] No FastMKV-specific pipeline bugs

---

## Test 5 — Cards + PerRepeat

**This is the most important new feature.**

**Expected Structure:**
```
Repeat 1
  ↓
  Card
  ↓
  Videos

Repeat 2
  ↓
  Card
  ↓
  Videos
```

**Verification:**
- [ ] Card inserted at repeat boundary
- [ ] Card count = repeat count
- [ ] Report card count matches actual
- [ ] Card displays correct repeat number (e.g., "🔁 Repeat 1 / 3")

---

## Test 6a — Split (Count) + Repeat

**Input:**
```
Repeat ×20
Split every 10 files
```

**Expected:** 2 parts

**Verification:**
- [ ] No orphan cards
- [ ] No broken repeat boundaries
- [ ] Split files merge correctly
- [ ] Report shows correct split structure

---

## Test 6b — Split (Duration) + Repeat

**Input:**
```
Repeat ×N
Split every X minutes
```

**Verification:**
- [ ] Repeat structure preserved across splits
- [ ] No mid-card splits
- [ ] Duration targets met

---

## Test 7 — Recovery + Repeat

**Critical test.**

**Scenario:**
```
Repeat ×100
Stop merge after Loop 40
Resume
```

**Expected:**
- [ ] Reuses normalized outputs from Loops 1-40
- [ ] Continues from Loop 41
- [ ] Does NOT start from beginning
- [ ] Checkpoint stores RepeatConfig
- [ ] Checkpoint stores original_file_count
- [ ] Checkpoint stores repeat_count

---

## Test 8 — Subtitle + Repeat

**Input:**
```
Videos with external subtitles
Repeat ×N
```

**Verification:**
- [ ] Subtitle arrays expanded correctly
- [ ] Each repeat cycle has correct subtitle references
- [ ] No subtitle mismatch errors
- [ ] Subtitle language tags preserved

---

## Certification Sign-Off

Once all tests pass:

```
Repeat Feature Status: ✅ Certified
Date: ________________
Tested By: ________________
```

---

## Notes

- Run tests in order (dependencies exist)
- Test 7 (Recovery) requires manual interruption
- Test 8 (Subtitles) needs videos with external .srt/.ass files
- Log all failures with exact reproduction steps
