# Real Endpoint Ping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real endpoint PING path that measures station base-url reachability without spending model tokens, and keep it separate from conversation/model latency.

**Architecture:** Store endpoint health at the `Station` level in a dedicated `station_endpoint_health` table because every key under the same station shares the same base URL. Keep `station_key_health.avg_latency_ms` as conversation/model probe latency. The channel status card reads both signals: conversation latency from request logs or key health, endpoint PING from station endpoint health.

**Tech Stack:** Tauri 2, Rust, rusqlite, reqwest blocking client, React, TypeScript, Vite, existing script-based Node tests.

---

## Current State And Constraints

- Current wrong behavior: `ChannelStatusTab` displays `station_key_health.avgLatencyMs` as `端点 PING`, but that field is updated by monitor probes, manual key connectivity tests, and proxy requests.
- Desired behavior: `端点 PING` must be a station-level endpoint reachability measurement that does not call `/v1/chat/completions` or `/v1/responses` and does not spend tokens.
- `对话延迟` remains key-level model/conversation latency.
- Do not use ICMP ping. Use HTTP/TCP-level endpoint probing inside Rust so it works consistently in the desktop app.
- The workspace is dirty. Execution must first isolate work in a branch/worktree and must not revert unrelated files.
- Do not stage with `git add .` or `git add -A`. Stage exact paths only if committing.

## File Structure

- Modify `src-tauri/src/models/stations.rs`
  - Add `StationEndpointHealth` and `EndpointPingResult` Rust structs.
- Modify `src-tauri/src/services/database.rs`
  - Create `station_endpoint_health`.
  - Add read/upsert helpers.
  - Join endpoint health into key-pool rows.
  - Add tests for schema, upsert, and key-pool join.
- Create `src-tauri/src/services/endpoint_ping.rs`
  - Owns the non-token endpoint probe implementation.
  - Normalizes URLs and measures HTTP reachability.
- Modify `src-tauri/src/commands/mod.rs`
  - Add `ping_station_endpoint` and `list_station_endpoint_health` commands.
- Modify `src-tauri/src/lib.rs`
  - Register new commands.
- Modify `src/lib/types/stations.ts`
  - Add `StationEndpointHealth`.
- Modify `src/lib/types/stationKeys.ts`
  - Add endpoint ping fields to `KeyPoolItem`.
- Modify `src/lib/api/stations.ts`
  - Add frontend API calls and browser fallback memory.
- Modify `src/lib/api/stationKeys.ts`
  - Populate fallback endpoint ping fields.
- Modify `src/features/channels/channelStatusViewModel.ts`
  - Extend latency helper to accept real endpoint ping.
- Modify `src/features/channels/ChannelStatusTab.tsx`
  - Fetch station endpoint health, map it into cards, and render true `端点 PING`.
- Modify `scripts/channel-status-view-model.test.mjs`
  - Add tests proving endpoint PING does not reuse conversation latency.

## Naming

Use these exact names consistently:

- Rust struct: `StationEndpointHealth`
- Rust ping result struct: `EndpointPingResult`
- Database table: `station_endpoint_health`
- DB fields: `station_id`, `status`, `latency_ms`, `checked_at`, `error_summary`, `updated_at`
- Tauri commands: `list_station_endpoint_health`, `ping_station_endpoint`
- TypeScript type: `StationEndpointHealth`
- `KeyPoolItem` fields: `endpointPingStatus`, `endpointPingMs`, `endpointPingCheckedAt`, `endpointPingError`

### Task 1: Isolate The Work And Capture Baseline

**Files:**
- No code files.

- [ ] **Step 1: Check current status**

Run:

```powershell
git status --short
```

Expected: Output may include existing unrelated modified files. Do not revert them.

- [ ] **Step 2: Create or switch to an isolated branch**

Run:

```powershell
git switch -c codex/real-endpoint-ping
```

Expected: New branch created. If the branch already exists, use:

```powershell
git switch codex/real-endpoint-ping
```

- [ ] **Step 3: Record the exact pre-change dirty paths**

Run:

```powershell
git status --short > $env:TEMP\relay-pool-real-ping-baseline-status.txt
Get-Content $env:TEMP\relay-pool-real-ping-baseline-status.txt
```

Expected: The baseline file lists unrelated work so later review can distinguish it from this task.

### Task 2: Add Endpoint Health Data Model And Schema

**Files:**
- Modify: `src-tauri/src/models/stations.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing database tests**

Add these tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/services/database.rs`.

```rust
#[test]
fn station_endpoint_health_defaults_to_unchecked() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = database
        .create_station(CreateStationInput {
            name: "endpoint health relay".to_string(),
            station_type: "sub2api".to_string(),
            base_url: "https://relay.example.test".to_string(),
            api_key: "sk-test".to_string(),
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            note: None,
        })
        .expect("station");

    let health = database
        .get_station_endpoint_health(station.id.clone())
        .expect("endpoint health");

    assert_eq!(health.station_id, station.id);
    assert_eq!(health.status, "unchecked");
    assert_eq!(health.latency_ms, None);
    assert_eq!(health.checked_at, None);
    assert_eq!(health.error_summary, None);
}

#[test]
fn station_endpoint_health_upsert_updates_existing_row() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = database
        .create_station(CreateStationInput {
            name: "endpoint ping relay".to_string(),
            station_type: "openai-compatible".to_string(),
            base_url: "https://relay.example.test".to_string(),
            api_key: "sk-test".to_string(),
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            note: None,
        })
        .expect("station");

    database
        .upsert_station_endpoint_health(
            &station.id,
            "success",
            Some(42),
            "1000",
            None,
        )
        .expect("first upsert");
    database
        .upsert_station_endpoint_health(
            &station.id,
            "failed",
            None,
            "2000",
            Some("HTTP 502"),
        )
        .expect("second upsert");

    let health = database
        .get_station_endpoint_health(station.id)
        .expect("endpoint health");

    assert_eq!(health.status, "failed");
    assert_eq!(health.latency_ms, None);
    assert_eq!(health.checked_at.as_deref(), Some("2000"));
    assert_eq!(health.error_summary.as_deref(), Some("HTTP 502"));
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml station_endpoint_health_defaults_to_unchecked station_endpoint_health_upsert_updates_existing_row
```

Expected: FAIL because `get_station_endpoint_health` and `upsert_station_endpoint_health` do not exist.

- [ ] **Step 3: Add Rust model structs**

In `src-tauri/src/models/stations.rs`, append:

```rust
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationEndpointHealth {
    pub station_id: String,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub checked_at: Option<String>,
    pub error_summary: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointPingResult {
    pub station_id: String,
    pub ok: bool,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub checked_at: String,
    pub error_summary: Option<String>,
}
```

- [ ] **Step 4: Import model in database service**

In `src-tauri/src/services/database.rs`, update the `models` import block to include `StationEndpointHealth`:

```rust
stations::{CreateStationInput, Station, StationEndpointHealth, UpdateStationInput},
```

If the import block shape differs, preserve the local ordering and add only `StationEndpointHealth`.

- [ ] **Step 5: Create table in schema setup**

In `initialize_schema` near the existing `station_key_health` table, add:

```rust
connection.execute_batch(
    "
    CREATE TABLE IF NOT EXISTS station_endpoint_health (
        station_id TEXT PRIMARY KEY,
        status TEXT NOT NULL CHECK(status IN ('unchecked', 'success', 'failed')),
        latency_ms INTEGER CHECK(latency_ms IS NULL OR latency_ms >= 0),
        checked_at TEXT,
        error_summary TEXT,
        updated_at TEXT NOT NULL,
        FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
    );
    ",
)?;
```

If `initialize_schema` already uses one large `execute_batch`, insert the SQL inside that batch immediately after `station_key_health`.

- [ ] **Step 6: Add database methods on `impl AppDatabase`**

In `impl AppDatabase`, near station health methods, add:

```rust
pub fn list_station_endpoint_health(&self) -> Result<Vec<StationEndpointHealth>, String> {
    let connection = self.connection()?;
    list_station_endpoint_health_from_connection(&connection)
}

pub fn get_station_endpoint_health(
    &self,
    station_id: String,
) -> Result<StationEndpointHealth, String> {
    let connection = self.connection()?;
    station_endpoint_health_by_id(&connection, &station_id)
}

pub fn upsert_station_endpoint_health(
    &self,
    station_id: &str,
    status: &str,
    latency_ms: Option<i64>,
    checked_at: &str,
    error_summary: Option<&str>,
) -> Result<StationEndpointHealth, String> {
    let connection = self.connection()?;
    upsert_station_endpoint_health_in_connection(
        &connection,
        station_id,
        status,
        latency_ms,
        checked_at,
        error_summary,
    )
}
```

- [ ] **Step 7: Add helper functions**

In `src-tauri/src/services/database.rs`, near `station_key_health_by_id`, add:

```rust
fn list_station_endpoint_health_from_connection(
    connection: &Connection,
) -> Result<Vec<StationEndpointHealth>, String> {
    let mut statement = connection
        .prepare(
            "SELECT station_id, status, latency_ms, checked_at, error_summary, updated_at
               FROM station_endpoint_health
              ORDER BY updated_at DESC",
        )
        .map_err(|error| format!("读取站点端点健康失败: {error}"))?;
    let rows = statement
        .query_map([], row_to_station_endpoint_health)
        .map_err(|error| format!("读取站点端点健康失败: {error}"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| format!("读取站点端点健康失败: {error}"))?;
    Ok(rows)
}

fn station_endpoint_health_by_id(
    connection: &Connection,
    station_id: &str,
) -> Result<StationEndpointHealth, String> {
    validate_station_exists(connection, station_id)?;
    let row = connection
        .query_row(
            "SELECT station_id, status, latency_ms, checked_at, error_summary, updated_at
               FROM station_endpoint_health
              WHERE station_id = ?1",
            params![station_id],
            row_to_station_endpoint_health,
        )
        .optional()
        .map_err(|error| format!("读取站点端点健康失败: {error}"))?;
    Ok(row.unwrap_or_else(|| default_station_endpoint_health(station_id)))
}

fn row_to_station_endpoint_health(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StationEndpointHealth> {
    Ok(StationEndpointHealth {
        station_id: row.get(0)?,
        status: row.get(1)?,
        latency_ms: row.get(2)?,
        checked_at: row.get(3)?,
        error_summary: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn default_station_endpoint_health(station_id: &str) -> StationEndpointHealth {
    StationEndpointHealth {
        station_id: station_id.to_string(),
        status: "unchecked".to_string(),
        latency_ms: None,
        checked_at: None,
        error_summary: None,
        updated_at: now_string(),
    }
}

fn upsert_station_endpoint_health_in_connection(
    connection: &Connection,
    station_id: &str,
    status: &str,
    latency_ms: Option<i64>,
    checked_at: &str,
    error_summary: Option<&str>,
) -> Result<StationEndpointHealth, String> {
    validate_station_exists(connection, station_id)?;
    if !matches!(status, "unchecked" | "success" | "failed") {
        return Err("Station endpoint health status must be unchecked, success, or failed".to_string());
    }
    if latency_ms.is_some_and(|value| value < 0) {
        return Err("Station endpoint latency_ms must be non-negative".to_string());
    }
    let updated_at = now_string();
    connection
        .execute(
            "INSERT INTO station_endpoint_health (
                station_id, status, latency_ms, checked_at, error_summary, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(station_id) DO UPDATE SET
                status = excluded.status,
                latency_ms = excluded.latency_ms,
                checked_at = excluded.checked_at,
                error_summary = excluded.error_summary,
                updated_at = excluded.updated_at",
            params![
                station_id,
                status,
                latency_ms,
                checked_at,
                redact_optional_text(error_summary.map(str::to_string)),
                updated_at,
            ],
        )
        .map_err(|error| format!("保存站点端点健康失败: {error}"))?;
    station_endpoint_health_by_id(connection, station_id)
}
```

- [ ] **Step 8: Run tests and verify they pass**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml station_endpoint_health_defaults_to_unchecked station_endpoint_health_upsert_updates_existing_row
```

Expected: PASS.

- [ ] **Step 9: Commit Task 2**

Stage exact paths:

```powershell
git add -- src-tauri/src/models/stations.rs src-tauri/src/services/database.rs
git commit -m "feat: add station endpoint health storage"
```

Expected: Commit succeeds. If the user asked not to commit in this session, skip this step and report exact modified paths.

### Task 3: Implement Non-Token Endpoint Ping Service

**Files:**
- Create: `src-tauri/src/services/endpoint_ping.rs`
- Modify: `src-tauri/src/services/mod.rs` if it exists and declares service modules
- Modify: `src-tauri/src/services/database.rs`
- Test: `src-tauri/src/services/endpoint_ping.rs`

- [ ] **Step 1: Write failing endpoint ping tests**

Create `src-tauri/src/services/endpoint_ping.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    fn spawn_endpoint(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 512];
            let _ = stream.read(&mut buffer);
            stream.write_all(response.as_bytes()).expect("write");
        });
        format!("http://{addr}")
    }

    #[test]
    fn endpoint_ping_uses_http_head_without_token_path() {
        let base_url = spawn_endpoint(
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
        );

        let result = ping_station_endpoint(&base_url, Duration::from_secs(2));

        assert!(result.ok);
        assert_eq!(result.status, "success");
        assert!(result.latency_ms.is_some());
        assert_eq!(result.error_summary, None);
    }

    #[test]
    fn endpoint_ping_reports_http_failure_without_model_request() {
        let base_url = spawn_endpoint(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n",
        );

        let result = ping_station_endpoint(&base_url, Duration::from_secs(2));

        assert!(!result.ok);
        assert_eq!(result.status, "failed");
        assert_eq!(result.latency_ms, None);
        assert!(result.error_summary.unwrap().contains("HTTP 503"));
    }

    #[test]
    fn endpoint_ping_normalizes_v1_base_url_to_root() {
        let url = endpoint_ping_url("https://relay.example.com/v1/");

        assert_eq!(url, "https://relay.example.com/");
    }
}
```

- [ ] **Step 2: Register module enough for compile and verify tests fail**

If `src-tauri/src/services/mod.rs` exists, add:

```rust
pub mod endpoint_ping;
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml endpoint_ping_
```

Expected: FAIL because `ping_station_endpoint` and `endpoint_ping_url` do not exist.

- [ ] **Step 3: Implement endpoint ping service**

Replace `src-tauri/src/services/endpoint_ping.rs` with:

```rust
use reqwest::blocking::Client;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct EndpointPingProbeResult {
    pub ok: bool,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub error_summary: Option<String>,
}

pub fn ping_station_endpoint(base_url: &str, timeout: Duration) -> EndpointPingProbeResult {
    let url = endpoint_ping_url(base_url);
    let started = Instant::now();
    let client = match Client::builder().timeout(timeout).build() {
        Ok(client) => client,
        Err(error) => {
            return EndpointPingProbeResult {
                ok: false,
                status: "failed".to_string(),
                latency_ms: None,
                error_summary: Some(short_ping_error(&format!("HTTP client: {error}"))),
            };
        }
    };

    match client.head(&url).send().or_else(|_| client.get(&url).send()) {
        Ok(response) if response.status().is_success() || response.status().is_redirection() => {
            EndpointPingProbeResult {
                ok: true,
                status: "success".to_string(),
                latency_ms: Some(started.elapsed().as_millis().min(i64::MAX as u128) as i64),
                error_summary: None,
            }
        }
        Ok(response) => EndpointPingProbeResult {
            ok: false,
            status: "failed".to_string(),
            latency_ms: None,
            error_summary: Some(short_ping_error(&format!("HTTP {}", response.status().as_u16()))),
        },
        Err(error) => EndpointPingProbeResult {
            ok: false,
            status: "failed".to_string(),
            latency_ms: None,
            error_summary: Some(short_ping_error(&error.to_string())),
        },
    }
}

pub fn endpoint_ping_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_suffix("/v1") {
        return format!("{}/", stripped.trim_end_matches('/'));
    }
    format!("{trimmed}/")
}

fn short_ping_error(message: &str) -> String {
    const MAX_LEN: usize = 180;
    let trimmed = message.trim();
    if trimmed.chars().count() <= MAX_LEN {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX_LEN).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    fn spawn_endpoint(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 512];
            let _ = stream.read(&mut buffer);
            stream.write_all(response.as_bytes()).expect("write");
        });
        format!("http://{addr}")
    }

    #[test]
    fn endpoint_ping_uses_http_head_without_token_path() {
        let base_url = spawn_endpoint(
            "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
        );

        let result = ping_station_endpoint(&base_url, Duration::from_secs(2));

        assert!(result.ok);
        assert_eq!(result.status, "success");
        assert!(result.latency_ms.is_some());
        assert_eq!(result.error_summary, None);
    }

    #[test]
    fn endpoint_ping_reports_http_failure_without_model_request() {
        let base_url = spawn_endpoint(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n",
        );

        let result = ping_station_endpoint(&base_url, Duration::from_secs(2));

        assert!(!result.ok);
        assert_eq!(result.status, "failed");
        assert_eq!(result.latency_ms, None);
        assert!(result.error_summary.unwrap().contains("HTTP 503"));
    }

    #[test]
    fn endpoint_ping_normalizes_v1_base_url_to_root() {
        let url = endpoint_ping_url("https://relay.example.com/v1/");

        assert_eq!(url, "https://relay.example.com/");
    }
}
```

- [ ] **Step 4: Run endpoint ping tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml endpoint_ping_
```

Expected: PASS.

- [ ] **Step 5: Commit Task 3**

Stage exact paths:

```powershell
git add -- src-tauri/src/services/endpoint_ping.rs src-tauri/src/services/mod.rs
git commit -m "feat: add non-token endpoint ping probe"
```

If `src-tauri/src/services/mod.rs` does not exist and no module registry file was changed, omit it from `git add`.

### Task 4: Expose Endpoint Ping Commands

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write failing command test**

In `src-tauri/src/commands/mod.rs` tests module, add:

```rust
#[test]
fn ping_station_endpoint_command_records_station_endpoint_health() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = database
        .create_station(CreateStationInput {
            name: "endpoint command relay".to_string(),
            station_type: "openai-compatible".to_string(),
            base_url: "http://127.0.0.1:9".to_string(),
            api_key: "sk-test".to_string(),
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            note: None,
        })
        .expect("station");

    let result = ping_station_endpoint_for_tests(&database, station.id.clone(), 1)
        .expect("ping result");
    let health = database
        .get_station_endpoint_health(station.id)
        .expect("endpoint health");

    assert_eq!(result.ok, false);
    assert_eq!(health.status, "failed");
    assert!(health.checked_at.is_some());
    assert!(health.error_summary.is_some());
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml ping_station_endpoint_command_records_station_endpoint_health
```

Expected: FAIL because `ping_station_endpoint_for_tests` does not exist.

- [ ] **Step 3: Add command imports**

In `src-tauri/src/commands/mod.rs`, add imports:

```rust
use crate::models::stations::{EndpointPingResult, StationEndpointHealth};
use crate::services::endpoint_ping::ping_station_endpoint;
use std::time::Duration;
```

If `Duration` is already imported, reuse the existing import.

- [ ] **Step 4: Add commands**

In `src-tauri/src/commands/mod.rs`, near other station commands, add:

```rust
#[tauri::command]
pub fn list_station_endpoint_health(
    database: State<'_, AppDatabase>,
) -> Result<Vec<StationEndpointHealth>, String> {
    database.list_station_endpoint_health()
}

#[tauri::command]
pub fn ping_station_endpoint_command(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<EndpointPingResult, String> {
    ping_station_endpoint_for_tests(&database, station_id, 5)
}

fn ping_station_endpoint_for_tests(
    database: &AppDatabase,
    station_id: String,
    timeout_seconds: u64,
) -> Result<EndpointPingResult, String> {
    let station = database
        .list_stations()?
        .into_iter()
        .find(|station| station.id == station_id)
        .ok_or_else(|| "中转站不存在，无法测试端点 PING".to_string())?;
    let checked_at = now_millis_for_services().to_string();
    let probe = ping_station_endpoint(
        &station.base_url,
        Duration::from_secs(timeout_seconds.max(1)),
    );
    let health = database.upsert_station_endpoint_health(
        &station.id,
        &probe.status,
        probe.latency_ms,
        &checked_at,
        probe.error_summary.as_deref(),
    )?;
    Ok(EndpointPingResult {
        station_id: health.station_id,
        ok: probe.ok,
        status: health.status,
        latency_ms: health.latency_ms,
        checked_at: health.checked_at.unwrap_or(checked_at),
        error_summary: health.error_summary,
    })
}
```

- [ ] **Step 5: Register commands in Tauri**

In `src-tauri/src/lib.rs`, add both command names to `tauri::generate_handler!`:

```rust
commands::list_station_endpoint_health,
commands::ping_station_endpoint_command,
```

- [ ] **Step 6: Run command test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml ping_station_endpoint_command_records_station_endpoint_health
```

Expected: PASS.

- [ ] **Step 7: Run Rust compile check**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected: PASS. Existing dead-code warnings are acceptable if they match the current repo baseline.

- [ ] **Step 8: Commit Task 4**

Stage exact paths:

```powershell
git add -- src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: expose station endpoint ping commands"
```

### Task 5: Surface Endpoint Ping In Frontend Types And APIs

**Files:**
- Modify: `src/lib/types/stations.ts`
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/lib/api/stations.ts`
- Modify: `src/lib/api/stationKeys.ts`

- [ ] **Step 1: Write failing TypeScript compile expectation**

Modify no production code yet. Add this temporary type usage to the bottom of `src/lib/api/stations.ts`:

```ts
const __endpointHealthTypeCheck: import("@/lib/types/stations").StationEndpointHealth | null = null;
void __endpointHealthTypeCheck;
```

- [ ] **Step 2: Run TypeScript and verify it fails**

Run:

```powershell
pnpm.cmd exec tsc --noEmit --pretty false
```

Expected: FAIL because `StationEndpointHealth` is not exported.

- [ ] **Step 3: Add frontend type**

In `src/lib/types/stations.ts`, add:

```ts
export type StationEndpointHealth = {
  stationId: string;
  status: "unchecked" | "success" | "failed";
  latencyMs: number | null;
  checkedAt: string | null;
  errorSummary: string | null;
  updatedAt: string;
};
```

- [ ] **Step 4: Extend key-pool item type**

In `src/lib/types/stationKeys.ts`, add fields to `KeyPoolItem`:

```ts
  endpointPingStatus: "unchecked" | "success" | "failed";
  endpointPingMs: number | null;
  endpointPingCheckedAt: string | null;
  endpointPingError: string | null;
```

- [ ] **Step 5: Add frontend station API functions**

In `src/lib/api/stations.ts`, import `StationEndpointHealth` and add:

```ts
const memoryEndpointHealth = new Map<string, StationEndpointHealth>();

export function listStationEndpointHealth() {
  return invoke<StationEndpointHealth[]>("list_station_endpoint_health").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return Array.from(memoryEndpointHealth.values());
    }
    throw error;
  });
}

export function pingStationEndpoint(stationId: string) {
  return invoke<StationEndpointHealth>("ping_station_endpoint_command", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const now = new Date().toISOString();
      const health: StationEndpointHealth = {
        stationId,
        status: "failed",
        latencyMs: null,
        checkedAt: now,
        errorSummary: "浏览器预览环境没有 Tauri 后端，无法执行真实端点 PING。",
        updatedAt: now,
      };
      memoryEndpointHealth.set(stationId, health);
      return health;
    }
    throw error;
  });
}
```

If `stations.ts` already has a memory map section, place `memoryEndpointHealth` next to the existing memory state.

- [ ] **Step 6: Populate station key fallback fields**

In `src/lib/api/stationKeys.ts`, update every `KeyPoolItem` construction path to include:

```ts
endpointPingStatus: "unchecked",
endpointPingMs: null,
endpointPingCheckedAt: null,
endpointPingError: null,
```

- [ ] **Step 7: Remove temporary type usage**

Delete:

```ts
const __endpointHealthTypeCheck: import("@/lib/types/stations").StationEndpointHealth | null = null;
void __endpointHealthTypeCheck;
```

- [ ] **Step 8: Run TypeScript check**

Run:

```powershell
pnpm.cmd exec tsc --noEmit --pretty false
```

Expected: PASS.

- [ ] **Step 9: Commit Task 5**

Stage exact paths:

```powershell
git add -- src/lib/types/stations.ts src/lib/types/stationKeys.ts src/lib/api/stations.ts src/lib/api/stationKeys.ts
git commit -m "feat: add endpoint ping frontend types"
```

### Task 6: Join Endpoint Health Into Key Pool Rows

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/station_keys.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing key-pool join test**

In `src-tauri/src/services/database.rs` tests module, add:

```rust
#[test]
fn key_pool_items_include_station_endpoint_ping() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = database
        .create_station(CreateStationInput {
            name: "endpoint ping in key pool".to_string(),
            station_type: "openai-compatible".to_string(),
            base_url: "https://relay.example.test".to_string(),
            api_key: "sk-test".to_string(),
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            note: None,
        })
        .expect("station");
    database
        .upsert_station_endpoint_health(
            &station.id,
            "success",
            Some(37),
            "1000",
            None,
        )
        .expect("endpoint health");

    let item = database
        .list_key_pool_items()
        .expect("key pool")
        .into_iter()
        .find(|item| item.station_id == station.id)
        .expect("station key item");

    assert_eq!(item.endpoint_ping_status, "success");
    assert_eq!(item.endpoint_ping_ms, Some(37));
    assert_eq!(item.endpoint_ping_checked_at.as_deref(), Some("1000"));
    assert_eq!(item.endpoint_ping_error, None);
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml key_pool_items_include_station_endpoint_ping
```

Expected: FAIL because `KeyPoolItem` lacks endpoint ping fields.

- [ ] **Step 3: Extend Rust key-pool model**

In `src-tauri/src/models/station_keys.rs`, add fields to `KeyPoolItem`:

```rust
pub endpoint_ping_status: String,
pub endpoint_ping_ms: Option<i64>,
pub endpoint_ping_checked_at: Option<String>,
pub endpoint_ping_error: Option<String>,
```

- [ ] **Step 4: Join endpoint health in key-pool SQL**

In `src-tauri/src/services/database.rs`, update `list_key_pool_items` SQL:

1. Add selected columns near station health columns:

```sql
COALESCE(eh.status, 'unchecked') AS endpoint_ping_status,
eh.latency_ms AS endpoint_ping_ms,
eh.checked_at AS endpoint_ping_checked_at,
eh.error_summary AS endpoint_ping_error
```

2. Add join:

```sql
LEFT JOIN station_endpoint_health eh ON eh.station_id = s.id
```

3. Update row mapping to assign:

```rust
endpoint_ping_status: row.get(endpoint_ping_status_index)?,
endpoint_ping_ms: row.get(endpoint_ping_ms_index)?,
endpoint_ping_checked_at: row.get(endpoint_ping_checked_at_index)?,
endpoint_ping_error: row.get(endpoint_ping_error_index)?,
```

Use the actual numeric indexes in the existing row mapping. Do not change unrelated selected columns.

- [ ] **Step 5: Run key-pool join test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml key_pool_items_include_station_endpoint_ping
```

Expected: PASS.

- [ ] **Step 6: Run related database tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml station_endpoint_health key_pool_items_include_station_endpoint_ping
```

Expected: PASS.

- [ ] **Step 7: Commit Task 6**

Stage exact paths:

```powershell
git add -- src-tauri/src/models/station_keys.rs src-tauri/src/services/database.rs
git commit -m "feat: include endpoint ping in key pool"
```

### Task 7: Render True Endpoint PING In Channel Status

**Files:**
- Modify: `src/features/channels/channelStatusViewModel.ts`
- Modify: `src/features/channels/ChannelStatusTab.tsx`
- Modify: `scripts/channel-status-view-model.test.mjs`

- [ ] **Step 1: Write failing view-model test**

In `scripts/channel-status-view-model.test.mjs`, add:

```js
assert.deepEqual(
  resolveChannelLatencyMetrics({
    requestLatencyMs: null,
    healthLatencyMs: 5422,
    endpointPingMs: 38,
  }),
  { conversationLatencyMs: 5422, endpointPingMs: 38 },
  "endpoint ping should come from the dedicated endpoint field",
);
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```powershell
node .\scripts\channel-status-view-model.test.mjs
```

Expected: FAIL because `resolveChannelLatencyMetrics` does not accept `endpointPingMs`.

- [ ] **Step 3: Update latency helper types**

In `src/features/channels/channelStatusViewModel.ts`, change:

```ts
export type ChannelLatencyMetricInput = {
  requestLatencyMs: number | null;
  healthLatencyMs: number | null;
};
```

to:

```ts
export type ChannelLatencyMetricInput = {
  requestLatencyMs: number | null;
  healthLatencyMs: number | null;
  endpointPingMs: number | null;
};
```

Change the helper to:

```ts
export function resolveChannelLatencyMetrics({
  requestLatencyMs,
  healthLatencyMs,
  endpointPingMs,
}: ChannelLatencyMetricInput): ChannelLatencyMetrics {
  return {
    conversationLatencyMs: requestLatencyMs ?? healthLatencyMs,
    endpointPingMs,
  };
}
```

Update existing tests that call `resolveChannelLatencyMetrics` to pass `endpointPingMs: null`.

- [ ] **Step 4: Update channel card mapping**

In `src/features/channels/ChannelStatusTab.tsx`, change:

```ts
const { conversationLatencyMs, endpointPingMs } = resolveChannelLatencyMetrics({
  requestLatencyMs,
  healthLatencyMs,
});
```

to:

```ts
const { conversationLatencyMs, endpointPingMs } = resolveChannelLatencyMetrics({
  requestLatencyMs,
  healthLatencyMs,
  endpointPingMs: key.endpointPingMs,
});
```

If `ChannelHealth` needs endpoint status details for titles or errors, add:

```ts
endpointPingStatus: KeyPoolItem["endpointPingStatus"];
endpointPingCheckedAt: string | null;
endpointPingError: string | null;
```

and map from the `key`.

- [ ] **Step 5: Add ping action to status tab**

Add `pingStationEndpoint` import:

```ts
import { pingStationEndpoint } from "@/lib/api/stations";
```

Add this function inside `ChannelStatusTab`:

```ts
async function pingAllVisibleStations() {
  const stationIds = Array.from(new Set(keys.map((key) => key.stationId)));
  if (stationIds.length === 0) {
    return;
  }
  try {
    await Promise.all(stationIds.map((stationId) => pingStationEndpoint(stationId)));
    await refresh(false);
    toast.success("端点 PING 已刷新");
  } catch (requestError) {
    const message = readError(requestError);
    toast.error("端点 PING 失败", message);
  }
}
```

Add a secondary button next to refresh:

```tsx
<Button variant="secondary" onClick={() => void pingAllVisibleStations()}>
  <Radio className="h-4 w-4" />
  PING
</Button>
```

- [ ] **Step 6: Run frontend tests**

Run:

```powershell
node .\scripts\channel-status-view-model.test.mjs
pnpm.cmd exec tsc --noEmit --pretty false
```

Expected: Both PASS.

- [ ] **Step 7: Commit Task 7**

Stage exact paths:

```powershell
git add -- src/features/channels/channelStatusViewModel.ts src/features/channels/ChannelStatusTab.tsx scripts/channel-status-view-model.test.mjs
git commit -m "feat: show true endpoint ping in channel status"
```

### Task 8: Add Optional Monitor Integration For Endpoint Ping

**Files:**
- Modify: `src-tauri/src/services/channel_monitors/mod.rs`
- Test: `src-tauri/src/services/channel_monitors/mod.rs`

- [ ] **Step 1: Write failing monitor integration test**

In `src-tauri/src/services/channel_monitors/mod.rs` tests module, add:

```rust
#[test]
fn monitor_run_updates_station_endpoint_ping_once_per_station() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let data_key = [3_u8; 32];
    let (base_url, _received) = spawn_upstream(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 29\r\n\r\n{\"model\":\"gpt-test\",\"ok\":true}",
    );
    let station = database
        .create_station_with_data_key(
            CreateStationInput {
                name: "monitor endpoint ping station".to_string(),
                station_type: "openai-compatible".to_string(),
                base_url,
                api_key: "sk-monitor-endpoint".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                note: None,
            },
            Some(&data_key),
        )
        .expect("station");
    let key = database
        .list_station_keys(station.id.clone())
        .expect("keys")
        .remove(0);
    let monitor = database
        .create_channel_monitor(CreateChannelMonitorInput {
            name: "Endpoint ping monitor".to_string(),
            target_type: "station_key".to_string(),
            station_id: station.id.clone(),
            station_key_id: Some(key.id),
            template_id: "builtin-openai-chat-low-token".to_string(),
            enabled: true,
            interval_seconds: 60,
            jitter_seconds: 0,
            timeout_seconds: 5,
            max_concurrency: 1,
            consecutive_failure_threshold: 3,
            fallback_models: vec!["gpt-test".to_string()],
            note: None,
        })
        .expect("monitor");

    let _runs = run_channel_monitor_now(&database, &data_key, &monitor.id).expect("manual run");
    let endpoint_health = database
        .get_station_endpoint_health(station.id)
        .expect("endpoint health");

    assert_eq!(endpoint_health.status, "success");
    assert!(endpoint_health.latency_ms.is_some());
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml monitor_run_updates_station_endpoint_ping_once_per_station
```

Expected: FAIL because monitor runs do not update endpoint health.

- [ ] **Step 3: Update monitor run to ping station endpoint**

In `src-tauri/src/services/channel_monitors/mod.rs`, import:

```rust
use crate::services::endpoint_ping::ping_station_endpoint;
```

In the manual monitor execution path, before key-specific probes for each station, call:

```rust
fn update_station_endpoint_ping(
    database: &AppDatabase,
    station_id: &str,
    station_base_url: &str,
    timeout_seconds: i64,
) -> Result<(), String> {
    let checked_at = now_string();
    let timeout = Duration::from_secs(timeout_seconds.max(1) as u64);
    let result = ping_station_endpoint(station_base_url, timeout);
    database.upsert_station_endpoint_health(
        station_id,
        &result.status,
        result.latency_ms,
        &checked_at,
        result.error_summary.as_deref(),
    )?;
    Ok(())
}
```

Call it once per station per monitor run. For a station-wide monitor, call it once before iterating keys. For a station-key monitor, call it once for that key's station.

- [ ] **Step 4: Run monitor integration test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml monitor_run_updates_station_endpoint_ping_once_per_station
```

Expected: PASS.

- [ ] **Step 5: Run broader Rust monitor tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml channel_monitor
```

Expected: PASS. Use a 180 second timeout if the first compile is slow.

- [ ] **Step 6: Commit Task 8**

Stage exact paths:

```powershell
git add -- src-tauri/src/services/channel_monitors/mod.rs
git commit -m "feat: refresh endpoint ping during monitor runs"
```

### Task 9: End-To-End Verification And UI Smoke

**Files:**
- No new feature files unless a test fixture needs updating.

- [ ] **Step 1: Run Rust formatting check**

Run:

```powershell
cargo fmt --manifest-path .\src-tauri\Cargo.toml --check
```

Expected: PASS.

- [ ] **Step 2: Run Rust compile check**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected: PASS. Existing dead-code warnings are acceptable if unchanged from baseline.

- [ ] **Step 3: Run targeted Rust tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml station_endpoint_health endpoint_ping_ key_pool_items_include_station_endpoint_ping ping_station_endpoint_command_records_station_endpoint_health monitor_run_updates_station_endpoint_ping_once_per_station
```

Expected: PASS.

- [ ] **Step 4: Run frontend checks**

Run:

```powershell
node .\scripts\channel-status-view-model.test.mjs
pnpm.cmd build
```

Expected: PASS. Vite may warn about chunk size; that warning is not a failure.

- [ ] **Step 5: Manual app smoke**

Run:

```powershell
pnpm.cmd tauri:dev
```

Manual expected behavior:

- Open `渠道状态`.
- Click `PING`.
- `端点 PING` shows a value for keys under stations whose base URL responds.
- `对话延迟` still shows model/conversation latency.
- Running a monitor refreshes both:
  - station endpoint ping from the base URL probe;
  - key conversation latency from the model probe.
- Failed endpoint ping shows `--` for latency and stores a redacted error in endpoint health.

- [ ] **Step 6: Review final diff**

Run:

```powershell
git diff --stat
git status --short
```

Expected: Only planned files are modified, plus unrelated baseline files that existed before this work.

## Acceptance Criteria

- `端点 PING` is backed by `station_endpoint_health.latency_ms`.
- `端点 PING` does not use `station_key_health.avg_latency_ms`.
- Endpoint ping does not call model endpoints and does not require API key.
- `对话延迟` still uses proxy request logs first and key health/model probe latency as fallback.
- Station-wide endpoint ping is measured once per station, not once per key.
- Manual `PING` refresh exists in the channel status page.
- Monitor runs refresh endpoint ping without breaking existing monitor run persistence.
- Rust and frontend checks pass with the commands listed above.

## Self-Review

- Spec coverage: The plan separates endpoint PING from conversation latency, adds storage, backend probe, commands, frontend API, UI rendering, monitor integration, and verification.
- Placeholder scan: No unresolved placeholder markers or open-ended “handle later” steps remain.
- Type consistency: The plan consistently uses `StationEndpointHealth`, `EndpointPingResult`, `station_endpoint_health`, `endpointPingMs`, and `ping_station_endpoint_command`.
