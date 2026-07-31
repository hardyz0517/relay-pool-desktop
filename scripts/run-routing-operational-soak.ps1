param(
  [int]$DurationMinutes = 5
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repoRoot

$deadline = (Get-Date).AddMinutes([Math]::Max($DurationMinutes, 0))
$iteration = 0

do {
  $iteration += 1
  Write-Host "routing operational loopback soak iteration $iteration"

  cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_loopback_e2e -- --nocapture
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_catalog_loopback -- --nocapture
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

  cargo test --locked --manifest-path src-tauri/Cargo.toml --test routing_policy_field_e2e -- --nocapture
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} while ((Get-Date) -lt $deadline)

Write-Host "routing operational loopback soak passed after $iteration iteration(s)"
