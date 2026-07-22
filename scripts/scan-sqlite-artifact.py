import json
import re
import sqlite3
import sys
from pathlib import Path


def fail(message):
    print(json.dumps({"ok": False, "findings": [message]}))
    raise SystemExit(1)


def quote_identifier(identifier):
    return '"' + identifier.replace('"', '""') + '"'


def is_sensitive_column(column):
    normalized = column.lower()
    if normalized.endswith(("_id", "_ids", "_ref", "_hash", "_fingerprint", "_present", "_status", "_at")):
        return False
    return any(
        marker in normalized
        for marker in (
            "api_key",
            "local_key",
            "access_token",
            "refresh_token",
            "password",
            "cookie",
            "authorization",
            "secret",
        )
    )


def scan_value(table, column, value, allowed, canaries, findings, sensitive_context=False):
    if value is None or value == "" or value == b"":
        return

    value_bytes = value if isinstance(value, bytes) else str(value).encode("utf-8", errors="replace")
    text = value_bytes.decode("utf-8", errors="replace")
    coordinate = f"{table}.{column}"

    for canary in canaries:
        if canary.encode("utf-8") in value_bytes:
            findings.append(f"{coordinate}: seeded secret canary is present")

    if re.search(r"[A-Za-z]:[\\/]Users[\\/][^\\/\x00\r\n]+[\\/]", text, re.IGNORECASE):
        findings.append(f"{coordinate}: absolute Windows user path is present")
    if re.search(r"/home/[^/\x00\r\n]+/", text):
        findings.append(f"{coordinate}: absolute Unix user path is present")

    high_confidence_patterns = (
        ("private key", r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
        ("OpenAI-style key", r"\bsk-(?:(?:proj|svcacct)-)?[A-Za-z0-9]{32,}\b"),
        ("GitHub token", r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})\b"),
        ("AWS access key", r"\bAKIA[0-9A-Z]{16}\b"),
        ("bearer token", r"(?i)\bbearer\s+[A-Za-z0-9._~+/]{32,}={0,2}"),
    )
    for label, pattern in high_confidence_patterns:
        if re.search(pattern, text):
            findings.append(f"{coordinate}: {label} is present")

    allowed_key = (table, column, text)
    if (sensitive_context or is_sensitive_column(column)) and allowed_key not in allowed:
        findings.append(f"{coordinate}: non-empty sensitive column value is not allowlisted")


def main():
    if len(sys.argv) != 2:
        fail("usage: scan-sqlite-artifact.py <database>")

    database = Path(sys.argv[1]).resolve()
    config = json.load(sys.stdin)
    allowed = {
        (entry["table"], entry["column"], entry["value"])
        for entry in config.get("allowedSensitiveValues", [])
    }
    canaries = [value for value in config.get("canaries", []) if value]
    findings = []

    try:
        connection = sqlite3.connect(f"{database.as_uri()}?mode=ro", uri=True)
        connection.execute("PRAGMA query_only = ON")
        integrity_rows = [row[0] for row in connection.execute("PRAGMA integrity_check")]
        if integrity_rows != ["ok"]:
            findings.append("database: integrity_check failed")

        tables = [
            row[0]
            for row in connection.execute(
                "SELECT name FROM sqlite_schema "
                "WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            )
        ]
        scanned_columns = 0
        scanned_rows = 0
        for table in tables:
            quoted_table = quote_identifier(table)
            columns = [row[1] for row in connection.execute(f"PRAGMA table_info({quoted_table})")]
            scanned_columns += len(columns)
            for row in connection.execute(f"SELECT * FROM {quoted_table}"):
                scanned_rows += 1
                row_values = dict(zip(columns, row))
                semantic_name = ""
                for semantic_column in ("key", "name"):
                    candidate = row_values.get(semantic_column)
                    if isinstance(candidate, str):
                        semantic_name = candidate
                        break
                for column, value in zip(columns, row):
                    if isinstance(value, (str, bytes)):
                        scan_value(
                            table,
                            column,
                            value,
                            allowed,
                            canaries,
                            findings,
                            sensitive_context=(column.lower() == "value" and is_sensitive_column(semantic_name)),
                        )
        connection.close()
    except (OSError, sqlite3.Error, ValueError) as error:
        fail(f"database: unable to inspect SQLite safely: {error}")

    result = {
        "ok": not findings,
        "findings": findings,
        "tables": len(tables),
        "columns": scanned_columns,
        "rows": scanned_rows,
    }
    print(json.dumps(result))
    raise SystemExit(0 if result["ok"] else 1)


if __name__ == "__main__":
    main()
