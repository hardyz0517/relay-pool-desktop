param(
  [int]$DurationMinutes = 60,
  [switch]$Smoke,
  [string]$CargoManifest = "src-tauri/Cargo.toml",
  [string]$OutputPath = "output/routing-operational/qualification/task24-predeletion/task24-predeletion-gate-latest.json"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

if ($Smoke) {
  $DurationMinutes = 0
}

if ($DurationMinutes -lt 0) {
  throw "DurationMinutes must be >= 0."
}

function Invoke-Task24GateStep {
  param(
    [string]$Name,
    [string]$Command,
    [string[]]$Arguments
  )

  $started = Get-Date
  Write-Host "task24 pre-deletion gate step '$Name' started at $($started.ToString('o'))"
  & $Command @Arguments | ForEach-Object { Write-Host $_ }
  $exitCode = $LASTEXITCODE
  $elapsedMs = [int]((Get-Date) - $started).TotalMilliseconds
  $record = [ordered]@{
    name = $Name
    command = "$Command $($Arguments -join ' ')"
    exitCode = $exitCode
    elapsedMs = $elapsedMs
    startedAt = $started.ToString("o")
    finishedAt = (Get-Date).ToString("o")
  }
  return $record
}

function Add-Task24GateStepOrThrow {
  param(
    [string]$Name,
    [string]$Command,
    [string[]]$Arguments
  )

  $record = Invoke-Task24GateStep -Name $Name -Command $Command -Arguments $Arguments
  $steps.Add($record)
  if ($record.exitCode -ne 0) {
    throw "task24 pre-deletion gate step '$Name' failed with exit code $($record.exitCode)"
  }
}

function Write-Task24GateReport {
  param([object]$Report)

  $resolvedOutputPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
  } else {
    Join-Path (Get-Location) $OutputPath
  }
  $outputDirectory = Split-Path -Parent $resolvedOutputPath
  if ($outputDirectory -and -not (Test-Path -LiteralPath $outputDirectory)) {
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
  }

  $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($resolvedOutputPath, ($Report | ConvertTo-Json -Depth 10), $utf8NoBom)
}

$startedAt = Get-Date
$candidateRevision = (& git rev-parse HEAD).Trim()
$dirtyStatus = @(& git status --porcelain)
$steps = New-Object System.Collections.Generic.List[object]
$failure = $null
$soakOutputPath = "output/routing-operational/qualification/task24-predeletion/task24-routing-operational-soak-latest.json"

try {
  Add-Task24GateStepOrThrow -Name "production-composition" -Command "cargo" -Arguments @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_production_composition", "--", "--nocapture")
  Add-Task24GateStepOrThrow -Name "stream-finalization-faults" -Command "cargo" -Arguments @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_stream_finalization_faults", "--", "--nocapture")
  Add-Task24GateStepOrThrow -Name "redaction-contract" -Command "node" -Arguments @("scripts/local-routing-redaction.test.mjs")

  $soakArgs = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/run-routing-operational-soak.ps1", "-DurationMinutes", "$DurationMinutes", "-CargoManifest", $CargoManifest, "-OutputPath", $soakOutputPath)
  if ($Smoke) {
    $soakArgs += "-Smoke"
  }
  Add-Task24GateStepOrThrow -Name "loopback-soak" -Command "powershell" -Arguments $soakArgs
} catch {
  $failure = $_.Exception.Message
}

$finishedAt = Get-Date
$worktreeCleanAtStart = ($dirtyStatus.Count -eq 0)
$deletionApproved = [bool](
  ($null -eq $failure) `
    -and (-not [bool]$Smoke) `
    -and ($DurationMinutes -ge 60) `
    -and $worktreeCleanAtStart
)
$report = [ordered]@{
  kind = "routing-operational-task24-predeletion-gate"
  candidateRevision = $candidateRevision
  worktreeCleanAtStart = $worktreeCleanAtStart
  dirtyStatusAtStart = $dirtyStatus
  startedAt = $startedAt.ToString("o")
  finishedAt = $finishedAt.ToString("o")
  requestedDurationMinutes = $DurationMinutes
  smoke = [bool]$Smoke
  deletionApproved = $deletionApproved
  approvalScope = "Task 24 pre-deletion observation only; final Task 26 qualification must rerun after deletion."
  soakOutputPath = $soakOutputPath
  steps = @($steps.ToArray())
  failure = $failure
}

Write-Task24GateReport -Report $report

if ($failure) {
  Write-Error "Task 24 pre-deletion gate failed. See $OutputPath. $failure"
}

if (-not $report.deletionApproved) {
  Write-Host "Task 24 pre-deletion gate completed without deletion approval. A clean 60-minute non-smoke run is required for approval. See $OutputPath"
} else {
  Write-Host "Task 24 pre-deletion gate approved. See $OutputPath"
}
