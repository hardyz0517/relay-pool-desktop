[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $repoRoot
try {
    & node scripts/architecture/validate-advisory-exceptions.mjs
    if ($LASTEXITCODE -ne 0) { throw "advisory exception validation failed with exit code $LASTEXITCODE" }

    & node scripts/architecture/check-npm-audit.mjs
    if ($LASTEXITCODE -ne 0) { throw "npm advisory gate failed with exit code $LASTEXITCODE" }

    $cargoDenyVersion = (& cargo deny --version)
    if ($LASTEXITCODE -ne 0) { throw "cargo-deny is required and must be installed at the ADR-pinned version" }
    if ($cargoDenyVersion -notmatch '^cargo-deny 0\.20\.2\b') { throw "cargo-deny 0.20.2 is required; found $cargoDenyVersion" }
    $generatedConfig = "output/architecture-scale/generated/deny.toml"
    & node scripts/architecture/prepare-cargo-deny-config.mjs --output $generatedConfig
    if ($LASTEXITCODE -ne 0) { throw "cargo-deny config generation failed with exit code $LASTEXITCODE" }
    $cargoDenyArgs = @()
    if ([Environment]::GetEnvironmentVariable("RELAY_POOL_CARGO_DENY_OFFLINE") -eq "1") {
        $cargoDenyArgs += "--offline"
    }
    $cargoDenyArgs += @(
        "--manifest-path", "src-tauri/Cargo.toml",
        "--config", $generatedConfig,
        "--target", "x86_64-pc-windows-msvc",
        "check", "advisories", "bans", "licenses", "sources"
    )
    & cargo deny @cargoDenyArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo deny failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}
