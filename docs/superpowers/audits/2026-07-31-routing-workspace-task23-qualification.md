# Routing Operational Task 23 Qualification

Status: Task 23 Tauri dev qualification passed with synthetic fixture evidence; not a Task 24 deletion approval
Date: 2026-07-31
Branch: `codex/routing-operational-upgrade`

## Scope

This audit records the current evidence for Task 23 of
`docs/superpowers/plans/2026-07-30-routing-operational-unification-upgrade.md`:

- integrated routing workspace;
- stable deep links from monitoring, pricing, collectors, Key pool, request logs, station endpoint health and change center;
- typed request decision timeline;
- source/layout contracts for narrow windows, long local names and bounded candidate tables;
- frontend-only responsibility boundary: display/navigation/formatting only, with authoritative facts from backend read models.

It marks the Task 23 routing workspace workflow as verified on a real Tauri dev run with synthetic fixture data. It intentionally does not approve Task 24 deletion; local observation/soak and deletion-ledger approval remain separate prerequisites.

## Automated evidence already collected

Latest relevant commits:

- `88987e6 test: cover routing workspace deep link flow`
- `cfb0aeb test: harden routing workspace layout contract`
- `f9acf12 chore: add routing workspace fixture upstream`
- `bcaf378 feat: link routing trace back to request logs`
- `e372732 feat: render typed request decision timeline`

Commands run on this source snapshot:

| Command | Result | Notes |
|---|---:|---|
| `pnpm.cmd test -- src/features/routing/RoutingOperationalPreviewPanel.test.tsx` | Pass | Vitest script executed the full frontend suite: 59 files, 216 tests. |
| `pnpm.cmd exec tsc --noEmit` | Pass | TypeScript compile check. |
| `pnpm.cmd generate:bindings --check` | Pass | IPC bindings deterministic check; existing Rust warnings only. |
| `node scripts/routing-workspace-integration.test.mjs` | Pass | Source contract for workspace query keys, read-model usage, deep links and no raw planning JSON. |
| `node --check scripts/verify-routing-workspace-tauri-cdp.mjs` | Pass | Verifier syntax check. |
| `node scripts/verify-routing-workspace-tauri-cdp.mjs` | Pass | Real Tauri dev + WebView2 CDP + synthetic fixture workflow; evidence JSON/screenshots under ignored `output/manual-routing-workspace/task23-routing-workspace-cdp/evidence/`. |
| `node scripts/local-routing-page-layout.test.mjs` | Pass | Source contract for bounded table, `min-w-0`, wrapping/truncation and error layout guards. |
| `pnpm.cmd architecture:typescript` | Pass | TypeScript boundary gate. |
| `pnpm.cmd lint` | Pass | Existing 82 warnings remain; no new lint error. |
| `pnpm.cmd build` | Pass | Existing Vite large chunk warning remains. |
| `git diff --check` | Pass | No whitespace errors before staging. |

## What the automated tests prove

- Request deep links call `getRequestDecisionTraceQuery` and render backend typed `trace.timeline`, not frontend-reconstructed `planningRounds` JSON.
- The decision trace panel can navigate back to request logs through the page-provided callback.
- Pricing-style simulate-model deep links call `simulateRouteQuery` with the policy, multiplier ceiling and routing group filter from the backend workspace snapshot.
- Candidate rows show backend-supplied price basis, capability evidence, runtime overlay health/in-flight data and snapshot-only capacity without inventing zero price or fake health.
- Workspace layout includes bounded candidate-table scrolling, fixed minimum table width, `min-w-0`, wrapping for long detail codes/key ids and wrapped typed error text.
- Loading, unavailable and typed backend error states stay explicit and do not fall back to fake healthy candidates or zero-price output.
- Routing workspace refresh/invalidation remains query-scoped; the page does not call `cancelQueries`, `removeQueries` or `resetQueries` for monitoring/collector authority.
- The frontend still uses backend read-model aliases and query functions; it does not import pricing/group/capability projectors as routing truth.

## Tauri dev synthetic workflow verification

Command run:

```powershell
node scripts/verify-routing-workspace-tauri-cdp.mjs
```

The verifier:

- starts `scripts/routing-workspace-fixture-server.mjs` on `http://127.0.0.1:18181/v1`;
- starts a real Tauri dev app with a temporary app identifier `dev.relaypool.desktop.routing-workspace-cdp`, Vite port `1431`, and WebView2 CDP port `9236`;
- creates a synthetic OpenAI-compatible station and station key using only `fixture-local-key`;
- records capabilities for `routing-fixture-chat` and `routing-fixture-embedding`;
- starts the local proxy, sends one synthetic `/v1/chat/completions` request, then stops the proxy;
- reads backend-owned `load_routing_workspace_snapshot`, `load_routing_runtime_overlay`, `simulate_route`, `list_recent_route_decisions`, `get_request_decision_trace`, and `list_request_logs`;
- opens the real routing workspace UI, opens the recent decision timeline, then opens the request log from the timeline;
- captures synthetic-only screenshots at 1280x800, 1024x768, and 980x640.

Evidence artifact:

- `output/manual-routing-workspace/task23-routing-workspace-cdp/evidence/routing-workspace-tauri-cdp-evidence.json`
- `output/manual-routing-workspace/task23-routing-workspace-cdp/evidence/routing-workspace-1280x800.png`
- `output/manual-routing-workspace/task23-routing-workspace-cdp/evidence/routing-workspace-1024x768.png`
- `output/manual-routing-workspace/task23-routing-workspace-cdp/evidence/routing-workspace-980x640.png`
- `output/manual-routing-workspace/task23-routing-workspace-cdp/evidence/request-log-opened-1024x768.png`

Observed evidence:

- workspace snapshot returned one backend candidate, not a frontend-reconstructed row;
- simulator selected the synthetic station key with `capacityMode = snapshot_only`;
- proxy request succeeded through `http://127.0.0.1:8787/v1/chat/completions`;
- request log stored `upstreamBaseUrl = null` and masked key data only;
- recent decision rendered in the routing workspace;
- decision trace rendered typed timeline sections: `legacy_summary`, `planning_round`, `slot_wait`, `attempt_protocol`, `fallback`, `downstream_delivery`, and `cost_aggregate`;
- the timeline's request-log action opened the real request log page;
- all three checked window sizes reported `viewportOverflowX = false`, while the candidate table used its bounded internal horizontal scroll at the minimum window.

Manual fallback launcher remains available:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-workspace-tauri-manual.ps1
```

The launcher uses a temporary app identifier and Vite port overlay so it can run beside another Relay Pool dev instance without disabling the normal single-instance or installation-lease protections.

Important boundary: the current `DemoBackend` returns unsupported for routing workspace APIs, so a browser demo page cannot prove the Task 23 Tauri workflow. A real desktop run must use an isolated fixture profile or explicitly redacted disposable local data.

## Task 24 gate

Do not start Task 24 deletion from this audit alone. Task 24 still requires:

- same-candidate local observation/soak evidence;
- deletion ledger approval showing default-v2 has no second selector, capacity, pricing, feedback or frontend truth path.

This audit is therefore a Task 23 qualification record, not a Task 24 deletion approval.
