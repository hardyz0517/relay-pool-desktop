param([switch]$Check)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$crate = Join-Path $repo "src-tauri"
$output = Join-Path $repo "output"
$database = Join-Path $output "sqlx-prepare.sqlite3"
$databaseArtifacts = @($database, "$database-wal", "$database-shm")
$previousDatabaseUrl = $env:DATABASE_URL
$previousSqlxOffline = $env:SQLX_OFFLINE
$locationPushed = $false

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Command $($Arguments -join ' ') failed with exit code $LASTEXITCODE"
    }
}

function Remove-DatabaseArtifacts {
    foreach ($candidate in $databaseArtifacts) {
        $fullPath = [IO.Path]::GetFullPath($candidate)
        $outputRoot = [IO.Path]::GetFullPath($output) + [IO.Path]::DirectorySeparatorChar
        if (-not $fullPath.StartsWith($outputRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw "refusing to remove SQLx preparation artifact outside output"
        }
        Remove-Item -LiteralPath $fullPath -Force -ErrorAction SilentlyContinue
    }
}

$sqlxVersion = & sqlx --version
if ($LASTEXITCODE -ne 0 -or $sqlxVersion -notmatch '^sqlx-cli 0\.8\.6(?:\s|$)') {
    throw "sqlx-cli 0.8.6 is required; found: $sqlxVersion"
}

New-Item -ItemType Directory -Force -Path $output | Out-Null

try {
    Remove-DatabaseArtifacts
    $env:DATABASE_URL = "sqlite:///$($database.Replace('\', '/'))"
    Remove-Item Env:SQLX_OFFLINE -ErrorAction SilentlyContinue
    Push-Location $crate
    $locationPushed = $true

    Invoke-Checked "sqlx" @("database", "create")
    Invoke-Checked "sqlx" @("migrate", "run", "--source", "src/persistence/migrations")

    $prepareArguments = @("sqlx", "prepare")
    if ($Check) {
        $prepareArguments += "--check"
    }
    $prepareArguments += @("--", "--all-targets")
    Invoke-Checked "cargo" $prepareArguments

    if (-not $Check) {
        $metadata = @(Get-ChildItem -LiteralPath (Join-Path $crate ".sqlx") -Filter "query-*.json" -File)
        if ($metadata.Count -eq 0) {
            throw "sqlx prepare produced no offline query metadata"
        }
    }
}
finally {
    if ($locationPushed) {
        Pop-Location
    }
    if ($null -eq $previousDatabaseUrl) {
        Remove-Item Env:DATABASE_URL -ErrorAction SilentlyContinue
    }
    else {
        $env:DATABASE_URL = $previousDatabaseUrl
    }
    if ($null -eq $previousSqlxOffline) {
        Remove-Item Env:SQLX_OFFLINE -ErrorAction SilentlyContinue
    }
    else {
        $env:SQLX_OFFLINE = $previousSqlxOffline
    }
    Remove-DatabaseArtifacts
}
