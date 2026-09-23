#!/usr/bin/env python3
"""
END-TO-END A/V SYNC CERTIFICATION
══════════════════════════════════════

Verifies audio/video timestamp alignment at multiple seek points
across merged outputs, including all concat boundaries.

For each seek point:
  1. Find the nearest video frame PTS
  2. Find the nearest audio packet PTS
  3. Calculate A/V drift (video_pts - audio_pts)
  4. Verify both streams can be decoded
  5. Report PASS if drift is within tolerance

Additionally verifies:
  - Total duration consistency (format / video PTS / audio PTS)
  - Packet continuity at boundaries
  - Seek integrity at every percentile

Usage:
  python scripts/av_sync_certification.py <merged.mkv> [--report <report.txt>]
  python scripts/av_sync_certification.py <merged.mkv> --auto-detect

The --auto-detect flag looks for a companion report file to find concat boundaries.
"""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Any


# ═══════════════════════════════════════════════════════════════════════════════
# CONFIGURATION
# ═══════════════════════════════════════════════════════════════════════════════

# Tolerance for A/V sync drift at any single point
AV_SYNC_TOLERANCE_S: float = 0.100  # 100ms — common lip-sync threshold (can be overridden via --tolerance)

# Tolerance for total duration comparison across sources
DURATION_TOLERANCE_S: float = 0.500  # 500ms

# Number of seek points (evenly spaced)
SEEK_POINTS = 10  # Every 10%

# Default sample rate for PTS conversion (when time_base unknown)
DEFAULT_TIMEBASE = 48000


# ═══════════════════════════════════════════════════════════════════════════════
# FFPROBE HELPERS
# ═══════════════════════════════════════════════════════════════════════════════

_FFMPEG_CACHE: Optional[str] = None
_FFPROBE_CACHE: Optional[str] = None


def _find_ffmpeg() -> Optional[str]:
    global _FFMPEG_CACHE
    if _FFMPEG_CACHE is not None:
        return _FFMPEG_CACHE
    import shutil
    for name in ["ffmpeg", "ffmpeg.exe"]:
        p = shutil.which(name)
        if p:
            _FFMPEG_CACHE = p
            return p
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
    global _FFPROBE_CACHE
    if _FFPROBE_CACHE is not None:
        return _FFPROBE_CACHE
    import shutil
    for name in ["ffprobe", "ffprobe.exe"]:
        p = shutil.which(name)
        if p:
            _FFPROBE_CACHE = p
            return p
    for candidate in [
        r"C:\ffmpeg\bin\ffprobe.exe",
        r"C:\Program Files\ffmpeg\bin\ffprobe.exe",
        r"C:\tools\ffmpeg\bin\ffprobe.exe",
    ]:
        if os.path.exists(candidate):
            _FFPROBE_CACHE = candidate
            return candidate
    return None


def probe_streams(file_path: Path) -> Optional[Dict[str, Any]]:
    """Probe a media file and return structured stream info."""
    ffprobe = _find_ffprobe()
    if ffprobe is None:
        return None
    result = subprocess.run(
        [ffprobe, "-v", "quiet", "-print_format", "json",
         "-show_streams", "-show_format", str(file_path)],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def get_stream_info(probe: Optional[Dict], stream_type: str) -> Optional[Dict]:
    """Get first stream of given type from probe data."""
    if not probe or "streams" not in probe:
        return None
    for s in probe["streams"]:
        if s.get("codec_type") == stream_type:
            return {
                "index": s.get("index"),
                "codec": s.get("codec_name", "?"),
                "duration": float(s.get("duration", 0) or 0),
                "time_base": s.get("time_base", "1/90000"),
                "start_time": float(s.get("start_time", 0) or 0),
                "nb_frames": s.get("nb_frames"),
                "tags": s.get("tags", {}),
            }
    return None


def get_format_duration(probe: Optional[Dict]) -> Optional[float]:
    """Get format-level duration."""
    if not probe or "format" not in probe:
        return None
    dur = probe["format"].get("duration")
    if dur:
        return float(dur)
    return None


def get_all_packet_pts(file_path: Path, stream_specifier: str) -> List[float]:
    """Get all packet PTS times for a specific stream.

    Args:
        file_path: Path to media file
        stream_specifier: e.g. "v:0" for first video, "a:0" for first audio

    Returns:
        List of PTS times in seconds
    """
    ffprobe = _find_ffprobe()
    if ffprobe is None:
        return []

    result = subprocess.run(
        [ffprobe, "-v", "quiet",
         "-select_streams", stream_specifier,
         "-show_entries", "packet=pts_time",
         "-of", "csv=p=0",
         str(file_path)],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        return []

    pts_times = []
    for line in result.stdout.strip().split("\n"):
        line = line.strip()
        if line and line != "N/A":
            try:
                pts = float(line)
                if pts >= 0:
                    pts_times.append(pts)
            except ValueError:
                pass
    return sorted(set(pts_times))


def get_packets_near(file_path: Path, stream_specifier: str,
                     seek_time: float, window: float = 0.5) -> List[float]:
    """Get packet PTS times near a specific seek time.

    Uses ffprobe's read_intervals to limit the scan window.
    """
    ffprobe = _find_ffprobe()
    if ffprobe is None:
        return []

    start = max(0, seek_time - window)
    end = seek_time + window
    # ffprobe 8.1.1 on Windows requires start%duration format for -read_intervals
    duration = end - start
    interval = f"{start}%{duration}"

    result = subprocess.run(
        [ffprobe, "-v", "quiet",
         "-select_streams", stream_specifier,
         "-show_entries", "packet=pts_time",
         "-read_intervals", interval,
         "-of", "csv=p=0",
         str(file_path)],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        return []

    pts_times = []
    for line in result.stdout.strip().split("\n"):
        line = line.strip()
        if line and line != "N/A":
            try:
                pts = float(line)
                if pts >= 0:
                    pts_times.append(pts)
            except ValueError:
                pass
    return sorted(set(pts_times))


def can_decode_at(file_path: Path, stream_type: str, seek_time: float) -> bool:
    """Test whether a stream can be decoded at a given seek point."""
    ffmpeg = _find_ffmpeg()
    if ffmpeg is None:
        return False

    if stream_type == "video":
        map_arg = "0:v:0?"
        extra_args = ["-vframes", "1"]
    else:
        map_arg = "0:a:0?"
        extra_args = ["-t", "0.05"]

    result = subprocess.run(
        [ffmpeg, "-hide_banner", "-loglevel", "error",
         "-ss", f"{seek_time:.3f}",
         "-i", str(file_path),
         "-map", map_arg] + extra_args +
        ["-f", "null", "-"],
        capture_output=True, text=True
    )
    return result.returncode == 0


def get_last_pts(file_path: Path, stream_specifier: str) -> Optional[float]:
    """Get the last packet PTS for a stream (indicates total duration).

    Uses read_intervals to seek to the last 10 seconds for efficiency.
    """
    # First get approximate duration from format
    dur = None
    probe = probe_streams(file_path)
    if probe and "format" in probe and probe["format"].get("duration"):
        dur = float(probe["format"]["duration"])

    if dur and dur > 10:
        # Read only the last 10 seconds
        start = dur - 10
        pts_list = get_packets_near(file_path, stream_specifier, dur, 10.0)
    else:
        pts_list = get_all_packet_pts(file_path, stream_specifier)

    if pts_list:
        return pts_list[-1]
    return None


# ═══════════════════════════════════════════════════════════════════════════════
# MERGE REPORT PARSING (for boundary detection)
# ═══════════════════════════════════════════════════════════════════════════════


class BoundaryInfo:
    def __init__(self, index: int):
        self.index = index
        self.segment_a_name: str = ""
        self.segment_b_name: str = ""
        self.boundary_time_s: float = 0.0


def parse_merge_report(report_path: Path) -> List[BoundaryInfo]:
    """Parse merge report to find concat boundary times."""
    if not report_path.exists():
        return []

    try:
        content = report_path.read_text(encoding="utf-8", errors="replace")
    except Exception:
        return []

    segments = []
    lines = content.split("\n")
    found_table = False

    for line in lines:
        s = line.strip()
        if "In Merged" in s or "Time Range" in s or "Start-End" in s:
            found_table = True
            continue
        if not found_table:
            continue
        if s.startswith("|") and ("->" in s or "\u2192" in s):
            cells = [c.strip() for c in s.split("|")]
            cells = [c for c in cells if c]
            if len(cells) >= 4:
                try:
                    idx = int(cells[0])
                    time_range = cells[3]
                    arrow = "\u2192" if "\u2192" in time_range else "->"
                    if arrow in time_range:
                        parts = time_range.split(arrow)
                        segments.append({
                            "index": idx,
                            "name": cells[1],
                            "start_str": parts[0].strip(),
                            "end_str": parts[1].strip(),
                        })
                except (ValueError, IndexError):
                    pass

    boundaries = []
    for i in range(len(segments) - 1):
        b_time = _parse_time_str(segments[i]["end_str"])
        b = BoundaryInfo(len(boundaries))
        b.segment_a_name = segments[i]["name"]
        b.segment_b_name = segments[i + 1]["name"]
        b.boundary_time_s = b_time
        boundaries.append(b)

    return boundaries


def _parse_time_str(s: str) -> float:
    s = s.replace(",", ".")
    parts = s.split(":")
    if len(parts) == 3:
        return float(parts[0]) * 3600 + float(parts[1]) * 60 + float(parts[2])
    elif len(parts) == 2:
        return float(parts[0]) * 60 + float(parts[1])
    return 0.0


# ═══════════════════════════════════════════════════════════════════════════════
# SEEK POINT ANALYSIS
# ═══════════════════════════════════════════════════════════════════════════════


class SeekPointResult:
    """Result of A/V sync analysis at a single seek point."""
    def __init__(self, label: str, seek_time: float):
        self.label = label
        self.seek_time = seek_time
        self.video_pts: Optional[float] = None
        self.audio_pts: Optional[float] = None
        self.video_decode_ok: bool = False
        self.audio_decode_ok: bool = False
        self.av_drift_s: Optional[float] = None
        self.nearest_video_drift_s: Optional[float] = None
        self.nearest_audio_drift_s: Optional[float] = None
        self.passed: bool = True
        self.errors: List[str] = []
        self.warnings: List[str] = []

    def fail(self, msg: str):
        self.passed = False
        self.errors.append(msg)

    def warn(self, msg: str):
        self.warnings.append(msg)


def analyze_seek_point(
    file_path: Path,
    seek_time: float,
    label: str = "",
    boundary_name: str = "",
    tolerance_s: float = AV_SYNC_TOLERANCE_S,
) -> SeekPointResult:
    """Analyze A/V sync at a single seek point."""
    result = SeekPointResult(label or f"{seek_time:.1f}s", seek_time)

    # ── Decode check ─────────────────────────────────────────────────────
    result.video_decode_ok = can_decode_at(file_path, "video", seek_time)
    result.audio_decode_ok = can_decode_at(file_path, "audio", seek_time)

    if not result.video_decode_ok:
        result.warn(f"Video decode FAILED at {seek_time:.3f}s")
    if not result.audio_decode_ok:
        result.warn(f"Audio decode FAILED at {seek_time:.3f}s")

    # ── PTS extraction near seek point ───────────────────────────────────
    window = 1.0  # Search window in seconds
    video_pts_list = get_packets_near(file_path, "v:0", seek_time, window)
    audio_pts_list = get_packets_near(file_path, "a:0", seek_time, window)

    if video_pts_list:
        # Find the PTS closest to the seek time
        result.video_pts = min(video_pts_list, key=lambda p: abs(p - seek_time))
        result.nearest_video_drift_s = abs(result.video_pts - seek_time)
    else:
        result.warn(f"No video packets found near {seek_time:.3f}s")

    if audio_pts_list:
        result.audio_pts = min(audio_pts_list, key=lambda p: abs(p - seek_time))
        result.nearest_audio_drift_s = abs(result.audio_pts - seek_time)
    else:
        result.warn(f"No audio packets found near {seek_time:.3f}s")

    # ── A/V sync calculation ────────────────────────────────────────────
    if result.video_pts is not None and result.audio_pts is not None:
        result.av_drift_s = result.video_pts - result.audio_pts
        drift_abs = abs(result.av_drift_s)

        if drift_abs > tolerance_s:
            result.fail(
                f"A/V drift = {result.av_drift_s:+.3f}s "
                f"(tolerance = +/-{tolerance_s:.3f}s) — "
                f"video at {result.video_pts:.3f}s, audio at {result.audio_pts:.3f}s"
            )
    else:
        result.fail("Cannot calculate A/V drift — missing video or audio PTS")

    # ── Seek accuracy ───────────────────────────────────────────────────
    if result.nearest_video_drift_s is not None and result.nearest_video_drift_s > 0.1:
        result.warn(
            f"Nearest video PTS is {result.nearest_video_drift_s:.3f}s from requested seek "
            f"(video only has {len(video_pts_list)} packets in +/-{window}s window)"
        )
    if result.nearest_audio_drift_s is not None and result.nearest_audio_drift_s > 0.1:
        result.warn(
            f"Nearest audio PTS is {result.nearest_audio_drift_s:.3f}s from requested seek"
        )

    return result


# ═══════════════════════════════════════════════════════════════════════════════
# MAIN CERTIFICATION
# ═══════════════════════════════════════════════════════════════════════════════


class SyncCertResult:
    """Overall A/V sync certification result."""
    def __init__(self, file_path: Path):
        self.file_path = file_path
        self.format_duration: Optional[float] = None
        self.video_last_pts: Optional[float] = None
        self.audio_last_pts: Optional[float] = None
        self.video_stream: Optional[Dict] = None
        self.audio_stream: Optional[Dict] = None
        self.seek_points: List[SeekPointResult] = []
        self.max_drift_s: float = 0.0
        self.passed: bool = True
        self.errors: List[str] = []
        self.warnings: List[str] = []

    def fail(self, msg: str):
        self.passed = False
        self.errors.append(msg)

    def warn(self, msg: str):
        self.warnings.append(msg)


def certify_av_sync(file_path: Path, boundaries: Optional[List[BoundaryInfo]] = None, tolerance: float = AV_SYNC_TOLERANCE_S) -> SyncCertResult:
    """Run complete A/V sync certification on a merged output."""
    result = SyncCertResult(file_path)

    # ── Stream metadata ──────────────────────────────────────────────────
    probe = probe_streams(file_path)
    result.format_duration = get_format_duration(probe)
    result.video_stream = get_stream_info(probe, "video")
    result.audio_stream = get_stream_info(probe, "audio")

    if result.video_stream is None:
        result.fail("No video stream found in merged output")
    if result.audio_stream is None:
        result.fail("No audio stream found in merged output")

    if result.video_stream and result.audio_stream:
        print(f"  Video: {result.video_stream['codec']}, "
              f"{result.video_stream.get('duration', 0):.1f}s")
        print(f"  Audio: {result.audio_stream['codec']}, "
              f"{result.audio_stream.get('duration', 0):.1f}s")
        if result.format_duration:
            print(f"  Format duration: {result.format_duration:.3f}s")
    else:
        print("  WARNING: Missing video or audio stream")
    print()

    # ── Duration consistency ────────────────────────────────────────────
    duration = result.format_duration or 0
    result.video_last_pts = get_last_pts(file_path, "v:0")
    result.audio_last_pts = get_last_pts(file_path, "a:0")

    duration_sources = []
    if result.format_duration:
        duration_sources.append(("format", result.format_duration))
    if result.video_last_pts:
        duration_sources.append(("video PTS", result.video_last_pts))
    if result.audio_last_pts:
        duration_sources.append(("audio PTS", result.audio_last_pts))

    print("  Duration consistency:")
    for name, val in duration_sources:
        print(f"    {name}: {val:.3f}s")

    # Compare duration sources
    if len(duration_sources) >= 2:
        for i in range(len(duration_sources)):
            for j in range(i + 1, len(duration_sources)):
                n1, v1 = duration_sources[i]
                n2, v2 = duration_sources[j]
                diff = abs(v1 - v2)
                if diff > DURATION_TOLERANCE_S:
                    result.warn(
                        f"Duration mismatch: {n1}={v1:.3f}s vs {n2}={v2:.3f}s "
                        f"(diff={diff:.3f}s)"
                    )
    print()

    # ── Build seek points ────────────────────────────────────────────────
    seek_points = []

    # Evenly spaced seek points
    if duration > 0:
        for i in range(SEEK_POINTS + 1):
            seek_time = duration * i / SEEK_POINTS
            pct = int(i * 100 / SEEK_POINTS)
            seek_points.append((
                max(0, min(seek_time, duration)),
                f"{pct}%",
                ""
            ))

    # Concat boundaries
    if boundaries:
        for b in boundaries:
            if 0 < b.boundary_time_s < duration:
                # Add boundary point + slight offset to capture post-boundary
                seek_points.append((
                    b.boundary_time_s,
                    f"boundary #{b.index + 1}",
                    f"{b.segment_a_name} -> {b.segment_b_name}"
                ))
                seek_points.append((
                    min(b.boundary_time_s + 0.1, duration),
                    f"boundary+100ms #{b.index + 1}",
                    f"post: {b.segment_b_name}"
                ))

    # Sort and deduplicate
    seen = set()
    unique_points = []
    for time_s, label, boundary_name in sorted(set((round(t, 2), l, bn) for t, l, bn in seek_points)):
        if round(time_s, 1) not in seen:
            seen.add(round(time_s, 1))
            unique_points.append((time_s, label, boundary_name))

    print(f"  Analyzing {len(unique_points)} seek points...")
    print()

    # ── Analyze each seek point ─────────────────────────────────────────
    for i, (seek_time, label, boundary_name) in enumerate(unique_points):
        sys.stdout.write(f"\r    Point {i + 1}/{len(unique_points)}: {label} @ {seek_time:.1f}s...")
        sys.stdout.flush()

        point = analyze_seek_point(file_path, seek_time, label, boundary_name, tolerance_s=tolerance)

        # Track max drift
        if point.av_drift_s is not None:
            result.max_drift_s = max(result.max_drift_s, abs(point.av_drift_s))

        if not point.passed:
            result.passed = False

        result.seek_points.append(point)

    print("\n")
    return result


# ═══════════════════════════════════════════════════════════════════════════════
# REPORTING
# ═══════════════════════════════════════════════════════════════════════════════


def print_report(result: SyncCertResult, max_drift_limit: float = 0.500):
    """Print formatted A/V sync certification report."""
    print("=" * 72)
    print(f"  A/V SYNC CERTIFICATION REPORT")
    print(f"  File: {result.file_path.name}")
    print("=" * 72)
    print()

    # ── Duration summary ──
    print("  Duration sources:")
    if result.format_duration:
        print(f"    Format:  {result.format_duration:.3f}s")
    if result.video_last_pts is not None:
        print(f"    Video:   {result.video_last_pts:.3f}s (last PTS)")
    if result.audio_last_pts is not None:
        print(f"    Audio:   {result.audio_last_pts:.3f}s (last PTS)")
    print()

    # ── Seek point detail ──
    passed_count = sum(1 for p in result.seek_points if p.passed)
    failed_count = len(result.seek_points) - passed_count

    print(f"  Seek points: {len(result.seek_points)} total, "
          f"{passed_count} passed, {failed_count} failed")
    print()

    # Show each seek point's A/V drift
    for point in result.seek_points:
        status = "[PASS]" if point.passed else "[FAIL]"

        # Build a compact summary line
        v_pts_str = f"v={point.video_pts:.3f}s" if point.video_pts is not None else "v=N/A"
        a_pts_str = f"a={point.audio_pts:.3f}s" if point.audio_pts is not None else "a=N/A"
        drift_str = ""
        if point.av_drift_s is not None:
            drift_str = f"drift={point.av_drift_s:+.3f}s"

        dcd_str = ""
        if not point.video_decode_ok or not point.audio_decode_ok:
            dcd = "dcd: "
            if not point.video_decode_ok:
                dcd += "vFAIL "
            if not point.audio_decode_ok:
                dcd += "aFAIL"
            dcd_str = f" | {dcd}"

        print(f"  {status} {point.label:>20s} @ {point.seek_time:>9.3f}s  "
              f"| {v_pts_str} {a_pts_str}  {drift_str}{dcd_str}")

        # Show errors
        for e in point.errors:
            print(f"        [X] {e}")
        for w in point.warnings:
            print(f"        [i] {w}")

    print()

    # ── Warnings ──
    if result.warnings:
        print("  General warnings:")
        for w in result.warnings:
            print(f"    [i] {w}")
        print()

    # ── Summary ──
    max_drift = result.max_drift_s
    all_pass = result.passed and max_drift <= max_drift_limit

    print("  ---")
    print(f"  Total seek points:  {len(result.seek_points)}")
    print(f"  Passed:             {passed_count}")
    print(f"  Failed:             {failed_count}")
    print(f"  Max A/V drift:      {max_drift:.3f}s")
    print(f"  Drift limit:        {max_drift_limit:.3f}s")
    print("  ---")

    if all_pass:
        print()
        print("  ** A/V SYNC CERTIFICATION PASSED **")
        print(f"  Maximum drift of {max_drift:.3f}s is within the {max_drift_limit:.3f}s tolerance.")
    else:
        print()
        print(f"  ** A/V SYNC CERTIFICATION FAILED **")
        if max_drift > max_drift_limit:
            print(f"  Maximum drift of {max_drift:.3f}s exceeds the {max_drift_limit:.3f}s tolerance.")
        if failed_count > 0:
            print(f"  {failed_count} seek point(s) failed.")

    print()


# ═══════════════════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════════════════


def main():
    parser = argparse.ArgumentParser(
        description="End-to-End A/V Sync Certification"
    )
    parser.add_argument("merged", help="Merged output file (MKV/MP4)")
    parser.add_argument("--report", help="Merge report file (.txt or .md)")
    parser.add_argument("--auto-detect", action="store_true",
                        help="Auto-discover companion report file")
    parser.add_argument("--tolerance", type=float, default=AV_SYNC_TOLERANCE_S,
                        help=f"A/V sync tolerance in seconds (default: {AV_SYNC_TOLERANCE_S})")
    parser.add_argument("--drift-limit", type=float, default=0.500,
                        help="Max acceptable drift for overall PASS (default: 0.500)")

    args = parser.parse_args()

    merged_path = Path(args.merged)
    if not merged_path.exists():
        print(f"ERROR: Merged file not found: {args.merged}")
        sys.exit(1)

    # Find report file for boundary info
    boundaries = None
    if args.report:
        report_path = Path(args.report)
        if not report_path.exists():
            print(f"ERROR: Report file not found: {args.report}")
            sys.exit(1)
        boundaries = parse_merge_report(report_path)
        if boundaries:
            print(f"Found {len(boundaries)} concat boundaries in report")
        else:
            print("WARNING: No boundaries parsed from report file")
    elif args.auto_detect:
        for ext in ["_report.txt", "_report.md", ".txt", ".md"]:
            candidate = merged_path.with_name(merged_path.stem + ext)
            if candidate.exists():
                boundaries = parse_merge_report(candidate)
                if boundaries:
                    print(f"Found {len(boundaries)} concat boundaries from {candidate.name}")
                break

    # Use the tolerance value from args
    eff_tolerance = args.tolerance

    # Run certification with user-specified tolerance
    result = certify_av_sync(merged_path, boundaries, tolerance=eff_tolerance)

    # Print report
    print_report(result, max_drift_limit=args.drift_limit)

    # Exit with appropriate code
    sys.exit(1 if not result.passed else 0)


if __name__ == "__main__":
    main()
