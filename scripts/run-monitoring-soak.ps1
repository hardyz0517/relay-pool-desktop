param(
    [int]$DurationMinutes = 60,
    [switch]$Quick,
    [switch]$ReleaseBuild,
    [switch]$MixedProviderWorkload,
    [string]$CargoManifest = "src-tauri/Cargo.toml",
    [string]$OutputPath = "docs/superpowers/audits/status-monitoring-soak-latest.json"
)

$ErrorActionPreference = "Stop"

if ($DurationMinutes -lt 1) {
    throw "DurationMinutes must be >= 1."
}

$startedAt = Get-Date
$deadline = if ($Quick) {
    $startedAt.AddMinutes([Math]::Min($DurationMinutes, 1))
} else {
    $startedAt.AddMinutes($DurationMinutes)
}

function New-CargoMonitoringTestCommand {
    param([string]$Suite)
    $arguments = @("test", "--manifest-path", $CargoManifest)
    if ($ReleaseBuild) {
        $arguments += "--release"
    }
    $arguments += @("--test", $Suite, "--", "--nocapture")
    return @("cargo", $arguments)
}

$coreSuites = @(
    "monitoring_faults",
    "monitoring_concurrency",
    "monitoring_scheduler",
    "monitoring_buckets_retention",
    "monitoring_read_model"
)

$mixedProviderSuites = @(
    "monitoring_adapter_contracts",
    "monitoring_profile_golden",
    "monitoring_transport",
    "monitoring_orchestrator",
    "monitoring_execution_integration",
    "monitoring_faults",
    "monitoring_concurrency",
    "monitoring_scheduler",
    "monitoring_buckets_retention",
    "monitoring_read_model"
)

$selectedSuites = if ($MixedProviderWorkload) {
    $mixedProviderSuites
} else {
    $coreSuites
}

$commands = @()
foreach ($suite in $selectedSuites) {
    $commands += ,(New-CargoMonitoringTestCommand -Suite $suite)
}

$iterations = 0
$failures = @()

while ((Get-Date) -lt $deadline) {
    foreach ($entry in $commands) {
        $command = $entry[0]
        $arguments = $entry[1]
        $runStarted = Get-Date
        & $command @arguments
        $exitCode = $LASTEXITCODE
        $elapsedMs = [int]((Get-Date) - $runStarted).TotalMilliseconds
        if ($exitCode -ne 0) {
            $failures += [ordered]@{
                command = "$command $($arguments -join ' ')"
                exitCode = $exitCode
                elapsedMs = $elapsedMs
                failedAt = (Get-Date).ToString("o")
            }
            break
        }
    }
    $iterations += 1
    if ($failures.Count -gt 0 -or $Quick) {
        break
    }
}

$result = [ordered]@{
    kind = "status-monitoring-soak"
    startedAt = $startedAt.ToString("o")
    finishedAt = (Get-Date).ToString("o")
    requestedDurationMinutes = $DurationMinutes
    quick = [bool]$Quick
    buildProfile = if ($ReleaseBuild) { "release" } else { "debug" }
    workload = if ($MixedProviderWorkload) { "mixed-provider-stream-retry-fallback-missing" } else { "core-fault-concurrency-scheduler-read-model" }
    commandPlan = @($commands | ForEach-Object { "$($_[0]) $($_[1] -join ' ')" })
    iterations = $iterations
    failures = $failures
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

if ($failures.Count -gt 0) {
    Write-Error "Monitoring soak failed. See $OutputPath"
}

Write-Host "Monitoring soak completed. See $OutputPath"
