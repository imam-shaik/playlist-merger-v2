# Media Asset Requirements for Certification

This document lists all media assets required to run the full certification matrix (Phases A-H).

## Minimum Required Set (Must Have)

These are REQUIRED for certification to proceed:

| Asset | File | Format | Purpose |
|-------|------|--------|---------|
| h264_720p_1 | h264_720p_1.mp4 | H.264 / AAC / MP4 | Primary test video |
| h264_720p_2 | h264_720p_2.mp4 | H.264 / AAC / MP4 | Concatenation tests |
| h264_720p_3 | h264_720p_3.mp4 | H.264 / AAC / MP4 | 3-file merge tests |
| h265_1080p_1 | h265_1080p_1.mp4 | H.265 / AAC / MP4 | HEVC compatibility |
| aac_stereo_44100 | aac_stereo_44100.mp4 | AAC Stereo 44.1kHz | Audio baseline |
| aac_51_48000 | aac_51_48000.mp4 | AAC 5.1 48kHz | Multi-channel audio |
| srt_embedded | srt_embedded.mp4 | H.264 + SRT / MP4 | Text subtitle test |
| pgs_embedded | pgs_embedded.mkv | H.264 + PGS / MKV | Bitmap subtitle test |
| external_srt | external_srt.srt | SRT text file | External subtitle test |

**Minimum files to create: 9**

## Recommended Set (Should Have)

These improve coverage but aren't blocking:

| Asset | File | Format | Purpose |
|-------|------|--------|---------|
| h264_720p_4 | h264_720p_4.mp4 | H.264 / AAC / MP4 | 5-file tests |
| h264_720p_5 | h264_720p_5.mp4 | H.264 / AAC / MP4 | 5-file tests |
| h265_1080p_2 | h265_1080p_2.mp4 | H.265 / AAC / MP4 | Multi-HEVC |
| h265_1080p_3 | h265_1080p_3.mp4 | H.265 / AAC / MP4 | Multi-HEVC |
| mkv_multi_audio_1 | mkv_multi_audio_1.mkv | H.264 + 3x AAC / MKV | Multi-audio MKV |
| mkv_multi_audio_2 | mkv_multi_audio_2.mkv | H.264 + stereo + 5.1 / MKV | Channel mismatch |
| mkv_multi_audio_3 | mkv_multi_audio_3.mkv | H.264 + commentary / MKV | Commentary track |
| aac_stereo_48000 | aac_stereo_48000.mp4 | AAC Stereo 48kHz | Sample rate variant |
| aac_71_48000 | aac_71_48000.mp4 | AAC 7.1 48kHz | 7.1 channel test |
| aac_commentary | aac_commentary.mp4 | AAC Commentary | Secondary audio |
| vp9_720p_1 | vp9_720p_1.webm | VP9 / Opus / WebM | VP9 codec test |
| vp9_720p_2 | vp9_720p_2.webm | VP9 / Opus / WebM | VP9 concatenation |
| ass_embedded | ass_embedded.mp4 | H.264 + ASS / MP4 | ASS subtitle test |
| vobsub_embedded | vobsub_embedded.mkv | H.264 + VobSub / MKV | VobSub bitmap test |
| external_ass | external_ass.ass | ASS text file | External ASS test |

## Stress Test Set (For Phase H)

For large playlist stress testing (Phase H):

| Asset | File | Size | Purpose |
|-------|------|------|---------|
| large_1gb_1 | large_1gb_1.mp4 | ~1GB | 500 file test |
| large_1gb_2 | large_1gb_2.mp4 | ~1GB | 500 file test |
| ... | ... | ~1GB | ... |
| large_1gb_50 | large_1gb_50.mp4 | ~1GB | 1000 file test |

**Minimum for Phase H: 5 files (for 500 file test with duplicates)**
**Recommended for Phase H: 10 files (for 1000 file test)**
**Full Phase H: 20 files (for 2000 file test)**

## Test Media Generation Guide

### FFmpeg Commands to Generate Test Media

```bash
# H.264 720p videos (3 seconds each)
for i in 1 2 3 4 5; do
  ffmpeg -f lavfi -i testsrc=duration=3:size=1280x720:rate=30 \
         -f lavfi -i sine=frequency=440:duration=3 \
         -c:v libx264 -preset fast -crf 23 \
         -c:a aac -b:a 128k \
         h264_720p_$i.mp4 -y
done

# H.265 1080p videos
for i in 1 2 3; do
  ffmpeg -f lavfi -i testsrc=duration=3:size=1920x1080:rate=30 \
         -f lavfi -i sine=frequency=440:duration=3 \
         -c:v libx265 -preset fast -crf 28 \
         -c:a aac -b:a 128k \
         h265_1080p_$i.mp4 -y
done

# AAC Stereo 44.1kHz
ffmpeg -f lavfi -i testsrc=duration=3:size=1280x720:rate=30 \
       -f lavfi -i sine=frequency=440:duration=3 \
       -c:v libx264 -preset fast \
       -c:a aac -b:a 128k -ar 44100 \
       aac_stereo_44100.mp4 -y

# AAC 5.1 48kHz
ffmpeg -f lavfi -i testsrc=duration=3:size=1280x720:rate=30 \
       -f lavfi -i sine=frequency=440:duration=3 \
       -c:v libx264 -preset fast \
       -c:a aac -b:a 256k -ar 48000 -channel_layout 5.1 \
       aac_51_48000.mp4 -y

# Multi-audio MKV (stereo + 5.1)
ffmpeg -f lavfi -i testsrc=duration=3:size=1280x720:rate=30 \
       -f lavfi -i "sine=frequency=440:duration=3" \
       -f lavfi -i "sine=frequency=880:duration=3" \
       -map 0:v -map 1:a -map 2:a \
       -c:v libx264 -preset fast \
       -c:a aac -b:a 128k -ar 48000 \
       -c:s copy \
       mkv_multi_audio_1.mkv -y

# PGS subtitles (generate bitmap subtitle track)
# First create SRT, then convert to PGS format for MKV

# VobSub subtitles (need external .sub + .idx files)
# PGS/VobSub are bitmap formats - hard to generate, use real samples

# External SRT
echo "1
00:00:00,000 --> 00:00:02,000
Test subtitle line 1

2
00:00:02,000 --> 00:00:04,000
Test subtitle line 2
" > external_srt.srt

# Large 1GB files (for stress testing)
# Use longer duration or higher bitrate
ffmpeg -f lavfi -i testsrc=duration=300:size=1280x720:rate=30 \
       -f lavfi -i sine=frequency=440:duration=300 \
       -c:v libx264 -preset fast -crf 18 \
       -c:a aac -b:a 192k \
       large_1gb_1.mp4 -y
```

## Quick Start: Minimal Media Set (9 files)

```bash
# Create minimal test media
mkdir -p media_assets
cd media_assets

# 5x H.264 MP4
for i in 1 2 3 4 5; do
  ffmpeg -f lavfi -i testsrc=duration=3:size=1280x720:rate=30 \
         -f lavfi -i sine=frequency=$((440+i*100)):duration=3 \
         -c:v libx264 -preset fast -crf 23 \
         -c:a aac -b:a 128k \
         h264_720p_$i.mp4 -y 2>/dev/null
done

# 3x H.265 MP4
for i in 1 2 3; do
  ffmpeg -f lavfi -i testsrc=duration=3:size=1920x1080:rate=30 \
         -f lavfi -i sine=frequency=440:duration=3 \
         -c:v libx265 -preset fast -crf 28 \
         -c:a aac -b:a 128k \
         h265_1080p_$i.mp4 -y 2>/dev/null
done

# 2x AAC audio files
ffmpeg -f lavfi -i testsrc=duration=3:size=1280x720:rate=30 \
       -f lavfi -i sine=frequency=440:duration=3 \
       -c:v libx264 -preset fast \
       -c:a aac -b:a 128k -ar 44100 \
       aac_stereo_44100.mp4 -y 2>/dev/null

ffmpeg -f lavfi -i testsrc=duration=3:size=1280x720:rate=30 \
       -f lavfi -i sine=frequency=440:duration=3 \
       -c:v libx264 -preset fast \
       -c:a aac -b:a 256k -ar 48000 -channel_layout 5.1 \
       aac_51_48000.mp4 -y 2>/dev/null

# SRT subtitle file
cat > external_srt.srt << 'EOF'
1
00:00:00,000 --> 00:00:02,000
Test subtitle

2
00:00:02,000 --> 00:00:04,000
Second line
EOF

# 1x MKV with subtitle
ffmpeg -f lavfi -i testsrc=duration=3:size=1280x720:rate=30 \
       -f lavfi -i sine=frequency=440:duration=3 \
       -c:v libx264 -preset fast \
       -c:a aac -b:a 128k \
       srt_embedded.mp4 -y 2>/dev/null

echo "Created $(ls *.mp4 *.mkv *.srt 2>/dev/null | wc -l) files"
```

## Media Acquisition Priority

1. **Week 1 (Must Have):** Generate minimal 9 files above
2. **Week 2 (Should Have):** Add VP9, multi-audio MKVs, ASS subtitles
3. **Week 3 (Stress Test):** Generate large 1GB files for Phase H
4. **Week 4 (Real Media):** Collect actual anime/udemy files with PGS/VobSub

## Verification

After creating media, verify with:

```bash
# List all media assets
certification_runner --media-path ./media_assets --verbose

# Check specific files
ffprobe -v error -show_entries format=duration,size -show_entries stream=codec_name,codec_type aac_51_48000.mp4
```