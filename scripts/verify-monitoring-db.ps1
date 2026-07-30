param(
    [Parameter(Mandatory = $true)]
    [string]$DatabasePath,
    [int]$MaxRunningAgeMinutes = 10,
    [string]$OutputPath = "docs/superpowers/audits/status-monitoring-db-latest.json"
)

$ErrorActionPreference = "Stop"

$resolvedDatabase = Resolve-Path -LiteralPath $DatabasePath
$sqlite = Get-Command sqlite3 -ErrorAction SilentlyContinue
if (-not $sqlite) {
    throw "sqlite3 is required for DB verification."
}

function Invoke-SqliteScalar {
    param([string]$Sql)
    $value = & $sqlite.Source $resolvedDatabase.Path $Sql
    if ($LASTEXITCODE -ne 0) {
        throw "sqlite3 query failed: $Sql"
    }
    return $value
}

$requiredTables = @(
    "channel_monitor_executions",
    "channel_monitor_target_results",
    "channel_monitor_attempts",
    "channel_monitor_bucket_rollups",
    "channel_monitor_probe_budget_usage",
    "station_key_health_observations"
)

$tableResults = @()
foreach ($table in $requiredTables) {
    $exists = Invoke-SqliteScalar "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '$table';"
    $count = if ($exists -eq "1") { Invoke-SqliteScalar "SELECT COUNT(*) FROM $table;" } else { "missing" }
    $tableResults += [ordered]@{
        table = $table
        exists = ($exists -eq "1")
        rowCount = $count
    }
}

$cutoffMs = [DateTimeOffset]::UtcNow.AddMinutes(-1 * $MaxRunningAgeMinutes).ToUnixTimeMilliseconds()
$staleRunning = Invoke-SqliteScalar "SELECT COUNT(*) FROM channel_monitor_executions WHERE status IN ('queued','running') AND COALESCE(started_at_ms, planned_at_ms, created_at_ms) < $cutoffMs;"
$legacyWrites = Invoke-SqliteScalar "SELECT COUNT(*) FROM channel_monitor_runs;"
$httpOnlyAuthoritative = Invoke-SqliteScalar "SELECT COUNT(*) FROM channel_monitor_target_results WHERE semantic_confidence = 'legacy_http_only' AND health_writeback_decision = 'write';"

$result = [ordered]@{
    kind = "status-monitoring-db-verification"
    generatedAt = (Get-Date).ToString("o")
    databasePath = $resolvedDatabase.Path
    maxRunningAgeMinutes = $MaxRunningAgeMinutes
    tables = $tableResults
    staleRunningExecutions = [int64]$staleRunning
    legacyChannelMonitorRunsRows = [int64]$legacyWrites
    legacyHttpOnlyAuthoritativeWritebacks = [int64]$httpOnlyAuthoritative
    status = if ([int64]$staleRunning -eq 0 -and [int64]$httpOnlyAuthoritative -eq 0) { "pass" } else { "fail" }
    note = "channel_monitor_runs may exist for one read-only observation cycle, but production code must not write it."
}

$outputDirectory = Split-Path -Parent $OutputPath
if ($outputDirectory -and -not (Test-Path -LiteralPath $outputDirectory)) {
    New-Item -ItemType Directory -Path $outputDirectory | Out-Null
}
$resolvedOutputPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path (Get-Location) $OutputPath
}
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($resolvedOutputPath, ($result | ConvertTo-Json -Depth 8), $utf8NoBom)

if ($result.status -ne "pass") {
    Write-Error "Monitoring DB verification failed. See $OutputPath"
}

Write-Host "Monitoring DB verification passed. See $OutputPath"
