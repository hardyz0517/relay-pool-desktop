[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string] $BaselinePath,

    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string] $OutputPath,

    [string] $MockV2QualificationPath,

    [string] $CargoTargetDir
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path

function Resolve-RepoPath {
    param([Parameter(Mandatory = $true)][string] $Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Get-Sha256File {
    param([Parameter(Mandatory = $true)][string] $Path)
    $stream = [System.IO.File]::OpenRead($Path)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        return ([System.BitConverter]::ToString($sha256.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
        $stream.Dispose()
    }
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

function Assert-HexSha256 {
    param([Parameter(Mandatory = $true)][string] $Name, [Parameter(Mandatory = $true)][object] $Value)
    if ([string]$Value -notmatch '^[a-f0-9]{64}$') {
        throw "$Name must be a lowercase SHA-256 value"
    }
}

function Assert-ReconstructedBaseline {
    param([Parameter(Mandatory = $true)][pscustomobject] $Baseline)
    if ($Baseline.schemaVersion -ne 1) {
        throw 'baseline schemaVersion must be 1'
    }
    if ($Baseline.baselineKind -ne 'reconstructed-v0.3.1-source-baseline') {
        throw 'baselineKind must explicitly be reconstructed-v0.3.1-source-baseline'
    }
    if ($Baseline.provenance.releaseCommit -ne '54751559aed8f3f7c159e322bc7bbcc71d993204') {
        throw 'baseline release commit does not match the immutable v0.3.1 tag commit'
    }
    if ($Baseline.provenance.releasedFixtureSha256 -ne 'ad1f159cd6feabbb7d9bb4d6a37bf4fbc979f98eab03a42f402eb6fa863f34c9') {
        throw 'baseline released fixture does not match the reviewed v0.3.1 fixture'
    }
    Assert-HexSha256 'derivedFixtureSha256' $Baseline.provenance.derivedFixtureSha256
    Assert-HexSha256 'benchmarkProbeSha256' $Baseline.provenance.benchmarkProbeSha256
    if ($null -eq $Baseline.PSObject.Properties['build'] -or
        $Baseline.build.profile -ne 'release' -or
        $Baseline.build.locked -ne $true) {
        throw 'baseline must come from a release-profile cargo --locked build'
    }
    if ($null -eq $Baseline.PSObject.Properties['environment']) {
        throw 'baseline must retain its controlled-machine environment'
    }
    foreach ($field in @(
        'cpuModel',
        'logicalProcessors',
        'installedMemoryBytes',
        'windowsCaption',
        'windowsVersion',
        'windowsBuild',
        'activePowerScheme',
        'rustcVersion',
        'cargoVersion',
        'gitHead',
        'worktreeDirty',
        'worktreeStatusSha256',
        'antivirusProducts',
        'defenderRealTimeProtection',
        'windowsSearchService'
    )) {
        if ($null -eq $Baseline.environment.PSObject.Properties[$field]) {
            throw "baseline environment is missing $field"
        }
    }
    if ($Baseline.environment.gitHead -ne '54751559aed8f3f7c159e322bc7bbcc71d993204') {
        throw 'baseline environment Git HEAD does not match v0.3.1'
    }
    Assert-HexSha256 'baseline worktreeStatusSha256' $Baseline.environment.worktreeStatusSha256
    if ($null -eq $Baseline.environment.PSObject.Properties['worktreeSnapshot'] -or
        $Baseline.environment.worktreeSnapshot.kind -notin @('clean-commit', 'hashed-dirty-worktree')) {
        throw 'baseline environment must retain a clean commit or stable dirty-worktree content hashes'
    }
    if ($Baseline.environment.worktreeSnapshot.kind -eq 'hashed-dirty-worktree') {
        Assert-HexSha256 'baseline trackedDiffSha256' $Baseline.environment.worktreeSnapshot.trackedDiffSha256
        Assert-HexSha256 'baseline untrackedContentSha256' $Baseline.environment.worktreeSnapshot.untrackedContentSha256
    }
    if ($null -eq $Baseline.PSObject.Properties['measurement'] -or
        [string]::IsNullOrWhiteSpace([string]$Baseline.measurement.startedAtUtc) -or
        [string]::IsNullOrWhiteSpace([string]$Baseline.measurement.completedAtUtc)) {
        throw 'baseline must retain ordered measurement start/completion timestamps'
    }
    $baselineStarted = [DateTimeOffset]::Parse([string]$Baseline.measurement.startedAtUtc)
    $baselineCompleted = [DateTimeOffset]::Parse([string]$Baseline.measurement.completedAtUtc)
    if ($baselineCompleted -lt $baselineStarted) {
        throw 'baseline measurement completion precedes its start'
    }
    if ($null -eq $Baseline.PSObject.Properties['standardFixture'] -or
        $Baseline.standardFixture.stations -ne 100 -or
        $Baseline.standardFixture.stationKeys -ne 1000 -or
        $Baseline.standardFixture.requestLogs -ne 10000 -or
        $Baseline.standardFixture.changeEvents -ne 100000) {
        throw 'baseline must retain the 100/1,000/10,000/100,000 standard fixture contract'
    }
    if ($null -eq $Baseline.PSObject.Properties['workloads'] -or
        $Baseline.workloads.requestLogs.rows -ne 500 -or
        $Baseline.workloads.requestLogs.projection -ne 'v0.3.1-production-full-row-representative-economics-attempt-model' -or
        $Baseline.workloads.changeEvents.queryLimit -ne 201 -or
        $Baseline.workloads.changeEvents.returnedRows -ne 200 -or
        $Baseline.workloads.changeEvents.projection -ne 'v0.3.1-production-full-row-representative-associated-fields' -or
        $Baseline.workloads.changeEvents.contract -ne 'normalized-first-page-not-v0.3.1-public-api' -or
        $Baseline.workloads.startup.migrationsIncluded -ne $false) {
        throw 'baseline workload must remain 500 full request logs, normalized LIMIT 201 to 200 change events, and no-migration startup'
    }
    $expectedSampleCounts = [ordered]@{
        hotRequestLogs = 40
        hotChangeEventsFirstPage = 40
        startupWithoutMigration = 15
    }
    foreach ($metric in $expectedSampleCounts.Keys) {
        $property = $Baseline.metrics.PSObject.Properties[$metric]
        $samples = if ($null -eq $property) { @() } else { @($property.Value.samplesNs) }
        if ($samples.Count -ne $expectedSampleCounts[$metric]) {
            throw "baseline metric $metric must retain exactly $($expectedSampleCounts[$metric]) raw nanosecond samples"
        }
        if (@($samples | Where-Object { $_ -isnot [ValueType] -or [decimal]$_ -le 0 }).Count -ne 0) {
            throw "baseline metric $metric contains an invalid raw nanosecond sample"
        }
    }
}

function Get-PercentileNanoseconds {
    param(
        [Parameter(Mandatory = $true)][object[]] $Samples,
        [Parameter(Mandatory = $true)][ValidateSet('median', 'p95')][string] $Quantile
    )
    if ($Samples.Count -eq 0) {
        throw "cannot calculate $Quantile from an empty sample set"
    }
    $sorted = @($Samples | ForEach-Object { [decimal]$_ } | Sort-Object)
    $fraction = if ($Quantile -eq 'median') { [decimal]0.50 } else { [decimal]0.95 }
    $rank = [Math]::Ceiling([decimal]$sorted.Count * $fraction) - 1
    return [decimal]$sorted[[Math]::Max(0, [int]$rank)]
}

function New-RelativeGate {
    param(
        [Parameter(Mandatory = $true)][string] $Name,
        [Parameter(Mandatory = $true)][object[]] $BaselineSamples,
        [Parameter(Mandatory = $true)][object[]] $CurrentSamples,
        [Parameter(Mandatory = $true)][ValidateSet('median', 'p95')][string] $Quantile
    )
    $baselineNs = Get-PercentileNanoseconds $BaselineSamples $Quantile
    $currentNs = Get-PercentileNanoseconds $CurrentSamples $Quantile
    $passed = ($currentNs * [decimal]10) -le ($baselineNs * [decimal]11)
    $gate = [ordered]@{
        quantile = $Quantile
        relativeLimit = 'V2 <= reconstructed V1 * 1.10'
        baselineNs = $baselineNs
        currentNs = $currentNs
        maximumAllowedNs = [Math]::Floor($baselineNs * [decimal]1.10)
        passed = $passed
    }
    if (-not $passed) {
        throw "relative performance gate failed for ${Name}: V2 $Quantile $currentNs ns exceeds reconstructed V1 $baselineNs ns by more than 10%"
    }
    return $gate
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

    $gitHead = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'unable to resolve the V2 Git HEAD' }
    $gitStatus = ((& git -C $repoRoot status --porcelain=v1 --untracked-files=all) -join "`n")
    if ($LASTEXITCODE -ne 0) { throw 'unable to capture the V2 worktree status' }

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
        antivirusProducts = $antivirusProducts
        defenderRealTimeProtection = $defenderState
        windowsSearchService = $searchState
    }
}

function Get-StableWorktreeSnapshot {
    $gitHead = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'unable to resolve the V2 Git HEAD for worktree snapshot' }
    $status = ((& git -C $repoRoot status --porcelain=v1 --untracked-files=all) -join "`n")
    if ($LASTEXITCODE -ne 0) { throw 'unable to capture the V2 worktree status for snapshot' }
    if ([string]::IsNullOrWhiteSpace($status)) {
        return [ordered]@{
            kind = 'clean-commit'
            gitHead = $gitHead
        }
    }

    $trackedDiff = ((& git -C $repoRoot diff --binary HEAD --no-ext-diff) -join "`n")
    if ($LASTEXITCODE -ne 0) { throw 'unable to capture the V2 tracked diff snapshot' }
    $untrackedPaths = @(& git -C $repoRoot ls-files --others --exclude-standard | Sort-Object)
    if ($LASTEXITCODE -ne 0) { throw 'unable to enumerate V2 untracked files' }
    $untrackedManifest = @($untrackedPaths | ForEach-Object {
        $absolute = Join-Path $repoRoot $_
        if (-not (Test-Path -LiteralPath $absolute -PathType Leaf)) {
            throw "untracked snapshot entry is not a file: $_"
        }
        "$_`0$(Get-Sha256File $absolute)"
    }) -join "`n"
    return [ordered]@{
        kind = 'hashed-dirty-worktree'
        gitHead = $gitHead
        trackedDiffSha256 = Get-Sha256Text $trackedDiff
        untrackedContentSha256 = Get-Sha256Text $untrackedManifest
        untrackedFileCount = $untrackedPaths.Count
    }
}

function Assert-V2ReleaseReport {
    param(
        [Parameter(Mandatory = $true)][pscustomobject] $Standard,
        [Parameter(Mandatory = $true)][pscustomobject] $Startup
    )
    if ($Standard.schemaVersion -ne 1 -or $Startup.schemaVersion -ne 1) {
        throw 'V2 reports must use schemaVersion 1'
    }
    if ($Standard.provenance.build.profile -ne 'release' -or
        $Standard.provenance.build.locked -ne $true -or
        $Standard.environment.debugAssertions -ne $false -or
        $Startup.provenance.build.profile -ne 'release' -or
        $Startup.provenance.build.locked -ne $true -or
        $Startup.environment.debugAssertions -ne $false) {
        throw 'V2 evidence must come from cargo test --release --locked with debug assertions disabled'
    }
    if ([string]$Standard.provenance.v2Commit -notmatch '^[a-f0-9]{40}$') {
        throw 'V2 report must retain the exact source commit'
    }
    if ($Startup.provenance.v2Commit -ne $Standard.provenance.v2Commit -or
        $Standard.environment.gitHead -ne $Standard.provenance.v2Commit) {
        throw 'V2 suites and captured environment must identify the same exact source commit'
    }
    $snapshot = $Standard.provenance.worktreeSnapshot
    if ($snapshot.kind -notin @('clean-commit', 'hashed-dirty-worktree')) {
        throw 'V2 report must retain a clean commit or stable dirty-worktree content hashes'
    }
    if ($snapshot.kind -eq 'hashed-dirty-worktree') {
        Assert-HexSha256 'V2 trackedDiffSha256' $snapshot.trackedDiffSha256
        Assert-HexSha256 'V2 untrackedContentSha256' $snapshot.untrackedContentSha256
    }
    foreach ($field in @(
        'cpuModel', 'logicalProcessors', 'installedMemoryBytes', 'windowsCaption',
        'windowsVersion', 'windowsBuild', 'activePowerScheme', 'rustcVersion',
        'cargoVersion', 'gitHead', 'worktreeDirty', 'worktreeStatusSha256',
        'antivirusProducts', 'defenderRealTimeProtection', 'windowsSearchService'
    )) {
        if ($null -eq $Standard.environment.PSObject.Properties[$field]) {
            throw "V2 environment is missing $field"
        }
    }
    if ($Standard.fixture.stations -ne 100 -or
        $Standard.fixture.stationKeys -ne 1000 -or
        $Standard.fixture.requestLogs -ne 10000 -or
        $Standard.fixture.changeEvents -ne 100000 -or
        $Standard.workloads.requestLogs.rows -ne 500 -or
        $Standard.workloads.requestLogs.projection -ne 'production-request-log-service-full-row-projection' -or
        $Standard.workloads.changeEvents.queryLimit -ne 201 -or
        $Standard.workloads.changeEvents.returnedRows -ne 200 -or
        $Standard.workloads.changeEvents.projection -ne 'production-change-service-first-page-projection' -or
        $Standard.workloads.startup.migrationsIncluded -ne $false -or
        $Startup.workloads.startup.migrationsIncluded -ne $false) {
        throw 'V2 report lost the approved fixture or 500/201-to-200/no-migration workload contract'
    }
    if ($Standard.memory.metric -ne 'PROCESS_MEMORY_COUNTERS_EX.PrivateUsage' -or
        $Standard.memory.sampleIntervalMs -ne 10 -or
        [decimal]$Standard.memory.sampleCount -le 0 -or
        [decimal]$Standard.memory.peakPrivateUsageBytes -lt [decimal]$Standard.memory.baselinePrivateUsageBytes -or
        [decimal]$Standard.memory.peakPrivateUsageDeltaBytes -ne
            ([decimal]$Standard.memory.peakPrivateUsageBytes - [decimal]$Standard.memory.baselinePrivateUsageBytes) -or
        [decimal]$Standard.memory.peakPrivateUsageDeltaBytes -gt 268435456 -or
        [decimal]$Standard.memory.limitBytes -ne 268435456) {
        throw 'V2 PrivateUsage observation is missing, incoherent, or exceeds 256 MiB'
    }
    $queue = $Standard.queues.writeCoordinator
    if ($Standard.queues.qualificationScope -ne 'write-coordinator-only-not-all-queues' -or
        $queue.currentDepth -ne 0 -or
        [decimal]$queue.peakDepth -lt [decimal]$queue.currentDepth -or
        [decimal]$queue.acquiredWrites -le 0 -or
        [decimal]$queue.acquiredWrites -ne ([decimal]$queue.committedWrites + [decimal]$queue.rolledBackWrites)) {
        throw 'V2 write coordinator queue did not terminate coherently or scope was overstated'
    }
    if ($Standard.queues.finalizationService.coverage -ne 'production-request-finalization-service-terminal-transition' -or
        $null -ne $Standard.queues.finalizationService.snapshot) {
        throw 'V2 finalization-service queue scope was missing or overstated'
    }
}

function Assert-SameControlledMachine {
    param(
        [Parameter(Mandatory = $true)][pscustomobject] $BaselineEnvironment,
        [Parameter(Mandatory = $true)][pscustomobject] $V2Environment
    )
    foreach ($field in @(
        'cpuModel', 'logicalProcessors', 'installedMemoryBytes', 'windowsCaption',
        'windowsVersion', 'windowsBuild', 'activePowerScheme',
        'defenderRealTimeProtection', 'windowsSearchService'
    )) {
        if ([string]$BaselineEnvironment.$field -cne [string]$V2Environment.$field) {
            throw "V1/V2 controlled-machine mismatch for $field"
        }
    }
}

function Invoke-CapturedCargoQualification {
    param(
        [Parameter(Mandatory = $true)][string] $ManifestPath
    )
    if ($ManifestPath.Contains('"')) {
        throw 'Cargo manifest path cannot contain a quote'
    }

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = 'cargo'
    $startInfo.Arguments = "test --release --locked --manifest-path `"$ManifestPath`" --lib persistence::performance_tests -- --nocapture --test-threads=1 --skip mock_wrapper_contract_emits_reports_from_rust_builders"
    $startInfo.WorkingDirectory = $repoRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) {
            throw 'unable to start Cargo performance qualification'
        }
        $standardOutputTask = $process.StandardOutput.ReadToEndAsync()
        $standardErrorTask = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $standardOutput = $standardOutputTask.GetAwaiter().GetResult()
        $standardError = $standardErrorTask.GetAwaiter().GetResult()
        return [ordered]@{
            exitCode = $process.ExitCode
            standardOutput = $standardOutput
            standardError = $standardError
        }
    }
    finally {
        $process.Dispose()
    }
}

function Invoke-V2Qualification {
    if (-not [string]::IsNullOrWhiteSpace($MockV2QualificationPath)) {
        $mockPath = Resolve-RepoPath $MockV2QualificationPath
        if (-not (Test-Path -LiteralPath $mockPath -PathType Leaf)) {
            throw "mock V2 qualification output does not exist: $mockPath"
        }
        return [ordered]@{
            sourcePath = $mockPath
            rawOutput = [System.IO.File]::ReadAllText($mockPath, [System.Text.Encoding]::UTF8)
            sha256 = Get-Sha256File $mockPath
            isMock = $true
            measurementStartedAtUtc = $null
            measurementCompletedAtUtc = $null
        }
    }

    $targetDir = if ([string]::IsNullOrWhiteSpace($CargoTargetDir)) {
        Join-Path ([System.IO.Path]::GetTempPath()) 'relay-pool-persistence-v2-performance-target'
    } else {
        Resolve-RepoPath $CargoTargetDir
    }
    $previousTarget = $env:CARGO_TARGET_DIR
    $previousEnvironment = $env:PERSISTENCE_QUALIFICATION_ENVIRONMENT_JSON
    $previousProvenance = $env:PERSISTENCE_QUALIFICATION_PROVENANCE_JSON
    $captureEnvironment = Get-ControlledEnvironment
    $worktreeSnapshot = Get-StableWorktreeSnapshot
    $measurementStartedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
    $captureEnvironment['debugAssertions'] = $false
    $captureProvenance = [ordered]@{
        v2Commit = $captureEnvironment.gitHead
        build = [ordered]@{ profile = 'release'; locked = $true }
        worktreeSnapshot = $worktreeSnapshot
        measurementStartedAtUtc = $measurementStartedAtUtc
    }
    $env:CARGO_TARGET_DIR = $targetDir
    $env:PERSISTENCE_QUALIFICATION_ENVIRONMENT_JSON = $captureEnvironment | ConvertTo-Json -Depth 20 -Compress
    $env:PERSISTENCE_QUALIFICATION_PROVENANCE_JSON = $captureProvenance | ConvertTo-Json -Depth 20 -Compress
    try {
        $captured = Invoke-CapturedCargoQualification `
            -ManifestPath (Join-Path $repoRoot 'src-tauri\Cargo.toml')
        $rawOutput = $captured.standardOutput
        if (-not [string]::IsNullOrEmpty($captured.standardError)) {
            $rawOutput += "`n" + $captured.standardError
        }
        if ($captured.exitCode -ne 0) {
            throw "V2 performance qualification failed with exit code $($captured.exitCode)`n$rawOutput"
        }
        $postWorktreeSnapshot = Get-StableWorktreeSnapshot
        $beforeSnapshotJson = $worktreeSnapshot | ConvertTo-Json -Depth 20 -Compress
        $afterSnapshotJson = $postWorktreeSnapshot | ConvertTo-Json -Depth 20 -Compress
        if ($beforeSnapshotJson -cne $afterSnapshotJson) {
            throw 'V2 worktree content changed during the qualification run'
        }
        $postEnvironment = Get-ControlledEnvironment
        foreach ($field in @(
            'cpuModel', 'logicalProcessors', 'installedMemoryBytes', 'windowsCaption',
            'windowsVersion', 'windowsBuild', 'activePowerScheme',
            'defenderRealTimeProtection', 'windowsSearchService', 'worktreeStatusSha256'
        )) {
            if ([string]$captureEnvironment[$field] -cne [string]$postEnvironment[$field]) {
                throw "V2 controlled environment changed during qualification for $field"
            }
        }
        $measurementCompletedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
        return [ordered]@{
            sourcePath = $null
            rawOutput = $rawOutput
            sha256 = Get-Sha256Text $rawOutput
            isMock = $false
            environment = $captureEnvironment
            provenance = $captureProvenance
            measurementStartedAtUtc = $measurementStartedAtUtc
            measurementCompletedAtUtc = $measurementCompletedAtUtc
        }
    }
    finally {
        if ($null -eq $previousTarget) {
            Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousTarget
        }
        if ($null -eq $previousEnvironment) {
            Remove-Item Env:\PERSISTENCE_QUALIFICATION_ENVIRONMENT_JSON -ErrorAction SilentlyContinue
        } else {
            $env:PERSISTENCE_QUALIFICATION_ENVIRONMENT_JSON = $previousEnvironment
        }
        if ($null -eq $previousProvenance) {
            Remove-Item Env:\PERSISTENCE_QUALIFICATION_PROVENANCE_JSON -ErrorAction SilentlyContinue
        } else {
            $env:PERSISTENCE_QUALIFICATION_PROVENANCE_JSON = $previousProvenance
        }
    }
}

$resolvedBaseline = Resolve-RepoPath $BaselinePath
$resolvedOutput = Resolve-RepoPath $OutputPath
if (-not (Test-Path -LiteralPath $resolvedBaseline -PathType Leaf)) {
    throw "baseline file does not exist: $resolvedBaseline"
}
$baseline = [System.IO.File]::ReadAllText($resolvedBaseline, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
Assert-ReconstructedBaseline $baseline

$v2Capture = Invoke-V2Qualification
$qualificationMarker = 'PERSISTENCE_QUALIFICATION '
$rawQualificationLines = @($v2Capture.rawOutput -split "`r?`n" | ForEach-Object {
    $markerIndex = $_.IndexOf($qualificationMarker, [System.StringComparison]::Ordinal)
    if ($markerIndex -ge 0) {
        $_.Substring($markerIndex)
    }
})
if ($rawQualificationLines.Count -eq 0) {
    throw 'V2 output did not contain a PERSISTENCE_QUALIFICATION JSON line'
}
$v2Reports = @($rawQualificationLines | ForEach-Object {
    $_.Substring('PERSISTENCE_QUALIFICATION '.Length) | ConvertFrom-Json
})
$v2Standard = @($v2Reports | Where-Object { $_.suite -eq 'standard' })
if ($v2Standard.Count -ne 1) {
    throw "V2 output must contain exactly one standard suite report; found $($v2Standard.Count)"
}
$v2Startup = @($v2Reports | Where-Object { $_.suite -eq 'startup-and-concurrent-finalization' })
if ($v2Startup.Count -ne 1) {
    throw "V2 output must contain exactly one startup-and-concurrent-finalization suite report; found $($v2Startup.Count)"
}
Assert-V2ReleaseReport -Standard $v2Standard[0] -Startup $v2Startup[0]
if (-not $v2Capture.isMock) {
    foreach ($field in @(
        'cpuModel', 'logicalProcessors', 'installedMemoryBytes', 'windowsCaption',
        'windowsVersion', 'windowsBuild', 'activePowerScheme', 'rustcVersion',
        'cargoVersion', 'gitHead', 'worktreeDirty', 'worktreeStatusSha256',
        'defenderRealTimeProtection', 'windowsSearchService', 'debugAssertions'
    )) {
        if ([string]$v2Standard[0].environment.$field -cne [string]$v2Capture.environment[$field]) {
            throw "V2 report environment does not match the wrapper capture for $field"
        }
    }
    $reportAntivirusJson = @($v2Standard[0].environment.antivirusProducts) | ConvertTo-Json -Compress
    $captureAntivirusJson = @($v2Capture.environment['antivirusProducts']) | ConvertTo-Json -Compress
    if ($reportAntivirusJson -cne $captureAntivirusJson) {
        throw 'V2 report antivirus products do not match the wrapper capture'
    }
    $reportSnapshot = $v2Standard[0].provenance.worktreeSnapshot
    $captureSnapshot = $v2Capture.provenance.worktreeSnapshot
    if ($v2Standard[0].provenance.v2Commit -ne $v2Capture.provenance.v2Commit -or
        $v2Standard[0].provenance.build.profile -ne $v2Capture.provenance.build.profile -or
        $v2Standard[0].provenance.build.locked -ne $v2Capture.provenance.build.locked) {
        throw 'V2 report provenance does not match the wrapper-captured release snapshot'
    }
    foreach ($field in @('kind', 'gitHead')) {
        if ([string]$reportSnapshot.$field -cne [string]$captureSnapshot.$field) {
            throw "V2 report worktree snapshot does not match the wrapper capture for $field"
        }
    }
    if ($captureSnapshot.kind -eq 'hashed-dirty-worktree') {
        foreach ($field in @('trackedDiffSha256', 'untrackedContentSha256', 'untrackedFileCount')) {
            if ([string]$reportSnapshot.$field -cne [string]$captureSnapshot.$field) {
                throw "V2 report worktree snapshot does not match the wrapper capture for $field"
            }
        }
    }
}
Assert-SameControlledMachine -BaselineEnvironment $baseline.environment -V2Environment $v2Standard[0].environment

$v2MeasurementStartedAtUtc = if ($v2Capture.isMock) {
    [string]$v2Standard[0].provenance.measurementStartedAtUtc
} else {
    [string]$v2Capture.measurementStartedAtUtc
}
$v2MeasurementCompletedAtUtc = if ($v2Capture.isMock) {
    [string]$v2Standard[0].provenance.measurementCompletedAtUtc
} else {
    [string]$v2Capture.measurementCompletedAtUtc
}
if ([string]::IsNullOrWhiteSpace($v2MeasurementStartedAtUtc) -or
    [string]::IsNullOrWhiteSpace($v2MeasurementCompletedAtUtc)) {
    throw 'V2 capture must retain ordered measurement start/completion timestamps'
}
$baselineCompletedAt = [DateTimeOffset]::Parse([string]$baseline.measurement.completedAtUtc)
$v2StartedAt = [DateTimeOffset]::Parse($v2MeasurementStartedAtUtc)
$v2CompletedAt = [DateTimeOffset]::Parse($v2MeasurementCompletedAtUtc)
if ($v2CompletedAt -lt $v2StartedAt -or $v2StartedAt -lt $baselineCompletedAt) {
    throw 'paired measurement order must be reconstructed V1 first, then V2 on the same machine'
}

$v2MetricContract = [ordered]@{
    hotRequestLogs = [ordered]@{
        baseline = $baseline.metrics.hotRequestLogs
        current = $v2Standard[0].metrics.hotRequestLogs
        samples = 40
        quantile = 'p95'
    }
    hotChangeEventsFirstPage = [ordered]@{
        baseline = $baseline.metrics.hotChangeEventsFirstPage
        current = $v2Standard[0].metrics.hotChangeEventsFirstPage
        samples = 40
        quantile = 'p95'
    }
    startupWithoutMigration = [ordered]@{
        baseline = $baseline.metrics.startupWithoutMigration
        current = $v2Startup[0].metrics.startupWithoutMigration
        samples = 15
        quantile = 'median'
    }
}
$relativeGates = [ordered]@{}
foreach ($metricName in $v2MetricContract.Keys) {
    $contract = $v2MetricContract[$metricName]
    $baselineSamples = if ($null -eq $contract.baseline) { @() } else { @($contract.baseline.samplesNs) }
    $currentSamples = if ($null -eq $contract.current) { @() } else { @($contract.current.samplesNs) }
    if ($baselineSamples.Count -ne $contract.samples -or $currentSamples.Count -ne $contract.samples) {
        throw "relative metric $metricName must retain exactly $($contract.samples) raw nanosecond samples on both V1 and V2"
    }
    if (@($currentSamples | Where-Object { $_ -isnot [ValueType] -or [decimal]$_ -le 0 }).Count -ne 0) {
        throw "V2 relative metric $metricName contains a non-positive raw nanosecond sample"
    }
    $relativeGates[$metricName] = New-RelativeGate `
        -Name $metricName `
        -BaselineSamples $baselineSamples `
        -CurrentSamples $currentSamples `
        -Quantile $contract.quantile
}
$v2Standard[0].baselineMetrics = [ordered]@{
    hotRequestLogs = $baseline.metrics.hotRequestLogs
    hotChangeEventsFirstPage = $baseline.metrics.hotChangeEventsFirstPage
    startupWithoutMigration = $baseline.metrics.startupWithoutMigration
}

$evidenceKind = if ($v2Capture.isMock) { 'mock-contract-validation' } else { 'paired-persistence-performance-qualification' }
$qualificationStatus = if ($v2Capture.isMock) { 'unqualified-mock-input' } else { 'qualified-paired-run' }
$payload = [ordered]@{
    schemaVersion = 1
    evidenceKind = $evidenceKind
    qualificationStatus = $qualificationStatus
    baseline = $baseline
    v2 = $v2Standard[0]
    v2Reports = $v2Reports
    relativeGates = $relativeGates
    environment = $v2Standard[0].environment
    measurementOrder = [ordered]@{
        sequence = @('reconstructed-v0.3.1-source-baseline', 'persistence-v2')
        baselineStartedAtUtc = [string]$baseline.measurement.startedAtUtc
        baselineCompletedAtUtc = [string]$baseline.measurement.completedAtUtc
        v2StartedAtUtc = $v2MeasurementStartedAtUtc
        v2CompletedAtUtc = $v2MeasurementCompletedAtUtc
    }
    rawQualificationLines = $rawQualificationLines
    inputHashes = [ordered]@{
        baselineSha256 = Get-Sha256File $resolvedBaseline
        v2QualificationSha256 = $v2Capture.sha256
    }
}
$payloadJson = $payload | ConvertTo-Json -Depth 100 -Compress
$payload.outputPayloadSha256 = Get-Sha256Text $payloadJson

$outputDirectory = Split-Path -Parent $resolvedOutput
if (-not [string]::IsNullOrWhiteSpace($outputDirectory)) {
    New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null
}
$prettyJson = ($payload | ConvertTo-Json -Depth 100) + "`n"
[System.IO.File]::WriteAllText(
    $resolvedOutput,
    $prettyJson,
    [System.Text.UTF8Encoding]::new($false)
)
Write-Output $resolvedOutput
