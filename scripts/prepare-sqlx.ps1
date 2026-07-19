param([switch]$Check)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$output = Join-Path $repo "output"
$db = Join-Path $output "sqlx-prepare.sqlite3"

New-Item -ItemType Directory -Force -Path $output | Out-Null
foreach ($candidate in @($db, "$db-wal", "$db-shm")) {
    $fullCandidate = [IO.Path]::GetFullPath($candidate)
    $fullOutput = [IO.Path]::GetFullPath($output)
    if ($fullCandidate.StartsWith($fullOutput)) {
        Remove-Item -LiteralPath $candidate -Force -ErrorAction SilentlyContinue
    }
}

$old = $env:DATABASE_URL
try {
    $env:DATABASE_URL = "sqlite:///$($db.Replace('\', '/'))"
    Push-Location (Join-Path $repo "src-tauri")
    cargo sqlx database create
    if ($LASTEXITCODE) { throw "sqlx database create failed" }
    cargo sqlx migrate run --source src/persistence/migrations
    if ($LASTEXITCODE) { throw "sqlx migrate failed" }
    $args = @("sqlx", "prepare")
    if ($Check) { $args += "--check" }
    $args += @("--", "--all-targets")
    & cargo @args
    if ($LASTEXITCODE) { throw "sqlx prepare failed" }
}
finally {
    Pop-Location -ErrorAction SilentlyContinue
    $env:DATABASE_URL = $old
    foreach ($candidate in @($db, "$db-wal", "$db-shm")) {
        Remove-Item -LiteralPath $candidate -Force -ErrorAction SilentlyContinue
    }
}
