# Local Routing Status Clarity And Cooldown Countdown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the Local Routing status tab around one reliable runtime status, one historical latest-decision record, and one scheduler-backed candidate preview, with a live cooldown countdown that stays correct across stale timestamps, hidden pages, and expiry.

**Architecture:** Keep `RoutingPage` as the orchestration boundary for queries, proxy start/stop commands, page visibility, and the single shared clock. Make the Rust routing snapshot reuse scheduler eligibility and expose stable preview decisions so summary counts and row states cannot disagree. Keep formatting and countdown math in a pure frontend view-model module, while dedicated status-only components render a compact desktop-tool layout without changing the drag-sort editor.

**Tech Stack:** Tauri 2, Rust, React 18, TypeScript 5.7, TanStack Query, Tailwind CSS, Node 24 type stripping, existing UI primitives

---

## Scope And File Responsibilities

- `src-tauri/src/services/proxy/scheduler/explanation.rs`: expose the existing stable scheduler rejection-code formatter for reuse outside the explanation surface.
- `src-tauri/src/services/proxy/routing_health.rs`: provide the shared now-aware cooldown/offline hard gate used by both runtime routing and status projection.
- `src-tauri/src/services/proxy/router.rs`: replace the fixed timestamp threshold with the shared health hard gate when projecting scheduler candidates.
- `src-tauri/src/services/proxy/routing_types.rs`: define the serialized preview eligibility fields and unambiguous summary counts.
- `src-tauri/src/services/proxy/routing_snapshot.rs`: project database candidates into scheduler eligibility, then derive row and summary preview facts from the same decisions.
- `src/lib/types/localRouting.ts`: mirror the Rust command contract exactly.
- `src/features/routing/localRoutingStatusViewModel.ts`: pure copy normalization, latest-decision presentation, and cooldown countdown math.
- `src/features/routing/useCooldownClock.ts`: own one visibility-aware one-second timer and one expiry notification per unique deadline.
- `src/features/routing/LocalRoutingStatusCandidateRow.tsx`: render the read-only status-page candidate grid. It must not own timers or drag behavior.
- `src/features/routing/LocalRoutingStatusTab.tsx`: render the status band, recent decision, and candidate preview sections.
- `src/features/routing/RoutingPage.tsx`: own proxy actions, the shared clock, expiry revalidation, and query invalidation.
- `src/features/routing/LocalRoutingCandidateRow.tsx`: remain the edit-tab drag/sync row; do not add status-only branches to it.
- `scripts/local-routing-status-view-model.test.mjs`: runtime tests for pure TypeScript presentation rules.
- `scripts/local-routing-cooldown-display.test.mjs`: source contract for one shared timer and backend-authoritative cooldown state.
- `scripts/local-routing-page-layout.test.mjs`: source contract for the new status-page information hierarchy.
- `scripts/local-routing-automatic-settings.test.mjs`: update the Rust/TypeScript workspace contract assertions for renamed summary fields.

## Fixed Product Semantics

- “运行中 / 未启动” comes only from `workspace.proxyStatus.running`.
- The page never uses “当前密钥”. `latestDecision` is always presented as “最近一次路由”; when the proxy is stopped it receives the “历史记录” badge.
- Candidate eligibility is a generic `chat_completions` preview with no model, tools, vision, reasoning, or streaming requirement. The section title remains “候选顺序预览” so it does not promise that every future request has identical gates.
- `previewEligibleCandidateCount` is counted from the same scheduler-backed row decision returned to the UI. It is never recomputed from unrelated summary fields in React.
- `healthState` is authoritative for whether a row is cooling down. `cooldownUntil` only supplies the remaining duration.
- Runtime routing and the status preview must call the same `health_is_blocked(health, now_ms)` helper. A future cooldown deadline and an explicit offline error block both paths; an expired, missing, or invalid deadline does not.
- If `healthState !== "cooldown"`, display “无” even when a stale future `cooldownUntil` exists.
- If `healthState === "cooldown"` and the deadline is valid and in the future, display a live `MM:SS` or `H:MM:SS` countdown.
- If the backend says cooldown but the deadline is missing or invalid, display “冷却中”. If the countdown reaches zero before refreshed backend data arrives, display “即将结束” and invalidate the workspace query once for that exact key/deadline pair.
- No negative countdown, repeated expiry requests, per-row intervals, hidden-page ticking, raw scheduler codes, or raw values such as `low_confidence`, `normal`, and `sub2api_groups_rates` may be visible.

---

### Task 1: Lock The New Frontend Contract With RED Tests

**Files:**
- Create: `scripts/local-routing-status-view-model.test.mjs`
- Modify: `scripts/local-routing-cooldown-display.test.mjs`
- Modify: `scripts/local-routing-page-layout.test.mjs`

- [ ] **Step 1: Add runtime tests for countdown and latest-decision semantics**

Create `scripts/local-routing-status-view-model.test.mjs` and import the pure TypeScript module through Node 24 type stripping:

```js
import assert from "node:assert/strict";
import {
  buildCooldownDisplay,
  buildLatestDecisionDisplay,
  formatPreviewRejectReason,
} from "../src/features/routing/localRoutingStatusViewModel.ts";

assert.deepEqual(
  buildCooldownDisplay("ready", 120_000, 60_000),
  { active: false, label: "无", remainingSeconds: null },
  "healthState must override a stale future cooldown timestamp",
);

assert.deepEqual(
  buildCooldownDisplay("cooldown", 125_000, 60_000),
  { active: true, label: "01:05", remainingSeconds: 65 },
);
assert.equal(buildCooldownDisplay("cooldown", 3_721_000, 60_000).label, "1:01:01");
assert.equal(buildCooldownDisplay("cooldown", null, 60_000).label, "冷却中");
assert.equal(buildCooldownDisplay("cooldown", Number.NaN, 60_000).label, "冷却中");
assert.equal(buildCooldownDisplay("cooldown", 59_999, 60_000).label, "即将结束");

assert.equal(formatPreviewRejectReason("routing_group_mismatch"), "分组不匹配");
assert.equal(formatPreviewRejectReason("multiplier_evidence_low_confidence"), "费率可信度不足");
assert.equal(formatPreviewRejectReason("routing_multiplier_limit_not_configured"), "倍率上限未设置");

const latestDecision = {
  id: "decision-1",
  decidedAt: "2026-07-13T01:29:38.000Z",
  endpoint: "chat_completions",
  model: null,
  selectedStationKeyId: "key-1",
  selectedStationId: "station-1",
  selectedStationName: "AI鸡神 / 鸡神",
  policy: "cost_stable_first",
  status: "selected",
  reason: "selected",
  fallbackCount: 0,
};

assert.equal(buildLatestDecisionDisplay(false, latestDecision).badge, "历史记录");
assert.equal(buildLatestDecisionDisplay(true, latestDecision).badge, "已选中");
assert.equal(buildLatestDecisionDisplay(true, null).title, "尚无路由记录");

console.log("local routing status view model ok");
```

- [ ] **Step 2: Replace the old static cooldown contract**

Update `scripts/local-routing-cooldown-display.test.mjs` so it requires a shared clock in `RoutingPage`, a `nowMs` prop on the status tab/row, and no timer inside any candidate row:

```js
assert.match(routingPageSource, /useCooldownClock/);
assert.match(routingPageSource, /refreshEnabled\s*&&\s*activeTab\s*===\s*"status"/);
assert.match(statusTabSource, /nowMs=\{nowMs\}/);
assert.match(statusRowSource, /buildCooldownDisplay/);
assert.doesNotMatch(statusRowSource + editRowSource, /setInterval|setTimeout/);
assert.doesNotMatch(statusRowSource, /candidate\.cooldownUntil\s*\?\s*"/);
assert.match(clockSource, /window\.setInterval/);
assert.match(clockSource, /window\.clearInterval/);
assert.match(clockSource, /notifiedDeadlinesRef/);
```

- [ ] **Step 3: Rewrite the layout contract around the approved hierarchy**

Update the status assertions in `scripts/local-routing-page-layout.test.mjs`:

```js
assertIncludes(statusTab, "本地路由状态", "LocalRoutingStatusTab");
assertIncludes(statusTab, "最近一次路由", "LocalRoutingStatusTab");
assertIncludes(statusTab, "候选顺序预览", "LocalRoutingStatusTab");
assertIncludes(statusTab, "previewEligibleCandidateCount", "LocalRoutingStatusTab");
assertIncludes(statusTab, "previewExcludedCandidateCount", "LocalRoutingStatusTab");
assertExcludes(statusTab, "当前秘钥", "LocalRoutingStatusTab");
assertExcludes(statusTab, "当前密钥", "LocalRoutingStatusTab");
assertExcludes(statusTab, "eligibleUnderMultiplierLimitCount", "LocalRoutingStatusTab");
assertExcludes(statusTab, "healthyCandidateCount", "LocalRoutingStatusTab");
assertExcludes(statusTab, "function Metric(", "LocalRoutingStatusTab");
```

- [ ] **Step 4: Run the focused tests and verify RED**

Run:

```powershell
node --experimental-strip-types .\scripts\local-routing-status-view-model.test.mjs
node .\scripts\local-routing-cooldown-display.test.mjs
node .\scripts\local-routing-page-layout.test.mjs
```

Expected: the first command fails because `localRoutingStatusViewModel.ts` does not exist; the two source-contract scripts fail because the page still has the old two-card layout and static “进行中” cooldown value.

- [ ] **Step 5: Commit only the RED tests**

```powershell
git add -- scripts/local-routing-status-view-model.test.mjs scripts/local-routing-cooldown-display.test.mjs scripts/local-routing-page-layout.test.mjs
git diff --cached --name-only
git commit -m "test: define local routing status clarity contract"
```

Expected staged paths: exactly the three scripts above.

---

### Task 2: Make Scheduler Preview Eligibility The Backend Source Of Truth

**Files:**
- Modify: `src-tauri/src/services/proxy/scheduler/explanation.rs`
- Modify: `src-tauri/src/services/proxy/routing_health.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/proxy/routing_types.rs`
- Modify: `src-tauri/src/services/proxy/routing_snapshot.rs`
- Modify: `src/lib/types/localRouting.ts`
- Modify: `scripts/local-routing-automatic-settings.test.mjs`

- [ ] **Step 1: Add failing Rust tests for consistent row and summary decisions**

Add a `#[cfg(test)]` module to `routing_snapshot.rs`. The tests must construct one matching candidate, one group mismatch, one low-confidence multiplier rejection, and one active cooldown, then assert:

```rust
#[test]
fn preview_summary_counts_the_same_decisions_exposed_on_rows() {
    let rows = preview_rows_for_test(vec![
        preview_candidate("eligible", PreviewFixture::Eligible),
        preview_candidate("group-mismatch", PreviewFixture::GroupMismatch),
        preview_candidate("low-confidence", PreviewFixture::LowConfidence),
        preview_candidate("cooldown", PreviewFixture::Cooldown),
    ], 1.0);

    assert!(rows[0].preview_eligible);
    assert_eq!(rows[1].preview_reject_reasons, vec!["routing_group_mismatch"]);
    assert_eq!(
        rows[2].preview_reject_reasons,
        vec!["multiplier_evidence_low_confidence"],
    );
    assert_eq!(rows[3].preview_reject_reasons, vec!["health_blocked"]);

    let summary = build_local_routing_summary(&rows, None);
    assert_eq!(summary.candidate_count, 4);
    assert_eq!(summary.preview_eligible_candidate_count, 1);
    assert_eq!(summary.preview_excluded_candidate_count, 3);
    assert_eq!(summary.cooldown_candidate_count, 1);
}

#[test]
fn missing_multiplier_limit_blocks_preview_without_guessing() {
    let rows = preview_rows_without_multiplier_limit_for_test();
    assert!(rows.iter().all(|row| !row.preview_eligible));
    assert!(rows.iter().all(|row| {
        row.preview_reject_reasons == vec!["routing_multiplier_limit_not_configured"]
    }));
}
```

Add focused tests beside the existing `routing_health.rs` tests so the shared hard gate is fixed before either caller changes:

```rust
#[test]
fn health_block_uses_current_time_instead_of_a_fixed_epoch_threshold() {
    let mut health = station_key_health();
    health.cooldown_until = Some("61000".to_string());
    assert!(health_is_blocked(Some(&health), 60_000));

    health.cooldown_until = Some("59999".to_string());
    assert!(!health_is_blocked(Some(&health), 60_000));

    health.cooldown_until = Some("invalid".to_string());
    assert!(!health_is_blocked(Some(&health), 60_000));
}

#[test]
fn explicit_offline_health_is_blocked() {
    let mut health = station_key_health();
    health.last_error_summary = Some("connection refused".to_string());
    assert!(health_is_blocked(Some(&health), 60_000));
}
```

- [ ] **Step 2: Run the Rust test and verify RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml health_block --lib
cargo test --manifest-path .\src-tauri\Cargo.toml routing_snapshot::tests --lib
```

Expected: compilation fails because `health_is_blocked`, `preview_eligible`, `preview_reject_reasons`, and `build_local_routing_summary` do not exist.

- [ ] **Step 3: Expose one stable scheduler rejection-code formatter**

In `scheduler/explanation.rs`, make the existing mapping reusable instead of creating a second mapping in `routing_snapshot.rs`:

```rust
pub fn rejection_code_label(code: CandidateRejectionCode) -> &'static str {
    match code {
        CandidateRejectionCode::AssetUnavailable => "asset_unavailable",
        CandidateRejectionCode::RoutingGroupMismatch => "routing_group_mismatch",
        CandidateRejectionCode::CapabilityMismatch => "capability_mismatch",
        CandidateRejectionCode::ModelMismatch => "model_mismatch",
        CandidateRejectionCode::HealthBlocked => "health_blocked",
        CandidateRejectionCode::BalanceDepleted => "balance_depleted",
        CandidateRejectionCode::NoMultiplierEvidence => "no_multiplier_evidence",
        CandidateRejectionCode::MultiplierEvidenceInvalid => "multiplier_evidence_invalid",
        CandidateRejectionCode::MultiplierEvidenceNegative => "multiplier_evidence_negative",
        CandidateRejectionCode::MultiplierEvidenceExpired => "multiplier_evidence_expired",
        CandidateRejectionCode::MultiplierEvidenceUnboundGroup => "multiplier_evidence_unbound_group",
        CandidateRejectionCode::MultiplierEvidenceLowConfidence => "multiplier_evidence_low_confidence",
        CandidateRejectionCode::MultiplierOverCeiling => "multiplier_over_ceiling",
    }
}
```

Keep `rejection_reason_codes` calling this function so existing route explanations and the new status snapshot cannot drift.

- [ ] **Step 4: Centralize the runtime health hard gate**

Add this helper to `routing_health.rs` and reuse the existing `error_summary_indicates_offline` classifier:

```rust
pub fn health_is_blocked(health: Option<&StationKeyHealth>, now_ms: i64) -> bool {
    let Some(health) = health else {
        return false;
    };
    let cooldown_active = health
        .cooldown_until
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|until_ms| until_ms > now_ms);
    let offline = health
        .last_error_summary
        .as_deref()
        .is_some_and(error_summary_indicates_offline);
    cooldown_active || offline
}
```

Change `rich_candidate_to_scheduler_candidate` in `router.rs` to accept `now_ms`, call it from the existing map with `request.now_ms`, and set:

```rust
health_blocked: health_is_blocked(candidate.health.as_ref(), now_ms),
```

Delete the fixed `cooldown_until > 1_800_000_000_000` check. This is required for the displayed preview and actual router to agree around cooldown expiry.

- [ ] **Step 5: Replace ambiguous summary fields and extend candidate rows**

In `routing_types.rs`, use this serialized contract:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingSummary {
    pub candidate_count: i64,
    pub preview_eligible_candidate_count: i64,
    pub preview_excluded_candidate_count: i64,
    pub cooldown_candidate_count: i64,
    pub last_decision_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalRoutingCandidateRow {
    // Preserve the existing identity, health, multiplier, group, and fact fields.
    pub preview_eligible: bool,
    pub preview_reject_reasons: Vec<String>,
}
```

Remove `enabled_candidate_count`, `healthy_candidate_count`, `eligible_under_multiplier_limit_count`, and `degraded_candidate_count` from `LocalRoutingSummary`; none of them expresses the scheduler preview contract shown in the UI.

- [ ] **Step 6: Evaluate each snapshot candidate through scheduler eligibility**

In `routing_snapshot.rs`, add focused private helpers:

```rust
fn preview_schedule_request(
    filter: RoutingGroupFilter,
    max_rate_multiplier: f64,
    now_ms: i64,
) -> ScheduleRequest {
    ScheduleRequest {
        endpoint: RouteEndpointKind::ChatCompletions,
        requested_model: None,
        mapped_model: None,
        routing_group_filter: filter,
        stream: false,
        uses_tools: false,
        uses_vision: false,
        uses_reasoning: false,
        max_rate_multiplier,
        session_hash: None,
        previous_response_id: None,
        excluded_key_ids: Vec::new(),
        now_ms,
    }
}

fn preview_decision(
    request: Option<&ScheduleRequest>,
    candidate: &SchedulerCandidate,
) -> (bool, Vec<String>) {
    let Some(request) = request else {
        return (
            false,
            vec!["routing_multiplier_limit_not_configured".to_string()],
        );
    };

    match evaluate_candidate(request, candidate) {
        Ok(()) => (true, Vec::new()),
        Err(rejection) => (
            false,
            rejection
                .reasons
                .into_iter()
                .map(|code| rejection_code_label(code).to_string())
                .collect(),
        ),
    }
}
```

Map `LocalRoutingReadCandidate` into `SchedulerCandidate` with its real capability, group, health, balance, and multiplier facts. Pass the same `now_ms` into `health_is_blocked(candidate.health.as_ref(), now_ms)`, and use the existing depleted balance statuses `depleted | insufficient | blocked`. Do not reproduce scheduler rejection ordering locally.

Change `candidate_row` to receive the optional preview request and set `preview_eligible` plus `preview_reject_reasons` from `preview_decision`.

- [ ] **Step 7: Derive summary counts only from projected rows**

Add and use:

```rust
fn build_local_routing_summary(
    rows: &[LocalRoutingCandidateRow],
    last_decision_at: Option<String>,
) -> LocalRoutingSummary {
    LocalRoutingSummary {
        candidate_count: rows.len() as i64,
        preview_eligible_candidate_count: rows
            .iter()
            .filter(|row| row.preview_eligible)
            .count() as i64,
        preview_excluded_candidate_count: rows
            .iter()
            .filter(|row| !row.preview_eligible)
            .count() as i64,
        cooldown_candidate_count: rows
            .iter()
            .filter(|row| row.health_state == RouteHealthState::Cooldown)
            .count() as i64,
        last_decision_at,
    }
}
```

This guarantees `candidateCount === previewEligibleCandidateCount + previewExcludedCandidateCount` by construction.

- [ ] **Step 8: Mirror the command contract in TypeScript**

Update `src/lib/types/localRouting.ts`:

```ts
export type LocalRoutingSummary = {
  candidateCount: number;
  previewEligibleCandidateCount: number;
  previewExcludedCandidateCount: number;
  cooldownCandidateCount: number;
  lastDecisionAt: string | null;
};

export type LocalRoutingCandidateRow = {
  // Preserve all existing fields.
  previewEligible: boolean;
  previewRejectReasons: string[];
};
```

Update `scripts/local-routing-automatic-settings.test.mjs` to require the four new Rust summary fields and reject the three removed ambiguous fields.

- [ ] **Step 9: Run backend and contract tests to verify GREEN**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml health_block --lib
cargo test --manifest-path .\src-tauri\Cargo.toml routing_snapshot::tests --lib
node .\scripts\local-routing-automatic-settings.test.mjs
pnpm.cmd build
```

Expected: all commands exit 0; the Vite chunk-size warning is allowed.

- [ ] **Step 10: Commit the backend truth boundary**

```powershell
git add -- src-tauri/src/services/proxy/scheduler/explanation.rs src-tauri/src/services/proxy/routing_health.rs src-tauri/src/services/proxy/router.rs src-tauri/src/services/proxy/routing_types.rs src-tauri/src/services/proxy/routing_snapshot.rs src/lib/types/localRouting.ts scripts/local-routing-automatic-settings.test.mjs
git diff --cached --name-only
git commit -m "refactor: expose reliable routing preview status"
```

Expected staged paths: exactly the seven files above.

---

### Task 3: Add Pure Presentation Rules And A Single Cooldown Clock

**Files:**
- Create: `src/features/routing/localRoutingStatusViewModel.ts`
- Create: `src/features/routing/useCooldownClock.ts`
- Test: `scripts/local-routing-status-view-model.test.mjs`
- Test: `scripts/local-routing-cooldown-display.test.mjs`

- [ ] **Step 1: Implement deterministic cooldown formatting**

Create `localRoutingStatusViewModel.ts` with no runtime imports so Node can execute it through type stripping:

```ts
import type { RouteDecisionSummary, RouteHealthState } from "@/lib/types/localRouting";

export type CooldownDisplay = {
  active: boolean;
  label: string;
  remainingSeconds: number | null;
};

export function buildCooldownDisplay(
  healthState: RouteHealthState,
  cooldownUntilMs: number | null,
  nowMs: number,
): CooldownDisplay {
  if (healthState !== "cooldown") {
    return { active: false, label: "无", remainingSeconds: null };
  }
  if (cooldownUntilMs == null || !Number.isFinite(cooldownUntilMs)) {
    return { active: true, label: "冷却中", remainingSeconds: null };
  }

  const remainingSeconds = Math.ceil((cooldownUntilMs - nowMs) / 1_000);
  if (remainingSeconds <= 0) {
    return { active: true, label: "即将结束", remainingSeconds: 0 };
  }

  const hours = Math.floor(remainingSeconds / 3_600);
  const minutes = Math.floor((remainingSeconds % 3_600) / 60);
  const seconds = remainingSeconds % 60;
  const label = hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  return { active: true, label, remainingSeconds };
}
```

Use `Math.ceil` so the UI does not display zero one fractional second before expiry.

- [ ] **Step 2: Normalize scheduler and collector copy in one mapping**

Add exhaustive known-code maps with an explicit unknown fallback:

```ts
const previewRejectReasonLabels: Record<string, string> = {
  asset_unavailable: "密钥不可用",
  routing_group_mismatch: "分组不匹配",
  capability_mismatch: "接口能力不匹配",
  model_mismatch: "模型不匹配",
  health_blocked: "健康状态阻止路由",
  balance_depleted: "余额不足",
  no_multiplier_evidence: "缺少倍率证据",
  multiplier_evidence_invalid: "倍率证据无效",
  multiplier_evidence_negative: "倍率不能为负数",
  multiplier_evidence_expired: "倍率证据已过期",
  multiplier_evidence_unbound_group: "倍率未绑定分组",
  multiplier_evidence_low_confidence: "费率可信度不足",
  multiplier_over_ceiling: "超过倍率上限",
  routing_multiplier_limit_not_configured: "倍率上限未设置",
};

export function formatPreviewRejectReason(code: string) {
  return previewRejectReasonLabels[code] ?? "当前请求条件不满足";
}

export function formatMultiplierSource(source: string | null, confidence: number | null) {
  if (!source) return "暂无可信来源";
  const sourceLabel = source === "sub2api_groups_rates" ? "Sub2API 分组费率" : source;
  return confidence == null
    ? sourceLabel
    : `${sourceLabel} · 可信度 ${(confidence * 100).toFixed(0)}%`;
}

export function formatBalanceStatus(value: string | null) {
  return ({ normal: "正常", low: "偏低", depleted: "已耗尽" } as Record<string, string>)[value ?? ""] ?? "未知";
}
```

- [ ] **Step 3: Make latest-decision presentation historical by definition**

Add `buildLatestDecisionDisplay(proxyRunning, latestDecision)` that returns:

```ts
export type LatestDecisionDisplay = {
  title: string;
  badge: "历史记录" | "已选中" | "已回退" | "失败" | "不可用" | null;
  tone: "neutral" | "healthy" | "warning" | "error";
  decidedAt: string | null;
};
```

Rules:

```ts
if (!latestDecision) {
  return { title: "尚无路由记录", badge: null, tone: "neutral", decidedAt: null };
}
if (!proxyRunning) {
  return {
    title: latestDecision.selectedStationName ?? "未选中密钥",
    badge: "历史记录",
    tone: "neutral",
    decidedAt: latestDecision.decidedAt,
  };
}
```

For a running proxy, map `selected -> 已选中`, `fallback -> 已回退`, `failed -> 失败`, and `unavailable -> 不可用`. Use `latestDecision.decidedAt`, not `summary.lastDecisionAt`, as the timestamp beside the decision.

- [ ] **Step 4: Implement one page-level ticker with deadline deduplication**

Create `useCooldownClock.ts`:

```ts
import { useEffect, useRef, useState } from "react";

export type CooldownDeadline = { id: string; untilMs: number };

export function useCooldownClock({
  active,
  deadlines,
  onExpired,
}: {
  active: boolean;
  deadlines: CooldownDeadline[];
  onExpired: (ids: string[]) => void;
}) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const notifiedDeadlinesRef = useRef(new Set<string>());

  useEffect(() => {
    const currentKeys = new Set(deadlines.map(({ id, untilMs }) => `${id}:${untilMs}`));
    for (const key of notifiedDeadlinesRef.current) {
      if (!currentKeys.has(key)) notifiedDeadlinesRef.current.delete(key);
    }
  }, [deadlines]);

  useEffect(() => {
    if (!active) return;
    const tick = () => {
      const nextNowMs = Date.now();
      setNowMs(nextNowMs);
      const expiredIds: string[] = [];
      for (const { id, untilMs } of deadlines) {
        const deadlineKey = `${id}:${untilMs}`;
        if (untilMs <= nextNowMs && !notifiedDeadlinesRef.current.has(deadlineKey)) {
          notifiedDeadlinesRef.current.add(deadlineKey);
          expiredIds.push(id);
        }
      }
      if (expiredIds.length > 0) onExpired(expiredIds);
    };
    tick();
    const intervalId = window.setInterval(tick, 1_000);
    return () => window.clearInterval(intervalId);
  }, [active, deadlines, onExpired]);

  return nowMs;
}
```

`RoutingPage` must memoize `deadlines` and `onExpired`; otherwise each one-second render would restart the interval.

- [ ] **Step 5: Run the pure behavior and source-contract tests**

```powershell
node --experimental-strip-types .\scripts\local-routing-status-view-model.test.mjs
node .\scripts\local-routing-cooldown-display.test.mjs
```

Expected: the view-model runtime test passes. The cooldown source contract still fails only because `RoutingPage` and the status row are not wired yet.

- [ ] **Step 6: Commit the pure frontend boundary**

```powershell
git add -- src/features/routing/localRoutingStatusViewModel.ts src/features/routing/useCooldownClock.ts
git diff --cached --name-only
git commit -m "feat: add routing cooldown presentation clock"
```

Expected staged paths: exactly the two new TypeScript files.

---

### Task 4: Rebuild The Status Surface And Candidate Rows

**Files:**
- Create: `src/features/routing/LocalRoutingStatusCandidateRow.tsx`
- Modify: `src/features/routing/LocalRoutingStatusTab.tsx`
- Test: `scripts/local-routing-page-layout.test.mjs`
- Test: `scripts/local-routing-cooldown-display.test.mjs`

- [ ] **Step 1: Build a status-only candidate row**

Create `LocalRoutingStatusCandidateRow.tsx` with props:

```ts
type LocalRoutingStatusCandidateRowProps = {
  candidate: LocalRoutingCandidate;
  order: number;
  nowMs: number;
};
```

Parse `cooldownUntil` through the shared time utility and pass milliseconds into `buildCooldownDisplay`:

```ts
const cooldownUntilMs = candidate.cooldownUntil == null
  ? null
  : toTimestampMillis(candidate.cooldownUntil);
const cooldown = buildCooldownDisplay(candidate.healthState, cooldownUntilMs, nowMs);
const primaryRejectReason = candidate.previewRejectReasons[0] ?? null;
const balanceFact = candidate.facts.find((fact) => fact.kind === "balance");
const multiplierLabel = candidate.effectiveMultiplier == null
  ? "未确认"
  : `${candidate.effectiveMultiplier.toFixed(4)}x`;
const multiplierSourceLabel = formatMultiplierSource(
  candidate.effectiveMultiplierSource,
  candidate.effectiveMultiplierConfidence,
);
const balanceLabel = formatBalanceStatus(balanceFact?.value ?? null);
```

Render one stable responsive grid, not an `ObjectRow` nested inside a `SectionCard`:

```tsx
<div className="grid min-h-[68px] gap-3 px-3 py-2.5 sm:grid-cols-[minmax(220px,1.6fr)_minmax(120px,.8fr)_minmax(104px,.65fr)_minmax(92px,.55fr)_minmax(88px,.5fr)] sm:items-center">
  <div className="min-w-0">
    <div className="flex items-center gap-2">
      <span className="text-xs font-semibold text-slate-500">#{order}</span>
      <span className="truncate text-[13px] font-semibold text-slate-900">{candidate.keyName}</span>
    </div>
    <div className="mt-0.5 truncate text-xs text-muted-foreground">{candidate.stationName} · 聊天补全</div>
  </div>
  <MetricCell label="参与状态">
    <StatusBadge tone={candidate.previewEligible ? "healthy" : "warning"}>
      {candidate.previewEligible ? "可参与" : "不参与"}
    </StatusBadge>
    {!candidate.previewEligible && primaryRejectReason ? (
      <div className="mt-1 text-xs text-amber-700">{formatPreviewRejectReason(primaryRejectReason)}</div>
    ) : null}
  </MetricCell>
  <MetricCell label="有效倍率" value={multiplierLabel} detail={multiplierSourceLabel} />
  <MetricCell label="余额" value={balanceLabel} />
  <MetricCell label="冷却" value={cooldown.label} tone={cooldown.active ? "warning" : "neutral"} />
</div>
```

Keep the responsive metric cell local to this status-only component:

```tsx
function MetricCell({
  label,
  value,
  detail,
  tone = "neutral",
  children,
}: {
  label: string;
  value?: ReactNode;
  detail?: ReactNode;
  tone?: "neutral" | "warning";
  children?: ReactNode;
}) {
  return (
    <div className="min-w-0">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      {children ?? (
        <div className={tone === "warning" ? "text-[13px] font-semibold text-amber-700" : "text-[13px] font-semibold text-slate-800"}>
          {value}
        </div>
      )}
      {detail ? <div className="truncate text-[11px] text-muted-foreground">{detail}</div> : null}
    </div>
  );
}
```

On narrow widths, `MetricCell` shows its label and wraps into two columns. On `sm` and wider, columns remain stable. Do not render the redundant normal-state badge trio “启用 / 就绪 / 分组匹配”.

- [ ] **Step 2: Replace the two summary cards with one runtime status band**

Refactor `LocalRoutingStatusTab` props:

```ts
type LocalRoutingStatusTabProps = {
  workspace: LocalRoutingWorkspace | null;
  loading: boolean;
  nowMs: number;
  proxyActionPending: boolean;
  onToggleProxy: () => void;
};
```

The first `SectionCard` must contain, in order:

1. Endpoint icon, `127.0.0.1:port`, endpoint name, and prominent `运行中 / 未启动` badge.
2. One context-aware command button: `启动路由` while stopped, `停止路由` while running.
3. An unframed four-column definition list: `倍率上限`, `分组筛选`, `可参与`, `不参与`.
4. A divider and compact “最近一次路由” row using `buildLatestDecisionDisplay`.

Use this metric structure instead of four mini-cards:

```tsx
<dl className="grid gap-x-6 gap-y-3 border-t border-slate-100 pt-3 sm:grid-cols-4">
  <StatusMetric label="倍率上限" value={multiplierLimitLabel} />
  <StatusMetric label="分组筛选" value={formatRoutingGroupFilter(workspace.settings.routingGroupFilter)} />
  <StatusMetric label="可参与" value={workspace.summary.previewEligibleCandidateCount} tone="good" />
  <StatusMetric label="不参与" value={workspace.summary.previewExcludedCandidateCount} tone={workspace.summary.previewExcludedCandidateCount > 0 ? "warning" : "neutral"} />
</dl>
```

Define `StatusMetric` locally because it is specific to this status band and should not become another global card primitive:

```tsx
function StatusMetric({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: number | string;
  tone?: "neutral" | "good" | "warning";
}) {
  const valueClass = tone === "good"
    ? "text-emerald-700"
    : tone === "warning"
      ? "text-amber-700"
      : "text-slate-900";
  return (
    <div>
      <dt className="text-[11px] text-muted-foreground">{label}</dt>
      <dd className={`mt-0.5 text-sm font-semibold ${valueClass}`}>{value}</dd>
    </div>
  );
}
```

The stopped state must never visually style the latest decision as currently active.

- [ ] **Step 3: Render an unframed candidate section with one list boundary**

Replace the outer candidate `SectionCard` with:

```tsx
<section aria-labelledby="local-routing-candidates-title">
  <div className="mb-2 flex items-center justify-between gap-3">
    <h2 id="local-routing-candidates-title" className="text-sm font-semibold text-slate-900">
      候选顺序预览
    </h2>
    <span className="text-xs text-muted-foreground">{workspace.summary.candidateCount} 个密钥</span>
  </div>
  {workspace.candidates.length === 0 ? (
    <EmptyState title="暂无候选密钥" description="当前配置下没有可预览的路由密钥。" />
  ) : (
    <div className="overflow-hidden rounded-[var(--surface-radius)] border border-slate-200 bg-white divide-y divide-slate-100">
      {workspace.candidates.map((candidate, index) => (
        <LocalRoutingStatusCandidateRow
          key={candidate.stationKeyId}
          candidate={candidate}
          order={index + 1}
          nowMs={nowMs}
        />
      ))}
    </div>
  )}
</section>
```

- [ ] **Step 4: Keep loading and empty states accurate**

Loading remains `正在加载本地路由状态...`. Replace the developer-facing empty description `请检查本地路由预览接口` with `刷新后仍无数据，请检查本地路由配置。` Do not expose command/API implementation language.

- [ ] **Step 5: Run layout tests and verify partial GREEN**

```powershell
node .\scripts\local-routing-page-layout.test.mjs
pnpm.cmd build
```

Expected: layout contract and build pass. The cooldown source contract still fails only on the not-yet-wired page clock.

- [ ] **Step 6: Commit the visual surface**

```powershell
git add -- src/features/routing/LocalRoutingStatusCandidateRow.tsx src/features/routing/LocalRoutingStatusTab.tsx
git diff --cached --name-only
git commit -m "feat: clarify local routing status layout"
```

Expected staged paths: exactly the two status UI files.

---

### Task 5: Wire Proxy Actions, Visibility, Countdown, And Expiry Refresh

**Files:**
- Modify: `src/features/routing/RoutingPage.tsx`
- Test: `scripts/local-routing-cooldown-display.test.mjs`
- Test: `scripts/local-routing-page-layout.test.mjs`

- [ ] **Step 1: Add one guarded proxy operation**

Import `startLocalProxy` and `stopLocalProxy`. Add one action state and use the existing toast boundary:

```ts
const [proxyActionPending, setProxyActionPending] = useState(false);

const handleToggleProxy = useCallback(async () => {
  if (!workspace || proxyActionPending) return;
  setProxyActionPending(true);
  try {
    if (workspace.proxyStatus.running) {
      await stopLocalProxy();
      toast.success("本地路由已停止");
    } else {
      await startLocalProxy();
      toast.success("本地路由已启动");
    }
    await queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace });
  } catch (actionError) {
    toast.error(
      workspace.proxyStatus.running ? "停止本地路由失败" : "启动本地路由失败",
      readError(actionError),
    );
  } finally {
    setProxyActionPending(false);
  }
}, [proxyActionPending, queryClient, toast, workspace]);
```

Disable both the status action and the top refresh button while this operation is pending.

- [ ] **Step 2: Parse and memoize only authoritative cooldown deadlines**

Use `toTimestampMillis` and keep only rows whose backend health state is cooldown and whose timestamp parses to a finite number:

```ts
const cooldownDeadlines = useMemo(
  () => (workspace?.candidates ?? []).flatMap((candidate) => {
    if (candidate.healthState !== "cooldown" || candidate.cooldownUntil == null) return [];
    const untilMs = toTimestampMillis(candidate.cooldownUntil);
    return Number.isFinite(untilMs) ? [{ id: candidate.stationKeyId, untilMs }] : [];
  }),
  [workspace],
);
```

- [ ] **Step 3: Tick only while the retained page and status tab are visible**

Memoize the expiry callback, then enable the clock only for the active page/status tab:

```ts
const handleCooldownExpired = useCallback(() => {
  void queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace });
}, [queryClient]);

const nowMs = useCooldownClock({
  active: refreshEnabled && activeTab === "status" && cooldownDeadlines.length > 0,
  deadlines: cooldownDeadlines,
  onExpired: handleCooldownExpired,
});
```

This is required because Relay Pool retains inactive pages; a timer tied only to component mount would continue running in the background.

- [ ] **Step 4: Pass orchestration state into the presentational tab**

```tsx
<LocalRoutingStatusTab
  loading={loading}
  workspace={workspace}
  nowMs={nowMs}
  proxyActionPending={proxyActionPending}
  onToggleProxy={() => void handleToggleProxy()}
/>
```

- [ ] **Step 5: Run all focused status tests to verify GREEN**

```powershell
node --experimental-strip-types .\scripts\local-routing-status-view-model.test.mjs
node .\scripts\local-routing-cooldown-display.test.mjs
node .\scripts\local-routing-page-layout.test.mjs
node .\scripts\local-routing-explanation.test.mjs
node .\scripts\local-routing-reorder.test.mjs
pnpm.cmd build
```

Expected: all commands exit 0. The explanation test proves the removed troubleshooting surface was not reintroduced; the reorder test proves the edit tab is unchanged.

- [ ] **Step 6: Commit the orchestration wiring**

```powershell
git add -- src/features/routing/RoutingPage.tsx
git diff --cached --name-only
git commit -m "feat: wire routing status actions and cooldown refresh"
```

Expected staged path: only `src/features/routing/RoutingPage.tsx`.

---

### Task 6: Verify Runtime Truth, Layout, And Scope

**Files:**
- Verify only

- [ ] **Step 1: Run the complete Local Routing regression set**

```powershell
node .\scripts\local-routing-automatic-settings.test.mjs
node .\scripts\local-routing-boundary-controls.test.mjs
node .\scripts\local-routing-cooldown-display.test.mjs
node .\scripts\local-routing-explanation.test.mjs
node .\scripts\local-routing-page-layout.test.mjs
node .\scripts\local-routing-query-service.test.mjs
node .\scripts\local-routing-redaction.test.mjs
node .\scripts\local-routing-reorder.test.mjs
node .\scripts\local-routing-settings-form.test.mjs
node .\scripts\local-routing-smart-edit.test.mjs
node --experimental-strip-types .\scripts\local-routing-status-view-model.test.mjs
```

Expected: every script exits 0.

- [ ] **Step 2: Run frontend and Rust verification**

```powershell
pnpm.cmd build
cargo fmt --manifest-path .\src-tauri\Cargo.toml -- --check
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml health_block --lib
cargo test --manifest-path .\src-tauri\Cargo.toml routing_snapshot::tests --lib
```

Expected: all commands exit 0. The existing Vite chunk-size warning is allowed; new warnings are not.

- [ ] **Step 3: Verify the real Tauri runtime**

Run:

```powershell
pnpm.cmd tauri:dev
```

In the real app, verify these states with current-source rendering:

1. Proxy stopped: “未启动” is prominent, the action says “启动路由”, and the latest decision is marked “历史记录”.
2. Proxy running: the status changes to “运行中”, the action says “停止路由”, and refresh returns the same candidate counts as the rows.
3. One eligible and one rejected candidate: top counts equal row states exactly.
4. Group mismatch, low confidence, over-ceiling multiplier, depleted balance, and cooldown each show Chinese reasons rather than raw codes.
5. Active cooldown: the value decreases once per second without changing layout width.
6. Countdown expiry: the row shows “即将结束” briefly, triggers one workspace refresh, and then reflects the refreshed backend health state.
7. Switch to Edit and another app page: no hidden-page countdown updates or repeated expiry requests occur.
8. Resize to 1440x900, 1024x768, and 375x800: no overlapping text, clipped buttons, or horizontal scroll; candidate metrics wrap below the identity column on narrow width.

- [ ] **Step 4: Audit exact scope and unstaged user work**

```powershell
git diff --check
git status --short
git diff --cached --name-only
```

Expected:

- No staged files remain after the task commits.
- Existing unrelated page-transition changes remain untouched: `src/app/ShellPageHost.tsx`, `src/app/TransientPageHost.tsx`, `src/styles.css`, and their related scripts.
- No API keys, logs, screenshots, local databases, `.env` files, or generated build artifacts are tracked.

## Self-Review Result

- Spec coverage: runtime state, historical decision semantics, scheduler-backed preview counts, normalized row copy, live cooldown countdown, expiry refresh, proxy action, responsive layout, and retained-page lifecycle all have explicit implementation and verification steps.
- Boundary check: scheduler eligibility remains authoritative in Rust; countdown formatting is pure; only `RoutingPage` owns effects; the edit-tab drag row remains separate.
- Scope check: no troubleshooting panel, drawer, channel probing, routing algorithm change, settings-editor change, or shared `ObjectRow` refactor is included.
- Placeholder scan: no deferred requirement or unspecified test remains.
