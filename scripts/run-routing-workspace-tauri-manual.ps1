param(
  [string]$ProfileName = "task23-routing-workspace",
  [string]$RustupToolchain = "1.95.0"
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..")
$profileRoot = Join-Path $repoRoot "output\manual-routing-workspace\$ProfileName"
$appData = Join-Path $profileRoot "AppData\Roaming"
$localAppData = Join-Path $profileRoot "AppData\Local"
$tempDir = Join-Path $profileRoot "Temp"

New-Item -ItemType Directory -Force -Path $appData, $localAppData, $tempDir | Out-Null

$env:APPDATA = $appData
$env:LOCALAPPDATA = $localAppData
$env:TEMP = $tempDir
$env:TMP = $tempDir
$env:RELAY_POOL_DEV_AUTO_START_PROXY = "0"
$env:RELAY_POOL_START_PROXY_ON_LAUNCH = "0"
$env:RUSTUP_TOOLCHAIN = $RustupToolchain

Write-Host "Relay Pool routing workspace manual verification profile"
Write-Host "Repository: $repoRoot"
Write-Host "Profile root: $profileRoot"
Write-Host "APPDATA: $env:APPDATA"
Write-Host "LOCALAPPDATA: $env:LOCALAPPDATA"
Write-Host "TEMP: $env:TEMP"
Write-Host ""
Write-Host "Use only synthetic/redacted stations, keys, request ids and screenshots in this profile."
Write-Host "Do not import real API keys, cookies, provider URLs, prompts or user request logs."
Write-Host ""

Set-Location $repoRoot
pnpm.cmd tauri:dev
