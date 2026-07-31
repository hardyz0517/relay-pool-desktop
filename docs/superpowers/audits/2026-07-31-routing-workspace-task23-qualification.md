# Routing Operational Task 23 Qualification

Status: partial; automated workspace/deep-link/layout evidence passed, Tauri dev visual workflow verification pending
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

It intentionally does not mark Task 23 complete yet. The plan still requires Tauri dev manual verification with redacted fixture data.

## Automated evidence already collected

Latest relevant commits:

- `88987e6 test: cover routing workspace deep link flow`
- `cfb0aeb test: harden routing workspace layout contract`
- `bcaf378 feat: link routing trace back to request logs`
- `e372732 feat: render typed request decision timeline`

Commands run on this source snapshot:

| Command | Result | Notes |
|---|---:|---|
| `pnpm.cmd test -- src/features/routing/RoutingOperationalPreviewPanel.test.tsx` | Pass | Vitest script executed the full frontend suite: 59 files, 214 tests. |
| `pnpm.cmd exec tsc --noEmit` | Pass | TypeScript compile check. |
| `pnpm.cmd generate:bindings --check` | Pass | IPC bindings deterministic check; existing Rust warnings only. |
| `node scripts/routing-workspace-integration.test.mjs` | Pass | Source contract for workspace query keys, read-model usage, deep links and no raw planning JSON. |
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
- The frontend still uses backend read-model aliases and query functions; it does not import pricing/group/capability projectors as routing truth.

## Pending Tauri dev manual verification

The following plan items are still not complete:

- Run the real Tauri dev application with a redacted/synthetic fixture data directory.
- Walk the workflow: monitoring -> Key detail -> route simulation -> decision trace -> request log -> routing workspace.
- Check 1280x800, 1024x768 and the minimum supported window.
- Confirm table, drawer/detail, tooltip, typed error and long text states do not overlap.
- Capture or record only redacted fixture evidence; do not store real provider URLs, API keys, cookies, request/response bodies, local user database paths or screenshots containing private data.

Important boundary: the current `DemoBackend` returns unsupported for routing workspace APIs, so a browser demo page cannot prove the Task 23 Tauri workflow. A real desktop run must use an isolated fixture profile or explicitly redacted disposable local data.

## Task 24 gate

Do not start Task 24 deletion from this audit alone. Task 24 still requires:

- Task 23 manual Tauri evidence above;
- same-candidate local observation/soak evidence;
- deletion ledger approval showing default-v2 has no second selector, capacity, pricing, feedback or frontend truth path.

This audit is therefore a progress checkpoint, not a deletion approval.
