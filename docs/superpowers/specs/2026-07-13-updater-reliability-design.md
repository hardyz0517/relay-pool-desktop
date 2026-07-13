# Updater Reliability Design

## Context

Relay Pool Desktop 0.2.2 can report `检查更新未完成` even when the
published GitHub manifest and the installed application are both version
0.2.2. Runtime investigation established the following request chain:

1. `@tauri-apps/plugin-updater` performs the authoritative update check.
2. When that request fails, `src/lib/api/updater.ts` invokes a Rust command to
   read `latest.json`.
3. When the Rust command does not return a version, the frontend tries to fetch
   the GitHub release asset directly.

The three paths do not share the same network configuration. The machine has a
Windows system proxy enabled, but the updater plugin and the fallback `ureq`
client do not receive that proxy. The browser fallback cannot read the GitHub
asset because the response does not permit cross-origin browser requests.
Consequently, all paths can fail even though the manifest is available through
the user's configured system proxy.

The audit also found lifecycle and state-coordination defects:

- A manifest-only newer version is exposed as installable even though no native
  updater resource exists.
- The install flow calls the legacy immediate cleanup command instead of the
  existing request-draining update preparation command.
- A new check can start while an update is downloading or installing and close
  the update resource in use.
- Check failures and install failures share one state, so stale release details
  and install warnings can be shown for a failed check.
- The non-Tauri `unsupported` result is presented as `up to date`.
- Most updater checks assert source strings rather than runtime behavior.

## Goals

- Make the native updater check and manifest fallback use one Windows system
  proxy decision.
- Report `up to date` when the fallback manifest is the same as, or older than,
  the installed version.
- Report an available update only when a native Tauri `Update` resource exists.
- Prevent checks from racing with download, cleanup, or installation.
- Drain active proxy requests before installation and preserve the running
  proxy when draining times out.
- Give check, download, preparation, and installation failures accurate UI
  states and recovery actions.
- Replace fragile source-shape assertions for decision logic with behavioral
  regression tests.
- Keep the design open to future proxy sources and release channels without
  duplicating updater orchestration.

## Non-Goals

- Adding a second updater-specific proxy setting to the UI.
- Treating the collector proxy as an updater proxy.
- Background download, forced update, downgrade, release channels, or download
  cancellation.
- Changing signing keys, GitHub Releases, release assets, or CI publication.
- Publishing a new release as part of this repair.

## Architecture

### 1. Shared updater network configuration

The existing Windows system proxy parser in
`src-tauri/src/services/outbound.rs` remains the single source of truth. Its
system-proxy lookup becomes visible within the Rust crate and is exposed through
a narrow Tauri updater-network command.

The frontend requests this configuration once per native check and passes the
returned proxy URL through the supported `check({ proxy })` option. The proxy is
therefore retained by the native `Update` resource for both check and download.
No proxy URL is logged or included in user-facing errors.

If no Windows system proxy is enabled, the updater uses its existing direct
behavior. Failure to read system proxy configuration must not prevent a direct
check.

### 2. Authoritative check plus bounded fallback

The Tauri updater plugin remains authoritative because it validates the
manifest shape, compares semantic versions, selects the platform artifact, and
creates the resource required for download and install.

If the native check fails, the backend manifest reader performs one bounded
request using the same system proxy decision. The browser `fetch` fallback is
removed because it is not a valid cross-origin transport for GitHub release
assets.

Fallback results have deliberately limited authority:

- Manifest version equal to installed version: return `current`.
- Manifest version older than installed version: return `current`.
- Manifest version newer than installed version: preserve the native check
  failure and explain that a newer manifest was found but the installer could
  not be prepared. Do not return `available`.
- Missing, malformed, or unreachable manifest: preserve a normalized check
  failure with an actionable network/system-proxy message.

Semantic version ordering is handled by the Rust `semver` crate. The handwritten
numeric array comparison is removed so prerelease identifiers and invalid
versions cannot silently produce the wrong decision.

The blocking `ureq` request runs through `spawn_blocking`; its timeout is owned
by the backend request instead of competing with a shorter frontend timer.

### 3. Testable update-check coordinator

Updater orchestration is split into a small coordinator with injected
dependencies:

- obtain current version;
- obtain network configuration;
- run native check;
- inspect the fallback manifest after native failure;
- return a discriminated result.

The production adapter supplies Tauri APIs. Node tests supply deterministic
functions and assert outcomes, proxy propagation, and resource cleanup. Module
state remains private to the adapter so tests do not require a WebView or a live
GitHub request.

This boundary also supports future release channels: a later channel resolver
can provide an endpoint and network configuration without changing provider UI
state transitions.

### 4. Explicit updater operation state

The reducer distinguishes the operation that failed: `check`, `download`,
`prepare`, or `install`. A check start clears stale manifest-only details. An
install-stage failure retains the native update metadata needed to explain what
failed, but does not claim the update completed.

The provider enforces one mutually exclusive updater operation:

- repeated checks are ignored while a check is active;
- checks are ignored while download, preparation, or installation is active;
- install is accepted only from the `available` phase;
- the settings check button is disabled for every busy updater phase.

An `unsupported` result gets its own message and never dispatches `UP_TO_DATE`.

### 5. Safe installation preparation

The updater imports and calls `prepareLocalProxyForUpdate()` from the existing
proxy API. This invokes the backend's 30-second drain-aware preparation flow.
The legacy updater-local `cleanupBeforeUpdate()` wrapper and its contract test
are removed or redirected so there is only one frontend boundary for proxy
lifecycle operations.

If preparation times out, installation stops, the backend restores the running
proxy, and the dialog reports a preparation failure. The installer is not run.

### 6. Failure-specific dialog content

For a failed check, the dialog shows only the check error and retry/close
actions. It does not show a version arrow, release-note placeholder, install
warning, or install action.

For download, preparation, or installation failures, the dialog retains the
target version and presents the relevant failure. Busy phases remain
non-dismissible and do not expose conflicting actions.

The network error copy becomes direct and actionable: GitHub could not be
reached using the current network/system-proxy configuration. It no longer asks
the user to compare release notes manually.

## Error Handling

- Network and timeout errors normalize to one actionable network message.
- A missing `latest.json` remains distinct from network failure.
- Manifest parse and semantic-version errors are reported as invalid update
  metadata, not as `up to date`.
- Signature verification remains distinct and is never hidden by fallback
  comparison.
- Errors must not contain full proxy credentials, API keys, cookies, or release
  signatures.
- Late native checks are detached and their resources are closed, preserving
  the existing timeout-recovery guarantee.

## Verification

### Behavioral frontend tests

- Native check returns `null`: result is `current`.
- Native check returns an update resource: result is `available`.
- Native check fails and fallback version equals current: result is `current`.
- Native check fails and fallback version is older: result is `current`.
- Native check fails and fallback version is newer: result is a check failure,
  never an installable update.
- The detected proxy URL is passed to the native check.
- No browser fetch is attempted.
- A late native result is closed after timeout abandonment.
- Check requests are rejected while install is active.
- Unsupported runtime is not reported as current.

### Rust tests

- Existing Windows proxy formats still resolve correctly.
- Updater manifest requests select the system proxy agent.
- Equal, older, newer, prerelease, and invalid semantic versions are classified
  explicitly.
- Missing and malformed manifest fields return typed errors.
- The drain-aware preparation command remains registered and used.

### Project checks

- Run focused Node updater tests and reducer tests.
- Run the complete updater contract suite.
- Run `pnpm build`.
- Run focused Cargo tests, `cargo test`, and `cargo check` for `src-tauri`.
- Launch current source with `pnpm tauri:dev` and verify the manual update check
  on Windows reports `已是最新` for installed/source version 0.2.2 against the
  published 0.2.2 manifest.

## Compatibility And Rollout

The change is Windows-first, matching the current release target. On other
platforms the network command returns no Windows proxy and preserves direct
behavior. Existing updater signing and installation formats are unchanged.

No release or tag is created by this task. A future release should run the same
end-to-end check from the previous published version because a current-version
check cannot validate installer download and signature verification by itself.
