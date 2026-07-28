[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("fast", "full", "release")]
    [string]$Profile,

    [ValidateSet("all", "prebundle", "postbundle")]
    [string]$ReleasePhase = "all"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$startedAt = Get-Date
$revision = (& git -C $repoRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw "cannot determine source revision" }
$failures = [System.Collections.Generic.List[string]]::new()
$pnpm = if ($IsWindows -or $env:OS -eq "Windows_NT") { "pnpm.cmd" } else { "pnpm" }

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter()][string[]]$Arguments = @()
    )
    $stepStarted = Get-Date
    Write-Host "::group::$Name"
    try {
        & $Command @Arguments
        $exitCode = $LASTEXITCODE
        if ($null -eq $exitCode) { $exitCode = 0 }
        if ($exitCode -ne 0) { throw "$Name failed with exit code $exitCode" }
        $elapsed = (Get-Date) - $stepStarted
        Write-Host "PASS $Name ($([math]::Round($elapsed.TotalSeconds, 2))s)"
    } catch {
        $failures.Add("$Name`: $($_.Exception.Message)")
        throw
    } finally {
        Write-Host "::endgroup::"
    }
}

function Invoke-ArchitectureGates {
    Invoke-Checked "Architecture bypass fixtures" node @("scripts/architecture/check-fixtures.mjs")
    Invoke-Checked "TypeScript boundaries" node @("scripts/architecture/check-typescript-boundaries.mjs")
    Invoke-Checked "Generated IPC bindings" $pnpm @("generate:bindings", "--check")
    Invoke-Checked "Command registry" node @("scripts/architecture/check-command-registry.mjs")
    Invoke-Checked "Tauri security" node @("scripts/architecture/check-tauri-security.mjs")
    Invoke-Checked "Production build entries" node @("scripts/architecture/check-build-entries.mjs")
    Invoke-Checked "Artifact policy" node @("scripts/architecture/check-artifact-policy.mjs")
    Invoke-Checked "Dependency lifecycle" node @("scripts/architecture/check-dependency-lifecycle.mjs")
}

function Write-Provenance {
    $artifactRoot = Join-Path $repoRoot "output/architecture-scale/qualification/release"
    New-Item -ItemType Directory -Force $artifactRoot | Out-Null
    $bundles = Get-ChildItem -Path (Join-Path $repoRoot "src-tauri/target") -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match "[\\/]release[\\/]bundle[\\/]" } |
        ForEach-Object {
            [ordered]@{ path = $_.FullName.Substring($repoRoot.Length + 1).Replace("\", "/"); sha256 = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant(); bytes = $_.Length }
        }
    $dirty = -not [string]::IsNullOrWhiteSpace((& git -C $repoRoot status --porcelain))
    [ordered]@{
        schema_version = 1
        source_revision = $revision
        dirty = $dirty
        profile = "release"
        target = "x86_64-pc-windows-msvc"
        generated_at = (Get-Date).ToUniversalTime().ToString("o")
        node = (& node --version).Trim()
        pnpm = (& $pnpm --version).Trim()
        rustc = (& rustc --version).Trim()
        artifacts = @($bundles)
    } | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $artifactRoot "provenance.json")
}

Push-Location $repoRoot
try {
    Write-Host "verify start=$($startedAt.ToUniversalTime().ToString('o')) revision=$revision profile=$Profile releasePhase=$ReleasePhase"

    if ($Profile -eq "release" -and $ReleasePhase -eq "postbundle") {
        Invoke-Checked "Final release bundle scan" $pnpm @("verify:release-bundle")
        Write-Provenance
        return
    }

    Invoke-ArchitectureGates
    Invoke-Checked "ESLint" $pnpm @("lint")
    Invoke-Checked "TypeScript check" $pnpm @("exec", "tsc", "--noEmit")
    Invoke-Checked "Rust architecture fixtures" cargo @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--test", "architecture_scale_boundaries")

    if ($Profile -in @("full", "release")) {
        Invoke-Checked "Tracked persistence artifact policy" $pnpm @("verify:persistence-artifacts")
        Invoke-Checked "Deterministic frontend scale baseline" $pnpm @("architecture:scale-baseline")
        Invoke-Checked "Advisory, license and source policy" "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "scripts/check-advisories.ps1")
        Invoke-Checked "Frontend contract tests" $pnpm @("test:contracts")
        Invoke-Checked "Frontend unit tests" $pnpm @("test")
        Invoke-Checked "Frontend production build" $pnpm @("build")
        Invoke-Checked "Rust formatting" cargo @("fmt", "--manifest-path", "src-tauri/Cargo.toml", "--", "--check")
        Invoke-Checked "Rust clippy" cargo @("clippy", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets")
        Invoke-Checked "Rust check" cargo @("check", "--locked", "--manifest-path", "src-tauri/Cargo.toml")
        Invoke-Checked "Rust tests" cargo @("test", "--locked", "--manifest-path", "src-tauri/Cargo.toml")
    }

    if ($Profile -eq "release") {
        Invoke-Checked "Release version contract" $pnpm @("verify:release-version", "--require-tag")
        Invoke-Checked "Locked Rust release build" cargo @("build", "--release", "--locked", "--manifest-path", "src-tauri/Cargo.toml", "--target", "x86_64-pc-windows-msvc")
        if ($ReleasePhase -eq "all") {
            foreach ($name in @("TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD")) {
                if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) { throw "$name is required for release bundling" }
            }
            Invoke-Checked "Signed Tauri bundle" $pnpm @("tauri:build", "--", "--target", "x86_64-pc-windows-msvc")
            Invoke-Checked "Final release bundle scan" $pnpm @("verify:release-bundle")
            Write-Provenance
        }
    }
} catch {
    if ($failures.Count -eq 0) { $failures.Add($_.Exception.Message) }
    Write-Error ("verification failed; first root cause: " + $failures[0] + [Environment]::NewLine + "failure summary:" + [Environment]::NewLine + ($failures -join [Environment]::NewLine))
    exit 1
} finally {
    $finishedAt = Get-Date
    Write-Host "verify end=$($finishedAt.ToUniversalTime().ToString('o')) duration=$([math]::Round(($finishedAt - $startedAt).TotalSeconds, 2))s"
    Pop-Location
}
