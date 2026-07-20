param(
    [Parameter(Mandatory = $true)]
    [string]$DatabasePath,

    [Parameter(Mandatory = $true)]
    [string]$RequestId,

    [switch]$AllowLegacyAttemptsJson
)

$ErrorActionPreference = "Stop"

function Fail($Message) {
    Write-Error $Message
    exit 1
}

function Mask-Value($Value) {
    if ($null -eq $Value) { return $null }
    $text = [string]$Value
    $text = $text -replace '(?i)bearer\s+[A-Za-z0-9._~+/=-]+', 'Bearer [REDACTED]'
    $text = $text -replace 'sk-[A-Za-z0-9._~+/=-]{8,}', 'sk-[REDACTED]'
    $text = $text -replace '(?i)(authorization|cookie)\s*[:=]\s*[^,\s}]+', '$1=[REDACTED]'
    return $text
}

if (-not (Test-Path -LiteralPath $DatabasePath)) {
    Fail "SQLite database not found: $DatabasePath"
}

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) {
    Fail "python is required for read-only SQLite verification"
}

$allowLegacy = if ($AllowLegacyAttemptsJson) { "1" } else { "0" }
$script = @'
import json
import os
import sqlite3
import sys

db_path = sys.argv[1]
request_id = sys.argv[2]
allow_legacy_attempts_json = sys.argv[3] == "1"

connection = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
connection.row_factory = sqlite3.Row

def table_columns(table):
    return {row[1] for row in connection.execute(f"PRAGMA table_info({table})")}

required_log_columns = {
    "request_id", "status", "lifecycle_status", "terminal_kind",
    "terminal_code", "terminal_detail", "protocol_completed",
    "delivery_terminal", "selected_attempt_ordinal", "attempt_count",
    "fallback_count", "attempts_json",
}
log_columns = table_columns("request_logs")
missing = sorted(required_log_columns - log_columns)
if missing:
    print(json.dumps({"ok": False, "error": "missing request_logs columns", "missing": missing}, ensure_ascii=False))
    sys.exit(2)

attempt_columns = table_columns("request_attempts")
required_attempt_columns = {
    "request_id", "ordinal", "station_id", "station_key_id", "endpoint_revision",
    "terminal_kind", "failure_kind", "failure_blame", "retry_disposition",
    "health_effect", "output_committed", "terminal_at_ms",
}
missing = sorted(required_attempt_columns - attempt_columns)
if missing:
    print(json.dumps({"ok": False, "error": "missing request_attempts columns", "missing": missing}, ensure_ascii=False))
    sys.exit(2)

logs = list(connection.execute(
    """
    SELECT request_id, status, lifecycle_status, terminal_kind, terminal_code,
           terminal_detail, protocol_completed, delivery_terminal,
           selected_attempt_ordinal, attempt_count, fallback_count,
           completion_source, failure_source, attempts_json
      FROM request_logs
     WHERE request_id = ?
    """,
    (request_id,),
))
if len(logs) != 1:
    print(json.dumps({"ok": False, "error": "request log row count mismatch", "request_id": request_id, "row_count": len(logs)}, ensure_ascii=False))
    sys.exit(3)

log = logs[0]
if not log["terminal_kind"]:
    print(json.dumps({"ok": False, "error": "request terminal_kind is empty", "request_id": request_id}, ensure_ascii=False))
    sys.exit(4)
if log["attempts_json"] and not allow_legacy_attempts_json:
    print(json.dumps({"ok": False, "error": "legacy attempts_json is still being written", "request_id": request_id}, ensure_ascii=False))
    sys.exit(5)

attempts = list(connection.execute(
    """
    SELECT ordinal, station_id, station_key_id, endpoint_revision, terminal_kind,
           failure_kind, failure_blame, retry_disposition, health_effect,
           output_committed, terminal_at_ms
      FROM request_attempts
     WHERE request_id = ?
     ORDER BY ordinal
    """,
    (request_id,),
))
expected_attempt_count = int(log["attempt_count"] or 0)
if len(attempts) != expected_attempt_count:
    print(json.dumps({
        "ok": False,
        "error": "attempt row count does not match request log",
        "request_id": request_id,
        "attempt_rows": len(attempts),
        "attempt_count": expected_attempt_count,
    }, ensure_ascii=False))
    sys.exit(6)

ordinals = [int(row["ordinal"]) for row in attempts]
if ordinals != list(range(len(ordinals))):
    print(json.dumps({"ok": False, "error": "attempt ordinals are not contiguous", "request_id": request_id, "ordinals": ordinals}, ensure_ascii=False))
    sys.exit(7)

duplicate_attempts = connection.execute(
    """
    SELECT ordinal, COUNT(*) AS count
      FROM request_attempts
     WHERE request_id = ?
     GROUP BY ordinal
    HAVING COUNT(*) > 1
    """,
    (request_id,),
).fetchall()
if duplicate_attempts:
    print(json.dumps({"ok": False, "error": "duplicate attempt records", "request_id": request_id}, ensure_ascii=False))
    sys.exit(8)

summary = {
    "ok": True,
    "request": {
        "request_id": log["request_id"],
        "status": log["status"],
        "lifecycle_status": log["lifecycle_status"],
        "terminal_kind": log["terminal_kind"],
        "terminal_code": log["terminal_code"],
        "protocol_completed": log["protocol_completed"],
        "delivery_terminal": log["delivery_terminal"],
        "selected_attempt_ordinal": log["selected_attempt_ordinal"],
        "attempt_count": expected_attempt_count,
        "fallback_count": log["fallback_count"],
        "completion_source": log["completion_source"],
        "failure_source": log["failure_source"],
        "legacy_attempts_json_written": bool(log["attempts_json"]),
    },
    "attempts": [
        {
            "ordinal": row["ordinal"],
            "station_id_present": bool(row["station_id"]),
            "station_key_id_present": bool(row["station_key_id"]),
            "endpoint_revision": row["endpoint_revision"],
            "terminal_kind": row["terminal_kind"],
            "failure_kind": row["failure_kind"],
            "failure_blame": row["failure_blame"],
            "retry_disposition": row["retry_disposition"],
            "health_effect": row["health_effect"],
            "output_committed": row["output_committed"],
            "terminal_at_ms": row["terminal_at_ms"],
        }
        for row in attempts
    ],
}
print(json.dumps(summary, ensure_ascii=False, indent=2))
'@

$encodedScript = [Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes($script))
$bootstrap = "import base64; exec(base64.b64decode('$encodedScript').decode('utf-8'))"
$output = & $python.Source -c $bootstrap $DatabasePath $RequestId $allowLegacy
$exitCode = $LASTEXITCODE
$safeOutput = ($output | ForEach-Object { Mask-Value $_ }) -join [Environment]::NewLine
Write-Output $safeOutput
if ($exitCode -ne 0) {
    exit $exitCode
}
