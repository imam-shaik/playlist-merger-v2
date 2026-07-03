# Record Test Results
param(
    [Parameter(Mandatory=$true)]
    [string]$TestName,
    
    [Parameter(Mandatory=$true)]
    [ValidateSet("PASS", "FAIL", "WARN")]
    [string]$Status,
    
    [string]$Notes = ""
)

$resultFile = "C:\test\merge\test-results.json"

# Load existing results
if (Test-Path $resultFile) {
    $results = Get-Content $resultFile | ConvertFrom-Json
} else {
    $results = @()
}

# Remove existing entry for this test
$results = $results | Where-Object { $_.Test -ne $TestName }

# Add new result
$results += [PSCustomObject]@{
    Test = $TestName
    Status = $Status
    Notes = $Notes
    Timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
}

# Save results
$results | ConvertTo-Json | Set-Content $resultFile

# Display
$color = if ($Status -eq "PASS") { "Green" } elseif ($Status -eq "FAIL") { "Red" } else { "Yellow" }
Write-Host "[$Status] $TestName" -ForegroundColor $color
if ($Notes) { Write-Host "       $Notes" -ForegroundColor Gray }

Write-Host "`nResults saved to: $resultFile" -ForegroundColor Gray
