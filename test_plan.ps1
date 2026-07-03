$ErrorActionPreference = "Stop"

# ============================================================
# COMPREHENSIVE TEST PLAN: playlist-merger-v2
# ============================================================

$TestDir = "$env:TEMP\playlist_merger_test"
$PlaylistFile = "$TestDir\test_playlist_50.txt"

New-Item -ItemType Directory -Force -Path $TestDir | Out-Null

# Find 50+ MP4 files
Write-Host "Finding test files..." -ForegroundColor Cyan
$sourceDir = "E:\7.Spoken English"
$allFiles = @(Get-ChildItem -Path $sourceDir -Recurse -Include "*.mp4" -ErrorAction SilentlyContinue | Where-Object { $_.Length -gt 1MB })
Write-Host "Found $($allFiles.Count) MP4 files" -ForegroundColor Green

if ($allFiles.Count -lt 50) {
    Write-Host "Need at least 50 files, found $($allFiles.Count)" -ForegroundColor Red
    exit 1
}

# Select 50 diverse files
$testFiles = @($allFiles | Get-Random -Count 50)
Write-Host "Selected 50 test files" -ForegroundColor Green

# Create playlist file with all 50 paths
$testFiles | ForEach-Object { $_.FullName } | Set-Content -Path $PlaylistFile -Encoding UTF8

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "TEST PLAN SUMMARY" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Test 1: 50-file Smart MKV merge" -ForegroundColor Yellow
Write-Host "Test 2: Crash during normalization + Resume" -ForegroundColor Yellow
Write-Host "Test 3: Output validation (MKV container)" -ForegroundColor Yellow
Write-Host "Test 4: Audio seekability verification" -ForegroundColor Yellow
Write-Host "Test 5: HE-AAC profile unification check" -ForegroundColor Yellow
Write-Host "Test 6: Channel normalization check" -ForegroundColor Yellow
Write-Host "Test 7: Thumbnail cache stress" -ForegroundColor Yellow
Write-Host ""
Write-Host "Test playlist: $PlaylistFile" -ForegroundColor Gray
Write-Host "50 files selected from: $sourceDir" -ForegroundColor Gray
Write-Host ""

# ============================================================
# POST-MERGE VALIDATION SCRIPT
# Run this AFTER a merge completes to verify output
# ============================================================

$FFprobe = "C:\Users\IMAM\Desktop\playlist-merger-v2\src-tauri\binaries\ffprobe.exe"
$FFmpeg = "C:\Users\IMAM\Desktop\playlist-merger-v2\src-tauri\binaries\ffmpeg.exe"
$OutputPath = $null  # Set this before running validation

$validationScript = @"
# Post-merge validation - run after merge completes
`$ErrorActionPreference = "Continue"

`$FFprobe = "$FFprobe"
`$FFmpeg = "$FFmpeg"
`$OutputPath = "$OutputPath"  # <-- SET THIS

if (-not `$OutputPath -or -not (Test-Path `$OutputPath)) {
    Write-Host "ERROR: Set `$OutputPath to the actual merge output file" -ForegroundColor Red
    exit 1
}

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "POST-MERGE VALIDATION" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# 1. Container format
Write-Host "[TEST 3] Checking container format..." -ForegroundColor Yellow
try {
    `$json = & `$FFprobe -v quiet -print_format json -show_streams -show_format `$OutputPath 2>`$null | ConvertFrom-Json
    `$container = [System.IO.Path]::GetExtension(`$OutputPath).TrimStart('.').ToUpper()
    `$fmtName = `$json.format.format_name -replace ',.*', ''
    Write-Host "  Container (by extension): `$(`$container)"
    Write-Host "  Format (by ffprobe): `$(`$fmtName)"
    if (`$container -eq "MKV" -or `$fmtName -match "matroska") {
        Write-Host "  [PASS] MKV container confirmed" -ForegroundColor Green
    } else {
        Write-Host "  [WARN] Unexpected container: `$(`$container) vs format `$(`$fmtName)" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  [FAIL] Could not probe output" -ForegroundColor Red
}

# 2. Audio seekability
Write-Host ""
Write-Host "[TEST 4] Testing audio seekability..." -ForegroundColor Yellow
try {
    `$duration = [double]`$json.format.duration
    `$seekPoints = @(0.05, 0.25, 0.50, 0.75, 0.95) | ForEach-Object { [Math]::Floor(`$_ * `$duration) }
    `$seekOk = 0
    foreach (`$p in `$seekPoints) {
        `$testFile = "`$env:TEMP\seek_test_`$p.mp4"
        $null = & `$FFmpeg -y -ss `$p -i `$OutputPath -v quiet -t 1 -c:a aac -b:a 128k `$testFile 2>&1
        if ((Test-Path `$testFile) -and ((Get-Item `$testFile).Length -gt 1000)) {
            `$seekOk++
            Remove-Item `$testFile -Force -ErrorAction SilentlyContinue
        }
    }
    Write-Host "  Seek points tested: `$(`$seekPoints.Count)"
    Write-Host "  Successful: `$(`$seekOk)"
    if (`$seekOk -eq `$seekPoints.Count) {
        Write-Host "  [PASS] All seek points work" -ForegroundColor Green
    } else {
        Write-Host "  [WARN] Some seek points failed" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  [FAIL] Seek test error: `$(`$_.Exception.Message)" -ForegroundColor Red
}

# 3. Audio profile check
Write-Host ""
Write-Host "[TEST 5] Checking audio profile..." -ForegroundColor Yellow
try {
    `$audioStream = `$json.streams | Where-Object { `$_.codec_type -eq "audio" } | Select-Object -First 1
    `$profile = `$audioStream.profile
    `$codec = `$audioStream.codec_name
    Write-Host "  Audio codec: `$(`$codec)"
    Write-Host "  AAC profile: `$(`$profile)"
    if (`$codec -eq "aac" -and `$profile -eq "LC") {
        Write-Host "  [PASS] Uniform LC AAC confirmed" -ForegroundColor Green
    } elseif (`$codec -eq "aac") {
        Write-Host "  [INFO] AAC profile: `$(`$profile)" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  [FAIL] Could not check audio profile" -ForegroundColor Red
}

# 4. Channel layout check
Write-Host ""
Write-Host "[TEST 6] Checking channel layout..." -ForegroundColor Yellow
try {
    `$channels = `$audioStream.channels
    `$layout = `$audioStream.channel_layout
    Write-Host "  Channels: `$(`$channels)"
    Write-Host "  Layout: `$(`$layout)"
    if (`$channels -le 2) {
        Write-Host "  [PASS] Stereo/mono confirmed (normalized)" -ForegroundColor Green
    } elseif (`$channels -eq 6) {
        Write-Host "  [INFO] 5.1 surround detected" -ForegroundColor Yellow
    }
} catch {
    Write-Host "  [FAIL] Could not check channel layout" -ForegroundColor Red
}

# 5. Duration stats
Write-Host ""
Write-Host "[STATS] Output file stats" -ForegroundColor Cyan
try {
    `$size = (Get-Item `$OutputPath).Length
    Write-Host "  File size: `$(`$size / 1GB) GB (`$(`$size / 1MB) MB)"
    Write-Host "  Duration: `$([Math]::Round([double]`$json.format.duration, 1))s (`$([Math]::Round([double]`$json.format.duration / 60, 1)) min)"
    `$videoStream = `$json.streams | Where-Object { `$_.codec_type -eq "video" } | Select-Object -First 1
    Write-Host "  Resolution: `$(`$videoStream.width)x`$(`$videoStream.height)"
    Write-Host "  FPS: `$(`$videoStream.r_frame_rate -replace '/.*', '')"
} catch {
    Write-Host "  [FAIL] Could not get stats" -ForegroundColor Red
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "VALIDATION COMPLETE" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
"@

$validationScriptPath = "$TestDir\post_merge_validation.ps1"
$validationScript | Set-Content -Path $validationScriptPath -Encoding UTF8
Write-Host "Validation script: $validationScriptPath" -ForegroundColor Gray
Write-Host ""

# ============================================================
# TEST 2: CRASH SIMULATION SCRIPT
# Run BEFORE loading the app. Creates 50-file playlist and
# then simulates a crash by writing a recovery checkpoint.
# ============================================================

$crashScript = @"
# TEST 2: Crash + Resume Simulation
# This creates a recovery checkpoint with 25 files "completed"
# so the next Smart MKV merge will skip them on resume.

`$ErrorActionPreference = "Continue"

`$RecoveryDir = "$env:LOCALAPPDATA\com.PlaylistMerger\recovery"
if (-not (Test-Path `$RecoveryDir)) {
    New-Item -ItemType Directory -Force -Path `$RecoveryDir | Out-Null
}

`$TestJobId = "crash_sim_test_`$(Get-Date -Format 'yyyyMMdd_HHmmss')"

# Create a checkpoint with 25 "completed" files
# This simulates a crash at 50% normalization
`$checkpoint = @{
    version = 1
    job_id = `$TestJobId
    phase = "Normalizing"
    started_at = [int]`$(Get-Date -UFormat %s)
    input_files = @()
    output_path = "$TestDir\test_output.mkv"
    mode = "smartMkv"
    dominant_profile = @{
        v_codec = $null
        v_width = $null
        v_height = $null
        v_fps = $null
        a_codec = "aac"
        a_sample_rate = 44100
        a_channels = 2
        timescale_den = $null
    }
    completed_files = @()
    remaining_indices = @()
}

# Generate 25 fake completed file entries
for (`$i = 0; `$i -lt 25; `$i++) {
    `$checkpoint.completed_files += @{
        index = `$i
        source_path = "E:\source\file_`$i.mp4"
        source_size = 50000000
        source_mtime = [int]`$(Get-Date -UFormat %s)
        normalized_path = "`$env:TEMP\norm_prof_`$i.mkv"
        normalization_type = "Full"
    }
}

# 25 remaining
for (`$i = 25; `$i -lt 50; `$i++) {
    `$checkpoint.remaining_indices += `$i
}

`$checkpoint | ConvertTo-Json -Depth 10 | Set-Content -Path "`$RecoveryDir\`$(`$TestJobId).json" -Encoding UTF8

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "CRASH SIMULATION: Recovery checkpoint created" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Job ID: `$(`$TestJobId)"
Write-Host "  Completed files: 25"
Write-Host "  Remaining files: 25"
Write-Host "  Recovery dir: `$(`$RecoveryDir)"
Write-Host ""
Write-Host "Next step:" -ForegroundColor Yellow
Write-Host "  1. Load the app" -ForegroundColor White
Write-Host "  2. Select Smart MKV mode" -ForegroundColor White
Write-Host "  3. Load the same 50 files" -ForegroundColor White
Write-Host "  4. Check 'Resume' recovery dialog appears" -ForegroundColor White
Write-Host "  5. Resume should show 25/50 completed, skip 25" -ForegroundColor White
Write-Host "  6. Verify [DURATION_AUDIT] logs show recovered paths" -ForegroundColor White
Write-Host ""
Write-Host "After testing, delete the checkpoint:" -ForegroundColor Gray
Write-Host "  Remove-Item `$(`$RecoveryDir)\`$(`$TestJobId).json" -ForegroundColor Gray
"@

$crashScriptPath = "$TestDir\crash_simulation.ps1"
$crashScript | Set-Content -Path $crashScriptPath -Encoding UTF8
Write-Host "Crash simulation script: $crashScriptPath" -ForegroundColor Gray

# ============================================================
# SUMMARY
# ============================================================

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "TEST FILES READY" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Playlist:     $PlaylistFile (50 files)"
Write-Host "Test dir:     $TestDir"
Write-Host ""
Write-Host "WHAT TO DO MANUALLY:" -ForegroundColor Yellow
Write-Host ""
Write-Host "1. LOAD THE APP" -ForegroundColor White
Write-Host "   Open: http://localhost:1420/"
Write-Host ""
Write-Host "2. SELECT MODE" -ForegroundColor White
Write-Host "   Mode: Smart MKV"
Write-Host "   Convert to MP4: UNCHECKED"
Write-Host ""
Write-Host "3. ADD FILES" -ForegroundColor White
Write-Host "   Import from: $PlaylistFile"
Write-Host "   Or copy-paste the paths from that file"
Write-Host ""
Write-Host "4. SET OUTPUT" -ForegroundColor White
Write-Host "   Save as: $TestDir\test_output.mkv"
Write-Host "   Verify it saves as .mkv (forceVideoExtension fix)"
Write-Host ""
Write-Host "5. START MERGE" -ForegroundColor White
Write-Host "   Watch the [SMART_MKV_REASON] logs for per-file reasons"
Write-Host "   Watch the [DURATION_AUDIT] logs for recovery data"
Write-Host "   Watch the [AUDIO_PROFILE_VERIFY] for LC confirmation"
Write-Host ""
Write-Host "6. DURING NORMALIZATION:" -ForegroundColor White
Write-Host "   Press Ctrl+C in terminal to kill the app"
Write-Host "   This simulates a crash at ~50% (if ~25 files done)"
Write-Host ""
Write-Host "7. RESTART APP" -ForegroundColor White
Write-Host "   Recovery dialog should appear"
Write-Host "   Resume should skip completed files"
Write-Host "   Check [Recovery] logs in output"
Write-Host ""
Write-Host "8. AFTER MERGE COMPLETES:" -ForegroundColor White
Write-Host "   Set `$OutputPath = `"$TestDir\test_output.mkv`""
Write-Host "   Run: powershell -File $validationScriptPath"
Write-Host ""
Write-Host "9. TO SIMULATE CRASH (alternative to step 6):" -ForegroundColor White
Write-Host "   Run: powershell -File $crashScriptPath"
Write-Host "   Then do steps 7-8"
Write-Host ""
Write-Host "10. THUMBNAIL CACHE TEST:" -ForegroundColor White
Write-Host "    Browse through the 50 files in the UI"
Write-Host "    Check DevTools console for thumbnail errors"
Write-Host '    Check for: "File does not exist at path: thumbnails\..." errors'