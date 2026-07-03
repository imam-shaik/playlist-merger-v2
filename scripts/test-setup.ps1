# Manual Test Setup Script
# Run this to prepare test environment

$testRoot = "C:\test\merge"
$folderA = "$testRoot\folder_a"
$folderB = "$testRoot\folder_b"

# Create directories
Write-Host "Creating test directories..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $folderA | Out-Null
New-Item -ItemType Directory -Force -Path $folderB | Out-Null

Write-Host "Test directories created:" -ForegroundColor Green
Write-Host "  $folderA"
Write-Host "  $folderB"

# Create placeholder files for testing
# Note: These are NOT real video files - they're placeholders
# You need to add real video files manually

Write-Host "`nPlace test files in these folders:" -ForegroundColor Yellow
Write-Host "  $folderA\  (3-4 .mp4 files)"
Write-Host "  $folderB\  (3-4 .mp4 files)"

Write-Host "`nOptional:" -ForegroundColor Yellow
Write-Host "  $testRoot\test.srt  (subtitle file)"
Write-Host "  $testRoot\folder_a\test.srt  (folder subtitle)"

Write-Host "`nTest setup complete!" -ForegroundColor Green
