# Relay Pool Shared Capabilities Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move repeated Relay Pool user operations into shared backend capabilities and thin TS APIs, then migrate pages away from page-local business workflows.

**Architecture:** Rust services and Tauri commands own complete business operations. TypeScript API modules expose those operations with browser-preview fallback. React pages submit intent and render normalized data instead of coordinating multi-step persistence or per-monitor run loading.

**Tech Stack:** Tauri 2, Rust, rusqlite, React, TypeScript, Vite, pnpm, existing `scripts/*.test.mjs` smoke tests.

**Worktree:** Execute in `D:\Dev\Projects\relay-pool-desktop\.worktrees\shared-capabilities-refactor` on branch `codex/shared-capabilities-refactor`.

**Baseline Evidence:** `pnpm.cmd install --frozen-lockfile` completed; `pnpm.cmd build` passed; `cargo check --manifest-path .\src-tauri\Cargo.toml` passed with existing dead-code warnings.

---

## Source Spec

Implement the accepted design in:

- `docs/superpowers/specs/2026-07-06-relay-pool-shared-capabilities-design.md`

Do not change the product scope from that spec. Do not redesign UI layout. Keep old lower-level commands available until all named consumers have migrated.

## File Structure

Create:

- `src-tauri/src/models/shared_capabilities.rs` - Rust input/output DTOs for shared capabilities.
- `src-tauri/src/services/shared_capabilities.rs` - Rust workflow orchestration for key save, group options, and channel monitor summaries.
- `src/features/stations/groupOptionViewModels.ts` - frontend-only display helpers for normalized station group options.
- `src/lib/errors.ts` - shared `readError`.
- `src/lib/formatters.ts` - shared numeric/rate formatting.
- `scripts/shared-capabilities-contract.test.mjs` - source-level contract/negative-proof tests.

Modify:

- `src-tauri/src/models/mod.rs` - export `shared_capabilities`.
- `src-tauri/src/services/mod.rs` - export `shared_capabilities`.
- `src-tauri/src/commands/mod.rs` - add thin Tauri commands.
- `src-tauri/src/lib.rs` - register new commands.
- `src-tauri/src/services/database.rs` - add transactional helpers used by shared capability workflows.
- `src/lib/types/stationKeys.ts` - add save DTO/result TS types.
- `src/lib/types/groupFacts.ts` - add `StationGroupOption`.
- `src/lib/types/channelMonitors.ts` - add `ChannelMonitorSummary`.
- `src/lib/api/stationKeys.ts` - add `saveStationKeyWithDefaults`.
- `src/lib/api/groupFacts.ts` - add `listStationGroupOptions`.
- `src/lib/api/channelMonitors.ts` - add `listChannelMonitorSummaries`.
- `src/features/key-pool/AddKeyPage.tsx` - use shared key save and group options.
- `src/features/key-pool/EditKeyPage.tsx` - use shared key save and group options.
- `src/features/key-pool/KeyPoolPage.tsx` - use shared key save and group options where dialog fallback remains.
- `src/features/stations/components/CreateRemoteKeyDialog.tsx` - use stable shared group option values.
- `src/features/stations/components/StationKeyRowsEditor.tsx` - use shared group option helpers.
- `src/features/channels/ChannelMonitoringTab.tsx` - use monitor summaries.
- `src/features/channels/ChannelStatusTab.tsx` - use monitor summaries.

Verification commands:

- `node scripts/shared-capabilities-contract.test.mjs`
- `node scripts/edit-key-page-flow.test.mjs`
- `node scripts/add-provider-key-groups.test.mjs`
- `node scripts/channel-status-view-model.test.mjs`
- `node scripts/key-pool-monitor-toggle.test.mjs`
- `pnpm.cmd build`
- `cargo test --manifest-path .\src-tauri\Cargo.toml shared_capabilities`
- `cargo check --manifest-path .\src-tauri\Cargo.toml`

## Task 1: Add Contract Tests Before Production Changes

**Files:**

- Create: `scripts/shared-capabilities-contract.test.mjs`

- [ ] **Step 1: Write the source-level contract test**

Create `scripts/shared-capabilities-contract.test.mjs` with this content:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const files = {
  stationKeysApi: await readFile("src/lib/api/stationKeys.ts", "utf8"),
  groupFactsApi: await readFile("src/lib/api/groupFacts.ts", "utf8"),
  channelApi: await readFile("src/lib/api/channelMonitors.ts", "utf8"),
  addKey: await readFile("src/features/key-pool/AddKeyPage.tsx", "utf8"),
  editKey: await readFile("src/features/key-pool/EditKeyPage.tsx", "utf8"),
  keyPool: await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8"),
  createRemoteKeyDialog: await readFile("src/features/stations/components/CreateRemoteKeyDialog.tsx", "utf8"),
  stationKeyRowsEditor: await readFile("src/features/stations/components/StationKeyRowsEditor.tsx", "utf8"),
  channelMonitoring: await readFile("src/features/channels/ChannelMonitoringTab.tsx", "utf8"),
  channelStatus: await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8"),
  rustCommands: await readFile("src-tauri/src/commands/mod.rs", "utf8"),
  rustLib: await readFile("src-tauri/src/lib.rs", "utf8"),
};

assert.ok(
  files.stationKeysApi.includes("saveStationKeyWithDefaults"),
  "stationKeys API should expose saveStationKeyWithDefaults",
);
assert.ok(
  files.groupFactsApi.includes("listStationGroupOptions"),
  "groupFacts API should expose listStationGroupOptions",
);
assert.ok(
  files.channelApi.includes("listChannelMonitorSummaries"),
  "channelMonitors API should expose listChannelMonitorSummaries",
);
assert.ok(
  files.rustCommands.includes("save_station_key_with_defaults") &&
    files.rustCommands.includes("list_station_group_options") &&
    files.rustCommands.includes("list_channel_monitor_summaries"),
  "Tauri commands should expose the three shared capabilities",
);
assert.ok(
  files.rustLib.includes("commands::save_station_key_with_defaults") &&
    files.rustLib.includes("commands::list_station_group_options") &&
    files.rustLib.includes("commands::list_channel_monitor_summaries"),
  "Tauri invoke handler should register the shared capability commands",
);

for (const [name, source] of [
  ["AddKeyPage", files.addKey],
  ["EditKeyPage", files.editKey],
]) {
  assert.ok(
    source.includes("saveStationKeyWithDefaults"),
    `${name} should save keys through saveStationKeyWithDefaults`,
  );
  assert.ok(
    !source.includes("updateStationKeyCapabilities"),
    `${name} should not persist default capabilities directly`,
  );
  assert.ok(
    !source.includes("updateStationKeyGroupBinding"),
    `${name} should not compose key save and group binding update in page code`,
  );
}

assert.ok(
  files.keyPool.includes("saveStationKeyWithDefaults"),
  "KeyPoolPage dialog fallback should use saveStationKeyWithDefaults",
);
assert.ok(
  !files.keyPool.includes("updateStationKeyCapabilities"),
  "KeyPoolPage should not persist default capabilities directly",
);

assert.ok(
  !files.createRemoteKeyDialog.includes("groupOptionValue(index)") &&
    !files.createRemoteKeyDialog.includes("Number(groupValue.replace"),
  "CreateRemoteKeyDialog should not use index-based group option values",
);
assert.ok(
  !files.stationKeyRowsEditor.includes("function normalizeGroupOptions") &&
    !files.stationKeyRowsEditor.includes("function groupOptionValue"),
  "StationKeyRowsEditor should use shared group option helpers",
);

assert.ok(
  files.channelMonitoring.includes("listChannelMonitorSummaries"),
  "ChannelMonitoringTab should load monitor summaries from shared API",
);
assert.ok(
  files.channelStatus.includes("listChannelMonitorSummaries"),
  "ChannelStatusTab should load monitor summaries from shared API",
);
assert.ok(
  !files.channelMonitoring.includes("listChannelMonitorRuns(monitor.id)") &&
    !files.channelStatus.includes("listChannelMonitorRuns(monitor.id)"),
  "channel tabs should not issue page-local per-monitor run loading",
);
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
node scripts/shared-capabilities-contract.test.mjs
```

Expected: fail at `stationKeys API should expose saveStationKeyWithDefaults`.

- [ ] **Step 3: Commit the failing contract test**

```powershell
git add -- scripts/shared-capabilities-contract.test.mjs
git commit -m "test: add shared capability contract checks"
```

## Task 2: Add Rust DTOs and Backend Shared Capability Service

**Files:**

- Create: `src-tauri/src/models/shared_capabilities.rs`
- Create: `src-tauri/src/services/shared_capabilities.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add shared capability DTOs**

Create `src-tauri/src/models/shared_capabilities.rs`:

```rust
use serde::{Deserialize, Serialize};

use super::{
    channel_monitors::{ChannelMonitor, ChannelMonitorRun},
    routing::{StationKeyCapabilities, UpdateStationKeyCapabilitiesInput},
    station_keys::{StationKey, StationKeyStatus},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaveStationKeyMode {
    Create,
    Update,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StationKeyGroupSelectionKind {
    Keep,
    Clear,
    Set,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyGroupSelection {
    pub kind: StationKeyGroupSelectionKind,
    #[serde(default)]
    pub group_binding_id: Option<String>,
    #[serde(default)]
    pub group_id_hash: Option<String>,
    #[serde(default)]
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStationKeyWithDefaultsInput {
    pub mode: SaveStationKeyMode,
    #[serde(default)]
    pub id: Option<String>,
    pub station_id: String,
    pub name: String,
    #[serde(default)]
    pub api_key: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub tier_label: Option<String>,
    #[serde(default)]
    pub balance_scope: Option<String>,
    #[serde(default)]
    pub status: Option<StationKeyStatus>,
    #[serde(default)]
    pub note: Option<String>,
    pub group_selection: StationKeyGroupSelection,
    #[serde(default)]
    pub capabilities: Option<UpdateStationKeyCapabilitiesInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStationKeyWithDefaultsResult {
    pub station_key: StationKey,
    pub capabilities: StationKeyCapabilities,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationGroupOption {
    pub value: String,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub rate_multiplier: Option<f64>,
    pub rate_source: Option<String>,
    pub selectable_for_remote_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMonitorRunsLoadStatus {
    Ok,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorSummary {
    pub monitor: ChannelMonitor,
    pub recent_runs: Vec<ChannelMonitorRun>,
    pub runs_load_status: ChannelMonitorRunsLoadStatus,
    pub latest_run: Option<ChannelMonitorRun>,
}
```

`StationKeyStatus` is already exported from `src-tauri/src/models/station_keys.rs`; keep `status: Option<StationKeyStatus>` in this DTO.

- [ ] **Step 2: Export model and service modules**

Modify `src-tauri/src/models/mod.rs`:

```rust
pub mod shared_capabilities;
```

Modify `src-tauri/src/services/mod.rs`:

```rust
pub mod shared_capabilities;
```

- [ ] **Step 3: Add database helpers**

In `src-tauri/src/services/database.rs`, add public methods on `impl AppDatabase` near existing station key methods:

```rust
pub fn save_station_key_with_defaults(
    &self,
    data_key: &[u8; 32],
    input: crate::models::shared_capabilities::SaveStationKeyWithDefaultsInput,
) -> Result<crate::models::shared_capabilities::SaveStationKeyWithDefaultsResult, String> {
    let connection = self.connection()?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("开始保存 Station Key 事务失败: {error}"))?;
    let result = crate::services::shared_capabilities::save_station_key_with_defaults_in_connection(
        &transaction,
        data_key,
        input,
    )?;
    transaction
        .commit()
        .map_err(|error| format!("提交保存 Station Key 事务失败: {error}"))?;
    Ok(result)
}

pub fn list_station_group_options(
    &self,
    station_id: String,
) -> Result<Vec<crate::models::shared_capabilities::StationGroupOption>, String> {
    let bindings = self.list_station_group_bindings(station_id.clone())?;
    let rates = self.list_group_rate_records(station_id)?;
    Ok(crate::services::shared_capabilities::station_group_options_from_facts(
        bindings,
        rates,
    ))
}

pub fn list_channel_monitor_summaries(
    &self,
) -> Result<Vec<crate::models::shared_capabilities::ChannelMonitorSummary>, String> {
    let monitors = self.list_channel_monitors()?;
    Ok(crate::services::shared_capabilities::channel_monitor_summaries_from_database(
        self,
        monitors,
    ))
}
```

If `connection()` returns a non-mutable guard that cannot start a transaction, adjust the helper to follow the existing transaction pattern in this file. Keep the transaction inside `database.rs`; do not expose raw connections through commands.

- [ ] **Step 4: Add backend workflow service**

Create `src-tauri/src/services/shared_capabilities.rs`:

```rust
use rusqlite::Connection;

use crate::{
    models::{
        channel_monitors::{ChannelMonitor, ChannelMonitorRun},
        group_facts::{GroupRateRecord, StationGroupBinding},
        routing::UpdateStationKeyCapabilitiesInput,
        shared_capabilities::{
            ChannelMonitorRunsLoadStatus, ChannelMonitorSummary, SaveStationKeyMode,
            SaveStationKeyWithDefaultsInput, SaveStationKeyWithDefaultsResult,
            StationGroupOption, StationKeyGroupSelectionKind,
        },
        station_keys::{CreateStationKeyInput, StationKey, UpdateStationKeyInput},
    },
    services::database::AppDatabase,
};

pub fn default_station_key_capabilities_input(station_key_id: String) -> UpdateStationKeyCapabilitiesInput {
    UpdateStationKeyCapabilitiesInput {
        station_key_id,
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: true,
        supports_stream: true,
        supports_tools: true,
        supports_vision: true,
        supports_reasoning: true,
        model_allowlist: Vec::new(),
        model_blocklist: Vec::new(),
        preferred_models: Vec::new(),
        only_use_as_backup: false,
        routing_tags: Vec::new(),
    }
}

pub fn save_station_key_with_defaults_in_connection(
    connection: &Connection,
    data_key: &[u8; 32],
    input: SaveStationKeyWithDefaultsInput,
) -> Result<SaveStationKeyWithDefaultsResult, String> {
    validate_group_selection(&input)?;
    let key = match input.mode {
        SaveStationKeyMode::Create => create_key(connection, data_key, &input)?,
        SaveStationKeyMode::Update => update_key(connection, data_key, &input)?,
    };
    let key = apply_group_selection(connection, key, &input)?;
    let capability_input = input
        .capabilities
        .unwrap_or_else(|| default_station_key_capabilities_input(key.id.clone()));
    let capabilities = super::database::update_station_key_capabilities_in_connection(
        connection,
        capability_input,
    )
    .map_err(|error| format!("Station Key 已保存，但路由能力保存失败: {error}"))?;
    Ok(SaveStationKeyWithDefaultsResult {
        station_key: key,
        capabilities,
        message: match input.mode {
            SaveStationKeyMode::Create => "Station Key 已创建。".to_string(),
            SaveStationKeyMode::Update => "Station Key 已更新。".to_string(),
        },
    })
}
```

Then add these private helpers in the same file:

```rust
fn validate_group_selection(input: &SaveStationKeyWithDefaultsInput) -> Result<(), String> {
    match input.group_selection.kind {
        StationKeyGroupSelectionKind::Keep => {
            if matches!(input.mode, SaveStationKeyMode::Create) {
                return Err("新建 Station Key 不能使用 keep 分组动作。".to_string());
            }
        }
        StationKeyGroupSelectionKind::Set => {
            if input
                .group_selection
                .group_binding_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err("设置分组时必须提供 group_binding_id。".to_string());
            }
        }
        StationKeyGroupSelectionKind::Clear => {}
    }
    Ok(())
}

fn create_key(
    connection: &Connection,
    data_key: &[u8; 32],
    input: &SaveStationKeyWithDefaultsInput,
) -> Result<StationKey, String> {
    let api_key = input
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "新建 Station Key 必须提供 API Key。".to_string())?;
    super::database::create_station_key_in_connection_with_data_key(
        connection,
        CreateStationKeyInput {
            station_id: input.station_id.clone(),
            name: input.name.clone(),
            api_key: api_key.to_string(),
            enabled: input.enabled,
            priority: input.priority,
            group_name: input.group_selection.group_name.clone(),
            tier_label: input.tier_label.clone(),
            group_binding_id: None,
            group_id_hash: input.group_selection.group_id_hash.clone(),
            rate_multiplier: None,
            rate_source: None,
            balance_scope: input.balance_scope.clone(),
            note: input.note.clone(),
        },
        Some(data_key),
    )
}

fn update_key(
    connection: &Connection,
    data_key: &[u8; 32],
    input: &SaveStationKeyWithDefaultsInput,
) -> Result<StationKey, String> {
    let id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "更新 Station Key 必须提供 id。".to_string())?;
    let existing = super::database::station_key_by_id(connection, id)?;
    super::database::update_station_key_in_connection_with_data_key(
        connection,
        UpdateStationKeyInput {
            id: id.to_string(),
            station_id: input.station_id.clone(),
            name: input.name.clone(),
            api_key: input.api_key.clone(),
            enabled: input.enabled,
            priority: input.priority.unwrap_or(existing.priority),
            group_name: existing.group_name,
            tier_label: input.tier_label.clone(),
            group_binding_id: existing.group_binding_id,
            group_id_hash: existing.group_id_hash,
            rate_multiplier: existing.rate_multiplier,
            rate_source: existing.rate_source,
            balance_scope: input.balance_scope.clone().or(existing.balance_scope),
            status: input.status.clone().unwrap_or(existing.status),
            note: input.note.clone(),
        },
        Some(data_key),
    )
}
```

The helper calls above require making `create_station_key_in_connection_with_data_key`, `update_station_key_in_connection_with_data_key`, `update_station_key_capabilities_in_connection`, and `station_key_by_id` visible inside `services`. Use `pub(super)` in `database.rs`, not `pub`.

- [ ] **Step 5: Add group action helper**

Continue `src-tauri/src/services/shared_capabilities.rs`:

```rust
fn apply_group_selection(
    connection: &Connection,
    key: StationKey,
    input: &SaveStationKeyWithDefaultsInput,
) -> Result<StationKey, String> {
    match input.group_selection.kind {
        StationKeyGroupSelectionKind::Keep => Ok(key),
        StationKeyGroupSelectionKind::Clear => {
            super::database::clear_station_key_group_binding_in_connection(connection, &key.id)
        }
        StationKeyGroupSelectionKind::Set => {
            let binding_id = input
                .group_selection
                .group_binding_id
                .as_deref()
                .expect("validated group_binding_id");
            super::database::update_station_key_group_binding_in_connection(
                connection,
                crate::models::group_facts::UpdateStationKeyGroupBindingInput {
                    station_key_id: key.id,
                    group_binding_id: binding_id.to_string(),
                },
            )
        }
    }
}
```

In `database.rs`, add `pub(super) fn clear_station_key_group_binding_in_connection(connection: &Connection, station_key_id: &str) -> Result<StationKey, String>` next to `update_station_key_group_binding_in_connection`:

```rust
pub(super) fn clear_station_key_group_binding_in_connection(
    connection: &Connection,
    station_key_id: &str,
) -> Result<StationKey, String> {
    validate_station_key_exists(connection, station_key_id)?;
    let now = now_string();
    connection
        .execute(
            "UPDATE station_keys
                SET group_binding_id = NULL,
                    group_id_hash = NULL,
                    group_name = NULL,
                    rate_multiplier = NULL,
                    rate_source = NULL,
                    rate_collected_at = NULL,
                    updated_at = ?1
              WHERE id = ?2",
            params![now, station_key_id],
        )
        .map_err(|error| format!("清除 Key 分组绑定失败: {error}"))?;
    station_key_by_id(connection, station_key_id)
}
```

- [ ] **Step 6: Add group option and channel summary helpers**

Add to `src-tauri/src/services/shared_capabilities.rs`:

```rust
pub fn station_group_options_from_facts(
    bindings: Vec<StationGroupBinding>,
    rates: Vec<GroupRateRecord>,
) -> Vec<StationGroupOption> {
    let mut options = Vec::new();
    for binding in bindings {
        if binding.binding_kind != "station_group"
            || binding.binding_status == "disabled"
            || binding.binding_status == "manual_legacy"
            || binding.rate_source.as_deref() == Some("legacy_key_group")
        {
            continue;
        }
        let latest_rate = rates
            .iter()
            .filter(|rate| {
                rate.binding_kind == "station_group"
                    && (rate.group_binding_id.as_deref() == Some(binding.id.as_str())
                        || rate.group_key_hash == binding.group_key_hash)
            })
            .max_by_key(|rate| rate.checked_at.clone());
        let rate_multiplier = binding
            .effective_rate_multiplier
            .or(binding.default_rate_multiplier)
            .or_else(|| latest_rate.and_then(|rate| rate.effective_rate_multiplier))
            .or_else(|| latest_rate.and_then(|rate| rate.default_rate_multiplier));
        options.push(StationGroupOption {
            value: group_option_value(&binding),
            group_binding_id: Some(binding.id.clone()),
            group_id_hash: binding.group_id_hash.clone().or(Some(binding.group_key_hash.clone())),
            group_name: binding.group_name.clone(),
            rate_multiplier,
            rate_source: binding
                .rate_source
                .clone()
                .or_else(|| latest_rate.map(|rate| rate.source.clone())),
            selectable_for_remote_key: binding.group_id_hash.is_some(),
        });
    }
    options.sort_by(|left, right| left.group_name.cmp(&right.group_name).then(left.value.cmp(&right.value)));
    options
}

fn group_option_value(binding: &StationGroupBinding) -> String {
    format!("binding:{}", binding.id)
}

pub fn channel_monitor_summaries_from_database(
    database: &AppDatabase,
    monitors: Vec<ChannelMonitor>,
) -> Vec<ChannelMonitorSummary> {
    monitors
        .into_iter()
        .map(|monitor| match database.list_channel_monitor_runs(monitor.id.clone()) {
            Ok(mut runs) => {
                runs.sort_by(|left, right| left.started_at.cmp(&right.started_at));
                let recent_runs = runs.into_iter().rev().take(60).collect::<Vec<_>>();
                let latest_run = recent_runs.first().cloned();
                ChannelMonitorSummary {
                    monitor,
                    recent_runs,
                    runs_load_status: ChannelMonitorRunsLoadStatus::Ok,
                    latest_run,
                }
            }
            Err(_) => ChannelMonitorSummary {
                monitor,
                recent_runs: Vec::new(),
                runs_load_status: ChannelMonitorRunsLoadStatus::Failed,
                latest_run: None,
            },
        })
        .collect()
}
```

After writing this code, verify whether the `recent_runs` order should be newest-first or oldest-first for the two pages. If the UI expects oldest-first, reverse before storing and keep `latest_run` as the newest run.

- [ ] **Step 7: Add Rust tests**

At the bottom of `src-tauri/src/services/shared_capabilities.rs`, add tests for:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::group_facts::{BINDING_STATUS_AVAILABLE, StationGroupBinding};

    #[test]
    fn default_capability_flags_are_all_enabled() {
        let input = default_station_key_capabilities_input("key-1".to_string());
        assert!(input.supports_chat_completions);
        assert!(input.supports_responses);
        assert!(input.supports_embeddings);
        assert!(input.supports_stream);
        assert!(input.supports_tools);
        assert!(input.supports_vision);
        assert!(input.supports_reasoning);
        assert!(input.model_allowlist.is_empty());
        assert!(input.model_blocklist.is_empty());
        assert!(input.preferred_models.is_empty());
        assert!(!input.only_use_as_backup);
        assert!(input.routing_tags.is_empty());
    }

    #[test]
    fn station_group_options_prefer_binding_identity() {
        let binding = StationGroupBinding {
            id: "binding-1".to_string(),
            station_id: "station-1".to_string(),
            station_key_id: None,
            binding_kind: "station_group".to_string(),
            parent_group_binding_id: None,
            group_key_hash: "group-key-hash".to_string(),
            group_id_hash: Some("remote-group-id".to_string()),
            group_name: "Pro".to_string(),
            binding_status: BINDING_STATUS_AVAILABLE.to_string(),
            default_rate_multiplier: Some(1.2),
            user_rate_multiplier: None,
            effective_rate_multiplier: Some(1.2),
            rate_source: Some("groups_api".to_string()),
            confidence: 1.0,
            last_seen_at: Some("1000".to_string()),
            last_checked_at: Some("1000".to_string()),
            last_rate_changed_at: None,
            raw_json_redacted: None,
            created_at: "1000".to_string(),
            updated_at: "1000".to_string(),
        };
        let options = station_group_options_from_facts(vec![binding], vec![]);
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].value, "binding:binding-1");
        assert_eq!(options[0].group_binding_id.as_deref(), Some("binding-1"));
        assert_eq!(options[0].group_id_hash.as_deref(), Some("remote-group-id"));
        assert!(options[0].selectable_for_remote_key);
    }
}
```

- [ ] **Step 8: Run Rust tests and commit**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml shared_capabilities
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: tests pass; `cargo check` may print existing dead-code warnings.

Commit:

```powershell
git add -- src-tauri/src/models/shared_capabilities.rs src-tauri/src/models/mod.rs src-tauri/src/services/shared_capabilities.rs src-tauri/src/services/mod.rs src-tauri/src/services/database.rs
git commit -m "feat: add shared capability backend services"
```

## Task 3: Add Tauri Commands and TypeScript API Wrappers

**Files:**

- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/lib/types/groupFacts.ts`
- Modify: `src/lib/types/channelMonitors.ts`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/lib/api/groupFacts.ts`
- Modify: `src/lib/api/channelMonitors.ts`

- [ ] **Step 1: Add commands**

In `src-tauri/src/commands/mod.rs`, import shared DTOs and add:

```rust
#[tauri::command]
pub fn save_station_key_with_defaults(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: SaveStationKeyWithDefaultsInput,
) -> Result<SaveStationKeyWithDefaultsResult, String> {
    database.save_station_key_with_defaults(secrets.data_key(), input)
}

#[tauri::command]
pub fn list_station_group_options(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<StationGroupOption>, String> {
    database.list_station_group_options(station_id)
}

#[tauri::command]
pub fn list_channel_monitor_summaries(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ChannelMonitorSummary>, String> {
    database.list_channel_monitor_summaries()
}
```

Add these types to the existing `use crate::models::{ ... }` block:

```rust
shared_capabilities::{
    ChannelMonitorSummary, SaveStationKeyWithDefaultsInput,
    SaveStationKeyWithDefaultsResult, StationGroupOption,
},
```

- [ ] **Step 2: Register commands**

In `src-tauri/src/lib.rs`, add to `tauri::generate_handler![...]`:

```rust
commands::save_station_key_with_defaults,
commands::list_station_group_options,
commands::list_channel_monitor_summaries,
```

Place `save_station_key_with_defaults` near the station key commands, `list_station_group_options` near group facts, and `list_channel_monitor_summaries` near channel monitor commands.

- [ ] **Step 3: Add TS types**

In `src/lib/types/stationKeys.ts`, add:

```ts
export type SaveStationKeyMode = "create" | "update";
export type StationKeyGroupSelection =
  | { kind: "keep" }
  | { kind: "clear" }
  | {
      kind: "set";
      groupBindingId: string;
      groupIdHash?: string | null;
      groupName?: string | null;
    };

export type SaveStationKeyWithDefaultsInput = {
  mode: SaveStationKeyMode;
  id?: string | null;
  stationId: string;
  name: string;
  apiKey?: string | null;
  enabled: boolean;
  priority?: number | null;
  tierLabel?: string | null;
  balanceScope?: string | null;
  status?: StationKeyStatus | null;
  note?: string | null;
  groupSelection: StationKeyGroupSelection;
};

export type SaveStationKeyWithDefaultsResult = {
  stationKey: StationKey;
  capabilities: import("@/lib/types/routing").StationKeyCapabilities;
  message: string;
};
```

In `src/lib/types/groupFacts.ts`, add:

```ts
export type StationGroupOption = {
  value: string;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupName: string;
  rateMultiplier: number | null;
  rateSource: string | null;
  selectableForRemoteKey: boolean;
};
```

In `src/lib/types/channelMonitors.ts`, add:

```ts
export type ChannelMonitorRunsLoadStatus = "ok" | "failed";

export type ChannelMonitorSummary = {
  monitor: ChannelMonitor;
  recentRuns: ChannelMonitorRun[];
  runsLoadStatus: ChannelMonitorRunsLoadStatus;
  latestRun: ChannelMonitorRun | null;
};
```

- [ ] **Step 4: Add TS API wrappers with memory fallback**

In `src/lib/api/stationKeys.ts`, import the new types and add:

```ts
export function saveStationKeyWithDefaults(
  input: SaveStationKeyWithDefaultsInput,
): Promise<SaveStationKeyWithDefaultsResult> {
  return invoke<SaveStationKeyWithDefaultsResult>("save_station_key_with_defaults", { input }).catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      return saveStationKeyWithDefaultsInMemory(input);
    }
    throw error;
  });
}
```

Add a memory fallback that calls `createStationKey` or `updateStationKey`, then returns `capabilities` with all support flags true:

```ts
async function saveStationKeyWithDefaultsInMemory(
  input: SaveStationKeyWithDefaultsInput,
): Promise<SaveStationKeyWithDefaultsResult> {
  const key = input.mode === "create"
    ? await createStationKey({
        stationId: input.stationId,
        name: input.name,
        apiKey: input.apiKey ?? "",
        enabled: input.enabled,
        priority: input.priority,
        groupBindingId: input.groupSelection.kind === "set" ? input.groupSelection.groupBindingId : null,
        groupIdHash: input.groupSelection.kind === "set" ? input.groupSelection.groupIdHash ?? null : null,
        groupName: input.groupSelection.kind === "set" ? input.groupSelection.groupName ?? null : null,
        tierLabel: input.tierLabel ?? null,
        balanceScope: input.balanceScope ?? null,
        note: input.note ?? null,
      })
    : await updateStationKey({
        id: input.id ?? "",
        stationId: input.stationId,
        name: input.name,
        apiKey: input.apiKey?.trim() ? input.apiKey : null,
        enabled: input.enabled,
        priority: input.priority ?? 0,
        groupBindingId: input.groupSelection.kind === "clear" ? null : undefined,
        groupIdHash: input.groupSelection.kind === "clear" ? null : undefined,
        groupName: input.groupSelection.kind === "set" ? input.groupSelection.groupName ?? null : null,
        tierLabel: input.tierLabel ?? null,
        balanceScope: input.balanceScope ?? null,
        status: input.status ?? "unchecked",
        note: input.note ?? null,
      });
  return {
    stationKey: key,
    capabilities: {
      stationKeyId: key.id,
      supportsChatCompletions: true,
      supportsResponses: true,
      supportsEmbeddings: true,
      supportsStream: true,
      supportsTools: true,
      supportsVision: true,
      supportsReasoning: true,
      modelAllowlist: [],
      modelBlocklist: [],
      preferredModels: [],
      onlyUseAsBackup: false,
      routingTags: [],
      updatedAt: new Date().toISOString(),
    },
    message: input.mode === "create" ? "浏览器预览模式：密钥已创建。" : "浏览器预览模式：密钥已更新。",
  };
}
```

In `src/lib/api/groupFacts.ts`, add:

```ts
export function listStationGroupOptions(stationId: string) {
  return invoke<StationGroupOption[]>("list_station_group_options", { stationId }).catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      const bindings = await listStationGroupBindings(stationId);
      return bindings.filter(isCollectedStationGroupBinding).map((binding) => ({
        value: `binding:${binding.id}`,
        groupBindingId: binding.id,
        groupIdHash: binding.groupIdHash ?? binding.groupKeyHash,
        groupName: binding.groupName,
        rateMultiplier: binding.effectiveRateMultiplier ?? binding.defaultRateMultiplier,
        rateSource: binding.rateSource,
        selectableForRemoteKey: Boolean(binding.groupIdHash),
      }));
    }
    throw error;
  });
}
```

In `src/lib/api/channelMonitors.ts`, add:

```ts
export function listChannelMonitorSummaries() {
  return invoke<ChannelMonitorSummary[]>("list_channel_monitor_summaries").catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      const monitors = await listChannelMonitors();
      return monitors.map((monitor) => {
        const recentRuns = (memoryRuns.get(monitor.id) ?? []).map(copyRun);
        return {
          monitor: copyMonitor(monitor),
          recentRuns,
          runsLoadStatus: "ok" as const,
          latestRun: recentRuns[0] ?? null,
        };
      });
    }
    throw error;
  });
}
```

Import `StationGroupOption`, `isCollectedStationGroupBinding`, `SaveStationKeyWithDefaultsInput`, `SaveStationKeyWithDefaultsResult`, and `ChannelMonitorSummary` as needed.

- [ ] **Step 5: Run contract test and build**

Run:

```powershell
node scripts/shared-capabilities-contract.test.mjs
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: contract test still fails on page migrations; build/check pass.

- [ ] **Step 6: Commit API surfaces**

```powershell
git add -- src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/types/stationKeys.ts src/lib/types/groupFacts.ts src/lib/types/channelMonitors.ts src/lib/api/stationKeys.ts src/lib/api/groupFacts.ts src/lib/api/channelMonitors.ts
git commit -m "feat: expose shared capability APIs"
```

## Task 4: Migrate Key Save Pages

**Files:**

- Modify: `src/features/key-pool/AddKeyPage.tsx`
- Modify: `src/features/key-pool/EditKeyPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`

- [ ] **Step 1: Migrate AddKeyPage**

Change imports:

```ts
import { listStationGroupOptions } from "@/lib/api/groupFacts";
import { saveStationKeyWithDefaults } from "@/lib/api/stationKeys";
import type { StationGroupOption } from "@/lib/types/groupFacts";
```

Remove imports for `updateStationKeyCapabilities`, `createStationKey`, `updateStationKeyGroupBinding`, `listStationGroupBindings`, and `isCollectedStationGroupBinding`.

Change state:

```ts
const [groupOptions, setGroupOptions] = useState<StationGroupOption[]>([]);
```

Replace `refreshBindings` with:

```ts
async function refreshGroupOptions(stationId: string, alive = true) {
  try {
    const nextOptions = await listStationGroupOptions(stationId);
    if (alive) setGroupOptions(nextOptions);
  } catch (requestError) {
    if (alive) toast.error("读取中转站分组失败", readError(requestError));
  }
}
```

Replace submit body with:

```ts
await saveStationKeyWithDefaults({
  mode: "create",
  stationId: form.stationId,
  name: form.name.trim(),
  apiKey: form.apiKey.trim(),
  enabled: true,
  priority: Number(form.priority),
  tierLabel: form.tierLabel.trim() ? form.tierLabel.trim() : null,
  note: form.note.trim() ? form.note.trim() : null,
  groupSelection: form.groupBindingId
    ? {
        kind: "set",
        groupBindingId: form.groupBindingId,
        groupName: form.groupName.trim() ? form.groupName.trim() : null,
      }
    : { kind: "clear" },
});
```

- [ ] **Step 2: Migrate EditKeyPage**

Use `listStationGroupOptions` and `saveStationKeyWithDefaults`. Remove direct capability/group-binding imports.

Group selection helper:

```ts
function groupSelectionFromEditForm(form: EditKeyFormState, sourceItem: KeyPoolItem) {
  if (!form.groupBindingId) {
    return sourceItem.groupBindingId ? { kind: "clear" as const } : { kind: "keep" as const };
  }
  if (form.groupBindingId === sourceItem.groupBindingId) {
    return { kind: "keep" as const };
  }
  return {
    kind: "set" as const,
    groupBindingId: form.groupBindingId,
    groupName: form.groupName.trim() ? form.groupName.trim() : null,
  };
}
```

Replace save with:

```ts
await saveStationKeyWithDefaults({
  mode: "update",
  id: form.id,
  stationId: form.stationId,
  name: form.name.trim(),
  apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
  enabled: form.enabled,
  priority: Number(form.priority),
  tierLabel: form.tierLabel.trim() ? form.tierLabel.trim() : null,
  balanceScope: sourceItem.balanceScope,
  status: form.status,
  note: form.note.trim() ? form.note.trim() : null,
  groupSelection: groupSelectionFromEditForm(form, sourceItem),
  capabilities: {
    stationKeyId: form.id,
    supportsChatCompletions: true,
    supportsResponses: true,
    supportsEmbeddings: true,
    supportsStream: true,
    supportsTools: true,
    supportsVision: true,
    supportsReasoning: true,
    modelAllowlist: linesToList(form.modelAllowlist),
    modelBlocklist: linesToList(form.modelBlocklist),
    preferredModels: linesToList(form.preferredModels),
    onlyUseAsBackup: form.onlyUseAsBackup,
    routingTags: commaListToList(form.routingTags),
  },
});
```

- [ ] **Step 3: Migrate KeyPoolPage legacy dialog fallback**

Keep enable/disable using `updateStationKey`; that is not the save-with-defaults workflow. Migrate only create/edit dialog submit handlers that currently compose create/update + group binding + capability.

Replace create dialog submit with `saveStationKeyWithDefaults({ mode: "create", ... })`.

Replace edit dialog submit with `saveStationKeyWithDefaults({ mode: "update", ... })`.

Keep monitor toggle logic unchanged.

- [ ] **Step 4: Verify migration**

Run:

```powershell
node scripts/shared-capabilities-contract.test.mjs
node scripts/edit-key-page-flow.test.mjs
pnpm.cmd build
```

Expected: contract test may still fail on group option and channel summary tasks; key save assertions pass.

- [ ] **Step 5: Commit key page migration**

```powershell
git add -- src/features/key-pool/AddKeyPage.tsx src/features/key-pool/EditKeyPage.tsx src/features/key-pool/KeyPoolPage.tsx
git commit -m "refactor: route key saves through shared capability"
```

## Task 5: Migrate Group Option Consumers

**Files:**

- Create: `src/features/stations/groupOptionViewModels.ts`
- Modify: `src/features/stations/components/CreateRemoteKeyDialog.tsx`
- Modify: `src/features/stations/components/StationKeyRowsEditor.tsx`
- Modify: `src/features/stations/AddProviderPage.tsx`

- [ ] **Step 1: Add frontend group option helpers**

Create `src/features/stations/groupOptionViewModels.ts`:

```ts
import type { StationGroupOption } from "@/lib/types/groupFacts";

export const noGroupOptionValue = "__none__";

export function stationGroupSelectValue(option: Pick<StationGroupOption, "value" | "groupBindingId" | "groupIdHash" | "groupName">) {
  if (option.value) return option.value;
  if (option.groupBindingId) return `binding:${option.groupBindingId}`;
  if (option.groupIdHash) return `remote:${option.groupIdHash}`;
  return `name:${option.groupName.trim()}`;
}

export function formatMultiplier(value: number | null | undefined, fallback = "未采集倍率") {
  if (value === null || value === undefined) return fallback;
  return Number.isInteger(value) ? String(value) : Number(value.toFixed(6)).toString();
}

export function findMatchingGroupOption(
  row: { groupBindingId: string | null; groupIdHash: string | null; groupName: string },
  options: StationGroupOption[],
) {
  return options.find((option) =>
    Boolean(
      (row.groupBindingId && option.groupBindingId === row.groupBindingId) ||
        (row.groupIdHash && option.groupIdHash === row.groupIdHash) ||
        (row.groupName.trim() && option.groupName.trim() === row.groupName.trim()),
    ),
  ) ?? null;
}

export function normalizeStationGroupOptions(options: StationGroupOption[]) {
  const seen = new Set<string>();
  return options.filter((option) => {
    const value = stationGroupSelectValue(option);
    if (seen.has(value)) return false;
    seen.add(value);
    return true;
  });
}
```

- [ ] **Step 2: Migrate CreateRemoteKeyDialog**

Replace local `RemoteKeyGroupOption` type with `StationGroupOption`.

Replace index-based values with `stationGroupSelectValue(group)`.

Replace `groupIndex` parsing with:

```ts
const selectedGroup = normalizedGroups.find((group) => stationGroupSelectValue(group) === groupValue) ?? null;
```

Remove local `groupOptionValue` and `formatMultiplier`; import `formatMultiplier` and `stationGroupSelectValue`.

- [ ] **Step 3: Migrate StationKeyRowsEditor**

Replace exported `StationKeyGroupOption` with `StationGroupOption` import. If external consumers still import `StationKeyGroupOption`, re-export:

```ts
export type StationKeyGroupOption = StationGroupOption;
```

Use `normalizeStationGroupOptions`, `findMatchingGroupOption`, `stationGroupSelectValue`, `noGroupOptionValue`, and `formatMultiplier`.

Remove local `normalizeGroupOptions`, `groupOptionValue`, and `formatMultiplier`.

- [ ] **Step 4: Update AddProviderPage group option adapter**

Find the object literals in `AddProviderPage` that are passed to `StationKeyRowsEditor` or `CreateRemoteKeyDialog` as station group options. Convert those objects to `StationGroupOption` by adding the shared stable value and selector fields:

```ts
value: group.groupBindingId ? `binding:${group.groupBindingId}` : group.groupIdHash ? `remote:${group.groupIdHash}` : `name:${group.groupName.trim()}`,
rateSource: null,
selectableForRemoteKey: Boolean(group.groupBindingId || group.groupIdHash),
```

Keep `AddProviderPage` deeper group-row refactor out of this task unless TypeScript requires it.

- [ ] **Step 5: Verify and commit**

Run:

```powershell
node scripts/shared-capabilities-contract.test.mjs
node scripts/add-provider-key-groups.test.mjs
pnpm.cmd build
```

Expected: group option assertions pass; contract test may still fail on channel summaries.

Commit:

```powershell
git add -- src/features/stations/groupOptionViewModels.ts src/features/stations/components/CreateRemoteKeyDialog.tsx src/features/stations/components/StationKeyRowsEditor.tsx src/features/stations/AddProviderPage.tsx
git commit -m "refactor: centralize station group option handling"
```

## Task 6: Migrate Channel Monitor Summary Consumers

**Files:**

- Modify: `src/features/channels/ChannelMonitoringTab.tsx`
- Modify: `src/features/channels/ChannelStatusTab.tsx`

- [ ] **Step 1: Migrate ChannelMonitoringTab refresh**

Change imports:

```ts
import {
  createChannelMonitor,
  deleteChannelMonitor,
  listChannelMonitorSummaries,
  listChannelMonitorTemplates,
  runChannelMonitorNow,
  updateChannelMonitor,
} from "@/lib/api/channelMonitors";
```

Remove `listChannelMonitorRuns` and `listChannelMonitors`.

Replace refresh monitor/runs loading:

```ts
const [summaries, nextStations, nextKeys, nextTemplates] = await Promise.all([
  listChannelMonitorSummaries(),
  listStations(),
  listKeyPoolItems(),
  listChannelMonitorTemplates(),
]);
const nextMonitors = summaries.map((summary) => summary.monitor);
setMonitors(nextMonitors);
setStations(nextStations);
setKeys(nextKeys);
setTemplates(nextTemplates);
setRunsByMonitor(new Map(summaries.map((summary) => [summary.monitor.id, summary.recentRuns] as const)));
setRunLoadFailedIds(new Set(summaries.filter((summary) => summary.runsLoadStatus === "failed").map((summary) => summary.monitor.id)));
```

- [ ] **Step 2: Migrate ChannelStatusTab refresh**

Change import:

```ts
import { listChannelMonitorSummaries } from "@/lib/api/channelMonitors";
```

Remove `listChannelMonitorRuns` and `listChannelMonitors`.

Replace monitor/runs loading:

```ts
const [nextKeys, nextLogs, nextHealth, summaries] = await Promise.all([
  listKeyPoolItems(),
  listRequestLogs(),
  listStationKeyHealth(),
  listChannelMonitorSummaries(),
]);
const nextMonitors = summaries.map((summary) => summary.monitor);
setKeys(nextKeys);
setLogs(nextLogs);
setHealth(nextHealth);
setMonitors(nextMonitors);
setRunsByMonitor(new Map(summaries.map((summary) => [summary.monitor.id, summary.recentRuns] as const)));
```

- [ ] **Step 3: Verify and commit**

Run:

```powershell
node scripts/shared-capabilities-contract.test.mjs
node scripts/channel-status-view-model.test.mjs
node scripts/key-pool-monitor-toggle.test.mjs
pnpm.cmd build
```

Expected: contract test passes for channel summary assertions.

Commit:

```powershell
git add -- src/features/channels/ChannelMonitoringTab.tsx src/features/channels/ChannelStatusTab.tsx
git commit -m "refactor: load channel monitor summaries through shared API"
```

## Task 7: Utility Cleanup

**Files:**

- Create: `src/lib/errors.ts`
- Create: `src/lib/formatters.ts`
- Modify: pages touched by prior tasks when they still define identical helpers.

- [ ] **Step 1: Add shared utilities**

Create `src/lib/errors.ts`:

```ts
export function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
```

Create `src/lib/formatters.ts`:

```ts
export function formatCompactMultiplier(value: number | null | undefined, fallback = "未采集") {
  if (value === null || value === undefined) return fallback;
  return Number.isInteger(value) ? String(value) : Number(value.toFixed(6)).toString();
}

export function formatRate(value: number | null | undefined, fallback = "未知") {
  if (value === null || value === undefined) return fallback;
  return Number.isInteger(value) ? String(value) : Number(value.toFixed(6)).toString();
}
```

- [ ] **Step 2: Replace helpers in touched pages**

In files already modified by this plan, replace local `readError` imports with:

```ts
import { readError } from "@/lib/errors";
```

Remove local `function readError(error: unknown)`.

Do not sweep every page in the repo in this task. Keep this cleanup to files already touched by shared capability migration unless build fails.

- [ ] **Step 3: Verify and commit**

Run:

```powershell
node scripts/shared-capabilities-contract.test.mjs
pnpm.cmd build
```

Commit:

```powershell
git add -- src/lib/errors.ts src/lib/formatters.ts src/features/key-pool/AddKeyPage.tsx src/features/key-pool/EditKeyPage.tsx src/features/key-pool/KeyPoolPage.tsx src/features/channels/ChannelMonitoringTab.tsx src/features/channels/ChannelStatusTab.tsx src/features/stations/groupOptionViewModels.ts
git commit -m "refactor: share common error and formatter helpers"
```

## Task 8: Final Verification and Handoff

**Files:**

- Modify only if previous tasks reveal a narrow compile/test issue.

- [ ] **Step 1: Run full planned verification**

Run:

```powershell
node scripts/shared-capabilities-contract.test.mjs
node scripts/edit-key-page-flow.test.mjs
node scripts/add-provider-key-groups.test.mjs
node scripts/channel-status-view-model.test.mjs
node scripts/key-pool-monitor-toggle.test.mjs
pnpm.cmd build
cargo test --manifest-path .\src-tauri\Cargo.toml shared_capabilities
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- All Node scripts pass.
- `pnpm.cmd build` passes.
- Rust tests pass.
- `cargo check` passes. Existing dead-code warnings may remain; do not broaden scope to remove them.

- [ ] **Step 2: Inspect duplicate-removal evidence**

Run:

```powershell
rg -n "updateStationKeyCapabilities|updateStationKeyGroupBinding|listChannelMonitorRuns\\(monitor\\.id\\)|groupOptionValue\\(index\\)|function normalizeGroupOptions|function readError" src/features src/lib
```

Expected:

- No `updateStationKeyCapabilities` in `AddKeyPage.tsx`, `EditKeyPage.tsx`, or `KeyPoolPage.tsx`.
- No `updateStationKeyGroupBinding` in `AddKeyPage.tsx`, `EditKeyPage.tsx`, or `KeyPoolPage.tsx`.
- No `listChannelMonitorRuns(monitor.id)` in channel tabs.
- No index-based group option value in `CreateRemoteKeyDialog.tsx`.
- Remaining `function readError` hits outside touched files can be left for a separate cleanup pass if not part of this plan.

- [ ] **Step 3: Check git status**

Run:

```powershell
git status --short
git log --oneline -8
```

Expected:

- Only intentional files are modified before final commit.
- The branch is `codex/shared-capabilities-refactor`.

- [ ] **Step 4: Final commit if needed**

If verification required small fixes:

```powershell
git add -- <exact fixed paths>
git commit -m "fix: verify shared capability refactor"
```

Use exact paths only. Do not use `git add .`, `git add -A`, or `git commit -a`.

## Completion Report Template

When implementation finishes, report:

- Branch: `codex/shared-capabilities-refactor`
- Worktree: `D:\Dev\Projects\relay-pool-desktop\.worktrees\shared-capabilities-refactor`
- Commits created, newest first
- Verification commands and pass/fail output summary
- Remaining warnings or known non-blocking issues
- Whether the main worktree was left untouched
