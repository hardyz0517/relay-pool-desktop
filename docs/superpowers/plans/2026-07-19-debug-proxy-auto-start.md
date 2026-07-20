# Debug Proxy Auto-Start Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Start the existing authenticated loopback proxy automatically in debug builds when `RELAY_POOL_DEV_AUTO_START_PROXY=1`, without depending on the desktop UI.

**Architecture:** Extract the persisted-settings startup sequence into one shared proxy startup function used by both the Tauri command and the development hook. Keep environment parsing and fire-and-report scheduling in a debug-only module, then invoke it after Tauri state registration.

**Tech Stack:** Rust, Tauri 2 managed state, Tokio/Tauri async runtime, rusqlite-backed `AppDatabase`, existing v2 `ProxyRuntimeState`.

---

### Task 1: Shared Persisted-Settings Startup Path

**Files:**
- Create: `src-tauri/src/services/proxy/startup.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs:443`
- Test: `src-tauri/src/services/proxy/startup.rs`

- [ ] **Step 1: Write a failing integration test**

Add a test that creates an in-memory database, replaces its configured proxy port with a free loopback port, calls `start_from_persisted_settings`, and asserts the runtime listens on that persisted port.

```rust
#[tokio::test]
async fn persisted_settings_start_uses_configured_proxy_port() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let data_key = crate::services::secrets::crypto::generate_data_key();
    let port = next_free_port().await;
    update_proxy_port(&database, port);
    let runtime = ProxyRuntimeState::for_tests(ProxyRuntimeMode::V2);

    let status = start_from_persisted_settings(&database, data_key, &runtime)
        .await
        .expect("start proxy");

    assert!(status.running);
    assert_eq!(status.port, port);
    runtime.stop(port).await.expect("stop proxy");
}

async fn next_free_port() -> u16 {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind free port");
    listener.local_addr().expect("local address").port()
}

fn update_proxy_port(database: &AppDatabase, port: u16) {
    let settings = database.get_settings().expect("settings");
    database
        .update_settings(UpdateSettingsInput {
            local_proxy_port: port,
            default_routing_strategy: settings.default_routing_strategy,
            collector_proxy_mode: settings.collector_proxy_mode,
            collector_proxy_url: settings.collector_proxy_url,
            max_rate_multiplier: Some(settings.max_rate_multiplier),
            default_routing_group_filter: Some(settings.default_routing_group_filter),
            scheduler_advanced_settings: Some(settings.scheduler_advanced_settings),
            low_balance_threshold_cny: settings.low_balance_threshold_cny,
            collector_interval_minutes: settings.collector_interval_minutes,
            balance_interval_minutes: settings.balance_interval_minutes,
            group_rate_interval_minutes: settings.group_rate_interval_minutes,
            model_list_interval_minutes: settings.model_list_interval_minutes,
            pricing_refresh_interval_minutes: settings.pricing_refresh_interval_minutes,
            collector_timeout_seconds: settings.collector_timeout_seconds,
            collector_max_concurrency: settings.collector_max_concurrency,
            allow_depleted_fallback: settings.allow_depleted_fallback,
            developer_mode_enabled: settings.developer_mode_enabled,
        })
        .expect("update proxy port");
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::startup::tests::persisted_settings_start_uses_configured_proxy_port -- --exact --nocapture
```

Expected: compilation fails because `start_from_persisted_settings` and the module do not exist.

- [ ] **Step 3: Implement the shared startup function**

```rust
pub async fn start_from_persisted_settings(
    database: &AppDatabase,
    data_key: [u8; 32],
    proxy: &ProxyRuntimeState,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    database.migrate_plaintext_secrets(&data_key)?;
    proxy
        .start(ProxyStartConfig::new(
            database.clone(),
            data_key,
            settings.local_proxy_port,
        ))
        .await
}
```

Export `pub mod startup;` from `services/proxy/mod.rs`, and replace the duplicated body in `commands::start_local_proxy` with this function.

- [ ] **Step 4: Run the focused test and command/runtime regressions**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::startup -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::runtime -- --nocapture
```

Expected: startup test passes; all runtime tests pass.

### Task 2: Debug-Only Environment Gate

**Files:**
- Create: `src-tauri/src/services/proxy/dev_auto_start.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Test: `src-tauri/src/services/proxy/dev_auto_start.rs`

- [ ] **Step 1: Write parser tests before implementation**

```rust
#[test]
fn auto_start_requires_exact_normalized_one() {
    assert!(enabled(Some("1")));
    assert!(enabled(Some(" 1 ")));
    assert!(!enabled(None));
    assert!(!enabled(Some("")));
    assert!(!enabled(Some("true")));
    assert!(!enabled(Some("01")));
}
```

- [ ] **Step 2: Run the parser test and verify RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::dev_auto_start::tests::auto_start_requires_exact_normalized_one -- --exact --nocapture
```

Expected: compilation fails because the debug-only module and `enabled` do not exist.

- [ ] **Step 3: Implement the debug-only scheduler**

Expose the module only under debug assertions:

```rust
#[cfg(debug_assertions)]
pub mod dev_auto_start;
```

Implement exact activation and use managed application state inside the spawned future:

```rust
const ENV_NAME: &str = "RELAY_POOL_DEV_AUTO_START_PROXY";

fn enabled(value: Option<&str>) -> bool {
    value.map(str::trim) == Some("1")
}

pub fn schedule(app: AppHandle) {
    if !enabled(std::env::var(ENV_NAME).ok().as_deref()) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        if let Err(error) = start_managed(&app).await {
            eprintln!("Relay Pool debug proxy auto-start failed: {error}");
        }
    });
}
```

`start_managed` obtains `AppDatabase`, `SecretManager`, and `ProxyRuntimeState` through `Manager::try_state/state`, then calls `startup::start_from_persisted_settings`. If the database is unavailable in recovery mode, return the fixed redacted message `data store is unavailable`.

- [ ] **Step 4: Run the debug gate tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::dev_auto_start -- --nocapture
```

Expected: all debug gate tests pass.

### Task 3: Tauri Setup Wiring

**Files:**
- Modify: `src-tauri/src/lib.rs:217`
- Test: `src-tauri/src/services/proxy/dev_auto_start.rs`

- [ ] **Step 1: Register runtime state before scheduling**

Keep the existing managed states and add the debug-only call after `ProxyRuntimeState` is managed:

```rust
app.manage(services::proxy::runtime::ProxyRuntimeState::default());
#[cfg(debug_assertions)]
services::proxy::dev_auto_start::schedule(app.handle().clone());
```

- [ ] **Step 2: Run formatting, compile, and focused regressions**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::dev_auto_start -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::runtime -- --nocapture
git diff --check
```

Expected: all commands exit 0; only existing dead-code warnings remain.

### Task 4: Live Auto-Start Verification

**Files:**
- No source changes expected.

- [ ] **Step 1: Ensure prior task-owned dev processes are stopped**

Verify that no `relay-pool-desktop.exe` process and no listener on `1430` or `8787` remain. Stop only processes created by this task.

- [ ] **Step 2: Launch with the debug environment gate**

Run:

```powershell
$env:RELAY_POOL_DEV_AUTO_START_PROXY='1'
$env:CARGO_TARGET_DIR='D:\Dev\Projects\relay-pool-desktop\output\local-routing-v2-target'
pnpm tauri:dev
```

Expected without UI interaction:

- `relay-pool-desktop.exe` is running;
- `127.0.0.1:1430` is listening;
- `127.0.0.1:8787` is listening from the same application PID.

- [ ] **Step 3: Send a real authenticated Responses stream**

Read the local bearer key from the real SQLite database without printing it. Send `stream=true`, a function tool, and `reasoning.effort=high` to `/v1/responses` using a direct loopback client.

Expected: either `response.completed` is received and the log records `success`, or an upstream truncation produces a stream error and the log records `upstream_stream_failed/body_incomplete`. A truncated stream must never be recorded as success.

- [ ] **Step 4: Inspect final scope**

Run:

```powershell
git status --short
git diff --stat
git diff --check
```

Expected: only the approved debug auto-start files plus the already in-scope local-routing fixes are modified. Do not stage or commit implementation unless the user requests it.
