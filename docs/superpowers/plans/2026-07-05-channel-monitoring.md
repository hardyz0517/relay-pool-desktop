# Channel Monitoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build complete scheduled channel monitoring with request templates, manual runs, background runner, history, and existing Station Key health writeback.

**Architecture:** Keep `Station Key` as the routable health target and write probe results into existing `station_key_health`. Add a focused monitor domain under Rust models/services and expose it through Tauri commands plus React API wrappers. Split `ChannelStatusPage` into a coordinator with `Status` and `Monitoring` tabs while preserving the current status card behavior.

**Tech Stack:** Tauri 2, Rust 2021, rusqlite, serde/serde_json, ureq, React 18, TypeScript, Vite, Tailwind CSS, existing local UI components.

---

## Scope Notes

This plan implements the approved complete path:

- persistent monitor records;
- built-in and custom request templates;
- manual and scheduled monitor execution;
- station-wide expansion to enabled keys;
- execution history;
- health writeback to `station_key_health` and `station_keys`;
- template management UI;
- `状态` / `监控` tab split in the existing route.

Do not copy code from Sub2API. Use its monitor/template concepts only.

## File Structure

### Rust Models

- Create `src-tauri/src/models/channel_monitors.rs`
  - Owns serializable monitor, template, run, input, and result DTOs.
- Modify `src-tauri/src/models/mod.rs`
  - Exports `channel_monitors`.

### Rust Services

- Create `src-tauri/src/services/channel_monitors/mod.rs`
  - Public service facade, runner state, and module exports.
- Create `src-tauri/src/services/channel_monitors/templates.rs`
  - Built-in template definitions and template rendering.
- Create `src-tauri/src/services/channel_monitors/redaction.rs`
  - Backend-only redaction for headers, JSON, and error strings.
- Create `src-tauri/src/services/channel_monitors/probe.rs`
  - HTTP probe execution with timeout and latency measurement.
- Modify `src-tauri/src/services/mod.rs`
  - Exports `channel_monitors`.
- Modify `src-tauri/src/services/database.rs`
  - Adds schema, CRUD methods, template seeding, run history methods, and monitor due queries.
- Modify `src-tauri/src/lib.rs`
  - Manages runner state and starts the background runner in setup.
- Modify `src-tauri/src/commands/mod.rs`
  - Adds monitor/template/run commands.

### Frontend Types And API

- Create `src/lib/types/channelMonitors.ts`
  - Monitor, template, run, and command input types.
- Create `src/lib/api/channelMonitors.ts`
  - Tauri invoke wrappers with in-memory browser fallback.

### Frontend UI

- Modify `src/features/channels/ChannelStatusPage.tsx`
  - Page coordinator and `状态` / `监控` segmented switch.
- Create `src/features/channels/ChannelStatusTab.tsx`
  - Current status card behavior moved here.
- Create `src/features/channels/ChannelMonitoringTab.tsx`
  - Monitor toolbar, summary, table/list, manual run actions.
- Create `src/features/channels/ChannelMonitorForm.tsx`
  - Create/edit monitor dialog.
- Create `src/features/channels/ChannelMonitorTemplateManager.tsx`
  - Template list and editor dialog.
- Create `src/features/channels/channelMonitorViewModel.ts`
  - Derived UI state, labels, validation helpers.

## Task 1: Rust Monitor Models And Database Schema

**Files:**
- Create: `src-tauri/src/models/channel_monitors.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add failing database tests for built-in template seeding and monitor CRUD**

Append these tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/services/database.rs`:

```rust
#[test]
fn channel_monitor_templates_seed_idempotently() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");

    let first = database
        .list_channel_monitor_templates()
        .expect("first templates");
    database
        .seed_builtin_channel_monitor_templates()
        .expect("seed again");
    let second = database
        .list_channel_monitor_templates()
        .expect("second templates");

    assert!(first.iter().any(|template| template.id == "builtin-openai-chat-default"));
    assert!(first.iter().any(|template| template.id == "builtin-openai-chat-low-token"));
    assert!(first.iter().any(|template| template.id == "builtin-openai-responses-default"));
    assert!(first.iter().any(|template| template.id == "builtin-openai-responses-low-token"));
    assert_eq!(first.len(), second.len());
}

#[test]
fn create_channel_monitor_for_station_key_round_trips() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "monitor-station");
    let key = database
        .list_station_keys(station.id.clone())
        .expect("station keys")
        .remove(0);

    let monitor = database
        .create_channel_monitor(CreateChannelMonitorInput {
            name: "Key health".to_string(),
            target_type: "station_key".to_string(),
            station_id: station.id.clone(),
            station_key_id: Some(key.id.clone()),
            template_id: "builtin-openai-chat-low-token".to_string(),
            primary_model: "gpt-4o-mini".to_string(),
            fallback_models: vec!["gpt-4.1-mini".to_string()],
            enabled: true,
            interval_seconds: 60,
            jitter_seconds: 5,
            timeout_seconds: 20,
            max_concurrency: 1,
            failure_cooldown_seconds: 300,
            consecutive_failure_threshold: 3,
        })
        .expect("create monitor");

    let monitors = database.list_channel_monitors().expect("monitors");

    assert_eq!(monitor.target_type, "station_key");
    assert_eq!(monitor.station_id, station.id);
    assert_eq!(monitor.station_key_id.as_deref(), Some(key.id.as_str()));
    assert_eq!(monitor.template_id, "builtin-openai-chat-low-token");
    assert_eq!(monitor.fallback_models, vec!["gpt-4.1-mini"]);
    assert_eq!(monitors.len(), 1);
}

#[test]
fn channel_monitor_rejects_station_key_from_another_station() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station_a = test_station(&database, "monitor-station-a");
    let station_b = test_station(&database, "monitor-station-b");
    let key_b = database
        .list_station_keys(station_b.id)
        .expect("station b keys")
        .remove(0);

    let result = database.create_channel_monitor(CreateChannelMonitorInput {
        name: "Mismatched key".to_string(),
        target_type: "station_key".to_string(),
        station_id: station_a.id,
        station_key_id: Some(key_b.id),
        template_id: "builtin-openai-chat-low-token".to_string(),
        primary_model: "gpt-4o-mini".to_string(),
        fallback_models: Vec::new(),
        enabled: true,
        interval_seconds: 60,
        jitter_seconds: 0,
        timeout_seconds: 20,
        max_concurrency: 1,
        failure_cooldown_seconds: 300,
        consecutive_failure_threshold: 3,
    });

    assert!(result
        .expect_err("station mismatch should fail")
        .contains("Station Key does not belong to the selected Station"));
}
```

- [ ] **Step 2: Run database tests and verify they fail because the new types/methods do not exist**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib channel_monitor -- --nocapture
```

Expected: compile failure mentioning `CreateChannelMonitorInput`, `list_channel_monitor_templates`, or `create_channel_monitor`.

- [ ] **Step 3: Create monitor DTO models**

Create `src-tauri/src/models/channel_monitors.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitor {
    pub id: String,
    pub name: String,
    pub target_type: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub primary_model: String,
    pub fallback_models: Vec<String>,
    pub enabled: bool,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub failure_cooldown_seconds: i64,
    pub consecutive_failure_threshold: i64,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub last_status: String,
    pub last_latency_ms: Option<i64>,
    pub last_error_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelMonitorInput {
    pub name: String,
    pub target_type: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub primary_model: String,
    pub fallback_models: Vec<String>,
    pub enabled: bool,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub failure_cooldown_seconds: i64,
    pub consecutive_failure_threshold: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelMonitorInput {
    pub id: String,
    pub name: String,
    pub target_type: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub primary_model: String,
    pub fallback_models: Vec<String>,
    pub enabled: bool,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub failure_cooldown_seconds: i64,
    pub consecutive_failure_threshold: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorRequestTemplate {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub protocol: String,
    pub method: String,
    pub path: String,
    pub headers_json: String,
    pub body_template_json: String,
    pub model_field_path: String,
    pub stream_field_path: Option<String>,
    pub max_tokens_field_path: Option<String>,
    pub default_max_tokens: i64,
    pub default_stream: bool,
    pub success_rule_json: String,
    pub error_extract_rule_json: String,
    pub description: Option<String>,
    pub builtin: bool,
    pub enabled: bool,
    pub default_for_protocol: bool,
    pub version: i64,
    pub linked_monitor_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelMonitorTemplateInput {
    pub name: String,
    pub provider: String,
    pub protocol: String,
    pub method: String,
    pub path: String,
    pub headers_json: String,
    pub body_template_json: String,
    pub model_field_path: String,
    pub stream_field_path: Option<String>,
    pub max_tokens_field_path: Option<String>,
    pub default_max_tokens: i64,
    pub default_stream: bool,
    pub success_rule_json: String,
    pub error_extract_rule_json: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub default_for_protocol: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelMonitorTemplateInput {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub protocol: String,
    pub method: String,
    pub path: String,
    pub headers_json: String,
    pub body_template_json: String,
    pub model_field_path: String,
    pub stream_field_path: Option<String>,
    pub max_tokens_field_path: Option<String>,
    pub default_max_tokens: i64,
    pub default_stream: bool,
    pub success_rule_json: String,
    pub error_extract_rule_json: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub default_for_protocol: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorRun {
    pub id: String,
    pub monitor_id: String,
    pub parent_run_id: Option<String>,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub target_type: String,
    pub model: Option<String>,
    pub status: String,
    pub status_code: Option<i64>,
    pub latency_ms: Option<i64>,
    pub error_summary: Option<String>,
    pub response_excerpt_redacted: Option<String>,
    pub checked_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct CreateChannelMonitorRunInput {
    pub monitor_id: String,
    pub parent_run_id: Option<String>,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub target_type: String,
    pub model: Option<String>,
    pub status: String,
    pub status_code: Option<i64>,
    pub latency_ms: Option<i64>,
    pub error_summary: Option<String>,
    pub response_excerpt_redacted: Option<String>,
    pub checked_at: String,
}
```

Modify `src-tauri/src/models/mod.rs`:

```rust
pub mod channel_monitors;
```

- [ ] **Step 4: Add schema and public database method signatures**

In `src-tauri/src/services/database.rs`, add imports near the existing model imports:

```rust
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            CreateChannelMonitorInput, CreateChannelMonitorRunInput,
            CreateChannelMonitorTemplateInput, UpdateChannelMonitorInput,
            UpdateChannelMonitorTemplateInput,
        },
```

In `initialize_schema`, call the new migration after `migrate_p9_fact_schema(connection)?`:

```rust
    migrate_p9_fact_schema(connection)?;
    migrate_channel_monitor_schema(connection)
```

Add methods inside `impl AppDatabase` after the existing health methods:

```rust
    pub fn seed_builtin_channel_monitor_templates(&self) -> Result<(), String> {
        let connection = self.connection()?;
        seed_builtin_channel_monitor_templates_in_connection(&connection)
    }

    pub fn list_channel_monitor_templates(
        &self,
    ) -> Result<Vec<ChannelMonitorRequestTemplate>, String> {
        let connection = self.connection()?;
        list_channel_monitor_templates_from_connection(&connection)
    }

    pub fn create_channel_monitor_template(
        &self,
        input: CreateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, String> {
        let connection = self.connection()?;
        create_channel_monitor_template_in_connection(&connection, input)
    }

    pub fn update_channel_monitor_template(
        &self,
        input: UpdateChannelMonitorTemplateInput,
    ) -> Result<ChannelMonitorRequestTemplate, String> {
        let connection = self.connection()?;
        update_channel_monitor_template_in_connection(&connection, input)
    }

    pub fn duplicate_channel_monitor_template(
        &self,
        id: String,
    ) -> Result<ChannelMonitorRequestTemplate, String> {
        let connection = self.connection()?;
        duplicate_channel_monitor_template_in_connection(&connection, &id)
    }

    pub fn delete_channel_monitor_template(&self, id: String) -> Result<(), String> {
        let connection = self.connection()?;
        delete_channel_monitor_template_in_connection(&connection, &id)
    }

    pub fn list_channel_monitors(&self) -> Result<Vec<ChannelMonitor>, String> {
        let connection = self.connection()?;
        list_channel_monitors_from_connection(&connection)
    }

    pub fn create_channel_monitor(
        &self,
        input: CreateChannelMonitorInput,
    ) -> Result<ChannelMonitor, String> {
        let connection = self.connection()?;
        create_channel_monitor_in_connection(&connection, input)
    }

    pub fn update_channel_monitor(
        &self,
        input: UpdateChannelMonitorInput,
    ) -> Result<ChannelMonitor, String> {
        let connection = self.connection()?;
        update_channel_monitor_in_connection(&connection, input)
    }

    pub fn delete_channel_monitor(&self, id: String) -> Result<(), String> {
        let connection = self.connection()?;
        delete_channel_monitor_in_connection(&connection, &id)
    }

    pub fn list_channel_monitor_runs(
        &self,
        monitor_id: String,
    ) -> Result<Vec<ChannelMonitorRun>, String> {
        let connection = self.connection()?;
        list_channel_monitor_runs_from_connection(&connection, &monitor_id)
    }

    pub fn insert_channel_monitor_run(
        &self,
        input: CreateChannelMonitorRunInput,
    ) -> Result<ChannelMonitorRun, String> {
        let connection = self.connection()?;
        insert_channel_monitor_run_in_connection(&connection, input)
    }
```

- [ ] **Step 5: Add schema creation**

Add this function near `migrate_p9_fact_schema`:

```rust
fn migrate_channel_monitor_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS channel_monitor_request_templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider TEXT NOT NULL,
            protocol TEXT NOT NULL,
            method TEXT NOT NULL,
            path TEXT NOT NULL,
            headers_json TEXT NOT NULL,
            body_template_json TEXT NOT NULL,
            model_field_path TEXT NOT NULL,
            stream_field_path TEXT,
            max_tokens_field_path TEXT,
            default_max_tokens INTEGER NOT NULL,
            default_stream INTEGER NOT NULL DEFAULT 0,
            success_rule_json TEXT NOT NULL,
            error_extract_rule_json TEXT NOT NULL,
            description TEXT,
            builtin INTEGER NOT NULL DEFAULT 0,
            enabled INTEGER NOT NULL DEFAULT 1,
            default_for_protocol INTEGER NOT NULL DEFAULT 0,
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_channel_monitor_templates_provider
            ON channel_monitor_request_templates(provider, protocol, enabled);

        CREATE TABLE IF NOT EXISTS channel_monitors (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            target_type TEXT NOT NULL,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            template_id TEXT NOT NULL,
            primary_model TEXT NOT NULL,
            fallback_models_json TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            interval_seconds INTEGER NOT NULL,
            jitter_seconds INTEGER NOT NULL DEFAULT 0,
            timeout_seconds INTEGER NOT NULL,
            max_concurrency INTEGER NOT NULL DEFAULT 1,
            failure_cooldown_seconds INTEGER NOT NULL DEFAULT 300,
            consecutive_failure_threshold INTEGER NOT NULL DEFAULT 3,
            last_run_at TEXT,
            next_run_at TEXT,
            last_status TEXT NOT NULL DEFAULT 'unchecked',
            last_latency_ms INTEGER,
            last_error_summary TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE,
            FOREIGN KEY(template_id) REFERENCES channel_monitor_request_templates(id)
        );

        CREATE INDEX IF NOT EXISTS idx_channel_monitors_due
            ON channel_monitors(enabled, next_run_at);
        CREATE INDEX IF NOT EXISTS idx_channel_monitors_station
            ON channel_monitors(station_id, station_key_id);

        CREATE TABLE IF NOT EXISTS channel_monitor_runs (
            id TEXT PRIMARY KEY,
            monitor_id TEXT NOT NULL,
            parent_run_id TEXT,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            template_id TEXT NOT NULL,
            target_type TEXT NOT NULL,
            model TEXT,
            status TEXT NOT NULL,
            status_code INTEGER,
            latency_ms INTEGER,
            error_summary TEXT,
            response_excerpt_redacted TEXT,
            checked_at TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(monitor_id) REFERENCES channel_monitors(id) ON DELETE CASCADE,
            FOREIGN KEY(parent_run_id) REFERENCES channel_monitor_runs(id) ON DELETE CASCADE,
            FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
            FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE SET NULL,
            FOREIGN KEY(template_id) REFERENCES channel_monitor_request_templates(id)
        );

        CREATE INDEX IF NOT EXISTS idx_channel_monitor_runs_monitor
            ON channel_monitor_runs(monitor_id, checked_at DESC);
        CREATE INDEX IF NOT EXISTS idx_channel_monitor_runs_key
            ON channel_monitor_runs(station_key_id, checked_at DESC);
        "#,
    )?;
    seed_builtin_channel_monitor_templates_in_connection(connection)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(std::io::ErrorKind::Other, error))))
}
```

- [ ] **Step 6: Write monitor input guards and CRUD implementations**

Add helpers below schema functions:

```rust
fn validate_channel_monitor_input(
    connection: &Connection,
    target_type: &str,
    station_id: &str,
    station_key_id: Option<&str>,
    template_id: &str,
    interval_seconds: i64,
    jitter_seconds: i64,
    timeout_seconds: i64,
    max_concurrency: i64,
    consecutive_failure_threshold: i64,
) -> Result<(), String> {
    if target_type != "station_key" && target_type != "station" {
        return Err("Monitor target type must be station_key or station".to_string());
    }
    if interval_seconds < 15 || interval_seconds > 3600 {
        return Err("Monitor interval must be between 15 and 3600 seconds".to_string());
    }
    if jitter_seconds < 0 || jitter_seconds > 600 {
        return Err("Monitor jitter must be between 0 and 600 seconds".to_string());
    }
    if interval_seconds - jitter_seconds < 15 {
        return Err("Monitor interval minus jitter must be at least 15 seconds".to_string());
    }
    if timeout_seconds < 5 || timeout_seconds > 120 {
        return Err("Monitor timeout must be between 5 and 120 seconds".to_string());
    }
    if max_concurrency < 1 || max_concurrency > 16 {
        return Err("Monitor concurrency must be between 1 and 16".to_string());
    }
    if consecutive_failure_threshold < 1 || consecutive_failure_threshold > 20 {
        return Err("Monitor failure threshold must be between 1 and 20".to_string());
    }
    validate_station_exists(connection, station_id)?;
    validate_channel_monitor_template_exists(connection, template_id)?;

    match (target_type, station_key_id) {
        ("station_key", Some(key_id)) => validate_station_key_belongs_to_station(connection, station_id, key_id),
        ("station_key", None) => Err("Station Key monitor requires a Station Key".to_string()),
        ("station", None) => Ok(()),
        ("station", Some(_)) => Err("Station-wide monitor must not include a Station Key".to_string()),
        _ => Err("Monitor target is invalid".to_string()),
    }
}

fn validate_channel_monitor_template_exists(connection: &Connection, template_id: &str) -> Result<(), String> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM channel_monitor_request_templates WHERE id = ?1 AND enabled = 1",
            params![template_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Read monitor template failed: {error}"))?;
    if count == 0 {
        return Err("Monitor template does not exist or is disabled".to_string());
    }
    Ok(())
}

fn validate_station_key_belongs_to_station(
    connection: &Connection,
    station_id: &str,
    station_key_id: &str,
) -> Result<(), String> {
    let actual_station_id: Option<String> = connection
        .query_row(
            "SELECT station_id FROM station_keys WHERE id = ?1",
            params![station_key_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("Read Station Key failed: {error}"))?;
    match actual_station_id {
        Some(actual) if actual == station_id => Ok(()),
        Some(_) => Err("Station Key does not belong to the selected Station".to_string()),
        None => Err("Station Key does not exist".to_string()),
    }
}
```

Then add CRUD functions with the same mapping style used by existing database functions. Use `serde_json::to_string(&input.fallback_models)` for `fallback_models_json`, `now_string()` for timestamps, and `random_id("channel-monitor")` / `random_id("channel-monitor-template")` / `random_id("channel-monitor-run")` if the helper exists. If there is no `random_id` helper, use the existing ID helper used by stations and keys.

- [ ] **Step 7: Implement built-in template seeding**

Add this function:

```rust
fn seed_builtin_channel_monitor_templates_in_connection(connection: &Connection) -> Result<(), String> {
    let now = now_string();
    let templates = [
        (
            "builtin-openai-chat-default",
            "OpenAI Compatible default check",
            "openai",
            "openai_chat_completions",
            "POST",
            "/v1/chat/completions",
            r#"{"content-type":"application/json"}"#,
            r#"{"model":"{{model}}","messages":[{"role":"user","content":"{{challenge}}"}],"max_tokens":{{max_tokens}},"stream":{{stream}}}"#,
            "model",
            Some("stream"),
            Some("max_tokens"),
            8,
            0,
            r#"{"okStatusMax":399,"requireJson":true}"#,
            r#"{"messagePaths":["error.message","message"]}"#,
            "POST /v1/chat/completions with a small messages payload.",
            1,
        ),
        (
            "builtin-openai-chat-low-token",
            "OpenAI Compatible low token check",
            "openai",
            "openai_chat_completions",
            "POST",
            "/v1/chat/completions",
            r#"{"content-type":"application/json"}"#,
            r#"{"model":"{{model}}","messages":[{"role":"user","content":"Reply ok."}],"max_tokens":1,"stream":false}"#,
            "model",
            Some("stream"),
            Some("max_tokens"),
            1,
            0,
            r#"{"okStatusMax":399,"requireJson":true}"#,
            r#"{"messagePaths":["error.message","message"]}"#,
            "Low token /v1/chat/completions probe for frequent monitoring.",
            0,
        ),
        (
            "builtin-openai-responses-default",
            "OpenAI Responses default check",
            "openai",
            "openai_responses",
            "POST",
            "/v1/responses",
            r#"{"content-type":"application/json"}"#,
            r#"{"model":"{{model}}","instructions":"Answer with a short health marker.","input":"{{challenge}}","max_output_tokens":{{max_tokens}},"stream":{{stream}}}"#,
            "model",
            Some("stream"),
            Some("max_output_tokens"),
            8,
            0,
            r#"{"okStatusMax":399,"requireJson":true}"#,
            r#"{"messagePaths":["error.message","message"]}"#,
            "POST /v1/responses probe for Responses-compatible upstreams.",
            1,
        ),
        (
            "builtin-openai-responses-low-token",
            "OpenAI Responses low token check",
            "openai",
            "openai_responses",
            "POST",
            "/v1/responses",
            r#"{"content-type":"application/json"}"#,
            r#"{"model":"{{model}}","input":"Reply ok.","max_output_tokens":1,"stream":false}"#,
            "model",
            Some("stream"),
            Some("max_output_tokens"),
            1,
            0,
            r#"{"okStatusMax":399,"requireJson":true}"#,
            r#"{"messagePaths":["error.message","message"]}"#,
            "Low token /v1/responses probe for frequent monitoring.",
            0,
        ),
    ];

    for template in templates {
        connection
            .execute(
                "INSERT INTO channel_monitor_request_templates (
                    id, name, provider, protocol, method, path, headers_json,
                    body_template_json, model_field_path, stream_field_path,
                    max_tokens_field_path, default_max_tokens, default_stream,
                    success_rule_json, error_extract_rule_json, description,
                    builtin, enabled, default_for_protocol, version, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 1, 1, ?17, 1, ?18, ?19)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    provider = excluded.provider,
                    protocol = excluded.protocol,
                    method = excluded.method,
                    path = excluded.path,
                    headers_json = excluded.headers_json,
                    body_template_json = excluded.body_template_json,
                    model_field_path = excluded.model_field_path,
                    stream_field_path = excluded.stream_field_path,
                    max_tokens_field_path = excluded.max_tokens_field_path,
                    default_max_tokens = excluded.default_max_tokens,
                    default_stream = excluded.default_stream,
                    success_rule_json = excluded.success_rule_json,
                    error_extract_rule_json = excluded.error_extract_rule_json,
                    description = excluded.description,
                    builtin = 1,
                    enabled = 1,
                    default_for_protocol = excluded.default_for_protocol,
                    updated_at = excluded.updated_at",
                params![
                    template.0,
                    template.1,
                    template.2,
                    template.3,
                    template.4,
                    template.5,
                    template.6,
                    template.7,
                    template.8,
                    template.9,
                    template.10,
                    template.11,
                    template.12,
                    template.13,
                    template.14,
                    template.15,
                    template.16,
                    now,
                    now,
                ],
            )
            .map_err(|error| format!("Seed monitor template failed: {error}"))?;
    }
    Ok(())
}
```

- [ ] **Step 8: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib channel_monitor -- --nocapture
```

Expected: all `channel_monitor` tests pass.

- [ ] **Step 9: Commit Task 1**

```powershell
git add -- src-tauri/src/models/channel_monitors.rs src-tauri/src/models/mod.rs src-tauri/src/services/database.rs
git commit -m "feat: add channel monitor storage"
```

## Task 2: Template Rendering, Redaction, And Probe Client

**Files:**
- Create: `src-tauri/src/services/channel_monitors/mod.rs`
- Create: `src-tauri/src/services/channel_monitors/templates.rs`
- Create: `src-tauri/src/services/channel_monitors/redaction.rs`
- Create: `src-tauri/src/services/channel_monitors/probe.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Write rendering and redaction tests**

Create `src-tauri/src/services/channel_monitors/templates.rs` with tests first:

```rust
use serde_json::Value;

use crate::models::channel_monitors::ChannelMonitorRequestTemplate;

#[derive(Debug, Clone)]
pub struct RenderedMonitorRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MonitorTemplateContext {
    pub model: String,
    pub max_tokens: i64,
    pub stream: bool,
    pub challenge: String,
}

pub fn render_monitor_request(
    template: &ChannelMonitorRequestTemplate,
    context: &MonitorTemplateContext,
) -> Result<RenderedMonitorRequest, String> {
    let rendered_headers = replace_template_tokens(&template.headers_json, context);
    let rendered_body = replace_template_tokens(&template.body_template_json, context);
    let headers_value: Value = serde_json::from_str(&rendered_headers)
        .map_err(|error| format!("Monitor template headers are invalid JSON: {error}"))?;
    let body_value: Value = serde_json::from_str(&rendered_body)
        .map_err(|error| format!("Monitor template body is invalid JSON: {error}"))?;
    let headers = headers_value
        .as_object()
        .ok_or_else(|| "Monitor template headers must be a JSON object".to_string())?
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|string| (key.to_ascii_lowercase(), string.to_string()))
                .ok_or_else(|| format!("Monitor template header {key} must be a string"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(RenderedMonitorRequest {
        method: template.method.to_ascii_uppercase(),
        path: template.path.clone(),
        headers,
        body: serde_json::to_vec(&body_value)
            .map_err(|error| format!("Monitor template body serialization failed: {error}"))?,
    })
}

fn replace_template_tokens(input: &str, context: &MonitorTemplateContext) -> String {
    input
        .replace("{{model}}", &json_string_fragment(&context.model))
        .replace("{{max_tokens}}", &context.max_tokens.to_string())
        .replace("{{stream}}", if context.stream { "true" } else { "false" })
        .replace("{{challenge}}", &json_string_fragment(&context.challenge))
        .replace("{{timestamp}}", &crate::services::database::now_millis_for_services().to_string())
}

fn json_string_fragment(value: &str) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|_| "\"\"".to_string())
        .trim_matches('"')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> ChannelMonitorRequestTemplate {
        ChannelMonitorRequestTemplate {
            id: "template".to_string(),
            name: "Template".to_string(),
            provider: "openai".to_string(),
            protocol: "openai_chat_completions".to_string(),
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers_json: r#"{"content-type":"application/json","x-probe":"health"}"#.to_string(),
            body_template_json: r#"{"model":"{{model}}","messages":[{"role":"user","content":"{{challenge}}"}],"max_tokens":{{max_tokens}},"stream":{{stream}}}"#.to_string(),
            model_field_path: "model".to_string(),
            stream_field_path: Some("stream".to_string()),
            max_tokens_field_path: Some("max_tokens".to_string()),
            default_max_tokens: 1,
            default_stream: false,
            success_rule_json: r#"{"okStatusMax":399}"#.to_string(),
            error_extract_rule_json: r#"{"messagePaths":["error.message"]}"#.to_string(),
            description: None,
            builtin: true,
            enabled: true,
            default_for_protocol: false,
            version: 1,
            linked_monitor_count: 0,
            created_at: "1000".to_string(),
            updated_at: "1000".to_string(),
        }
    }

    #[test]
    fn render_monitor_request_substitutes_json_values() {
        let rendered = render_monitor_request(
            &template(),
            &MonitorTemplateContext {
                model: "gpt-4o-mini".to_string(),
                max_tokens: 1,
                stream: false,
                challenge: "Reply ok.".to_string(),
            },
        )
        .expect("render");
        let body: Value = serde_json::from_slice(&rendered.body).expect("body json");

        assert_eq!(rendered.method, "POST");
        assert_eq!(rendered.path, "/v1/chat/completions");
        assert!(rendered.headers.contains(&("content-type".to_string(), "application/json".to_string())));
        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["max_tokens"], 1);
        assert_eq!(body["stream"], false);
    }
}
```

Create `src-tauri/src/services/channel_monitors/redaction.rs` with tests first:

```rust
use serde_json::Value;

const SECRET_KEYS: &[&str] = &[
    "authorization",
    "cookie",
    "set-cookie",
    "api_key",
    "apikey",
    "access_token",
    "refresh_token",
    "token",
    "secret",
    "session",
    "password",
];

pub fn redact_monitor_text(input: &str) -> String {
    let mut output = input.to_string();
    for marker in ["Bearer ", "sk-", "sess-", "eyJ"] {
        if let Some(index) = output.find(marker) {
            let end = output[index..]
                .find(char::is_whitespace)
                .map(|offset| index + offset)
                .unwrap_or(output.len());
            output.replace_range(index..end, "[redacted]");
        }
    }
    if output.len() > 500 {
        output.truncate(500);
        output.push_str("...");
    }
    output
}

pub fn redact_monitor_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    if SECRET_KEYS.iter().any(|secret| key.eq_ignore_ascii_case(secret)) {
                        (key.clone(), Value::String("[redacted]".to_string()))
                    } else {
                        (key.clone(), redact_monitor_json(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_monitor_json).collect()),
        Value::String(text) => Value::String(redact_monitor_text(text)),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_secret_fields_recursively() {
        let value = serde_json::json!({
            "authorization": "Bearer sk-live",
            "nested": { "refresh_token": "secret-refresh", "safe": "ok" }
        });

        let redacted = redact_monitor_json(&value);

        assert_eq!(redacted["authorization"], "[redacted]");
        assert_eq!(redacted["nested"]["refresh_token"], "[redacted]");
        assert_eq!(redacted["nested"]["safe"], "ok");
    }

    #[test]
    fn redacts_bearer_tokens_in_text() {
        let redacted = redact_monitor_text("upstream said Bearer sk-secret-value failed");

        assert!(!redacted.contains("sk-secret-value"));
        assert!(redacted.contains("[redacted]"));
    }
}
```

- [ ] **Step 2: Run service tests and verify failures are limited to missing module exports**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib channel_monitors -- --nocapture
```

Expected: compile failure until `mod.rs` and `services/mod.rs` are wired.

- [ ] **Step 3: Add service module exports**

Create `src-tauri/src/services/channel_monitors/mod.rs`:

```rust
pub mod probe;
pub mod redaction;
pub mod templates;
```

Modify `src-tauri/src/services/mod.rs`:

```rust
pub mod channel_monitors;
```

- [ ] **Step 4: Add probe client**

Create `src-tauri/src/services/channel_monitors/probe.rs`:

```rust
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{
    redaction::{redact_monitor_json, redact_monitor_text},
    templates::RenderedMonitorRequest,
};

#[derive(Debug, Clone)]
pub struct MonitorProbeResult {
    pub ok: bool,
    pub status_code: Option<i64>,
    pub latency_ms: i64,
    pub error_summary: Option<String>,
    pub response_excerpt_redacted: Option<String>,
}

pub fn run_monitor_probe(
    base_url: &str,
    api_key: &str,
    request: &RenderedMonitorRequest,
    timeout_seconds: i64,
) -> MonitorProbeResult {
    let started = Instant::now();
    let url = format!("{}{}", base_url.trim_end_matches('/'), request.path.as_str());
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(timeout_seconds.max(1) as u64))
        .build();
    let mut call = match request.method.as_str() {
        "POST" => agent.post(&url),
        "GET" => agent.get(&url),
        other => {
            return MonitorProbeResult {
                ok: false,
                status_code: None,
                latency_ms: 0,
                error_summary: Some(format!("Unsupported monitor method {other}")),
                response_excerpt_redacted: None,
            };
        }
    }
    .set("authorization", &format!("Bearer {api_key}"));

    for (key, value) in &request.headers {
        if key != "authorization" && key != "cookie" {
            call = call.set(key, value);
        }
    }

    let response = if request.method == "POST" {
        call.send_bytes(&request.body)
    } else {
        call.call()
    };
    let latency_ms = started.elapsed().as_millis() as i64;

    match response {
        Ok(response) => {
            let status = response.status() as i64;
            let text = response.into_string().unwrap_or_default();
            let excerpt = redacted_excerpt(&text);
            MonitorProbeResult {
                ok: status < 400,
                status_code: Some(status),
                latency_ms,
                error_summary: if status < 400 { None } else { Some(format!("HTTP {status}")) },
                response_excerpt_redacted: excerpt,
            }
        }
        Err(ureq::Error::Status(status, response)) => {
            let text = response.into_string().unwrap_or_default();
            MonitorProbeResult {
                ok: false,
                status_code: Some(status as i64),
                latency_ms,
                error_summary: Some(format!("HTTP {status}: {}", redact_monitor_text(&text))),
                response_excerpt_redacted: redacted_excerpt(&text),
            }
        }
        Err(error) => MonitorProbeResult {
            ok: false,
            status_code: None,
            latency_ms,
            error_summary: Some(redact_monitor_text(&error.to_string())),
            response_excerpt_redacted: None,
        },
    }
}

fn redacted_excerpt(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<Value>(text) {
        return Some(redact_monitor_json(&value).to_string());
    }
    Some(redact_monitor_text(text))
}
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib channel_monitors -- --nocapture
```

Expected: rendering and redaction tests pass.

- [ ] **Step 6: Commit Task 2**

```powershell
git add -- src-tauri/src/services/channel_monitors/mod.rs src-tauri/src/services/channel_monitors/templates.rs src-tauri/src/services/channel_monitors/redaction.rs src-tauri/src/services/channel_monitors/probe.rs src-tauri/src/services/mod.rs
git commit -m "feat: add channel monitor probe templates"
```

## Task 3: Manual Runs, Runner State, And Health Writeback

**Files:**
- Modify: `src-tauri/src/services/channel_monitors/mod.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write manual run tests**

Append to `src-tauri/src/services/channel_monitors/mod.rs`:

```rust
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    models::{
        channel_monitors::{ChannelMonitor, ChannelMonitorRun, CreateChannelMonitorRunInput},
        station_keys::KeyPoolItem,
    },
    services::{
        channel_monitors::{
            probe::run_monitor_probe,
            templates::{render_monitor_request, MonitorTemplateContext},
        },
        database::{now_millis_for_services, AppDatabase},
    },
};

#[derive(Debug, Default)]
pub struct ChannelMonitorRunnerState {
    inner: Mutex<ChannelMonitorRunnerInner>,
}

#[derive(Debug, Default)]
struct ChannelMonitorRunnerInner {
    running: bool,
    stop_signal: Option<Arc<AtomicBool>>,
    handle: Option<JoinHandle<()>>,
}

pub fn run_channel_monitor_now(
    database: &AppDatabase,
    data_key: &[u8; 32],
    monitor_id: &str,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let monitor = database.get_channel_monitor(monitor_id.to_string())?;
    run_monitor(database, data_key, &monitor)
}

fn run_monitor(
    database: &AppDatabase,
    data_key: &[u8; 32],
    monitor: &ChannelMonitor,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let keys = monitor_target_keys(database, monitor)?;
    if keys.is_empty() {
        return Ok(vec![database.insert_channel_monitor_run(CreateChannelMonitorRunInput {
            monitor_id: monitor.id.clone(),
            parent_run_id: None,
            station_id: monitor.station_id.clone(),
            station_key_id: None,
            template_id: monitor.template_id.clone(),
            target_type: monitor.target_type.clone(),
            model: Some(monitor.primary_model.clone()),
            status: "skipped".to_string(),
            status_code: None,
            latency_ms: None,
            error_summary: Some("No enabled Station Keys matched this monitor".to_string()),
            response_excerpt_redacted: None,
            checked_at: now_millis_for_services().to_string(),
        })?]);
    }

    let template = database.get_channel_monitor_template(monitor.template_id.clone())?;
    let mut runs = Vec::new();
    for key in keys {
        let checked_at = now_millis_for_services().to_string();
        let result = if !key.api_key_present {
            crate::services::channel_monitors::probe::MonitorProbeResult {
                ok: false,
                status_code: None,
                latency_ms: 0,
                error_summary: Some("Station Key has no saved API key".to_string()),
                response_excerpt_redacted: None,
            }
        } else {
            let api_key = database.resolve_station_key_secret_with_data_key(data_key, &key.id)?;
            let request = render_monitor_request(
                &template,
                &MonitorTemplateContext {
                    model: monitor.primary_model.clone(),
                    max_tokens: template.default_max_tokens,
                    stream: template.default_stream,
                    challenge: "Reply ok.".to_string(),
                },
            )?;
            run_monitor_probe(&key.station_base_url, &api_key, &request, monitor.timeout_seconds)
        };
        if result.ok {
            database.record_station_key_success(&key.id, result.latency_ms, &checked_at)?;
        } else {
            database.record_station_key_failure(
                &key.id,
                result.error_summary.as_deref().unwrap_or("Monitor probe failed"),
                &checked_at,
            )?;
        }
        let run = database.insert_channel_monitor_run(CreateChannelMonitorRunInput {
            monitor_id: monitor.id.clone(),
            parent_run_id: None,
            station_id: key.station_id.clone(),
            station_key_id: Some(key.id.clone()),
            template_id: monitor.template_id.clone(),
            target_type: "station_key".to_string(),
            model: Some(monitor.primary_model.clone()),
            status: if result.ok { "success" } else { "failed" }.to_string(),
            status_code: result.status_code,
            latency_ms: Some(result.latency_ms),
            error_summary: result.error_summary,
            response_excerpt_redacted: result.response_excerpt_redacted,
            checked_at,
        })?;
        database.update_channel_monitor_after_run(
            &monitor.id,
            &run.status,
            run.latency_ms,
            run.error_summary.as_deref(),
        )?;
        runs.push(run);
    }
    database.schedule_next_channel_monitor_run(&monitor.id)?;
    Ok(runs)
}

fn monitor_target_keys(database: &AppDatabase, monitor: &ChannelMonitor) -> Result<Vec<KeyPoolItem>, String> {
    let keys = database.list_key_pool_items()?;
    let filtered = keys
        .into_iter()
        .filter(|key| key.enabled)
        .filter(|key| key.station_id == monitor.station_id)
        .filter(|key| match monitor.target_type.as_str() {
            "station_key" => monitor.station_key_id.as_deref() == Some(key.id.as_str()),
            "station" => true,
            _ => false,
        })
        .collect();
    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    use crate::{
        models::{
            channel_monitors::CreateChannelMonitorInput,
            stations::CreateStationInput,
        },
        services::secrets::crypto::generate_data_key,
    };

    #[test]
    fn manual_monitor_run_updates_station_key_health() {
        let upstream = success_upstream();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = database
            .create_station(CreateStationInput {
                name: "monitor-run".to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: upstream.base_url.clone(),
                api_key: "sk-monitor-run".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                note: None,
            })
            .expect("station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Run now".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(key.id.clone()),
                template_id: "builtin-openai-chat-low-token".to_string(),
                primary_model: "gpt-4o-mini".to_string(),
                fallback_models: Vec::new(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 10,
                max_concurrency: 1,
                failure_cooldown_seconds: 300,
                consecutive_failure_threshold: 3,
            })
            .expect("monitor");

        let runs = run_channel_monitor_now(&database, &generate_data_key(), &monitor.id).expect("run");
        let health = database.get_station_key_health(key.id).expect("health");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert_eq!(health.success_count, 1);
        assert_eq!(health.failure_count, 0);
        upstream.join();
    }

    struct TestUpstream {
        base_url: String,
        handle: JoinHandle<()>,
    }

    impl TestUpstream {
        fn join(self) {
            self.handle.join().expect("join upstream");
        }
    }

    fn success_upstream() -> TestUpstream {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let port = listener.local_addr().expect("addr").port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0_u8; 2048];
            let _ = stream.read(&mut buf);
            let body = br#"{"choices":[{"message":{"content":"ok"}}]}"#;
            let header = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).expect("header");
            stream.write_all(body).expect("body");
        });
        TestUpstream {
            base_url: format!("http://127.0.0.1:{port}"),
            handle,
        }
    }
}
```

- [ ] **Step 2: Run tests and verify missing database methods fail**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib manual_monitor_run_updates_station_key_health -- --nocapture
```

Expected: compile failure for `get_channel_monitor`, `get_channel_monitor_template`, `update_channel_monitor_after_run`, or `schedule_next_channel_monitor_run`.

- [ ] **Step 3: Add required database helpers**

Add these methods to `impl AppDatabase`:

```rust
    pub fn get_channel_monitor(&self, id: String) -> Result<ChannelMonitor, String> {
        let connection = self.connection()?;
        get_channel_monitor_from_connection(&connection, &id)
    }

    pub fn get_channel_monitor_template(
        &self,
        id: String,
    ) -> Result<ChannelMonitorRequestTemplate, String> {
        let connection = self.connection()?;
        get_channel_monitor_template_from_connection(&connection, &id)
    }

    pub fn update_channel_monitor_after_run(
        &self,
        id: &str,
        status: &str,
        latency_ms: Option<i64>,
        error_summary: Option<&str>,
    ) -> Result<(), String> {
        let connection = self.connection()?;
        let now = now_millis_for_services().to_string();
        connection
            .execute(
                "UPDATE channel_monitors
                    SET last_run_at = ?2,
                        last_status = ?3,
                        last_latency_ms = ?4,
                        last_error_summary = ?5,
                        updated_at = ?2
                  WHERE id = ?1",
                params![id, now, status, latency_ms, error_summary],
            )
            .map_err(|error| format!("Update channel monitor run summary failed: {error}"))?;
        Ok(())
    }

    pub fn schedule_next_channel_monitor_run(&self, id: &str) -> Result<(), String> {
        let connection = self.connection()?;
        let monitor = get_channel_monitor_from_connection(&connection, id)?;
        let next_run = (now_millis_for_services() + monitor.interval_seconds * 1000).to_string();
        connection
            .execute(
                "UPDATE channel_monitors SET next_run_at = ?2, updated_at = ?2 WHERE id = ?1",
                params![id, next_run],
            )
            .map_err(|error| format!("Schedule next channel monitor run failed: {error}"))?;
        Ok(())
    }
```

Add `get_channel_monitor_from_connection` and `get_channel_monitor_template_from_connection` next to list functions using the same row mapping helpers created in Task 1.

- [ ] **Step 4: Add runner start/stop state**

Extend `ChannelMonitorRunnerState` in `src-tauri/src/services/channel_monitors/mod.rs`:

```rust
impl ChannelMonitorRunnerState {
    pub fn start(&self, database: AppDatabase, data_key: [u8; 32]) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Channel monitor runner lock is damaged".to_string())?;
        if inner.running {
            return Ok(());
        }
        let stop_signal = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop_signal);
        let handle = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                if let Ok(monitors) = database.due_channel_monitors(now_millis_for_services().to_string()) {
                    for monitor in monitors {
                        if thread_stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let _ = run_monitor(&database, &data_key, &monitor);
                    }
                }
                thread::sleep(Duration::from_secs(5));
            }
        });
        inner.running = true;
        inner.stop_signal = Some(stop_signal);
        inner.handle = Some(handle);
        Ok(())
    }

    pub fn stop(&self) {
        let handle = {
            let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(stop_signal) = &inner.stop_signal {
                stop_signal.store(true, Ordering::Relaxed);
            }
            inner.running = false;
            inner.stop_signal = None;
            inner.handle.take()
        };
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}
```

Add `due_channel_monitors` to `AppDatabase`:

```rust
    pub fn due_channel_monitors(&self, now: String) -> Result<Vec<ChannelMonitor>, String> {
        let connection = self.connection()?;
        due_channel_monitors_from_connection(&connection, &now)
    }
```

Implement `due_channel_monitors_from_connection` with:

```sql
SELECT ...
  FROM channel_monitors
 WHERE enabled = 1
   AND (next_run_at IS NULL OR next_run_at <= ?1)
 ORDER BY COALESCE(next_run_at, '0') ASC
 LIMIT 10
```

- [ ] **Step 5: Start runner in Tauri setup**

Modify `src-tauri/src/lib.rs` setup block after managing the proxy state:

```rust
            let monitor_runner = services::channel_monitors::ChannelMonitorRunnerState::default();
            monitor_runner.start(database.inner().clone(), *secret_manager.data_key())?;
            app.manage(monitor_runner);
```

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml --lib manual_monitor_run_updates_station_key_health -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml --lib channel_monitor -- --nocapture
```

Expected: both pass.

- [ ] **Step 7: Commit Task 3**

```powershell
git add -- src-tauri/src/services/channel_monitors/mod.rs src-tauri/src/services/database.rs src-tauri/src/lib.rs
git commit -m "feat: run channel monitors"
```

## Task 4: Tauri Commands And Frontend API Wrappers

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/lib/types/channelMonitors.ts`
- Create: `src/lib/api/channelMonitors.ts`

- [ ] **Step 1: Add command imports**

In `src-tauri/src/commands/mod.rs`, extend model imports:

```rust
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            CreateChannelMonitorInput, CreateChannelMonitorTemplateInput,
            UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
        },
```

- [ ] **Step 2: Add Tauri command functions**

Add after existing health commands:

```rust
#[tauri::command]
pub fn list_channel_monitors(database: State<'_, AppDatabase>) -> Result<Vec<ChannelMonitor>, String> {
    database.list_channel_monitors()
}

#[tauri::command]
pub fn create_channel_monitor(
    database: State<'_, AppDatabase>,
    input: CreateChannelMonitorInput,
) -> Result<ChannelMonitor, String> {
    database.create_channel_monitor(input)
}

#[tauri::command]
pub fn update_channel_monitor(
    database: State<'_, AppDatabase>,
    input: UpdateChannelMonitorInput,
) -> Result<ChannelMonitor, String> {
    database.update_channel_monitor(input)
}

#[tauri::command]
pub fn delete_channel_monitor(database: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    database.delete_channel_monitor(id)
}

#[tauri::command]
pub fn list_channel_monitor_runs(
    database: State<'_, AppDatabase>,
    monitor_id: String,
) -> Result<Vec<ChannelMonitorRun>, String> {
    database.list_channel_monitor_runs(monitor_id)
}

#[tauri::command]
pub fn list_channel_monitor_templates(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ChannelMonitorRequestTemplate>, String> {
    database.list_channel_monitor_templates()
}

#[tauri::command]
pub fn create_channel_monitor_template(
    database: State<'_, AppDatabase>,
    input: CreateChannelMonitorTemplateInput,
) -> Result<ChannelMonitorRequestTemplate, String> {
    database.create_channel_monitor_template(input)
}

#[tauri::command]
pub fn update_channel_monitor_template(
    database: State<'_, AppDatabase>,
    input: UpdateChannelMonitorTemplateInput,
) -> Result<ChannelMonitorRequestTemplate, String> {
    database.update_channel_monitor_template(input)
}

#[tauri::command]
pub fn duplicate_channel_monitor_template(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<ChannelMonitorRequestTemplate, String> {
    database.duplicate_channel_monitor_template(id)
}

#[tauri::command]
pub fn delete_channel_monitor_template(database: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    database.delete_channel_monitor_template(id)
}

#[tauri::command]
pub async fn run_channel_monitor_now(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    monitor_id: String,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::channel_monitors::run_channel_monitor_now(&database, &data_key, &monitor_id)
    })
    .await
    .map_err(|error| format!("Channel monitor run task failed: {error}"))?
}
```

- [ ] **Step 3: Register commands in `src-tauri/src/lib.rs`**

Add entries in `tauri::generate_handler!` near station key health commands:

```rust
            commands::list_channel_monitors,
            commands::create_channel_monitor,
            commands::update_channel_monitor,
            commands::delete_channel_monitor,
            commands::run_channel_monitor_now,
            commands::list_channel_monitor_runs,
            commands::list_channel_monitor_templates,
            commands::create_channel_monitor_template,
            commands::update_channel_monitor_template,
            commands::duplicate_channel_monitor_template,
            commands::delete_channel_monitor_template,
```

- [ ] **Step 4: Add frontend types**

Create `src/lib/types/channelMonitors.ts`:

```typescript
export type ChannelMonitorTargetType = "station_key" | "station";
export type ChannelMonitorRunStatus = "unchecked" | "success" | "warning" | "failed" | "skipped";

export type ChannelMonitor = {
  id: string;
  name: string;
  targetType: ChannelMonitorTargetType;
  stationId: string;
  stationKeyId: string | null;
  templateId: string;
  primaryModel: string;
  fallbackModels: string[];
  enabled: boolean;
  intervalSeconds: number;
  jitterSeconds: number;
  timeoutSeconds: number;
  maxConcurrency: number;
  failureCooldownSeconds: number;
  consecutiveFailureThreshold: number;
  lastRunAt: string | null;
  nextRunAt: string | null;
  lastStatus: ChannelMonitorRunStatus;
  lastLatencyMs: number | null;
  lastErrorSummary: string | null;
  createdAt: string;
  updatedAt: string;
};

export type CreateChannelMonitorInput = Omit<
  ChannelMonitor,
  "id" | "lastRunAt" | "nextRunAt" | "lastStatus" | "lastLatencyMs" | "lastErrorSummary" | "createdAt" | "updatedAt"
>;

export type UpdateChannelMonitorInput = CreateChannelMonitorInput & { id: string };

export type ChannelMonitorRequestTemplate = {
  id: string;
  name: string;
  provider: string;
  protocol: string;
  method: string;
  path: string;
  headersJson: string;
  bodyTemplateJson: string;
  modelFieldPath: string;
  streamFieldPath: string | null;
  maxTokensFieldPath: string | null;
  defaultMaxTokens: number;
  defaultStream: boolean;
  successRuleJson: string;
  errorExtractRuleJson: string;
  description: string | null;
  builtin: boolean;
  enabled: boolean;
  defaultForProtocol: boolean;
  version: number;
  linkedMonitorCount: number;
  createdAt: string;
  updatedAt: string;
};

export type CreateChannelMonitorTemplateInput = Omit<
  ChannelMonitorRequestTemplate,
  "id" | "builtin" | "version" | "linkedMonitorCount" | "createdAt" | "updatedAt"
>;

export type UpdateChannelMonitorTemplateInput = CreateChannelMonitorTemplateInput & { id: string };

export type ChannelMonitorRun = {
  id: string;
  monitorId: string;
  parentRunId: string | null;
  stationId: string;
  stationKeyId: string | null;
  templateId: string;
  targetType: ChannelMonitorTargetType;
  model: string | null;
  status: Exclude<ChannelMonitorRunStatus, "unchecked">;
  statusCode: number | null;
  latencyMs: number | null;
  errorSummary: string | null;
  responseExcerptRedacted: string | null;
  checkedAt: string;
  createdAt: string;
};
```

- [ ] **Step 5: Add frontend API wrappers**

Create `src/lib/api/channelMonitors.ts`:

```typescript
import { invoke } from "@tauri-apps/api/core";
import type {
  ChannelMonitor,
  ChannelMonitorRequestTemplate,
  ChannelMonitorRun,
  CreateChannelMonitorInput,
  CreateChannelMonitorTemplateInput,
  UpdateChannelMonitorInput,
  UpdateChannelMonitorTemplateInput,
} from "@/lib/types/channelMonitors";

let memoryMonitors: ChannelMonitor[] = [];
let memoryTemplates: ChannelMonitorRequestTemplate[] = builtinTemplates();
let memoryRuns: ChannelMonitorRun[] = [];

export function listChannelMonitors() {
  return invoke<ChannelMonitor[]>("list_channel_monitors").catch((error) => {
    if (isInvokeUnavailable(error)) return memoryMonitors;
    throw error;
  });
}

export function createChannelMonitor(input: CreateChannelMonitorInput) {
  return invoke<ChannelMonitor>("create_channel_monitor", { input }).catch((error) => {
    if (!isInvokeUnavailable(error)) throw error;
    const now = new Date().toISOString();
    const next: ChannelMonitor = {
      ...input,
      id: `monitor-${Date.now()}`,
      lastRunAt: null,
      nextRunAt: null,
      lastStatus: "unchecked",
      lastLatencyMs: null,
      lastErrorSummary: null,
      createdAt: now,
      updatedAt: now,
    };
    memoryMonitors = [next, ...memoryMonitors];
    return next;
  });
}

export function updateChannelMonitor(input: UpdateChannelMonitorInput) {
  return invoke<ChannelMonitor>("update_channel_monitor", { input }).catch((error) => {
    if (!isInvokeUnavailable(error)) throw error;
    const now = new Date().toISOString();
    const previous = memoryMonitors.find((monitor) => monitor.id === input.id);
    const next: ChannelMonitor = {
      ...(previous ?? {
        lastRunAt: null,
        nextRunAt: null,
        lastStatus: "unchecked" as const,
        lastLatencyMs: null,
        lastErrorSummary: null,
        createdAt: now,
      }),
      ...input,
      updatedAt: now,
    };
    memoryMonitors = memoryMonitors.map((monitor) => (monitor.id === next.id ? next : monitor));
    return next;
  });
}

export function deleteChannelMonitor(id: string) {
  return invoke<void>("delete_channel_monitor", { id }).catch((error) => {
    if (!isInvokeUnavailable(error)) throw error;
    memoryMonitors = memoryMonitors.filter((monitor) => monitor.id !== id);
  });
}

export function runChannelMonitorNow(monitorId: string) {
  return invoke<ChannelMonitorRun[]>("run_channel_monitor_now", { monitorId }).catch((error) => {
    if (!isInvokeUnavailable(error)) throw error;
    const monitor = memoryMonitors.find((item) => item.id === monitorId);
    if (!monitor) return [];
    const now = new Date().toISOString();
    const run: ChannelMonitorRun = {
      id: `run-${Date.now()}`,
      monitorId,
      parentRunId: null,
      stationId: monitor.stationId,
      stationKeyId: monitor.stationKeyId,
      templateId: monitor.templateId,
      targetType: monitor.targetType,
      model: monitor.primaryModel,
      status: "success",
      statusCode: 200,
      latencyMs: 120,
      errorSummary: null,
      responseExcerptRedacted: "{\"ok\":true}",
      checkedAt: now,
      createdAt: now,
    };
    memoryRuns = [run, ...memoryRuns];
    memoryMonitors = memoryMonitors.map((item) =>
      item.id === monitorId ? { ...item, lastRunAt: now, lastStatus: "success", lastLatencyMs: 120, updatedAt: now } : item,
    );
    return [run];
  });
}

export function listChannelMonitorRuns(monitorId: string) {
  return invoke<ChannelMonitorRun[]>("list_channel_monitor_runs", { monitorId }).catch((error) => {
    if (isInvokeUnavailable(error)) return memoryRuns.filter((run) => run.monitorId === monitorId);
    throw error;
  });
}

export function listChannelMonitorTemplates() {
  return invoke<ChannelMonitorRequestTemplate[]>("list_channel_monitor_templates").catch((error) => {
    if (isInvokeUnavailable(error)) return memoryTemplates;
    throw error;
  });
}

export function createChannelMonitorTemplate(input: CreateChannelMonitorTemplateInput) {
  return invoke<ChannelMonitorRequestTemplate>("create_channel_monitor_template", { input }).catch((error) => {
    if (!isInvokeUnavailable(error)) throw error;
    const now = new Date().toISOString();
    const next: ChannelMonitorRequestTemplate = {
      ...input,
      id: `template-${Date.now()}`,
      builtin: false,
      version: 1,
      linkedMonitorCount: 0,
      createdAt: now,
      updatedAt: now,
    };
    memoryTemplates = [next, ...memoryTemplates];
    return next;
  });
}

export function updateChannelMonitorTemplate(input: UpdateChannelMonitorTemplateInput) {
  return invoke<ChannelMonitorRequestTemplate>("update_channel_monitor_template", { input }).catch((error) => {
    if (!isInvokeUnavailable(error)) throw error;
    const now = new Date().toISOString();
    const previous = memoryTemplates.find((template) => template.id === input.id);
    if (previous?.builtin) throw new Error("内置模板不能直接编辑，请先复制。");
    const next: ChannelMonitorRequestTemplate = {
      ...(previous ?? { builtin: false, version: 1, linkedMonitorCount: 0, createdAt: now }),
      ...input,
      updatedAt: now,
    };
    memoryTemplates = memoryTemplates.map((template) => (template.id === next.id ? next : template));
    return next;
  });
}

export function duplicateChannelMonitorTemplate(id: string) {
  return invoke<ChannelMonitorRequestTemplate>("duplicate_channel_monitor_template", { id }).catch((error) => {
    if (!isInvokeUnavailable(error)) throw error;
    const source = memoryTemplates.find((template) => template.id === id);
    if (!source) throw new Error("模板不存在");
    return createChannelMonitorTemplate({
      ...source,
      name: `${source.name} 副本`,
      builtin: undefined as never,
      version: undefined as never,
      linkedMonitorCount: undefined as never,
      createdAt: undefined as never,
      updatedAt: undefined as never,
      id: undefined as never,
    });
  });
}

export function deleteChannelMonitorTemplate(id: string) {
  return invoke<void>("delete_channel_monitor_template", { id }).catch((error) => {
    if (!isInvokeUnavailable(error)) throw error;
    const template = memoryTemplates.find((item) => item.id === id);
    if (template?.builtin) throw new Error("内置模板不能删除");
    if (memoryMonitors.some((monitor) => monitor.templateId === id)) throw new Error("模板正在被监控使用");
    memoryTemplates = memoryTemplates.filter((item) => item.id !== id);
  });
}

function builtinTemplates(): ChannelMonitorRequestTemplate[] {
  const now = new Date().toISOString();
  return [
    {
      id: "builtin-openai-chat-low-token",
      name: "OpenAI Compatible 低 token 检测",
      provider: "openai",
      protocol: "openai_chat_completions",
      method: "POST",
      path: "/v1/chat/completions",
      headersJson: "{\"content-type\":\"application/json\"}",
      bodyTemplateJson: "{\"model\":\"{{model}}\",\"messages\":[{\"role\":\"user\",\"content\":\"Reply ok.\"}],\"max_tokens\":1,\"stream\":false}",
      modelFieldPath: "model",
      streamFieldPath: "stream",
      maxTokensFieldPath: "max_tokens",
      defaultMaxTokens: 1,
      defaultStream: false,
      successRuleJson: "{\"okStatusMax\":399,\"requireJson\":true}",
      errorExtractRuleJson: "{\"messagePaths\":[\"error.message\",\"message\"]}",
      description: "最小 chat completions 探针。",
      builtin: true,
      enabled: true,
      defaultForProtocol: true,
      version: 1,
      linkedMonitorCount: 0,
      createdAt: now,
      updatedAt: now,
    },
  ];
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
```

After writing the file, fix the `duplicateChannelMonitorTemplate` fallback if TypeScript rejects the `undefined as never` omission pattern by replacing it with an explicit object that copies only `CreateChannelMonitorTemplateInput` fields.

- [ ] **Step 6: Run checks**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm.cmd build
```

Expected: both pass.

- [ ] **Step 7: Commit Task 4**

```powershell
git add -- src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/types/channelMonitors.ts src/lib/api/channelMonitors.ts
git commit -m "feat: expose channel monitor api"
```

## Task 5: Split Current Status View Into A Status Tab

**Files:**
- Modify: `src/features/channels/ChannelStatusPage.tsx`
- Create: `src/features/channels/ChannelStatusTab.tsx`

- [ ] **Step 1: Create `ChannelStatusTab.tsx` from the existing status implementation**

Move the current imports, types, `refresh` data flow, card rendering, and helper functions from `ChannelStatusPage.tsx` into `ChannelStatusTab.tsx`. The exported component signature should be:

```tsx
type ChannelStatusTabProps = {
  refreshToken: number;
};

export function ChannelStatusTab({ refreshToken }: ChannelStatusTabProps) {
  const toast = useToast();
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [logs, setLogs] = useState<RequestLog[]>([]);
  const [health, setHealth] = useState<StationKeyHealth[]>([]);
  const [timeWindow, setTimeWindow] = useState<ChannelWindow>("recent");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, [refreshToken]);

  const visibleLogs = useMemo(() => filterLogsByWindow(logs, timeWindow), [logs, timeWindow]);
  const channels = useMemo(() => buildChannels(keys, visibleLogs, health), [health, keys, visibleLogs]);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const [nextKeys, nextLogs, nextHealth] = await Promise.all([
        listKeyPoolItems(),
        listRequestLogs(),
        listStationKeyHealth(),
      ]);
      setKeys(nextKeys);
      setLogs(nextLogs);
      setHealth(nextHealth);
      if (showSuccess) toast.success("渠道状态已刷新");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新渠道状态失败", message);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-3">
      <div className="flex justify-end">
        <div className="flex items-center gap-2">
          <SegmentedControl
            ariaLabel="渠道状态范围"
            value={timeWindow}
            options={[
              { value: "recent", label: "最近请求" },
              { value: "24h", label: "24 小时" },
              { value: "7d", label: "7 天" },
            ]}
            onChange={setTimeWindow}
          />
          <Button variant="secondary" onClick={() => void refresh(true)}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      </div>

      {error && <div className="rounded-xl border border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
      {channels.length === 0 ? (
        <EmptyState
          title={loading ? "正在读取渠道状态" : "暂无可展示的密钥"}
          description="添加并启用密钥后，本地代理请求会在这里形成状态。"
        />
      ) : (
        <div className="grid gap-3 md:grid-cols-2 2xl:grid-cols-3">
          {channels.map((channel) => (
            <ChannelHealthCard key={channel.id} channel={channel} />
          ))}
        </div>
      )}
    </div>
  );
}
```

Preserve the existing `ChannelHealthCard`, `ChannelMetric`, `buildChannels`, `filterLogsByWindow`, and formatting helpers in this file.

- [ ] **Step 2: Replace `ChannelStatusPage.tsx` with a coordinator**

Use this structure:

```tsx
import { useState } from "react";
import { Activity, Radar } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { SegmentedControl } from "@/components/ui";
import { ChannelMonitoringTab } from "./ChannelMonitoringTab";
import { ChannelStatusTab } from "./ChannelStatusTab";

type ChannelPageTab = "status" | "monitoring";

export function ChannelStatusPage() {
  const [activeTab, setActiveTab] = useState<ChannelPageTab>("status");
  const [statusRefreshToken, setStatusRefreshToken] = useState(0);

  return (
    <PageScaffold
      title="渠道状态"
      actions={
        <SegmentedControl
          ariaLabel="渠道页面"
          value={activeTab}
          options={[
            { value: "status", label: "状态" },
            { value: "monitoring", label: "监控" },
          ]}
          onChange={setActiveTab}
        />
      }
    >
      {activeTab === "status" ? (
        <ChannelStatusTab refreshToken={statusRefreshToken} />
      ) : (
        <ChannelMonitoringTab onHealthChanged={() => setStatusRefreshToken((value) => value + 1)} />
      )}
    </PageScaffold>
  );
}
```

Remove unused icon imports if TypeScript reports them.

- [ ] **Step 3: Temporarily stub `ChannelMonitoringTab` to keep build green**

Create `src/features/channels/ChannelMonitoringTab.tsx`:

```tsx
import { EmptyState } from "@/components/ui";

type ChannelMonitoringTabProps = {
  onHealthChanged: () => void;
};

export function ChannelMonitoringTab({ onHealthChanged: _onHealthChanged }: ChannelMonitoringTabProps) {
  return <EmptyState title="暂无监控任务" description="新建监控后，这里会展示定时检测状态。" />;
}
```

- [ ] **Step 4: Run frontend build**

Run:

```powershell
pnpm.cmd build
```

Expected: TypeScript and Vite build pass.

- [ ] **Step 5: Commit Task 5**

```powershell
git add -- src/features/channels/ChannelStatusPage.tsx src/features/channels/ChannelStatusTab.tsx src/features/channels/ChannelMonitoringTab.tsx
git commit -m "feat: split channel status tabs"
```

## Task 6: Monitoring Tab And Monitor Form

**Files:**
- Modify: `src/features/channels/ChannelMonitoringTab.tsx`
- Create: `src/features/channels/ChannelMonitorForm.tsx`
- Create: `src/features/channels/channelMonitorViewModel.ts`

- [ ] **Step 1: Add view model helpers**

Create `src/features/channels/channelMonitorViewModel.ts`:

```typescript
import type { ChannelMonitor, ChannelMonitorRequestTemplate } from "@/lib/types/channelMonitors";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export const monitorStatusLabel = {
  unchecked: "未检测",
  success: "正常",
  warning: "警告",
  failed: "失败",
  skipped: "跳过",
} as const;

export const monitorStatusTone = {
  unchecked: "info",
  success: "healthy",
  warning: "warning",
  failed: "error",
  skipped: "disabled",
} as const;

export function monitorTargetLabel(monitor: ChannelMonitor, stations: Station[], keys: KeyPoolItem[]) {
  const station = stations.find((item) => item.id === monitor.stationId);
  if (monitor.targetType === "station") {
    return `${station?.name ?? "未知中转站"} / 全部启用 Key`;
  }
  const key = keys.find((item) => item.id === monitor.stationKeyId);
  return `${station?.name ?? "未知中转站"} / ${key?.name ?? "未知 Key"}`;
}

export function templateLabel(templateId: string, templates: ChannelMonitorRequestTemplate[]) {
  return templates.find((template) => template.id === templateId)?.name ?? "未知模板";
}

export function formatMonitorInterval(monitor: ChannelMonitor) {
  return monitor.jitterSeconds > 0
    ? `${monitor.intervalSeconds}s ± ${monitor.jitterSeconds}s`
    : `${monitor.intervalSeconds}s`;
}

export function formatMonitorTime(value: string | null) {
  if (!value) return "--";
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function validateMonitorDraft(draft: {
  name: string;
  targetType: "station_key" | "station";
  stationId: string;
  stationKeyId: string | null;
  templateId: string;
  primaryModel: string;
  intervalSeconds: number;
  jitterSeconds: number;
  timeoutSeconds: number;
}) {
  if (!draft.name.trim()) return "请输入监控名称";
  if (!draft.stationId) return "请选择中转站";
  if (draft.targetType === "station_key" && !draft.stationKeyId) return "请选择 Station Key";
  if (!draft.templateId) return "请选择请求模板";
  if (!draft.primaryModel.trim()) return "请输入主模型";
  if (draft.intervalSeconds < 15 || draft.intervalSeconds > 3600) return "检测间隔需在 15 - 3600 秒之间";
  if (draft.jitterSeconds < 0 || draft.jitterSeconds > 600) return "随机抖动需在 0 - 600 秒之间";
  if (draft.intervalSeconds - draft.jitterSeconds < 15) return "检测间隔减去抖动后至少 15 秒";
  if (draft.timeoutSeconds < 5 || draft.timeoutSeconds > 120) return "超时时间需在 5 - 120 秒之间";
  return null;
}
```

- [ ] **Step 2: Create monitor form**

Create `src/features/channels/ChannelMonitorForm.tsx` using existing `Dialog`, `Button`, `SelectControl`, `SwitchControl`:

```tsx
import { useMemo, useState } from "react";
import { Button, Dialog, SelectControl, SwitchControl } from "@/components/ui";
import type { ChannelMonitor, ChannelMonitorRequestTemplate, CreateChannelMonitorInput } from "@/lib/types/channelMonitors";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { validateMonitorDraft } from "./channelMonitorViewModel";

type ChannelMonitorFormProps = {
  open: boolean;
  monitor: ChannelMonitor | null;
  stations: Station[];
  keys: KeyPoolItem[];
  templates: ChannelMonitorRequestTemplate[];
  saving: boolean;
  onClose: () => void;
  onSubmit: (input: CreateChannelMonitorInput, id: string | null) => void;
};

export function ChannelMonitorForm({
  open,
  monitor,
  stations,
  keys,
  templates,
  saving,
  onClose,
  onSubmit,
}: ChannelMonitorFormProps) {
  const [draft, setDraft] = useState(() => monitorToDraft(monitor));
  const stationKeys = useMemo(() => keys.filter((key) => key.stationId === draft.stationId), [draft.stationId, keys]);
  const error = validateMonitorDraft(draft);

  function submit() {
    if (error) return;
    onSubmit(
      {
        ...draft,
        fallbackModels: draft.fallbackModelsText
          .split(/\r?\n|,/)
          .map((item) => item.trim())
          .filter(Boolean),
        maxConcurrency: draft.targetType === "station" ? draft.maxConcurrency : 1,
        failureCooldownSeconds: draft.failureCooldownSeconds,
        consecutiveFailureThreshold: draft.consecutiveFailureThreshold,
      },
      monitor?.id ?? null,
    );
  }

  return (
    <Dialog open={open} title={monitor ? "编辑渠道监控" : "新增渠道监控"} onClose={onClose}>
      <div className="space-y-3">
        <label className="block text-sm font-medium text-slate-700">
          名称
          <input
            className="mt-1 h-9 w-full rounded-[8px] border border-border bg-white px-3 text-sm outline-none focus:ring-2 focus:ring-[hsl(var(--accent)/0.22)]"
            value={draft.name}
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
          />
        </label>

        <SelectControl
          label="目标类型"
          value={draft.targetType}
          options={[
            { value: "station_key", label: "单个 Key" },
            { value: "station", label: "中转站全部 Key" },
          ]}
          onChange={(targetType) => setDraft({ ...draft, targetType: targetType as "station_key" | "station", stationKeyId: null })}
        />

        <SelectControl
          label="中转站"
          value={draft.stationId}
          options={stations.map((station) => ({ value: station.id, label: station.name }))}
          onChange={(stationId) => setDraft({ ...draft, stationId, stationKeyId: null })}
        />

        {draft.targetType === "station_key" && (
          <SelectControl
            label="Station Key"
            value={draft.stationKeyId ?? ""}
            options={stationKeys.map((key) => ({ value: key.id, label: key.name }))}
            onChange={(stationKeyId) => setDraft({ ...draft, stationKeyId })}
          />
        )}

        <SelectControl
          label="请求模板"
          value={draft.templateId}
          options={templates.filter((template) => template.enabled).map((template) => ({ value: template.id, label: template.name }))}
          onChange={(templateId) => setDraft({ ...draft, templateId })}
        />

        <div className="grid gap-3 md:grid-cols-3">
          <NumberField label="检测间隔（秒）" value={draft.intervalSeconds} onChange={(intervalSeconds) => setDraft({ ...draft, intervalSeconds })} />
          <NumberField label="随机抖动（秒）" value={draft.jitterSeconds} onChange={(jitterSeconds) => setDraft({ ...draft, jitterSeconds })} />
          <NumberField label="超时（秒）" value={draft.timeoutSeconds} onChange={(timeoutSeconds) => setDraft({ ...draft, timeoutSeconds })} />
        </div>

        <label className="block text-sm font-medium text-slate-700">
          主模型
          <input
            className="mt-1 h-9 w-full rounded-[8px] border border-border bg-white px-3 text-sm outline-none focus:ring-2 focus:ring-[hsl(var(--accent)/0.22)]"
            value={draft.primaryModel}
            onChange={(event) => setDraft({ ...draft, primaryModel: event.target.value })}
          />
        </label>

        <label className="block text-sm font-medium text-slate-700">
          备用模型
          <textarea
            className="mt-1 min-h-20 w-full rounded-[8px] border border-border bg-white px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-[hsl(var(--accent)/0.22)]"
            value={draft.fallbackModelsText}
            onChange={(event) => setDraft({ ...draft, fallbackModelsText: event.target.value })}
          />
        </label>

        <SwitchControl checked={draft.enabled} onCheckedChange={(enabled) => setDraft({ ...draft, enabled })} label="启用监控" />

        {error && <div className="rounded-[8px] border border-amber-100 bg-amber-50 px-3 py-2 text-sm text-amber-700">{error}</div>}

        <div className="flex justify-end gap-2 border-t border-border pt-3">
          <Button variant="secondary" onClick={onClose}>取消</Button>
          <Button variant="primary" disabled={saving || Boolean(error)} onClick={submit}>
            {monitor ? "保存" : "创建"}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}

function NumberField({ label, value, onChange }: { label: string; value: number; onChange: (value: number) => void }) {
  return (
    <label className="block text-sm font-medium text-slate-700">
      {label}
      <input
        type="number"
        className="mt-1 h-9 w-full rounded-[8px] border border-border bg-white px-3 text-sm outline-none focus:ring-2 focus:ring-[hsl(var(--accent)/0.22)]"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
      />
    </label>
  );
}

function monitorToDraft(monitor: ChannelMonitor | null) {
  return {
    name: monitor?.name ?? "",
    targetType: monitor?.targetType ?? "station_key",
    stationId: monitor?.stationId ?? "",
    stationKeyId: monitor?.stationKeyId ?? null,
    templateId: monitor?.templateId ?? "",
    primaryModel: monitor?.primaryModel ?? "gpt-4o-mini",
    fallbackModelsText: monitor?.fallbackModels.join("\n") ?? "",
    enabled: monitor?.enabled ?? true,
    intervalSeconds: monitor?.intervalSeconds ?? 60,
    jitterSeconds: monitor?.jitterSeconds ?? 0,
    timeoutSeconds: monitor?.timeoutSeconds ?? 20,
    maxConcurrency: monitor?.maxConcurrency ?? 3,
    failureCooldownSeconds: monitor?.failureCooldownSeconds ?? 300,
    consecutiveFailureThreshold: monitor?.consecutiveFailureThreshold ?? 3,
  } satisfies CreateChannelMonitorInput & { fallbackModelsText: string };
}
```

If `SelectControl` prop names differ, inspect `src/components/ui/SelectControl.tsx` and align the call sites exactly.

- [ ] **Step 3: Replace monitoring tab stub**

Implement `ChannelMonitoringTab` with load/save/run flows:

```tsx
import { useEffect, useMemo, useState } from "react";
import { Copy, Play, Plus, RefreshCw, Settings2, Trash2 } from "lucide-react";
import { Button, DataTableLite, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import { createChannelMonitor, deleteChannelMonitor, listChannelMonitorTemplates, listChannelMonitors, runChannelMonitorNow, updateChannelMonitor } from "@/lib/api/channelMonitors";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import type { ChannelMonitor, CreateChannelMonitorInput } from "@/lib/types/channelMonitors";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { ChannelMonitorForm } from "./ChannelMonitorForm";
import { formatMonitorInterval, formatMonitorTime, monitorStatusLabel, monitorStatusTone, monitorTargetLabel, templateLabel } from "./channelMonitorViewModel";

type ChannelMonitoringTabProps = {
  onHealthChanged: () => void;
};

export function ChannelMonitoringTab({ onHealthChanged }: ChannelMonitoringTabProps) {
  const toast = useToast();
  const [monitors, setMonitors] = useState<ChannelMonitor[]>([]);
  const [stations, setStations] = useState<Station[]>([]);
  const [keys, setKeys] = useState<KeyPoolItem[]>([]);
  const [templates, setTemplates] = useState([]);
  const [editing, setEditing] = useState<ChannelMonitor | null>(null);
  const [formOpen, setFormOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const [runningId, setRunningId] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh() {
    const [nextMonitors, nextStations, nextKeys, nextTemplates] = await Promise.all([
      listChannelMonitors(),
      listStations(),
      listKeyPoolItems(),
      listChannelMonitorTemplates(),
    ]);
    setMonitors(nextMonitors);
    setStations(nextStations);
    setKeys(nextKeys);
    setTemplates(nextTemplates);
  }

  async function saveMonitor(input: CreateChannelMonitorInput, id: string | null) {
    setSaving(true);
    try {
      if (id) {
        await updateChannelMonitor({ ...input, id });
        toast.success("监控已保存");
      } else {
        await createChannelMonitor(input);
        toast.success("监控已创建");
      }
      setFormOpen(false);
      setEditing(null);
      await refresh();
    } catch (error) {
      toast.error("保存监控失败", readError(error));
    } finally {
      setSaving(false);
    }
  }

  async function runNow(monitor: ChannelMonitor) {
    setRunningId(monitor.id);
    try {
      await runChannelMonitorNow(monitor.id);
      toast.success("检测已完成");
      await refresh();
      onHealthChanged();
    } catch (error) {
      toast.error("立即检测失败", readError(error));
    } finally {
      setRunningId(null);
    }
  }

  const enabledCount = monitors.filter((monitor) => monitor.enabled).length;
  const failedCount = monitors.filter((monitor) => monitor.lastStatus === "failed").length;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap gap-2 text-sm text-slate-600">
          <SummaryPill label="启用" value={enabledCount} />
          <SummaryPill label="失败" value={failedCount} />
          <SummaryPill label="模板" value={templates.length} />
        </div>
        <div className="flex items-center gap-2">
          <Button variant="secondary" onClick={() => void refresh()}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
          <Button variant="secondary">
            <Settings2 className="h-4 w-4" />
            模板管理
          </Button>
          <Button variant="primary" onClick={() => { setEditing(null); setFormOpen(true); }}>
            <Plus className="h-4 w-4" />
            新建监控
          </Button>
        </div>
      </div>

      <DataTableLite
        rows={monitors}
        columns={[
          { key: "name", header: "名称", render: (monitor) => monitor.name },
          { key: "target", header: "目标", render: (monitor) => monitorTargetLabel(monitor, stations, keys) },
          { key: "template", header: "模板", render: (monitor) => templateLabel(monitor.templateId, templates) },
          { key: "model", header: "主模型", render: (monitor) => monitor.primaryModel },
          { key: "interval", header: "频率", render: formatMonitorInterval },
          {
            key: "status",
            header: "最近结果",
            render: (monitor) => (
              <StatusBadge tone={monitorStatusTone[monitor.lastStatus]}>
                {monitorStatusLabel[monitor.lastStatus]}
              </StatusBadge>
            ),
          },
          { key: "latency", header: "延迟", render: (monitor) => monitor.lastLatencyMs === null ? "--" : `${monitor.lastLatencyMs}ms` },
          { key: "lastRun", header: "最后检测", render: (monitor) => formatMonitorTime(monitor.lastRunAt) },
          {
            key: "enabled",
            header: "启用",
            render: (monitor) => <SwitchControl checked={monitor.enabled} onCheckedChange={(enabled) => void saveMonitor({ ...monitor, enabled }, monitor.id)} />,
          },
          {
            key: "actions",
            header: "操作",
            render: (monitor) => (
              <div className="flex justify-end gap-1">
                <Button size="sm" variant="secondary" disabled={runningId === monitor.id} onClick={() => void runNow(monitor)}>
                  <Play className="h-3.5 w-3.5" />
                  立即检测
                </Button>
                <Button size="sm" variant="ghost" onClick={() => { setEditing(monitor); setFormOpen(true); }}>编辑</Button>
                <Button size="sm" variant="ghost" onClick={() => void duplicateMonitor(monitor)}><Copy className="h-3.5 w-3.5" /></Button>
                <Button size="sm" variant="danger" onClick={() => void removeMonitor(monitor)}><Trash2 className="h-3.5 w-3.5" /></Button>
              </div>
            ),
          },
        ]}
        emptyTitle="暂无监控任务"
        emptyDescription="创建监控后，可以按频率检测单个 Key 或中转站下所有启用 Key。"
      />

      <ChannelMonitorForm
        open={formOpen}
        monitor={editing}
        stations={stations}
        keys={keys}
        templates={templates}
        saving={saving}
        onClose={() => { setFormOpen(false); setEditing(null); }}
        onSubmit={(input, id) => void saveMonitor(input, id)}
      />
    </div>
  );

  async function duplicateMonitor(monitor: ChannelMonitor) {
    const { id: _id, lastRunAt: _lastRunAt, nextRunAt: _nextRunAt, lastStatus: _lastStatus, lastLatencyMs: _lastLatencyMs, lastErrorSummary: _lastErrorSummary, createdAt: _createdAt, updatedAt: _updatedAt, ...input } = monitor;
    await createChannelMonitor({ ...input, name: `${monitor.name} 副本` });
    await refresh();
  }

  async function removeMonitor(monitor: ChannelMonitor) {
    await deleteChannelMonitor(monitor.id);
    await refresh();
  }
}

function SummaryPill({ label, value }: { label: string; value: number }) {
  return <span className="rounded-[8px] border border-border bg-white px-2.5 py-1">{label} {value}</span>;
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
```

Adjust `DataTableLite` prop names if its local API differs.

- [ ] **Step 4: Run frontend build**

Run:

```powershell
pnpm.cmd build
```

Expected: build passes.

- [ ] **Step 5: Commit Task 6**

```powershell
git add -- src/features/channels/ChannelMonitoringTab.tsx src/features/channels/ChannelMonitorForm.tsx src/features/channels/channelMonitorViewModel.ts
git commit -m "feat: add channel monitoring ui"
```

## Task 7: Template Manager UI

**Files:**
- Create: `src/features/channels/ChannelMonitorTemplateManager.tsx`
- Modify: `src/features/channels/ChannelMonitoringTab.tsx`

- [ ] **Step 1: Create template manager dialog**

Create `src/features/channels/ChannelMonitorTemplateManager.tsx`:

```tsx
import { useState } from "react";
import { Copy, Plus, Trash2 } from "lucide-react";
import { Button, Dialog, SegmentedControl, StatusBadge, useToast } from "@/components/ui";
import { createChannelMonitorTemplate, deleteChannelMonitorTemplate, duplicateChannelMonitorTemplate, updateChannelMonitorTemplate } from "@/lib/api/channelMonitors";
import type { ChannelMonitorRequestTemplate, CreateChannelMonitorTemplateInput } from "@/lib/types/channelMonitors";

type ChannelMonitorTemplateManagerProps = {
  open: boolean;
  templates: ChannelMonitorRequestTemplate[];
  onClose: () => void;
  onChanged: () => void;
};

type ProviderTab = "openai" | "anthropic" | "gemini" | "custom";

export function ChannelMonitorTemplateManager({ open, templates, onClose, onChanged }: ChannelMonitorTemplateManagerProps) {
  const toast = useToast();
  const [provider, setProvider] = useState<ProviderTab>("openai");
  const [editing, setEditing] = useState<ChannelMonitorRequestTemplate | null>(null);
  const visible = templates.filter((template) => template.provider === provider);

  async function duplicate(id: string) {
    await duplicateChannelMonitorTemplate(id);
    toast.success("模板副本已创建");
    onChanged();
  }

  async function remove(template: ChannelMonitorRequestTemplate) {
    await deleteChannelMonitorTemplate(template.id);
    toast.success("模板已删除");
    onChanged();
  }

  return (
    <Dialog open={open} title="请求模板管理" onClose={onClose}>
      <div className="space-y-3">
        <div className="flex items-center justify-between gap-2">
          <SegmentedControl
            value={provider}
            ariaLabel="模板供应商"
            options={[
              { value: "openai", label: `OpenAI ${countFor("openai")}` },
              { value: "anthropic", label: `Anthropic ${countFor("anthropic")}` },
              { value: "gemini", label: `Gemini ${countFor("gemini")}` },
              { value: "custom", label: `Custom ${countFor("custom")}` },
            ]}
            onChange={setProvider}
          />
          <Button variant="primary" onClick={() => setEditing(emptyTemplate(provider))}>
            <Plus className="h-4 w-4" />
            新建模板
          </Button>
        </div>

        <div className="space-y-2">
          {visible.map((template) => (
            <div key={template.id} className="rounded-[8px] border border-border bg-white px-3 py-2">
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <div className="truncate text-sm font-semibold text-slate-900">{template.name}</div>
                    {template.builtin && <StatusBadge tone="info">内置</StatusBadge>}
                    {template.defaultForProtocol && <StatusBadge tone="healthy">默认</StatusBadge>}
                    <StatusBadge tone="disabled">{template.protocol}</StatusBadge>
                  </div>
                  <div className="mt-1 truncate text-xs text-slate-500">
                    {template.method} {template.path} / 已关联 {template.linkedMonitorCount} 个监控
                  </div>
                </div>
                <div className="flex shrink-0 gap-1">
                  <Button size="sm" variant="secondary" onClick={() => void duplicate(template.id)}><Copy className="h-3.5 w-3.5" />复制</Button>
                  {!template.builtin && <Button size="sm" variant="ghost" onClick={() => setEditing(template)}>编辑</Button>}
                  {!template.builtin && <Button size="sm" variant="danger" onClick={() => void remove(template)}><Trash2 className="h-3.5 w-3.5" />删除</Button>}
                </div>
              </div>
            </div>
          ))}
        </div>

        {editing && (
          <TemplateEditor
            template={editing}
            onCancel={() => setEditing(null)}
            onSaved={() => {
              setEditing(null);
              onChanged();
            }}
          />
        )}
      </div>
    </Dialog>
  );

  function countFor(nextProvider: ProviderTab) {
    return templates.filter((template) => template.provider === nextProvider).length;
  }
}

function TemplateEditor({ template, onCancel, onSaved }: { template: ChannelMonitorRequestTemplate; onCancel: () => void; onSaved: () => void }) {
  const toast = useToast();
  const [draft, setDraft] = useState(template);
  const jsonError = validateTemplateJson(draft);

  async function save() {
    const input: CreateChannelMonitorTemplateInput = {
      name: draft.name,
      provider: draft.provider,
      protocol: draft.protocol,
      method: draft.method,
      path: draft.path,
      headersJson: draft.headersJson,
      bodyTemplateJson: draft.bodyTemplateJson,
      modelFieldPath: draft.modelFieldPath,
      streamFieldPath: draft.streamFieldPath,
      maxTokensFieldPath: draft.maxTokensFieldPath,
      defaultMaxTokens: draft.defaultMaxTokens,
      defaultStream: draft.defaultStream,
      successRuleJson: draft.successRuleJson,
      errorExtractRuleJson: draft.errorExtractRuleJson,
      description: draft.description,
      enabled: draft.enabled,
      defaultForProtocol: draft.defaultForProtocol,
    };
    if (draft.id.startsWith("new-template-")) {
      await createChannelMonitorTemplate(input);
    } else {
      await updateChannelMonitorTemplate({ ...input, id: draft.id });
    }
    toast.success("模板已保存");
    onSaved();
  }

  return (
    <div className="rounded-[8px] border border-border bg-slate-50 p-3">
      <div className="grid gap-3 md:grid-cols-2">
        <TextField label="名称" value={draft.name} onChange={(name) => setDraft({ ...draft, name })} />
        <TextField label="协议" value={draft.protocol} onChange={(protocol) => setDraft({ ...draft, protocol })} />
        <TextField label="Method" value={draft.method} onChange={(method) => setDraft({ ...draft, method: method.toUpperCase() })} />
        <TextField label="Path" value={draft.path} onChange={(path) => setDraft({ ...draft, path })} />
      </div>
      <JsonField label="Headers JSON" value={draft.headersJson} onChange={(headersJson) => setDraft({ ...draft, headersJson })} />
      <JsonField label="Body Template JSON" value={draft.bodyTemplateJson} onChange={(bodyTemplateJson) => setDraft({ ...draft, bodyTemplateJson })} />
      <JsonField label="Success Rule JSON" value={draft.successRuleJson} onChange={(successRuleJson) => setDraft({ ...draft, successRuleJson })} />
      <JsonField label="Error Extract Rule JSON" value={draft.errorExtractRuleJson} onChange={(errorExtractRuleJson) => setDraft({ ...draft, errorExtractRuleJson })} />
      {jsonError && <div className="mt-2 rounded-[8px] border border-amber-100 bg-amber-50 px-3 py-2 text-sm text-amber-700">{jsonError}</div>}
      <div className="mt-3 flex justify-end gap-2">
        <Button variant="secondary" onClick={onCancel}>取消</Button>
        <Button variant="primary" disabled={Boolean(jsonError)} onClick={() => void save()}>保存模板</Button>
      </div>
    </div>
  );
}

function TextField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <label className="block text-sm font-medium text-slate-700">
      {label}
      <input className="mt-1 h-9 w-full rounded-[8px] border border-border bg-white px-3 text-sm" value={value} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

function JsonField({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <label className="mt-3 block text-sm font-medium text-slate-700">
      {label}
      <textarea className="mt-1 min-h-24 w-full rounded-[8px] border border-border bg-white px-3 py-2 font-mono text-xs" value={value} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

function validateTemplateJson(template: ChannelMonitorRequestTemplate) {
  for (const [label, value] of [
    ["Headers JSON", template.headersJson],
    ["Body Template JSON", template.bodyTemplateJson],
    ["Success Rule JSON", template.successRuleJson],
    ["Error Extract Rule JSON", template.errorExtractRuleJson],
  ] as const) {
    try {
      JSON.parse(value);
    } catch (error) {
      return `${label} 不是有效 JSON：${error instanceof Error ? error.message : String(error)}`;
    }
  }
  return null;
}

function emptyTemplate(provider: ProviderTab): ChannelMonitorRequestTemplate {
  const now = new Date().toISOString();
  return {
    id: `new-template-${Date.now()}`,
    name: "自定义检测模板",
    provider,
    protocol: "custom_http",
    method: "POST",
    path: "/v1/chat/completions",
    headersJson: "{\"content-type\":\"application/json\"}",
    bodyTemplateJson: "{\"model\":\"{{model}}\",\"messages\":[{\"role\":\"user\",\"content\":\"{{challenge}}\"}],\"max_tokens\":{{max_tokens}},\"stream\":{{stream}}}",
    modelFieldPath: "model",
    streamFieldPath: "stream",
    maxTokensFieldPath: "max_tokens",
    defaultMaxTokens: 1,
    defaultStream: false,
    successRuleJson: "{\"okStatusMax\":399,\"requireJson\":true}",
    errorExtractRuleJson: "{\"messagePaths\":[\"error.message\",\"message\"]}",
    description: null,
    builtin: false,
    enabled: true,
    defaultForProtocol: false,
    version: 1,
    linkedMonitorCount: 0,
    createdAt: now,
    updatedAt: now,
  };
}
```

- [ ] **Step 2: Wire template manager into monitoring tab**

In `ChannelMonitoringTab.tsx`, add state:

```tsx
const [templateManagerOpen, setTemplateManagerOpen] = useState(false);
```

Replace template button:

```tsx
<Button variant="secondary" onClick={() => setTemplateManagerOpen(true)}>
  <Settings2 className="h-4 w-4" />
  模板管理
</Button>
```

Render manager near `ChannelMonitorForm`:

```tsx
<ChannelMonitorTemplateManager
  open={templateManagerOpen}
  templates={templates}
  onClose={() => setTemplateManagerOpen(false)}
  onChanged={() => void refresh()}
/>
```

Import it:

```tsx
import { ChannelMonitorTemplateManager } from "./ChannelMonitorTemplateManager";
```

- [ ] **Step 3: Run frontend build**

Run:

```powershell
pnpm.cmd build
```

Expected: build passes.

- [ ] **Step 4: Commit Task 7**

```powershell
git add -- src/features/channels/ChannelMonitorTemplateManager.tsx src/features/channels/ChannelMonitoringTab.tsx
git commit -m "feat: manage channel monitor templates"
```

## Task 8: Verification, Visual Smoke, And Closeout

**Files:**
- Modify only files required by fixes discovered during verification.

- [ ] **Step 1: Run Rust checks**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib channel_monitor -- --nocapture
```

Expected:

- `cargo check` passes.
- monitor-related Rust tests pass.

- [ ] **Step 2: Run frontend build**

Run:

```powershell
pnpm.cmd build
```

Expected: TypeScript and Vite build pass.

- [ ] **Step 3: Start the app for smoke**

Run:

```powershell
pnpm.cmd tauri:dev
```

Expected:

- frontend dev server starts on a local Vite port;
- Tauri app opens;
- no startup panic from database schema or monitor runner.

If `pnpm.cmd tauri:dev` is blocked by existing dirty runtime state or port conflict, capture the exact terminal error and use `pnpm.cmd build` plus Rust tests as partial evidence.

- [ ] **Step 4: Browser/app smoke**

Open `渠道状态` and verify:

- the page shows `状态` and `监控` tabs;
- `状态` tab still shows existing channel status cards or the existing empty state;
- `监控` tab loads without console/runtime errors;
- template manager opens and lists built-in OpenAI templates;
- creating a monitor validates interval, jitter, target, template, and model fields;
- `立即检测` updates the monitor row and refreshes status health data.

- [ ] **Step 5: Check git status**

Run:

```powershell
git status --short
git diff --cached --name-only
```

Expected:

- no staged files unless preparing the final commit;
- only files touched by this implementation are modified;
- unrelated pre-existing workspace changes are not reverted.

- [ ] **Step 6: Commit final verification fixes**

If verification required fixes:

```powershell
git add -- <exact files changed by verification fixes>
git commit -m "fix: stabilize channel monitoring"
```

If no verification fixes were needed, do not create an empty commit.

## Final Expected Verification Commands

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib channel_monitor -- --nocapture
pnpm.cmd build
```

## Self-Review

- Spec coverage: Tasks 1-3 cover storage, templates, scheduled/manual execution, history, and health writeback. Tasks 4-7 cover commands, frontend APIs, status/monitoring UI, monitor forms, and template manager. Task 8 covers verification.
- Scope check: notifications, cloud sync, router replacement, and chart-heavy analytics are excluded.
- Type consistency: Rust DTO names match TypeScript DTO names after serde camelCase conversion. Monitor target values are `station_key` and `station` in both layers.
- Red-flag scan: this plan contains concrete file paths, function names, validation ranges, command names, commit commands, and test expectations.
