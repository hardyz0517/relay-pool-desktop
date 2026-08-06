param(
  [string]$CargoManifest = "src-tauri/Cargo.toml",
  [string]$OutputPath = "output/routing-operational/self-check/local-self-check/routing-operational-local-self-check-latest.json"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

function Invoke-LocalSelfCheckStep {
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

function Invoke-CaptureText {
  param(
    [string]$Command,
    [string[]]$Arguments = @()
  )
  try {
    $value = (& $Command @Arguments 2>$null)
    if ($LASTEXITCODE -ne 0) { return $null }
    return ($value -join "`n").Trim()
  } catch {
    return $null
  }
}

function Write-LocalSelfCheckReport {
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
$sourceRevision = (& git rev-parse HEAD).Trim()
$dirtyStatus = @(& git status --porcelain)
$steps = New-Object System.Collections.Generic.List[object]
$failures = New-Object System.Collections.Generic.List[object]

$commandPlan = @(
  [pscustomobject]@{ id = "self-check-contract"; command = "node"; arguments = @("scripts/routing-operational-local-self-check.test.mjs"); proves = @("Task 27 runner command coverage", "no provider credential requirement", "development-only self-check semantics") },
  [pscustomobject]@{ id = "operational-fact-reader"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "operational_fact_reader", "--", "--nocapture"); proves = @("snapshot-consistent operational fact reads", "fixed query count candidate assembly", "no secret/raw URL leakage from fact bundle") },
  [pscustomobject]@{ id = "known-schema-import"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "persistence_upgrade", "--", "--nocapture"); proves = @("legacy fixture DB imports to current dev schema", "request lifecycle/import facts survive reimport", "future/unsupported schema fails closed") },
  [pscustomobject]@{ id = "upgrade-recovery"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "persistence_upgrade_recovery", "--", "--nocapture"); proves = @("interrupted upgrade has deterministic recovery plan", "unsafe observations halt without destructive replay") },
  [pscustomobject]@{ id = "fresh-generation-two"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "persistence_startup_cutover", "--", "--nocapture"); proves = @("fresh generation-two data store uses current DB filename", "v3 data-dir config round-trips") },
  [pscustomobject]@{ id = "sanitizer-resume-startup"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_url_sanitizer_migration", "--", "--nocapture"); proves = @("sanitizer interruption/resume", "runtime ready gate blocks incomplete sanitizer", "schema17 startup upgrade sanitizes before open") },
  [pscustomobject]@{ id = "startup-lifecycle-reconciliation"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_lifecycle_reconciliation", "--", "--nocapture"); proves = @("startup marks interrupted requests honestly", "bounded reconciliation batches") },
  [pscustomobject]@{ id = "production-startup-admission"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_production_startup_shutdown", "--", "--nocapture"); proves = @("production command facade reconciles before proxy admission", "shutdown leaves no active requests") },
  [pscustomobject]@{ id = "configured-policy-fields"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_policy_field_e2e", "--", "--nocapture"); proves = @("configured profile uses alias/preferred/backup fields in real route execution") },
  [pscustomobject]@{ id = "catalog-decision-cost"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_catalog_loopback", "--", "--nocapture"); proves = @("model listing fallback persists outcomes", "decision/cost stores remain queryable") },
  [pscustomobject]@{ id = "redaction-boundary"; command = "cargo"; arguments = @("test", "--locked", "--manifest-path", $CargoManifest, "--test", "routing_security_boundaries", "--", "--nocapture"); proves = @("request logs/traces/errors do not rehydrate full URL or secrets") },
  [pscustomobject]@{ id = "proxy-auth-boundary"; command = "node"; arguments = @("scripts/local-proxy-auth-contract.test.mjs"); proves = @("proxy authentication and ingress redaction ownership remain on the active boundary") },
  [pscustomobject]@{ id = "legacy-doc-anti-regression"; command = "node"; arguments = @("scripts/routing-operational-legacy-doc-consistency.test.mjs"); proves = @("debug legacy runtime deletion is reflected in current docs", "legacy env switch is not documented as a supported fallback") },
  [pscustomobject]@{ id = "task26-self-check-preflight"; command = "node"; arguments = @("scripts/routing-operational-qualification.mjs", "--preflight"); proves = @("Task 26 self-check wiring remains available") }
)

foreach ($entry in $commandPlan) {
  $record = Invoke-LocalSelfCheckStep -Command $entry.command -Arguments $entry.arguments
  $steps.Add($record)
  if ($record.exitCode -ne 0) {
    $failures.Add($record)
    break
  }
}

$finishedAt = Get-Date
$dirtyStatusAtFinish = @(& git status --porcelain)
$report = [ordered]@{
  schemaVersion = 1
  kind = "routing-operational-local-self-check"
  sourceRevision = $sourceRevision
  worktreeCleanAtStart = ($dirtyStatus.Count -eq 0)
  dirtyStatusAtStart = $dirtyStatus
  worktreeCleanAtFinish = ($dirtyStatusAtFinish.Count -eq 0)
  dirtyStatusAtFinish = $dirtyStatusAtFinish
  startedAt = $startedAt.ToString("o")
  finishedAt = $finishedAt.ToString("o")
  environment = [ordered]@{
    os = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
    processArchitecture = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
    powershell = $PSVersionTable.PSVersion.ToString()
    node = Invoke-CaptureText "node" @("--version")
    pnpm = Invoke-CaptureText "pnpm.cmd" @("--version")
    rustc = Invoke-CaptureText "rustc" @("--version")
    cargo = Invoke-CaptureText "cargo" @("--version")
  }
  boundaries = [ordered]@{
    realProviderRequired = $false
    realProviderStatus = "not-run-without-user-authorization"
    recoveryContract = "reset/reimport/reconfigure with the current dev binary"
    trackedRuntimeResultsAllowed = $false
  }
  commandPlan = @($commandPlan | ForEach-Object {
      [ordered]@{
        id = $_.id
        command = "$($_.command) $($_.arguments -join ' ')"
        proves = $_.proves
      }
    })
  totalSteps = $steps.Count
  failures = @($failures.ToArray())
  steps = @($steps.ToArray())
}

Write-LocalSelfCheckReport -Report $report

if ($failures.Count -gt 0) {
  Write-Error "Routing operational local self-check failed. See $OutputPath"
}

Write-Host "routing operational local self-check passed. See $OutputPath"
