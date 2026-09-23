# Fresh Split Merge Workflow — Close Bug #1

## Prerequisites

- Binary rebuilt with Bug #1 fix: `cd src-tauri && cargo build` ✅ **(already done)**
- FFmpeg and ffprobe available on PATH
- Test media files in source folders

## Step 1: Run a fresh split merge

1. **Start the app**: `cargo tauri dev` (or run the built binary)
2. **Import source folders** containing video files with subtitles
3. **Enable Split by Folder** in the merge panel
4. **Set subtitle mode** to `Embed` (the mode that was broken before the fix)
5. **Run the merge**
6. Wait for completion — outputs will appear in the destination directory

## Step 2: Run the certification

After the merge completes, run the certification script on the output directory:

```bash
python scripts/split_subtitle_certification.py /path/to/output/dir --auto-detect
```

The script will:
- Auto-discover all MKV+SRT sibling pairs
- Look for companion report files (`.md` or `.txt`)
- Verify all 8 criteria for every split output
- Show segment indices from the report files
- Print PASS/FAIL for every split

## Expected Results

**With the Bug #1 fix applied**, all splits should **PASS** certification, including Split #41:

| Criteria | Expected |
|----------|----------|
| 1. Filenames match | ✅ |
| 2. Same directory | ✅ |
| 3. SRT exists & non-empty | ✅ |
| 4. Cue count matches | ✅ |
| 5. First cue near 00:00 | ✅ |
| 6. Last cue ≤ video duration | ✅ **(no more 76.9s overflow)** |
| 7. Sequential timestamps | ✅ |
| 8. Embedded == exported | ✅ |

## If a split still fails

Run targeted forensic analysis:

```bash
python -c "
import re
with open('/path/to/_Merged_XXX.srt') as f:
    content = f.read()
# ... parse and analyze specific overflow
"
```

Then use ffprobe to verify the video duration matches the expected split scope.

## Closing Bug #1

Once certification passes on fresh outputs:

- Commit the Bug #1 fix (`git add -A && git commit -m "Fix: per-part SRT export for split merges"`)
- Update `CHANGELOG.md`
- Close the ticket

## Next: Audio Boundary Certification

After Bug #1 is closed, shift focus to the **intermittent audio dropout** (Bug #2). The next step is building an Audio Boundary Certification tool that performs packet-level and PCM-level forensic analysis on merged outputs.
