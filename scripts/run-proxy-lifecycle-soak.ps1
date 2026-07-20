param(
  [int]$DurationMinutes = 60,
  [switch]$Smoke
)

$ErrorActionPreference = "Stop"

if ($Smoke) {
  $DurationMinutes = 0
}

$root = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $root "src-tauri/Cargo.toml"
$samples = New-Object System.Collections.Generic.List[double]
$deadline = (Get-Date).AddMinutes($DurationMinutes)
$pass = 0

do {
  $pass += 1
  $started = Get-Date
  Write-Host "proxy lifecycle soak pass $pass started at $($started.ToString('o'))"

  cargo test --manifest-path $manifest --lib services::proxy::soak_tests -- --nocapture
  if ($LASTEXITCODE -ne 0) {
    throw "proxy lifecycle soak failed on pass $pass"
  }

  $elapsed = ((Get-Date) - $started).TotalMilliseconds
  $samples.Add($elapsed)
  Write-Host "proxy lifecycle soak pass $pass completed in $([Math]::Round($elapsed, 2)) ms"
} while (-not $Smoke -and (Get-Date) -lt $deadline)

$ordered = @($samples | Sort-Object)
$p95Index = [Math]::Max(0, [Math]::Ceiling($ordered.Count * 0.95) - 1)
$p95 = if ($ordered.Count -gt 0) { $ordered[$p95Index] } else { 0 }

Write-Host "proxy lifecycle soak passes: $pass"
Write-Host "proxy lifecycle soak samples_ms: $($samples -join ',')"
Write-Host "proxy lifecycle soak p95_ms: $([Math]::Round($p95, 2))"
