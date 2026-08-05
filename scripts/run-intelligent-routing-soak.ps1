param([switch]$Smoke)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
  node scripts/intelligent-routing-qualification.mjs
  if (-not $Smoke) {
    cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_dispatch -- --nocapture
    cargo test --locked --manifest-path src-tauri/Cargo.toml --test intelligent_routing_coordinator -- --nocapture
  }
} finally { Pop-Location }
