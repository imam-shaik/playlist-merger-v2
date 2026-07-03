# SmartMKV Regression Corpus

This directory contains a comprehensive test media corpus for regression testing.

## Directory Structure

| Directory | Purpose | Test Cases |
|-----------|---------|-----------|
| `mp4/` | Standard MP4 files | Container compatibility, stream copy |
| `mkv/` | Matroska/MKV files | mkvmerge-specific features, attachments |
| `avi/` | AVI containers | Legacy format support |
| `mov/` | QuickTime MOV | Apple codec support |
| `webm/` | WebM containers | VP8/VP9 video, WebM strict mode |
| `vfr/` | Variable frame rate | Timestamp handling, PTS/DTS verification |
| `hdr/` | HDR content | Color space, bit depth, HDR metadata |
| `hevc/` | HEVC/H.265 content | Modern codec support |
| `h264/` | H.264/AVC content | Legacy codec support |
| `multiple_audio/` | Multiple audio tracks | Audio stream selection, language handling |
| `multiple_subtitles/` | Multiple subtitle tracks | Subtitle embedding, SRT merging |
| `attachments/` | Files with attachments | Font embedding, mkvmerge attachment handling |
| `chapters/` | Files with chapters | Chapter preservation, mkvmerge vs FFmpeg |
| `unicode/` | Unicode filenames/paths | International character support |
| `corrupted/` | Partially corrupted files | Error handling, graceful degradation |
| `damaged/` | Intentionally damaged files | Recovery behavior, crash prevention |
| `long_paths/` | Very long paths (>260 chars) | Windows long path support |
| `network_paths/` | UNC paths, mapped drives | Network file handling |

## Using This Corpus

### Regression Testing
```bash
# Run full regression against corpus
cargo test --test regression

# Test specific format
cargo test --test regression -- mkv

# Test stress scenarios
cargo test --test regression -- long_playlist
```

### CI/CD Integration
```bash
# Add to CI pipeline
./scripts/run_regression_corpus.sh --format all

# Quick smoke test
./scripts/run_regression_corpus.sh --format mp4,mkv,h264
```

### Manual Testing
1. Copy media files to the appropriate directories
2. Run the stability certification suite
3. Review the `stability_report.json` output

## File Requirements

Each directory should contain representative samples:

- **Minimum 1 file** per directory for basic coverage
- **Recommended 3-5 files** per directory for thorough testing
- **Format-specific requirements** noted below

### mp4/
- H.264/AAC baseline
- H.264/AAC with B-frames
- HEVC/AAC
- Various resolution (720p, 1080p, 4K)

### mkv/
- H.264 with attachments (fonts)
- HEVC with chapters
- Multiple subtitle tracks

### hdr/
- HDR10 content
- Dolby Vision content
- HDR10+ content

### corrupted/
- Truncated headers
- Invalid timestamps
- Missing stream data

## Maintenance

- **Add new samples** when bugs are found in specific formats
- **Remove stale samples** when formats become obsolete
- **Update metadata** when tests reveal edge cases
- **Document failures** in `KNOWN_ISSUES.md`

## Known Limitations

- Large file tests (>4GB) require additional disk space
- Network path tests require network share configuration
- HDR tests require display capable of HDR presentation