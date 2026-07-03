# Manual Test Runner
# Executes manual tests and records results

param(
    [string]$TestRoot = "C:\test\merge"
)

$results = @()

function Write-TestResult {
    param(
        [string]$TestName,
        [string]$Status,
        [string]$Notes = ""
    )
    $script:results += [PSCustomObject]@{
        Test = $TestName
        Status = $Status
        Notes = $Notes
    }
    $color = if ($Status -eq "PASS") { "Green" } elseif ($Status -eq "FAIL") { "Red" } else { "Yellow" }
    Write-Host "[$Status] $TestName" -ForegroundColor $color
    if ($Notes) { Write-Host "       $Notes" -ForegroundColor Gray }
}

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "   MANUAL TEST RUNNER" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

# Pre-flight checks
Write-Host "PRE-FLIGHT CHECKS" -ForegroundColor Yellow
Write-Host "----------------------------------------" -ForegroundColor Yellow

# Check if app is running
$tauriProcess = Get-Process -Name "playlist-merger*" -ErrorAction SilentlyContinue
if ($tauriProcess) {
    Write-TestResult "App Running" "PASS" "Found PID: $($tauriProcess.Id)"
} else {
    Write-TestResult "App Running" "WARN" "App not running. Start with: cargo tauri dev"
}

# Check test directory
if (Test-Path $TestRoot) {
    $files = Get-ChildItem -Path $TestRoot -Recurse -File -Filter "*.mp4" -ErrorAction SilentlyContinue
    Write-TestResult "Test Files" $(if ($files.Count -ge 6) { "PASS" } else { "WARN" }) "Found $($files.Count) .mp4 files"
} else {
    Write-TestResult "Test Directory" "WARN" "Directory not found: $TestRoot"
}

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "   TEST INSTRUCTIONS" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

Write-Host "M1: IMPORT TEST FILES" -ForegroundColor Yellow
Write-Host "----------------------------------------"
Write-Host "1. Open the Playlist Merger app"
Write-Host "2. Click 'Import Folder'"
Write-Host "3. Select: $TestRoot\folder_a"
Write-Host "4. Wait for files to load"
Write-Host "5. Click 'Import Folder' again"
Write-Host "6. Select: $TestRoot\folder_b"
Write-Host "7. Verify: 6-8 files shown in playlist"
Write-Host ""

Write-Host "M2: SMARTMKV + CARDS TEST" -ForegroundColor Yellow
Write-Host "----------------------------------------"
Write-Host "1. In the merge panel:"
Write-Host "   - Enable Cards: ON"
Write-Host "   - Card Color: #00CCCC"
Write-Host "   - Frequency: PerVideo"
Write-Host "   - Duration: 2"
Write-Host "2. Select Mode: SmartMKV"
Write-Host "3. Click 'Merge'"
Write-Host "4. Wait for completion"
Write-Host "5. Open merge report"
Write-Host "6. Verify:"
Write-Host "   - Segment count = 13 (7 videos + 6 cards)"
Write-Host "   - Cards show 'Canvas' badge"
Write-Host "   - No 404 errors in console"
Write-Host ""

Write-Host "M3: FASTMKV + CARDS TEST" -ForegroundColor Yellow
Write-Host "----------------------------------------"
Write-Host "1. Import same files again"
Write-Host "2. Enable Cards: ON (same settings)"
Write-Host "3. Select Mode: FastMKV"
Write-Host "4. Run merge"
Write-Host "5. Open report"
Write-Host "6. Verify:"
Write-Host "   - Same as M2"
Write-Host "   - Cards correctly identified"
Write-Host ""

Write-Host "M4: FOLDER SPLIT + CARDS TEST" -ForegroundColor Yellow
Write-Host "----------------------------------------"
Write-Host "1. Import files from both folders"
Write-Host "2. Enable Cards: ON"
Write-Host "3. Enable Split: Folder Split"
Write-Host "4. Run merge"
Write-Host "5. Verify:"
Write-Host "   - Multiple output files"
Write-Host "   - Cards within each part"
Write-Host "   - No orphan cards"
Write-Host ""

Write-Host "M5: PROGRESS EVENTS TEST" -ForegroundColor Yellow
Write-Host "----------------------------------------"
Write-Host "1. Run SmartMKV with 7 videos"
Write-Host "2. Watch progress panel"
Write-Host "3. Verify:"
Write-Host "   - Phase labels correct"
Write-Host "   - File names shown"
Write-Host "   - Progress % updates"
Write-Host "   - Dashboard shows correct counts"
Write-Host ""

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "   RECORD RESULTS" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

Write-Host "After completing each test, run:"
Write-Host "  .\test-record.ps1 -TestName 'M2' -Status 'PASS'"
Write-Host "  .\test-record.ps1 -TestName 'M2' -Status 'FAIL' -Notes 'Card count wrong'"
Write-Host ""

Write-Host "To view results:"
Write-Host "  .\test-results.ps1"
