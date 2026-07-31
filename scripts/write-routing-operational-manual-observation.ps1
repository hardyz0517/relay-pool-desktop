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

    [string]$IndexPath = "output\routing-operational\qualification\manual-observation\routing-operational-manual-observation-index.json",

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

function Resolve-RepoPath {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return $Path
    }
    return Join-Path $repoRoot $Path
}

function Write-JsonFile {
    param(
        [string]$Path,
        [object]$Value
    )
    $directory = Split-Path -Parent $Path
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 12), $utf8NoBom)
}

if (-not $AuthorizeManualObservation) {
    Fail "Manual routing operational observation recording is disabled by default. Re-run with -AuthorizeManualObservation after completing the named real/manual check."
}

if ($Status -eq "passed" -and $EvidenceIndex.Count -eq 0) {
    Fail "A passed manual observation must include at least one evidence index entry. Record references only; do not copy secrets, raw logs, local databases, or private screenshots into git."
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$resolvedOutputPath = Resolve-RepoPath $OutputPath
$resolvedIndexPath = Resolve-RepoPath $IndexPath
$recordDirectory = Split-Path -Parent $resolvedOutputPath
$recordTimestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssfffZ")
$recordFileName = "routing-operational-manual-observation-$Scenario-$recordTimestamp.json"
$resolvedRecordPath = Join-Path $recordDirectory $recordFileName

$sourceRevision = Invoke-CaptureText "git" @("-C", $repoRoot, "rev-parse", "HEAD")
$dirtyStatus = @(& git -C $repoRoot status --porcelain | ForEach-Object { Redact-Text $_ })

$record = [ordered]@{
    schemaVersion = 1
    kind = "routing-operational-manual-observation"
    scenario = $Scenario
    status = $Status
    authorized = $true
    generatedAt = (Get-Date).ToString("o")
    immutableRecordPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) { $resolvedRecordPath } else { (Join-Path (Split-Path -Parent $OutputPath) $recordFileName) }
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

Write-JsonFile -Path $resolvedRecordPath -Value $record
Write-JsonFile -Path $resolvedOutputPath -Value $record

$existingRecords = @()
if (Test-Path -LiteralPath $resolvedIndexPath -PathType Leaf) {
    try {
        $existing = Get-Content -Raw -LiteralPath $resolvedIndexPath | ConvertFrom-Json
        if ($existing.records) {
            $existingRecords = @($existing.records)
        }
    } catch {
        $existingRecords = @()
    }
}

$scenarioMap = [ordered]@{}
foreach ($entry in $existingRecords) {
    if ($entry.scenario) {
        $scenarioMap[[string]$entry.scenario] = $entry
    }
}
$scenarioMap[$Scenario] = [pscustomobject]@{
    scenario = $Scenario
    status = $Status
    generatedAt = $record.generatedAt
    sourceRevision = $sourceRevision
    immutableRecordPath = $record.immutableRecordPath
    evidenceCount = $EvidenceIndex.Count
}

$index = [ordered]@{
    schemaVersion = 1
    kind = "routing-operational-manual-observation-index"
    generatedAt = (Get-Date).ToString("o")
    latestRecordPath = if ([System.IO.Path]::IsPathRooted($OutputPath)) { $resolvedOutputPath } else { $OutputPath }
    records = @($scenarioMap.Values)
    missingPassedScenarios = @(
        "real_local_client_smoke",
        "real_provider_semantic",
        "ccswitch_fixed_local_entry",
        "windows_sleep_resume",
        "ui_timeline_sqlite_reconciliation",
        "default_v2_no_p0_p1"
    ) | Where-Object {
        -not $scenarioMap.Contains($_) -or $scenarioMap[$_].status -ne "passed"
    }
    boundaries = [ordered]@{
        recordOnly = $true
        copiesEvidenceFiles = $false
        storesRawSecrets = $false
        storesRawProviderUrl = $false
        storesLocalDatabase = $false
        outputPathTracked = $false
    }
}
Write-JsonFile -Path $resolvedIndexPath -Value $index

Write-Host "routing operational manual observation written to $OutputPath"
Write-Host "immutable record written to $($record.immutableRecordPath)"
Write-Host "manual observation index written to $IndexPath"
