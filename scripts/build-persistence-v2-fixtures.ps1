[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string[]] $Tags
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$fixtureRoot = Join-Path $repoRoot 'src-tauri\tests\persistence_upgrade\fixtures'
$manifestPath = Join-Path $repoRoot 'docs\superpowers\audits\persistence-v2-released-schema-manifest.json'
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) "relay-pool-persistence-v2-fixtures-$PID"
$probeTarget = Join-Path $tempRoot 'cargo-target'
$probeSource = @'

#[cfg(test)]
mod persistence_fixture_export_probe {
    use super::*;

    #[test]
    fn export_release_database_through_normal_test_initialization() {
        let output = std::env::var("RELAY_POOL_FIXTURE_OUTPUT").expect("fixture output path");
        let database = AppDatabase::new_in_memory_for_tests().expect("release database init");
        let connection = database.connection().expect("release database connection");
        connection
            .execute("VACUUM INTO ?1", rusqlite::params![output])
            .expect("export initialized release database");
    }
}
'@

function Invoke-Git {
    param([Parameter(ValueFromRemainingArguments = $true)][string[]] $Arguments)
    & git -C $repoRoot @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "git failed: $($Arguments -join ' ')"
    }
}

function Invoke-ReleaseProbe {
    param([string] $Tag, [string] $Worktree, [string] $Output)

    Invoke-Git worktree add --detach $Worktree $Tag
    $databaseSource = Join-Path $Worktree 'src-tauri\src\services\database.rs'
    if (-not (Test-Path -LiteralPath $databaseSource -PathType Leaf)) {
        throw "$Tag does not contain src-tauri/src/services/database.rs"
    }
    Add-Content -LiteralPath $databaseSource -Value $probeSource -Encoding utf8
    $env:RELAY_POOL_FIXTURE_OUTPUT = $Output
    $env:CARGO_TARGET_DIR = $probeTarget
    try {
        & cargo test --manifest-path (Join-Path $Worktree 'src-tauri\Cargo.toml') --lib `
            'persistence_fixture_export_probe::export_release_database_through_normal_test_initialization' `
            -- --nocapture
        if ($LASTEXITCODE -ne 0) {
            throw "release initialization probe failed for $Tag"
        }
    }
    finally {
        Remove-Item Env:\RELAY_POOL_FIXTURE_OUTPUT -ErrorAction SilentlyContinue
    }
    if (-not (Test-Path -LiteralPath $Output -PathType Leaf)) {
        throw "release initialization probe did not create a database for $Tag"
    }
}

function Get-FixtureEvidence {
    param([string] $DatabasePath, [string] $EvidencePath)

    $python = @'
import hashlib, json, sqlite3, sys

database_path, evidence_path = sys.argv[1:3]
connection = sqlite3.connect(database_path)
connection.execute("PRAGMA foreign_keys = ON")

def columns(table):
    return {row[1] for row in connection.execute(f"PRAGMA table_info({table})")}

def exists(table):
    return connection.execute(
        "SELECT 1 FROM sqlite_schema WHERE type='table' AND name=?", (table,)
    ).fetchone() is not None

def insert_known(table, values):
    if not exists(table):
        return
    selected = {key: value for key, value in values.items() if key in columns(table)}
    names = list(selected)
    placeholders = ", ".join("?" for _ in names)
    connection.execute(
        f"INSERT OR REPLACE INTO {table} ({', '.join(names)}) VALUES ({placeholders})",
        [selected[name] for name in names],
    )

now = "1700000000000"
if exists("settings"):
    for setting, value in (
        ("local_proxy_port", "18787"),
        ("collector_interval_minutes", "17"),
        ("local_key", "fixture-local-placeholder"),
    ):
        connection.execute(
            "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?, ?, ?)",
            (setting, value, now),
        )

insert_known("stations", {
    "id": "fixture-station-001", "name": "Fixture Station", "station_type": "custom",
    "base_url": "https://fixture.invalid", "website_url": "https://fixture.invalid",
    "api_base_url": "https://fixture.invalid", "api_key": "", "enabled": 1, "priority": 0,
    "credit_per_cny": 1.0, "collection_interval_minutes": 17, "status": "unchecked",
    "created_at": now, "updated_at": now,
})
insert_known("station_keys", {
    "id": "fixture-station-key-001", "station_id": "fixture-station-001", "name": "Fixture Credential",
    "api_key": "", "enabled": 1, "priority": 0, "routing_order": 0, "max_concurrency": 2,
    "schedulable": 1, "status": "unchecked", "created_at": now, "updated_at": now,
})
insert_known("model_aliases", {
    "id": "fixture-alias-001", "client_model": "fixture-client-model",
    "upstream_model": "fixture-upstream-model", "enabled": 1, "created_at": now, "updated_at": now,
})
connection.commit()
connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")

schema_rows = connection.execute("""
    SELECT type, name, tbl_name, COALESCE(sql, '')
    FROM sqlite_schema
    WHERE name NOT LIKE 'sqlite_%'
    ORDER BY type ASC, name ASC, tbl_name ASC
""").fetchall()
canonical = "\n".join("\x1f".join(row) for row in schema_rows).encode("utf-8")
schema_hash = hashlib.sha256(canonical).hexdigest()
target_tables = {
    "settings", "secrets", "stations", "station_credentials", "station_keys",
    "remote_station_keys", "station_key_capabilities", "model_aliases", "collector_runs",
    "collector_snapshots", "station_group_bindings", "group_rate_records", "pricing_rules",
    "model_base_prices", "balance_snapshots", "channel_monitor_request_templates",
    "channel_monitors", "channel_monitor_runs", "request_logs", "station_key_health",
    "station_endpoint_health", "change_events",
}
tables = [row[0] for row in connection.execute(
    "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
) if row[0] in target_tables]
table_counts = {
    table: connection.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
    for table in tables
    if table != "settings"
}
connection.close()

with open(database_path, "rb") as fixture:
    fixture_hash = hashlib.sha256(fixture.read()).hexdigest()
with open(evidence_path, "w", encoding="utf-8", newline="\n") as output:
    json.dump({"schema_hash": schema_hash, "fixture_hash": fixture_hash, "table_counts": table_counts}, output, indent=2)
    output.write("\n")
'@
    $pythonPath = Join-Path $tempRoot 'fixture_evidence.py'
    Set-Content -LiteralPath $pythonPath -Value $python -Encoding utf8
    & python $pythonPath $DatabasePath $EvidencePath
    if ($LASTEXITCODE -ne 0) {
        throw 'Python 3 with the standard sqlite3 module is required to finalize fixtures'
    }
}

New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
$worktrees = [System.Collections.Generic.List[string]]::new()
try {
    $releaseEvidence = [ordered]@{}
    foreach ($tag in $Tags) {
        if ($tag -notmatch '^v\d+\.\d+\.\d+$') {
            throw "invalid release tag: $tag"
        }
        Invoke-Git rev-parse --verify "refs/tags/$tag^{commit}" | Out-Null
        $safeTag = $tag -replace '[^A-Za-z0-9._-]', '_'
        $worktree = Join-Path $tempRoot "worktree-$safeTag"
        $database = Join-Path $tempRoot "$safeTag.sqlite3"
        $evidence = Join-Path $tempRoot "$safeTag.json"
        $worktrees.Add($worktree)
        Invoke-ReleaseProbe -Tag $tag -Worktree $worktree -Output $database
        Get-FixtureEvidence -DatabasePath $database -EvidencePath $evidence
        $releaseEvidence[$tag] = [pscustomobject]@{
            database = $database
            evidence = Get-Content -Raw -LiteralPath $evidence | ConvertFrom-Json
            tree = (& git -C $repoRoot rev-parse "$tag^{tree}").Trim()
        }
        Invoke-Git worktree remove --force $worktree
        $worktrees.Remove($worktree) | Out-Null
    }

    $groups = $releaseEvidence.GetEnumerator() | Group-Object { $_.Value.evidence.schema_hash }
    $profiles = [ordered]@{}
    $releases = [ordered]@{}
    $profileNumber = 0
    New-Item -ItemType Directory -Force -Path $fixtureRoot | Out-Null
    foreach ($group in $groups) {
        $profileNumber += 1
        $profileId = 'profile_{0:d3}' -f $profileNumber
        $representative = $group.Group[0]
        $profileDir = Join-Path $fixtureRoot $profileId
        New-Item -ItemType Directory -Force -Path $profileDir | Out-Null
        Copy-Item -LiteralPath $representative.Value.database -Destination (Join-Path $profileDir 'source.sqlite3') -Force
        $expected = [ordered]@{
            profile = $profileId
            schema_hash = $representative.Value.evidence.schema_hash
            fixture_sha256 = $representative.Value.evidence.fixture_hash
            table_counts = $representative.Value.evidence.table_counts
        }
        $expected | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $profileDir 'expected_manifest.json') -Encoding utf8
        $tagsForProfile = @($group.Group | ForEach-Object { $_.Key })
        $profiles[$profileId] = [ordered]@{
            schema_hash = $representative.Value.evidence.schema_hash
            fixture_sha256 = $representative.Value.evidence.fixture_hash
            fixture_status = 'generated-task-12'
            releases = $tagsForProfile
        }
        foreach ($release in $group.Group) {
            $releases[$release.Key] = [ordered]@{
                tree = $release.Value.tree
                schema_profile = $profileId
                schema_hash = $release.Value.evidence.schema_hash
            }
        }
    }

    $profileSourceRoot = Join-Path $repoRoot 'src-tauri\src\persistence\legacy_import\profiles'
    foreach ($profile in $profiles.GetEnumerator()) {
        $profileSource = Join-Path $profileSourceRoot "$($profile.Key).rs"
        if (-not (Test-Path -LiteralPath $profileSource -PathType Leaf)) {
            throw "generated profile $($profile.Key) has no explicit importer module; review and add it before accepting fixtures"
        }
        $sourceText = Get-Content -Raw -LiteralPath $profileSource
        if ($sourceText -notmatch [regex]::Escape([string]$profile.Value.schema_hash)) {
            throw "generated schema hash for $($profile.Key) does not match its explicit importer descriptor"
        }
    }

    $manifest = [ordered]@{
        version = 2
        status = 'released-schema-fixtures-generated'
        hash_contract = [ordered]@{
            schema_hash_algorithm = "sha256(ordered sqlite_schema rows joined with U+001F and LF; sqlite_% excluded)"
            fixture_hash_algorithm = 'sha256(source.sqlite3 after deterministic sanitized canary insertion and WAL checkpoint)'
            profile_grouping = 'tags share a profile only when canonical schema hashes are identical'
        }
        profiles = $profiles
        releases = $releases
    }
    $manifest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $manifestPath -Encoding utf8
}
finally {
    foreach ($worktree in $worktrees) {
        if (Test-Path -LiteralPath $worktree) {
            & git -C $repoRoot worktree remove --force $worktree 2>$null
        }
    }
    Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    if (Test-Path -LiteralPath $tempRoot) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
