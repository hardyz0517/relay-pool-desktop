param(
  [int]$DurationMinutes = 5,
  [switch]$Smoke,
  [string]$CargoManifest = "src-tauri/Cargo.toml",
  [string]$OutputPath = "output/routing-operational/qualification/soak/routing-operational-soak-latest.json"
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

function Invoke-RoutingSoakStep {
  param(
    [string]$Command,
    [string[]]$Arguments
  )

  $started = Get-Date
  & $Command @Arguments | ForEach-Object { Write-Host $_ }
  $exitCode = $LASTEXITCODE
  $elapsedMs = [int]((Get-Date) - $started).TotalMilliseconds

  return [ordered]@{
    command = "$Command $($Arguments -join ' ')"
    exitCode = $exitCode
    elapsedMs = $elapsedMs
    startedAt = $started.ToString("o")
    finishedAt = (Get-Date).ToString("o")
  }
}

function Write-RoutingSoakReport {
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
$deadline = $startedAt.AddMinutes($DurationMinutes)
$iteration = 0
$steps = New-Object System.Collections.Generic.List[object]
$failures = New-Object System.Collections.Generic.List[object]

$commandPlan = @(
  [pscustomobject]@{ command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_loopback_e2e", "--", "--nocapture") },
  [pscustomobject]@{ command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_catalog_loopback", "--", "--nocapture") },
  [pscustomobject]@{ command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_policy_field_e2e", "--", "--nocapture") }
)

do {
  $iteration += 1
  Write-Host "routing operational loopback soak iteration $iteration"

  foreach ($entry in $commandPlan) {
    $record = Invoke-RoutingSoakStep -Command $entry.command -Arguments $entry.arguments
    $steps.Add($record)
    if ($record.exitCode -ne 0) {
      $failures.Add($record)
      break
    }
  }

  if ($failures.Count -gt 0) {
    break
  }
} while (-not $Smoke -and (Get-Date) -lt $deadline)

$finishedAt = Get-Date
$report = [ordered]@{
  kind = "routing-operational-loopback-soak"
  candidateRevision = $candidateRevision
  worktreeCleanAtStart = ($dirtyStatus.Count -eq 0)
  dirtyStatusAtStart = $dirtyStatus
  startedAt = $startedAt.ToString("o")
  finishedAt = $finishedAt.ToString("o")
  requestedDurationMinutes = $DurationMinutes
  smoke = [bool]$Smoke
  commandPlan = @($commandPlan | ForEach-Object { "$($_.command) $($_.arguments -join ' ')" })
  iterations = $iteration
  totalSteps = $steps.Count
  failures = @($failures.ToArray())
  steps = @($steps.ToArray())
}

Write-RoutingSoakReport -Report $report

if ($failures.Count -gt 0) {
  Write-Error "Routing operational loopback soak failed. See $OutputPath"
}

Write-Host "routing operational loopback soak passed after $iteration iteration(s). See $OutputPath"
