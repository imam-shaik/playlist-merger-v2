# View Test Results

$resultFile = "C:\test\merge\test-results.json"

if (-not (Test-Path $resultFile)) {
    Write-Host "No test results found." -ForegroundColor Yellow
    Write-Host "Run tests first, then record results with: .\test-record.ps1"
    exit
}

$results = Get-Content $resultFile | ConvertFrom-Json

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "   TEST RESULTS SUMMARY" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

# Summary
$pass = ($results | Where-Object { $_.Status -eq "PASS" }).Count
$fail = ($results | Where-Object { $_.Status -eq "FAIL" }).Count
$warn = ($results | Where-Object { $_.Status -eq "WARN" }).Count
$total = $results.Count

Write-Host "TOTAL: $total tests" -ForegroundColor White
Write-Host "PASS:  $pass" -ForegroundColor Green
Write-Host "FAIL:  $fail" -ForegroundColor Red
Write-Host "WARN:  $warn" -ForegroundColor Yellow

Write-Host "`n----------------------------------------" -ForegroundColor Gray

# Detailed results
foreach ($result in $results) {
    $color = switch ($result.Status) {
        "PASS" { "Green" }
        "FAIL" { "Red" }
        "WARN" { "Yellow" }
    }
    Write-Host "[$($result.Status)] $($result.Test)" -ForegroundColor $color
    if ($result.Notes) {
        Write-Host "       $($result.Notes)" -ForegroundColor Gray
    }
}

Write-Host "`n----------------------------------------" -ForegroundColor Gray

# Certification status
if ($fail -eq 0 -and $warn -eq 0) {
    Write-Host "`nCERTIFICATION: PASS" -ForegroundColor Green
    Write-Host "All tests passed!" -ForegroundColor Green
} elseif ($fail -eq 0) {
    Write-Host "`nCERTIFICATION: PASS (with warnings)" -ForegroundColor Yellow
    Write-Host "No failures, but some warnings to review." -ForegroundColor Yellow
} else {
    Write-Host "`nCERTIFICATION: FAIL" -ForegroundColor Red
    Write-Host "$fail test(s) failed. Review issues." -ForegroundColor Red
}

Write-Host ""
