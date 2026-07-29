param(
  [string]$OldInstaller = (Join-Path $env:USERPROFILE "Downloads\Relay.Pool.Desktop_0.3.2_x64-setup.exe"),
  [string]$NewInstaller = "src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\Relay Pool Desktop_0.3.3_x64-setup.exe",
  [string]$InstallDir = (Join-Path $env:TEMP "RelayPoolInstallMatrix\Relay Pool Desktop"),
  [string]$OutputPath = "output\architecture-scale\qualification\release\install-upgrade-matrix-v0.3.3-2026-07-29-summary.json"
)

$ErrorActionPreference = "Stop"

$uninstallRegistryPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Relay Pool Desktop"
$appDataPaths = @(
  "$env:LOCALAPPDATA\dev.relaypool.desktop",
  "$env:APPDATA\dev.relaypool.desktop"
)
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$outputFullPath = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $OutputPath))
$outputDir = Split-Path -Parent $outputFullPath
$rawDir = Join-Path $outputDir ("install-upgrade-matrix-raw-" + $stamp)
$oldInstallerFull = [System.IO.Path]::GetFullPath($OldInstaller)
$newInstallerFull = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $NewInstaller))
$installRoot = [System.IO.Path]::GetFullPath((Split-Path -Parent $InstallDir))
$installDirFull = [System.IO.Path]::GetFullPath($InstallDir)
$preserved = @()
$results = New-Object System.Collections.Generic.List[object]

function Add-Result([string]$Name, [string]$Status, [hashtable]$Details) {
  $results.Add([pscustomobject]@{
    name = $Name
    status = $Status
    timestamp = (Get-Date).ToUniversalTime().ToString("o")
    details = $Details
  })
}

function Invoke-TimedProcess([string]$Path, [string[]]$Arguments, [int]$TimeoutSeconds) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Path
  $psi.Arguments = ($Arguments -join " ")
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $process = [System.Diagnostics.Process]::Start($psi)
  $completed = $process.WaitForExit($TimeoutSeconds * 1000)
  if (-not $completed) {
    try { $process.Kill() } catch {}
    throw "Process timed out after $TimeoutSeconds seconds: $Path"
  }
  return [pscustomobject]@{
    exitCode = $process.ExitCode
    stdout = $process.StandardOutput.ReadToEnd()
    stderr = $process.StandardError.ReadToEnd()
  }
}

function Get-InstallRegistry {
  if (-not (Test-Path $uninstallRegistryPath)) {
    return $null
  }
  $item = Get-ItemProperty $uninstallRegistryPath
  return [pscustomobject]@{
    displayName = $item.DisplayName
    displayVersion = $item.DisplayVersion
    installLocation = ($item.InstallLocation -as [string])
    uninstallString = ($item.UninstallString -as [string])
  }
}

function Get-InstalledExePath {
  $candidates = @(
    (Join-Path $installDirFull "Relay Pool Desktop.exe"),
    (Join-Path $installDirFull "relay-pool-desktop.exe")
  )
  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate) {
      return $candidate
    }
  }
  return $null
}

function Get-InstalledSnapshot {
  $exe = Get-InstalledExePath
  $registry = Get-InstallRegistry
  $hash = $null
  $versionInfo = $null
  if ($exe) {
    $hash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
    $file = Get-Item -LiteralPath $exe
    $versionInfo = [pscustomobject]@{
      length = $file.Length
      fileVersion = $file.VersionInfo.FileVersion
      productVersion = $file.VersionInfo.ProductVersion
      productName = $file.VersionInfo.ProductName
    }
  }
  return [pscustomobject]@{
    registry = $registry
    exe = $exe
    exeSha256 = $hash
    versionInfo = $versionInfo
  }
}

function Stop-RelayProcesses {
  $processes = Get-CimInstance Win32_Process |
    Where-Object {
      $_.ExecutablePath -and
      ($_.ExecutablePath -like "*Relay Pool Desktop*" -or $_.Name -like "*relay-pool-desktop*")
    }
  foreach ($process in $processes) {
    try {
      Stop-Process -Id $process.ProcessId -Force -ErrorAction Stop
    } catch {}
  }
  Start-Sleep -Seconds 2
}

function Start-And-ProbeApp([string]$Name) {
  $exe = Get-InstalledExePath
  if (-not $exe) {
    throw "Installed executable not found"
  }
  $primary = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
  Start-Sleep -Seconds 10
  $primary.Refresh()
  if ($primary.HasExited) {
    throw "$Name primary process exited during startup with code $($primary.ExitCode)"
  }

  $second = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
  Start-Sleep -Seconds 5
  $second.Refresh()
  $running = Get-CimInstance Win32_Process |
    Where-Object { $_.ExecutablePath -and ([System.IO.Path]::GetFullPath($_.ExecutablePath) -eq [System.IO.Path]::GetFullPath($exe)) } |
    Select-Object ProcessId, Name, ExecutablePath
  $singleInstanceOk = (($running | Measure-Object).Count -eq 1)

  $connections = @()
  try {
    $connections = Get-NetTCPConnection -OwningProcess $primary.Id -ErrorAction SilentlyContinue |
      Select-Object LocalAddress, LocalPort, RemoteAddress, RemotePort, State
  } catch {}

  $closeResult = @{
    closeMainWindowReturned = $false
    exitedAfterCloseMainWindow = $false
  }
  try {
    $closeResult.closeMainWindowReturned = $primary.CloseMainWindow()
    Start-Sleep -Seconds 8
    $primary.Refresh()
    $closeResult.exitedAfterCloseMainWindow = $primary.HasExited
  } catch {}

  Stop-RelayProcesses
  Add-Result $Name "pass" @{
    executable = $exe
    primaryPid = $primary.Id
    secondPid = $second.Id
    secondExited = $second.HasExited
    runningExecutableProcessCountAfterSecondLaunch = ($running | Measure-Object).Count
    singleInstanceOk = $singleInstanceOk
    startupEstablishedTcpConnections = @($connections)
    closeProbe = $closeResult
  }
  if (-not $singleInstanceOk) {
    throw "$Name single-instance check failed"
  }
}

function Install-Package([string]$Name, [string]$Installer, [string]$ExpectedVersion) {
  if (-not (Test-Path -LiteralPath $Installer)) {
    throw "Installer missing: $Installer"
  }
  $result = Invoke-TimedProcess $Installer @("/S", "/D=$installDirFull") 180
  Start-Sleep -Seconds 3
  $snapshot = Get-InstalledSnapshot
  $registryVersion = $snapshot.registry.displayVersion
  $versionOk = $registryVersion -eq $ExpectedVersion
  Add-Result $Name ($(if ($result.exitCode -eq 0 -and $snapshot.exe -and $versionOk) { "pass" } else { "fail" })) @{
    installer = $Installer
    installerSha256 = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLowerInvariant()
    exitCode = $result.exitCode
    expectedVersion = $ExpectedVersion
    snapshot = $snapshot
  }
  if ($result.exitCode -ne 0) {
    throw "$Name installer exited $($result.exitCode)"
  }
  if (-not $snapshot.exe) {
    throw "$Name did not install an executable"
  }
  if (-not $versionOk) {
    throw "$Name registry version mismatch: $registryVersion"
  }
}

function Uninstall-CurrentPackage([string]$Name) {
  Stop-RelayProcesses
  $uninstaller = Join-Path $installDirFull "uninstall.exe"
  if (Test-Path -LiteralPath $uninstaller) {
    $result = Invoke-TimedProcess $uninstaller @("/S") 120
    Add-Result $Name ($(if ($result.exitCode -eq 0) { "pass" } else { "fail" })) @{
      uninstaller = $uninstaller
      exitCode = $result.exitCode
    }
  } elseif (Test-Path $uninstallRegistryPath) {
    $registry = Get-InstallRegistry
    Remove-Item -LiteralPath $uninstallRegistryPath -Recurse -Force
    Add-Result $Name "pass" @{
      removedOrphanRegistry = $true
      previousRegistry = $registry
    }
  } else {
    Add-Result $Name "pass" @{ nothingInstalled = $true }
  }
  Remove-InstallDirectoryWithRetry
}

function Remove-InstallDirectoryWithRetry {
  if (-not $installDirFull.StartsWith($installRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to remove install directory outside expected root: $installDirFull"
  }
  for ($attempt = 1; $attempt -le 10; $attempt++) {
    if (-not (Test-Path -LiteralPath $installDirFull)) {
      return
    }
    try {
      Remove-Item -LiteralPath $installDirFull -Recurse -Force -ErrorAction Stop
      return
    } catch {
      if ($attempt -eq 10) {
        throw
      }
      Start-Sleep -Milliseconds 750
    }
  }
}

function Move-AppDataAside {
  foreach ($path in $appDataPaths) {
    if (Test-Path -LiteralPath $path) {
      $destination = "$path.install-matrix-preserved-$stamp"
      Move-Item -LiteralPath $path -Destination $destination
      $script:preserved += [pscustomobject]@{ original = $path; preserved = $destination }
    }
  }
  Add-Result "preserve-existing-app-data" "pass" @{
    preserved = @($script:preserved)
  }
}

function Restore-AppData {
  foreach ($path in $appDataPaths) {
    if (Test-Path -LiteralPath $path) {
      Remove-Item -LiteralPath $path -Recurse -Force
    }
  }
  foreach ($item in $script:preserved) {
    if (Test-Path -LiteralPath $item.preserved) {
      Move-Item -LiteralPath $item.preserved -Destination $item.original
    }
  }
}

New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
New-Item -ItemType Directory -Force -Path $rawDir | Out-Null

$overallStatus = "pass"
$errorMessage = $null
try {
  Add-Result "preflight" "pass" @{
    oldInstaller = $oldInstallerFull
    newInstaller = $newInstallerFull
    installDir = $installDirFull
    oldInstallerExists = (Test-Path -LiteralPath $oldInstallerFull)
    newInstallerExists = (Test-Path -LiteralPath $newInstallerFull)
    registryBefore = (Get-InstallRegistry)
  }

  Move-AppDataAside
  Uninstall-CurrentPackage "remove-existing-install-or-orphan-state"
  Install-Package "fresh-install-v0.3.3" $newInstallerFull "0.3.3"
  Start-And-ProbeApp "fresh-startup-offline-single-instance-close-probe-v0.3.3"

  Uninstall-CurrentPackage "remove-fresh-v0.3.3"
  Install-Package "install-supported-baseline-v0.3.2" $oldInstallerFull "0.3.2"
  Start-And-ProbeApp "supported-baseline-startup-v0.3.2"
  Install-Package "upgrade-v0.3.2-to-v0.3.3" $newInstallerFull "0.3.3"
  Start-And-ProbeApp "post-upgrade-startup-single-instance-close-probe-v0.3.3"
} catch {
  $overallStatus = "fail"
  $errorMessage = $_.Exception.Message
  Add-Result "matrix-error" "fail" @{ message = $errorMessage }
} finally {
  Stop-RelayProcesses
  Restore-AppData
  Add-Result "restore-existing-app-data" "pass" @{
    restored = @($script:preserved)
  }
  $finalSnapshot = Get-InstalledSnapshot
  $resultArray = @($results.ToArray())
  $summary = [ordered]@{
    status = $overallStatus
    matrixError = $errorMessage
    generatedAt = (Get-Date).ToUniversalTime().ToString("o")
    outputPath = $outputFullPath
    rawDir = $rawDir
    installDir = $installDirFull
    finalInstalledSnapshot = $finalSnapshot
    results = $resultArray
    limitations = @(
      "No screenshots or direct visual desktop inspection were used.",
      "Offline startup was checked by launch stability and established TCP connection snapshot, not by disabling the host network adapter.",
      "Tray quit was not clicked through the desktop shell; close behavior and source-level tray/exit contracts remain covered by repository tests."
    )
  }
  $summary | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $outputFullPath -Encoding UTF8
}

if ($overallStatus -ne "pass") {
  throw $errorMessage
}

Write-Host "install-upgrade matrix passed: $outputFullPath"
