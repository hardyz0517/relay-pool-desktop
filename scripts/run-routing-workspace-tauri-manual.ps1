param(
  [string]$ProfileName = "task23-routing-workspace",
  [string]$RustupToolchain = "1.95.0",
  [int]$DevServerPort = 1431,
  [string]$AppIdentifier = "dev.relaypool.desktop.routing-workspace-manual",
  [int]$WebViewDebugPort = 0
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Resolve-Path (Join-Path $scriptDir "..")
$profileRoot = Join-Path $repoRoot "output\manual-routing-workspace\$ProfileName"
$appData = Join-Path $profileRoot "AppData\Roaming"
$localAppData = Join-Path $profileRoot "AppData\Local"
$tempDir = Join-Path $profileRoot "Temp"
$overlayPath = Join-Path $profileRoot "tauri-dev-overlay.json"

New-Item -ItemType Directory -Force -Path $appData, $localAppData, $tempDir | Out-Null

$overlay = @{
  identifier = $AppIdentifier
  build = @{
    devUrl = "http://127.0.0.1:$DevServerPort"
    beforeDevCommand = "pnpm dev --port $DevServerPort --strictPort"
  }
} | ConvertTo-Json -Depth 4
Set-Content -Path $overlayPath -Value $overlay -Encoding UTF8

$env:APPDATA = $appData
$env:LOCALAPPDATA = $localAppData
$env:TEMP = $tempDir
$env:TMP = $tempDir
$env:RELAY_POOL_DEV_AUTO_START_PROXY = "0"
$env:RELAY_POOL_START_PROXY_ON_LAUNCH = "0"
$env:RUSTUP_TOOLCHAIN = $RustupToolchain
if ($WebViewDebugPort -gt 0) {
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$WebViewDebugPort"
}

Write-Host "Relay Pool routing workspace manual verification profile"
Write-Host "Repository: $repoRoot"
Write-Host "Profile root: $profileRoot"
Write-Host "Tauri app identifier: $AppIdentifier"
Write-Host "Tauri config overlay: $overlayPath"
Write-Host "Vite dev URL: http://127.0.0.1:$DevServerPort"
if ($WebViewDebugPort -gt 0) {
  Write-Host "WebView2 debug port: $WebViewDebugPort"
}
Write-Host "APPDATA: $env:APPDATA"
Write-Host "LOCALAPPDATA: $env:LOCALAPPDATA"
Write-Host "TEMP: $env:TEMP"
Write-Host ""
Write-Host "Use only synthetic/redacted stations, keys, request ids and screenshots in this profile."
Write-Host "Do not import real API keys, cookies, provider URLs, prompts or user request logs."
Write-Host "The temporary app identifier keeps the verification data store and installation lease separate from the normal app."
Write-Host ""

Set-Location $repoRoot
pnpm.cmd tauri dev --config $overlayPath
