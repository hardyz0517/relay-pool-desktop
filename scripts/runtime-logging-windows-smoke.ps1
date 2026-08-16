[CmdletBinding()]
param(
    [string]$BinaryPath,
    [switch]$RunPackaged,
    [switch]$RunPackagedFaults,
    [switch]$RequirePackaged,
    [switch]$BuildPackaged,
    [int]$TimeoutSeconds = 12
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$manifest = Join-Path $repoRoot 'src-tauri/Cargo.toml'
$reuseCargoTargetForPackaged = $RunPackaged -and
    [string]::IsNullOrWhiteSpace($PSBoundParameters['BinaryPath']) -and
    ($BuildPackaged -or $RequirePackaged)

if ($RunPackagedFaults) {
    $RunPackaged = $true
    $BuildPackaged = $true
    $RequirePackaged = $true
}

Write-Output 'runtime logging Windows smoke'
Write-Output "repo: $repoRoot"

if ($RunPackaged -and -not [string]::IsNullOrWhiteSpace($BinaryPath) -and
    $BinaryPath -match '[\\/]target-tauri-feature-isolation([\\/]|$)') {
    Write-Output 'packaged smoke: blocked (stale target-tauri-feature-isolation binary; rebuild with the current tauri-test feature or omit -BinaryPath)'
    if ($RequirePackaged) {
        exit 2
    }
    exit 0
}

Write-Output 'lease/restart harness: running'

$previousCargoTarget = $env:CARGO_TARGET_DIR
$cargoTarget = Join-Path $repoRoot ('.tmp-runtime-logging-smoke/cargo-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $cargoTarget | Out-Null
$env:CARGO_TARGET_DIR = $cargoTarget

Push-Location $repoRoot
try {
    & cargo test --locked --manifest-path $manifest --lib `
        observability::runtime::service::tests::writer_lease_is_exclusive_across_process_restart_and_recovers `
        -- --nocapture
    $leaseExit = $LASTEXITCODE
}
finally {
    Pop-Location
    if ([string]::IsNullOrWhiteSpace($previousCargoTarget)) {
        Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    }
    else {
        $env:CARGO_TARGET_DIR = $previousCargoTarget
    }

    # This target is unique to this smoke run and contains only Cargo output.
    # Always reclaim it so repeated runs do not accumulate full Rust builds.
    if (-not $reuseCargoTargetForPackaged -and (Test-Path -LiteralPath $cargoTarget)) {
        try {
            Remove-Item -LiteralPath $cargoTarget -Recurse -Force -ErrorAction Stop
        }
        catch {
            Write-Warning "failed to remove smoke Cargo target '$cargoTarget': $($_.Exception.Message)"
        }
    }
}

if ($leaseExit -ne 0) {
    throw "lease/restart harness failed with exit code $leaseExit"
}
Write-Output 'lease/restart harness: passed'

if (-not $RunPackaged) {
    Write-Output 'packaged smoke: not requested (use -RunPackaged)'
    exit 0
}

$smokeRoot = Join-Path $repoRoot '.tmp-runtime-logging-smoke/windows-packaged'
New-Item -ItemType Directory -Force -Path $smokeRoot | Out-Null
$envRoot = Join-Path $smokeRoot ([guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $envRoot | Out-Null
$packagedTarget = $null
$packagedExitCode = 0
$packagedRunAllowed = $true

try {
    if ([string]::IsNullOrWhiteSpace($BinaryPath)) {
        $BinaryPath = Join-Path $repoRoot 'src-tauri/target/debug/relay-pool-desktop.exe'
    }

    # A packaged smoke must use the explicit debug-only path/exit seam. Build
    # it into a unique target when the caller requests a required run (or
    # passes -BuildPackaged); never launch an arbitrary production binary with
    # redirected APPDATA because Tauri may ignore those variables.
    $explicitBinaryProvided = -not [string]::IsNullOrWhiteSpace($PSBoundParameters['BinaryPath'])
    $resolvedBinary = Resolve-Path -LiteralPath $BinaryPath -ErrorAction SilentlyContinue
    if (-not $explicitBinaryProvided -and ($BuildPackaged -or $RequirePackaged)) {
        if ($reuseCargoTargetForPackaged) {
            $packagedTarget = $cargoTarget
        }
        else {
            $packagedTarget = Join-Path $smokeRoot ('cargo-packaged-' + [guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Force -Path $packagedTarget | Out-Null
        }
        $previousPackagedCargoTarget = $env:CARGO_TARGET_DIR
        $env:CARGO_TARGET_DIR = $packagedTarget
        Push-Location $repoRoot
        try {
            & cargo build --locked --manifest-path $manifest --features runtime-logging-windows-smoke --bin relay-pool-desktop
            $buildExit = $LASTEXITCODE
        }
        finally {
            Pop-Location
            if ([string]::IsNullOrWhiteSpace($previousPackagedCargoTarget)) {
                Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
            }
            else {
                $env:CARGO_TARGET_DIR = $previousPackagedCargoTarget
            }
        }
        if ($buildExit -ne 0) {
            Write-Output "packaged smoke: blocked (smoke binary build failed with exit code $buildExit)"
            $packagedRunAllowed = $false
            $packagedExitCode = if ($RequirePackaged) { 2 } else { 0 }
        }
        else {
            $BinaryPath = Join-Path $packagedTarget 'debug/relay-pool-desktop.exe'
            $resolvedBinary = Resolve-Path -LiteralPath $BinaryPath -ErrorAction SilentlyContinue
        }
    }

    if ($packagedRunAllowed -and $null -eq $resolvedBinary) {
        Write-Output "packaged smoke: blocked (binary not found or not built: $BinaryPath)"
        $packagedRunAllowed = $false
        $packagedExitCode = if ($RequirePackaged) { 2 } else { 0 }
    }

    if ($packagedRunAllowed) {
        # Reject the historical tauri-test target and any binary that was not
        # compiled with the debug-only override. This check runs before the
        # child process can touch a KnownFolder.
        $resolvedBinaryPath = [IO.Path]::GetFullPath($resolvedBinary.Path)
        if ($resolvedBinaryPath -match '[\\/]target-tauri-feature-isolation[\\/]') {
            Write-Output 'packaged smoke: blocked (stale target-tauri-feature-isolation binary; rebuild with -BuildPackaged)'
            $packagedRunAllowed = $false
            $packagedExitCode = if ($RequirePackaged) { 2 } else { 0 }
        }
        else {
            $binaryText = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($resolvedBinaryPath))
            if (-not $binaryText.Contains('RELAY_POOL_RUNTIME_LOGGING_SMOKE_ROOT')) {
                Write-Output 'packaged smoke: blocked (binary lacks debug-only isolated-root seam; use -BuildPackaged)'
                $packagedRunAllowed = $false
                $packagedExitCode = if ($RequirePackaged) { 2 } else { 0 }
            }
        }
    }

    if ($packagedRunAllowed) {
        # Tauri's normal KnownFolder paths are intentionally not modified.
        # The binary receives a compile-time-gated direct root instead; reject
        # a collision with the real profile before starting the child.
        $knownRoaming = [IO.Path]::GetFullPath([Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData)).TrimEnd('\')
        $knownLocal = [IO.Path]::GetFullPath([Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)).TrimEnd('\')
        $envRootFull = [IO.Path]::GetFullPath($envRoot).TrimEnd('\')
        $knownRoots = @($knownRoaming, $knownLocal) | Where-Object {
            -not [string]::IsNullOrWhiteSpace($_)
        }
        $rootCollidesWithKnownFolder = @(
            foreach ($knownRoot in $knownRoots) {
                $envRootFull.Equals($knownRoot, [StringComparison]::OrdinalIgnoreCase) -or
                    $envRootFull.StartsWith($knownRoot + '\', [StringComparison]::OrdinalIgnoreCase)
            }
        ) -contains $true
        if ($rootCollidesWithKnownFolder) {
            Write-Output 'packaged smoke: blocked (temporary root resolves to a real KnownFolder)'
            $packagedRunAllowed = $false
            $packagedExitCode = 2
        }
    }

    if ($packagedRunAllowed) {
        $psi = [System.Diagnostics.ProcessStartInfo]::new()
        $psi.FileName = $resolvedBinaryPath
        $psi.WorkingDirectory = Split-Path -Parent $resolvedBinaryPath
        $psi.UseShellExecute = $false
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.Environment['APPDATA'] = Join-Path $envRoot 'roaming'
        $psi.Environment['LOCALAPPDATA'] = Join-Path $envRoot 'local'
        $psi.Environment['TEMP'] = Join-Path $envRoot 'temp'
        $psi.Environment['TMP'] = $psi.Environment['TEMP']
        $psi.Environment['RELAY_POOL_RUNTIME_LOGGING_SMOKE_ROOT'] = $envRoot
        $psi.Environment['RELAY_POOL_RUNTIME_LOGGING_SMOKE_EXIT'] = '1'
        foreach ($name in @('APPDATA', 'LOCALAPPDATA', 'TEMP', 'TMP')) {
            New-Item -ItemType Directory -Force -Path $psi.Environment[$name] | Out-Null
        }

        $processExitCodes = @()
        $faultEvidence = $true
        if ($RunPackagedFaults) {
            # Inject marker initialization failure through the debug-only
            # lifecycle seam. The production binary does not compile this
            # environment-controlled branch.
            $psi.Environment['RELAY_POOL_RUNTIME_LOGGING_SMOKE_FAULT'] = 'marker-io'
            $psi.Environment['RELAY_POOL_RUNTIME_LOGGING_SMOKE_RUN'] = '1'
            $markerFaultProcess = [System.Diagnostics.Process]::new()
            $markerFaultProcess.StartInfo = $psi
            [void]$markerFaultProcess.Start()
            $markerFaultExited = $markerFaultProcess.WaitForExit($TimeoutSeconds * 1000)
            if (-not $markerFaultExited) {
                try { $markerFaultProcess.Kill($true) } catch { }
                [void]$markerFaultProcess.WaitForExit(3000)
                Write-Output "packaged marker I/O fault: blocked (process did not exit within ${TimeoutSeconds}s)"
                $faultEvidence = $false
            }
            else {
                $markerFaultStdout = $markerFaultProcess.StandardOutput.ReadToEnd()
                $markerFaultStderr = $markerFaultProcess.StandardError.ReadToEnd()
                $markerFaultRoot = Join-Path $envRoot 'data/runtime-logs'
                $markerFaultEvents = @(
                    if (Test-Path -LiteralPath $markerFaultRoot -PathType Container) {
                        Get-ChildItem -LiteralPath $markerFaultRoot -Filter '*.jsonl' -File -ErrorAction SilentlyContinue |
                            Select-String -SimpleMatch 'runtime.crash_marker.unavailable'
                    }
                )
                $markerFaultPaths = @(
                    (Join-Path $envRoot 'data/runtime-logs/runtime-crash.marker')
                    (Join-Path $envRoot 'data/runtime-crash.marker')
                )
                $markerFaultEvidence = $markerFaultProcess.ExitCode -eq 0 -and
                    $markerFaultEvents.Count -gt 0 -and
                    $markerFaultStderr.Contains('runtime.crash_marker.unavailable') -and
                    -not $markerFaultStderr.Contains('fixture marker I/O failure') -and
                    -not $markerFaultStderr.Contains('sk-smoke-secret') -and
                    -not $markerFaultStdout.Contains('sk-smoke-secret') -and
                    (@($markerFaultPaths | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }).Count -eq 0)
                $faultEvidence = $faultEvidence -and $markerFaultEvidence
                if (-not $markerFaultEvidence) {
                    Write-Output 'packaged marker I/O fault: blocked (exit, fixed stderr, JSONL, or marker evidence incomplete)'
                }
            }
            [void]$psi.Environment.Remove('RELAY_POOL_RUNTIME_LOGGING_SMOKE_FAULT')

            $psi.Environment['RELAY_POOL_RUNTIME_LOGGING_SMOKE_FAULT'] = 'panic'
            $faultProcess = [System.Diagnostics.Process]::new()
            $faultProcess.StartInfo = $psi
            [void]$faultProcess.Start()
            $faultExited = $faultProcess.WaitForExit($TimeoutSeconds * 1000)
            if (-not $faultExited) {
                try { $faultProcess.Kill($true) } catch { }
                [void]$faultProcess.WaitForExit(3000)
                Write-Output "packaged panic fault: blocked (process did not exit within ${TimeoutSeconds}s)"
                $faultEvidence = $false
            }
            else {
                $faultStdout = $faultProcess.StandardOutput.ReadToEnd()
                $faultStderr = $faultProcess.StandardError.ReadToEnd()
                Write-Output "packaged panic fault exit code: $($faultProcess.ExitCode)"
                if ($faultStdout) {
                    Write-Output 'packaged panic fault stdout:'
                    Write-Output $faultStdout.TrimEnd()
                }
                if ($faultStderr) {
                    Write-Output 'packaged panic fault stderr:'
                    Write-Output $faultStderr.TrimEnd()
                }
                $markerPath = Join-Path $envRoot 'data/runtime-logs/runtime-crash.marker'
                $markerContents = if (Test-Path -LiteralPath $markerPath -PathType Leaf) {
                    [IO.File]::ReadAllText($markerPath)
                }
                else {
                    ''
                }
                $null = ($faultEvidence = $faultProcess.ExitCode -ne 0 -and
                    $markerContents -eq "panic`n" -and
                    -not $faultStderr.Contains('sk-smoke-secret') -and
                    -not $faultStdout.Contains('sk-smoke-secret'))
                if (-not $faultEvidence) {
                    Write-Output 'packaged panic fault: blocked (exit, marker, or redaction evidence incomplete)'
                }
            }
            [void]$psi.Environment.Remove('RELAY_POOL_RUNTIME_LOGGING_SMOKE_FAULT')
        }
        # Launch one process and require Tauri itself to create the restart
        # child. The smoke-only child writes `complete` immediately before its
        # normal clean exit, so the harness can distinguish a real restart
        # from a second script-launched process.
        [void]$psi.Environment.Remove('RELAY_POOL_RUNTIME_LOGGING_SMOKE_RUN')
        $binaryName = [IO.Path]::GetFileName($resolvedBinaryPath)
        $processProbeAvailable = $true
        try {
            $beforeProcessIds = @(
                Get-CimInstance Win32_Process -Filter "Name='$binaryName'" -ErrorAction Stop |
                    Where-Object { $_.ExecutablePath -eq $resolvedBinaryPath } |
                    Select-Object -ExpandProperty ProcessId
            )
        }
        catch {
            $processProbeAvailable = $false
            $beforeProcessIds = @()
            Write-Output "packaged smoke: blocked (cannot inspect restart child processes: $($_.Exception.Message))"
        }

        if ($packagedRunAllowed -and $processProbeAvailable) {
            $process = [System.Diagnostics.Process]::new()
            $process.StartInfo = $psi
            [void]$process.Start()
            $exited = $process.WaitForExit($TimeoutSeconds * 1000)
            if (-not $exited) {
                try { $process.Kill($true) } catch { }
                [void]$process.WaitForExit(3000)
                Write-Output "packaged smoke: blocked (initial process did not exit within ${TimeoutSeconds}s)"
                $packagedRunAllowed = $false
                $packagedExitCode = if ($RequirePackaged) { 2 } else { 0 }
            }
            else {
                $processExitCodes += $process.ExitCode
                $stdout = $process.StandardOutput.ReadToEnd()
                $stderr = $process.StandardError.ReadToEnd()
                Write-Output "packaged initial process exit code: $($process.ExitCode)"
                if ($stdout) {
                    Write-Output 'packaged initial process stdout:'
                    Write-Output $stdout.TrimEnd()
                }
                if ($stderr) {
                    Write-Output 'packaged initial process stderr:'
                    Write-Output $stderr.TrimEnd()
                }

                $statePath = Join-Path $envRoot 'runtime-logging-smoke-restart.state'
                $restartCompleted = $false
                $restartChildObserved = $false
                $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
                while ([DateTime]::UtcNow -lt $deadline) {
                    $state = if (Test-Path -LiteralPath $statePath -PathType Leaf) {
                        [IO.File]::ReadAllText($statePath).Trim()
                    }
                    else {
                        ''
                    }
                    $running = @(
                        Get-CimInstance Win32_Process -Filter "Name='$binaryName'" -ErrorAction SilentlyContinue |
                            Where-Object {
                                $_.ExecutablePath -eq $resolvedBinaryPath -and
                                    $beforeProcessIds -notcontains $_.ProcessId -and
                                    $_.ProcessId -ne $process.Id
                            }
                    )
                    if ($running.Count -gt 0) {
                        $restartChildObserved = $true
                    }
                    if ($state -eq 'complete' -and $restartChildObserved -and $running.Count -eq 0) {
                        $restartCompleted = $true
                        break
                    }
                    Start-Sleep -Milliseconds 100
                }
                if (-not $restartCompleted) {
                    Write-Output 'packaged smoke: blocked (Tauri restart child/state completion evidence incomplete)'
                    $packagedRunAllowed = $false
                    $packagedExitCode = if ($RequirePackaged) { 2 } else { 0 }
                }
            }
        }

        if ($packagedRunAllowed) {
            $runtimeLogRoots = @(
                (Join-Path $envRoot 'data/runtime-logs'),
                (Join-Path $psi.Environment['APPDATA'] 'dev.relaypool.desktop/runtime-logs'),
                (Join-Path $psi.Environment['LOCALAPPDATA'] 'dev.relaypool.desktop/runtime-logs')
            )
            $published = @(
                foreach ($runtimeLogRoot in $runtimeLogRoots) {
                    Get-ChildItem -LiteralPath $runtimeLogRoot -Filter '*.jsonl' -File -ErrorAction SilentlyContinue
                }
            )
            $bundleFiles = @('manifest.json', 'runtime-summary.json', 'runtime-events.jsonl')
            $bundleRoots = @(1, 2) | ForEach-Object { Join-Path $envRoot "data/runtime-support-bundle-$_" }
            $bundleComplete = @(
                foreach ($bundleRoot in $bundleRoots) {
                    (Test-Path -LiteralPath $bundleRoot -PathType Container) -and
                        (($bundleFiles | Where-Object { -not (Test-Path -LiteralPath (Join-Path $bundleRoot $_) -PathType Leaf) }).Count -eq 0)
                }
            )
            $bundleEventCount = @(
                foreach ($bundleRoot in $bundleRoots) {
                    if (Test-Path -LiteralPath (Join-Path $bundleRoot 'runtime-events.jsonl') -PathType Leaf) {
                        ([IO.File]::ReadAllLines((Join-Path $bundleRoot 'runtime-events.jsonl'))).Count
                    }
                }
            ) | Measure-Object -Sum | Select-Object -ExpandProperty Sum
            $crashMarkerPaths = @(
                (Join-Path $envRoot 'data/runtime-logs/runtime-crash.marker')
                (Join-Path $envRoot 'data/runtime-crash.marker')
            )
            $cleanShutdownMarkerAbsent = @($crashMarkerPaths | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }).Count -eq 0
            $restartExitCode = [int32]::MaxValue
            if ($processExitCodes.Count -ne 1 -or $processExitCodes[0] -ne $restartExitCode) {
                Write-Output "packaged smoke: blocked (initial process exit code was not Tauri RESTART_EXIT_CODE: $($processExitCodes -join ', '))"
                $packagedRunAllowed = $false
            }
            if ($packagedRunAllowed -and
                $published.Count -ge 2 -and $bundleComplete.Count -eq 2 -and $bundleEventCount -gt 0 -and
                $cleanShutdownMarkerAbsent -and $faultEvidence) {
                $faultLabel = if ($RunPackagedFaults) { ', marker I/O + panic marker fault/redaction' } else { '' }
                Write-Output "packaged smoke: passed ($($published.Count) published segment(s), real restart child, 2 starts, reader/export, rotation$faultLabel)"
                $packagedExitCode = 0
            }
            else {
                Write-Output 'packaged smoke: blocked (startup, rotation, diagnostics reader/export, or restart evidence incomplete)'
                $packagedExitCode = if ($RequirePackaged) { 2 } else { 0 }
            }
        }
    }
}
finally {
    # The root and Cargo target contain only this run's diagnostics/build
    # output. Remove both even when the child fails or PowerShell throws.
    if (Test-Path -LiteralPath $envRoot) {
        try { Remove-Item -LiteralPath $envRoot -Recurse -Force -ErrorAction Stop }
        catch { Write-Warning "failed to remove packaged smoke root '$envRoot': $($_.Exception.Message)" }
    }
    if ($null -ne $packagedTarget -and (Test-Path -LiteralPath $packagedTarget)) {
        try { Remove-Item -LiteralPath $packagedTarget -Recurse -Force -ErrorAction Stop }
        catch { Write-Warning "failed to remove packaged smoke target '$packagedTarget': $($_.Exception.Message)" }
    }
    if (Test-Path -LiteralPath $smokeRoot) {
        try { Remove-Item -LiteralPath $smokeRoot -Recurse -Force -ErrorAction Stop }
        catch { Write-Warning "failed to remove packaged smoke directory '$smokeRoot': $($_.Exception.Message)" }
    }
}

exit $packagedExitCode
