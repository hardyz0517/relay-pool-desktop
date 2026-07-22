[CmdletBinding(DefaultParameterSetName = 'Measure')]
param(
    [Parameter(Mandatory = $true, ParameterSetName = 'Measure')]
    [ValidateNotNullOrEmpty()]
    [string] $OutputPath,

    [Parameter(Mandatory = $true, ParameterSetName = 'Validate')]
    [switch] $ValidateInputsOnly,

    [Parameter(ParameterSetName = 'Measure')]
    [string] $CargoTargetDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$releaseTag = 'v0.3.1'
$expectedCommit = '54751559aed8f3f7c159e322bc7bbcc71d993204'
$expectedFixtureSha256 = 'ad1f159cd6feabbb7d9bb4d6a37bf4fbc979f98eab03a42f402eb6fa863f34c9'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$probePath = Join-Path $PSScriptRoot 'persistence-v1-performance-probe.rs'
$fixturePath = Join-Path $repoRoot 'src-tauri\tests\persistence_upgrade\fixtures\profile_001\source.sqlite3'

function Get-Sha256File {
    param([Parameter(Mandatory = $true)][string] $Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-Sha256Text {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string] $Text)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        return ([System.BitConverter]::ToString($sha256.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
}

function Invoke-Git {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]] $Arguments)
    $output = & git -C $repoRoot @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "git failed: $($Arguments -join ' ')"
    }
    return $output
}

function Get-CommandVersion {
    param([Parameter(Mandatory = $true)][string] $Command)
    try {
        $output = & $Command '--version' 2>$null
        if ($LASTEXITCODE -eq 0) { return ($output | Select-Object -First 1).Trim() }
    }
    catch {}
    return 'unavailable'
}

function Get-ControlledEnvironment {
    param([Parameter(Mandatory = $true)][string] $Worktree)
    $processor = Get-CimInstance Win32_Processor | Select-Object -First 1
    $operatingSystem = Get-CimInstance Win32_OperatingSystem | Select-Object -First 1
    $powerScheme = 'unavailable'
    try {
        $powerOutput = & powercfg /getactivescheme 2>$null
        if ($LASTEXITCODE -eq 0 -and $powerOutput) {
            $schemeMatch = [regex]::Match(($powerOutput | Out-String), '[0-9a-fA-F]{8}-(?:[0-9a-fA-F]{4}-){3}[0-9a-fA-F]{12}')
            if ($schemeMatch.Success) {
                $powerScheme = $schemeMatch.Value.ToLowerInvariant()
            }
        }
    }
    catch {}
    $defenderState = 'unavailable'
    try {
        $defender = Get-MpComputerStatus
        $defenderState = if ($defender.RealTimeProtectionEnabled) { 'enabled' } else { 'disabled' }
    }
    catch {}
    $antivirusProducts = @()
    try {
        $antivirusProducts = @(Get-CimInstance -Namespace 'root/SecurityCenter2' -ClassName AntivirusProduct |
            ForEach-Object { [string]$_.displayName } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            Sort-Object -Unique)
    }
    catch {}
    $searchState = 'unavailable'
    try {
        $search = Get-Service -Name WSearch
        $searchState = if ($search.Status -eq 'Running') { 'running' } else { 'stopped' }
    }
    catch {}
    $gitHead = (& git -C $Worktree rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'unable to resolve the detached V1 worktree HEAD' }
    $gitStatus = ((& git -C $Worktree status --porcelain=v1 --untracked-files=all) -join "`n")
    if ($LASTEXITCODE -ne 0) { throw 'unable to capture the detached V1 worktree status' }
    $trackedDiff = ((& git -C $Worktree diff --binary HEAD --no-ext-diff) -join "`n")
    if ($LASTEXITCODE -ne 0) { throw 'unable to capture the detached V1 tracked diff' }
    $untrackedPaths = @(& git -C $Worktree ls-files --others --exclude-standard | Sort-Object)
    if ($LASTEXITCODE -ne 0) { throw 'unable to enumerate detached V1 untracked files' }
    $untrackedManifest = @($untrackedPaths | ForEach-Object {
        $absolute = Join-Path $Worktree $_
        "$_`0$(Get-Sha256File $absolute)"
    }) -join "`n"
    $snapshotKind = if ([string]::IsNullOrWhiteSpace($gitStatus)) { 'clean-commit' } else { 'hashed-dirty-worktree' }
    return [ordered]@{
        cpuModel = ([string]$processor.Name).Trim()
        logicalProcessors = [int]$processor.NumberOfLogicalProcessors
        installedMemoryBytes = [int64]$operatingSystem.TotalVisibleMemorySize * 1024
        windowsCaption = [string]$operatingSystem.Caption
        windowsVersion = [string]$operatingSystem.Version
        windowsBuild = [string]$operatingSystem.BuildNumber
        activePowerScheme = $powerScheme
        rustcVersion = Get-CommandVersion 'rustc'
        cargoVersion = Get-CommandVersion 'cargo'
        gitHead = $gitHead
        worktreeDirty = -not [string]::IsNullOrWhiteSpace($gitStatus)
        worktreeStatusSha256 = Get-Sha256Text $gitStatus
        worktreeSnapshot = [ordered]@{
            kind = $snapshotKind
            trackedDiffSha256 = Get-Sha256Text $trackedDiff
            untrackedContentSha256 = Get-Sha256Text $untrackedManifest
            untrackedFileCount = $untrackedPaths.Count
        }
        antivirusProducts = $antivirusProducts
        defenderRealTimeProtection = $defenderState
        windowsSearchService = $searchState
    }
}

if (-not (Test-Path -LiteralPath $probePath -PathType Leaf)) {
    throw "missing V1 benchmark probe: $probePath"
}
if (-not (Test-Path -LiteralPath $fixturePath -PathType Leaf)) {
    throw "missing reviewed v0.3.1 fixture: $fixturePath"
}
$releaseCommit = ((Invoke-Git rev-parse "$releaseTag^{commit}") | Out-String).Trim()
if ($releaseCommit -ne $expectedCommit) {
    throw "$releaseTag resolved to $releaseCommit instead of $expectedCommit"
}
$fixtureSha256 = Get-Sha256File $fixturePath
if ($fixtureSha256 -ne $expectedFixtureSha256) {
    throw "reviewed v0.3.1 fixture SHA-256 changed: $fixtureSha256"
}
$probeSha256 = Get-Sha256File $probePath

$inputValidation = [ordered]@{
    status = 'inputs-validated-not-measured'
    baselineKind = 'reconstructed-v0.3.1-source-baseline'
    releaseCommit = $releaseCommit
    releasedFixtureSha256 = $fixtureSha256
    benchmarkProbeSha256 = $probeSha256
    standardFixture = [ordered]@{
        stations = 100
        stationKeys = 1000
        requestLogs = 10000
        changeEvents = 100000
    }
    workloads = [ordered]@{
        requestLogs = [ordered]@{
            rows = 500
            projection = 'v0.3.1-production-full-row-representative-economics-attempt-model'
        }
        changeEvents = [ordered]@{
            queryLimit = 201
            returnedRows = 200
            projection = 'v0.3.1-production-full-row-representative-associated-fields'
            contract = 'normalized-first-page-not-v0.3.1-public-api'
        }
        startup = [ordered]@{ migrationsIncluded = $false }
    }
}
if ($ValidateInputsOnly) {
    Write-Output ($inputValidation | ConvertTo-Json -Depth 10 -Compress)
    exit 0
}

$resolvedOutput = if ([System.IO.Path]::IsPathRooted($OutputPath)) {
    [System.IO.Path]::GetFullPath($OutputPath)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputPath))
}
$systemTempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$tempRoot = [System.IO.Path]::GetFullPath((Join-Path $systemTempRoot "relay-pool-v031-performance-$PID"))
if (-not $tempRoot.StartsWith($systemTempRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
    $tempRoot -eq $systemTempRoot) {
    throw "refusing unsafe temporary baseline root: $tempRoot"
}
$worktree = Join-Path $tempRoot 'worktree'
$probeOutput = Join-Path $tempRoot 'reconstructed-v0.3.1-source-baseline.json'
$targetDir = if ([string]::IsNullOrWhiteSpace($CargoTargetDir)) {
    Join-Path $tempRoot 'cargo-target'
} elseif ([System.IO.Path]::IsPathRooted($CargoTargetDir)) {
    [System.IO.Path]::GetFullPath($CargoTargetDir)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $CargoTargetDir))
}

New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
$worktreeCreated = $false
$previousTarget = $env:CARGO_TARGET_DIR
try {
    Invoke-Git worktree add --detach $worktree $releaseTag | Out-Null
    $worktreeCreated = $true
    $databaseSource = Join-Path $worktree 'src-tauri\src\services\database.rs'
    if (-not (Test-Path -LiteralPath $databaseSource -PathType Leaf)) {
        throw "$releaseTag does not contain the V1 AppDatabase source"
    }
    Get-Content -Raw -LiteralPath $probePath |
        Add-Content -LiteralPath $databaseSource -Encoding utf8
    $databaseSourceSha256 = Get-Sha256File $databaseSource

    $env:CARGO_TARGET_DIR = $targetDir
    $env:PERSISTENCE_V1_RELEASED_FIXTURE = $fixturePath
    $env:PERSISTENCE_V1_BASELINE_OUTPUT = $probeOutput
    $env:PERSISTENCE_V1_BENCHMARK_PROBE_SHA256 = $probeSha256
    $baselineEnvironment = Get-ControlledEnvironment $worktree
    $env:PERSISTENCE_V1_ENVIRONMENT_JSON = $baselineEnvironment | ConvertTo-Json -Depth 10 -Compress
    $measurementStartedAtUtc = (Get-Date).ToUniversalTime().ToString('o')

    & cargo test --release --locked --manifest-path (Join-Path $worktree 'src-tauri\Cargo.toml') --lib `
        'persistence_v1_performance_probe::reconstructed_v031_baseline' -- --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) {
        throw "reconstructed v0.3.1 source baseline probe failed with exit code $LASTEXITCODE"
    }
    $postBaselineEnvironment = Get-ControlledEnvironment $worktree
    if ((Get-Sha256File $databaseSource) -ne $databaseSourceSha256) {
        throw 'V1 performance probe source changed during baseline measurement'
    }
    $allowedBuildMutations = @(
        'src-tauri/gen/schemas/desktop-schema.json',
        'src-tauri/gen/schemas/windows-schema.json',
        'src-tauri/src/services/database.rs'
    )
    $changedPaths = @(& git -C $worktree diff --name-only HEAD)
    if ($LASTEXITCODE -ne 0) { throw 'unable to inspect detached V1 worktree after baseline measurement' }
    $unexpectedBuildMutations = @($changedPaths | Where-Object { $_ -notin $allowedBuildMutations })
    if ($unexpectedBuildMutations.Count -gt 0) {
        throw "V1 baseline build changed unexpected tracked paths: $($unexpectedBuildMutations -join ', ')"
    }
    foreach ($field in @(
        'cpuModel', 'logicalProcessors', 'installedMemoryBytes', 'windowsCaption',
        'windowsVersion', 'windowsBuild', 'activePowerScheme',
        'defenderRealTimeProtection', 'windowsSearchService'
    )) {
        if ([string]$baselineEnvironment[$field] -cne [string]$postBaselineEnvironment[$field]) {
            throw "V1 controlled environment changed during baseline measurement for $field"
        }
    }
    if (-not (Test-Path -LiteralPath $probeOutput -PathType Leaf)) {
        throw 'V1 probe completed without producing its baseline JSON'
    }
    # Cargo emits the probe report as UTF-8 without a BOM; Windows PowerShell 5.1
    # otherwise decodes non-ASCII environment metadata with the active code page.
    $baseline = [System.IO.File]::ReadAllText($probeOutput, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
    if ($baseline.baselineKind -ne 'reconstructed-v0.3.1-source-baseline') {
        throw 'V1 probe output lost its reconstructed provenance label'
    }
    if ($baseline.provenance.releaseCommit -ne $expectedCommit) {
        throw 'V1 probe output release commit mismatch'
    }
    $measurementCompletedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    $baseline | Add-Member -NotePropertyName measurement -NotePropertyValue ([ordered]@{
        startedAtUtc = $measurementStartedAtUtc
        completedAtUtc = $measurementCompletedAtUtc
        sequenceRole = 'first-reconstructed-v1'
    })
    $outputDirectory = Split-Path -Parent $resolvedOutput
    if (-not [string]::IsNullOrWhiteSpace($outputDirectory)) {
        New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
    }
    [System.IO.File]::WriteAllText(
        $resolvedOutput,
        (($baseline | ConvertTo-Json -Depth 100) + "`n"),
        [System.Text.UTF8Encoding]::new($false)
    )
    Write-Output $resolvedOutput
}
finally {
    foreach ($name in @(
        'PERSISTENCE_V1_RELEASED_FIXTURE',
        'PERSISTENCE_V1_BASELINE_OUTPUT',
        'PERSISTENCE_V1_BENCHMARK_PROBE_SHA256',
        'PERSISTENCE_V1_ENVIRONMENT_JSON'
    )) {
        Remove-Item "Env:\$name" -ErrorAction SilentlyContinue
    }
    if ($null -eq $previousTarget) {
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousTarget
    }
    if ($worktreeCreated) {
        & git -C $repoRoot worktree remove --force $worktree 2>$null
    }
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
