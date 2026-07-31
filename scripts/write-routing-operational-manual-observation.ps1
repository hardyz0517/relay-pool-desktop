[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        "real_local_client_smoke",
        "real_provider_semantic",
        "ccswitch_fixed_local_entry",
        "windows_sleep_resume",
        "ui_timeline_sqlite_reconciliation",
        "default_v2_no_p0_p1"
    )]
    [string]$Scenario,

    [Parameter(Mandatory = $true)]
    [ValidateSet("passed", "failed", "blocked", "not_run")]
    [string]$Status,

    [string[]]$EvidenceIndex = @(),

    [string]$Notes = "",

    [string]$Observer = $env:USERNAME,

    [string]$OutputPath = "output\routing-operational\qualification\manual-observation\routing-operational-manual-observation-latest.json",

    [switch]$AuthorizeManualObservation
)

$ErrorActionPreference = "Stop"

function Fail($Message) {
    Write-Error $Message
    exit 1
}

function Redact-Text {
    param([AllowNull()][string]$Value)
    if ([string]::IsNullOrWhiteSpace($Value)) { return "" }
    $text = [string]$Value
    $text = $text -replace '(?i)bearer\s+[A-Za-z0-9._~+/=-]+', 'Bearer [REDACTED]'
    $text = $text -replace 'sk-[A-Za-z0-9._~+/=-]{8,}', 'sk-[REDACTED]'
    $text = $text -replace '(?i)(authorization|cookie|api[-_]?key|token)\s*[:=]\s*[^,\s}]+', '$1=[REDACTED]'
    $text = $text -replace 'https?://[^\s`"''\)\]\}]+', '[url-redacted]'
    if ($text.Length -gt 1000) {
        return $text.Substring(0, 1000)
    }
    return $text
}

function Invoke-CaptureText {
    param(
        [string]$Command,
        [string[]]$Arguments = @()
    )
    try {
        $value = (& $Command @Arguments 2>$null)
        if ($LASTEXITCODE -ne 0) { return $null }
        return (($value | ForEach-Object { Redact-Text $_ }) -join "`n").Trim()
    } catch {
        return $null
    }
}

if (-not $AuthorizeManualObservation) {
    Fail "Manual routing operational observation recording is disabled by default. Re-run with -AuthorizeManualObservation after completing the named real/manual check."
}

if ($Status -eq "passed" -and $EvidenceIndex.Count -eq 0) {
    Fail "A passed manual observation must include at least one evidence index entry. Record references only; do not copy secrets, raw logs, local databases, or private screenshots into git."
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$resolvedOutputPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    $OutputPath
} else {
    Join-Path $repoRoot $OutputPath
}
$outputDirectory = Split-Path -Parent $resolvedOutputPath
if ($outputDirectory -and -not (Test-Path -LiteralPath $outputDirectory)) {
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
}

$sourceRevision = Invoke-CaptureText "git" @("-C", $repoRoot, "rev-parse", "HEAD")
$dirtyStatus = @(& git -C $repoRoot status --porcelain | ForEach-Object { Redact-Text $_ })

$record = [ordered]@{
    schemaVersion = 1
    kind = "routing-operational-manual-observation"
    scenario = $Scenario
    status = $Status
    authorized = $true
    generatedAt = (Get-Date).ToString("o")
    sourceRevision = $sourceRevision
    worktreeClean = ($dirtyStatus.Count -eq 0)
    dirtyStatus = $dirtyStatus
    observer = Redact-Text $Observer
    evidenceIndex = @($EvidenceIndex | ForEach-Object { Redact-Text $_ })
    notes = Redact-Text $Notes
    boundaries = [ordered]@{
        recordOnly = $true
        copiesEvidenceFiles = $false
        storesRawSecrets = $false
        storesRawProviderUrl = $false
        storesLocalDatabase = $false
        outputPathTracked = $false
    }
    requiredFollowUp = if ($Status -eq "passed") { @() } else { @("Do not treat this scenario as satisfied until a passed record with evidenceIndex exists.") }
}

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($resolvedOutputPath, ($record | ConvertTo-Json -Depth 8), $utf8NoBom)
Write-Host "routing operational manual observation written to $OutputPath"
