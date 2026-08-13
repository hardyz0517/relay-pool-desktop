# P4.2 Login State Collector Real Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `信息采集` actually log in with the saved station username/password, then collect balance, groups, and rate multipliers from the authenticated station APIs and persist a real collector snapshot.

**Architecture:** The collector flow stays centered on `Station` as the account asset, but the execution path must now pass through the stored secret layer before any request is sent. `collect_station_info` becomes the real login-state collector entry, `test_station_login` becomes a true login probe, and the Sub2API/NewAPI adapter layer is responsible for authenticated endpoint probing plus normalization into stable snapshot fields.

**Tech Stack:** Tauri commands, Rust collector services, SQLite-backed station credentials, `SecretManager`, `ureq`, React/Tauri front-end, `collector_snapshot` / normalized JSON, Rust integration tests.

---

### Task 1: Trace and lock the real collector contract

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
- Modify: `src-tauri/src/models/collector.rs`
- Modify: `src/lib/api/collector.ts`
- Modify: `src/lib/types/collector.ts`
- Modify: `src/features/collectors/CollectorsPage.tsx`

- [ ] **Step 1: Add a failing test that proves `collect_station_info` must require the secret layer**

```rust
#[test]
fn collect_station_info_requires_secret_access() {
    // Build a station with saved login credentials and assert the collector
    // can only proceed when the decrypted password is available.
}
```

- [ ] **Step 2: Run the test and confirm current behavior is still incomplete**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml collect_station_info_requires_secret_access --lib -v`

Expected: the test fails or clearly shows the current collector is not yet using the secret layer for the real login path.

- [ ] **Step 3: Route `collect_station_info` through `SecretManager` and the decrypted password**

```rust
#[tauri::command]
pub async fn collect_station_info(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        collectors::collect_station_info(&database, &data_key, station_id)
    })
    .await
    .map_err(|error| format!("采集任务执行失败: {error}"))?
}
```

- [ ] **Step 4: Keep the snapshot shape stable**

```rust
#[serde(rename_all = "camelCase")]
pub struct CollectorSnapshot {
    pub id: String,
    pub station_id: String,
    pub source: String,
    pub status: String,
    pub fetched_at: String,
    pub summary_json: Value,
    pub normalized_json: Value,
    pub raw_json_redacted: Option<Value>,
    pub error_message: Option<String>,
    pub created_at: String,
}
```

- [ ] **Step 5: Verify the collector entry still compiles**

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml`

Expected: command handler signatures compile and the front-end invoke bridge still matches the Rust command.

---

### Task 2: Make login testing a real login probe

**Files:**
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src/lib/api/collector.ts`
- Modify: `src/lib/types/collector.ts`
- Modify: `src/features/collectors/CollectorsPage.tsx`

- [ ] **Step 1: Add a failing test for `test_station_login` using a saved password**

```rust
#[test]
fn test_station_login_uses_saved_password() {
    // Assert the login probe receives the decrypted password and can reach the
    // adapter path that produces a real login attempt result.
}
```

- [ ] **Step 2: Run the test and verify the existing placeholder path**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml test_station_login_uses_saved_password --lib -v`

Expected: the test reveals the current flow only checks credential presence or still uses a fake login value.

- [ ] **Step 3: Thread the decrypted password into `attempt_login`**

```rust
pub fn collect_login_state(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let credentials = database.get_station_credentials(station_id.clone())?;
    let Some(username) = credentials.login_username.clone() else {
        return Ok(login_state_manual_required(...));
    };
    let Some(password) = database.get_station_login_password_with_data_key(station_id.clone(), data_key)? else {
        return Ok(login_state_manual_required(...));
    };
    let login_attempt = attempt_login(&agent, &station.base_url, &username, &password)?;
    ...
}
```

- [ ] **Step 4: Replace the fake password placeholder**

```rust
fn attempt_login(
    agent: &Agent,
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginAttempt, String> {
    for path in LOGIN_PATHS {
        for (field, value) in username_variants {
            let payload = json!({
                field: value,
                "password": password,
            });
            ...
        }
    }
}
```

- [ ] **Step 5: Verify the login probe now produces a token or a concrete error**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml login --lib -v`

Expected: the result distinguishes missing credentials, invalid credentials, captcha/2FA/manual-required, and successful login.

---

### Task 3: Normalize authenticated balance/group/rate payloads

**Files:**
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
- Add or modify tests in: `src-tauri/src/services/collectors/sub2api.rs`
- Modify: `src/lib/types/collector.ts`
- Modify: `src/lib/api/collector.ts`
- Modify: `src/features/collectors/CollectorsPage.tsx`

- [ ] **Step 1: Add a failing test for authenticated normalization**

```rust
#[test]
fn authenticated_probe_normalizes_balance_groups_and_rates() {
    // Feed a synthetic authenticated response that contains balance, groups,
    // and rate multipliers. Assert the normalized snapshot keeps all three.
}
```

- [ ] **Step 2: Run it and confirm current normalization is still generic**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml authenticated_probe_normalizes_balance_groups_and_rates --lib -v`

Expected: the current generic keyword scan misses some structured fields or collapses them into weak output.

- [ ] **Step 3: Introduce explicit adapter parsing for the known station patterns**

```rust
fn normalize_probe(raw: &Value) -> Value {
    let mut groups = Vec::new();
    let mut rate_multipliers = Vec::new();
    let mut keys = Vec::new();
    let mut balance = Value::Null;

    // Keep the generic matcher as a fallback, but explicitly parse common
    // auth payload shapes before the fallback loop.
}
```

- [ ] **Step 4: Shape the normalized snapshot for the UI**

```rust
json!({
    "balance": {
        "value": balance_value,
        "currency": balance_currency,
        "source": source_label,
    },
    "groups": groups,
    "rateMultipliers": rate_multipliers,
    "keys": keys,
    "matchedFields": matches,
})
```

- [ ] **Step 5: Verify collector pages show the new normalized values**

Run: `pnpm build`

Expected: the front-end still compiles and can read the richer `summaryJson` / `normalizedJson` shape.

---

### Task 4: Remove remaining developer-era placeholder language from the active UI

**Files:**
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/stations/components/StationDetailPanel.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/lib/mock/settings.ts`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`

- [ ] **Step 1: Replace any current UI copy that says “等待 / 占位 / 后续阶段” when a real action exists**

```tsx
description="站点账号资产已保存；Key 池和信息采集页会使用这份配置。"
```

```tsx
description="只展示脱敏值；真实本地访问密钥由 SecretManager 管理。"
```

- [ ] **Step 2: Remove disabled buttons that promise future work the UI cannot do yet**

```tsx
<Button variant="outline" onClick={onEdit}>
  <Edit3 className="h-4 w-4" />
  编辑
</Button>
```

- [ ] **Step 3: Replace tray / collector-frequency placeholder rows with truthful state or remove them**

```tsx
<SettingRow
  control={<StatusBadge tone="healthy">已启用</StatusBadge>}
  description="Station Key、站点 API Key 和登录密码通过 SecretManager 写入本地加密存储。"
  label="加密存储"
/>
```

- [ ] **Step 4: Run a literal placeholder scan on the active UI surfaces**

Run: `rg -n "等待|占位|后续阶段|未接入|P4|P3 阶段|测试连接|刷新余额|刷新倍率" src\\features src\\components src\\lib src-tauri\\src`

Expected: only historical docs, mock fixtures, or truly intentional disabled states remain.

- [ ] **Step 5: Verify the UI still builds**

Run: `pnpm build`

Expected: build passes after the copy cleanup.

---

### Task 5: Prove the collector end-to-end with real data

**Files:**
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/secrets/*` if required by the password flow

- [ ] **Step 1: Add a minimal integration test around the collector response**

```rust
#[test]
fn collect_login_state_produces_real_group_and_balance_fields() {
    // Seed a station, credentials, and a mock adapter response.
    // Assert the snapshot contains balance/groups/rateMultipliers.
}
```

- [ ] **Step 2: Run the collector tests**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml collector --lib`

Expected: real-data collector cases pass, and the old manual-required fallbacks still behave for missing credentials.

- [ ] **Step 3: Smoke the UI with a real station**

```text
1. Save a station with username + password
2. Click 测试登录
3. Click 采集信息
4. Confirm the snapshot shows balance, groups, and rate multipliers
5. Confirm request logs / snapshot JSON remain redacted
```

- [ ] **Step 4: Verify secret boundaries still hold**

Run:
```powershell
rg -n "sk-[A-Za-z0-9]|cookie|session|token|password" src-tauri\\target src\\features src\\lib
```

Expected: only masked or mock-safe values appear; no raw secrets leak into logs or UI output.

- [ ] **Step 5: Stop and summarize the remaining non-goals**

Expected outcome:
- WebView capture remains an advanced fallback, not the mainline.
- Price routing and cost strategy remain out of scope.
- The collector is now genuinely usable for saved credentials on supported stations.

---

### Self-Review Checklist

- [ ] Every task has a concrete file list.
- [ ] Every code-changing step shows the real shape of the change.
- [ ] Every test step names the command and the expected outcome.
- [ ] No task says “TODO”, “TBD”, or “implement later”.
- [ ] The plan separates login probe, authenticated collection, normalization, and UI cleanup.
- [ ] The plan does not smuggle in P5/P6/P7 work.

