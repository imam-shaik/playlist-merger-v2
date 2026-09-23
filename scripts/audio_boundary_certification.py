#!/usr/bin/env python3
"""
AUDIO BOUNDARY PRODUCTION CERTIFICATION
═══════════════════════════════════════════

Determines why some merged outputs contain brief audio dropouts, silence,
or metallic "kee kee" artifacts at concat boundaries.

For every concat boundary between consecutive video segments:
  1. Decode the last 2 seconds of audio from the previous segment
  2. Decode the first 2 seconds of audio from the next segment
  3. Decode the corresponding region from the final merged output
  4. Compare all three and report PASS/FAIL per boundary

Audit categories:
  • AAC encoder delay / skip_samples / discard_padding
  • Packet continuity, loss, duplication
  • PTS / DTS continuity
  • Waveform continuity (RMS, peak, silence, clipping)
  • Stream metadata (channel layout, sample rate, codec, bitrate)
  • FFmpeg decoder warnings / stderr

Usage:
  python scripts/audio_boundary_certification.py <merged.mkv> [--report <report.txt>] [--inputs <input1.mp4> <input2.mp4> ...]
  python scripts/audio_boundary_certification.py <merged.mkv> --auto-detect

The --auto-detect flag looks for a companion report file (<merged>_report.txt or .md)
to discover input files and boundaries automatically.

Output:
  Generates a forensic report per boundary and a final PASS/FAIL summary.
"""

import argparse
import json
import math
import os
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Dict, List, Optional, Tuple, Any


# ═══════════════════════════════════════════════════════════════════════════════
# CONFIGURATION
# ═══════════════════════════════════════════════════════════════════════════════

# Seconds of audio to extract on each side of a boundary
BOUNDARY_WINDOW_S = 2.0

# Thresholds
SILENCE_RMS_THRESHOLD = 0.005      # RMS below this = silence
CLIPPING_SAMPLE_THRESHOLD = 0.99   # |sample| above this = clipping
CORRELATION_MIN = 0.85             # Minimum waveform correlation to PASS
DRIFT_TOLERANCE_S = 0.050          # 50ms PTS drift tolerance
MAX_EXPECTED_PACKET_GAP_S = 0.100  # Max expected gap between packets at boundary


# ═══════════════════════════════════════════════════════════════════════════════
# FFMPEG / FFPROBE HELPERS
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


def get_audio_stream_info(probe_data: Optional[Dict]) -> Optional[Dict]:
    """Extract first audio stream info from probe data."""
    if not probe_data or "streams" not in probe_data:
        return None
    for s in probe_data["streams"]:
        if s.get("codec_type") == "audio":
            info = {
                "index": s.get("index", -1),
                "codec": s.get("codec_name", "unknown"),
                "sample_rate": int(s.get("sample_rate", 0)),
                "channels": s.get("channels", 0),
                "channel_layout": s.get("channel_layout", "unknown"),
                "bitrate": s.get("bit_rate", "N/A"),
                "duration": float(s.get("duration", 0)),
                "start_pts": s.get("start_pts"),
                "start_time": float(s.get("start_time", 0)),
                "bits_per_sample": s.get("bits_per_sample", 0),
            }
            # Check for encoder delay / skip_samples / discard_padding
            # FFmpeg may report these in side_data_list
            side_data = s.get("side_data_list", [])
            for sd in side_data:
                sd_type = sd.get("side_data_type", "")
                if "skip" in sd_type.lower():
                    info["skip_samples"] = sd.get("skip_samples", 0)
                if "discard" in sd_type.lower():
                    info["discard_padding"] = sd.get("discard_padding", 0)
            return info
    return None


def get_format_info(probe_data: Optional[Dict]) -> Optional[Dict]:
    """Extract format-level info from probe data."""
    if not probe_data or "format" not in probe_data:
        return None
    f = probe_data["format"]
    return {
        "duration": float(f.get("duration", 0)),
        "bitrate": f.get("bit_rate", "N/A"),
        "size": int(f.get("size", 0)),
    }


def extract_audio_segment(
    file_path: Path,
    start_time: float,
    duration: float,
    output_path: Path,
    sample_rate: int = 48000,
) -> Tuple[bool, str]:
    """Extract a segment of audio from a file as raw PCM s16le.

    Returns (success, stderr_output).
    """
    ffmpeg = _find_ffmpeg()
    if ffmpeg is None:
        return False, "FFmpeg not found"

    result = subprocess.run(
        [ffmpeg, "-y",
         "-ss", str(start_time),
         "-i", str(file_path),
         "-t", str(duration),
         "-vn",              # No video
         "-map", "0:a:0",    # First audio stream
         "-c:a", "pcm_s16le",
         "-ar", str(sample_rate),
         "-ac", "1",         # Mono for consistent channel-accurate comparison
         "-f", "s16le",
         str(output_path)],
        capture_output=True, text=True
    )
    return result.returncode == 0, result.stderr


def decode_with_decoder_warnings(
    file_path: Path,
    start_time: float,
    duration: float,
    output_path: Path,
) -> Tuple[bool, str]:
    """Decode audio and capture FFmpeg stderr for decoder warnings.

    Uses the same approach as extract_audio_segment but preserves
    the complete stderr output for decoder warning analysis.
    """
    ffmpeg = _find_ffmpeg()
    if ffmpeg is None:
        return False, "FFmpeg not found"

    result = subprocess.run(
        [ffmpeg, "-y",
         "-ss", str(start_time),
         "-i", str(file_path),
         "-t", str(duration),
         "-vn",
         "-map", "0:a:0",
         "-c:a", "pcm_s16le",
         "-ar", "48000",
         "-ac", "1",         # Mono for waveform comparison accuracy
         "-f", "s16le",
         str(output_path)],
        capture_output=True, text=True
    )
    return result.returncode == 0, result.stderr


def get_packet_level_info(file_path: Path) -> Dict[str, Any]:
    """Get packet-level information for the audio stream using ffprobe.

    Reports packet count, PTS/DTS continuity, gaps, and duplicates.
    """
    ffprobe = _find_ffprobe()
    if ffprobe is None:
        return {"error": "ffprobe not found"}

    # Use ffprobe to dump packets for the first audio stream
    result = subprocess.run(
        [ffprobe, "-v", "quiet", "-print_format", "json",
         "-show_packets", "-select_streams", "a:0",
         str(file_path)],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        return {"error": f"ffprobe failed: {result.stderr[:200]}"}

    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        return {"error": "JSON parse failed"}

    packets = data.get("packets", [])
    if not packets:
        return {"total_packets": 0, "warning": "No audio packets found"}

    info = {
        "total_packets": len(packets),
        "pts_gaps": [],
        "dts_gaps": [],
        "duplicate_pts": 0,
        "missing_packets": 0,
    }

    seen_pts = set()
    prev_pts = None
    prev_dts = None

    for pkt in packets:
        pts = pkt.get("pts")
        dts = pkt.get("dts")
        duration = float(pkt.get("duration", 0))

        if pts is not None:
            if pts in seen_pts:
                info["duplicate_pts"] += 1
            seen_pts.add(pts)
            if prev_pts is not None:
                gap = pts - prev_pts
                if gap > duration * 1.5:  # Gap larger than expected
                    info["pts_gaps"].append({
                        "from_pts": prev_pts,
                        "to_pts": pts,
                        "gap": gap,
                        "gap_seconds": gap / 90000 if pkt.get("time_base") == "1/90000" else gap / 48000,
                    })
            prev_pts = pts

        if dts is not None:
            if prev_dts is not None:
                gap = dts - prev_dts
                if gap > duration * 1.5:
                    info["dts_gaps"].append({
                        "from_dts": prev_dts,
                        "to_dts": dts,
                        "gap": gap,
                    })
            prev_dts = dts

    # Check for initial encoder delay (first few packets with small duration)
    if len(packets) >= 3:
        first_durations = [float(p.get("duration", 0)) for p in packets[:5]]
        avg_first_dur = sum(first_durations) / len(first_durations)
        info["encoder_delay_hint"] = f"First 5 packets avg duration: {avg_first_dur:.6f}"

    return info


def get_astats(file_path: Path, start_time: float, duration: float) -> Optional[Dict]:
    """Get audio statistics for a segment using FFmpeg's astats filter."""
    ffmpeg = _find_ffmpeg()
    if ffmpeg is None:
        return None

    result = subprocess.run(
        [ffmpeg, "-y",
         "-ss", str(start_time),
         "-i", str(file_path),
         "-t", str(duration),
         "-vn",
         "-map", "0:a:0",
         "-af", "astats=metadata=1:reset=1",
         "-f", "null", "-"],
        capture_output=True, text=True
    )

    # Parse astats output from stderr
    stats = {}
    for line in result.stderr.split("\n"):
        line = line.strip()
        if ":" in line and ("_rms" in line or "_peak" in line or "DC_offset" in line
                            or "Crest_factor" in line or "Flat_factor" in line
                            or "Bit_depth" in line or "Dynamic_range" in line):
            parts = line.split(":", 1)
            key = parts[0].strip().split(".")[-1]  # Get last part after channel prefix
            value = parts[1].strip()
            try:
                value = float(value)
            except ValueError:
                pass
            stats[key] = value

    return stats if stats else None


# ═══════════════════════════════════════════════════════════════════════════════
# PCM ANALYSIS
# ═══════════════════════════════════════════════════════════════════════════════


def read_pcm_s16le(path: Path) -> Optional[bytes]:
    """Read raw PCM s16le data from a file."""
    try:
        return path.read_bytes()
    except Exception:
        return None


def pcm_to_samples(data: bytes) -> Optional[List[float]]:
    """Convert raw s16le PCM bytes to normalized float samples (-1.0 to 1.0)."""
    try:
        count = len(data) // 2
        samples = []
        for i in range(count):
            raw = struct.unpack_from("<h", data, i * 2)[0]
            samples.append(raw / 32768.0)
        return samples
    except Exception:
        return None


def compute_rms(samples: List[float]) -> float:
    """Compute RMS (root mean square) of samples."""
    if not samples:
        return 0.0
    sq_sum = sum(s * s for s in samples)
    return math.sqrt(sq_sum / len(samples))


def compute_peak(samples: List[float]) -> float:
    """Compute peak absolute sample value."""
    if not samples:
        return 0.0
    return max(abs(s) for s in samples)


def detect_silence(samples: List[float], threshold: float = SILENCE_RMS_THRESHOLD) -> Tuple[bool, float]:
    """Detect if the audio segment is silence.

    Returns (is_silent, rms_level).
    """
    rms = compute_rms(samples)
    return rms < threshold, rms


def detect_clipping(samples: List[float], threshold: float = CLIPPING_SAMPLE_THRESHOLD) -> Tuple[int, float]:
    """Detect clipped samples.

    Returns (clipped_count, max_peak).
    """
    peak = compute_peak(samples)
    clipped = sum(1 for s in samples if abs(s) >= threshold)
    return clipped, peak


def compute_correlation(a: List[float], b: List[float]) -> float:
    """Compute Pearson correlation coefficient between two sample arrays."""
    n = min(len(a), len(b))
    if n < 100:
        return 0.0

    a = a[:n]
    b = b[:n]

    mean_a = sum(a) / n
    mean_b = sum(b) / n

    cov = sum((a[i] - mean_a) * (b[i] - mean_b) for i in range(n))
    var_a = sum((x - mean_a) ** 2 for x in a)
    var_b = sum((x - mean_b) ** 2 for x in b)

    if var_a == 0 or var_b == 0:
        return 1.0 if var_a == var_b else 0.0

    return cov / math.sqrt(var_a * var_b)


def cross_correlation(a: List[float], b: List[float], max_shift: int = 100) -> Tuple[float, int]:
    """Find best cross-correlation and sample shift between two signals.

    Returns (max_correlation, shift_in_samples).
    Positive shift means 'a' is ahead of 'b'.
    """
    n = min(len(a), len(b))
    if n < 200:
        return 0.0, 0

    a = a[:n]
    b = b[:n]

    best_corr = -1.0
    best_shift = 0

    for shift in range(-max_shift, max_shift + 1):
        if shift < 0:
            start = -shift
            end_a = n + shift
            corr = compute_correlation(a[start:], b[:end_a])
        elif shift > 0:
            start = shift
            end_b = n - shift
            corr = compute_correlation(a[:end_b], b[start:])
        else:
            corr = compute_correlation(a, b)

        if corr > best_corr:
            best_corr = corr
            best_shift = shift

    return best_corr, best_shift


# ═══════════════════════════════════════════════════════════════════════════════
# MERGE REPORT PARSING
# ═══════════════════════════════════════════════════════════════════════════════


class BoundaryInfo:
    """Information about a single concat boundary."""
    def __init__(self, boundary_index: int):
        self.index = boundary_index
        self.segment_a_name: str = ""
        self.segment_b_name: str = ""
        self.segment_a_dur_s: float = 0.0
        self.segment_b_dur_s: float = 0.0
        self.boundary_time_s: float = 0.0  # Time in merged output
        self.segment_a_path: Optional[Path] = None
        self.segment_b_path: Optional[Path] = None


def parse_merge_report(report_path: Path) -> Tuple[List[BoundaryInfo], List[Dict]]:
    """Parse a merge report to find concat boundaries and segment info.

    Returns (boundaries, segments).
    """
    if not report_path.exists():
        return [], []

    try:
        content = report_path.read_text(encoding="utf-8", errors="replace")
    except Exception:
        return [], []

    segments = []
    lines = content.split("\n")
    found_table = False

    for line in lines:
        line_stripped = line.strip()
        if "In Merged" in line_stripped or "Time Range" in line_stripped or "Start-End" in line_stripped:
            found_table = True
            continue

        if not found_table:
            continue

        if line_stripped.startswith("|") and ("\u2192" in line_stripped or "->" in line_stripped):
            # Parse table row with arrow (→)
            cells = [c.strip() for c in line_stripped.split("|")]
            cells = [c for c in cells if c]

            if len(cells) >= 4:
                try:
                    idx = int(cells[0])
                    # Parse time range like "00:00:00 → 00:08:33"
                    time_range = cells[3]
                    if "\u2192" in time_range:
                        parts = time_range.split("\u2192")
                        start_str = parts[0].strip()
                        end_str = parts[1].strip()
                    elif "->" in time_range:
                        parts = time_range.split("->")
                        start_str = parts[0].strip()
                        end_str = parts[1].strip()
                    else:
                        continue

                    segments.append({
                        "index": idx,
                        "name": cells[1],
                        "duration_str": cells[2],
                        "start_str": start_str,
                        "end_str": end_str,
                    })
                except (ValueError, IndexError):
                    pass

    # Build boundaries from adjacent segments
    boundaries = []
    for i in range(len(segments) - 1):
        sa = segments[i]
        sb = segments[i + 1]

        # Calculate boundary time (end of segment A = start of segment B)
        b_time = _parse_time_str(sa["end_str"])

        boundary = BoundaryInfo(len(boundaries))
        boundary.segment_a_name = sa["name"]
        boundary.segment_b_name = sb["name"]
        boundary.segment_a_dur_s = _parse_duration_str(sa["duration_str"])
        boundary.segment_b_dur_s = _parse_duration_str(sb["duration_str"])
        boundary.boundary_time_s = b_time
        boundaries.append(boundary)

    return boundaries, segments


def _parse_time_str(s: str) -> float:
    """Parse HH:MM:SS or HH:MM:SS.mmm to seconds."""
    s = s.replace(",", ".")
    parts = s.split(":")
    if len(parts) == 3:
        return float(parts[0]) * 3600 + float(parts[1]) * 60 + float(parts[2])
    elif len(parts) == 2:
        return float(parts[0]) * 60 + float(parts[1])
    return 0.0


def _parse_duration_str(s: str) -> float:
    """Parse duration string (e.g., '513.0s', '00:08:33') to seconds."""
    s = s.strip().lower()
    if s.endswith("s"):
        try:
            return float(s[:-1])
        except ValueError:
            pass
    return _parse_time_str(s)


# ═══════════════════════════════════════════════════════════════════════════════
# BOUNDARY CERTIFICATION
# ═══════════════════════════════════════════════════════════════════════════════


class BoundaryResult:
    """Result of certifying a single concat boundary."""
    def __init__(self, boundary_index: int):
        self.index = boundary_index
        self.segment_a_name: str = ""
        self.segment_b_name: str = ""
        self.boundary_time_s: float = 0.0
        self.segment_a_ok: bool = False
        self.segment_b_ok: bool = False
        self.merged_a_ok: bool = False
        self.merged_b_ok: bool = False

        # Stream metadata
        self.input_a_audio: Optional[Dict] = None
        self.input_b_audio: Optional[Dict] = None
        self.merged_audio: Optional[Dict] = None

        # Packet-level analysis
        self.input_a_packets: Dict = {}
        self.input_b_packets: Dict = {}
        self.merged_packets: Dict = {}

        # PCM analysis
        self.rms_a_original: Optional[float] = None
        self.rms_a_merged: Optional[float] = None
        self.rms_b_original: Optional[float] = None
        self.rms_b_merged: Optional[float] = None
        self.peak_a_original: Optional[float] = None
        self.peak_a_merged: Optional[float] = None
        self.peak_b_original: Optional[float] = None
        self.peak_b_merged: Optional[float] = None
        self.clip_a_original: int = 0
        self.clip_a_merged: int = 0
        self.clip_b_original: int = 0
        self.clip_b_merged: int = 0
        self.silence_a_original: bool = False
        self.silence_a_merged: bool = False
        self.silence_b_original: bool = False
        self.silence_b_merged: bool = False

        # Waveform correlation (original vs merged)
        self.corr_a: float = 0.0
        self.corr_b: float = 0.0
        self.shift_a: int = 0
        self.shift_b: int = 0
        self.shift_a_s: float = 0.0
        self.shift_b_s: float = 0.0

        # Decoder warnings
        self.decoder_warnings_a: str = ""
        self.decoder_warnings_merged_a: str = ""
        self.decoder_warnings_b: str = ""
        self.decoder_warnings_merged_b: str = ""

        # Comprehensive checks
        self.checks: List[Dict] = []
        self.errors: List[str] = []
        self.warnings: List[str] = []
        self.passed: bool = True

    def fail(self, check: str, detail: str):
        self.passed = False
        self.errors.append(f"[FAIL] {check}: {detail}")
        self.checks.append({"check": check, "passed": False, "detail": detail})

    def warn(self, check: str, detail: str):
        self.warnings.append(f"[WARN] {check}: {detail}")
        self.checks.append({"check": check, "passed": True, "detail": f"WARNING: {detail}"})

    def pass_check(self, check: str, detail: str):
        self.checks.append({"check": check, "passed": True, "detail": detail})


def certify_boundary(
    boundary: BoundaryInfo,
    merged_path: Path,
    input_dir: Optional[Path] = None,
    temp_dir: Optional[Path] = None,
) -> BoundaryResult:
    """Certify a single concat boundary with full forensic analysis."""
    result = BoundaryResult(boundary.index)
    result.segment_a_name = boundary.segment_a_name
    result.segment_b_name = boundary.segment_b_name
    result.boundary_time_s = boundary.boundary_time_s

    if temp_dir is None:
        temp_dir = Path(tempfile.gettempdir()) / "audio_boundary_cert"

    temp_dir.mkdir(parents=True, exist_ok=True)

    b_time = boundary.boundary_time_s
    a_end = b_time
    b_start = b_time

    # ── Find input files ─────────────────────────────────────────────────────
    input_a_path = find_input_file(boundary.segment_a_name, input_dir)
    input_b_path = find_input_file(boundary.segment_b_name, input_dir)

    if input_a_path is None:
        result.fail("INPUT_A_NOT_FOUND", f"Cannot locate input file for '{boundary.segment_a_name}'")
        # Can still analyse merged output
    if input_b_path is None:
        result.fail("INPUT_B_NOT_FOUND", f"Cannot locate input file for '{boundary.segment_b_name}'")

    # ── Stage 1: Stream metadata ──────────────────────────────────────────────
    if input_a_path:
        probe_a = probe_streams(input_a_path)
        result.input_a_audio = get_audio_stream_info(probe_a)
        if result.input_a_audio:
            result.pass_check("INPUT_A_STREAMS",
                f"codec={result.input_a_audio['codec']}, sr={result.input_a_audio['sample_rate']}, "
                f"ch={result.input_a_audio['channels']}, layout={result.input_a_audio['channel_layout']}")
        else:
            result.warn("INPUT_A_STREAMS", "No audio stream found in input A")

    if input_b_path:
        probe_b = probe_streams(input_b_path)
        result.input_b_audio = get_audio_stream_info(probe_b)
        if result.input_b_audio:
            result.pass_check("INPUT_B_STREAMS",
                f"codec={result.input_b_audio['codec']}, sr={result.input_b_audio['sample_rate']}, "
                f"ch={result.input_b_audio['channels']}, layout={result.input_b_audio['channel_layout']}")
        else:
            result.warn("INPUT_B_STREAMS", "No audio stream found in input B")

    # Probe merged output
    probe_merged = probe_streams(merged_path)
    result.merged_audio = get_audio_stream_info(probe_merged)
    if result.merged_audio:
        result.pass_check("MERGED_STREAMS",
            f"codec={result.merged_audio['codec']}, sr={result.merged_audio['sample_rate']}, "
            f"ch={result.merged_audio['channels']}, layout={result.merged_audio['channel_layout']}")
    else:
        result.fail("MERGED_STREAMS", "No audio stream found in merged output")

    # Compare stream compatibility
    if result.input_a_audio and result.input_b_audio:
        compat = check_stream_compatibility(result.input_a_audio, result.input_b_audio,
                                             result.merged_audio)
        for check_name, check_detail, check_passed in compat:
            if check_passed:
                result.pass_check(check_name, check_detail)
            else:
                result.warn(check_name, check_detail)

    # ── Stage 2: Packet-level analysis ────────────────────────────────────────
    if input_a_path:
        result.input_a_packets = get_packet_level_info(input_a_path)
        result.pass_check("INPUT_A_PACKETS",
            f"{result.input_a_packets.get('total_packets', '?')} packets, "
            f"{result.input_a_packets.get('duplicate_pts', 0)} dup PTS, "
            f"{len(result.input_a_packets.get('pts_gaps', []))} PTS gaps")
        if result.input_a_packets.get("duplicate_pts", 0) > 0:
            result.warn("INPUT_A_PACKETS", f"{result.input_a_packets['duplicate_pts']} duplicate PTS values")
        if result.input_a_packets.get("pts_gaps", []):
            max_gap = max(g["gap_seconds"] for g in result.input_a_packets["pts_gaps"])
            if max_gap > MAX_EXPECTED_PACKET_GAP_S:
                result.warn("INPUT_A_PTS_GAP", f"Max PTS gap: {max_gap:.3f}s")

    if input_b_path:
        result.input_b_packets = get_packet_level_info(input_b_path)
        result.pass_check("INPUT_B_PACKETS",
            f"{result.input_b_packets.get('total_packets', '?')} packets, "
            f"{result.input_b_packets.get('duplicate_pts', 0)} dup PTS, "
            f"{len(result.input_b_packets.get('pts_gaps', []))} PTS gaps")

    # Packet-level analysis on merged output
    result.merged_packets = get_packet_level_info(merged_path)
    result.pass_check("MERGED_PACKETS",
        f"{result.merged_packets.get('total_packets', '?')} total packets, "
        f"{result.merged_packets.get('duplicate_pts', 0)} dup PTS, "
        f"{len(result.merged_packets.get('pts_gaps', []))} PTS gaps")

    # ── Stage 3: PCM-level analysis ──────────────────────────────────────────
    sample_rate = 48000
    # Extract audio regions
    region_a_end = max(0, a_end - BOUNDARY_WINDOW_S)
    region_b_start = b_start
    region_a_merged = max(0, b_time - BOUNDARY_WINDOW_S)

    # Input A: last 2 seconds
    if input_a_path and result.input_a_audio:
        dur_a = result.input_a_audio["duration"]
        a_last_start = max(0, dur_a - BOUNDARY_WINDOW_S)
        pcm_a_orig = temp_dir / f"b{boundary.index}_a_orig.pcm"
        ok_a, stderr_a = decode_with_decoder_warnings(input_a_path, a_last_start, BOUNDARY_WINDOW_S, pcm_a_orig)
        result.decoder_warnings_a = extract_decoder_warnings(stderr_a)
        result.segment_a_ok = ok_a
        if ok_a:
            data = read_pcm_s16le(pcm_a_orig)
            if data:
                samples = pcm_to_samples(data)
                if samples:
                    result.rms_a_original = compute_rms(samples)
                    result.peak_a_original = compute_peak(samples)
                    result.silence_a_original, _ = detect_silence(samples)
                    result.clip_a_original, _ = detect_clipping(samples)
                else:
                    result.warn("INPUT_A_DECODE", "PCM decode produced no samples")
        else:
            result.warn("INPUT_A_DECODE", f"FFmpeg decode stderr: {stderr_a[:300]}")

    # Input B: first 2 seconds
    if input_b_path:
        pcm_b_orig = temp_dir / f"b{boundary.index}_b_orig.pcm"
        ok_b, stderr_b = decode_with_decoder_warnings(input_b_path, 0, BOUNDARY_WINDOW_S, pcm_b_orig)
        result.decoder_warnings_b = extract_decoder_warnings(stderr_b)
        result.segment_b_ok = ok_b
        if ok_b:
            data = read_pcm_s16le(pcm_b_orig)
            if data:
                samples = pcm_to_samples(data)
                if samples:
                    result.rms_b_original = compute_rms(samples)
                    result.peak_b_original = compute_peak(samples)
                    result.silence_b_original, _ = detect_silence(samples)
                    result.clip_b_original, _ = detect_clipping(samples)
        else:
            result.warn("INPUT_B_DECODE", f"FFmpeg decode stderr: {stderr_b[:300]}")

    # Merged: corresponding region (last 2s before boundary)
    pcm_a_merged = temp_dir / f"b{boundary.index}_a_merged.pcm"
    ok_ma, stderr_ma = decode_with_decoder_warnings(merged_path, max(0, b_time - BOUNDARY_WINDOW_S), BOUNDARY_WINDOW_S, pcm_a_merged)
    result.decoder_warnings_merged_a = extract_decoder_warnings(stderr_ma)
    result.merged_a_ok = ok_ma
    if ok_ma:
        data = read_pcm_s16le(pcm_a_merged)
        if data:
            samples = pcm_to_samples(data)
            if samples:
                result.rms_a_merged = compute_rms(samples)
                result.peak_a_merged = compute_peak(samples)
                result.silence_a_merged, _ = detect_silence(samples)
                result.clip_a_merged, _ = detect_clipping(samples)

    # Merged: corresponding region (first 2s after boundary)
    pcm_b_merged = temp_dir / f"b{boundary.index}_b_merged.pcm"
    ok_mb, stderr_mb = decode_with_decoder_warnings(merged_path, b_time, BOUNDARY_WINDOW_S, pcm_b_merged)
    result.decoder_warnings_merged_b = extract_decoder_warnings(stderr_mb)
    result.merged_b_ok = ok_mb
    if ok_mb:
        data = read_pcm_s16le(pcm_b_merged)
        if data:
            samples = pcm_to_samples(data)
            if samples:
                result.rms_b_merged = compute_rms(samples)
                result.peak_b_merged = compute_peak(samples)
                result.silence_b_merged, _ = detect_silence(samples)
                result.clip_b_merged, _ = detect_clipping(samples)

    # ── Stage 4: Waveform correlation (original vs merged) ────────────────────
    if result.segment_a_ok and result.merged_a_ok:
        data_a_orig = read_pcm_s16le(pcm_a_orig)
        data_a_merged = read_pcm_s16le(pcm_a_merged)
        if data_a_orig and data_a_merged:
            samples_a_orig = pcm_to_samples(data_a_orig)
            samples_a_merged = pcm_to_samples(data_a_merged)
            if samples_a_orig and samples_a_merged:
                corr, shift = cross_correlation(samples_a_orig, samples_a_merged)
                result.corr_a = corr
                result.shift_a = shift
                result.shift_a_s = shift / sample_rate
                if corr >= CORRELATION_MIN:
                    result.pass_check("WAVEFORM_CORR_A",
                        f"original vs merged correlation={corr:.4f}, shift={shift} samples ({shift/sample_rate:.3f}s)")
                else:
                    result.fail("WAVEFORM_CORR_A",
                        f"original vs merged correlation={corr:.4f} (threshold={CORRELATION_MIN}), "
                        f"shift={shift} samples ({shift/sample_rate:.3f}s)")

    if result.segment_b_ok and result.merged_b_ok:
        data_b_orig = read_pcm_s16le(pcm_b_orig)
        data_b_merged = read_pcm_s16le(pcm_b_merged)
        if data_b_orig and data_b_merged:
            samples_b_orig = pcm_to_samples(data_b_orig)
            samples_b_merged = pcm_to_samples(data_b_merged)
            if samples_b_orig and samples_b_merged:
                corr, shift = cross_correlation(samples_b_orig, samples_b_merged)
                result.corr_b = corr
                result.shift_b = shift
                result.shift_b_s = shift / sample_rate
                if corr >= CORRELATION_MIN:
                    result.pass_check("WAVEFORM_CORR_B",
                        f"original vs merged correlation={corr:.4f}, shift={shift} samples ({shift/sample_rate:.3f}s)")
                else:
                    result.fail("WAVEFORM_CORR_B",
                        f"original vs merged correlation={corr:.4f} (threshold={CORRELATION_MIN}), "
                        f"shift={shift} samples ({shift/sample_rate:.3f}s)")

    # ── Stage 5: Silence/corruption detection at boundary in merged output ────
    if result.merged_a_ok:
        data_a_merged = read_pcm_s16le(pcm_a_merged)
        if data_a_merged:
            samples = pcm_to_samples(data_a_merged)
            if samples:
                is_silent, rms = detect_silence(samples)
                if is_silent and rms > 0:
                    result.warn("MERGED_A_SILENCE",
                        f"Audio before boundary looks silent (RMS={rms:.6f})")
                clipped, peak = detect_clipping(samples)
                if clipped > len(samples) * 0.01:  # More than 1% clipped
                    result.warn("MERGED_A_CLIPPING",
                        f"{clipped}/{len(samples)} samples clipped (peak={peak:.4f})")

    if result.merged_b_ok:
        data_b_merged = read_pcm_s16le(pcm_b_merged)
        if data_b_merged:
            samples = pcm_to_samples(data_b_merged)
            if samples:
                is_silent, rms = detect_silence(samples)
                if is_silent and rms > 0:
                    result.warn("MERGED_B_SILENCE",
                        f"Audio after boundary looks silent (RMS={rms:.6f})")
                clipped, peak = detect_clipping(samples)
                if clipped > len(samples) * 0.01:
                    result.warn("MERGED_B_CLIPPING",
                        f"{clipped}/{len(samples)} samples clipped (peak={peak:.4f})")

    # ── Stage 6: Decoder warnings ────────────────────────────────────────────
    if result.decoder_warnings_a:
        result.warn("DECODER_A", result.decoder_warnings_a[:200])
    if result.decoder_warnings_b:
        result.warn("DECODER_B", result.decoder_warnings_b[:200])
    if result.decoder_warnings_merged_a:
        result.warn("DECODER_MERGED_A", result.decoder_warnings_merged_a[:200])
    if result.decoder_warnings_merged_b:
        result.warn("DECODER_MERGED_B", result.decoder_warnings_merged_b[:200])

    # ── Summary assessment ──────────────────────────────────────────────────
    return result


def find_input_file(name: str, input_dir: Optional[Path]) -> Optional[Path]:
    """Try to locate an input file by name.
    
    Searches: input_dir, cwd, and common test fixture paths.
    """
    if input_dir:
        candidate = input_dir / name
        if candidate.exists():
            return candidate

    # Try current working directory
    cwd = Path.cwd()
    candidate = cwd / name
    if candidate.exists():
        return candidate

    # Try test fixtures
    for test_dir in [cwd / "tests" / "fixtures", cwd / "tests" / "fixtures" / "controlled_test"]:
        candidate = test_dir / name
        if candidate.exists():
            return candidate

    return None


def check_stream_compatibility(
    audio_a: Dict, audio_b: Dict, merged: Optional[Dict]
) -> List[Tuple[str, str, bool]]:
    """Check stream-level compatibility between inputs and merged output.

    Returns list of (check_name, detail, passed).
    """
    checks = []

    if audio_a.get("sample_rate") != audio_b.get("sample_rate"):
        checks.append(("SAMPLE_RATE_MISMATCH",
            f"Input A: {audio_a.get('sample_rate')}Hz, Input B: {audio_b.get('sample_rate')}Hz",
            False))

    if audio_a.get("channels") != audio_b.get("channels"):
        checks.append(("CHANNEL_MISMATCH",
            f"Input A: {audio_a.get('channels')}ch, Input B: {audio_b.get('channels')}ch",
            False))

    if audio_a.get("channel_layout") != audio_b.get("channel_layout"):
        checks.append(("LAYOUT_MISMATCH",
            f"Input A: {audio_a.get('channel_layout')}, Input B: {audio_b.get('channel_layout')}",
            False))

    if audio_a.get("codec") != audio_b.get("codec"):
        checks.append(("CODEC_MISMATCH",
            f"Input A: {audio_a.get('codec')}, Input B: {audio_b.get('codec')}",
            False))

    if merged:
        if merged.get("sample_rate") and audio_a.get("sample_rate"):
            if merged["sample_rate"] != audio_a["sample_rate"]:
                checks.append(("MERGED_SAMPLE_RATE_CHANGED",
                    f"Input: {audio_a['sample_rate']}Hz, Merged: {merged['sample_rate']}Hz",
                    False))
        if merged.get("channels") and audio_a.get("channels"):
            if merged["channels"] != audio_a["channels"]:
                checks.append(("MERGED_CHANNELS_CHANGED",
                    f"Input: {audio_a['channels']}ch, Merged: {merged['channels']}ch",
                    False))

    return checks


def extract_decoder_warnings(stderr: str) -> str:
    """Extract relevant decoder warnings from FFmpeg stderr output."""
    warnings = []
    for line in stderr.split("\n"):
        lower = line.lower()
        if any(kw in lower for kw in ["warning", "error", "invalid", "corrupt",
                                        "skip", "discard", "failed", "unable"]):
            warnings.append(line.strip())
    return "\n".join(warnings[:5])  # Limit to 5 warnings


# ═══════════════════════════════════════════════════════════════════════════════
# REPORTING
# ═══════════════════════════════════════════════════════════════════════════════


def print_report(results: List[BoundaryResult], merged_name: str):
    """Print a formatted audio boundary certification report."""
    print()
    print("=" * 72)
    print(f"  AUDIO BOUNDARY CERTIFICATION REPORT")
    print(f"  Merged: {merged_name}")
    print("=" * 72)
    print()

    for result in results:
        status = "[PASS]" if result.passed else "[FAIL]"
        print(f"  {status} — Boundary #{result.index + 1}")
        print(f"      {result.segment_a_name}")
        print(f"      {'->'} {result.segment_b_name}")
        print(f"      Boundary at: {result.boundary_time_s:.3f}s")

        # Stream info
        if result.input_a_audio:
            aa = result.input_a_audio
            print(f"      Input A: {aa['codec']} {aa['sample_rate']}Hz "
                  f"{aa['channels']}ch ({aa['channel_layout']})")
        if result.input_b_audio:
            ab = result.input_b_audio
            print(f"      Input B: {ab['codec']} {ab['sample_rate']}Hz "
                  f"{ab['channels']}ch ({ab['channel_layout']})")
        if result.merged_audio:
            am = result.merged_audio
            print(f"      Merged:  {am['codec']} {am['sample_rate']}Hz "
                  f"{am['channels']}ch ({am['channel_layout']})")

        # PCM stats
        if result.rms_a_original is not None:
            print(f"      RMS (before boundary): original={result.rms_a_original:.6f}, "
                  f"merged={result.rms_a_merged:.6f}")
        if result.rms_b_original is not None:
            print(f"      RMS (after boundary):  original={result.rms_b_original:.6f}, "
                  f"merged={result.rms_b_merged:.6f}")

        # Waveform correlation
        if result.corr_a > 0 or result.corr_b > 0:
            corr_str = f"      Waveform correlation: before={result.corr_a:.4f}, after={result.corr_b:.4f}"
            if result.shift_a != 0 or result.shift_b != 0:
                corr_str += f" | shift: {result.shift_a_s*1000:.1f}ms / {result.shift_b_s*1000:.1f}ms"
            print(corr_str)

        # Packet details
        for label, pkt in [("Input A", result.input_a_packets),
                           ("Input B", result.input_b_packets),
                           ("Merged", result.merged_packets)]:
            if pkt and "error" not in pkt:
                n_pkt = pkt.get("total_packets", "?")
                n_gaps = len(pkt.get("pts_gaps", []))
                n_dup = pkt.get("duplicate_pts", 0)
                if n_gaps > 0 or n_dup > 0:
                    print(f"      {label} packets: {n_pkt} total, {n_gaps} gaps, {n_dup} dup PTS")

        # Check results
        for check in result.checks:
            icon = "[OK]" if check["passed"] else "[X]"
            print(f"      {icon} {check['check']}: {check['detail'][:100]}")

        for w in result.warnings:
            print(f"      [i] {w[:120]}")

        for e in result.errors:
            print(f"      [X] {e[:120]}")

        print()

    # Summary
    passed = sum(1 for r in results if r.passed)
    failed = len(results) - passed

    print("  ---")
    print(f"  Total boundaries: {len(results)}")
    print(f"  Passed:           {passed}")
    print(f"  Failed:           {failed}")
    print(f"  Total checks:     {sum(len(r.checks) for r in results)}")
    print(f"  Total warnings:   {sum(len(r.warnings) for r in results)}")
    print("  ---")

    if failed == 0:
        print()
        print("  ** ALL BOUNDARY AUDIO CERTIFICATION TESTS PASSED **")
        print("  No audio corruption detected at any concat boundary.")
    else:
        print()
        print(f"  ** {failed} boundary/boundaries FAILED certification **")

    print()


# ═══════════════════════════════════════════════════════════════════════════════
# MAIN
# ═══════════════════════════════════════════════════════════════════════════════


def main():
    parser = argparse.ArgumentParser(
        description="Audio Boundary Production Certification"
    )
    parser.add_argument("merged", help="Merged output file (MKV/MP4)")
    parser.add_argument("--report", help="Merge report file (.txt or .md)")
    parser.add_argument("--inputs", nargs="+", help="Input source files in order")
    parser.add_argument("--input-dir", help="Directory containing input source files")
    parser.add_argument("--auto-detect", action="store_true",
                        help="Auto-discover report file and inputs")

    args = parser.parse_args()

    merged_path = Path(args.merged)
    if not merged_path.exists():
        print(f"ERROR: Merged file not found: {args.merged}")
        sys.exit(1)

    # Find report file
    report_path = None
    if args.report:
        report_path = Path(args.report)
        if not report_path.exists():
            print(f"ERROR: Report file not found: {args.report}")
            sys.exit(1)
    elif args.auto_detect:
        # Look for companion report
        for ext in ["_report.txt", "_report.md", ".txt", ".md"]:
            candidate = merged_path.with_suffix("").with_name(merged_path.stem + ext)
            if candidate.exists():
                report_path = candidate
                break
        if report_path is None:
            # Try in parent directory with various names
            for pattern in [f"{merged_path.stem}_report*"]:
                import glob as glob_module
                matches = list(merged_path.parent.glob(pattern))
                if matches:
                    report_path = matches[0]
                    break

    if report_path:
        print(f"Using report: {report_path}")
        boundaries, segments = parse_merge_report(report_path)
        if not boundaries:
            print("WARNING: No boundaries parsed from report file")
    else:
        print("WARNING: No report file found. Use --report or --auto-detect.")
        boundaries = []

    # If no boundaries from report, try to create manually from input list
    if not boundaries and args.inputs:
        print(f"Using {len(args.inputs)} input files to infer boundaries")
        for i in range(len(args.inputs) - 1):
            b = BoundaryInfo(i)
            b.segment_a_name = args.inputs[i]
            b.segment_b_name = args.inputs[i + 1]
            boundaries.append(b)

    if not boundaries:
        print("ERROR: No boundaries to certify. Provide a report file or input list.")
        sys.exit(1)

    print(f"Found {len(boundaries)} concat boundaries to certify")

    # Input directory for locating source files
    input_dir = Path(args.input_dir) if args.input_dir else None

    # Create temp dir
    temp_dir = Path(tempfile.gettempdir()) / f"audio_boundary_cert_{int(time.time())}"
    temp_dir.mkdir(parents=True, exist_ok=True)

    # Certify each boundary
    results = []
    for i, boundary in enumerate(boundaries):
        print(f"\r  Certifying boundary {i + 1}/{len(boundaries)}...", end="", file=sys.stderr)
        sys.stderr.flush()
        result = certify_boundary(boundary, merged_path, input_dir=input_dir, temp_dir=temp_dir)
        results.append(result)
    print(file=sys.stderr)

    # Print report
    print_report(results, merged_path.name)

    # Cleanup temp files
    for f in temp_dir.glob("*"):
        try:
            f.unlink()
        except Exception:
            pass
    try:
        temp_dir.rmdir()
    except Exception:
        pass

    # Exit with appropriate code
    failed_count = sum(1 for r in results if not r.passed)
    sys.exit(1 if failed_count > 0 else 0)


if __name__ == "__main__":
    main()
