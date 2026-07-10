# Windows Auto Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add signed Windows x86_64 NSIS updates from public GitHub Releases, with startup checks, user-confirmed download/install, and safe local-proxy draining before relaunch.

**Architecture:** Build the prerequisite proxy `running -> draining -> stopped` lifecycle first, expose one backend update-preparation command through the existing `src/lib/api/*` boundary, then add a feature-local updater controller and global dialog. Keep release/version tooling separate from runtime UI, and verify Draft artifacts through a test-only local updater endpoint before publishing.

**Tech Stack:** Tauri 2, Rust, React 18, TypeScript, `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`, Node.js contract tests, GitHub Actions, `tauri-apps/tauri-action`.

---

## File Map

- Modify `src-tauri/src/services/proxy/runtime.rs`: own proxy lifecycle, reject new work while draining, wait for active requests, and restore service on timeout.
- Modify `src-tauri/src/models/proxy.rs`: serialize the proxy lifecycle to the frontend.
- Modify `src-tauri/src/commands/mod.rs`: expose `prepare_local_proxy_for_update`.
- Modify `src-tauri/src/lib.rs`: register updater/process plugins and the preparation command.
- Modify `src/lib/types/proxy.ts`: mirror the proxy lifecycle union.
- Modify `src/lib/api/proxy.ts`: wrap the new backend command and preserve browser fallback behavior.
- Create `src/lib/api/updater.ts`: contain every direct updater/process plugin call and manage the Tauri `Update` resource.
- Create `src/features/updater/updaterState.ts`: define the pure updater state machine.
- Create `src/features/updater/UpdaterProvider.tsx`: run startup/manual checks and orchestrate download, drain, install, recovery, and resource cleanup.
- Create `src/features/updater/UpdateDialog.tsx`: render version details, release notes, progress, retry, and GitHub fallback actions.
- Modify `src/main.tsx`: mount the updater provider once for the whole app.
- Modify `src/features/settings/SettingsPage.tsx`: show current version, in-memory last-check result, and manual check action.
- Create `src-tauri/capabilities/default.json`: grant updater default workflow and process restart only.
- Modify `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `package.json`, and `pnpm-lock.yaml`: add plugins, enable NSIS/updater artifacts, and define version commands.
- Create `scripts/updater-contract.test.mjs`: enforce updater boundaries and UI contract.
- Create `scripts/updater-config.test.mjs`: enforce Tauri bundle, endpoint, capability, and dependency configuration.
- Create `scripts/versioning.mjs`, `scripts/versioning.test.mjs`, `scripts/set-version.mjs`, and `scripts/check-release-version.mjs`: provide one version source and CI tag validation.
- Create `.github/workflows/release.yml`: build signed Draft Releases on version tags.
- Create `scripts/validate-updater-release.mjs`: validate downloaded Draft artifacts and build a local `latest.json` fixture.
- Create `docs/RELEASING.md`: document key custody, bootstrap release, local signed-update smoke, publish, and post-publish checks.

### Task 1: Add A Recoverable Proxy Drain Lifecycle

**Files:**
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src/lib/types/proxy.ts`

- [ ] **Step 1: Write failing Rust lifecycle tests**

Add focused tests in the existing `#[cfg(test)]` module in `runtime.rs`:

```rust
fn running_proxy_for_drain_test(active_count: u32) -> ProxyRuntimeState {
    ProxyRuntimeState {
        inner: Mutex::new(ProxyRuntimeInner {
            running: true,
            lifecycle: ProxyLifecycle::Running,
            port: 8787,
            started_at: Some("test".to_string()),
            last_error: None,
            request_count: Some(Arc::new(AtomicU64::new(0))),
            stop_signal: Some(Arc::new(AtomicBool::new(false))),
            active_requests: Some(Arc::new(AtomicU32::new(active_count))),
            accepting_requests: Some(Arc::new(AtomicBool::new(true))),
            handle: None,
        }),
    }
}

fn drain_test_active_requests(proxy: &ProxyRuntimeState) -> Arc<AtomicU32> {
    proxy
        .inner
        .lock()
        .expect("proxy lock")
        .active_requests
        .as_ref()
        .cloned()
        .expect("active counter")
}

fn drain_test_accepting_requests(proxy: &ProxyRuntimeState) -> bool {
    proxy
        .inner
        .lock()
        .expect("proxy lock")
        .accepting_requests
        .as_ref()
        .expect("accepting flag")
        .load(Ordering::Relaxed)
}

#[test]
fn prepare_for_update_drains_active_requests_before_stopping() {
    let proxy = running_proxy_for_drain_test(1);
    let active = drain_test_active_requests(&proxy);
    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(40));
        active.store(0, Ordering::Relaxed);
    });

    let status = proxy
        .prepare_for_update(8787, Duration::from_millis(250))
        .expect("drain succeeds");

    worker.join().expect("worker joins");
    assert!(!status.running);
    assert_eq!(status.lifecycle, ProxyLifecycle::Stopped);
}

#[test]
fn prepare_for_update_timeout_restores_running_proxy() {
    let proxy = running_proxy_for_drain_test(1);

    let error = proxy
        .prepare_for_update(8787, Duration::from_millis(20))
        .expect_err("drain times out");

    assert!(error.contains("30 秒内仍有活动请求") || error.contains("仍有活动请求"));
    let status = proxy.status(8787);
    assert!(status.running);
    assert_eq!(status.lifecycle, ProxyLifecycle::Running);
    assert!(drain_test_accepting_requests(&proxy));
}

#[test]
fn draining_proxy_rejects_new_requests_without_incrementing_active_count() {
    let active_requests = Arc::new(AtomicU32::new(2));
    let accepting_requests = Arc::new(AtomicBool::new(false));
    let before = active_requests.load(Ordering::Relaxed);

    assert!(!should_accept_proxy_request(&accepting_requests));
    assert_eq!(active_requests.load(Ordering::Relaxed), before);
}
```

The test helper may construct `ProxyRuntimeInner` directly because the tests live in the same module; it must set `running: true`, lifecycle `Running`, an activity counter, and an accepting flag, with no listener handle.

- [ ] **Step 2: Run the focused tests to prove RED**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml prepare_for_update -- --nocapture
```

Expected: compilation fails because `ProxyLifecycle`, `prepare_for_update`, and the drain helpers do not exist.

- [ ] **Step 3: Add the lifecycle model and runtime state**

In `src-tauri/src/models/proxy.rs`, add:

```rust
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyLifecycle {
    Stopped,
    Running,
    Draining,
}
```

Add `pub lifecycle: ProxyLifecycle` to `ProxyStatus`. In `runtime.rs`, store `lifecycle: ProxyLifecycle` and `accepting_requests: Option<Arc<AtomicBool>>` in `ProxyRuntimeInner`; set both in `start()`. Preserve the existing `running` boolean in serialized status for compatibility, deriving it from lifecycle where practical.

Before spawning a request worker in `run_server`, check the accepting flag. When it is false, write a small JSON `503 Service Unavailable` response with `Retry-After: 1`, close the stream, and do not create `ActiveRequestGuard`.

Pass the accepting flag into the server loop and gate worker creation:

```rust
fn run_server(
    listener: TcpListener,
    stop_signal: Arc<AtomicBool>,
    accepting_requests: Arc<AtomicBool>,
    context: Arc<ProxyServerContext>,
) {
    while !stop_signal.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((mut stream, _)) if !accepting_requests.load(Ordering::Relaxed) => {
                let _ = stream.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nRetry-After: 1\r\nConnection: close\r\nContent-Length: 54\r\n\r\n{\"error\":{\"message\":\"application update in progress\"}}",
                );
                let _ = stream.shutdown(Shutdown::Both);
            }
            Ok((stream, _)) => {
                let context = Arc::clone(&context);
                thread::spawn(move || {
                    let _guard = ActiveRequestGuard::new(Arc::clone(&context.active_requests));
                    handle_connection(stream, &context);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(30));
            }
            Err(_) => break,
        }
    }
}
```

Implement:

```rust
const UPDATE_DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub fn prepare_for_update(
    &self,
    default_port: u16,
    timeout: Duration,
) -> Result<ProxyStatus, String> {
    let (accepting, active_requests) = {
        let mut inner = self.inner.lock().map_err(|_| "代理状态锁已损坏".to_string())?;
        if !inner.running {
            return Ok(self.status_from_inner(&inner, default_port));
        }
        inner.lifecycle = ProxyLifecycle::Draining;
        let accepting = inner.accepting_requests.as_ref().cloned()
            .ok_or_else(|| "代理缺少接入状态".to_string())?;
        let active = inner.active_requests.as_ref().cloned()
            .ok_or_else(|| "代理缺少活动请求计数".to_string())?;
        accepting.store(false, Ordering::Relaxed);
        (accepting, active)
    };

    let deadline = Instant::now() + timeout;
    while active_requests.load(Ordering::Relaxed) > 0 {
        if Instant::now() >= deadline {
            accepting.store(true, Ordering::Relaxed);
            let mut inner = self.inner.lock().map_err(|_| "代理状态锁已损坏".to_string())?;
            inner.lifecycle = ProxyLifecycle::Running;
            return Err(format!(
                "准备更新超时，仍有 {} 个活动请求",
                active_requests.load(Ordering::Relaxed)
            ));
        }
        thread::sleep(UPDATE_DRAIN_POLL_INTERVAL);
    }

    self.stop(default_port)
}
```

Update `stop()` so a stopped runtime reports lifecycle `Stopped`, and ensure timeout restores both lifecycle and request acceptance without clearing counters.

Mirror the field in TypeScript:

```ts
export type ProxyLifecycle = "stopped" | "running" | "draining";

export type ProxyStatus = {
  running: boolean;
  lifecycle: ProxyLifecycle;
  // existing fields remain unchanged
};
```

- [ ] **Step 4: Run focused and regression tests to prove GREEN**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml prepare_for_update -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml proxy_status_reports_localhost_bind_only -- --nocapture
```

Expected: all selected tests pass.

- [ ] **Step 5: Commit the runtime prerequisite**

```powershell
git add -- src-tauri/src/models/proxy.rs src-tauri/src/services/proxy/runtime.rs src/lib/types/proxy.ts
git commit -m "feat: add recoverable proxy drain lifecycle"
```

### Task 2: Expose One Update-Preparation Backend Capability

**Files:**
- Create: `scripts/updater-backend-contract.test.mjs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/api/proxy.ts`

- [ ] **Step 1: Write the failing backend boundary test**

Create a Node source-contract test that asserts:

```js
assert.ok(commands.includes("pub fn prepare_local_proxy_for_update"));
assert.ok(commands.includes("Duration::from_secs(30)"));
assert.ok(lib.includes("commands::prepare_local_proxy_for_update"));
assert.ok(proxyApi.includes('invoke<ProxyStatus>("prepare_local_proxy_for_update")'));
```

Also assert no feature component directly invokes that command.

- [ ] **Step 2: Run the contract test to prove RED**

Run `node scripts/updater-backend-contract.test.mjs`.

Expected: FAIL because the command and wrapper are absent.

- [ ] **Step 3: Implement and register the command**

Add to `commands/mod.rs`:

```rust
#[tauri::command]
pub fn prepare_local_proxy_for_update(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    proxy.prepare_for_update(
        settings.local_proxy_port,
        std::time::Duration::from_secs(30),
    )
}
```

Register it in `src-tauri/src/lib.rs`. Add `prepareLocalProxyForUpdate()` to `src/lib/api/proxy.ts`; in browser fallback return a stopped status with lifecycle `"stopped"` so Vite UI tests remain usable.

- [ ] **Step 4: Run checks to prove GREEN**

Run:

```powershell
node scripts/updater-backend-contract.test.mjs
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: contract test prints its pass message; frontend build and Cargo check exit 0.

- [ ] **Step 5: Commit the backend capability**

```powershell
git add -- scripts/updater-backend-contract.test.mjs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/api/proxy.ts
git commit -m "feat: expose proxy preparation for updates"
```

### Task 3: Configure Signed Windows Updater Artifacts

**Files:**
- Create: `scripts/updater-config.test.mjs`
- Create: `src-tauri/capabilities/default.json`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write the failing updater configuration test**

The test must parse JSON where possible and assert exact values:

```js
assert.equal(tauri.version, "../package.json");
assert.equal(tauri.bundle.active, true);
assert.equal(tauri.bundle.targets, "nsis");
assert.equal(tauri.bundle.createUpdaterArtifacts, true);
assert.equal(tauri.bundle.windows.nsis.installMode, "currentUser");
assert.equal(tauri.plugins.updater.windows.installMode, "passive");
assert.deepEqual(tauri.plugins.updater.endpoints, [
  "https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json",
]);
assert.ok(capability.permissions.includes("updater:default"));
assert.ok(capability.permissions.includes("process:allow-restart"));
assert.ok(!JSON.stringify(tauri).includes("dangerousInsecureTransportProtocol"));
```

Read Cargo and package sources to assert both updater/process dependencies and Rust plugin initialization are present.

- [ ] **Step 2: Run the configuration test to prove RED**

Run `node scripts/updater-config.test.mjs`.

Expected: FAIL on the first missing dependency/configuration assertion.

- [ ] **Step 3: Add plugins using the project package managers**

Run:

```powershell
pnpm.cmd add @tauri-apps/plugin-updater@^2 @tauri-apps/plugin-process@^2
cargo add --manifest-path .\src-tauri\Cargo.toml tauri-plugin-updater@2 tauri-plugin-process@2
```

Register both plugins in `Builder` before `.setup(...)`:

```rust
.plugin(tauri_plugin_updater::Builder::new().build())
.plugin(tauri_plugin_process::init())
```

- [ ] **Step 4: Add capability and production configuration**

Before editing configuration, generate the updater key outside the repository. Stop and obtain the user's passphrase interactively; never invent, log, or persist it in the plan/workspace:

```powershell
$keyDir = Join-Path $env:USERPROFILE ".tauri"
$keyPath = Join-Path $keyDir "relay-pool-desktop.key"
New-Item -ItemType Directory -Force -Path $keyDir | Out-Null
if (Test-Path -LiteralPath $keyPath) { throw "Updater private key already exists: $keyPath" }
pnpm.cmd tauri signer generate -w $keyPath
git status --short
```

Expected: the private key and `.pub` file exist only under `%USERPROFILE%\.tauri`; `git status --short` shows no key file. Put the public-key content into Tauri config. Store the private key content/password in GitHub Secrets only after the user confirms their offline encrypted backup exists.

Create `src-tauri/capabilities/default.json` scoped to the main window:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Main window desktop capabilities",
  "windows": ["main"],
  "permissions": ["core:default", "updater:default", "process:allow-restart"]
}
```

Update `tauri.conf.json` with the exact bundle/updater settings asserted above. Set the updater `pubkey` to the generated public key only; never put the private key or password in this file. Do not enable insecure transport in production config.

- [ ] **Step 5: Run config, frontend, and Cargo checks**

Run:

```powershell
node scripts/updater-config.test.mjs
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit updater infrastructure**

```powershell
git add -- package.json pnpm-lock.yaml src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json src-tauri/capabilities/default.json src-tauri/src/lib.rs scripts/updater-config.test.mjs
git commit -m "build: configure signed Windows updates"
```

### Task 4: Add the Updater API and State Machine

**Files:**
- Create: `scripts/updater-contract.test.mjs`
- Create: `src/lib/api/updater.ts`
- Create: `src/features/updater/updaterState.ts`

- [ ] **Step 1: Write the failing updater boundary test**

Assert that only `src/lib/api/updater.ts` imports updater/process plugins, that it calls `Update.close()`, and that the state module defines these states:

```ts
type UpdaterPhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "preparing"
  | "installing"
  | "failed";
```

Assert the API exposes `checkForUpdate`, `downloadAvailableUpdate`, `installDownloadedUpdate`, and `closeAvailableUpdate`.

- [ ] **Step 2: Run the contract test to prove RED**

Run `node scripts/updater-contract.test.mjs`.

Expected: FAIL because the API and state files do not exist.

- [ ] **Step 3: Implement the direct Tauri adapter**

`src/lib/api/updater.ts` must own a single `Update | null` resource. Use `isTauri()` to return an explicit unsupported result in Vite, `getVersion()` for the displayed current version, `check({ timeout: 10_000 })`, `download()` for progress, `install()`, and `relaunch()` only after successful install. Close and clear the resource on dismiss, replacement, and terminal failure.

Expose serializable data rather than the plugin object:

```ts
export type AvailableUpdate = {
  currentVersion: string;
  version: string;
  date: string | null;
  notes: string | null;
};

export type UpdateCheckResult =
  | { kind: "unsupported"; currentVersion: string }
  | { kind: "current"; currentVersion: string }
  | { kind: "available"; update: AvailableUpdate };
```

The download callback must convert `Started`, `Progress`, and `Finished` events into `{ downloadedBytes, totalBytes }`, preserving `null` when content length is unavailable.

- [ ] **Step 4: Implement a pure reducer**

Define an initial state and explicit events. Reject stale/concurrent transitions by making `checking` idempotent and allowing download only from `available`. Keep the last check result in memory only.

- [ ] **Step 5: Run contract and build checks**

Run:

```powershell
node scripts/updater-contract.test.mjs
pnpm.cmd build
```

Expected: contract test and TypeScript/Vite build pass.

- [ ] **Step 6: Commit the updater core**

```powershell
git add -- scripts/updater-contract.test.mjs src/lib/api/updater.ts src/features/updater/updaterState.ts
git commit -m "feat: add desktop updater state machine"
```

### Task 5: Add Global Update UX and Settings Controls

**Files:**
- Modify: `scripts/updater-contract.test.mjs`
- Create: `src/features/updater/UpdaterProvider.tsx`
- Create: `src/features/updater/UpdateDialog.tsx`
- Modify: `src/main.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`

- [ ] **Step 1: Extend the failing UI contract**

Assert the provider is mounted once outside `App`, schedules one startup check after 4 seconds, the dialog contains “稍后更新” and “立即更新” but no download cancel action, and Settings consumes `useUpdater()` rather than calling plugin APIs.

- [ ] **Step 2: Run the UI contract to prove RED**

Run `node scripts/updater-contract.test.mjs`.

Expected: FAIL on missing provider/dialog/settings integration.

- [ ] **Step 3: Implement the provider orchestration**

`UpdaterProvider` must expose:

```ts
type UpdaterContextValue = {
  state: UpdaterState;
  checkNow: (source?: "startup" | "manual") => Promise<void>;
  dismiss: () => Promise<void>;
  install: () => Promise<void>;
  retry: () => Promise<void>;
};
```

On mount, schedule `checkNow("startup")` after 4 seconds and clear the timer on unmount. A module/ref guard must prevent React StrictMode from producing overlapping checks. Startup failures update in-memory status without a toast or blocking dialog; manual failures remain visible in Settings.

Install orchestration must be:

```ts
await downloadAvailableUpdate(setProgress);
const before = await getProxyStatus();
if (before.running) await prepareLocalProxyForUpdate();
try {
  await installDownloadedUpdate();
} catch (error) {
  if (before.running) await startLocalProxy().catch(() => undefined);
  throw error;
}
```

Do not call `downloadAndInstall()` because the app must drain the proxy between download and installation.

- [ ] **Step 4: Implement the dialog**

Use the existing `Dialog` and `Button` components. Show current/target version, release notes as whitespace-preserving plain text, and a stable progress bar whose width derives from known byte totals. While total size is unknown, show an indeterminate bar without resizing the dialog. Disable closing during `preparing` and `installing`; do not render a fake cancel button during download.

- [ ] **Step 5: Add Settings update status**

Add a compact `SectionCard` titled “应用更新” with current version, last in-memory check status, and a RefreshCw icon button/text action “检查更新”. Reuse `checkNow("manual")`; do not persist updater state in AppSettings or SQLite.

- [ ] **Step 6: Run UI and build checks**

Run:

```powershell
node scripts/updater-contract.test.mjs
pnpm.cmd build
```

Expected: all checks pass with no TypeScript errors.

- [ ] **Step 7: Commit the user-facing flow**

```powershell
git add -- scripts/updater-contract.test.mjs src/features/updater/UpdaterProvider.tsx src/features/updater/UpdateDialog.tsx src/main.tsx src/features/settings/SettingsPage.tsx
git commit -m "feat: add user-confirmed update flow"
```

### Task 6: Make Versioning Deterministic

**Files:**
- Create: `scripts/versioning.mjs`
- Create: `scripts/versioning.test.mjs`
- Create: `scripts/set-version.mjs`
- Create: `scripts/check-release-version.mjs`
- Modify: `package.json`

- [ ] **Step 1: Write RED tests for SemVer and source synchronization**

Test valid stable versions, rejection of malformed versions, Cargo replacement in an isolated fixture string, and comparison against `vX.Y.Z` tags. Export pure functions `parseStableVersion`, `replaceCargoPackageVersion`, and `assertReleaseVersion` from `versioning.mjs`.

- [ ] **Step 2: Run tests to prove RED**

Run `node --test scripts/versioning.test.mjs`.

Expected: FAIL because `versioning.mjs` is missing.

- [ ] **Step 3: Implement version helpers and CLIs**

Use JSON parsing/serialization for `package.json`. Restrict the Cargo edit to the first `[package]` block and require exactly one version replacement. `set-version.mjs 0.1.0` updates package.json and Cargo.toml; `check-release-version.mjs` reads `GITHUB_REF_NAME` or an explicit CLI argument and verifies package, Cargo, Tauri `../package.json`, and tag consistency.

Add scripts:

```json
{
  "version:set": "node scripts/set-version.mjs",
  "version:check-release": "node scripts/check-release-version.mjs"
}
```

- [ ] **Step 4: Prove GREEN without changing the project version**

Run:

```powershell
node --test scripts/versioning.test.mjs
pnpm.cmd version:check-release -- v0.0.0
```

Expected: tests pass and the current `0.0.0` sources match tag argument `v0.0.0`.

- [ ] **Step 5: Commit version tooling**

```powershell
git add -- package.json scripts/versioning.mjs scripts/versioning.test.mjs scripts/set-version.mjs scripts/check-release-version.mjs
git commit -m "build: add deterministic release versioning"
```

### Task 7: Build Draft Releases in GitHub Actions

**Files:**
- Create: `.github/workflows/release.yml`
- Modify: `scripts/updater-config.test.mjs`

- [ ] **Step 1: Extend the config test with workflow security assertions**

Assert the workflow triggers only on `v*` tags, declares `contents: write`, uses `windows-latest`, runs `pnpm version:check-release`, builds `x86_64-pc-windows-msvc`, creates a Draft Release, and reads `TAURI_SIGNING_PRIVATE_KEY` plus `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Assert `tauri-apps/tauri-action` uses a 40-character commit SHA rather than `@v0` or a branch.

- [ ] **Step 2: Run the test to prove RED**

Run `node scripts/updater-config.test.mjs`.

Expected: FAIL because `.github/workflows/release.yml` does not exist.

- [ ] **Step 3: Create the release workflow**

The workflow must perform checkout, pnpm setup, Node setup with pnpm cache, Rust stable MSVC setup, dependency install with frozen lockfile, contract/version/build/Cargo checks, and `tauri-action`. Use:

```yaml
on:
  push:
    tags: ["v*"]

permissions:
  contents: write

jobs:
  release:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
      - uses: pnpm/action-setup@f40ffcd9367d9f12939873eb1018b921a783ffaa
        with:
          run_install: false
      - uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020
        with:
          node-version: 22
          cache: pnpm
      - uses: dtolnay/rust-toolchain@4be7066ada62dd38de10e7b70166bc74ed198c30
        with:
          toolchain: stable-x86_64-pc-windows-msvc
      - run: pnpm install --frozen-lockfile
      - run: pnpm version:check-release -- $env:GITHUB_REF_NAME
      - run: pnpm build
      - run: cargo check --manifest-path .\src-tauri\Cargo.toml
      - uses: tauri-apps/tauri-action@fce9c6108b31ea247710505d3aaaa893ee6768d4
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          tagName: v__VERSION__
          releaseName: Relay Pool Desktop v__VERSION__
          releaseDraft: true
          prerelease: false
          args: --target x86_64-pc-windows-msvc
```

The pinned SHAs above were resolved from the official `v4`, `stable`, and `v0` refs while writing this plan. Re-verify their provenance before implementation, but do not replace them with movable tag or branch references.

- [ ] **Step 4: Run local workflow contract checks**

Run:

```powershell
node scripts/updater-config.test.mjs
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: all checks exit 0. Do not push a tag as part of this task.

- [ ] **Step 5: Commit the workflow**

```powershell
git add -- .github/workflows/release.yml scripts/updater-config.test.mjs
git commit -m "ci: build signed Windows draft releases"
```

### Task 8: Add Release Validation and Operator Runbook

**Files:**
- Create: `scripts/validate-updater-release.mjs`
- Create: `scripts/validate-updater-release.test.mjs`
- Create: `docs/RELEASING.md`
- Modify: `package.json`

- [ ] **Step 1: Write RED tests for release metadata validation**

Use temporary directories and fixture JSON to prove the validator rejects a missing `.sig`, empty signature, wrong version, non-HTTPS production URL, and missing `windows-x86_64` platform entry. Prove local-fixture mode rewrites only the artifact URL and preserves version/signature fields.

- [ ] **Step 2: Run tests to prove RED**

Run `node --test scripts/validate-updater-release.test.mjs`.

Expected: FAIL because the validator is missing.

- [ ] **Step 3: Implement the validator**

Accept `--dir`, `--version`, and optional `--local-base-url`. Parse latest.json structurally, verify referenced artifact and signature files exist, and write `latest.local.json` only when local base URL is supplied. Never print signing secrets or artifact contents.

Add:

```json
{
  "release:validate": "node scripts/validate-updater-release.mjs"
}
```

- [ ] **Step 4: Write the release runbook**

Document exact operator steps:

1. Generate updater keys offline and add only the public key to Tauri config.
2. Store private key/password separately; configure GitHub Secrets and encrypted offline backup.
3. Perform a backup recovery/sign/verify rehearsal before `0.1.0`.
4. Publish `0.1.0` as the manual bootstrap installer.
5. For `0.1.1`, download Draft artifacts with authenticated `gh release download`.
6. Set `$artifactDir = Resolve-Path .\output\updater-smoke` and `$fixturePort = 18431`, then run `pnpm release:validate -- --dir $artifactDir --version 0.1.1 --local-base-url "http://127.0.0.1:$fixturePort"`.
7. Build the old test client with a test-only config that enables localhost insecure transport; verify check, signed download, proxy drain, install, and relaunch.
8. Verify a corrupted artifact is rejected.
9. Publish the Draft, then verify the public GitHub latest.json from installed `0.1.0`.
10. Record Authenticode as a separate production-distribution gate, not a substitute for updater signatures.

State explicitly that the test-only insecure config must never be used for a release build, and that a lost updater private key cannot be repaired by generating a new key for already-installed clients.

- [ ] **Step 5: Run validator tests and full local verification**

Run:

```powershell
node --test scripts/versioning.test.mjs scripts/validate-updater-release.test.mjs
node scripts/updater-backend-contract.test.mjs
node scripts/updater-config.test.mjs
node scripts/updater-contract.test.mjs
pnpm.cmd build
cargo test --manifest-path .\src-tauri\Cargo.toml prepare_for_update -- --nocapture
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: all commands pass. Actual signed installer/relaunch validation remains a release-candidate gate because it requires repository Secrets and a Draft Release.

- [ ] **Step 6: Commit release operations documentation**

```powershell
git add -- package.json scripts/validate-updater-release.mjs scripts/validate-updater-release.test.mjs docs/RELEASING.md
git commit -m "docs: add signed updater release runbook"
```

## Final Acceptance Gate

- [ ] Confirm only Windows x86_64 NSIS current-user artifacts are produced.
- [ ] Confirm production config contains HTTPS GitHub endpoint and no insecure transport flag.
- [ ] Confirm updater/process permissions are limited to the reviewed capability entries.
- [ ] Confirm proxy drain timeout restores request acceptance and install failure attempts proxy recovery.
- [ ] Confirm `Update.close()` runs on dismiss/replacement/failure.
- [ ] Confirm `0.1.0` is treated as manual bootstrap and automatic update is tested with `0.1.0 -> 0.1.1`.
- [ ] Confirm no private key, password, token, Draft artifact, local fixture, database, or log is staged.
- [ ] Confirm final staging uses exact paths only and no push/tag/release occurs without explicit user authorization.
