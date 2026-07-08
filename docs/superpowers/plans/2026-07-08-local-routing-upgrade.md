# Local Routing Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade Relay Pool Desktop local routing into a trustworthy low-cost, stable, explainable, drag-priority router without breaking existing proxy, simulator, model alias, monitoring, or log flows.

**Architecture:** Build on the existing Tauri proxy runtime and router. Add a redacted local-routing read model first, then introduce global `routing_order`, split routing policy into pure modules, add lifecycle-aware logging, health classification, route affinity, and bounded switch-before-confirm probes. React consumes typed API/query modules only; it never reconstructs routing decisions from raw arrays.

**Tech Stack:** Tauri 2, Rust 2021, rusqlite, React 18, TypeScript, Vite, Tailwind CSS, @dnd-kit, local Node regression scripts.

---

## Execution Rules

- Start execution in a clean feature branch or isolated worktree. If the working tree is dirty, record `git status --short` and do not touch unrelated files.
- Never use `git add .` or `git add -A`; stage exact paths only.
- Keep each task commit small enough to review independently.
- For every backend behavior task, write the failing Rust test first and run the focused Cargo test before implementation.
- For every frontend/query task, write the failing Node regression script first and run it before implementation.
- Do not push unless explicitly requested.

## Source Blueprint

Primary spec:

- `docs/superpowers/specs/2026-07-08-local-routing-redesign-design.md`

Current anchor files:

- `src-tauri/src/services/proxy/router.rs` owns current candidate selection and policy scoring.
- `src-tauri/src/services/proxy/runtime.rs` owns socket/proxy IO, request forwarding, current request logging, and health writes.
- `src-tauri/src/services/database.rs` owns schema migration, route candidates, simulation, settings, health, request logs, and key reordering.
- `src-tauri/src/models/routing.rs` owns routing types exposed through Tauri.
- `src-tauri/src/commands/mod.rs` owns Tauri commands.
- `src/lib/types/routing.ts`, `src/lib/api/routing.ts`, and `src/lib/queries/routingQueries.ts` own existing frontend routing boundaries.
- `src/features/routing/RoutingPage.tsx` owns current routing UI.
- `src/features/channels/ChannelStatusTab.tsx` and `src/features/key-pool/KeyPoolPage.tsx` contain existing @dnd-kit patterns.

## Planned File Structure

Create Rust modules:

- `src-tauri/src/services/proxy/routing_types.rs`
  - Domain structs/enums for decision facts, local workspace rows, health state, failure classification, lifecycle status, and redacted explanations.
- `src-tauri/src/services/proxy/routing_snapshot.rs`
  - Builds immutable route snapshots from database candidates, settings, aliases, proxy status, latest logs, and route affinity when Task 10 adds it.
- `src-tauri/src/services/proxy/routing_policy.rs`
  - Pure candidate filtering/ranking functions and legacy wrapper around the old selector.
- `src-tauri/src/services/proxy/routing_explanation.rs`
  - Converts decision facts to redacted route explanations and request-log metadata.
- `src-tauri/src/services/proxy/routing_failure.rs`
  - Pure failure classifier.
- `src-tauri/src/services/proxy/routing_health.rs`
  - Health state transition and cooldown logic.
- `src-tauri/src/services/proxy/routing_affinity.rs`
  - In-memory affinity store and price hysteresis checks.
- `src-tauri/src/services/proxy/routing_probe.rs`
  - Bounded low-token switch-before-confirm probe orchestration.
- `src-tauri/src/services/proxy/routing_lifecycle.rs`
  - Coordinates route selection, switch-before-confirm probe when Task 11 enables it, upstream attempts, stream boundary, final log, and health update.

Create frontend files:

- `src/lib/types/localRouting.ts`
  - Redacted local-routing workspace, row, summary, decision, and command input types.
- `src/lib/api/localRouting.ts`
  - Typed Tauri command wrappers and browser-preview fallbacks.
- `src/lib/queries/localRoutingQueries.ts`
  - `loadLocalRoutingWorkspace()` query boundary.
- `src/features/routing/LocalRoutingStatusTab.tsx`
  - Status-first read model UI.
- `src/features/routing/LocalRoutingEditTab.tsx`
  - Drag priority, auto-save, sync states, and no weight controls.
- `src/features/routing/LocalRoutingCandidateRow.tsx`
  - Compact row shared by status/edit variants.
- `src/features/routing/RouteExplanationPanel.tsx`
  - Structured explanation renderer.

Create regression scripts:

- `scripts/local-routing-query-service.test.mjs`
- `scripts/local-routing-page-layout.test.mjs`
- `scripts/local-routing-reorder.test.mjs`
- `scripts/local-routing-redaction.test.mjs`
- `scripts/local-routing-explanation.test.mjs`

Modify existing files:

- `src-tauri/src/services/proxy/mod.rs`
- `src-tauri/src/services/proxy/router.rs`
- `src-tauri/src/services/proxy/runtime.rs`
- `src-tauri/src/services/database.rs`
- `src-tauri/src/models/routing.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/lib.rs`
- `src/lib/types/routing.ts`
- `src/lib/api/routing.ts`
- `src/lib/queries/routingQueries.ts`
- `src/features/routing/RoutingPage.tsx`

---

### Task 1: Local Routing Read Model Types and Redaction Contract

**Files:**
- Create: `src/lib/types/localRouting.ts`
- Create: `scripts/local-routing-redaction.test.mjs`
- Modify: `src-tauri/src/models/routing.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Create: `src-tauri/src/services/proxy/routing_types.rs`

- [ ] **Step 1: Write the frontend redaction regression script**

Create `scripts/local-routing-redaction.test.mjs`:

```js
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const typeSource = readFileSync("src/lib/types/localRouting.ts", "utf8");

assert.match(typeSource, /export type LocalRoutingWorkspace =/);
assert.match(typeSource, /proxyStatus: ProxyStatus/);
assert.match(typeSource, /candidates: LocalRoutingCandidateRow\[\]/);
assert.match(typeSource, /latestDecision: RouteDecisionSummary \| null/);

const forbiddenNames = [
  "apiKey:",
  "api_key:",
  "authorization:",
  "cookie:",
  "setCookie:",
  "rawBody:",
  "requestBody:",
  "upstreamErrorBody:",
];

for (const forbidden of forbiddenNames) {
  assert.equal(typeSource.includes(forbidden), false, `Local routing types must not expose ${forbidden}`);
}

console.log("local routing redaction type contract ok");
```

- [ ] **Step 2: Run the regression script and confirm it fails**

Run: `node scripts/local-routing-redaction.test.mjs`

Expected: FAIL because `src/lib/types/localRouting.ts` does not exist.

- [ ] **Step 3: Add frontend local-routing types**

Create `src/lib/types/localRouting.ts`:

```ts
import type { ProxyStatus } from "@/lib/types/proxy";
import type { RouteEndpointKind } from "@/lib/types/routing";

export type LocalRoutingMode = "balanced";
export type LocalRoutingSyncState = "synced" | "saving" | "failed";
export type LocalRoutingCandidateStatus =
  | "healthy"
  | "observing"
  | "probing"
  | "cooling"
  | "rate_limited"
  | "insufficient_balance"
  | "auth_error"
  | "model_blocked"
  | "manual_disabled"
  | "unchecked";

export type LocalRoutingSettings = {
  mode: LocalRoutingMode;
  strategyLabel: string;
  switchProbeEnabled: boolean;
};

export type LocalRoutingSummary = {
  currentPrimaryKeyName: string | null;
  schedulableCount: number;
  candidateCount: number;
  todayCostLabel: string;
  recentAbnormalCount: number;
};

export type LocalRoutingCandidateRow = {
  stationKeyId: string;
  stationId: string;
  order: number;
  keyName: string;
  stationName: string;
  enabled: boolean;
  schedulable: boolean;
  selected: boolean;
  currentAffinity: boolean;
  status: LocalRoutingCandidateStatus;
  priceLabel: string;
  balanceLabel: string;
  latencyLabel: string;
  lastOutcomeLabel: string;
  reason: string;
  syncState?: LocalRoutingSyncState;
};

export type RouteDecisionSummary = {
  requestId: string;
  endpoint: RouteEndpointKind;
  clientModel: string | null;
  mappedModel: string | null;
  selectedStationKeyId: string | null;
  selectedKeyName: string | null;
  selectedReason: string;
  keptForStability: boolean;
  candidateCount: number;
  rejectedCount: number;
  startedAt: string;
  finishedAt: string | null;
};

export type RouteDecisionEvent = {
  id: string;
  requestId: string | null;
  stationKeyId: string | null;
  kind: "selected" | "rejected" | "probe" | "failover" | "cooldown" | "recovered";
  message: string;
  createdAt: string;
};

export type LocalRoutingWorkspace = {
  proxyStatus: ProxyStatus;
  settings: LocalRoutingSettings;
  summary: LocalRoutingSummary;
  candidates: LocalRoutingCandidateRow[];
  latestDecision: RouteDecisionSummary | null;
  recentEvents: RouteDecisionEvent[];
};
```

- [ ] **Step 4: Add Rust domain types**

Create `src-tauri/src/services/proxy/routing_types.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::models::{
    proxy::ProxyStatus,
    routing::RouteEndpointKind,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteHealthState {
    Healthy,
    Observing,
    Probing,
    Cooling,
    RateLimited,
    InsufficientBalance,
    AuthError,
    ModelBlocked,
    ManualDisabled,
    Unchecked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionFactKind {
    Accepted,
    Rejected,
    Penalized,
    KeptForStability,
    ConfirmedByProbe,
    TieBrokenByUserOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionFact {
    pub kind: DecisionFactKind,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingSettingsView {
    pub mode: String,
    pub strategy_label: String,
    pub switch_probe_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingSummary {
    pub current_primary_key_name: Option<String>,
    pub schedulable_count: i64,
    pub candidate_count: i64,
    pub today_cost_label: String,
    pub recent_abnormal_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingCandidateRow {
    pub station_key_id: String,
    pub station_id: String,
    pub order: i64,
    pub key_name: String,
    pub station_name: String,
    pub enabled: bool,
    pub schedulable: bool,
    pub selected: bool,
    pub current_affinity: bool,
    pub status: RouteHealthState,
    pub price_label: String,
    pub balance_label: String,
    pub latency_label: String,
    pub last_outcome_label: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDecisionSummary {
    pub request_id: String,
    pub endpoint: RouteEndpointKind,
    pub client_model: Option<String>,
    pub mapped_model: Option<String>,
    pub selected_station_key_id: Option<String>,
    pub selected_key_name: Option<String>,
    pub selected_reason: String,
    pub kept_for_stability: bool,
    pub candidate_count: i64,
    pub rejected_count: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDecisionEvent {
    pub id: String,
    pub request_id: Option<String>,
    pub station_key_id: Option<String>,
    pub kind: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingWorkspace {
    pub proxy_status: ProxyStatus,
    pub settings: LocalRoutingSettingsView,
    pub summary: LocalRoutingSummary,
    pub candidates: Vec<LocalRoutingCandidateRow>,
    pub latest_decision: Option<RouteDecisionSummary>,
    pub recent_events: Vec<RouteDecisionEvent>,
}
```

Modify `src-tauri/src/services/proxy/mod.rs`:

```rust
pub mod routing_types;
```

- [ ] **Step 5: Run contract checks**

Run: `node scripts/local-routing-redaction.test.mjs`

Expected: PASS with `local routing redaction type contract ok`.

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src/lib/types/localRouting.ts scripts/local-routing-redaction.test.mjs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/routing_types.rs
git commit -m "feat: add local routing read model types"
```

### Task 2: Backend Workspace Command and TS Query Boundary

**Files:**
- Create: `src/lib/api/localRouting.ts`
- Create: `src/lib/queries/localRoutingQueries.ts`
- Create: `scripts/local-routing-query-service.test.mjs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write query boundary regression**

Create `scripts/local-routing-query-service.test.mjs`:

```js
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const querySource = readFileSync("src/lib/queries/localRoutingQueries.ts", "utf8");
const apiSource = readFileSync("src/lib/api/localRouting.ts", "utf8");

assert.match(querySource, /loadLocalRoutingWorkspace/);
assert.match(querySource, /loadLocalRoutingWorkspaceApi/);
assert.equal(querySource.includes("@tauri-apps/api/core"), false, "query layer must not invoke Tauri directly");
assert.match(apiSource, /invoke<LocalRoutingWorkspace>\("load_local_routing_workspace"\)/);
assert.match(apiSource, /isInvokeUnavailable/);

console.log("local routing query boundary ok");
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `node scripts/local-routing-query-service.test.mjs`

Expected: FAIL because local routing API/query files do not exist.

- [ ] **Step 3: Add TS API and query files**

Create `src/lib/api/localRouting.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

import { isInvokeUnavailable } from "@/lib/api/shared";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";

export function loadLocalRoutingWorkspaceApi() {
  return invoke<LocalRoutingWorkspace>("load_local_routing_workspace").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return previewWorkspace();
    }
    throw error;
  });
}

function previewWorkspace(): LocalRoutingWorkspace {
  return {
    proxyStatus: {
      running: false,
      bindAddr: "127.0.0.1",
      port: 14301,
      startedAt: null,
      lastError: null,
      activeRequests: 0,
      requestCount: 0,
    },
    settings: {
      mode: "balanced",
      strategyLabel: "低价优先 + 稳定保持",
      switchProbeEnabled: true,
    },
    summary: {
      currentPrimaryKeyName: null,
      schedulableCount: 0,
      candidateCount: 0,
      todayCostLabel: "¥0.0000",
      recentAbnormalCount: 0,
    },
    candidates: [],
    latestDecision: null,
    recentEvents: [],
  };
}
```

Create `src/lib/queries/localRoutingQueries.ts`:

```ts
import { loadLocalRoutingWorkspaceApi } from "@/lib/api/localRouting";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";

export function loadLocalRoutingWorkspace(): Promise<LocalRoutingWorkspace> {
  return loadLocalRoutingWorkspaceApi();
}
```

- [ ] **Step 4: Add backend workspace loader**

Modify `src-tauri/src/services/database.rs` with a method returning a redacted workspace. The initial implementation can use existing settings, key candidates, health, and request logs without changing routing behavior:

```rust
pub fn load_local_routing_workspace(
    &self,
    proxy_status: crate::models::proxy::ProxyStatus,
) -> Result<crate::services::proxy::routing_types::LocalRoutingWorkspace, String> {
    crate::services::proxy::routing_snapshot::load_local_routing_workspace(self, proxy_status)
}
```

Create `src-tauri/src/services/proxy/routing_snapshot.rs` with the first implementation:

```rust
use crate::{
    models::proxy::ProxyStatus,
    services::{
        database::AppDatabase,
        proxy::routing_types::{
            LocalRoutingCandidateRow, LocalRoutingSettingsView, LocalRoutingSummary,
            LocalRoutingWorkspace, RouteHealthState,
        },
    },
};

pub fn load_local_routing_workspace(
    database: &AppDatabase,
    proxy_status: ProxyStatus,
) -> Result<LocalRoutingWorkspace, String> {
    let settings = database.get_settings()?;
    let candidates = database.proxy_rich_route_candidates()?;
    let rows = candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| LocalRoutingCandidateRow {
            station_key_id: candidate.candidate.station_key_id,
            station_id: candidate.candidate.station_id,
            order: (index + 1) as i64,
            key_name: candidate.key_name,
            station_name: candidate.station_name,
            enabled: true,
            schedulable: true,
            selected: index == 0,
            current_affinity: false,
            status: candidate
                .health
                .as_ref()
                .map(|health| {
                    if health.cooldown_until.is_some() {
                        RouteHealthState::Cooling
                    } else if health.consecutive_failures > 0 {
                        RouteHealthState::Observing
                    } else {
                        RouteHealthState::Healthy
                    }
                })
                .unwrap_or(RouteHealthState::Unchecked),
            price_label: "价格待计算".to_string(),
            balance_label: "余额待确认".to_string(),
            latency_label: candidate
                .health
                .as_ref()
                .and_then(|health| health.avg_latency_ms)
                .map(|latency| format!("{latency} ms"))
                .unwrap_or_else(|| "未测".to_string()),
            last_outcome_label: candidate
                .health
                .as_ref()
                .and_then(|health| health.last_error_summary.clone())
                .unwrap_or_else(|| "暂无异常".to_string()),
            reason: "按当前路由候选顺序展示".to_string(),
        })
        .collect::<Vec<_>>();

    Ok(LocalRoutingWorkspace {
        proxy_status,
        settings: LocalRoutingSettingsView {
            mode: "balanced".to_string(),
            strategy_label: "低价优先 + 稳定保持".to_string(),
            switch_probe_enabled: true,
        },
        summary: LocalRoutingSummary {
            current_primary_key_name: rows
                .iter()
                .find(|row| row.selected)
                .map(|row| row.key_name.clone()),
            schedulable_count: rows.iter().filter(|row| row.schedulable).count() as i64,
            candidate_count: rows.len() as i64,
            today_cost_label: "¥0.0000".to_string(),
            recent_abnormal_count: 0,
        },
        candidates: rows,
        latest_decision: None,
        recent_events: Vec::new(),
    })
}
```

Modify `src-tauri/src/services/proxy/mod.rs`:

```rust
pub mod routing_snapshot;
```

- [ ] **Step 5: Add Tauri command**

Modify `src-tauri/src/commands/mod.rs`:

```rust
#[tauri::command]
pub fn load_local_routing_workspace(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<crate::services::proxy::routing_types::LocalRoutingWorkspace, String> {
    let settings = database.get_settings()?;
    let status = proxy.status(settings.local_proxy_port);
    database.load_local_routing_workspace(status)
}
```

Modify `src-tauri/src/lib.rs` to register the command:

```rust
commands::load_local_routing_workspace,
```

- [ ] **Step 6: Verify**

Run: `node scripts/local-routing-query-service.test.mjs`

Expected: PASS.

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml`

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add -- src/lib/api/localRouting.ts src/lib/queries/localRoutingQueries.ts scripts/local-routing-query-service.test.mjs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/services/database.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/routing_snapshot.rs
git commit -m "feat: load local routing workspace"
```

### Task 3: Status/Edit UI Skeleton Without Behavior Change

**Files:**
- Modify: `src/features/routing/RoutingPage.tsx`
- Create: `src/features/routing/LocalRoutingStatusTab.tsx`
- Create: `src/features/routing/LocalRoutingEditTab.tsx`
- Create: `src/features/routing/LocalRoutingCandidateRow.tsx`
- Create: `src/features/routing/RouteExplanationPanel.tsx`
- Create: `scripts/local-routing-page-layout.test.mjs`

- [ ] **Step 1: Write layout regression**

Create `scripts/local-routing-page-layout.test.mjs`:

```js
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const page = readFileSync("src/features/routing/RoutingPage.tsx", "utf8");
const status = readFileSync("src/features/routing/LocalRoutingStatusTab.tsx", "utf8");
const edit = readFileSync("src/features/routing/LocalRoutingEditTab.tsx", "utf8");

assert.match(page, /SegmentedControl/);
assert.match(page, /value=\{activeTab\}/);
assert.match(page, /状态/);
assert.match(page, /编辑/);
assert.match(status, /本地端点/);
assert.match(status, /当前主 Key/);
assert.match(edit, /低价优先 \+ 稳定保持/);
assert.equal(edit.includes("权重"), false, "edit tab must not expose weight");
assert.equal(page.includes("保存策略"), false, "page-level save strategy button must not exist");

console.log("local routing page layout contract ok");
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `node scripts/local-routing-page-layout.test.mjs`

Expected: FAIL because new tab components do not exist.

- [ ] **Step 3: Add candidate row component**

Create `src/features/routing/LocalRoutingCandidateRow.tsx`:

```tsx
import { GripVertical, KeyRound } from "lucide-react";

import type { LocalRoutingCandidateRow as Candidate } from "@/lib/types/localRouting";

type LocalRoutingCandidateRowProps = {
  candidate: Candidate;
  variant: "status" | "edit";
  dragHandleProps?: React.HTMLAttributes<HTMLButtonElement>;
};

export function LocalRoutingCandidateRow({ candidate, variant, dragHandleProps }: LocalRoutingCandidateRowProps) {
  return (
    <div className="grid min-w-[760px] grid-cols-[2.5rem_minmax(13rem,1fr)_7rem_7rem_7rem_minmax(12rem,1fr)] items-center gap-3 border-b border-slate-100 px-3 py-2 text-sm last:border-b-0">
      <div className="flex items-center gap-2 text-slate-500">
        {variant === "edit" ? (
          <button
            type="button"
            className="inline-flex h-7 w-7 items-center justify-center rounded-md text-slate-400 transition-colors hover:bg-slate-100 hover:text-slate-700"
            aria-label={`拖动 ${candidate.keyName} 调整优先级`}
            {...dragHandleProps}
          >
            <GripVertical className="h-4 w-4" />
          </button>
        ) : (
          <span className="inline-flex h-7 w-7 items-center justify-center rounded-md bg-slate-50 font-medium text-slate-600">
            {candidate.order}
          </span>
        )}
      </div>
      <div className="min-w-0">
        <div className="flex items-center gap-2 font-medium text-slate-900">
          <KeyRound className="h-4 w-4 text-slate-400" />
          <span className="truncate">{candidate.keyName}</span>
        </div>
        <div className="truncate text-xs text-slate-500">{candidate.stationName}</div>
      </div>
      <div className="text-slate-700">{candidate.status}</div>
      <div className="text-slate-700">{candidate.priceLabel}</div>
      <div className="text-slate-700">{candidate.balanceLabel}</div>
      <div className="truncate text-xs text-slate-500">{candidate.reason}</div>
    </div>
  );
}
```

- [ ] **Step 4: Add status and edit tabs**

Create `src/features/routing/RouteExplanationPanel.tsx`:

```tsx
import type { RouteDecisionSummary } from "@/lib/types/localRouting";

export function RouteExplanationPanel({ decision }: { decision: RouteDecisionSummary | null }) {
  if (!decision) {
    return (
      <div className="rounded-lg border border-dashed border-slate-200 bg-slate-50 px-3 py-4 text-sm text-slate-500">
        暂无路由决策记录
      </div>
    );
  }

  return (
    <div className="rounded-lg border border-slate-200 bg-white px-3 py-3 text-sm">
      <div className="font-medium text-slate-900">{decision.selectedKeyName ?? "未选中 Key"}</div>
      <div className="mt-1 text-slate-600">{decision.selectedReason}</div>
    </div>
  );
}
```

Create `src/features/routing/LocalRoutingStatusTab.tsx`:

```tsx
import { ExternalLink, Play, Power } from "lucide-react";

import { Button } from "@/components/ui/Button";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import { LocalRoutingCandidateRow } from "./LocalRoutingCandidateRow";
import { RouteExplanationPanel } from "./RouteExplanationPanel";

export function LocalRoutingStatusTab({ workspace }: { workspace: LocalRoutingWorkspace }) {
  return (
    <div className="space-y-4">
      <section className="rounded-lg border border-slate-200 bg-white p-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-sm font-semibold text-slate-900">本地端点</h2>
            <p className="mt-1 text-sm text-slate-500">
              http://{workspace.proxyStatus.bindAddr}:{workspace.proxyStatus.port}
            </p>
          </div>
          <Button variant={workspace.proxyStatus.running ? "secondary" : "primary"}>
            <Power className="h-4 w-4" />
            {workspace.proxyStatus.running ? "停止" : "启动"}
          </Button>
        </div>
      </section>

      <section className="grid gap-3 md:grid-cols-4">
        <Metric label="当前主 Key" value={workspace.summary.currentPrimaryKeyName ?? "未选择"} />
        <Metric label="可调度 Key" value={`${workspace.summary.schedulableCount}/${workspace.summary.candidateCount}`} />
        <Metric label="今日成本" value={workspace.summary.todayCostLabel} />
        <Metric label="近期异常" value={`${workspace.summary.recentAbnormalCount}`} />
      </section>

      <section className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
        <div className="border-b border-slate-100 px-3 py-2 text-sm font-medium text-slate-900">候选顺序</div>
        {workspace.candidates.map((candidate) => (
          <LocalRoutingCandidateRow key={candidate.stationKeyId} candidate={candidate} variant="status" />
        ))}
      </section>

      <section className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-sm font-semibold text-slate-900">最近路由解释</h2>
          <div className="flex gap-2">
            <Button variant="secondary"><Play className="h-4 w-4" />模拟一次请求</Button>
            <Button variant="secondary"><ExternalLink className="h-4 w-4" />打开渠道状态</Button>
          </div>
        </div>
        <RouteExplanationPanel decision={workspace.latestDecision} />
      </section>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-slate-200 bg-white px-3 py-3">
      <div className="text-xs text-slate-500">{label}</div>
      <div className="mt-1 truncate text-sm font-semibold text-slate-900">{value}</div>
    </div>
  );
}
```

Create `src/features/routing/LocalRoutingEditTab.tsx`:

```tsx
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import { LocalRoutingCandidateRow } from "./LocalRoutingCandidateRow";

export function LocalRoutingEditTab({ workspace }: { workspace: LocalRoutingWorkspace }) {
  return (
    <div className="space-y-4">
      <section className="rounded-lg border border-slate-200 bg-white p-4">
        <div className="text-sm font-semibold text-slate-900">低价优先 + 稳定保持</div>
        <div className="mt-1 text-sm text-slate-500">均衡模式，自动保存修改。</div>
      </section>
      <section className="overflow-x-auto rounded-lg border border-slate-200 bg-white">
        <div className="border-b border-slate-100 px-3 py-2 text-sm font-medium text-slate-900">Key 优先级</div>
        {workspace.candidates.map((candidate) => (
          <LocalRoutingCandidateRow key={candidate.stationKeyId} candidate={candidate} variant="edit" />
        ))}
      </section>
    </div>
  );
}
```

- [ ] **Step 5: Refactor RoutingPage to load workspace and tabs**

Modify `src/features/routing/RoutingPage.tsx` to keep existing alias/simulator sections below the new local-routing tabs or behind the edit tab. Add:

```tsx
const [activeTab, setActiveTab] = useState<"status" | "edit">("status");
const [localRoutingWorkspace, setLocalRoutingWorkspace] = useState<LocalRoutingWorkspace | null>(null);
```

Use `PageScaffold` actions:

```tsx
<SegmentedControl
  ariaLabel="本地路由视图"
  value={activeTab}
  options={[
    { value: "status", label: "状态" },
    { value: "edit", label: "编辑" },
  ]}
  onChange={(value) => setActiveTab(value as "status" | "edit")}
/>
```

Render:

```tsx
{localRoutingWorkspace && activeTab === "status" && (
  <LocalRoutingStatusTab workspace={localRoutingWorkspace} />
)}
{localRoutingWorkspace && activeTab === "edit" && (
  <LocalRoutingEditTab workspace={localRoutingWorkspace} />
)}
```

- [ ] **Step 6: Verify**

Run: `node scripts/local-routing-page-layout.test.mjs`

Expected: PASS.

Run: `pnpm.cmd build`

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add -- src/features/routing/RoutingPage.tsx src/features/routing/LocalRoutingStatusTab.tsx src/features/routing/LocalRoutingEditTab.tsx src/features/routing/LocalRoutingCandidateRow.tsx src/features/routing/RouteExplanationPanel.tsx scripts/local-routing-page-layout.test.mjs
git commit -m "feat: add local routing status edit shell"
```

### Task 4: Global `routing_order` Migration and Reorder Command

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/routing.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src/lib/types/localRouting.ts`
- Modify: `src/lib/api/localRouting.ts`
- Create: `scripts/local-routing-reorder.test.mjs`

- [ ] **Step 1: Write Rust migration test**

Add a database test in `src-tauri/src/services/database.rs`:

```rust
#[test]
fn local_routing_order_migration_initializes_global_order_without_changing_priority() {
    let database = test_database();
    let station = create_test_station(&database, "order-station");
    let key_a = create_test_station_key_with_priority(&database, &station.id, "A", 20);
    let key_b = create_test_station_key_with_priority(&database, &station.id, "B", 10);

    let rows = database.local_routing_order_rows_for_tests().expect("routing order rows");

    let row_a = rows.iter().find(|row| row.station_key_id == key_a.id).expect("key a row");
    let row_b = rows.iter().find(|row| row.station_key_id == key_b.id).expect("key b row");

    assert_eq!(row_b.routing_order, 1);
    assert_eq!(row_a.routing_order, 2);
    assert_eq!(row_a.priority, 20);
    assert_eq!(row_b.priority, 10);
}
```

- [ ] **Step 2: Run focused test and confirm it fails**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml local_routing_order_migration_initializes_global_order_without_changing_priority`

Expected: FAIL because helper/schema does not exist.

- [ ] **Step 3: Add migration and database APIs**

Modify schema setup/migrations in `src-tauri/src/services/database.rs`:

```rust
add_column_if_missing(connection, "station_keys", "routing_order", "INTEGER")?;
initialize_station_key_routing_order(connection)?;
```

Add initializer:

```rust
fn initialize_station_key_routing_order(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare(
        "SELECT id
           FROM station_keys
          WHERE routing_order IS NULL
          ORDER BY priority ASC, created_at ASC, id ASC",
    )?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut next_order = connection.query_row(
        "SELECT COALESCE(MAX(routing_order), 0) + 1 FROM station_keys WHERE routing_order IS NOT NULL",
        [],
        |row| row.get::<_, i64>(0),
    )?;

    for id in ids {
        connection.execute(
            "UPDATE station_keys SET routing_order = ?1 WHERE id = ?2",
            params![next_order, id],
        )?;
        next_order += 1;
    }
    Ok(())
}
```

Add reorder method:

```rust
pub fn reorder_local_routing_keys(&self, station_key_ids: Vec<String>) -> Result<(), String> {
    let mut connection = self.connection()?;
    let tx = connection.transaction().map_err(|error| format!("开始本地路由排序事务失败: {error}"))?;
    let updated_at = now_string();
    for (index, id) in station_key_ids.iter().enumerate() {
        let affected = tx
            .execute(
                "UPDATE station_keys SET routing_order = ?1, updated_at = ?2 WHERE id = ?3",
                params![(index + 1) as i64, updated_at, id],
            )
            .map_err(|error| format!("更新本地路由排序失败: {error}"))?;
        if affected == 0 {
            return Err(format!("Station Key 不存在，无法排序: {id}"));
        }
    }
    tx.commit().map_err(|error| format!("提交本地路由排序失败: {error}"))
}
```

- [ ] **Step 4: Add command and frontend API**

Add type to `src/lib/types/localRouting.ts`:

```ts
export type ReorderLocalRoutingKeysInput = {
  stationKeyIds: string[];
};
```

Add API in `src/lib/api/localRouting.ts`:

```ts
export function reorderLocalRoutingKeys(input: ReorderLocalRoutingKeysInput) {
  return invoke<LocalRoutingWorkspace>("reorder_local_routing_keys", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return previewWorkspace();
    }
    throw error;
  });
}
```

Add command:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderLocalRoutingKeysInput {
    pub station_key_ids: Vec<String>,
}

#[tauri::command]
pub fn reorder_local_routing_keys(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
    input: ReorderLocalRoutingKeysInput,
) -> Result<crate::services::proxy::routing_types::LocalRoutingWorkspace, String> {
    database.reorder_local_routing_keys(input.station_key_ids)?;
    let settings = database.get_settings()?;
    database.load_local_routing_workspace(proxy.status(settings.local_proxy_port))
}
```

- [ ] **Step 5: Write frontend reorder API regression**

Create `scripts/local-routing-reorder.test.mjs`:

```js
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const api = readFileSync("src/lib/api/localRouting.ts", "utf8");
const types = readFileSync("src/lib/types/localRouting.ts", "utf8");
const edit = readFileSync("src/features/routing/LocalRoutingEditTab.tsx", "utf8");

assert.match(types, /ReorderLocalRoutingKeysInput/);
assert.match(api, /reorderLocalRoutingKeys/);
assert.match(api, /"reorder_local_routing_keys"/);
assert.equal(edit.includes("权重"), false);

console.log("local routing reorder contract ok");
```

- [ ] **Step 6: Verify**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml local_routing_order_migration_initializes_global_order_without_changing_priority`

Expected: PASS.

Run: `node scripts/local-routing-reorder.test.mjs`

Expected: PASS.

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml`

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/models/routing.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/types/localRouting.ts src/lib/api/localRouting.ts
git commit -m "feat: persist local routing order"
```

### Task 5: Drag Reorder Auto-Save UI

**Files:**
- Modify: `src/features/routing/LocalRoutingEditTab.tsx`
- Modify: `src/features/routing/LocalRoutingCandidateRow.tsx`
- Modify: `scripts/local-routing-reorder.test.mjs`

- [ ] **Step 1: Add failing assertions for dnd-kit and no page save button**

Update `scripts/local-routing-reorder.test.mjs`:

```js
assert.match(edit, /DndContext/);
assert.match(edit, /SortableContext/);
assert.match(edit, /reorderLocalRoutingKeys/);
assert.match(edit, /保存中/);
assert.match(edit, /保存失败/);
assert.equal(edit.includes("保存策略"), false);
```

- [ ] **Step 2: Run and confirm failure**

Run: `node scripts/local-routing-reorder.test.mjs`

Expected: FAIL because edit tab is not sortable yet.

- [ ] **Step 3: Implement sortable edit tab**

Use existing `@dnd-kit` imports:

```tsx
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
```

In `LocalRoutingEditTab`, keep local rows:

```tsx
const [rows, setRows] = useState(workspace.candidates);
const [syncState, setSyncState] = useState<"synced" | "saving" | "failed">("synced");

useEffect(() => {
  if (syncState !== "saving") {
    setRows(workspace.candidates);
  }
}, [syncState, workspace.candidates]);
```

Handle drag:

```tsx
async function handleDragEnd(event: DragEndEvent) {
  const { active, over } = event;
  if (!over || active.id === over.id) {
    return;
  }
  const oldIndex = rows.findIndex((row) => row.stationKeyId === active.id);
  const newIndex = rows.findIndex((row) => row.stationKeyId === over.id);
  if (oldIndex === -1 || newIndex === -1) {
    return;
  }
  const nextRows = arrayMove(rows, oldIndex, newIndex).map((row, index) => ({ ...row, order: index + 1 }));
  setRows(nextRows);
  setSyncState("saving");
  try {
    await reorderLocalRoutingKeys({ stationKeyIds: nextRows.map((row) => row.stationKeyId) });
    setSyncState("synced");
  } catch (error) {
    setSyncState("failed");
  }
}
```

Render sync text:

```tsx
<span className="text-xs text-slate-500">
  {syncState === "saving" ? "保存中" : syncState === "failed" ? "保存失败" : "已同步"}
</span>
```

- [ ] **Step 4: Verify**

Run: `node scripts/local-routing-reorder.test.mjs`

Expected: PASS.

Run: `pnpm.cmd build`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add -- src/features/routing/LocalRoutingEditTab.tsx src/features/routing/LocalRoutingCandidateRow.tsx scripts/local-routing-reorder.test.mjs
git commit -m "feat: auto-save local routing reorder"
```

### Task 6: Extract Routing Policy Pure Module With Legacy Wrapper

**Files:**
- Create: `src-tauri/src/services/proxy/routing_policy.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`

- [ ] **Step 1: Write focused legacy compatibility test**

Move or add tests in `routing_policy.rs`:

```rust
#[test]
fn legacy_priority_fallback_keeps_existing_candidate_order() {
    let request = route_request(RoutingPolicy::PriorityFallback);
    let candidates = vec![
        rich_candidate("second", 2),
        rich_candidate("first", 1),
    ];

    let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

    assert_eq!(selected.accepted[0].candidate.station_key_id, "first");
    assert_eq!(selected.accepted[1].candidate.station_key_id, "second");
}
```

- [ ] **Step 2: Run and confirm baseline passes before extraction**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml legacy_priority_fallback_keeps_existing_candidate_order`

Expected: PASS after adding the test in the existing module, proving current behavior.

- [ ] **Step 3: Extract without behavior change**

Move these functions from `router.rs` to `routing_policy.rs`:

```rust
pub fn select_route_candidates(...)
fn mapped_model(...)
fn collect_rejections(...)
fn candidate_score(...)
fn cheap_first_score(...)
fn balance_penalty(...)
fn estimated_cost(...)
fn candidate_economic_reasons(...)
```

Keep re-export in `router.rs`:

```rust
pub use crate::services::proxy::routing_policy::select_route_candidates;
```

- [ ] **Step 4: Verify old tests**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml selector_`

Expected: PASS for existing selector tests.

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add -- src-tauri/src/services/proxy/router.rs src-tauri/src/services/proxy/routing_policy.rs src-tauri/src/services/proxy/mod.rs
git commit -m "refactor: extract routing policy selector"
```

### Task 7: Decision Facts and `cost_stable_first`

**Files:**
- Modify: `src-tauri/src/models/routing.rs`
- Modify: `src-tauri/src/services/proxy/routing_policy.rs`
- Modify: `src-tauri/src/services/proxy/routing_types.rs`
- Modify: `src/lib/types/routing.ts`

- [ ] **Step 1: Write `cost_stable_first` tests**

Add tests:

```rust
#[test]
fn cost_stable_first_keeps_current_key_when_price_delta_is_small() {
    let request = route_request_with_affinity("current", RoutingPolicy::CostStableFirst);
    let candidates = vec![
        priced_candidate("current", 2, 0.012),
        priced_candidate("cheaper", 1, 0.011),
    ];

    let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

    assert_eq!(selected.accepted[0].candidate.station_key_id, "current");
    assert!(selected.explanations.iter().any(|item| {
        item.station_key_id == "current" && item.reasons.iter().any(|reason| reason.contains("stability"))
    }));
}

#[test]
fn cost_stable_first_switches_when_current_key_has_hard_failure() {
    let request = route_request_with_affinity("current", RoutingPolicy::CostStableFirst);
    let candidates = vec![
        auth_error_candidate("current", 1),
        priced_candidate("fallback", 2, 0.02),
    ];

    let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

    assert_eq!(selected.accepted[0].candidate.station_key_id, "fallback");
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml cost_stable_first_`

Expected: FAIL because policy value and affinity inputs are not implemented.

- [ ] **Step 3: Add policy enum and TS union**

Rust:

```rust
#[serde(rename = "cost_stable_first")]
CostStableFirst,
```

TypeScript:

```ts
export type RoutingPolicy =
  | "priority_fallback"
  | "stable_first"
  | "backup_only"
  | "cheap_first"
  | "cost_stable_first";
```

- [ ] **Step 4: Implement decision facts and sorting tuple**

In `routing_policy.rs`, replace opaque score-only logic with:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CandidateRank {
    hard_rejected: bool,
    stability_bucket: i64,
    price_bucket: i64,
    health_penalty: i64,
    routing_order: i64,
    station_key_id: String,
}
```

For legacy policies, populate the tuple so existing tests still pass.

- [ ] **Step 5: Verify**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml cost_stable_first_`

Expected: PASS.

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml selector_`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/models/routing.rs src-tauri/src/services/proxy/routing_policy.rs src-tauri/src/services/proxy/routing_types.rs src/lib/types/routing.ts
git commit -m "feat: add cost stable routing policy"
```

### Task 8: Failure Classifier and Health State Machine

**Files:**
- Create: `src-tauri/src/services/proxy/routing_failure.rs`
- Create: `src-tauri/src/services/proxy/routing_health.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write classifier tests**

Create tests in `routing_failure.rs`:

```rust
#[test]
fn classifier_treats_single_timeout_as_observe() {
    let failure = classify_route_failure(RouteFailureInput::timeout(false));

    assert_eq!(failure.kind, RouteFailureKind::Timeout);
    assert_eq!(failure.action, RouteFailureAction::Observe);
    assert!(failure.retryable_before_output);
}

#[test]
fn classifier_ignores_client_bad_request_for_key_health() {
    let failure = classify_route_failure(RouteFailureInput::http_status(400, false));

    assert_eq!(failure.action, RouteFailureAction::IgnoreForKeyHealth);
    assert_eq!(failure.scope, RouteFailureScope::RequestOnly);
}
```

- [ ] **Step 2: Write health transition tests**

Create tests in `routing_health.rs`:

```rust
#[test]
fn one_ambiguous_failure_moves_healthy_key_to_observing() {
    let current = RouteHealthState::Healthy;
    let failure = ClassifiedRouteFailure::timeout_observe();

    let next = apply_health_transition(current, &failure, 1, 0);

    assert_eq!(next.state, RouteHealthState::Observing);
    assert!(next.cooldown_until_ms.is_none());
}

#[test]
fn repeated_ambiguous_failures_move_to_cooling() {
    let current = RouteHealthState::Observing;
    let failure = ClassifiedRouteFailure::timeout_observe();

    let next = apply_health_transition(current, &failure, 3, 1_000);

    assert_eq!(next.state, RouteHealthState::Cooling);
    assert!(next.cooldown_until_ms.unwrap() > 1_000);
}
```

- [ ] **Step 3: Run and confirm failure**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml classifier_ one_ambiguous repeated_ambiguous`

Expected: FAIL because modules do not exist.

- [ ] **Step 4: Implement classifier and health modules**

Define:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteFailureKind {
    AuthError,
    InsufficientBalance,
    RateLimited,
    ModelUnavailable,
    CapabilityMismatch,
    BadRequest,
    TemporaryNetwork,
    Upstream5xx,
    Timeout,
    StreamInterrupted,
    Unknown,
}
```

Define:

```rust
pub fn classify_route_failure(input: RouteFailureInput) -> ClassifiedRouteFailure {
    if input.output_started && input.transport_error {
        return ClassifiedRouteFailure::stream_interrupted();
    }
    match input.http_status {
        Some(401 | 403) => ClassifiedRouteFailure::auth_error(),
        Some(402) => ClassifiedRouteFailure::insufficient_balance(),
        Some(429) => ClassifiedRouteFailure::rate_limited(input.retry_after_ms),
        Some(500..=599) => ClassifiedRouteFailure::upstream_5xx(),
        Some(400) => ClassifiedRouteFailure::bad_request(),
        _ if input.timeout => ClassifiedRouteFailure::timeout_observe(),
        _ => ClassifiedRouteFailure::unknown_observe(),
    }
}
```

Implement `apply_health_transition(...)` with the state transitions from the spec.

- [ ] **Step 5: Wire runtime failure writes through classifier**

Replace direct calls:

```rust
record_candidate_failure(context, candidate, "warning", &checked_at, &error);
```

with:

```rust
let classified = classify_route_failure(RouteFailureInput::from_upstream_status(response.status_code, false));
record_classified_candidate_failure(context, candidate, &classified, &checked_at);
```

- [ ] **Step 6: Verify**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml classifier_ one_ambiguous repeated_ambiguous`

Expected: PASS.

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml runtime_skips_key_in_cooldown_and_uses_next_candidate`

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/src/services/proxy/routing_failure.rs src-tauri/src/services/proxy/routing_health.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/database.rs
git commit -m "feat: classify route failures and health states"
```

### Task 9: Lifecycle-Aware Stream Logging

**Files:**
- Create: `src-tauri/src/services/proxy/routing_lifecycle.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write streaming log tests**

Add tests in `runtime.rs`:

```rust
#[test]
fn streaming_request_log_finalizes_after_body_forwarding() {
    let fixture = streaming_fixture_with_two_chunks();
    let context = fixture.context();
    let request = chat_request("gpt-5.4", true);

    let response = forward_chat_request(&context, &request);
    assert!(matches!(response.body, ProxyResponseBody::Streamed(_)));

    let mut downstream = Vec::new();
    write_http_response_to_writer(&mut downstream, response).expect("write stream");

    let logs = context.database.list_request_logs().expect("logs");
    let log = logs.first().expect("request log");
    assert_eq!(log.status, "success");
    assert!(log.finished_at.is_some());
}

#[test]
fn stream_interruption_after_first_output_is_not_replayed() {
    let fixture = streaming_fixture_that_breaks_after_first_chunk();
    let context = fixture.context();
    let request = chat_request("gpt-5.4", true);

    let response = forward_chat_request(&context, &request);
    let result = write_http_response_to_failing_writer(response);

    assert!(result.is_err());
    let logs = context.database.list_request_logs().expect("logs");
    assert_eq!(logs[0].status, "interrupted");
    assert_eq!(logs[0].fallback_count, 0);
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml streaming_request_log_finalizes_after_body_forwarding stream_interruption_after_first_output_is_not_replayed`

Expected: FAIL because logs are inserted before body write.

- [ ] **Step 3: Introduce lifecycle finalization**

Add lifecycle status:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestLifecycleStatus {
    Received,
    CandidateSelected,
    HeadersReceived,
    FirstOutputWritten,
    Completed,
    Interrupted,
}
```

Change `handle_connection` flow:

```rust
let mut response = handle_proxy_request(context, &request);
let write_result = write_http_response(&mut stream, &mut response);
finalize_request_log(context, method, path, response, started_at, started, write_result.as_ref());
```

Change `write_http_response` to report:

```rust
pub struct WriteOutcome {
    pub first_output_written: bool,
    pub bytes_written: u64,
}
```

- [ ] **Step 4: Add lifecycle schema columns**

Add nullable columns:

```rust
add_column_if_missing(connection, "request_logs", "lifecycle_status", "TEXT")?;
add_column_if_missing(connection, "request_logs", "route_decision_json", "TEXT")?;
```

- [ ] **Step 5: Verify**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml streaming_request_log_finalizes_after_body_forwarding stream_interruption_after_first_output_is_not_replayed`

Expected: PASS.

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml forward_chat_request_preserves_stream_metadata_for_sse_success forward_responses_request_streams_with_sse_accept_header`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/proxy/routing_lifecycle.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/database.rs
git commit -m "feat: finalize stream route logs by lifecycle"
```

### Task 10: Route Affinity and Stability Hysteresis

**Files:**
- Create: `src-tauri/src/services/proxy/routing_affinity.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/routing_policy.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`

- [ ] **Step 1: Write affinity tests**

Create tests:

```rust
#[test]
fn affinity_key_uses_endpoint_and_model() {
    let key_a = RouteAffinityKey::new(RouteEndpointKind::ChatCompletions, Some("gpt-4o-mini"));
    let key_b = RouteAffinityKey::new(RouteEndpointKind::Responses, Some("gpt-4o-mini"));

    assert_ne!(key_a, key_b);
}

#[test]
fn models_endpoint_does_not_update_affinity() {
    let mut store = RouteAffinityStore::default();

    store.record_success(RouteEndpointKind::Models, None, "key-a", 1_000);

    assert!(store.lookup(RouteEndpointKind::Models, None, 1_001).is_none());
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml affinity_key_uses_endpoint_and_model models_endpoint_does_not_update_affinity`

Expected: FAIL because affinity module does not exist.

- [ ] **Step 3: Implement in-memory affinity store**

Create:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouteAffinityKey {
    endpoint: RouteEndpointKind,
    model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RouteAffinityValue {
    pub station_key_id: String,
    pub expires_at_ms: i64,
}

#[derive(Default)]
pub struct RouteAffinityStore {
    entries: std::collections::HashMap<RouteAffinityKey, RouteAffinityValue>,
}
```

Skip `Models` and local usage endpoints in `record_success`.

- [ ] **Step 4: Wire store into proxy context**

Add to `ProxyServerContext`:

```rust
route_affinity: Arc<Mutex<RouteAffinityStore>>,
```

Update after successful request completion:

```rust
context.route_affinity.lock().unwrap().record_success(endpoint, model.as_deref(), station_key_id, now_ms);
```

- [ ] **Step 5: Verify**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml affinity_ models_endpoint_does_not_update_affinity`

Expected: PASS.

Run: `cargo check --manifest-path .\src-tauri\Cargo.toml`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/proxy/routing_affinity.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/routing_policy.rs src-tauri/src/services/proxy/runtime.rs
git commit -m "feat: keep stable route affinity"
```

### Task 11: Switch-Before-Confirm Probe

**Files:**
- Create: `src-tauri/src/services/proxy/routing_probe.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write probe skip tests**

Create tests:

```rust
#[test]
fn switch_probe_skips_depleted_cooling_and_auth_error_keys() {
    let candidates = vec![
        probe_candidate("depleted", RouteHealthState::InsufficientBalance),
        probe_candidate("cooling", RouteHealthState::Cooling),
        probe_candidate("auth", RouteHealthState::AuthError),
        probe_candidate("healthy", RouteHealthState::Healthy),
    ];

    let selected = next_probe_candidate(&candidates, "gpt-4o-mini");

    assert_eq!(selected.unwrap().station_key_id, "healthy");
}

#[test]
fn probe_cache_deduplicates_burst_for_same_key_endpoint_model() {
    let mut cache = ProbeConfirmationCache::default();
    let key = ProbeCacheKey::new("key-a", RouteEndpointKind::ChatCompletions, Some("gpt-4o-mini"));

    assert!(cache.should_probe(&key, 1_000));
    cache.record_pass(&key, 1_000, 30_000);
    assert!(!cache.should_probe(&key, 1_100));
}
```

- [ ] **Step 2: Run and confirm failure**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml switch_probe_skips_depleted probe_cache_deduplicates`

Expected: FAIL because probe module does not exist.

- [ ] **Step 3: Implement probe orchestration**

Create:

```rust
pub struct ProbeCacheKey {
    pub station_key_id: String,
    pub endpoint: RouteEndpointKind,
    pub mapped_model: Option<String>,
}

pub struct ProbeConfirmationCache {
    passed_until_ms: HashMap<ProbeCacheKey, i64>,
}
```

Implement:

```rust
pub fn should_skip_probe(state: &RouteHealthState) -> bool {
    matches!(
        state,
        RouteHealthState::InsufficientBalance
            | RouteHealthState::AuthError
            | RouteHealthState::ManualDisabled
            | RouteHealthState::Cooling
    )
}
```

Use existing low-token connectivity probe body from `commands::build_station_key_connectivity_probe_body`.

- [ ] **Step 4: Wire into failover path before output**

Before switching candidate after a retryable pre-output failure:

```rust
let confirmation = confirm_fallback_candidate(context, &candidate, &route_request)?;
if !confirmation.usable {
    continue;
}
```

Do not call this after `first_output_written`.

- [ ] **Step 5: Verify**

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml switch_probe_skips_depleted probe_cache_deduplicates`

Expected: PASS.

Run: `cargo test --manifest-path .\src-tauri\Cargo.toml runtime_skips_key_in_cooldown_and_uses_next_candidate`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/proxy/routing_probe.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/commands/mod.rs
git commit -m "feat: confirm fallback keys before switching"
```

### Task 12: Structured Route Explanations and Diagnostics Links

**Files:**
- Modify: `src-tauri/src/services/proxy/routing_explanation.rs`
- Modify: `src-tauri/src/services/proxy/routing_snapshot.rs`
- Modify: `src/features/routing/RouteExplanationPanel.tsx`
- Modify: `src/features/routing/LocalRoutingStatusTab.tsx`
- Create: `scripts/local-routing-explanation.test.mjs`

- [ ] **Step 1: Write explanation regression**

Create `scripts/local-routing-explanation.test.mjs`:

```js
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const panel = readFileSync("src/features/routing/RouteExplanationPanel.tsx", "utf8");
const status = readFileSync("src/features/routing/LocalRoutingStatusTab.tsx", "utf8");

assert.match(panel, /keptForStability/);
assert.match(panel, /selectedReason/);
assert.match(status, /打开渠道状态/);
assert.match(status, /查看请求日志/);
assert.equal(panel.includes("apiKey"), false);
assert.equal(panel.includes("Authorization"), false);

console.log("local routing explanation contract ok");
```

- [ ] **Step 2: Run and confirm failure**

Run: `node scripts/local-routing-explanation.test.mjs`

Expected: FAIL until panel renders structured fields and status tab includes both links.

- [ ] **Step 3: Implement explanation rendering**

Update `RouteExplanationPanel.tsx`:

```tsx
{decision.keptForStability && (
  <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700">
    为保持稳定继续使用
  </span>
)}
<dl className="mt-3 grid gap-2 text-xs text-slate-600 sm:grid-cols-3">
  <div>
    <dt className="text-slate-400">候选</dt>
    <dd>{decision.candidateCount}</dd>
  </div>
  <div>
    <dt className="text-slate-400">跳过</dt>
    <dd>{decision.rejectedCount}</dd>
  </div>
  <div>
    <dt className="text-slate-400">模型</dt>
    <dd className="truncate">{decision.mappedModel ?? decision.clientModel ?? "未指定"}</dd>
  </div>
</dl>
```

Add status action:

```tsx
<Button variant="secondary"><ExternalLink className="h-4 w-4" />查看请求日志</Button>
```

- [ ] **Step 4: Verify**

Run: `node scripts/local-routing-explanation.test.mjs`

Expected: PASS.

Run: `pnpm.cmd build`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add -- src-tauri/src/services/proxy/routing_explanation.rs src-tauri/src/services/proxy/routing_snapshot.rs src/features/routing/RouteExplanationPanel.tsx src/features/routing/LocalRoutingStatusTab.tsx scripts/local-routing-explanation.test.mjs
git commit -m "feat: show structured local route explanations"
```

### Task 13: Full Verification and Visual QA

**Files:**
- Modify only files needed to fix verification failures found in this task.

- [ ] **Step 1: Run backend focused tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml selector_
cargo test --manifest-path .\src-tauri\Cargo.toml cost_stable_first_
cargo test --manifest-path .\src-tauri\Cargo.toml classifier_
cargo test --manifest-path .\src-tauri\Cargo.toml affinity_
cargo test --manifest-path .\src-tauri\Cargo.toml switch_probe_
cargo test --manifest-path .\src-tauri\Cargo.toml streaming_request_log_finalizes_after_body_forwarding stream_interruption_after_first_output_is_not_replayed
```

Expected: all PASS.

- [ ] **Step 2: Run frontend regression scripts**

Run:

```powershell
node scripts/local-routing-query-service.test.mjs
node scripts/local-routing-page-layout.test.mjs
node scripts/local-routing-reorder.test.mjs
node scripts/local-routing-redaction.test.mjs
node scripts/local-routing-explanation.test.mjs
node scripts/channel-status-drag-transform.test.mjs
node scripts/delete-confirmation-dialogs.test.mjs
```

Expected: all PASS.

- [ ] **Step 3: Run build/checks**

Run:

```powershell
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: both PASS.

- [ ] **Step 4: Browser visual check**

Start dev server:

```powershell
pnpm.cmd dev -- --port 1430
```

Open `http://127.0.0.1:1430/` and verify:

- `本地路由` first viewport shows status, not model mapping.
- Top-right segmented control shows `状态 / 编辑`.
- Edit tab has draggable rows with numeric order, not `P1/P2`.
- No `权重`.
- No `保存策略`.
- Text fits at desktop width and narrow width.

- [ ] **Step 5: Final status and commit**

Run:

```powershell
git status --short
git diff --cached --name-only
```

Stage only exact files touched by fixes in this task:

```powershell
git add -- src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/routing_lifecycle.rs src-tauri/src/services/proxy/routing_policy.rs src-tauri/src/services/proxy/routing_failure.rs src-tauri/src/services/proxy/routing_health.rs src-tauri/src/services/proxy/routing_affinity.rs src-tauri/src/services/proxy/routing_probe.rs src-tauri/src/services/proxy/routing_snapshot.rs src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/features/routing/RoutingPage.tsx src/features/routing/LocalRoutingStatusTab.tsx src/features/routing/LocalRoutingEditTab.tsx src/features/routing/LocalRoutingCandidateRow.tsx src/features/routing/RouteExplanationPanel.tsx src/lib/api/localRouting.ts src/lib/queries/localRoutingQueries.ts src/lib/types/localRouting.ts src/lib/types/routing.ts scripts/local-routing-query-service.test.mjs scripts/local-routing-page-layout.test.mjs scripts/local-routing-reorder.test.mjs scripts/local-routing-redaction.test.mjs scripts/local-routing-explanation.test.mjs
git commit -m "test: verify local routing upgrade"
```

If there are no fixes, do not create an empty commit.

## Final Acceptance Checklist

- [ ] The local routing page is status-first with `状态 / 编辑`.
- [ ] Dragging edit rows changes global `routing_order`.
- [ ] No weight UI exists.
- [ ] No page-level `保存策略` button exists.
- [ ] Existing `simulate_route` and legacy policies still work during migration.
- [ ] `cost_stable_first` is implemented with tests before becoming backend default.
- [ ] One ambiguous failure does not kill a key.
- [ ] Hard auth/balance/model failures are scoped correctly.
- [ ] Streamed requests are not replayed after first output.
- [ ] Stream request logs finalize after downstream forwarding.
- [ ] `/v1/models` does not update route affinity.
- [ ] Switch-before-confirm skips depleted/cooling/auth-error keys.
- [ ] Local routing workspace/log/explanation objects do not expose full secrets or raw request bodies.
- [ ] `pnpm.cmd build` passes.
- [ ] `cargo check --manifest-path .\src-tauri\Cargo.toml` passes.
