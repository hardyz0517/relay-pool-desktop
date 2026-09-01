[CmdletBinding()]
param(
  [ValidateSet(
    "frontend-contracts",
    "startup-probe-plan",
    "migrations",
    "secret-baseline",
    "sanitizer",
    "routing-v3",
    "schema15-fixture",
    "persistence-artifacts",
    "install-contract"
  )]
  [string]$StartModule = "frontend-contracts"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$moduleOrder = @(
  "frontend-contracts",
  "startup-probe-plan",
  "migrations",
  "secret-baseline",
  "sanitizer",
  "routing-v3",
  "schema15-fixture",
  "persistence-artifacts",
  "install-contract"
)
$startIndex = [Array]::IndexOf($moduleOrder, $StartModule)
if ($startIndex -lt 0) { throw "unknown schema upgrade module: $StartModule" }

function Invoke-Checked([string]$Name, [string]$Command, [string[]]$Arguments = @()) {
  Write-Host "::group::$Name"
  try {
    # Capture the native exit code before formatting the output.  Cmdlets in a
    # pipeline can otherwise make it difficult to tell which command's status
    # is being checked, and a filtered test could be reported as a false green.
    $outputItems = @(& $Command @Arguments 2>&1)
    $exitCode = $LASTEXITCODE
    if ($null -eq $exitCode) { $exitCode = 0 }
    $output = ($outputItems | Out-String)
    Write-Host $output
    if ($exitCode -ne 0) { throw "$Name failed with exit code $exitCode" }
    return $output
  } finally {
    Write-Host "::endgroup::"
  }
}

function Invoke-CargoTests([string]$Name, [string[]]$Arguments) {
  $output = Invoke-Checked $Name "cargo" $Arguments
  $counts = [regex]::Matches($output, "running\s+(\d+)\s+tests?") |
    ForEach-Object { [int]$_.Groups[1].Value }
  if (-not ($counts | Where-Object { $_ -gt 0 })) {
    throw "$Name executed zero tests; refusing a false-green filtered command"
  }
}

Push-Location $repoRoot
try {
  for ($index = $startIndex; $index -lt $moduleOrder.Count; $index++) {
    $module = $moduleOrder[$index]
    switch ($module) {
      "frontend-contracts" {
        Invoke-Checked "Frontend upgrade contracts" "node" @("scripts/data-store-upgrade-matrix.test.mjs") | Out-Null
      }
      "startup-probe-plan" {
        Invoke-CargoTests "Startup probe tests" @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "startup_probe", "--", "--nocapture")
        Invoke-CargoTests "Startup upgrade planner tests" @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "startup_upgrade", "--", "--nocapture")
      }
      "migrations" {
        Invoke-CargoTests "Persistence migration tests" @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "persistence::migrations::tests", "--", "--nocapture")
      }
      "secret-baseline" {
        Invoke-CargoTests "Encrypted secret baseline tests" @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "baseline_conversion::tests", "--", "--nocapture")
      }
      "sanitizer" {
        Invoke-CargoTests "Request-log sanitizer integration tests" @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--test", "routing_url_sanitizer_migration", "--", "--nocapture")
      }
      "routing-v3" {
        Invoke-CargoTests "Routing v3 migration tests" @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--test", "routing_v3_migration", "--", "--nocapture")
      }
      "schema15-fixture" {
        Invoke-CargoTests "Schema 15 frozen fixture upgrade tests" @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--test", "schema15_upgrade_fixture", "--", "--nocapture")
      }
      "persistence-artifacts" {
        Invoke-Checked "Persistence artifact policy" "pnpm.cmd" @("verify:persistence-artifacts") | Out-Null
      }
      "install-contract" {
        Invoke-Checked "Install upgrade matrix contract" "node" @("scripts/install-upgrade-matrix-contract.test.mjs") | Out-Null
      }
    }
    Write-Host "PASS schema-upgrade-module=$module"
  }
} finally {
  Pop-Location
}
