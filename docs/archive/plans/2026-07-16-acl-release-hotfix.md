# ACL Release Hotfix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore the pricing, channel-status, and bulk-read IPC paths and make command authorization completeness a mandatory local and Release gate.

**Architecture:** Keep Tauri's existing least-privilege capability model. Add the three missing main-window grants, classify invoke failures in one shared module, and make both local verification and the Windows Release workflow call the same contract runner. This plan does not touch database selection, startup, or persistence.

**Tech Stack:** Tauri 2 permissions, React/TypeScript, Node 22 contract scripts, Vitest, Cargo, GitHub Actions.

---

## Locked file map

- `src-tauri/permissions/main-window.toml`: main-window command allowlist only.
- `src/lib/tauriErrors.ts`: the only invoke-error text classifier.
- `src/lib/queries/pricingQueries.ts`: old-backend fallback consumer.
- `src/lib/queries/channelQueries.ts`: old-backend fallback consumer.
- `src/lib/api/changeEvents.ts`: bulk-read old-backend fallback consumer.
- `scripts/manual-authorization-capability.test.mjs`: registered-command/ACL invariant.
- `scripts/tauri-command-fallback.test.mjs`: real error-text classifier contract.
- `scripts/run-contract-tests.mjs`: one ordered contract-test registry.
- `package.json`: `test:contracts` and `verify:release` entry points.
- `.github/workflows/release.yml`: calls `pnpm verify:release`; does not maintain a duplicate test list.

Do not change `src-tauri/src/services/database.rs`, data-directory config, page layout, routing, collectors, pricing calculations, or capture-window permissions in this plan.

### Task 1: Restore the missing main-window command grants

**Files:**
- Modify: `src-tauri/permissions/main-window.toml`
- Test: `scripts/manual-authorization-capability.test.mjs`

- [ ] **Step 1: Run the existing invariant and record the RED result**

Run:

```powershell
node scripts/manual-authorization-capability.test.mjs
```

Expected: exit 1 with `main-window permission must allow registered command load_channel_status_workspace` (the first missing command in handler order).

- [ ] **Step 2: Add only the three missing grants**

Insert these entries in the relevant command groups in `commands.allow`:

```toml
  "load_channel_status_workspace",
  "load_pricing_comparison_workspace",
  "mark_change_events_read",
```

Do not add `record_capture_event`; the existing negative assertion must remain green.

- [ ] **Step 3: Re-run the capability invariant**

Run:

```powershell
node scripts/manual-authorization-capability.test.mjs
```

Expected: exit 0 with no assertion output.

- [ ] **Step 4: Commit the narrow authorization fix**

```powershell
git add -- src-tauri/permissions/main-window.toml
git diff --cached --name-only
git commit -m "fix: authorize page workspace commands"
```

Expected staged path before commit: only `src-tauri/permissions/main-window.toml`.

### Task 2: Make ACL denial impossible to misclassify as fallback

**Files:**
- Modify: `src/lib/tauriErrors.ts`
- Modify: `src/lib/api/{changeEvents,channelMonitors,collector,collectorRuns,economics,external,groupFacts,localRouting,proxy,routing,secrets,settings,stationKeys,stations}.ts`
- Test: `scripts/tauri-command-fallback.test.mjs`
- Create: `scripts/tauri-error-classification-ownership.test.mjs`

- [ ] **Step 1: Extend the classifier test with real positive and negative samples**

Replace the invented `not allowed. Command not found` samples with this table:

```js
const cases = [
  ["Command load_pricing_comparison_workspace not found", "command-not-found"],
  ["Command load_channel_status_workspace not found", "command-not-found"],
  ["Command mark_change_events_read not found", "command-not-found"],
  ["Command load_pricing_comparison_workspace not allowed by ACL", "acl-denied"],
  ["database record not found", "other"],
  ["Cannot read properties of undefined (reading '__TAURI_INTERNALS__')", "runtime-unavailable"],
];

for (const [message, expected] of cases) {
  assert.equal(classifyTauriInvokeError(new Error(message)), expected, message);
}

assert.equal(
  isTauriCommandNotFound(new Error("Command load_pricing_comparison_workspace not allowed by ACL")),
  false,
);
```

Update the compiled-module destructuring to import `classifyTauriInvokeError`, `isTauriCommandNotFound`, and `isTauriInvokeUnavailable`.

- [ ] **Step 2: Run the classifier test and verify RED**

Run:

```powershell
node scripts/tauri-command-fallback.test.mjs
```

Expected: exit 1 because `classifyTauriInvokeError` is not exported and the ACL sample is not covered by the old table.

- [ ] **Step 3: Implement one ordered classifier**

Replace `src/lib/tauriErrors.ts` with the following shape; ordering is part of the contract:

```ts
export type TauriInvokeErrorKind =
  | "acl-denied"
  | "command-not-found"
  | "runtime-unavailable"
  | "other";

export function tauriErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function classifyTauriInvokeError(error: unknown): TauriInvokeErrorKind {
  const message = tauriErrorMessage(error);
  if (/not allowed by ACL/i.test(message)) return "acl-denied";
  if (/^Command\s+[^\s]+\s+not found$/i.test(message.trim())) return "command-not-found";
  if (/invoke|__TAURI(?:_INTERNALS__)?/i.test(message)) {
    return "runtime-unavailable";
  }
  return "other";
}

export function isTauriInvokeUnavailable(error: unknown) {
  return classifyTauriInvokeError(error) === "runtime-unavailable";
}

export function isTauriCommandNotFound(error: unknown) {
  return classifyTauriInvokeError(error) === "command-not-found";
}
```

- [ ] **Step 4: Remove duplicate invoke-unavailable classifiers**

In every API file listed for this task, import `isTauriInvokeUnavailable` from `@/lib/tauriErrors`, delete its local `isInvokeUnavailable` helper, and replace call sites one-for-one. In `changeEvents.ts`, import both shared predicates:

```ts
import { isTauriCommandNotFound, isTauriInvokeUnavailable } from "@/lib/tauriErrors";
```

Keep the order in `listChangeEventsForStation` and `markChangeEventsRead`: command-not-found fallback first, browser-preview fallback second, otherwise throw. The classifier checks ACL denial before the broad Runtime regex, so `not allowed by ACL` can never become a browser mock.

Create `scripts/tauri-error-classification-ownership.test.mjs` to scan `src/lib/api/*.ts` and assert no file defines `function isInvokeUnavailable`; files that use invoke-unavailable fallback must import `isTauriInvokeUnavailable` from `@/lib/tauriErrors`. Add this script to the Task 3 contract runner list.

- [ ] **Step 5: Run the focused fallback and query tests**

Run:

```powershell
node scripts/tauri-command-fallback.test.mjs
node scripts/tauri-error-classification-ownership.test.mjs
node scripts/page-switch-data-path-performance.test.mjs
node scripts/change-center-mark-read.test.mjs
pnpm test
```

Expected: all commands exit 0; do not treat “no tests found” as a pass.

- [ ] **Step 6: Commit the classifier fix**

```powershell
git add -- src/lib/tauriErrors.ts src/lib/api/changeEvents.ts src/lib/api/channelMonitors.ts src/lib/api/collector.ts src/lib/api/collectorRuns.ts src/lib/api/economics.ts src/lib/api/external.ts src/lib/api/groupFacts.ts src/lib/api/localRouting.ts src/lib/api/proxy.ts src/lib/api/routing.ts src/lib/api/secrets.ts src/lib/api/settings.ts src/lib/api/stationKeys.ts src/lib/api/stations.ts scripts/tauri-command-fallback.test.mjs scripts/tauri-error-classification-ownership.test.mjs
git diff --cached --name-only
git commit -m "fix: distinguish ACL denial from command fallback"
```

### Task 3: Create one local and Release verification entry point

**Files:**
- Create: `scripts/run-contract-tests.mjs`
- Modify: `package.json`
- Modify: `.github/workflows/release.yml`
- Test: `scripts/run-contract-tests.mjs`

- [ ] **Step 1: Add a failing package-script contract**

Create `scripts/release-verification-entrypoint.test.mjs` with exact assertions:

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const release = await readFile(".github/workflows/release.yml", "utf8");
assert.equal(pkg.scripts["test:contracts"], "node scripts/run-contract-tests.mjs");
assert.match(pkg.scripts["verify:release"], /pnpm test:contracts/);
assert.match(pkg.scripts["verify:release"], /pnpm test/);
assert.match(pkg.scripts["verify:release"], /pnpm build/);
assert.match(pkg.scripts["verify:release"], /cargo check/);
assert.match(release, /run: pnpm verify:release/);
assert.doesNotMatch(release, /run: node scripts\/updater-/);
```

- [ ] **Step 2: Run the entry-point contract and verify RED**

Run `node scripts/release-verification-entrypoint.test.mjs`.

Expected: exit 1 because `test:contracts` does not exist.

- [ ] **Step 3: Create the ordered contract runner**

Create `scripts/run-contract-tests.mjs`:

```js
import { spawnSync } from "node:child_process";

const contracts = [
  ["node", ["scripts/manual-authorization-capability.test.mjs"]],
  ["node", ["scripts/tauri-command-fallback.test.mjs"]],
  ["node", ["scripts/tauri-error-classification-ownership.test.mjs"]],
  ["node", ["scripts/updater-config.test.mjs"]],
  ["node", ["scripts/updater-cleanup-contract.test.mjs"]],
  ["node", ["scripts/updater-current-version-fallback.test.mjs"]],
  ["node", ["scripts/updater-timeout-recovery.test.mjs"]],
  ["node", ["--test", "scripts/updater-check-coordinator.test.ts"]],
  ["node", ["scripts/dashboard-update-action.test.mjs"]],
  ["node", ["--test", "scripts/updater-error-message.test.ts"]],
  ["node", ["--test", "scripts/updater-state-flow.test.ts"]],
  ["node", ["scripts/updater-ui-contract.test.mjs"]],
  ["node", ["scripts/release-verification-entrypoint.test.mjs"]],
];

for (const [command, args] of contracts) {
  const result = spawnSync(command, args, { stdio: "inherit", shell: process.platform === "win32" });
  if (result.status !== 0) process.exit(result.status ?? 1);
}
```

- [ ] **Step 4: Add package scripts and collapse the workflow list**

Add to `package.json`:

```json
"test:contracts": "node scripts/run-contract-tests.mjs",
"verify:release": "pnpm test:contracts && pnpm test && pnpm build && cargo test --manifest-path src-tauri/Cargo.toml services::updater -- --nocapture && cargo test --manifest-path src-tauri/Cargo.toml prepare_for_update -- --nocapture && cargo check --manifest-path src-tauri/Cargo.toml"
```

In `.github/workflows/release.yml`, replace the updater-script list, `pnpm build`, the two focused Cargo tests, and `cargo check` with one step:

```yaml
      - run: pnpm verify:release
```

Keep the subsequent `tauri-apps/tauri-action` build/sign/publish step unchanged.

- [ ] **Step 5: Verify the shared gate**

Run:

```powershell
node scripts/release-verification-entrypoint.test.mjs
pnpm test:contracts
pnpm verify:release
```

Expected: all three exit 0. `pnpm verify:release` must show contract tests, Vitest, Vite build, both focused Cargo test groups, and `cargo check`; a timeout is not a pass.

- [ ] **Step 6: Commit the release gate**

```powershell
git add -- scripts/run-contract-tests.mjs scripts/release-verification-entrypoint.test.mjs package.json .github/workflows/release.yml
git diff --cached --name-only
git commit -m "ci: require command contracts before release"
```

### Task 4: Verify the actual Windows release candidate

**Files:**
- Create: `docs/release/ACL_HOTFIX_SMOKE_CHECKLIST.md`
- Test: packaged `relay-pool-desktop.exe`

- [ ] **Step 1: Write the fixed smoke checklist**

The checklist must contain these exact pass conditions:

```markdown
- Install over the previous public release without deleting AppData.
- Open Price / Multipliers: no ACL toast; rows or a truthful empty state render.
- Open Channel Status: no ACL toast; the page workspace renders.
- In Change Center, mark at least two events read; navigate away and back; state remains read.
- Fully exit from the tray, restart, and repeat the three checks.
- Open a capture authorization window and confirm it still cannot invoke a normal main-window command.
```

- [ ] **Step 2: Build the Windows release candidate**

Run:

```powershell
pnpm verify:release
pnpm tauri:build -- --target x86_64-pc-windows-msvc
```

Expected: both exit 0 and produce the Windows bundle under `src-tauri/target/x86_64-pc-windows-msvc/release/bundle/`.

- [ ] **Step 3: Execute the checklist against the installed candidate**

Install the generated bundle over the previous release and record each item as PASS/FAIL in a local, untracked copy. Any ACL error, non-persistent bulk read, or expanded capture permission blocks release.

- [ ] **Step 4: Commit the durable checklist only**

```powershell
git add -- docs/release/ACL_HOTFIX_SMOKE_CHECKLIST.md
git diff --cached --name-only
git commit -m "docs: add ACL hotfix release smoke gate"
```

## Plan completion gate

Before tagging the ACL hotfix, require all of the following evidence in the same revision:

```powershell
pnpm verify:release
git diff --exit-code -- src-tauri/permissions/main-window.toml src/lib/tauriErrors.ts src/lib/api scripts/manual-authorization-capability.test.mjs scripts/tauri-command-fallback.test.mjs scripts/tauri-error-classification-ownership.test.mjs scripts/run-contract-tests.mjs scripts/release-verification-entrypoint.test.mjs package.json .github/workflows/release.yml docs/release/ACL_HOTFIX_SMOKE_CHECKLIST.md
git status --short
```

`git status --short` may show pre-existing user files, but no task file may remain modified or untracked. Do not stage `src-tauri/src/services/collectors/adapters/newapi/test_support.rs`, `output/`, or existing target directories unless the user separately scopes them in.
