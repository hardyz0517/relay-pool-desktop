# Local Routing Redesign Design

Date: 2026-07-08
Status: Draft for user review
Scope: Relay Pool Desktop local routing experience, routing decision architecture, and first implementation plan boundary.

## 1. Background

Relay Pool Desktop already has the right product model:

- `Station` is the upstream site/account asset.
- `Station Key` is the request routing object.
- `Key Pool` manages all station keys.
- `Channel Status` owns monitoring, probing, latency, recent outcomes, cooldown, and health visibility.
- `Pricing / Rates` owns normalized economic facts.
- `Request Logs` explain individual real requests.

The current local routing experience is still hard to trust because the important decisions are spread across Dashboard, Key Pool, Routing Rules, Channel Status, Pricing, and Logs. Users cannot quickly answer:

- Is my fixed local OpenAI-compatible endpoint safe to use right now?
- Which key will the next request use?
- Why did the router keep or switch keys?
- How do I change the preferred key order without learning another routing DSL?
- Will route switching break streaming output or cache locality?
- Will a flaky probe misclassify a healthy key?

The redesign makes `Local Routing` a focused control surface rather than another all-in-one admin page.

## 2. External Reference Findings

These references were inspected as current engineering examples. They are used for design principles only. Do not copy code or product shape directly.

Live reference HEADs checked on 2026-07-08:

- CC Switch: `e78aa8a7c3fd2a611f377c6b8a16127463b5cd48`
- sub2api: `6f43986c376d76144cb39c7a562c179e19ac7439`
- CLIProxyAPI / CPA: `4f2e19042cdd174cc6f17651061eb05e47f74660`
- ModelFlux: `aad5a28b08f719ccd04eb03bd07c7237f9c500db`

- CC Switch: https://github.com/farion1231/cc-switch
  - Relevant principle: keep local routing, provider config, model mapping, and client integration explicit. Users need a clear local service state before they can trust provider switching.
- sub2api: https://github.com/Wei-Shaw/sub2api
  - Relevant principle: model routing state as scheduler facts, not UI-only flags. Its account model and tests show fields such as priority, schedulable state, cooldown, temporary unschedulable state, rate-limit reset, and scheduled test recovery.
  - Relevant principle: layered candidate filtering is easier to test than one large scoring function. The inspected tests filter by priority, then load, then least-recent use.
  - Relevant principle: scheduled low-cost tests can recover accounts after cooldown, but must be bounded by worker limits and next-run scheduling.
- CLIProxyAPI / CPA: https://github.com/router-for-me/CLIProxyAPI
  - Relevant principle: retry, max retry credentials, cooldown, transient error cooldown, model alias pools, session affinity, streaming bootstrap retries, and cache identity concerns should be separate settings or internal policies.
  - Relevant principle: automatic failover should happen only before output has started or when a request can be retried safely.
- ModelFlux: https://github.com/dabao-yi/model-flux
  - Relevant principle: use an account scheduler with explicit account states (`healthy`, `probing`, `insufficient_balance`, `rate_limited`, `auth_error`, `temporary_error`, `manual_disabled`) and a small classifier for upstream HTTP/exception failures.
  - Relevant principle: runtime snapshots should expose schedulable, in-flight, last success, last error, cooldown remaining, and due-probe information for UI and tests.

Adopted pattern for Relay Pool:

- Keep the UI simple.
- Make the routing engine explicit, typed, and testable.
- Use small pure functions for candidate filtering, ranking, failure classification, and route explanation.
- Keep probing, monitoring, and route selection related but not tangled.
- Treat streaming output as a hard boundary: do not interrupt an in-flight stream for background health changes.

## 3. Product Goals

### 3.1 Primary Goal

Make the local routing feature something the user is willing to turn on and keep on.

The experience should feel like:

- I have one stable local API endpoint.
- The app chooses a low-cost healthy key by default.
- It avoids needless key switching to preserve cache locality and reduce task interruption.
- It switches only when there is enough evidence.
- I can drag keys to express preferred order.
- I can understand why a key was selected or skipped.

### 3.2 Non-Goals

This redesign must not:

- Build another large admin backend inside the desktop UI.
- Replace Key Pool as the key asset management page.
- Replace Channel Status as the probing/monitoring page.
- Add key groups as the primary mental model.
- Add user-facing weight controls in the first version.
- Add a full routing rule DSL.
- Add team/account/payment/cloud features.
- Interrupt active streaming requests because health status changed mid-stream.

## 4. UX Design

### 4.1 Page Identity

Introduce or reshape the local routing page as the main control surface for the fixed local OpenAI-compatible endpoint.

Recommended page title:

- `本地路由`

Recommended tabs, following the existing `渠道状态` pattern:

- `状态`
- `编辑`

Use the existing `PageScaffold` actions area with `SegmentedControl`, matching `ChannelStatusPage`.

### 4.2 Status Tab

Purpose: answer "Can I use the local endpoint now, and how will it route?"

First viewport content:

- Local endpoint state:
  - running/stopped
  - base URL
  - masked local access key
  - start/stop button
  - copy URL/key buttons
- Summary metrics:
  - current primary key
  - schedulable key count
  - today's cost
  - switch risk or recent abnormal count
- Candidate order:
  - `1 Key A`
  - `2 Key B`
  - `3 Key C`
  - each row shows compact facts: price state, health, balance, last success/failure, cooldown/observation state.
- Latest route explanation:
  - last real request or last simulation result
  - selected key
  - skipped keys and reasons
  - whether current key was kept for stability.
- Actions:
  - `模拟一次请求`
  - `打开渠道状态`
  - optional `查看请求日志`

Status tab should avoid deep controls. It is a read model plus safe navigation.

### 4.3 Edit Tab

Purpose: answer "How do I adjust the router without learning internals?"

Edit tab content:

- Automatic strategy section:
  - strategy label: `低价优先 + 稳定保持`
  - optional simple mode selector later: `更稳 / 均衡 / 更省钱`
  - first implementation can keep `均衡` fixed if strategy tuning is not ready.
- Key priority order:
  - rows show `1`, `2`, `3`, not `P1/P2/P3`.
  - drag row to change priority order.
  - drop triggers auto-save.
  - no separate priority number input.
  - no weight field in first version.
  - each row shows enabled state, health state, brief economic state, sync state, and `进入详情`.
- Switch-before-confirm setting:
  - default on.
  - compact row only.
  - text: use low-token template; do not proactively probe depleted/cooling keys.
  - if this feels too technical during implementation, hide it behind an informational row and keep behavior default-on.
- `新增 Key` action:
  - routes to existing key creation flow.

Auto-save rules:

- Drag reorder auto-saves.
- Toggle/strategy edits auto-save.
- Row-level sync states: `已同步`, `保存中`, `保存失败`.
- No page-level `保存策略` button.
- Destructive actions such as disabling many keys, deleting keys, or stopping local routing still require normal app confirmation patterns.

### 4.4 Detail Navigation

Do not use a drawer for this redesign.

If a key row needs more detail:

- navigate to the existing key/station detail surface, or
- open the existing small-window/dialog pattern if that is the established flow for a single key.

The local routing page should not become a diagnostic page.

### 4.5 Visual Direction

Follow existing Relay Pool Desktop UI rules:

- light desktop-tool style
- white/near-white panels
- thin borders
- compact rows
- low-saturation status colors
- scan-friendly information density
- no marketing hero
- no dark theme introduction
- no decorative gradients or large illustration blocks

Use lucide icons for copy, power, refresh, play/simulate, grip, key, alert, external navigation.

## 5. Routing Behavior

### 5.1 Strategy Name

First-version strategy:

`低价优先 + 稳定保持`

English internal name suggestion:

`cost_stable_first`

This should supersede the current user-facing ambiguity between `cheap_first`, `stable_first`, and `priority_fallback`.

### 5.2 Selection Principles

The router chooses from station keys, not stations.

Candidate selection pipeline:

1. Build a candidate snapshot from enabled station keys.
2. Filter capability/protocol mismatches:
   - endpoint kind
   - model compatibility
   - streaming support if relevant
   - tools/vision/reasoning/embeddings support
3. Filter hard-unavailable state:
   - disabled
   - no API key
   - auth error requiring manual action
   - insufficient balance or depleted
   - explicit cooldown still active
   - station disabled
4. Apply economics:
   - prefer complete price facts.
   - treat group-rate-only facts as explanation, not exact cheap sorting.
   - avoid routing on missing price if a comparable complete-price key is healthy.
5. Apply stability:
   - keep current sticky key if it is still healthy enough, price is not significantly worse, and no hard failure exists.
   - avoid changing key on every small price difference.
6. Apply user priority order:
   - drag order is a tie-breaker and override signal.
   - lower visible order number means more preferred.
   - first version does not expose weights.
7. Apply load/recent-use only as backend tiebreakers:
   - do not expose this as user-facing complexity.
   - avoid hammering one key if multiple equivalent keys exist.

### 5.3 Stability Window

The router should maintain a lightweight route affinity:

- Scope: model plus endpoint kind, optionally client identity when safely available.
- First version can use `model + endpoint`.
- TTL default: 10-30 minutes, configurable internally.
- Stable key can be kept if:
  - still enabled
  - still schedulable
  - not balance-depleted
  - no hard failure
  - price delta is within tolerance

Do not implement conversation-level stickiness unless reliable request identifiers exist. Avoid inventing fragile heuristics that could misgroup unrelated requests.

### 5.4 Streaming Boundary

Streaming request lifecycle:

- Once the upstream has produced the first meaningful byte/chunk, do not fail over that request.
- Continue reading and forwarding the stream unless the upstream connection actually ends or errors.
- Health updates caused by the stream should affect future requests, not interrupt the current output.
- If failure happens before output starts, failover may retry another candidate.
- If failure happens after output starts, log partial stream failure and classify it, but do not replay automatically unless the protocol has an explicit safe retry mechanism.

This is essential to avoid broken long-running Codex/Claude/Gemini tasks.

### 5.5 Switch-Before-Confirm

Before switching from the current key to a fallback key, the router should confirm that the fallback is likely usable.

Default behavior:

- Background monitoring maintains coarse key health.
- On demand failover uses a bounded, low-token, target-model-aware probe when needed.
- Do not probe:
  - disabled keys
  - keys with known insufficient balance
  - keys in hard auth error
  - keys in cooldown before due time
  - keys with recent deterministic failure for the requested model
- Probe budget:
  - single fallback probe per request attempt
  - short timeout
  - small max output tokens
  - use existing channel-monitor low-token template family where possible
- If probe passes:
  - mark candidate as confirmed for short TTL.
  - route the retry/future request.
- If probe fails:
  - classify result.
  - try next candidate only within retry budget.

For streaming, switch-before-confirm only applies before output begins.

## 6. Failure Classification

Do not store failures as plain strings only. Add a normalized classification layer.

Suggested internal enum:

```ts
type RouteFailureKind =
  | "auth_error"
  | "insufficient_balance"
  | "rate_limited"
  | "model_unavailable"
  | "capability_mismatch"
  | "bad_request"
  | "temporary_network"
  | "upstream_5xx"
  | "timeout"
  | "stream_interrupted"
  | "unknown";
```

Classification outcomes:

- `hard_exclude`
  - auth error
  - insufficient balance
  - model explicitly unavailable
  - capability mismatch
- `cooldown`
  - rate limited
  - upstream 5xx
  - repeated timeout
  - repeated temporary network failure
- `observe`
  - one-off timeout
  - one-off connection reset
  - ambiguous stream interruption
- `ignore_for_key_health`
  - client bad request
  - malformed user payload
  - request canceled by downstream client

Important rule:

One ambiguous failure must not immediately mark a normal key as bad.

Evidence thresholds:

- hard deterministic failures act immediately.
- ambiguous failures require consecutive evidence or a moving-window threshold.
- successful real requests reset or reduce suspicion.
- successful scheduled probe can recover a key from cooldown/observation.

## 7. Engine Architecture

### 7.1 Proposed Domain Modules

Keep the routing engine small and testable.

Rust side suggested modules under `src-tauri/src/services/proxy/` or a focused routing service namespace:

- `routing_snapshot.rs`
  - builds immutable input snapshot for one routing decision.
  - includes keys, capabilities, health, price facts, balance facts, settings, route affinity.
- `routing_policy.rs`
  - pure candidate filtering/ranking functions.
  - no DB writes, no network, no UI dependency.
- `routing_failure.rs`
  - classifies upstream HTTP and transport errors.
  - maps classification to cooldown/observe/hard-exclude.
- `routing_affinity.rs`
  - stores model/endpoint sticky selection with TTL.
  - small in-memory table first; persistent only if proven necessary.
- `routing_probe.rs`
  - switch-before-confirm orchestration.
  - calls existing channel monitor/test-key mechanism or a shared low-token probe service.
- `routing_explanation.rs`
  - converts decision facts into stable explanation objects for UI/logs.
- `routing_events.rs`
  - emits route decision, failover, probe, cooldown, recovery events for request logs/change center if needed.

The UI must not reconstruct routing decisions from scattered arrays. It should consume a read model.

### 7.2 Decision Read Model

Add a backend-generated workspace read model:

```ts
type LocalRoutingWorkspace = {
  proxyStatus: ProxyStatus;
  settings: LocalRoutingSettings;
  summary: LocalRoutingSummary;
  candidates: LocalRoutingCandidateRow[];
  latestDecision: RouteDecisionSummary | null;
  recentEvents: RouteDecisionEvent[];
};
```

Candidate row:

```ts
type LocalRoutingCandidateRow = {
  stationKeyId: string;
  stationId: string;
  order: number;
  keyName: string;
  stationName: string;
  enabled: boolean;
  schedulable: boolean;
  selected: boolean;
  currentAffinity: boolean;
  status: "healthy" | "observing" | "cooling" | "blocked" | "disabled" | "unchecked";
  priceLabel: string;
  balanceLabel: string;
  latencyLabel: string;
  lastOutcomeLabel: string;
  reason: string;
  syncState?: "synced" | "saving" | "failed";
};
```

Decision summary:

```ts
type RouteDecisionSummary = {
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
```

Explanation objects should be structured, then rendered in Chinese in the UI. Do not store only localized strings in the engine.

### 7.3 Commands / API Boundary

Frontend must go through existing typed API boundaries, not direct `invoke` from feature components.

Suggested TS API additions:

- `src/lib/types/localRouting.ts`
- `src/lib/api/localRouting.ts`
- `src/lib/queries/localRoutingQueries.ts`

Suggested commands:

- `load_local_routing_workspace`
- `reorder_local_routing_keys`
- `update_local_routing_mode` (optional first version)
- `simulate_local_route`

`reorder_local_routing_keys` should persist exact order into station key priority/order fields and return the new workspace or candidate rows.

### 7.4 Persistence

First version should avoid a big new schema if possible.

Use existing `station_keys.priority` or equivalent ordering field if it is already the router's source of truth.

If existing semantics conflict, add a clear field:

- `routing_order INTEGER NOT NULL DEFAULT 0`

Only add this if `priority` is overloaded or cannot safely represent drag order.

Do not add user-facing weight.

Possible internal-only settings:

- `routing_affinity_ttl_seconds`
- `routing_price_stability_tolerance_pct`
- `routing_switch_probe_enabled`
- `routing_switch_probe_timeout_ms`
- `routing_ambiguous_failure_threshold`

These can remain defaults in Rust until UI tuning is needed.

### 7.5 Concurrency and State

The router must be safe under concurrent requests.

Rules:

- Build decision snapshots atomically enough that one request sees a consistent candidate list.
- Use per-key in-flight counters only as internal tiebreakers.
- Do not write key health on every streaming chunk.
- Record success/failure once per request lifecycle.
- Release in-flight counters on all exit paths.
- Use request IDs for all logs and explanation events.
- Switch-before-confirm must have per-key/per-model short TTL to avoid repeated probes in a burst.

## 8. UI Engineering Plan

### 8.1 Routing Page Shape

Refactor current `RoutingPage` from model mapping/simulator-first into a tabbed page:

- `LocalRoutingPage`
  - state: active tab `status | edit`
  - actions: `SegmentedControl`
- `LocalRoutingStatusTab`
  - consumes `LocalRoutingWorkspace`
  - renders endpoint card, summary metrics, candidate order, latest explanation.
- `LocalRoutingEditTab`
  - consumes candidate rows.
  - drag-and-drop reorder using existing `@dnd-kit` pattern from `KeyPoolPage` / `ChannelStatusTab`.
  - auto-save reorder.
  - no weight input.
  - no page-level save button.
- `LocalRoutingCandidateRow`
  - shared row component for status/edit variants if useful.
- `RouteExplanationPanel`
  - compact structured explanation.
- `RouteSimulationPanel`
  - can reuse current simulator logic, but visually subordinate to status.

Keep model alias management either:

- as a secondary section below edit, collapsed/advanced; or
- as a separate `模型映射` compact section after first implementation.

Do not let model mapping dominate the first viewport.

### 8.2 Existing Page Responsibilities

- Dashboard:
  - can keep a small current-route summary.
  - link to `本地路由`.
- Key Pool:
  - still owns CRUD and detailed key config.
  - can retain current drag ordering if needed, but local routing edit becomes the clearer place for route priority.
- Channel Status:
  - owns probes and monitor templates.
  - local routing page links to it for diagnosis.
- Request Logs:
  - owns per-request deep explanation.
- Pricing / Rates:
  - owns price matrix.

## 9. Testing Strategy

### 9.1 Unit Tests

Add focused tests for pure routing functions:

- candidate filters:
  - disabled key
  - missing API key
  - capability mismatch
  - model mismatch
  - cooling key
  - depleted balance
- economics:
  - complete price beats incomplete/group-rate-only when otherwise comparable
  - missing price does not crash ranking
  - low price does not override hard health failure
- stability:
  - current affinity remains when healthy
  - current affinity is broken on hard failure
  - small price delta does not switch
  - large price delta can switch after stability window/tolerance
- priority:
  - drag order is respected as tiebreaker/override signal
  - reorder returns contiguous 1/2/3 display order
- failure classification:
  - 401/403 -> auth error
  - 402 or known balance text -> insufficient balance
  - 429 -> rate limited with cooldown
  - 5xx -> temporary upstream
  - timeout/socket reset -> observe or temporary
  - 400 client payload -> does not hurt key health
- stream boundary:
  - failover allowed before first byte
  - failover blocked after first byte

### 9.2 Integration Tests

Backend:

- `simulate_local_route` returns selected candidate and rejected reasons.
- `reorder_local_routing_keys` persists exact order and read model reflects it.
- switch-before-confirm does not probe depleted/cooling/auth-error keys.
- successful probe recovers observation/cooldown state only when allowed.
- request log records selected key, skipped candidates, failure class, and retry/fallback status.

Frontend:

- tab switch renders status/edit.
- edit tab drag reorder calls typed API and shows saving/synced/failed states.
- no page-level save button exists.
- no weight field exists.
- status tab shows candidate order and latest explanation.

Browser/visual:

- verify desktop and narrow viewport.
- text does not overflow row controls.
- drag handles are visible and keyboard/focus states exist.
- start/stop routing button colors remain consistent with current dashboard route toggle.

### 9.3 Regression Scripts

Add or update focused scripts only after implementation:

- `scripts/local-routing-page-layout.test.mjs`
- `scripts/local-routing-reorder.test.mjs`
- `scripts/local-routing-explanation.test.mjs`

These should not touch real user keys or external network by default. Use mock/invoke-unavailable fallback or test fixtures.

## 10. Phased Delivery

### Phase 1: Spec and Read Model

- Add `LocalRoutingWorkspace` types.
- Add backend/TS query to load current status, candidates, latest decision using existing data.
- No routing engine behavior change yet.
- Implement status/edit skeleton using current data.

### Phase 2: Drag Priority and Auto-Save

- Implement drag reorder in edit tab.
- Persist priority/order through typed API.
- Show row-level sync state.
- Remove weight from first-version UI.

### Phase 3: Decision Engine Refactor

- Extract pure routing policy functions.
- Add structured explanations.
- Preserve existing simulator behavior while moving internals to testable functions.
- Make `cost_stable_first` the default display strategy, backed by current strategy fields where possible.

### Phase 4: Failure Classification and Stability

- Add route affinity.
- Add normalized failure classification.
- Add ambiguous failure observation threshold.
- Ensure streaming first-byte boundary prevents unsafe replay.

### Phase 5: Switch-Before-Confirm

- Reuse existing low-token monitor template family.
- Add bounded target-model-aware probe before failover.
- Add probe skip rules for depleted/cooling/auth-error keys.
- Add short TTL for confirmed fallback candidates.

### Phase 6: Logs and Diagnostics Links

- Request Logs show structured route explanation.
- Status tab links to Channel Status and Request Logs for deep diagnosis.
- Change Center can receive only important route-impact events, not noisy probe churn.

## 11. Acceptance Criteria

User-facing:

- Opening `本地路由` first shows status, not a configuration table.
- User can see current local endpoint, selected/current key, candidate order, and latest route reason.
- User can switch to `编辑`, drag key rows, and see order save automatically.
- No weight field is visible.
- No page-level `保存策略` button exists.
- Key details do not open in a drawer.
- Probe/monitor details remain in `渠道状态`.

Engine:

- Routing decision code is split into focused, testable modules.
- Candidate selection is explainable and deterministic under equal inputs.
- Low price does not override hard health, balance, or capability failure.
- Ambiguous one-off failures do not immediately kill a key.
- Streaming output is not interrupted by background health changes.
- Failover probes are bounded, low-cost, and skip known-bad/depleted/cooling keys.

Validation:

- TypeScript/Vite checks pass for frontend changes.
- Cargo checks or focused Rust tests pass for backend changes.
- New unit tests cover priority order, failure classification, stream boundary, and switch-before-confirm skip rules.

## 12. Open Questions

- Should the first implementation expose the simple strategy selector (`更稳 / 均衡 / 更省钱`) or keep it fixed as `均衡`?
- Should model alias management stay on the redesigned routing page as a lower advanced section, or move to Settings later?
- Should route affinity be persisted across app restarts, or remain in memory for the first version?
- Should `priority` be reused for drag order, or should a new `routing_order` field be added to avoid semantic conflicts?

## 13. Anti-Shit-Mountain Rules

- No UI component should compute final routing decisions from raw arrays.
- No giant `scoreCandidate()` function that mixes DB access, network probing, UI strings, and ranking.
- No user-facing weight control in the first version.
- No probing loop that keeps hammering depleted or auth-broken keys.
- No stream replay after first output chunk.
- No direct UI `invoke`; route frontend calls through typed API modules.
- No broad page refactor that rewrites Key Pool, Channel Status, Pricing, and Logs at once.
- No copied code from reference repositories without license review and explicit attribution.
