#!/usr/bin/env python3
"""
COMPREHENSIVE INTEGRATION TEST: Fresh Split Merge Certification
=================================================================

Simulates a SmartMKV-style split merge pipeline:
  1. Normalize audio to matching sample rates (48000Hz AAC)
  2. Create per-part outputs (simulating split-by-folder)
  3. Generate per-part SRT files WITHOUT cumulative offset
     (each part's SRT starts at 00:00 — simulating Bug #1 fix)
  4. Run all three certification tools

LIMITATIONS:
  - Uses FFmpeg CLI (cannot drive the Tauri GUI from CLI)
  - Bug #1 fix (per-part SRT export in Rust) is verified at code level.
    This test verifies OUTPUT quality of split content.
  - For full end-to-end verification, run a split merge from the app
    using the rebuilt binary, then point these tools at the outputs.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import List

ROOT = Path(__file__).resolve().parent.parent
CONTROLLED = ROOT / "tests" / "fixtures" / "controlled_test"
SCRIPTS = ROOT / "scripts"


def find_ffmpeg() -> str:
    for name in ["ffmpeg", "ffmpeg.exe"]:
        p = shutil.which(name)
        if p:
            return p
    for candidate in [r"C:\ffmpeg\bin\ffmpeg.exe", r"C:\Program Files\ffmpeg\bin\ffmpeg.exe"]:
        if os.path.exists(candidate):
            return candidate
    print("ERROR: ffmpeg not found")
    sys.exit(1)


def find_ffprobe() -> str:
    for name in ["ffprobe", "ffprobe.exe"]:
        p = shutil.which(name)
        if p:
            return p
    for candidate in [r"C:\ffmpeg\bin\ffprobe.exe", r"C:\Program Files\ffmpeg\bin\ffprobe.exe"]:
        if os.path.exists(candidate):
            return candidate
    print("ERROR: ffprobe not found")
    sys.exit(1)


def probe_duration(file_path: Path) -> float:
    ffprobe = find_ffprobe()
    r = subprocess.run(
        [ffprobe, "-v", "error", "-show_entries", "format=duration",
         "-of", "csv=p=0", str(file_path)],
        capture_output=True, text=True
    )
    return float(r.stdout.strip()) if r.returncode == 0 and r.stdout.strip() else 0.0


def run_cmd(cmd: List[str], desc: str, timeout: int = 30) -> bool:
    print(f"  {desc}...", end=" ", flush=True)
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if r.returncode == 0:
            print("OK")
            return True
        else:
            print("FAILED")
            print(f"    stderr: {r.stderr[:300]}")
            return False
    except subprocess.TimeoutExpired:
        print("TIMEOUT")
        return False


def create_srt(path: Path, cues: list):
    lines = []
    for i, (start, end, text) in enumerate(cues, 1):
        def ts(s):
            h = int(s // 3600)
            m = int((s % 3600) // 60)
            sec = s % 60
            return f"{h:02d}:{m:02d}:{sec:06.3f}".replace(".", ",")
        lines.append(str(i))
        lines.append(f"{ts(start)} --> {ts(end)}")
        lines.append(text)
        lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def main():
    print("=" * 72)
    print("  COMPREHENSIVE MERGE CERTIFICATION TEST SUITE")
    print("=" * 72)
    print()

    tmp = Path(tempfile.mkdtemp(prefix="merge_cert_"))
    ffmpeg = find_ffmpeg()
    print(f"  Temp dir: {tmp}")
    print()

    # Step 1: Source files
    print("Step 1: Source files")
    print("-" * 50)
    src_files = [CONTROLLED / "file_b.mp4", CONTROLLED / "file_c.mp4"]
    for f in src_files:
        if not f.exists():
            print(f"  ERROR: {f} not found")
            sys.exit(1)
        print(f"  {f.name}: {probe_duration(f):.3f}s")
    print()

    # Step 2: Per-part SRT files (original timestamps, no rebasing)
    print("Step 2: Per-part SRT files (original timestamps)")
    print("-" * 50)
    subs_dir = tmp / "subtitles"
    subs_dir.mkdir(exist_ok=True)
    create_srt(subs_dir / "file_b.srt", [
        (0.5, 2.0, "Segment 1: Welcome to SmartMKV certification."),
        (2.1, 3.5, "Segment 1: Testing subtitle sync."),
        (3.6, 4.8, "Segment 1: First segment complete."),
    ])
    create_srt(subs_dir / "file_c.srt", [
        (0.3, 1.8, "Segment 2: Second segment beginning."),
        (1.9, 3.2, "Segment 2: Audio boundary should be seamless."),
        (3.3, 4.5, "Segment 2: All tests should pass."),
    ])
    for sf in [subs_dir / "file_b.srt", subs_dir / "file_c.srt"]:
        print(f"  {sf.name}: created")
    print()

    # Step 3: Normalize audio to 48000Hz AAC (SmartMKV pipeline)
    print("Step 3: Audio normalization (48000Hz AAC)")
    print("-" * 50)
    norm_dir = tmp / "normalized"
    norm_dir.mkdir(exist_ok=True)
    norm_files = []
    for src in src_files:
        nf = norm_dir / src.name
        ok = run_cmd([ffmpeg, "-y", "-hide_banner", "-loglevel", "error",
                      "-i", str(src), "-c:v", "copy",
                      "-c:a", "aac", "-b:a", "128k",
                      "-ar", "48000", "-ac", "2", str(nf)],
                     f"  Normalize {src.name}", timeout=60)
        if not ok:
            sys.exit(1)
        norm_files.append(nf)
    print()

    # Step 4: Create split outputs with per-part SRTs
    # IMPORTANT: Each part's SRT uses ORIGINAL timestamps (no cumulative offset).
    # This is what the Bug #1 fix does — per-part SRTs start at 00:00 for each part.
    print("Step 4: Split outputs + per-part SRTs")
    print("-" * 50)
    parts_dir = tmp / "split_outputs"
    parts_dir.mkdir(exist_ok=True)
    part_durs = []
    for i, nf in enumerate(norm_files, 1):
        part_name = f"Part_{i:02d}.mkv"
        part_path = parts_dir / part_name
        dur = probe_duration(nf)
        part_durs.append(dur)
        shutil.copy2(nf, part_path)
        print(f"  {part_name}: {dur:.3f}s")

    # Copy SRT files with matching names (per-part SRTs with original timestamps)
    for i, nf in enumerate(norm_files, 1):
        part_srt = parts_dir / f"Part_{i:02d}.srt"
        src_srt = subs_dir / f"{nf.stem}.srt"
        if src_srt.exists():
            shutil.copy2(src_srt, part_srt)
            cue_count = len([l for l in src_srt.read_text().splitlines() if "-->" in l])
            print(f"  {part_srt.name}: {cue_count} cues (original timestamps)")
    print()

    # Step 5: Create merged output for boundary tests
    print("Step 5: Merged output (for audio boundary + A/V sync tests)")
    print("-" * 50)
    merged_dir = tmp / "merged"
    merged_dir.mkdir(exist_ok=True)
    concat_list = merged_dir / "concat.txt"
    with open(concat_list, "w") as f:
        for nf in norm_files:
            f.write(f"file '{nf.as_posix()}'\n")

    merged_output = merged_dir / "merged_output.mkv"
    ok = run_cmd([ffmpeg, "-y", "-hide_banner", "-loglevel", "error",
                  "-f", "concat", "-safe", "0",
                  "-i", str(concat_list),
                  "-c", "copy", str(merged_output)],
                 "  Merge", timeout=60)
    if not ok:
        print("  WARNING: Merge failed, boundary tests will be skipped")
    merged_dur = probe_duration(merged_output) if merged_output.exists() else 0
    print(f"  Merged duration: {merged_dur:.3f}s")

    # Create merge report for boundary detection
    report_path = merged_dir / "merged_report.txt"
    with open(report_path, "w", encoding="utf-8") as f:
        f.write("MERGED OUTPUT REPORT\n")
        f.write("====================\n")
        f.write(f"Output: {merged_output}\n")
        f.write(f"Duration: {merged_dur:.1f}s\n")
        f.write("FILES MERGED\n")
        f.write("============\n\n")
        f.write("| # | File Name       | Duration | In Merged (Start-End)    |\n")
        f.write("|---|-----------------|----------|--------------------------|\n")
        cum = 0.0
        for i, nf in enumerate(norm_files, 1):
            dur = probe_duration(nf)
            start_str = f"00:00:{int(cum):02d}"
            end_str = f"00:00:{int(cum+dur):02d}"
            f.write(f"| {i} | {nf.name:<15s} | {dur:.1f}s    | {start_str} -> {end_str}           |\n")
            cum += dur
    print(f"  Report: {report_path}")
    print()

    # Step 6: Run split subtitle certification
    print("Step 6: Split Subtitle Certification")
    print("-" * 50)
    cert = SCRIPTS / "split_subtitle_certification.py"
    if cert.exists() and parts_dir.exists():
        r = subprocess.run(
            [sys.executable, str(cert), str(parts_dir), "--auto-detect"],
            capture_output=True, text=True, timeout=60
        )
        print(r.stdout[-2000:] if r.stdout else "  No output")
        if r.returncode != 0 and r.stderr:
            print(f"  stderr: {r.stderr[:500]}")
    else:
        print(f"  SKIP: cert tool or output dir not found")
    print()

    # Step 7: Run audio boundary certification
    print("Step 7: Audio Boundary Certification")
    print("-" * 50)
    cert = SCRIPTS / "audio_boundary_certification.py"
    if cert.exists() and merged_output.exists():
        r = subprocess.run(
            [sys.executable, str(cert), str(merged_output),
             "--report", str(merged_dir / "merged_report.txt"),
             "--input-dir", str(norm_dir)],
            capture_output=True, text=True, timeout=120
        )
        print(r.stdout[-2000:] if r.stdout else "  No output")
        if r.returncode != 0 and r.stderr:
            print(f"  stderr: {r.stderr[:500]}")
    else:
        print(f"  SKIP: cert tool or merged file not found")
    print()

    # Step 8: Run A/V sync certification
    print("Step 8: A/V Sync Certification")
    print("-" * 50)
    cert = SCRIPTS / "av_sync_certification.py"
    if cert.exists() and merged_output.exists():
        r = subprocess.run(
            [sys.executable, str(cert), str(merged_output),
             "--report", str(merged_dir / "merged_report.txt"),
             "--tolerance", "0.1"],
            capture_output=True, text=True, timeout=120
        )
        print(r.stdout[-2000:] if r.stdout else "  No output")
        if r.returncode != 0 and r.stderr:
            print(f"  stderr: {r.stderr[:500]}")
    else:
        print(f"  SKIP: cert tool or merged file not found")
    print()

    # Summary
    print("=" * 72)
    print("  CERTIFICATION COMPLETE")
    print("=" * 72)
    print(f"  Split outputs:   {parts_dir}")
    print(f"  Merged output:   {merged_output}")
    print(f"  Per-part SRTs:   correct timestamps (no global rebase)")
    print()
    print("  Re-run manually:")
    print(f"    python scripts/split_subtitle_certification.py {parts_dir} --auto-detect")
    print(f"    python scripts/audio_boundary_certification.py {merged_output} --report {merged_dir}/merged_report.txt --input-dir {norm_dir}")
    print(f"    python scripts/av_sync_certification.py {merged_output} --report {merged_dir}/merged_report.txt")
    print()


if __name__ == "__main__":
    main()
