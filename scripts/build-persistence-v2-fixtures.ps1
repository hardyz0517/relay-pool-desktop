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

request_lifecycle_columns = {
    "endpoint", "terminal_kind", "terminal_code", "terminal_detail",
    "protocol_completed", "delivery_terminal", "selected_attempt_ordinal", "terminal_at_ms",
}

def normalize_identifier(value):
    return (value or "").strip().lower()

def normalize_type(value):
    return " ".join((value or "").split()).upper()

def record(kind, *fields):
    return "\x1f".join((kind, *(str(field) for field in fields)))

def hash_records(records):
    return hashlib.sha256("\n".join(sorted(records)).encode("utf-8")).hexdigest()

def semantic_fingerprint():
    base_records = []
    capability_records = []
    request_attempts_present = False
    lifecycle_columns_present = set()
    objects = connection.execute("""
        SELECT type, name, tbl_name
        FROM sqlite_schema
        WHERE name NOT LIKE 'sqlite_%'
          AND type IN ('table', 'view', 'trigger')
        ORDER BY type, name, tbl_name
    """).fetchall()
    for object_type, name, table_name in objects:
        normalized_name = normalize_identifier(name)
        normalized_table = normalize_identifier(table_name)
        is_attempts = object_type == "table" and normalized_name == "request_attempts"
        request_attempts_present = request_attempts_present or is_attempts
        target = capability_records if is_attempts else base_records
        target.append(record("object", object_type, normalized_name, normalized_table))
        if object_type != "table":
            continue

        for column in connection.execute("""
            SELECT name, type, \"notnull\", pk, hidden
            FROM pragma_table_xinfo(?)
        """, (name,)):
            column_name, column_type, not_null, primary_key_position, hidden = column
            normalized_column = normalize_identifier(column_name)
            column_record = record(
                "column", normalized_name, normalized_column, normalize_type(column_type),
                not_null, primary_key_position, hidden,
            )
            is_lifecycle_column = (
                normalized_name == "request_logs"
                and normalized_column in request_lifecycle_columns
            )
            if is_attempts or is_lifecycle_column:
                capability_records.append(column_record)
                if is_lifecycle_column:
                    lifecycle_columns_present.add(normalized_column)
            else:
                base_records.append(column_record)

        for foreign_key in connection.execute("""
            SELECT \"table\", \"from\", \"to\", on_update, on_delete, \"match\"
            FROM pragma_foreign_key_list(?)
        """, (name,)):
            referenced_table, source_column, referenced_column, on_update, on_delete, match_kind = foreign_key
            foreign_key_record = record(
                "foreign_key", normalized_name, normalize_identifier(referenced_table),
                normalize_identifier(source_column), normalize_identifier(referenced_column),
                on_update.upper(), on_delete.upper(), match_kind.upper(),
            )
            (capability_records if is_attempts else base_records).append(foreign_key_record)

        for index_name, unique, origin, partial in connection.execute("""
            SELECT name, \"unique\", origin, partial FROM pragma_index_list(?)
        """, (name,)):
            if not unique or origin == "pk":
                continue
            fields = [normalized_name, origin, partial]
            for column_name, descending, collation, is_key in connection.execute("""
                SELECT COALESCE(name, ''), \"desc\", COALESCE(coll, ''), \"key\"
                FROM pragma_index_xinfo(?) WHERE \"key\" = 1 ORDER BY seqno
            """, (index_name,)):
                fields.append(
                    f"{normalize_identifier(column_name)}:{descending}:{collation.upper()}:{is_key}"
                )
            (capability_records if is_attempts else base_records).append(record("unique", *fields))

    has_capability_markers = request_attempts_present or bool(lifecycle_columns_present)
    return {
        "semantic_base_schema_hash": hash_records(base_records),
        "semantic_base_record_count": len(base_records),
        "request_lifecycle_schema_hash": (
            hash_records(capability_records) if has_capability_markers else None
        ),
        "request_lifecycle_record_count": len(capability_records),
    }

schema_rows = connection.execute("""
    SELECT type, name, tbl_name, COALESCE(sql, '')
    FROM sqlite_schema
    WHERE name NOT LIKE 'sqlite_%'
    ORDER BY type ASC, name ASC, tbl_name ASC
""").fetchall()
canonical = "\n".join("\x1f".join(row) for row in schema_rows).encode("utf-8")
raw_schema_hash = hashlib.sha256(canonical).hexdigest()
fingerprint = semantic_fingerprint()
target_tables = {
    "settings", "secrets", "stations", "station_credentials", "station_keys",
    "remote_station_keys", "station_key_capabilities", "model_aliases", "collector_runs",
    "collector_snapshots", "station_group_bindings", "group_rate_records", "pricing_rules",
    "model_base_prices", "balance_snapshots", "channel_monitor_request_templates",
    "channel_monitors", "channel_monitor_runs", "request_logs", "request_attempts", "station_key_health",
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
    json.dump({
        "raw_schema_hash": raw_schema_hash,
        **fingerprint,
        "fixture_hash": fixture_hash,
        "table_counts": table_counts,
    }, output, indent=2)
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

    $existingManifest = if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
        Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
    } else {
        $null
    }
    $groups = $releaseEvidence.GetEnumerator() | Group-Object { $_.Value.evidence.semantic_base_schema_hash }
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
            raw_schema_hash = $representative.Value.evidence.raw_schema_hash
            semantic_base_schema_hash = $representative.Value.evidence.semantic_base_schema_hash
            request_lifecycle_schema_hash = $representative.Value.evidence.request_lifecycle_schema_hash
            fixture_sha256 = $representative.Value.evidence.fixture_hash
            table_counts = $representative.Value.evidence.table_counts
        }
        $expected | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $profileDir 'expected_manifest.json') -Encoding utf8
        $tagsForProfile = @($group.Group | ForEach-Object { $_.Key })
        $acceptedCapabilities = @(
            $group.Group | ForEach-Object { $_.Value.evidence.request_lifecycle_schema_hash } | Where-Object { $_ }
        )
        if ($null -ne $existingManifest) {
            $existingProfileProperty = $existingManifest.profiles.PSObject.Properties[$profileId]
            if ($null -ne $existingProfileProperty) {
                $acceptedCapabilities += @($existingProfileProperty.Value.accepted_capabilities.request_lifecycle)
            }
        }
        $acceptedCapabilities = @($acceptedCapabilities | Sort-Object -Unique)
        $profiles[$profileId] = [ordered]@{
            semantic_base_schema_hash = $representative.Value.evidence.semantic_base_schema_hash
            accepted_capabilities = [ordered]@{
                request_lifecycle = $acceptedCapabilities
            }
            fixture_raw_schema_hash = $representative.Value.evidence.raw_schema_hash
            fixture_sha256 = $representative.Value.evidence.fixture_hash
            fixture_status = 'generated'
            releases = $tagsForProfile
        }
        foreach ($release in $group.Group) {
            $releases[$release.Key] = [ordered]@{
                tree = $release.Value.tree
                schema_profile = $profileId
                raw_schema_hash = $release.Value.evidence.raw_schema_hash
                semantic_base_schema_hash = $release.Value.evidence.semantic_base_schema_hash
                request_lifecycle_schema_hash = $release.Value.evidence.request_lifecycle_schema_hash
                fixture_sha256 = $release.Value.evidence.fixture_hash
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
        if ($sourceText -notmatch [regex]::Escape([string]$profile.Value.semantic_base_schema_hash)) {
            throw "generated semantic base hash for $($profile.Key) does not match its explicit importer descriptor"
        }
        foreach ($capabilityHash in $profile.Value.accepted_capabilities.request_lifecycle) {
            if ($sourceText -notmatch [regex]::Escape([string]$capabilityHash)) {
                throw "accepted request lifecycle hash for $($profile.Key) does not match its explicit importer descriptor"
            }
        }
    }

    $manifest = [ordered]@{
        version = 3
        status = 'released-schema-fixtures-generated'
        hash_contract = [ordered]@{
            raw_schema_hash_algorithm = "sha256(ordered sqlite_schema DDL rows joined with U+001F and LF; provenance only)"
            semantic_schema_hash_algorithm = "v1 sha256(sorted object, column, foreign-key, and unique-constraint records; DDL text, column order, defaults, and non-unique indexes excluded)"
            capability_contract = "request_attempts and eight request_logs lifecycle columns are one fail-closed optional capability"
            fixture_hash_algorithm = 'sha256(source.sqlite3 after deterministic sanitized canary insertion and WAL checkpoint)'
            profile_grouping = 'tags share a profile only when semantic base schema hashes are identical'
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
