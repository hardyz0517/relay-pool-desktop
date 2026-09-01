param(
  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [string]$OldInstaller,

  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [string]$NewInstaller,

  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [string]$OldVersion,

  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [string]$NewVersion,

  [Parameter(Mandatory = $true)]
  [ValidateNotNullOrEmpty()]
  [string]$OutputPath,

  [string]$InstallDir = (Join-Path $env:TEMP "RelayPoolInstallMatrix\Relay Pool Desktop")
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
$installRoot = [System.IO.Path]::GetFullPath((Split-Path -Parent $InstallDir))
$installDirFull = [System.IO.Path]::GetFullPath($InstallDir)
$preserved = @()
$results = New-Object System.Collections.Generic.List[object]

function Resolve-ExplicitPath([string]$Path) {
  if ([System.IO.Path]::IsPathRooted($Path)) {
    return [System.IO.Path]::GetFullPath($Path)
  }
  return [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $Path))
}

$oldInstallerFull = Resolve-ExplicitPath $OldInstaller
$newInstallerFull = Resolve-ExplicitPath $NewInstaller

function Add-Result([string]$Name, [string]$Status, [hashtable]$Details) {
  $results.Add([pscustomobject]@{
    name = $Name
    status = $Status
    timestamp = (Get-Date).ToUniversalTime().ToString("o")
    details = $Details
  })
}

function Invoke-TimedProcess([string]$Path, [string[]]$Arguments, [int]$TimeoutSeconds) {
  $psi = [System.Diagnostics.ProcessStartInfo]::new()
  $psi.FileName = $Path
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  # ArgumentList preserves spaces and quoting in the install directory.  A
  # hand-built Arguments string would split `/D=` at `Relay Pool Desktop`.
  foreach ($argument in $Arguments) {
    [void]$psi.ArgumentList.Add([string]$argument)
  }

  $process = [System.Diagnostics.Process]::new()
  $process.StartInfo = $psi
  try {
    if (-not $process.Start()) {
      throw "failed to start process: $Path"
    }
    # Drain both redirected streams while the process is running so a verbose
    # installer cannot block on a full pipe before WaitForExit returns.
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    $completed = $process.WaitForExit($TimeoutSeconds * 1000)
    if (-not $completed) {
      try { $process.Kill($true) } catch { try { $process.Kill() } catch {} }
      $process.WaitForExit()
      throw "Process timed out after $TimeoutSeconds seconds: $Path"
    }
    return [pscustomobject]@{
      exitCode = $process.ExitCode
      stdout = $stdoutTask.GetAwaiter().GetResult()
      stderr = $stderrTask.GetAwaiter().GetResult()
    }
  } finally {
    $process.Dispose()
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

function Invoke-SqliteScalar([string]$DatabasePath, [string]$Query) {
  $sqlite = Get-Command sqlite3 -ErrorAction SilentlyContinue
  if (-not $sqlite) { throw "sqlite3 is required for the durable startup probe" }
  $output = & $sqlite.Source -readonly -batch -noheader -cmd ".timeout 5000" $DatabasePath $Query 2>&1
  if ($LASTEXITCODE -ne 0) { throw "sqlite3 startup probe query failed with exit code $LASTEXITCODE" }
  return (($output | ForEach-Object { $_.ToString() }) -join "`n").Trim()
}

function Get-InstalledDatabasePath {
  $candidates = foreach ($root in $appDataPaths) {
    if (-not (Test-Path -LiteralPath $root -PathType Container)) { continue }
    Get-ChildItem -LiteralPath $root -Filter "relay-pool-desktop-v2.sqlite3" -File -Recurse -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -notmatch "[\\/]backups[\\/]" }
  }
  return ($candidates | Sort-Object { $_.FullName.Length } | Select-Object -First 1).FullName
}

function Get-DurableStartupSnapshot([int64]$MinimumSchemaVersion, [Nullable[int64]]$PreviousWriteProbeCount, [bool]$RequireLocalProxy) {
  $databasePath = Get-InstalledDatabasePath
  if (-not $databasePath) { throw "installed database was not found under the application data roots" }
  $compatibility = Invoke-SqliteScalar $databasePath "SELECT schema_version || '|' || database_generation FROM persistence_schema_compatibility WHERE singleton_key = 1;"
  $compatibilityParts = $compatibility -split '\|', 2
  if ($compatibilityParts.Count -ne 2) { throw "startup probe returned malformed schema compatibility metadata" }
  $schemaVersion = [int64]$compatibilityParts[0]
  $databaseGeneration = [int64]$compatibilityParts[1]
  $migrationVersion = [int64](Invoke-SqliteScalar $databasePath "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1;")
  $health = Invoke-SqliteScalar $databasePath "SELECT write_probe_count || '|' || last_open_mode || '|' || COALESCE(last_checked_at, '') FROM persistence_runtime_health WHERE singleton_key = 1;"
  $healthParts = $health -split '\|', 3
  if ($healthParts.Count -lt 2) { throw "startup probe returned malformed runtime health metadata" }
  $writeProbeCount = [int64]$healthParts[0]
  $openMode = $healthParts[1]
  $lastCheckedAt = if ($healthParts.Count -ge 3) { $healthParts[2] } else { "" }
  $quickCheck = Invoke-SqliteScalar $databasePath "PRAGMA quick_check;"
  if ($quickCheck -ne "ok") { throw "startup probe quick_check failed: $quickCheck" }
  $foreignKeyCheck = Invoke-SqliteScalar $databasePath "PRAGMA foreign_key_check;"
  if (-not [string]::IsNullOrWhiteSpace($foreignKeyCheck)) { throw "startup probe foreign_key_check returned violations" }
  $sanitizerStatus = $null
  if ($schemaVersion -ge 18) {
    $sanitizerStatus = Invoke-SqliteScalar $databasePath "SELECT status FROM request_log_url_sanitizer_progress WHERE id = 'request_logs_upstream_base_url_v1';"
    if ($sanitizerStatus -ne "complete") { throw "startup probe request-log sanitizer is not complete: $sanitizerStatus" }
  }
  $configuredPort = [int](Invoke-SqliteScalar $databasePath "SELECT value FROM settings WHERE key = 'local_proxy_port';")
  if ($configuredPort -le 0 -or $configuredPort -gt 65535) { throw "startup probe found invalid local proxy port" }
  if ($databaseGeneration -ne 2) { throw "startup probe database generation mismatch: $databaseGeneration" }
  if ($schemaVersion -lt $MinimumSchemaVersion) { throw "startup probe schema is below expected minimum: $schemaVersion < $MinimumSchemaVersion" }
  if ($migrationVersion -lt $schemaVersion) { throw "startup probe migration ledger is behind schema metadata: $migrationVersion < $schemaVersion" }
  if ($openMode -ne "writable") { throw "startup probe runtime open mode is not writable: $openMode" }
  if ($null -ne $PreviousWriteProbeCount -and $writeProbeCount -le $PreviousWriteProbeCount) { throw "startup probe did not observe a writable open during this process" }
  $listener = @(Get-NetTCPConnection -State Listen -LocalPort $configuredPort -ErrorAction SilentlyContinue | Select-Object LocalAddress, LocalPort, OwningProcess, State)
  if ($RequireLocalProxy -and $listener.Count -eq 0) { throw "startup probe local proxy is not listening on configured port" }
  $proxyHttpStatus = $null
  if ($RequireLocalProxy) {
    $client = [System.Net.Http.HttpClient]::new()
    try {
      $response = $client.GetAsync("http://127.0.0.1:$configuredPort/v1/models").GetAwaiter().GetResult()
      $proxyHttpStatus = [int]$response.StatusCode
      if ($proxyHttpStatus -ne 401) { throw "startup probe local proxy /v1/models returned HTTP $proxyHttpStatus instead of protected 401" }
    } finally { $client.Dispose() }
  }
  return [ordered]@{
    databasePath = $databasePath
    mode = "writable"
    decision = "ready"
    failureReason = $null
    currentSchemaVersion = $schemaVersion
    sqlMigrationVersion = $migrationVersion
    databaseGeneration = $databaseGeneration
    writeProbeCount = $writeProbeCount
    previousWriteProbeCount = $PreviousWriteProbeCount
    lastCheckedAt = $lastCheckedAt
    sanitizerStatus = $sanitizerStatus
    runtimeRegistered = $true
    localProxyRegistered = ($listener.Count -gt 0)
    localProxyPort = $configuredPort
    localProxyHttpStatus = $proxyHttpStatus
    listeners = $listener
    evidenceSource = "sqlite durable health metadata and process-local listener"
  }
}

function Start-And-ProbeApp([string]$Name, [int64]$MinimumSchemaVersion, [bool]$RequireLocalProxy) {
  $exe = Get-InstalledExePath
  if (-not $exe) {
    throw "Installed executable not found"
  }
  $databaseBeforeStart = Get-InstalledDatabasePath
  $previousWriteProbeCount = $null
  if ($databaseBeforeStart) {
    try {
      $previousWriteProbeCount = [int64](Invoke-SqliteScalar $databaseBeforeStart "SELECT write_probe_count FROM persistence_runtime_health WHERE singleton_key = 1;")
    } catch { $previousWriteProbeCount = $null }
  }
  $primary = Start-Process -FilePath $exe -PassThru -WindowStyle Hidden
  $startupProbe = $null
  $probeError = $null
  for ($attempt = 1; $attempt -le 30; $attempt++) {
    Start-Sleep -Seconds 2
    $primary.Refresh()
    if ($primary.HasExited) {
      throw "$Name primary process exited during startup with code $($primary.ExitCode)"
    }
    try {
      $startupProbe = Get-DurableStartupSnapshot $MinimumSchemaVersion $previousWriteProbeCount $RequireLocalProxy
      break
    } catch { $probeError = $_.Exception.Message }
  }
  if ($null -eq $startupProbe) {
    Add-Result $Name "fail" @{
      executable = $exe
      primaryPid = $primary.Id
      startupProbe = @{ mode = "unknown"; decision = "unknown"; failureReason = "startupProbeFailed"; lastError = $probeError }
    }
    throw "$Name startup probe failed: $probeError"
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
  $probeStatus = if ($singleInstanceOk) { "pass" } else { "fail" }
  Add-Result $Name $probeStatus @{
    executable = $exe
    primaryPid = $primary.Id
    secondPid = $second.Id
    secondExited = $second.HasExited
    runningExecutableProcessCountAfterSecondLaunch = ($running | Measure-Object).Count
    singleInstanceOk = $singleInstanceOk
    startupEstablishedTcpConnections = @($connections)
    startupProbe = $startupProbe
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
  $installRootPrefix = $installRoot.TrimEnd([char[]]@('\', '/')) + [System.IO.Path]::DirectorySeparatorChar
  if ([string]::Equals($installDirFull, $installRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
      -not $installDirFull.StartsWith($installRootPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
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
    oldVersion = $OldVersion
    newVersion = $NewVersion
    installDir = $installDirFull
    oldInstallerExists = (Test-Path -LiteralPath $oldInstallerFull)
    newInstallerExists = (Test-Path -LiteralPath $newInstallerFull)
    registryBefore = (Get-InstallRegistry)
  }

  # Ensure no running target process keeps database/WAL files open while the
  # user's existing app-data directory is moved out of the way.
  Stop-RelayProcesses
  Move-AppDataAside
  Uninstall-CurrentPackage "remove-existing-install-or-orphan-state"
  Install-Package "fresh-install-candidate" $newInstallerFull $NewVersion
  $env:RELAY_POOL_START_PROXY_ON_LAUNCH = "1"
  Start-And-ProbeApp "fresh-startup-offline-single-instance-close-probe-candidate" 71 $true

  Uninstall-CurrentPackage "remove-fresh-candidate"
  Install-Package "install-supported-baseline" $oldInstallerFull $OldVersion
  Start-And-ProbeApp "supported-baseline-startup" 1 $false
  Install-Package "upgrade-baseline-to-candidate" $newInstallerFull $NewVersion
  Start-And-ProbeApp "post-upgrade-startup-single-instance-close-probe-candidate" 71 $true
} catch {
  $overallStatus = "fail"
  $errorMessage = $_.Exception.Message
  Add-Result "matrix-error" "fail" @{ message = $errorMessage }
} finally {
  Remove-Item Env:RELAY_POOL_START_PROXY_ON_LAUNCH -ErrorAction SilentlyContinue
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
    baseline = [ordered]@{
      version = $OldVersion
      installer = $oldInstallerFull
      installerSha256 = if (Test-Path -LiteralPath $oldInstallerFull) { (Get-FileHash -LiteralPath $oldInstallerFull -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
    }
    candidate = [ordered]@{
      version = $NewVersion
      installer = $newInstallerFull
      installerSha256 = if (Test-Path -LiteralPath $newInstallerFull) { (Get-FileHash -LiteralPath $newInstallerFull -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
    }
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
