# Updater Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make update checks reliably use the Windows system proxy, report same-version manifests as current, expose only installable native updates, and make the install lifecycle race-free and drain-aware.

**Architecture:** A pure TypeScript coordinator owns native-check/fallback decisions while the Tauri adapter owns native `Update` resources and timeouts. A focused Rust updater service reuses the existing outbound system-proxy resolver, fetches and semantically classifies the GitHub manifest off the main thread, and exposes narrow commands. The React reducer/provider represent operation-specific failures and serialize check/install operations.

**Tech Stack:** Tauri 2, Rust, `semver`, `ureq`, React 18, TypeScript, Node 22 test runner, Vite.

---

## File Structure

- Create `src/lib/api/updaterCheckCoordinator.ts`: pure update decision logic with injected native and manifest dependencies.
- Create `scripts/updater-check-coordinator.test.ts`: behavioral tests for proxy propagation and fallback authority.
- Create `src-tauri/src/services/updater.rs`: updater network configuration, manifest transport, and semantic-version classification.
- Modify `src-tauri/src/services/outbound.rs`: expose the existing system proxy resolver within the crate.
- Modify `src-tauri/src/services/mod.rs`: register the updater service module.
- Modify `src-tauri/src/commands/mod.rs`: replace the version-only fallback command with updater network and manifest inspection commands.
- Modify `src-tauri/src/lib.rs`: register the new commands and remove the obsolete command.
- Modify `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`: add direct `semver` ownership.
- Modify `src/lib/api/updater.ts`: wire the pure coordinator to Tauri APIs and keep native resource lifecycle private.
- Modify `src/lib/api/updaterErrors.ts`: normalize typed updater failures and actionable network copy.
- Modify `src/features/updater/updateState.ts`: track failure operation and expose one busy-phase predicate.
- Modify `src/features/updater/UpdaterProvider.tsx`: serialize operations and use drain-aware proxy preparation.
- Modify `src/features/updater/UpdateDialog.tsx`: render check failures separately from install-stage failures.
- Modify `src/features/settings/SettingsPage.tsx`: disable manual checks for all busy updater phases.
- Modify updater scripts and `.github/workflows/release.yml`: replace brittle fallback contracts with behavioral coverage and keep release gates synchronized.

### Task 1: Pure Update Check Coordinator

**Files:**
- Create: `scripts/updater-check-coordinator.test.ts`
- Create: `src/lib/api/updaterCheckCoordinator.ts`

- [ ] **Step 1: Write the failing coordinator tests**

Create tests covering native current, native available, proxy propagation, fallback current/older, fallback newer, and fallback failure:

```typescript
import assert from "node:assert/strict";
import test from "node:test";

const modulePath = new URL("../src/lib/api/updaterCheckCoordinator.ts", import.meta.url);

test("passes the detected proxy to the authoritative native check", async () => {
  const { coordinateUpdateCheck } = await import(modulePath.href);
  let receivedProxy: string | null = null;
  const result = await coordinateUpdateCheck({
    currentVersion: "0.2.2",
    proxyUrl: "http://127.0.0.1:7890",
    checkNative: async (proxyUrl) => {
      receivedProxy = proxyUrl;
      return null;
    },
    inspectPublished: async () => {
      throw new Error("fallback must not run");
    },
  });
  assert.equal(receivedProxy, "http://127.0.0.1:7890");
  assert.deepEqual(result, { kind: "current", currentVersion: "0.2.2" });
});

test("uses a same-or-older fallback only to prove the app is current", async () => {
  const { coordinateUpdateCheck } = await import(modulePath.href);
  const result = await coordinateUpdateCheck({
    currentVersion: "0.2.2",
    proxyUrl: null,
    checkNative: async () => { throw new Error("native network failure"); },
    inspectPublished: async () => ({
      relation: "current_or_older",
      version: "0.2.2",
      notes: null,
    }),
  });
  assert.deepEqual(result, { kind: "current", currentVersion: "0.2.2" });
});

test("never turns a manifest-only newer version into an installable update", async () => {
  const { coordinateUpdateCheck, ManifestNewerButNativeUnavailableError } =
    await import(modulePath.href);
  await assert.rejects(
    coordinateUpdateCheck({
      currentVersion: "0.2.2",
      proxyUrl: null,
      checkNative: async () => { throw new Error("native network failure"); },
      inspectPublished: async () => ({
        relation: "newer",
        version: "0.2.3",
        notes: "Fixes",
      }),
    }),
    (error) => error instanceof ManifestNewerButNativeUnavailableError &&
      error.publishedVersion === "0.2.3",
  );
});
```

- [ ] **Step 2: Run the coordinator test and verify RED**

Run: `node --test scripts/updater-check-coordinator.test.ts`

Expected: FAIL because `src/lib/api/updaterCheckCoordinator.ts` does not exist.

- [ ] **Step 3: Implement the minimal pure coordinator**

Create a dependency-injected coordinator with no Tauri or alias imports:

```typescript
export type NativeUpdateLike = {
  currentVersion: string;
  version: string;
  body?: string;
};

export type PublishedUpdateInspection = {
  relation: "current_or_older" | "newer";
  version: string;
  notes: string | null;
};

export class ManifestNewerButNativeUnavailableError extends Error {
  readonly code = "manifest-newer-native-unavailable";
  constructor(
    readonly publishedVersion: string,
    readonly nativeError: unknown,
  ) {
    super(`Published update ${publishedVersion} exists but the native updater is unavailable`);
  }
}

export async function coordinateUpdateCheck<T extends NativeUpdateLike>(dependencies: {
  currentVersion: string;
  proxyUrl: string | null;
  checkNative: (proxyUrl: string | null) => Promise<T | null>;
  inspectPublished: (currentVersion: string) => Promise<PublishedUpdateInspection>;
}) {
  try {
    const update = await dependencies.checkNative(dependencies.proxyUrl);
    return update
      ? { kind: "available" as const, update }
      : { kind: "current" as const, currentVersion: dependencies.currentVersion };
  } catch (nativeError) {
    let inspection: PublishedUpdateInspection;
    try {
      inspection = await dependencies.inspectPublished(dependencies.currentVersion);
    } catch {
      throw nativeError;
    }
    if (inspection.relation === "current_or_older") {
      return { kind: "current" as const, currentVersion: dependencies.currentVersion };
    }
    throw new ManifestNewerButNativeUnavailableError(inspection.version, nativeError);
  }
}
```

- [ ] **Step 4: Run the coordinator test and verify GREEN**

Run: `node --test scripts/updater-check-coordinator.test.ts`

Expected: all coordinator tests PASS.

- [ ] **Step 5: Commit the coordinator slice**

```powershell
git add -- scripts/updater-check-coordinator.test.ts src/lib/api/updaterCheckCoordinator.ts
git diff --cached --name-only
git commit -m "test: define updater fallback authority"
```

### Task 2: Rust Updater Network And Manifest Service

**Files:**
- Create: `src-tauri/src/services/updater.rs`
- Modify: `src-tauri/src/services/outbound.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

- [ ] **Step 1: Write failing Rust tests for manifest classification**

Add tests in the new service for equal, older, newer, prerelease, invalid, and missing versions:

```rust
#[test]
fn classifies_equal_and_older_manifests_as_current_or_older() {
    assert_eq!(
        inspect_manifest_body(r#"{"version":"0.2.2","notes":""}"#, "0.2.2")
            .unwrap()
            .relation,
        PublishedVersionRelation::CurrentOrOlder,
    );
    assert_eq!(
        inspect_manifest_body(r#"{"version":"0.2.1"}"#, "0.2.2")
            .unwrap()
            .relation,
        PublishedVersionRelation::CurrentOrOlder,
    );
}

#[test]
fn classifies_newer_and_prerelease_versions_with_semver_rules() {
    assert_eq!(
        inspect_manifest_body(r#"{"version":"0.2.3"}"#, "0.2.2")
            .unwrap()
            .relation,
        PublishedVersionRelation::Newer,
    );
    assert_eq!(
        inspect_manifest_body(r#"{"version":"0.2.3-beta.1"}"#, "0.2.3")
            .unwrap()
            .relation,
        PublishedVersionRelation::CurrentOrOlder,
    );
}

#[test]
fn rejects_missing_or_invalid_manifest_versions() {
    assert!(inspect_manifest_body("{}", "0.2.2").is_err());
    assert!(inspect_manifest_body(r#"{"version":"not-semver"}"#, "0.2.2").is_err());
}
```

- [ ] **Step 2: Run focused Rust tests and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml services::updater -- --nocapture`

Expected: FAIL because the updater service and direct `semver` dependency do not exist.

- [ ] **Step 3: Implement the updater service and commands**

Add `semver = "1"`, expose `current_system_proxy_url` as `pub(crate)`, and implement:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterNetworkConfig {
    pub proxy_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishedVersionRelation {
    CurrentOrOlder,
    Newer,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedUpdateInspection {
    pub relation: PublishedVersionRelation,
    pub version: String,
    pub notes: Option<String>,
}

pub fn updater_network_config() -> UpdaterNetworkConfig {
    UpdaterNetworkConfig { proxy_url: current_system_proxy_url() }
}

pub fn inspect_manifest_body(
    body: &str,
    current_version: &str,
) -> Result<PublishedUpdateInspection, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| format!("Invalid updater manifest JSON: {error}"))?;
    let version = value.get("version").and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Updater manifest does not contain a version".to_string())?;
    let normalize_version = |value: &str| {
        let value = value.trim();
        value.strip_prefix('v').or_else(|| value.strip_prefix('V')).unwrap_or(value)
    };
    let published = semver::Version::parse(normalize_version(version))
        .map_err(|error| format!("Invalid published updater version: {error}"))?;
    let current = semver::Version::parse(normalize_version(current_version))
        .map_err(|error| format!("Invalid current application version: {error}"))?;
    Ok(PublishedUpdateInspection {
        relation: if published > current {
            PublishedVersionRelation::Newer
        } else {
            PublishedVersionRelation::CurrentOrOlder
        },
        version: version.to_string(),
        notes: value.get("notes").and_then(serde_json::Value::as_str)
            .filter(|notes| !notes.is_empty()).map(str::to_string),
    })
}
```

The blocking manifest request must use
`agent_builder_for_proxy(&ProxyConfig { mode: "system", url: None })`, a 10-second
timeout, and run through `tauri::async_runtime::spawn_blocking` in the command.
Register `updater_network_config` and `inspect_latest_update_manifest`; remove
`latest_update_manifest_version`.

- [ ] **Step 4: Run focused Rust tests and verify GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml services::updater -- --nocapture`

Expected: updater service tests PASS.

- [ ] **Step 5: Run Cargo check for command registration**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS with both updater commands registered.

- [ ] **Step 6: Commit the Rust service slice**

```powershell
git add -- src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/services/outbound.rs src-tauri/src/services/mod.rs src-tauri/src/services/updater.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git diff --cached --name-only
git commit -m "fix: unify updater system proxy handling"
```

### Task 3: Wire Native Resource Lifecycle To The Coordinator

**Files:**
- Modify: `src/lib/api/updater.ts`
- Modify: `src/lib/api/updaterErrors.ts`
- Modify: `scripts/updater-current-version-fallback.test.mjs`
- Modify: `scripts/updater-timeout-recovery.test.mjs`
- Modify: `scripts/updater-error-message.test.ts`

- [ ] **Step 1: Tighten failing adapter and error tests**

Require the adapter to invoke both new commands, pass `proxy` into native
`check`, remove browser `fetch`, remove handwritten version comparison, and map
`manifest-newer-native-unavailable` distinctly:

```typescript
test("newer manifest without a native resource has actionable copy", async () => {
  const { normalizeUpdaterError } = await import(errorModulePath.href);
  assert.equal(
    normalizeUpdaterError({
      code: "manifest-newer-native-unavailable",
      publishedVersion: "0.2.3",
    }),
    "发现新版本 0.2.3，但更新器无法准备下载；请检查网络或系统代理后重试。",
  );
});
```

- [ ] **Step 2: Run focused adapter tests and verify RED**

Run:

```powershell
node scripts/updater-current-version-fallback.test.mjs
node scripts/updater-timeout-recovery.test.mjs
node --test scripts/updater-error-message.test.ts
```

Expected: FAIL because the old commands, browser fallback, manual comparison,
and old copy remain.

- [ ] **Step 3: Implement the adapter wiring**

Update `checkForAppUpdate()` to:

```typescript
const network = await invoke<UpdaterNetworkConfig>("updater_network_config")
  .catch(() => ({ proxyUrl: null }));
const result = await coordinateUpdateCheck({
  currentVersion,
  proxyUrl: network.proxyUrl,
  checkNative: async (proxyUrl) => {
    try {
      return await withTimeout(startNativeUpdateCheck(proxyUrl), 12_000, "更新检查超时");
    } catch (error) {
      abandonNativeUpdateCheck();
      throw error;
    }
  },
  inspectPublished: (version) =>
    invoke<PublishedUpdateInspection>("inspect_latest_update_manifest", {
      currentVersion: version,
    }),
});
```

Set `pendingUpdate` only from a native `available` result. Remove
`fetchLatestManifestVersionFromBrowser`, `versionsMatch`, `isVersionNewer`,
`versionParts`, and `ensurePendingUpdateForInstall`. Make
`startNativeUpdateCheck(proxyUrl)` call `check({ timeout: 10_000, proxy })` when
a proxy exists. Preserve late-resource closing.

- [ ] **Step 4: Run focused adapter tests and verify GREEN**

Run the three commands from Step 2 plus:

`node --test scripts/updater-check-coordinator.test.ts`

Expected: all PASS.

- [ ] **Step 5: Commit the adapter slice**

```powershell
git add -- src/lib/api/updater.ts src/lib/api/updaterErrors.ts scripts/updater-current-version-fallback.test.mjs scripts/updater-timeout-recovery.test.mjs scripts/updater-error-message.test.ts
git diff --cached --name-only
git commit -m "fix: make updater fallback non-installable"
```

### Task 4: Serialize State And Use Drain-Aware Preparation

**Files:**
- Modify: `scripts/updater-state-flow.test.ts`
- Modify: `scripts/updater-cleanup-contract.test.mjs`
- Modify: `src/features/updater/updateState.ts`
- Modify: `src/features/updater/UpdaterProvider.tsx`
- Modify: `src/lib/api/updater.ts`

- [ ] **Step 1: Add failing reducer and boundary tests**

Add tests proving check failure clears stale metadata, install-stage failure
retains target metadata, and all install phases are busy:

```typescript
test("check failures clear stale update details", async () => {
  const { initialUpdaterState, reduceUpdaterState } = await import(modulePath.href);
  const available = reduceUpdaterState(initialUpdaterState, {
    type: "UPDATE_AVAILABLE",
    currentVersion: "0.2.2",
    version: "0.2.3",
    notes: "Fixes",
  });
  const checking = reduceUpdaterState(available, { type: "CHECK_STARTED" });
  const failed = reduceUpdaterState(checking, {
    type: "FAILED",
    operation: "check",
    message: "offline",
  });
  assert.equal(failed.version, null);
  assert.equal(failed.notes, null);
  assert.equal(failed.failedOperation, "check");
});
```

Update the cleanup contract to require `prepareLocalProxyForUpdate()` and reject
any updater import or invoke of `cleanup_before_update`.

- [ ] **Step 2: Run state and cleanup tests and verify RED**

Run:

```powershell
node --test scripts/updater-state-flow.test.ts
node scripts/updater-cleanup-contract.test.mjs
```

Expected: FAIL because operation-specific failure state and drain-aware wiring
do not exist.

- [ ] **Step 3: Implement explicit failure operation and serialization**

Add:

```typescript
export type UpdaterFailureOperation = "check" | "download" | "prepare" | "install";
export function isUpdaterBusyPhase(phase: UpdaterPhase) {
  return phase === "checking" || phase === "downloading" ||
    phase === "cleaning" || phase === "installing";
}
```

Add `failedOperation` to state and `operation` to `FAILED`. Clear update details
on `CHECK_STARTED` and check failure; retain them for download/prepare/install
failures.

In the provider:

- return from `checkNow` when `checkingRef` or `installingRef` is true;
- handle `unsupported` as a check failure rather than `UP_TO_DATE`;
- allow install only in `available` phase;
- track the current install operation before each awaited stage;
- call `prepareLocalProxyForUpdate()` between download and install;
- dispatch failure with the correct operation.

Remove the updater-local `cleanupBeforeUpdate()` API.

- [ ] **Step 4: Run state and cleanup tests and verify GREEN**

Run the two commands from Step 2.

Expected: all tests PASS.

- [ ] **Step 5: Commit the state/lifecycle slice**

```powershell
git add -- scripts/updater-state-flow.test.ts scripts/updater-cleanup-contract.test.mjs src/features/updater/updateState.ts src/features/updater/UpdaterProvider.tsx src/lib/api/updater.ts
git diff --cached --name-only
git commit -m "fix: serialize updater install lifecycle"
```

### Task 5: Failure-Specific UI And Release Gates

**Files:**
- Modify: `src/features/updater/UpdateDialog.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `scripts/updater-ui-contract.test.mjs`
- Modify: `scripts/updater-config.test.mjs`
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Add failing UI contract assertions**

Require `UpdateDialog` to branch on `failedOperation === "check"`, suppress
release/install content for that branch, and require Settings to use
`isUpdaterBusyPhase(state.phase)` for the check button. Require the workflow to
run `node --test scripts/updater-check-coordinator.test.ts` and the focused Rust
updater service tests.

- [ ] **Step 2: Run UI/config contracts and verify RED**

Run:

```powershell
node scripts/updater-ui-contract.test.mjs
node scripts/updater-config.test.mjs
```

Expected: FAIL because the UI and workflow gates are not wired.

- [ ] **Step 3: Implement failure-specific dialog rendering**

Derive:

```typescript
const checkFailed = state.phase === "failed" && state.failedOperation === "check";
const showReleaseDetails = state.phase === "available" || busy ||
  (state.phase === "failed" && !checkFailed);
```

For `checkFailed`, render the error and retry/close actions only. For release
details, retain the version, notes, interruption warning, progress, and relevant
failure. Use `isUpdaterBusyPhase` in Settings to disable the check button and
animate its icon only during `checking`.

Add the coordinator and Rust updater tests to the release workflow and update
the config contract accordingly.

- [ ] **Step 4: Run UI/config contracts and verify GREEN**

Run the two commands from Step 2.

Expected: both PASS.

- [ ] **Step 5: Commit the UI/gate slice**

```powershell
git add -- src/features/updater/UpdateDialog.tsx src/features/settings/SettingsPage.tsx scripts/updater-ui-contract.test.mjs scripts/updater-config.test.mjs .github/workflows/release.yml
git diff --cached --name-only
git commit -m "fix: clarify updater failure recovery"
```

### Task 6: Full Verification And Runtime Proof

**Files:**
- Verify only; no planned source changes.

- [ ] **Step 1: Run the full updater test suite**

```powershell
node --test scripts/updater-check-coordinator.test.ts
node scripts/updater-config.test.mjs
node scripts/updater-cleanup-contract.test.mjs
node scripts/updater-current-version-fallback.test.mjs
node scripts/updater-timeout-recovery.test.mjs
node scripts/dashboard-update-action.test.mjs
node --test scripts/updater-error-message.test.ts
node --test scripts/updater-state-flow.test.ts
node scripts/updater-ui-contract.test.mjs
node scripts/updater-backend-contract.test.mjs
```

Expected: every command exits 0.

- [ ] **Step 2: Run frontend verification**

Run: `pnpm build`

Expected: TypeScript and Vite build PASS; the existing chunk-size warning is
acceptable but no new errors or warnings are introduced.

- [ ] **Step 3: Run Rust verification**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::updater -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml prepare_for_update -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all tests and checks PASS.

- [ ] **Step 4: Audit the final task boundary**

Run:

```powershell
git status --short
git diff --name-only 7df3514..HEAD
git diff --cached --name-only
```

Expected: user-owned `README.md` and the unrelated capture plan remain outside
all updater commits; no files are staged.

- [ ] **Step 5: Launch and verify the real Tauri path**

Start current source with `pnpm tauri:dev`. On the Settings page, trigger
`检查更新` while Windows system proxy `127.0.0.1:7890` is enabled.

Expected: the published `0.2.2` manifest is read through the shared proxy and
the app displays `已是最新`, with no failed-check dialog. Confirm the check
button cannot start a second check during a busy updater operation.

- [ ] **Step 6: Final exact-path status check**

Run: `git status --short`

Expected: only the pre-existing user-owned changes remain, plus any deliberately
uncommitted plan file if the user chose not to commit it.
