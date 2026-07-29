param(
    [long]$DatasetSizeBytes = 1073741824,
    [string]$WorkingDirectory = "",
    [switch]$KeepDataset
)

$ErrorActionPreference = "Stop"

if ($DatasetSizeBytes -le 0) {
    throw "DatasetSizeBytes must be positive"
}

if ([string]::IsNullOrWhiteSpace($WorkingDirectory)) {
    $WorkingDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("relay-pool-portable-migration-perf-" + [guid]::NewGuid().ToString("N"))
}

New-Item -ItemType Directory -Force -Path $WorkingDirectory | Out-Null
$datasetPath = Join-Path $WorkingDirectory "portable-migration-dataset.bin"
$reportPath = Join-Path $WorkingDirectory "portable-migration-performance-report.json"
$bufferSize = 1024 * 1024
$buffer = New-Object byte[] $bufferSize
$written = [long]0
$progressEvents = 0
$process = [System.Diagnostics.Process]::GetCurrentProcess()
$peakRss = [long]0
$started = Get-Date

$stream = [System.IO.File]::Open($datasetPath, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
try {
    while ($written -lt $DatasetSizeBytes) {
        $remaining = $DatasetSizeBytes - $written
        $chunk = [int][Math]::Min($bufferSize, $remaining)
        $stream.Write($buffer, 0, $chunk)
        $written += $chunk
        $progressEvents += 1
        $process.Refresh()
        if ($process.WorkingSet64 -gt $peakRss) { $peakRss = $process.WorkingSet64 }
    }
    $stream.Flush($true)
}
finally {
    $stream.Dispose()
}

$elapsed = (Get-Date) - $started
$hashStarted = Get-Date
$sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $datasetPath).Hash.ToLowerInvariant()
$hashElapsed = (Get-Date) - $hashStarted

$report = [ordered]@{
    schemaVersion = 1
    qualification = "portable-migration-streaming-harness"
    datasetSizeBytes = $DatasetSizeBytes
    bufferBytes = $bufferSize
    progressEvents = $progressEvents
    peakRssBytes = $peakRss
    rssShouldBeBelowBytes = 512MB
    datasetSha256 = $sha256
    writeElapsedMs = [int64]$elapsed.TotalMilliseconds
    hashElapsedMs = [int64]$hashElapsed.TotalMilliseconds
    containsRealSecrets = $false
    notes = @(
        "Dataset is deterministic zero-filled test data and contains no real API keys, cookies, tokens, or user records.",
        "Task 20 must replace this harness result with export/import measurements from the enabled portable migration path before release."
    )
}

$report | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $reportPath -Encoding UTF8

if ($peakRss -gt 512MB) {
    throw "Peak RSS $peakRss exceeded SHOULD budget 536870912; report written to $reportPath"
}
if ($bufferSize -gt 1MB) {
    throw "Streaming buffer exceeded 1 MiB"
}

if (-not $KeepDataset) {
    Remove-Item -LiteralPath $datasetPath -Force
}

Write-Host "portable migration performance harness passed"
Write-Host "report: $reportPath"
