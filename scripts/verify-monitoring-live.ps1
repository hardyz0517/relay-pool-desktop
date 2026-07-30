param(
    [switch]$AuthorizeLiveProviderProbe,
    [ValidateSet("openai", "anthropic", "gemini", "xai-grok", "generic-openai")]
    [string[]]$Providers = @("openai", "anthropic", "gemini", "xai-grok", "generic-openai"),
    [ValidateSet("standard_api", "codex_cli_compat", "claude_code_compat", "gemini_cli_compat")]
    [string[]]$Profiles = @("standard_api"),
    [string]$OutputPath = "docs/superpowers/audits/status-monitoring-live-latest.json"
)

$ErrorActionPreference = "Stop"

if (-not $AuthorizeLiveProviderProbe) {
    throw "Live provider probes are disabled by default. Re-run with -AuthorizeLiveProviderProbe after confirming the account owner permits low-frequency synthetic probes."
}

$providerEnv = @{
    "openai" = "OPENAI_API_KEY"
    "anthropic" = "ANTHROPIC_API_KEY"
    "gemini" = "GEMINI_API_KEY"
    "xai-grok" = "XAI_API_KEY"
    "generic-openai" = "MONITORING_GENERIC_OPENAI_API_KEY"
}

$records = @()
foreach ($provider in $Providers) {
    $envName = $providerEnv[$provider]
    $hasSecret = -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($envName))
    $records += [ordered]@{
        provider = $provider
        secretEnv = $envName
        secretPresent = $hasSecret
        profiles = $Profiles
        status = if ($hasSecret) { "ready_for_manual_app_probe" } else { "missing_secret" }
        note = "This script intentionally does not print or transmit secrets. Run the desktop app monitor against the configured local SecretManager/env entry, then cross-check DB evidence with verify-monitoring-db.ps1."
    }
}

if ($Providers -contains "xai-grok" -and $Profiles -contains "grok_cli_compat") {
    throw "grok_cli_compat is intentionally unavailable until separately verified and enabled."
}

$result = [ordered]@{
    kind = "status-monitoring-live-authorization-gate"
    generatedAt = (Get-Date).ToString("o")
    authorized = $true
    providers = $records
    requiredEvidence = @(
        "client-visible terminal/app execution id",
        "runtime sanitized diagnostics",
        "SQLite channel_monitor_executions row",
        "SQLite channel_monitor_target_results row",
        "SQLite channel_monitor_attempts row",
        "SQLite station_key_health_observations row"
    )
}

$outputDirectory = Split-Path -Parent $OutputPath
if ($outputDirectory -and -not (Test-Path -LiteralPath $outputDirectory)) {
    New-Item -ItemType Directory -Path $outputDirectory | Out-Null
}
$resolvedOutputPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path (Get-Location) $OutputPath
}
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($resolvedOutputPath, ($result | ConvertTo-Json -Depth 8), $utf8NoBom)
Write-Host "Live verification gate written to $OutputPath"
