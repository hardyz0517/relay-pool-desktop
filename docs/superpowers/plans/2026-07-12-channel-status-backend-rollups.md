# Channel Status Backend Rollups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move channel-status 24h/7d availability and timeline statistics from frontend detail slicing to a reliable backend summary API with bounded query cost.

**Architecture:** Keep raw `channel_monitor_runs` as the source of truth. Add backend rollup/query helpers that return per-monitor window summaries and bounded timelines, then let `ChannelStatusTab` consume those summaries through `src/lib/api/channelMonitors.ts` and `src/lib/queries/channelQueries.ts`. The backend owns window math, limits, indexes, and fallback behavior; the frontend only renders the returned facts.

**Tech Stack:** Tauri 2 commands, Rust services over SQLite/rusqlite, React + TypeScript view models, existing Node contract scripts, `cargo test`, `cargo check`, `pnpm build`.

---

## File Structure

- Modify: `src-tauri/src/models/shared_capabilities.rs`
  - Add serializable channel status summary DTOs: per window availability, latency, last check, timeline points.
- Modify: `src-tauri/src/services/database.rs`
  - Add indexed aggregate queries over `channel_monitor_runs`.
  - Keep raw-run APIs unchanged for monitoring/admin views.
- Modify: `src-tauri/src/services/shared_capabilities.rs`
  - Build channel status summaries from monitor rows and aggregate query results.
- Modify: `src-tauri/src/commands/mod.rs`
  - Add or extend a command for channel-status summaries.
- Modify: `src/lib/types/channelMonitors.ts`
  - Add TypeScript types matching the backend DTOs.
- Modify: `src/lib/api/channelMonitors.ts`
  - Expose the typed API wrapper; UI must not call `invoke` directly.
- Modify: `src/lib/queries/channelQueries.ts`
  - Load backend summaries for the status page while leaving monitoring-page loading light.
- Modify: `src/features/channels/channelStatusViewModel.ts`
  - Convert backend summaries into card-ready view data.
- Modify: `src/features/channels/ChannelStatusTab.tsx`
  - Render backend status summary instead of deriving window statistics from loaded raw runs.
- Test: `scripts/channel-status-backend-rollup-contract.test.mjs`
  - Source-level contract for command/API/query boundaries.
- Test: `scripts/channel-status-view-model.test.mjs`
  - View-model regression for window switching behavior.
- Test: Rust unit tests in `src-tauri/src/services/database.rs`
  - Database-level proof for 24h/7d aggregation, limits, and empty-window behavior.

---

### Task 1: Define Backend DTOs and API Boundary

**Files:**
- Modify: `src-tauri/src/models/shared_capabilities.rs`
- Modify: `src/lib/types/channelMonitors.ts`
- Create: `scripts/channel-status-backend-rollup-contract.test.mjs`

- [ ] **Step 1: Write the failing source contract test**

Create `scripts/channel-status-backend-rollup-contract.test.mjs`:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const rustModels = await readFile("src-tauri/src/models/shared_capabilities.rs", "utf8");
const tsTypes = await readFile("src/lib/types/channelMonitors.ts", "utf8");
const apiSource = await readFile("src/lib/api/channelMonitors.ts", "utf8");
const querySource = await readFile("src/lib/queries/channelQueries.ts", "utf8");
const statusSource = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");

assert.ok(
  rustModels.includes("pub struct ChannelStatusWindowSummary") &&
    rustModels.includes("pub struct ChannelStatusTimelinePoint") &&
    rustModels.includes("pub struct ChannelStatusSummary"),
  "Rust shared capability models should expose channel status summary DTOs",
);

assert.ok(
  tsTypes.includes("export type ChannelStatusWindowSummary") &&
    tsTypes.includes("export type ChannelStatusTimelinePoint") &&
    tsTypes.includes("export type ChannelStatusSummary"),
  "TypeScript channel monitor types should mirror backend status summary DTOs",
);

assert.ok(
  apiSource.includes("listChannelStatusSummaries") &&
    apiSource.includes('invoke<ChannelStatusSummary[]>("list_channel_status_summaries"'),
  "channel monitor API should expose a typed channel status summary command wrapper",
);

assert.ok(
  querySource.includes("listChannelStatusSummaries()") &&
    querySource.includes("channelStatusSummaries: ChannelStatusSummary[]"),
  "channel status query service should load backend summaries",
);

assert.ok(
  !statusSource.includes("list_channel_status_summaries") &&
    !statusSource.includes("@tauri-apps/api/core"),
  "ChannelStatusTab must consume query/API helpers instead of invoking Tauri directly",
);
```

- [ ] **Step 2: Run the contract test and verify RED**

Run:

```powershell
node scripts/channel-status-backend-rollup-contract.test.mjs
```

Expected: FAIL with `Rust shared capability models should expose channel status summary DTOs`.

- [ ] **Step 3: Add Rust DTOs**

In `src-tauri/src/models/shared_capabilities.rs`, add after `ChannelMonitorSummary`:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusTimelinePoint {
    pub status: String,
    pub latency_ms: Option<i64>,
    pub endpoint_ping_ms: Option<i64>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusWindowSummary {
    pub window: String,
    pub total_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub warning_count: i64,
    pub availability_percent: Option<f64>,
    pub avg_latency_ms: Option<i64>,
    pub avg_endpoint_ping_ms: Option<i64>,
    pub last_checked_at: Option<String>,
    pub latest_status: Option<String>,
    pub latest_error_message: Option<String>,
    pub timeline: Vec<ChannelStatusTimelinePoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusSummary {
    pub monitor: ChannelMonitor,
    pub recent: ChannelStatusWindowSummary,
    pub last24h: ChannelStatusWindowSummary,
    pub last7d: ChannelStatusWindowSummary,
}
```

- [ ] **Step 4: Add matching TypeScript types**

In `src/lib/types/channelMonitors.ts`, add after `ChannelMonitorSummary`:

```ts
export type ChannelStatusTimelinePoint = {
  status: ChannelMonitorRunStatus;
  latencyMs: number | null;
  endpointPingMs: number | null;
  checkedAt: string;
};

export type ChannelStatusWindowSummary = {
  window: "recent" | "24h" | "7d";
  totalCount: number;
  successCount: number;
  failureCount: number;
  warningCount: number;
  availabilityPercent: number | null;
  avgLatencyMs: number | null;
  avgEndpointPingMs: number | null;
  lastCheckedAt: string | null;
  latestStatus: ChannelMonitorRunStatus | null;
  latestErrorMessage: string | null;
  timeline: ChannelStatusTimelinePoint[];
};

export type ChannelStatusSummary = {
  monitor: ChannelMonitor;
  recent: ChannelStatusWindowSummary;
  last24h: ChannelStatusWindowSummary;
  last7d: ChannelStatusWindowSummary;
};
```

- [ ] **Step 5: Run the contract test and keep expected RED**

Run:

```powershell
node scripts/channel-status-backend-rollup-contract.test.mjs
```

Expected: FAIL with `channel monitor API should expose a typed channel status summary command wrapper`.

---

### Task 2: Add Bounded SQLite Summary Queries

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/shared_capabilities.rs`

- [ ] **Step 1: Add failing Rust database test**

In `src-tauri/src/services/database.rs`, inside the existing `#[cfg(test)] mod tests`, add:

```rust
#[test]
fn channel_status_summaries_aggregate_recent_24h_and_7d_windows() {
    let database = test_database();
    let monitor = test_channel_monitor(&database, "window summary");
    let now = now_millis_for_services() as i64;

    for (index, (age_ms, status, latency)) in [
        (2 * 60 * 60 * 1000, "success", 100_i64),
        (30 * 60 * 60 * 1000, "failed", 300_i64),
        (8 * 24 * 60 * 60 * 1000, "warning", 500_i64),
    ]
    .into_iter()
    .enumerate()
    {
        let started_at = (now - age_ms).to_string();
        database
            .insert_channel_monitor_run(CreateChannelMonitorRunInput {
                monitor_id: monitor.id.clone(),
                template_id: monitor.template_id.clone(),
                station_id: monitor.station_id.clone(),
                station_key_id: monitor.station_key_id.clone(),
                status: status.to_string(),
                started_at: started_at.clone(),
                finished_at: Some(started_at),
                duration_ms: Some(latency),
                http_status: Some(if status == "failed" { 500 } else { 200 }),
                latency_ms: Some(latency),
                response_model: Some(format!("model-{index}")),
                fallback_model: None,
                error_message: (status == "failed").then(|| "boom".to_string()),
            })
            .expect("insert run");
    }

    let summaries = database
        .list_channel_status_summaries()
        .expect("status summaries");
    let summary = summaries
        .iter()
        .find(|summary| summary.monitor.id == monitor.id)
        .expect("monitor summary");

    assert_eq!(summary.recent.total_count, 3);
    assert_eq!(summary.last24h.total_count, 1);
    assert_eq!(summary.last24h.success_count, 1);
    assert_eq!(summary.last24h.availability_percent, Some(100.0));
    assert_eq!(summary.last7d.total_count, 2);
    assert_eq!(summary.last7d.success_count, 1);
    assert_eq!(summary.last7d.failure_count, 1);
    assert_eq!(summary.last7d.availability_percent, Some(50.0));
    assert_eq!(summary.last7d.latest_error_message.as_deref(), Some("boom"));
}
```

- [ ] **Step 2: Run Rust test and verify RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml channel_status_summaries_aggregate_recent_24h_and_7d_windows -- --nocapture
```

Expected: FAIL because `list_channel_status_summaries` does not exist.

- [ ] **Step 3: Add database method skeleton**

In `impl AppDatabase` in `src-tauri/src/services/database.rs`, add:

```rust
pub fn list_channel_status_summaries(&self) -> Result<Vec<ChannelStatusSummary>, String> {
    let monitors = self.list_channel_monitors()?;
    crate::services::shared_capabilities::channel_status_summaries_from_database(self, monitors)
}
```

Also add `ChannelStatusSummary` to the existing `use crate::models::shared_capabilities::{...};` import list.

- [ ] **Step 4: Add focused aggregate helper types and query**

In `src-tauri/src/services/database.rs`, near the channel-monitor run query helpers, add:

```rust
#[derive(Debug, Clone)]
pub struct ChannelStatusWindowFacts {
    pub monitor_id: String,
    pub total_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub warning_count: i64,
    pub avg_latency_ms: Option<i64>,
    pub avg_endpoint_ping_ms: Option<i64>,
    pub last_checked_at: Option<String>,
    pub latest_status: Option<String>,
    pub latest_error_message: Option<String>,
    pub timeline: Vec<ChannelStatusTimelinePoint>,
}
```

Then add:

```rust
pub fn channel_status_window_facts(
    &self,
    monitor_id: &str,
    since_ms: Option<i64>,
    timeline_limit: usize,
) -> Result<ChannelStatusWindowFacts, String> {
    let connection = self.connection()?;
    channel_status_window_facts_from_connection(&connection, monitor_id, since_ms, timeline_limit)
}
```

- [ ] **Step 5: Implement bounded query function**

Add below `list_channel_monitor_runs_for_summary_from_connection`:

```rust
const CHANNEL_STATUS_TIMELINE_LIMIT: usize = 60;

fn channel_status_window_facts_from_connection(
    connection: &Connection,
    monitor_id: &str,
    since_ms: Option<i64>,
    timeline_limit: usize,
) -> Result<ChannelStatusWindowFacts, String> {
    channel_monitor_by_id(connection, monitor_id)?;
    let timeline_limit = timeline_limit.clamp(1, CHANNEL_STATUS_TIMELINE_LIMIT) as i64;

    let where_since = if since_ms.is_some() {
        " AND CAST(started_at AS INTEGER) >= ?2"
    } else {
        ""
    };
    let sql = format!(
        "SELECT
            COUNT(*) AS total_count,
            SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) AS success_count,
            SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failure_count,
            SUM(CASE WHEN status IN ('warning', 'skipped') THEN 1 ELSE 0 END) AS warning_count,
            CAST(AVG(latency_ms) AS INTEGER) AS avg_latency_ms
         FROM channel_monitor_runs
         WHERE monitor_id = ?1{where_since}"
    );

    let (total_count, success_count, failure_count, warning_count, avg_latency_ms) =
        if let Some(since_ms) = since_ms {
            connection.query_row(
                &sql,
                params![monitor_id, since_ms],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
        } else {
            connection.query_row(
                &sql,
                params![monitor_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
        }
        .map_err(|error| format!("聚合渠道状态窗口失败: {error}"))?;

    let timeline = channel_status_timeline_from_connection(
        connection,
        monitor_id,
        since_ms,
        timeline_limit,
    )?;
    let latest = timeline.first();

    Ok(ChannelStatusWindowFacts {
        monitor_id: monitor_id.to_string(),
        total_count,
        success_count,
        failure_count,
        warning_count,
        avg_latency_ms,
        avg_endpoint_ping_ms: None,
        last_checked_at: latest.map(|point| point.checked_at.clone()),
        latest_status: latest.map(|point| point.status.clone()),
        latest_error_message: latest.and_then(|_| latest_channel_status_error(connection, monitor_id, since_ms).ok().flatten()),
        timeline,
    })
}
```

- [ ] **Step 6: Implement bounded timeline query**

Add:

```rust
fn channel_status_timeline_from_connection(
    connection: &Connection,
    monitor_id: &str,
    since_ms: Option<i64>,
    limit: i64,
) -> Result<Vec<ChannelStatusTimelinePoint>, String> {
    let where_since = if since_ms.is_some() {
        " AND CAST(started_at AS INTEGER) >= ?2"
    } else {
        ""
    };
    let sql = format!(
        "SELECT status, latency_ms, started_at
           FROM channel_monitor_runs
          WHERE monitor_id = ?1{where_since}
          ORDER BY CAST(started_at AS INTEGER) DESC
          LIMIT ?3"
    );

    let map_row = |row: &rusqlite::Row<'_>| {
        Ok(ChannelStatusTimelinePoint {
            status: row.get(0)?,
            latency_ms: row.get(1)?,
            endpoint_ping_ms: None,
            checked_at: row.get(2)?,
        })
    };

    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("读取渠道状态时间线失败: {error}"))?;
    let rows = if let Some(since_ms) = since_ms {
        statement.query_map(params![monitor_id, since_ms, limit], map_row)
    } else {
        statement.query_map(params![monitor_id, i64::MIN, limit], map_row)
    }
    .map_err(|error| format!("查询渠道状态时间线失败: {error}"))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析渠道状态时间线失败: {error}"))
}
```

- [ ] **Step 7: Add shared capability builder**

In `src-tauri/src/services/shared_capabilities.rs`, add:

```rust
pub fn channel_status_summaries_from_database(
    database: &AppDatabase,
    monitors: Vec<ChannelMonitor>,
) -> Result<Vec<ChannelStatusSummary>, String> {
    let now = crate::services::database::now_millis_for_services() as i64;
    let last24h_since = now - 24 * 60 * 60 * 1000;
    let last7d_since = now - 7 * 24 * 60 * 60 * 1000;

    monitors
        .into_iter()
        .map(|monitor| {
            let recent = database.channel_status_window_facts(&monitor.id, None, 60)?;
            let last24h = database.channel_status_window_facts(&monitor.id, Some(last24h_since), 60)?;
            let last7d = database.channel_status_window_facts(&monitor.id, Some(last7d_since), 60)?;
            Ok(ChannelStatusSummary {
                monitor,
                recent: window_summary("recent", recent),
                last24h: window_summary("24h", last24h),
                last7d: window_summary("7d", last7d),
            })
        })
        .collect()
}

fn window_summary(window: &str, facts: ChannelStatusWindowFacts) -> ChannelStatusWindowSummary {
    let availability_percent = if facts.total_count == 0 {
        None
    } else {
        Some((facts.success_count as f64 / facts.total_count as f64) * 100.0)
    };
    ChannelStatusWindowSummary {
        window: window.to_string(),
        total_count: facts.total_count,
        success_count: facts.success_count,
        failure_count: facts.failure_count,
        warning_count: facts.warning_count,
        availability_percent,
        avg_latency_ms: facts.avg_latency_ms,
        avg_endpoint_ping_ms: facts.avg_endpoint_ping_ms,
        last_checked_at: facts.last_checked_at,
        latest_status: facts.latest_status,
        latest_error_message: facts.latest_error_message,
        timeline: facts.timeline,
    }
}
```

- [ ] **Step 8: Run Rust test and verify GREEN**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml channel_status_summaries_aggregate_recent_24h_and_7d_windows -- --nocapture
```

Expected: PASS.

---

### Task 3: Expose Tauri Command Through Typed API

**Files:**
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api/channelMonitors.ts`
- Modify: `src/lib/queries/channelQueries.ts`

- [ ] **Step 1: Keep contract test RED at API boundary**

Run:

```powershell
node scripts/channel-status-backend-rollup-contract.test.mjs
```

Expected: FAIL with API wrapper or query-service assertion.

- [ ] **Step 2: Add Tauri command**

In `src-tauri/src/commands/mod.rs`, import `ChannelStatusSummary` and add:

```rust
#[tauri::command]
pub fn list_channel_status_summaries(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ChannelStatusSummary>, String> {
    database.list_channel_status_summaries()
}
```

- [ ] **Step 3: Register command**

In `src-tauri/src/lib.rs`, add `commands::list_channel_status_summaries` to the existing invoke handler list next to `list_channel_monitor_summaries`.

- [ ] **Step 4: Add typed frontend API**

In `src/lib/api/channelMonitors.ts`, add `ChannelStatusSummary` to imports and add:

```ts
export function listChannelStatusSummaries() {
  return invoke<ChannelStatusSummary[]>("list_channel_status_summaries").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryMonitors.map((monitor) => ({
        monitor: copyMonitor(monitor),
        recent: buildMemoryStatusWindow(monitor.id, "recent"),
        last24h: buildMemoryStatusWindow(monitor.id, "24h"),
        last7d: buildMemoryStatusWindow(monitor.id, "7d"),
      }));
    }
    throw error;
  });
}
```

Add helper:

```ts
function buildMemoryStatusWindow(
  monitorId: string,
  window: "recent" | "24h" | "7d",
): ChannelStatusWindowSummary {
  const now = Date.now();
  const cutoff =
    window === "recent" ? null : now - (window === "24h" ? 24 * 60 * 60 * 1000 : 7 * 24 * 60 * 60 * 1000);
  const runs = (memoryRuns.get(monitorId) ?? [])
    .filter((run) => cutoff === null || toTime(run.startedAt) >= cutoff)
    .sort((left, right) => toTime(right.startedAt) - toTime(left.startedAt));
  const successCount = runs.filter((run) => run.status === "success").length;
  const failureCount = runs.filter((run) => run.status === "failed").length;
  const warningCount = runs.filter((run) => run.status === "warning" || run.status === "skipped").length;
  const avgLatencyMs = averageNullable(runs.map((run) => run.latencyMs ?? run.durationMs));
  return {
    window,
    totalCount: runs.length,
    successCount,
    failureCount,
    warningCount,
    availabilityPercent: runs.length === 0 ? null : (successCount / runs.length) * 100,
    avgLatencyMs,
    avgEndpointPingMs: null,
    lastCheckedAt: runs[0]?.finishedAt ?? runs[0]?.startedAt ?? null,
    latestStatus: runs[0]?.status ?? null,
    latestErrorMessage: runs.find((run) => run.errorMessage)?.errorMessage ?? null,
    timeline: runs.slice(0, 60).map((run) => ({
      status: run.status,
      latencyMs: run.latencyMs ?? run.durationMs,
      endpointPingMs: null,
      checkedAt: run.finishedAt ?? run.startedAt,
    })),
  };
}

function averageNullable(values: Array<number | null | undefined>) {
  const present = values.filter((value): value is number => typeof value === "number");
  if (present.length === 0) return null;
  return Math.round(present.reduce((sum, value) => sum + value, 0) / present.length);
}
```

- [ ] **Step 5: Wire query service**

In `src/lib/queries/channelQueries.ts`, import `listChannelStatusSummaries` and type `ChannelStatusSummary`; update `ChannelStatusWorkspace`:

```ts
export type ChannelStatusWorkspace = {
  keyPoolItems: KeyPoolItem[];
  requestLogs: RequestLog[];
  stationKeyHealth: StationKeyHealth[];
  monitorSummaries: ChannelMonitorSummary[];
  channelStatusSummaries: ChannelStatusSummary[];
};
```

Update loader:

```ts
const [keyPoolItems, requestLogs, stationKeyHealth, monitorSummaries, channelStatusSummaries] = await Promise.all([
  listKeyPoolItems(),
  listRequestLogs(),
  listStationKeyHealth(),
  listChannelMonitorSummaries(),
  listChannelStatusSummaries(),
]);
```

Return `channelStatusSummaries`.

- [ ] **Step 6: Run contract test and verify GREEN**

Run:

```powershell
node scripts/channel-status-backend-rollup-contract.test.mjs
```

Expected: PASS.

---

### Task 4: Move Card Projection to Backend Summaries

**Files:**
- Modify: `src/features/channels/channelStatusViewModel.ts`
- Modify: `src/features/channels/ChannelStatusTab.tsx`
- Modify: `scripts/channel-status-view-model.test.mjs`

- [ ] **Step 1: Add failing view-model test**

In `scripts/channel-status-view-model.test.mjs`, import `selectChannelStatusWindowSummary` and add:

```js
const backendSummary = {
  recent: { window: "recent", totalCount: 3, availabilityPercent: 66.67 },
  last24h: { window: "24h", totalCount: 1, availabilityPercent: 100 },
  last7d: { window: "7d", totalCount: 2, availabilityPercent: 50 },
};

assert.equal(
  selectChannelStatusWindowSummary(backendSummary, "24h").availabilityPercent,
  100,
  "24h cards should use backend 24h summary instead of frontend raw-run slicing",
);
assert.equal(
  selectChannelStatusWindowSummary(backendSummary, "7d").availabilityPercent,
  50,
  "7d cards should use backend 7d summary",
);
```

- [ ] **Step 2: Run test and verify RED**

Run:

```powershell
node scripts/channel-status-view-model.test.mjs
```

Expected: FAIL because `selectChannelStatusWindowSummary` is not exported.

- [ ] **Step 3: Add selector helper**

In `src/features/channels/channelStatusViewModel.ts`, add:

```ts
import type { ChannelStatusSummary, ChannelStatusWindowSummary } from "@/lib/types/channelMonitors";
```

Add:

```ts
export function selectChannelStatusWindowSummary(
  summary: Pick<ChannelStatusSummary, "recent" | "last24h" | "last7d">,
  timeWindow: ChannelWindow,
): ChannelStatusWindowSummary {
  if (timeWindow === "24h") return summary.last24h;
  if (timeWindow === "7d") return summary.last7d;
  return summary.recent;
}
```

- [ ] **Step 4: Update `ChannelStatusTab` data state**

In `ChannelStatusTab.tsx`, import `ChannelStatusSummary`, add:

```ts
const [statusSummaries, setStatusSummaries] = useState<ChannelStatusSummary[]>([]);
```

In `refresh`, set:

```ts
setStatusSummaries(workspace.channelStatusSummaries);
```

- [ ] **Step 5: Feed summaries into `buildChannels`**

Change `buildChannels` signature:

```ts
function buildChannels(
  keys: KeyPoolItem[],
  logs: RequestLog[],
  health: StationKeyHealth[],
  monitors: ChannelMonitor[],
  statusSummaries: ChannelStatusSummary[],
  timeWindow: ChannelWindow,
): ChannelHealth[] {
```

Inside the function:

```ts
const summaryByMonitor = new Map(statusSummaries.map((summary) => [summary.monitor.id, summary] as const));
```

For each monitor:

```ts
const backendSummary = summaryByMonitor.get(monitor.id);
const windowSummary = backendSummary ? selectChannelStatusWindowSummary(backendSummary, timeWindow) : null;
```

Use `windowSummary` for:

```ts
const availabilityPercent = windowSummary?.availabilityPercent ?? null;
const latencyMs = windowSummary?.avgLatencyMs ?? null;
const endpointPingMs = windowSummary?.avgEndpointPingMs ?? key.endpointPingMs;
const lastCheckedAt = windowSummary?.lastCheckedAt ?? null;
const lastError = windowSummary?.latestErrorMessage ?? null;
const successCount = windowSummary?.successCount ?? 0;
const failureCount = windowSummary?.failureCount ?? 0;
const recentOutcomes = windowSummary
  ? buildMonitorTimelineOutcomes(windowSummary.timeline)
  : buildRecentOutcomes(recentLogs, timeWindow === "recent" ? keyHealth : null);
```

- [ ] **Step 6: Add timeline outcome helper**

In `channelStatusViewModel.ts`, add:

```ts
export function buildMonitorTimelineOutcomes(
  timeline: Array<Pick<ChannelStatusTimelinePoint, "status">>,
) {
  return padRecentOutcomes([...timeline].reverse().slice(-60).map(monitorRunToOutcome));
}
```

- [ ] **Step 7: Run view-model and card contract tests**

Run:

```powershell
node scripts/channel-status-view-model.test.mjs
node scripts/channel-status-card-layout.test.mjs
```

Expected: PASS.

---

### Task 5: Reliability Hardening

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `scripts/channel-status-backend-rollup-contract.test.mjs`

- [ ] **Step 1: Add source-contract checks for query bounds**

Append to `scripts/channel-status-backend-rollup-contract.test.mjs`:

```js
const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");

assert.ok(
  databaseSource.includes("CHANNEL_STATUS_TIMELINE_LIMIT") &&
    databaseSource.includes(".clamp(1, CHANNEL_STATUS_TIMELINE_LIMIT)") &&
    databaseSource.includes("idx_channel_monitor_runs_monitor_started"),
  "channel status backend summaries should be bounded and indexed",
);
```

- [ ] **Step 2: Run contract test**

Run:

```powershell
node scripts/channel-status-backend-rollup-contract.test.mjs
```

Expected: PASS if previous tasks already added bounded indexed queries.

- [ ] **Step 3: Add empty-window Rust test**

In `database.rs` tests, add:

```rust
#[test]
fn channel_status_summaries_return_null_availability_for_empty_window() {
    let database = test_database();
    let monitor = test_channel_monitor(&database, "empty window");

    let summaries = database
        .list_channel_status_summaries()
        .expect("status summaries");
    let summary = summaries
        .iter()
        .find(|summary| summary.monitor.id == monitor.id)
        .expect("monitor summary");

    assert_eq!(summary.last24h.total_count, 0);
    assert_eq!(summary.last24h.availability_percent, None);
    assert!(summary.last24h.timeline.is_empty());
}
```

- [ ] **Step 4: Run empty-window test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml channel_status_summaries_return_null_availability_for_empty_window -- --nocapture
```

Expected: PASS.

---

### Task 6: Final Verification and Commit

**Files:**
- Verify only.
- Stage exact paths only if committing.

- [ ] **Step 1: Run focused Node tests**

Run:

```powershell
node scripts/channel-status-backend-rollup-contract.test.mjs
node scripts/channel-status-view-model.test.mjs
node scripts/channel-query-service.test.mjs
node scripts/channel-status-card-layout.test.mjs
node scripts/shared-capabilities-contract.test.mjs
```

Expected: all PASS.

- [ ] **Step 2: Run frontend build**

Run:

```powershell
pnpm.cmd build
```

Expected: PASS. Existing Vite chunk-size warning is acceptable if unchanged.

- [ ] **Step 3: Run Rust checks**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml channel_status_summaries -- --nocapture
cargo check --manifest-path .\src-tauri\Cargo.toml; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
```

Expected: tests PASS and `cargo check` exit code 0. Existing dead-code warnings are acceptable if unchanged.

- [ ] **Step 4: Review working tree**

Run:

```powershell
git status --short
git diff --check -- scripts/channel-status-backend-rollup-contract.test.mjs scripts/channel-status-view-model.test.mjs scripts/channel-query-service.test.mjs scripts/channel-status-card-layout.test.mjs src/features/channels/ChannelStatusTab.tsx src/features/channels/channelStatusViewModel.ts src/lib/api/channelMonitors.ts src/lib/queries/channelQueries.ts src/lib/types/channelMonitors.ts src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/models/shared_capabilities.rs src-tauri/src/services/database.rs src-tauri/src/services/shared_capabilities.rs
```

Expected: only intended paths have changes; no whitespace errors.

- [ ] **Step 5: Optional exact-path commit**

Only commit if requested:

```powershell
git add -- scripts/channel-status-backend-rollup-contract.test.mjs scripts/channel-status-view-model.test.mjs scripts/channel-query-service.test.mjs scripts/channel-status-card-layout.test.mjs src/features/channels/ChannelStatusTab.tsx src/features/channels/channelStatusViewModel.ts src/lib/api/channelMonitors.ts src/lib/queries/channelQueries.ts src/lib/types/channelMonitors.ts src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/models/shared_capabilities.rs src-tauri/src/services/database.rs src-tauri/src/services/shared_capabilities.rs
git diff --cached --name-only
git commit -m "feat: add backend channel status summaries"
```

Expected staged paths exactly match this task. Do not push unless explicitly requested.

---

## Reliability / Maintainability / Extensibility Notes

- Reliability: raw `channel_monitor_runs` remains authoritative; backend summaries are computed from persisted facts, not frontend cache state.
- Reliability: empty windows return `availabilityPercent: null`, not cumulative fallback values that make filters look fake.
- Maintainability: command access stays behind `src/lib/api/channelMonitors.ts`; React components consume query services and view models.
- Maintainability: monitoring/admin pages keep raw recent runs; status page gets summary facts. These are separate read models.
- Extensibility: this first backend pass supports fixed windows (`recent`, `24h`, `7d`). If later we need 30d/monthly/high-frequency exact history, add daily/hourly rollup tables rather than increasing timeline limits.
- Extensibility: Sub2API-style preaggregation can be added later as `channel_monitor_hourly_rollups` and `channel_monitor_daily_rollups`; the frontend contract should not need to change if `ChannelStatusSummary` stays stable.

