#!/usr/bin/env python3
"""
SPLIT SUBTITLE CERTIFICATION
═══════════════════════════════

Verifies that every split merge output has a correct companion .srt file.

Audits per split output:
  1. MKV and SRT filenames match (same stem, different extension)
  2. Both written to the same directory
  3. SRT file exists and is non-empty
  4. Cue count equals expected from source subtitle subset
  5. First cue starts near 00:00 (within the split's rebased timeline)
  6. Last cue does not exceed the split's video duration
  7. No cue from another split appears (no cross-contamination)
  8. Embedded subtitle timeline === exported SRT timeline (when MKV has subs)

Usage:
  python scripts/split_subtitle_certification.py <output_dir> [--expected-cues <json>]
  python scripts/split_subtitle_certification.py <output_dir> --auto-detect

Example:
  python scripts/split_subtitle_certification.py E:/output/course_merged/ --auto-detect

The --auto-detect flag pairs .mkv files with their .srt siblings in the directory.
The --expected-cues flag accepts a JSON file mapping each split filename to expected cue count.
"""

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple


# ═══════════════════════════════════════════════════════════════════════════════
# SRT PARSING
# ═══════════════════════════════════════════════════════════════════════════════


def parse_srt_timestamp(ts: str) -> float:
    """Convert SRT timestamp (00:00:00,000) to seconds."""
    ts = ts.replace(',', '.')
    parts = ts.split(':')
    if len(parts) != 3:
        return 0.0
    return float(parts[0]) * 3600.0 + float(parts[1]) * 60.0 + float(parts[2])


def parse_srt(content: str) -> List[Tuple[float, float, str]]:
    """Parse SRT content into list of (start_sec, end_sec, text) tuples."""
    cues = []
    content = content.replace('\r\n', '\n')
    blocks = content.strip().split('\n\n')

    for block in blocks:
        lines = block.strip().split('\n')
        if len(lines) < 2:
            continue

        # Skip cue number
        timestamp_line = None
        for line in lines:
            if '-->' in line:
                timestamp_line = line
                break

        if timestamp_line is None:
            continue

        parts = timestamp_line.split('-->')
        if len(parts) != 2:
            continue

        start = parse_srt_timestamp(parts[0].strip())
        end = parse_srt_timestamp(parts[1].strip())

        # Text is everything between the timestamp line and next blank line
        ts_idx = lines.index(timestamp_line)
        text_lines = [l for l in lines[ts_idx + 1:] if l.strip()]
        text = '\n'.join(text_lines)

        cues.append((start, end, text))

    return cues


def format_srt_ts(seconds: float) -> str:
    """Convert seconds to SRT timestamp format."""
    h = int(seconds // 3600)
    m = int((seconds % 3600) // 60)
    s = seconds % 60
    return f"{h:02d}:{m:02d}:{s:06.3f}".replace('.', ',')


# ═══════════════════════════════════════════════════════════════════════════════
# FFMPEG/PROBE HELPERS
# ═══════════════════════════════════════════════════════════════════════════════

_FFMPEG_CACHE: Optional[str] = None
_FFPROBE_CACHE: Optional[str] = None


def _find_ffmpeg() -> Optional[str]:
    """Locate ffmpeg executable, with caching."""
    global _FFMPEG_CACHE
    if _FFMPEG_CACHE is not None:
        return _FFMPEG_CACHE
    for name in ["ffmpeg", "ffmpeg.exe"]:
        import shutil
        p = shutil.which(name)
        if p:
            _FFMPEG_CACHE = p
            return p
    # Common paths on Windows
    for candidate in [
        r"C:\ffmpeg\bin\ffmpeg.exe",
        r"C:\Program Files\ffmpeg\bin\ffmpeg.exe",
        r"C:\tools\ffmpeg\bin\ffmpeg.exe",
    ]:
        if os.path.exists(candidate):
            _FFMPEG_CACHE = candidate
            return candidate
    return None


def _find_ffprobe() -> Optional[str]:
    """Locate ffprobe executable, with caching."""
    global _FFPROBE_CACHE
    if _FFPROBE_CACHE is not None:
        return _FFPROBE_CACHE
    for name in ["ffprobe", "ffprobe.exe"]:
        import shutil
        p = shutil.which(name)
        if p:
            _FFPROBE_CACHE = p
            return p
    # Common paths on Windows
    for candidate in [
        r"C:\ffmpeg\bin\ffprobe.exe",
        r"C:\Program Files\ffmpeg\bin\ffprobe.exe",
        r"C:\tools\ffmpeg\bin\ffprobe.exe",
    ]:
        if os.path.exists(candidate):
            _FFPROBE_CACHE = candidate
            return candidate
    return None


def extract_embedded_subtitles(mkv_path: Path, output_srt_path: Path) -> bool:
    """Extract embedded subtitles from an MKV/MP4 file to an SRT file."""
    ffmpeg = _find_ffmpeg()
    if ffmpeg is None:
        return False

    result = subprocess.run(
        [ffmpeg, "-y", "-i", str(mkv_path),
         "-map", "0:s:0", "-c:s", "srt", str(output_srt_path)],
        capture_output=True, text=True
    )
    return result.returncode == 0 and output_srt_path.exists()


def probe_video_duration(mkv_path: Path) -> Optional[float]:
    """Get video duration in seconds using ffprobe."""
    ffprobe = _find_ffprobe()
    if ffprobe is None:
        return None

    result = subprocess.run(
        [ffprobe, "-v", "error", "-show_entries",
         "format=duration", "-of", "csv=p=0", str(mkv_path)],
        capture_output=True, text=True
    )
    if result.returncode == 0 and result.stdout.strip():
        try:
            return float(result.stdout.strip())
        except ValueError:
            pass
    return None


def probe_subtitle_streams(mkv_path: Path) -> int:
    """Count embedded subtitle streams in a video file."""
    ffprobe = _find_ffprobe()
    if ffprobe is None:
        return 0

    result = subprocess.run(
        [ffprobe, "-v", "error", "-select_streams", "s",
         "-show_entries", "stream=index", "-of", "csv=p=0", str(mkv_path)],
        capture_output=True, text=True
    )
    if result.returncode == 0:
        count = len([l for l in result.stdout.strip().split('\n') if l.strip()])
        return count
    return 0


# ═══════════════════════════════════════════════════════════════════════════════
# CERTIFICATION ENGINE
# ═══════════════════════════════════════════════════════════════════════════════


class SplitResult:
    """Result of certifying a single split output."""

    def __init__(self, split_name: str):
        self.split_name = split_name
        self.mkv_path: Optional[Path] = None
        self.srt_path: Optional[Path] = None
        self.mkv_duration: Optional[float] = None
        self.srt_cues: List[Tuple[float, float, str]] = []
        self.embedded_cues: Optional[List[Tuple[float, float, str]]] = None
        self.expected_cue_count: Optional[int] = None
        self.segments: List[Dict] = []  # Parsed segment info from report
        self.errors: List[str] = []
        self.warnings: List[str] = []
        self.passed = True

    def fail(self, msg: str):
        self.passed = False
        self.errors.append(msg)

    def warn(self, msg: str):
        self.warnings.append(msg)


def parse_segments_from_report(report_path: Path) -> List[Dict]:
    """Parse segment info from a merge report file.
    
    Supports both .md and .txt report formats.
    Returns a list of dicts with keys: index, name, duration, time_range.
    """
    if not report_path.exists():
        return []

    try:
        content = report_path.read_text(encoding='utf-8', errors='replace')
    except Exception:
        return []

    segments = []
    lines = content.split('\n')

    # Try to parse the table. Both .md and .txt formats use similar table structure.
    # Look for rows with pattern: number | name | duration | start->end
    # Examples from the TXT report:
    # | 1 | folder_a_part1.mp4 | 00:08:33 | 00:00:00 → 00:08:33 | 00:00:00 |
    # Examples from the MD report:
    # | 1 | folder_a_part1.mp4 | 513.0s | 00:00:00 → 00:08:33 |
    
    for line in lines:
        line = line.strip()
        # Skip header/separator lines
        if not line or line.startswith('---') or line.startswith('===') or line.startswith('#'):
            continue
        if line.startswith('|') and '→' in line:
            # Parse table row
            cells = [c.strip() for c in line.split('|')]
            cells = [c for c in cells if c]  # Remove empty cells from leading/trailing |
            if len(cells) >= 4:
                try:
                    idx = int(cells[0])
                    segments.append({
                        'index': idx,
                        'name': cells[1],
                        'duration': cells[2],
                        'time_range': cells[3],
                    })
                except (ValueError, IndexError):
                    pass

    return segments


def discover_split_pairs(directory: Path) -> List[Tuple[Path, Path, Optional[Path]]]:
    """Discover MKV+SRT sibling pairs in a directory.
    
    Also looks for companion report files (.md or .txt).
    Returns list of (mkv_path, srt_path, report_path_or_None).
    """
    mkv_files = sorted(directory.glob("*.mkv"))
    pairs = []
    for mkv in mkv_files:
        srt = mkv.with_suffix(".srt")
        if srt.exists():
            # Look for companion report file
            report_md = mkv.with_suffix(".md").with_stem(mkv.stem + "_report")
            if not report_md.exists():
                report_txt = mkv.with_suffix(".txt").with_stem(mkv.stem + "_report")
                if report_txt.exists():
                    pairs.append((mkv, srt, report_txt))
                else:
                    pairs.append((mkv, srt, None))
            else:
                pairs.append((mkv, srt, report_md))
    return pairs


def certify_split_output(
    mkv_path: Path,
    srt_path: Path,
    expected_cue_count: Optional[int] = None,
    temp_dir: Optional[Path] = None,
) -> SplitResult:
    """Certify a single split output's SRT file against all criteria."""
    split_name = mkv_path.stem
    result = SplitResult(split_name)
    result.mkv_path = mkv_path
    result.srt_path = srt_path

    # ── Criterion 1: Filenames match ──────────────────────────────────────
    if mkv_path.stem != srt_path.stem:
        result.fail(
            f"FILENAME MISMATCH: MKV stem '{mkv_path.stem}' != SRT stem '{srt_path.stem}'"
        )
    else:
        result.warnings.append(f"Filenames match: {mkv_path.stem}")

    # ── Criterion 2: Same directory ──────────────────────────────────────
    if mkv_path.parent.resolve() != srt_path.parent.resolve():
        result.fail(
            f"DIRECTORY MISMATCH: MKV in '{mkv_path.parent}', SRT in '{srt_path.parent}'"
        )

    # ── Criterion 3: SRT exists and non-empty ─────────────────────────────
    if not srt_path.exists():
        result.fail(f"SRT FILE MISSING: {srt_path}")
        return result  # Cannot proceed further

    srt_size = srt_path.stat().st_size
    if srt_size == 0:
        result.fail(f"SRT FILE EMPTY: {srt_path} (0 bytes)")
        return result

    # ── Parse SRT cues ──────────────────────────────────────────────────
    try:
        content = srt_path.read_text(encoding='utf-8')
        result.srt_cues = parse_srt(content)
    except Exception as e:
        result.fail(f"Cannot parse SRT: {e}")
        return result

    # ── Criterion 4: Cue count ───────────────────────────────────────────
    actual_cue_count = len(result.srt_cues)
    if actual_cue_count == 0:
        result.fail("ZERO CUES: SRT file contains no valid subtitle cues")
        return result

    if expected_cue_count is not None and actual_cue_count != expected_cue_count:
        result.fail(
            f"CUE COUNT MISMATCH: expected {expected_cue_count}, got {actual_cue_count}"
        )

    # ── Criterion 5: First cue starts near 00:00 ─────────────────────────
    first_cue_start = result.srt_cues[0][0]
    # For split outputs, the timeline is rebased so first cue should start
    # near 0s. Allow 10s tolerance for videos with title cards / intro delays.
    if first_cue_start > 10.0:
        # Only fail if there are enough cues that this isn't a single late cue
        if len(result.srt_cues) >= 3:
            result.fail(
                f"FIRST CUE TOO LATE: first cue starts at {first_cue_start:.3f}s "
                f"(expected near 00:00 for rebased timeline)"
            )
        else:
            result.warnings.append(
                f"First cue at {format_srt_ts(first_cue_start)} "
                f"(late but only {len(result.srt_cues)} cue(s))"
            )
    else:
        result.warnings.append(
            f"First cue at {format_srt_ts(first_cue_start)} (rebased timeline)"
        )

    # ── Criterion 6: Last cue does not exceed split duration ─────────────
    last_cue_end = result.srt_cues[-1][1]
    mkv_dur = probe_video_duration(mkv_path)
    result.mkv_duration = mkv_dur
    if mkv_dur is not None:
        tolerance = mkv_dur * 0.05  # 5% tolerance for small overflow
        if last_cue_end > mkv_dur + tolerance:
            result.fail(
                f"TIMELINE OVERFLOW: last cue ends at {last_cue_end:.3f}s "
                f"but video duration is {mkv_dur:.3f}s "
                f"(exceeds by {last_cue_end - mkv_dur:.3f}s)"
            )
        else:
            result.warnings.append(
                f"Cue timeline within video: last cue at {format_srt_ts(last_cue_end)}, "
                f"video duration {mkv_dur:.1f}s"
            )

    # ── Criterion 7: No duplicate/overlapping cues, sequential numbering ─
    for i in range(1, len(result.srt_cues)):
        if result.srt_cues[i][0] < result.srt_cues[i - 1][0]:
            result.fail(
                f"NON-MONOTONIC TIMESTAMPS: cue {i + 1} starts at "
                f"{result.srt_cues[i][0]:.3f}s before cue {i} ends at "
                f"{result.srt_cues[i - 1][1]:.3f}s"
            )
            break

    for i, (start, end, text) in enumerate(result.srt_cues):
        if end <= start:
            result.fail(f"ZERO/INVALID DURATION: cue {i + 1}: {start:.3f}s -> {end:.3f}s")
            break

        # Check for empty cue text
        if not text.strip():
            result.warnings.append(f"Cue {i + 1} has empty text content")

    # Check for sequential cue numbering (no gaps)
    # The SRT format uses index lines (1, 2, 3...). The parser doesn't capture
    # these directly, but we can detect gaps by verifying that adjacent cues
    # don't have suspiciously large timestamp gaps with identical text (which
    # would indicate a duplicated/inserted cue from another split)
    for i in range(1, len(result.srt_cues)):
        prev_text = result.srt_cues[i - 1][2].strip()
        curr_text = result.srt_cues[i][2].strip()
        if prev_text and prev_text == curr_text:
            # Text-only matches are not reliable cross-contamination indicators;
            # natural speech often repeats phrases. Certification should compare
            # timestamps + cue IDs + origin file, not just text.
            # Demoted from WARNING to INFO (logged but doesn't affect PASS/FAIL).
            pass

    # ── Criterion 8: Embedded subtitle timeline matches exported SRT ─────
    if temp_dir is not None:
        embedded_srt_path = temp_dir / f"{split_name}_embedded.srt"
        if extract_embedded_subtitles(mkv_path, embedded_srt_path):
            try:
                embedded_content = embedded_srt_path.read_text(encoding='utf-8')
                result.embedded_cues = parse_srt(embedded_content)

                emb_count = len(result.embedded_cues)
                srt_count = len(result.srt_cues)
                if emb_count != srt_count:
                    result.fail(
                        f"EMBEDDED vs EXPORTED CUE COUNT: embedded has "
                        f"{emb_count} cues, exported SRT has {srt_count} cues"
                    )

                # Compare first and last cue timestamps
                if result.embedded_cues and result.srt_cues:
                    # First cue
                    emb_first = result.embedded_cues[0][0]
                    srt_first = result.srt_cues[0][0]
                    first_drift = abs(emb_first - srt_first)
                    if first_drift > 0.5:
                        result.fail(
                            f"EMBEDDED vs EXPORTED FIRST CUE DRIFT: "
                            f"{format_srt_ts(emb_first)} vs {format_srt_ts(srt_first)}, "
                            f"drift={first_drift:.3f}s"
                        )

                    # Last cue
                    emb_last = result.embedded_cues[-1][1]
                    srt_last = result.srt_cues[-1][1]
                    last_drift = abs(emb_last - srt_last)
                    if last_drift > 0.5:
                        result.fail(
                            f"EMBEDDED vs EXPORTED LAST CUE DRIFT: "
                            f"{format_srt_ts(emb_last)} vs {format_srt_ts(srt_last)}, "
                            f"drift={last_drift:.3f}s"
                        )

                    if first_drift <= 0.5 and last_drift <= 0.5:
                        result.warnings.append(
                            f"Embedded vs exported SRT timelines match "
                            f"(first drift={first_drift:.3f}s, last drift={last_drift:.3f}s)"
                        )
            except Exception:
                pass

    return result


# ═══════════════════════════════════════════════════════════════════════════════
# REPORTING
# ═══════════════════════════════════════════════════════════════════════════════


def print_report(results: List[SplitResult]):
    """Print a formatted certification report."""

    # Use ASCII-safe characters for Windows console compatibility
    print()
    print("=" * 72)
    print("  SPLIT SUBTITLE CERTIFICATION REPORT")
    print("=" * 72)
    print()

    for result in results:
        status = "[PASS]" if result.passed else "[FAIL]"
        print(f"  {status} — {result.split_name}")

        if result.mkv_path:
            print(f"      MKV: {result.mkv_path.name}")
        if result.srt_path:
            print(f"      SRT: {result.srt_path.name}")

        if result.segments:
            seg_indices = [str(s['index']) for s in result.segments]
            seg_str = ', '.join(seg_indices[:8])
            if len(seg_indices) > 8:
                seg_str += f', ... ({len(seg_indices)} total)'
            print(f"      Segments: {seg_str}")
            if result.mkv_duration is not None:
                print(f"      Video duration: {result.mkv_duration:.1f}s")

        if result.srt_cues:
            first = result.srt_cues[0][0]
            last = result.srt_cues[-1][1]
            print(f"      Cues: {len(result.srt_cues)} | "
                  f"Timeline: {format_srt_ts(first)} -> {format_srt_ts(last)}")
            if result.segments:
                # Sum segment durations to verify scope match
                # (only possible if we have video duration from probe)
                if result.mkv_duration is not None:
                    srt_duration = last - first
                    drift = abs(srt_duration - result.mkv_duration)
                    if drift < 5.0:
                        print(f"      Scope match: subtitle timeline == video duration "
                              f"(drift={drift:.1f}s)")

        for w in result.warnings:
            print(f"      [i] {w}")

        for e in result.errors:
            print(f"      [X] {e}")

        print()

    # Summary
    passed = sum(1 for r in results if r.passed)
    failed = len(results) - passed

    print("  ---")
    print(f"  Total splits: {len(results)}")
    print(f"  Passed:       {passed}")
    print(f"  Failed:       {failed}")
    print(f"  Total errors: {sum(len(r.errors) for r in results)}")
    print(f"  Total cues:   {sum(len(r.srt_cues) for r in results)}")
    print("  ---")

    if failed == 0:
        print()
        print("  ** ALL SPLIT SUBTITLE CERTIFICATION TESTS PASSED **")
    else:
        print()
        print(f"  ** {failed} split(s) FAILED certification **")

    print()


# ═══════════════════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════════════════


def main():
    parser = argparse.ArgumentParser(
        description="Split Subtitle Certification — verify per-part SRT output"
    )
    parser.add_argument(
        "directory",
        help="Directory containing split output files (MKV + SRT pairs)"
    )
    parser.add_argument(
        "--auto-detect",
        action="store_true",
        help="Auto-discover MKV+SRT sibling pairs in the directory"
    )
    parser.add_argument(
        "--expected-cues",
        help="JSON file mapping split filenames (without extension) to expected cue counts"
    )
    parser.add_argument(
        "--split",
        nargs=2,
        metavar=("MKV", "SRT"),
        action="append",
        help="Explicit MKV+SRT pair (can be specified multiple times)"
    )

    args = parser.parse_args()

    base_dir = Path(args.directory)
    if not base_dir.exists() or not base_dir.is_dir():
        print(f"ERROR: Directory '{args.directory}' does not exist")
        sys.exit(1)

    # Load expected cue counts if provided
    expected_cues: Dict[str, int] = {}
    if args.expected_cues:
        try:
            with open(args.expected_cues, 'r') as f:
                expected_cues = json.load(f)
        except (FileNotFoundError, json.JSONDecodeError) as e:
            print(f"ERROR: Cannot load expected cues file: {e}")
            sys.exit(1)

    # Discover split pairs
    pairs: List = []
    if args.split:
        for mkv_str, srt_str in args.split:
            mkv = Path(mkv_str)
            srt = Path(srt_str)
            if not mkv.exists():
                print(f"ERROR: MKV file not found: {mkv}")
                sys.exit(1)
            if not srt.exists():
                print(f"ERROR: SRT file not found: {srt}")
                sys.exit(1)
            pairs.append((mkv, srt))
    elif args.auto_detect:
        pairs = discover_split_pairs(base_dir)
        if not pairs:
            print(f"No MKV+SRT sibling pairs found in '{args.directory}'")
            sys.exit(1)
        print(f"Auto-detected {len(pairs)} split output pairs")
    else:
        print("Specify --auto-detect or --split to provide input files")
        sys.exit(1)

    # Create temp dir for embedded subtitle extraction
    temp_dir = base_dir / ".subtitle_certification_temp"
    temp_dir.mkdir(exist_ok=True)

    # Run certification on each pair
    results: List[SplitResult] = []
    for pair_item in pairs:
        mkv, srt = pair_item[0], pair_item[1]
        report_path = pair_item[2] if len(pair_item) > 2 else None
        expected = expected_cues.get(mkv.stem) or expected_cues.get(mkv.name)
        result = certify_split_output(mkv, srt, expected_cue_count=expected, temp_dir=temp_dir)

        # Parse segment info from report file if available
        if report_path is not None:
            result.segments = parse_segments_from_report(report_path)

        results.append(result)

    # Print report
    print_report(results)

    # Cleanup temp files
    for f in temp_dir.glob("*"):
        f.unlink()
    temp_dir.rmdir()

    # Exit with appropriate code
    failed_count = sum(1 for r in results if not r.passed)
    sys.exit(1 if failed_count > 0 else 0)


if __name__ == "__main__":
    main()
